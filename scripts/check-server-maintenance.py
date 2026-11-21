#!/usr/bin/env python3
"""Exercise server-maintenance.py end to end against local directories (host 'local')."""
import importlib.util
import json
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'maintenance', Path(__file__).with_name('server-maintenance.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

DAY = 24 * 3600


def age(path, seconds):
    t = time.time() - seconds
    for dirpath, dirnames, filenames in os.walk(path, topdown=False):
        for name in filenames + dirnames:
            os.utime(os.path.join(dirpath, name), (t, t), follow_symlinks=False)
    os.utime(path, (t, t))


def git(*args, cwd):
    subprocess.run(['git', *args], cwd=cwd, check=True, capture_output=True)


class MaintenanceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.state, self.repo, self.dest = root / 'state', root / 'repo', root / 'backup' / 'games'
        self.games = self.state / 'games'
        directory = {'games': {}}
        for name, expired, seconds in [('game-old', True, 2 * DAY), ('game-fresh', True, 3600),
                                       ('game-open', False, 2 * DAY)]:
            replay = self.games / name / 'replays' / 'match-1' / 'results' / '7'
            replay.mkdir(parents=True)
            (replay / 'events.json').write_text(name * 1000)
            (self.games / name / 'setup.json').write_text('{}')
            age(self.games / name, seconds)
            directory['games'][name] = {'id': name, 'expired': expired}
        orphan = self.games / 'game-orphan'
        orphan.mkdir()
        (orphan / 'worker.log').write_text('log')
        age(orphan, 3 * DAY)
        (self.games / 'directory.json').write_text(json.dumps(directory))
        (self.games / 'visits.jsonl').write_text('{}\n')
        os.utime(self.games / 'visits.jsonl', (0, 0))
        self.repo.mkdir()
        git('init', '-q', cwd=self.repo)
        git('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'x',
            cwd=self.repo)
        releases = self.state / 'releases'
        for i, name in enumerate(['old1', 'current', 'old2', 'newest', 'building']):
            git('worktree', 'add', '-q', '--detach', str(releases / name), cwd=self.repo)
            if name != 'building':
                (releases / name / '.ready').touch()
                os.utime(releases / name / '.ready', (1000 + i, 1000 + i))
        (self.state / 'current').write_text('current\n')

    def tearDown(self):
        self.tmp.cleanup()

    def run_tool(self, *extra):
        return m.main(['--host', 'local', '--state', str(self.state), '--repo', str(self.repo),
                       '--dest', str(self.dest), *extra])

    def test_deletes_only_verified_expired_old_games_and_old_releases(self):
        self.assertEqual(self.run_tool(), 0)
        left = sorted(p.name for p in self.games.iterdir())
        self.assertEqual(left, ['directory.json', 'game-fresh', 'game-open', 'visits.jsonl'])
        for name in ['game-old', 'game-orphan', 'game-fresh', 'game-open']:
            self.assertTrue((self.dest / name / 'setup.json').exists() or name == 'game-orphan')
        self.assertEqual((self.dest / 'game-old/replays/match-1/results/7/events.json').read_text(),
                         'game-old' * 1000)
        self.assertEqual(sorted(p.name for p in (self.state / 'releases').iterdir()),
                         ['building', 'current', 'newest'])
        run = json.loads((self.dest.parent / 'maintenance-runs.jsonl').read_text().splitlines()[-1])
        self.assertTrue(run['ok'])
        self.assertEqual(sorted(run['deleted_folders']), ['game-old', 'game-orphan'])
        self.assertEqual(sorted(run['removed_releases']), ['old1', 'old2'])
        # A second run keeps the mirror intact even though the server copies are gone.
        self.assertEqual(self.run_tool(), 0)
        self.assertTrue((self.dest / 'game-old/setup.json').exists())

    def test_five_completions_rank_across_games_and_rematches(self):
        directory = json.loads((self.games / 'directory.json').read_text())
        for i in range(8):
            # The two newest completions share one lobby. The newest unfinished match
            # cannot displace an older win. Start order is the reverse of finish order.
            name = f'game-ranked-{min(i, 6)}'
            directory['games'][name] = {'expired': True}
            root = self.games / name / 'replays' / f'match-{8-i}'
            root.mkdir(parents=True)
            (root / 'manifest.json').write_text('{}')
            (root / 'rounds').mkdir()
            last = root / 'rounds/10.json'
            last.write_text(json.dumps({'score': {'match_winners': [0] if i else []}}))
            if i % 2:
                (root / 'completed.json').write_text(str(i * 1000))
            os.utime(last, (i, i))
        for root in self.games.glob('game-ranked-*'):
            age(root, 3 * DAY)
            for last in root.glob('replays/*/rounds/10.json'):
                i = 8 - int(last.parent.parent.name.split('-')[1])
                os.utime(last, (i, i))
        (self.games / 'directory.json').write_text(json.dumps(directory))
        self.assertEqual(self.run_tool(), 0)
        self.assertEqual(sorted(p.name for p in self.games.glob('game-ranked-*')),
                         [f'game-ranked-{i}' for i in (3, 4, 5, 6)])
        self.assertTrue((self.dest / 'game-ranked-2/replays/match-6/manifest.json').is_file())

    def test_dry_run_deletes_nothing(self):
        self.assertEqual(self.run_tool('--dry-run'), 0)
        self.assertTrue((self.games / 'game-old').exists())
        self.assertEqual(len(list((self.state / 'releases').iterdir())), 5)
        self.assertTrue((self.dest / 'game-old/setup.json').exists())

    def test_mismatched_mirror_copy_keeps_server_folder(self):
        self.assertEqual(self.run_tool('--dry-run'), 0)
        bad = self.dest / 'game-old/replays/match-1/results/7/events.json'
        bad.write_text('X' * len(bad.read_text()))  # same size, different bytes
        with patch.object(m, 'sync', return_value=0):
            self.assertEqual(self.run_tool(), 1)
        self.assertTrue((self.games / 'game-old/replays/match-1/results/7/events.json').exists())
        self.assertTrue((self.games / 'game-orphan').exists())
        run = json.loads((self.dest.parent / 'maintenance-runs.jsonl').read_text().splitlines()[-1])
        self.assertEqual(run['unverified'], ['game-old'])

    def test_file_changed_after_planning_is_not_deleted(self):
        real = m.on_server
        def delayed(args, op, payload=None):
            if op == 'delete':
                path = self.games / 'game-old/setup.json'
                path.write_text('{"late": true}')
            return real(args, op, payload)
        with patch.object(m, 'on_server', side_effect=delayed):
            self.assertEqual(self.run_tool(), 0)
        self.assertEqual((self.games / 'game-old/setup.json').read_text(), '{"late": true}')
        self.assertTrue((self.games / 'game-old/replays').exists())

    def test_sync_failure_deletes_nothing(self):
        with patch.object(m, 'rsync_source', return_value='/nonexistent/games/'):
            self.assertEqual(self.run_tool(), 1)
        self.assertTrue((self.games / 'game-old').exists())
        self.assertEqual(len(list((self.state / 'releases').iterdir())), 5)
        run = json.loads((self.dest.parent / 'maintenance-runs.jsonl').read_text().splitlines()[-1])
        self.assertFalse(run['ok'])


if __name__ == '__main__':
    unittest.main()
