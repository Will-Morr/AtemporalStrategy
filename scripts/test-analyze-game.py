"""Tests use tiny synthetic archives; no dependency on private local games."""
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import unittest

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('analyze', Path(__file__).with_name('analyze-game.py'))
analyze = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyze)


class ArchiveAnalysisTests(unittest.TestCase):
    def test_stream_chunk_boundaries_and_truncation(self):
        with tempfile.TemporaryDirectory() as root:
            p = Path(root) / 'array.json'
            values = [{'text': 'é' * 100000}, {'nested': [1, {}, 'x']}]
            p.write_text(json.dumps(values))
            self.assertEqual(list(analyze.array_records(p)), values)
            for bad in ['[{}', '[{},]', '[{} {}]', '[]garbage']:
                p.write_text(bad)
                with self.assertRaises(ValueError):
                    list(analyze.array_records(p))

    def test_revision_metrics_and_pending_are_separate(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            def write(name, data):
                p = root / name
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(json.dumps(data))
            write('manifest.json', dict(match_id='test', config=dict(player_count=1), fingerprint={}))
            player = dict(player_id=0, bank=42, counters=dict(mined=12),
                          currently_eliminated=False, elimination_reasons=[])
            entity = dict(owner=0, type_key='miner', lifecycle='complete', action=dict(kind='attack_move'))
            states = [dict(tick=0, players=[player], entities=[entity], blueprints=[]),
                      dict(tick=100, players=[player], entities=[], blueprints=[])]
            write('results/1/checkpoints.json', states)
            write('results/1/complete.json', {})
            turn = dict(player=0, round=1, tick=0, commands=[])
            write('turns/1-0.json', turn)
            write('turns/1-0.draft', dict(tick=0, commands=[]))
            write('turns/2-0.draft', dict(tick=100, commands=[]))
            write('rounds/1.json', dict(revision=1, turns=['turns/1-0.json'], outcome=dict(kind='win'),
                                     command_outcomes=[dict(command_id=dict(player=0), skipped=['gone'])]))
            report = analyze.analyze(root, tick=90)
            revision = report['revisions'][0]
            p = revision['players']['0']
            self.assertEqual(p['peak_non_mining_miners'], 1)
            self.assertEqual(p['peak_units'], {'miner': 1})
            self.assertEqual(p['final']['units'], {})
            self.assertEqual(p['snapshot']['tick'], 100)
            self.assertEqual(p['requested_tick'], 90)
            self.assertEqual(revision['turns'], [turn])
            self.assertEqual(len(revision['skipped']), 1)
            self.assertEqual([d['file'] for d in report['pending_drafts']], ['2-0.draft'])


if __name__ == '__main__':
    unittest.main()
