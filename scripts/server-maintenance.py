#!/usr/bin/env python3
"""Hourly live-server maintenance, run from the operator's machine (see docs/hosting.md).

1. Pull the server's game logs (`STATE/games`) into one local mirror with rsync. The mirror is
   updated in place: unchanged files are skipped and nothing is ever deleted locally, so it holds
   one copy of every game that was ever on the server.
2. Pick expired game folders whose newest file is older than `--max-age-hours`.
3. Verify every file in those folders against the mirror with an rsync checksum comparison and a
   local size check. A folder with any mismatch is kept on the server.
4. Delete only verified files whose size and mtime are unchanged since planning, then empty dirs.
5. Remove release worktrees other than the current one, the newest finished build and any
   build still in progress. Code and executables are not backed up.

Every run appends a JSON summary to `<dest>/../maintenance-runs.jsonl`. Any sync or verification
failure stops the run before anything is deleted.
"""
import argparse
import base64
import fcntl
import json
import os
import subprocess
import sys
import time
from pathlib import Path

SSH = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=20', '-o', 'ServerAliveInterval=30']


# ---- Runs on the server (shipped over ssh as source; stdlib only). ---------------------------

def protected_games(games, listed):
    """Rank completed matches, including rematches and logs from pre-marker servers."""
    completed = []
    for match in games.glob('game-*/replays/*'):
        if not (match / 'manifest.json').is_file():
            continue
        marker = match / 'completed.json'
        if marker.exists():
            finished = json.loads(marker.read_text())
            if type(finished) is not int or finished < 0:
                raise ValueError(f'invalid completion timestamp in {marker}')
        else:
            rounds = (p for p in (match / 'rounds').glob('*.json') if p.stem.isascii() and p.stem.isdecimal())
            last = max(rounds, key=lambda p: int(p.stem), default=None)
            if last is None:
                continue
            data = json.loads(last.read_text())
            if not any((data.get(field) or {}).get('match_winners') for field in ('score', 'timed')):
                continue
            finished = last.stat().st_mtime_ns // 1_000_000
        completed.append((finished, match.parent.parent.name, match.name))
    completed.sort(reverse=True)
    # Also preserve the five the directory can serve if an orphan log is present.
    visible = (r for r in completed if r[1] in listed)
    from itertools import islice
    return {game for _, game, _ in completed[:5] + list(islice(visible, 5))}


def remote_plan(cfg):
    games = Path(cfg['state']) / 'games'
    cutoff = time.time() - cfg['max_age_hours'] * 3600
    try:
        listed = json.loads((games / 'directory.json').read_text()).get('games', {})
    except FileNotFoundError:
        listed = {}
    protected = protected_games(games, listed)
    folders = []
    for entry in sorted(games.iterdir()):
        if not entry.name.startswith('game-') or not entry.is_dir() or entry.is_symlink():
            continue
        if entry.name in protected:
            continue
        record = listed.get(entry.name)
        if record is not None and not record.get('expired'):
            continue
        files, newest = [], entry.stat().st_mtime
        for dirpath, dirnames, filenames in os.walk(entry):
            for name in dirnames + filenames:
                st = os.lstat(os.path.join(dirpath, name))
                newest = max(newest, st.st_mtime)
                if name in filenames:
                    rel = os.path.relpath(os.path.join(dirpath, name), games)
                    files.append([rel, st.st_size, st.st_mtime_ns])
        if newest < cutoff:
            folders.append({'name': entry.name, 'files': files})
    releases = Path(cfg['state']) / 'releases'
    current_file = Path(cfg['state']) / 'current'
    current = current_file.read_text().strip() if current_file.exists() else None
    ready = sorted((r for r in releases.iterdir() if (r / '.ready').exists()),
                   key=lambda r: (r / '.ready').stat().st_mtime) if releases.exists() else []
    keep = {current, ready[-1].name if ready else None}
    remove = [r.name for r in ready if r.name not in keep] if current else []
    st = os.statvfs(games)
    return {'protected_games': sorted(protected), 'folders': folders, 'releases': remove, 'current': current,
            'free_bytes': st.f_bavail * st.f_frsize}


