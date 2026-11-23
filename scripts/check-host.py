#!/usr/bin/env python3
"""Exercise release staging without rebuilding the game or touching a live deployment."""
import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import os
import socket
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('host', Path(__file__).with_name('host.py'))
host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host)


class ReleaseTests(unittest.TestCase):
    def test_failed_build_never_marks_release_ready_and_retry_finishes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            release = root / 'revision'
            def command(args, **kwargs):
                if args[0] == 'git':
                    release.mkdir()
                if args[:4] == ['nice', '-n', '9', 'cargo']:
                    raise subprocess.CalledProcessError(1, args)
            with patch.object(host, 'run', side_effect=command):
                with self.assertRaises(subprocess.CalledProcessError):
                    host.build(root, root, 'revision')
            self.assertFalse((release / '.ready').exists())
            with patch.object(host, 'run') as run:
                self.assertEqual(host.build(root, root, 'revision'), release)
                self.assertEqual(
                    [call.args[0][:4] for call in run.call_args_list],
                    [['nice', '-n', '9', name] for name in ['npm', 'npm', 'cargo']],
                )
            self.assertTrue((release / '.ready').exists())
            with patch.object(host, 'run') as run:
                host.build(root, root, 'revision')
                run.assert_not_called()

    def test_supervisor_installs_new_main_and_keeps_serving_on_build_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, origin, state = root / 'repo', root / 'origin', root / 'state'
            def git(*args, cwd=None):
                return subprocess.check_output(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', *args], cwd=cwd, stderr=subprocess.DEVNULL, text=True).strip()
            git('init', '--bare', '--initial-branch=main', str(origin))
            git('clone', str(origin), str(repo))
            def commit(value):
                (repo / 'version').write_text(value)
                git('add', '.', cwd=repo)
                git('commit', '-m', value, cwd=repo)
                revision = git('rev-parse', 'HEAD', cwd=repo)
                git('push', 'origin', 'main', cwd=repo)
                return revision
            first = commit('first')
            state.mkdir()
            (state / 'current').write_text(first)
            # Small HTTP stand-in: the real coordinator's idle gate is tested in Playwright.
            source = Path(host.__file__).read_text()

            def stage(revision, supervisor=None, fails_startup=False):
                release = state / 'releases' / revision
                if supervisor is not None:
                    script = release / 'scripts/host.py'
                    script.parent.mkdir(parents=True)
                    script.write_text(supervisor)
                binary = release / 'target/release/atemporal-server'
                binary.parent.mkdir(parents=True)
                binary.write_text("#!/usr/bin/env python3\nraise SystemExit(1)\n" if fails_startup else """#!/usr/bin/env python3
import sys, pathlib, os
from http.server import HTTPServer, BaseHTTPRequestHandler
port=int(sys.argv[sys.argv.index('--port')+1])
root=pathlib.Path(sys.argv[sys.argv.index('--coordinator-dir')+1])
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b'{}')
    def log_message(self,*args): pass
server=HTTPServer(('127.0.0.1',port),Handler); server.timeout=.1
while True:
    server.handle_request()
    if (root/'update-request').exists() and not (root/'hold-update').exists(): os._exit(75)
""")
                binary.chmod(0o755)
                (release / '.ready').touch()
            stage(first)
            with socket.socket() as probe:
                probe.bind(('127.0.0.1', 0))
                port = probe.getsockname()[1]
            # Unstaged revisions fail their Rust build.
            commands = root / 'commands'; commands.mkdir()
            for name, code in [('npm', 0), ('cargo', 1)]:
                path = commands / name; path.write_text(f'#!/bin/sh\nexit {code}\n'); path.chmod(0o755)
            env = {**os.environ, 'PATH': str(commands) + ':' + os.environ['PATH']}
            def eventually(predicate):
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    if predicate(): return
                    time.sleep(.1)
                self.fail('supervisor condition timed out')
            log_path = root / 'host.log'
            args = ['python3', str(Path(host.__file__)), '--repo', str(repo), '--state', str(state), '--port', str(port), '--poll-seconds', '.2']
            with log_path.open('w') as log:
                child = subprocess.Popen(args, env=env, stdout=log, stderr=log)
                try:
                    eventually(lambda: host.healthy(port))
                    installed = state / 'supervisor.py'
                    self.assertEqual(installed.read_text(), source)
                    hold = state / 'games/hold-update'
                    hold.touch()
                    # Publish only after staging so the fixture does not trigger a real build.
                    (repo / 'version').write_text('second'); git('add', '.', cwd=repo); git('commit', '-m', 'second', cwd=repo)
                    second = git('rev-parse', 'HEAD', cwd=repo)
                    updated_source = source + '\n# supervisor version two\n'
                    stage(second, updated_source)
                    git('push', 'origin', 'main', cwd=repo)
                    eventually(lambda: (state / 'games/update-request').exists())
                    self.assertEqual((state / 'current').read_text(), first)
                    self.assertEqual(installed.read_text(), source)
                    self.assertTrue(host.healthy(port))
                    hold.unlink()
                    eventually(lambda: (state / 'current').read_text() == second)
                    eventually(lambda: installed.read_text() == updated_source)
                    eventually(lambda: f'Updated supervisor and server to {second}' in log_path.read_text())
                    self.assertIsNone(child.poll())  # In-place exec kept the service PID.
                    self.assertTrue(host.healthy(port))
                    commit('broken build')
                    eventually(lambda: 'Update failed' in log_path.read_text())
                    self.assertEqual((state / 'current').read_text(), second)
                    self.assertTrue(host.healthy(port))

                    (repo / 'version').write_text('invalid supervisor')
                    git('add', '.', cwd=repo)
                    git('commit', '-m', 'invalid supervisor', cwd=repo)
                    invalid = git('rev-parse', 'HEAD', cwd=repo)
                    stage(invalid, source + '\nraise (\n')
                    git('push', 'origin', 'main', cwd=repo)
                    eventually(lambda: 'SyntaxError' in log_path.read_text())
                    self.assertFalse((state / 'games/update-request').exists())
                    self.assertEqual((state / 'current').read_text(), second)
                    self.assertTrue(host.healthy(port))

                    (repo / 'version').write_text('broken handoff')
                    git('add', '.', cwd=repo)
                    git('commit', '-m', 'broken handoff', cwd=repo)
                    broken = git('rev-parse', 'HEAD', cwd=repo)
                    stage(broken, source + '\n# failing version\n', fails_startup=True)
                    git('push', 'origin', 'main', cwd=repo)
                    eventually(lambda: 'Supervisor handoff failed' in log_path.read_text())
                    eventually(lambda: host.healthy(port))
                    self.assertEqual((state / 'current').read_text(), second)
                    self.assertEqual((state / 'supervisor-rejected').read_text(), broken)
                    self.assertEqual(installed.read_text(), updated_source)
                    self.assertIsNone(child.poll())

                    # Simulate a service reboot from the stale checkout after old build cleanup.
                    child.terminate()
                    child.wait(timeout=25)
                    shutil.rmtree(state / 'releases' / first)
                    (state / 'releases' / second / 'scripts/host.py').unlink()
                    child = subprocess.Popen(args, env=env, stdout=log, stderr=log)
                    eventually(lambda: host.healthy(port))
                    self.assertEqual((state / 'current').read_text(), second)
                    self.assertEqual(installed.read_text(), updated_source)
                finally:
                    if child.poll() is None:
                        child.terminate()
                        child.wait(timeout=25)

    def test_missing_server_is_not_healthy(self):
        with patch.object(host.urllib.request, 'urlopen', side_effect=OSError('offline')):
            self.assertFalse(host.healthy(8080))


if __name__ == '__main__':
    unittest.main()
