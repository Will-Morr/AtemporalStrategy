#!/usr/bin/env python3
"""Exercise release staging without rebuilding the game or touching a live deployment."""
import importlib.util
from pathlib import Path
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
                if args[0] == 'cargo':
                    raise subprocess.CalledProcessError(1, args)
            with patch.object(host, 'run', side_effect=command):
                with self.assertRaises(subprocess.CalledProcessError):
                    host.build(root, root, 'revision')
            self.assertFalse((release / '.ready').exists())
            with patch.object(host, 'run') as run:
                self.assertEqual(host.build(root, root, 'revision'), release)
                self.assertEqual([call.args[0][0] for call in run.call_args_list], ['npm', 'npm', 'cargo'])
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
            def stage(revision):
                release = state / 'releases' / revision
                binary = release / 'target/release/atemporal-server'
                binary.parent.mkdir(parents=True)
                binary.write_text("""#!/usr/bin/env python3
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
    if (root/'update-request').exists(): os._exit(75)
""")
                binary.chmod(0o755)
                (release / '.ready').touch()
            stage(first)
            with socket.socket() as probe:
                probe.bind(('127.0.0.1', 0))
                port = probe.getsockname()[1]
            # The third revision fails its build, without changing either live release.
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
            with (root / 'host.log').open('w') as log:
                child = subprocess.Popen(['python3', str(Path(host.__file__)), '--repo', str(repo), '--state', str(state), '--port', str(port), '--poll-seconds', '.2'], env=env, stdout=log, stderr=log)
                try:
                    eventually(lambda: host.healthy(port))
                    # Publish only after staging so the fixture does not trigger a real build.
                    (repo / 'version').write_text('second'); git('add', '.', cwd=repo); git('commit', '-m', 'second', cwd=repo)
                    second = git('rev-parse', 'HEAD', cwd=repo); stage(second); git('push', 'origin', 'main', cwd=repo)
                    eventually(lambda: (state / 'current').read_text() == second)
                    self.assertTrue(host.healthy(port))
                    commit('broken build')
                    eventually(lambda: 'Update failed' in (root / 'host.log').read_text())
                    self.assertEqual((state / 'current').read_text(), second)
                    self.assertTrue(host.healthy(port))
                finally:
                    child.terminate(); child.wait(timeout=25)

    def test_missing_server_is_not_healthy(self):
        with patch.object(host.urllib.request, 'urlopen', side_effect=OSError('offline')):
            self.assertFalse(host.healthy(8080))


if __name__ == '__main__':
    unittest.main()