def remote_delete(cfg, payload):
    import pwd
    import shutil
    games = Path(cfg['state']) / 'games'
    freed, deleted, kept = 0, [], []
    # Recheck eligibility before deleting: another match may have finished since verification.
    eligible = {folder['name'] for folder in remote_plan(cfg)['folders']}
    for folder in payload['folders']:
        if folder['name'] not in eligible:
            kept.append(folder['name'])
            continue
        root = games / folder['name']
        changed = 0
        for rel, size, mtime_ns in folder['files']:
            path = games / rel
            try:
                st = os.lstat(path)
            except FileNotFoundError:
                continue
            if st.st_size != size or st.st_mtime_ns != mtime_ns:
                changed += 1
                continue
            path.unlink()
            freed += size
        for dirpath, _, _ in sorted(os.walk(root), key=lambda w: -len(w[0])):
            try:
                os.rmdir(dirpath)
            except OSError:
                pass
        (kept if root.exists() or changed else deleted).append(folder['name'])
    releases = Path(cfg['state']) / 'releases'
    repo = Path(cfg['repo'])
    owner = pwd.getpwuid(repo.stat().st_uid).pw_name
    as_owner = ['runuser', '-u', owner, '--'] if os.geteuid() == 0 and owner != 'root' else []
    removed, failed = [], []
    for name in payload['releases']:
        path = releases / name
        if name in (payload['current'], '') or '/' in name or not path.is_dir():
            continue
        before = shutil.disk_usage(releases).free
        result = subprocess.run(as_owner + ['git', '-C', str(repo), 'worktree', 'remove', '--force',
                                            str(path)], capture_output=True, text=True)
        if result.returncode == 0:
            removed.append(name)
            freed += max(0, shutil.disk_usage(releases).free - before)
        else:
            failed.append([name, result.stderr.strip()[-300:]])
    subprocess.run(as_owner + ['git', '-C', str(repo), 'worktree', 'prune'], capture_output=True)
    return {'freed_bytes': freed, 'deleted_folders': deleted, 'kept_folders': kept,
            'removed_releases': removed, 'failed_releases': failed,
            'free_bytes': shutil.disk_usage(games).free}


def remote_entry(argv):
    cfg = json.loads(argv[1])
    if argv[0] == 'plan':
        result = remote_plan(cfg)
    else:
        result = remote_delete(cfg, json.load(sys.stdin))
    json.dump(result, sys.stdout)


# ---- Runs locally. ----------------------------------------------------------------------------

def on_server(args, op, payload=None):
    """Run a remote_* function on the host ('local' runs it here, for tests)."""
    cfg = json.dumps({'state': args.state, 'repo': args.repo, 'max_age_hours': args.max_age_hours})
    source = base64.b64encode(Path(__file__).read_bytes()).decode()
    boot = ('import base64,sys;g={"__name__":"remote"};'
            'exec(base64.b64decode(sys.argv[1]).decode(),g);g["remote_entry"](sys.argv[2:])')
    cmd = ['python3', '-I', '-c', boot, source, op, cfg]
    if args.host != 'local':
        cmd = SSH + [args.host, ' '.join(shell_quote(c) for c in cmd)]
    result = subprocess.run(cmd, input=json.dumps(payload) if payload is not None else '',
                            capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f'server {op} failed: {result.stderr.strip()[-2000:]}')
    return json.loads(result.stdout)


def shell_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


def rsync_source(args):
    path = args.state.rstrip('/') + '/games/'
    return path if args.host == 'local' else f'{args.host}:{path}'


def rsync(args, extra):
    cmd = ['rsync', '-a', '--partial', '--exclude', '/directory.tmp']
    if args.host != 'local':
        cmd += ['-e', ' '.join(SSH)]
    return subprocess.run(cmd + extra + [rsync_source(args), str(args.dest) + '/'],
                          capture_output=True, text=True)


