#!/usr/bin/env python3
"""One machine, one server, polling origin/main; build before requesting an idle restart."""
import argparse
import fcntl
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time
import urllib.request


def healthy(port):
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/api/directory", timeout=2) as response:
            return response.status == 200
    except OSError:
        return False


def run(args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def build(repo, releases, revision):
    release = releases / revision
    if (release / '.ready').exists():
        return release
    if not release.exists():
        run(['git', '-C', str(repo), 'worktree', 'add', '--detach', str(release), revision])
    env = {**os.environ, 'CARGO_BUILD_JOBS': '2'}
    run(['npm', 'ci', '--prefix', 'client', '--ignore-scripts'], cwd=release, env=env)
    run(['npm', 'run', 'build', '--prefix', 'client'], cwd=release, env=env)
    run(['cargo', 'build', '--release', '--locked', '-p', 'atemporal-server'], cwd=release, env=env)
    (release / '.ready').touch()
    return release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--port', type=int, default=8080)
    parser.add_argument('--poll-seconds', type=float, default=60)
    args = parser.parse_args()
    if args.poll_seconds <= 0 or not 0 < args.port < 65536:
        parser.error('poll interval must be positive; port must be 1..65535')
    repo, state = args.repo.resolve(), args.state.resolve()
    state.mkdir(parents=True, exist_ok=True)
    lock = (state / 'host.lock').open('w')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    releases = state / 'releases'
    releases.mkdir(exist_ok=True)
    games = state / 'games'
    games.mkdir(exist_ok=True)
    request = games / 'update-request'
    current_file = state / 'current'
    stopping = threading.Event()
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, lambda *_: stopping.set())
    child = None
    pending = None
    try:
        if current_file.exists():
            current = current_file.read_text().strip()
            release = releases / current
            if not (release / '.ready').exists():
                raise RuntimeError('Current release is missing or incomplete')
        else:
            run(['git', '-C', str(repo), 'fetch', 'origin', 'main'])
            current = run(['git', '-C', str(repo), 'rev-parse', 'FETCH_HEAD'], capture_output=True).stdout.strip()
            release = build(repo, releases, current)
            current_file.write_text(current)
        next_poll = 0
        while not stopping.is_set():
            if child is None:
                if pending:
                    request.touch()
                else:
                    request.unlink(missing_ok=True)
                child = subprocess.Popen([str(release / 'target/release/atemporal-server'),
                    '--port', str(args.port), '--coordinator-dir', str(games)], cwd=release)
                print(f'Serving {current} on port {args.port}', flush=True)
            code = child.poll()
            if code is not None:
                child = None
                if code == 75 and pending:
                    candidate, candidate_release = pending
                    # Smoke start before changing the durable release pointer. If startup fails,
                    # return to the previous binary and its unchanged game data.
                    request.unlink(missing_ok=True)
                    probe = subprocess.Popen([str(candidate_release / 'target/release/atemporal-server'),
                        '--port', str(args.port), '--coordinator-dir', str(games)], cwd=candidate_release)
                    if not stopping.wait(5) and probe.poll() is None and healthy(args.port):
                        current, release, child = candidate, candidate_release, probe
                        tmp = current_file.with_suffix('.tmp')
                        tmp.write_text(current)
                        tmp.replace(current_file)
                        print(f'Updated to {current}', flush=True)
                    else:
                        if probe.poll() is None:
                            probe.terminate()
                            probe.wait(timeout=20)
                        print('Candidate failed startup; returning to previous release', file=sys.stderr, flush=True)
                    pending = None
                else:
                    print(f'Server exited ({code}); restarting in five seconds', file=sys.stderr, flush=True)
                    stopping.wait(5)
                continue
            if time.monotonic() >= next_poll:
                try:
                    run(['git', '-C', str(repo), 'fetch', 'origin', 'main'])
                    revision = run(['git', '-C', str(repo), 'rev-parse', 'FETCH_HEAD'], capture_output=True).stdout.strip()
                    if revision == current and pending:
                        request.unlink(missing_ok=True)
                        pending = None
                    if revision != current and pending is None:
                        candidate_release = build(repo, releases, revision)
                        if not stopping.is_set():
                            pending = (revision, candidate_release)
                            request.touch()
                            print(f'{revision} built; waiting for server to become idle', flush=True)
                except (subprocess.CalledProcessError, OSError) as error:
                    print(f'Update failed; existing server remains in service: {error}', file=sys.stderr, flush=True)
                next_poll = time.monotonic() + args.poll_seconds
            stopping.wait(1)
    finally:
        request.unlink(missing_ok=True)
        if child is not None and child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=20)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


if __name__ == '__main__':
    main()
