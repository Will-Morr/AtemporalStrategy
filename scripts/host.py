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


SUPERVISOR_PROTOCOL = 1
SCRIPT_PATH = Path(__file__).resolve()


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
    # The server inherits the supervisor's priority. Lower only build processes
    # and their children so a release can compile while a game is simulating.
    build_priority = ['nice', '-n', '9']
    run([*build_priority, 'npm', 'ci', '--prefix', 'client', '--ignore-scripts'], cwd=release, env=env)
    run([*build_priority, 'npm', 'run', 'build', '--prefix', 'client'], cwd=release, env=env)
    run([*build_priority, 'cargo', 'build', '--release', '--locked', '-p', 'atemporal-server'], cwd=release, env=env)
    (release / '.ready').touch()
    return release


def write_atomic(path, data):
    temporary = path.with_suffix('.tmp')
    temporary.write_bytes(data)
    temporary.replace(path)


def exec_supervisor(script, args, lock, candidate=None):
    env = {**os.environ, 'ATEMPORAL_HOST_LOCK_FD': str(lock.fileno())}
    if candidate is not None:
        env['ATEMPORAL_HOST_CANDIDATE'] = candidate
    lock_fd = lock.fileno()
    os.set_inheritable(lock_fd, True)
    try:
        os.execve(sys.executable, [sys.executable, str(script), *args], env)
    finally:
        os.set_inheritable(lock_fd, False)


def stop_server(child):
    if child is not None and child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=20)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def launch_server(release, revision, port, games):
    return subprocess.Popen([str(release / 'target/release/atemporal-server'),
        '--port', str(port), '--coordinator-dir', str(games)], cwd=release,
        env={**os.environ, 'ATEMPORAL_COMMIT': revision})


def probe_server(release, revision, port, games, stopping):
    child = launch_server(release, revision, port, games)
    if not stopping.wait(5) and child.poll() is None and healthy(port):
        return child
    stop_server(child)
    return None


def new_supervisor(release, running_source):
    script = release / 'scripts/host.py'
    if not script.is_file():
        return None
    source = script.read_bytes()
    if source == running_source or b'SUPERVISOR_PROTOCOL = 1\n' not in source:
        return None
    # Check parsing and imports before asking the coordinator to drain.
    run([sys.executable, str(script), '--help'], stdout=subprocess.DEVNULL)
    return script


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
    inherited_fd = os.environ.pop('ATEMPORAL_HOST_LOCK_FD', None)
    handoff = os.environ.pop('ATEMPORAL_HOST_CANDIDATE', None)
    if inherited_fd is None:
        lock = (state / 'host.lock').open('w')
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    else:
        lock = os.fdopen(int(inherited_fd), 'w')
        os.set_inheritable(lock.fileno(), False)
    releases = state / 'releases'
    releases.mkdir(exist_ok=True)
    games = state / 'games'
    games.mkdir(exist_ok=True)
    request = games / 'update-request'
    current_file = state / 'current'
    installed = state / 'supervisor.py'
    rejected = state / 'supervisor-rejected'
    # Always pass the original repo explicitly. A versioned or installed
    # supervisor lives outside it and would otherwise change the default.
    forwarded_args = ['--repo', str(repo), '--state', str(state), '--port', str(args.port),
        '--poll-seconds', str(args.poll_seconds)]
    running_source = SCRIPT_PATH.read_bytes()
    if handoff is None:
        rejected_revision = rejected.read_text().strip() if rejected.exists() else None
        if (rejected_revision is not None and current_file.exists()
                and current_file.read_text().strip() == rejected_revision):
            # The coordinator was already confirmed and published before a restart.
            # Finish installing the paired supervisor if interruption fell between the two writes.
            write_atomic(installed, (releases / rejected_revision / 'scripts/host.py').read_bytes())
            rejected.unlink()
        if installed.exists():
            if SCRIPT_PATH != installed or running_source != installed.read_bytes():
                exec_supervisor(installed, forwarded_args, lock)
        else:
            write_atomic(installed, running_source)
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
        if handoff is not None:
            candidate_release = releases / handoff
            request.unlink(missing_ok=True)
            try:
                child = probe_server(candidate_release, handoff, args.port, games, stopping)
                if child is not None:
                    write_atomic(current_file, handoff.encode())
                    write_atomic(installed, running_source)
                    rejected.unlink(missing_ok=True)
                    current, release = handoff, candidate_release
                    print(f'Updated supervisor and server to {current}', flush=True)
                elif not stopping.is_set():
                    raise RuntimeError('candidate coordinator failed startup')
            except (OSError, RuntimeError) as error:
                stop_server(child)
                child = None
                print(f'Supervisor handoff failed; returning to previous release: {error}',
                    file=sys.stderr, flush=True)
                exec_supervisor(installed, forwarded_args, lock)
        next_poll = 0
        while not stopping.is_set():
            if child is None:
                if pending:
                    request.touch()
                else:
                    request.unlink(missing_ok=True)
                child = launch_server(release, current, args.port, games)
                print(f'Serving {current} on port {args.port}', flush=True)
            code = child.poll()
            if code is not None:
                child = None
                if code == 75 and pending:
                    if stopping.is_set():
                        break
                    candidate, candidate_release, supervisor = pending
                    # Smoke start before changing the durable release pointer. If startup fails,
                    # return to the previous binary and its unchanged game data.
                    request.unlink(missing_ok=True)
                    if supervisor is not None:
                        try:
                            # A failed exec or a supervisor that exits before completing the
                            # handoff must not create a restart loop on this revision.
                            write_atomic(rejected, candidate.encode())
                            exec_supervisor(supervisor, forwarded_args, lock, candidate)
                        except OSError as error:
                            print(f'Supervisor handoff failed; returning to previous release: {error}',
                                file=sys.stderr, flush=True)
                            pending = None
                            continue
                    try:
                        probe = probe_server(candidate_release, candidate, args.port, games, stopping)
                    except OSError as error:
                        print(f'Candidate startup error: {error}', file=sys.stderr, flush=True)
                        probe = None
                    if probe is not None:
                        current, release, child = candidate, candidate_release, probe
                        write_atomic(current_file, current.encode())
                        print(f'Updated to {current}', flush=True)
                    else:
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
                    rejected_revision = rejected.read_text().strip() if rejected.exists() else None
                    if revision != current and revision != rejected_revision and pending is None:
                        candidate_release = build(repo, releases, revision)
                        supervisor = new_supervisor(candidate_release, running_source)
                        if not stopping.is_set():
                            pending = (revision, candidate_release, supervisor)
                            request.touch()
                            print(f'{revision} built; waiting for server to become idle', flush=True)
                except (subprocess.CalledProcessError, OSError) as error:
                    print(f'Update failed; existing server remains in service: {error}', file=sys.stderr, flush=True)
                next_poll = time.monotonic() + args.poll_seconds
            stopping.wait(1)
    finally:
        request.unlink(missing_ok=True)
        stop_server(child)


if __name__ == '__main__':
    main()