def sync(args):
    args.dest.mkdir(parents=True, exist_ok=True)
    result = rsync(args, ['--stats'])
    if result.returncode != 0:
        raise RuntimeError(f'rsync exited {result.returncode}: {result.stderr.strip()[-2000:]}')
    return result.returncode


def verify(args, folders):
    """Return the names of folders whose every file is byte-identical in the mirror."""
    if not folders:
        return set()
    listing = args.dest.parent / '.verify-files'
    listing.write_text(''.join(f[0] + '\n' for folder in folders for f in folder['files']))
    try:
        result = rsync(args, ['-n', '-c', '--itemize-changes', '--out-format=%i %n',
                              f'--files-from={listing}'])
    finally:
        listing.unlink(missing_ok=True)
    if result.returncode != 0:
        raise RuntimeError(f'verification rsync exited {result.returncode}: '
                           f'{result.stderr.strip()[-2000:]}')
    differing = set()
    for line in result.stdout.splitlines():
        flags, _, name = line.partition(' ')
        # '>'/'c'/'*' mean the content would be transferred, created or deleted; '.' is metadata.
        if flags[:1] in ('>', 'c', '*', '<'):
            differing.add(name.split('/', 1)[0])
    good = set()
    for folder in folders:
        if folder['name'] in differing:
            continue
        if all((args.dest / rel).is_file() and (args.dest / rel).stat().st_size == size
               for rel, size, _ in folder['files']):
            good.add(folder['name'])
    return good


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--host', default='root@personal-vps')
    parser.add_argument('--state', default='/srv/atemporal/state')
    parser.add_argument('--repo', default='/srv/atemporal/repo')
    parser.add_argument('--dest', type=Path,
                        default=Path.home() / 'server-backups/personal-vps/games')
    parser.add_argument('--max-age-hours', type=float, default=24)
    parser.add_argument('--dry-run', action='store_true', help='sync and verify, delete nothing')
    args = parser.parse_args(argv)
    args.dest = args.dest.expanduser().resolve()
    args.dest.parent.mkdir(parents=True, exist_ok=True)
    lock = open(args.dest.parent / '.maintenance.lock', 'w')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        print('another maintenance run is in progress', flush=True)
        lock.close()
        return 0
    record = {'started': time.strftime('%Y-%m-%dT%H:%M:%S%z'), 'host': args.host,
              'mirror': str(args.dest), 'dry_run': args.dry_run}
    status = 0
    try:
        record['rsync_exit'] = sync(args)
        plan = on_server(args, 'plan')
        verified = verify(args, plan['folders'])
        record['candidates'] = len(plan['folders'])
        record['verified'] = len(verified)
        record['unverified'] = sorted({f['name'] for f in plan['folders']} - verified)
        record['protected_games'] = plan['protected_games']
        if record['unverified']:
            raise RuntimeError('checksum confirmation failed; cleanup stopped')
        payload = {'folders': [f for f in plan['folders'] if f['name'] in verified],
                   'releases': plan['releases'], 'current': plan['current']}
        if args.dry_run:
            record['would_delete_folders'] = sorted(verified)
            record['would_remove_releases'] = plan['releases']
            record['free_bytes'] = plan['free_bytes']
        else:
            record.update(on_server(args, 'delete', payload))
        record['ok'] = True
    except Exception as e:  # recorded and reported; nothing is deleted after a failure
        record['ok'], record['error'] = False, str(e)
        status = 1
    record['finished'] = time.strftime('%Y-%m-%dT%H:%M:%S%z')
    with open(args.dest.parent / 'maintenance-runs.jsonl', 'a') as log:
        log.write(json.dumps(record) + '\n')
    lock.close()
    print(json.dumps(record), flush=True)
    return status


if __name__ == '__main__':
    sys.exit(main())
