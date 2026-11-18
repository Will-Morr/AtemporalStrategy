#!/usr/bin/env python3
"""Read-only, dependency-free archive diagnostics. Never reads session/token files."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys


def read(path):
    with path.open() as f:
        return json.load(f)


def array_records(path):
    """Stream top-level JSON arrays; memory is bounded by the largest record."""
    decoder = json.JSONDecoder()
    with path.open() as f:
        buffer = ''
        eof = False
        def refill():
            nonlocal buffer, eof
            chunk = f.read(65536)
            buffer += chunk
            eof = not chunk
        refill()
        while not buffer.lstrip() and not eof:
            refill()
        buffer = buffer.lstrip()
        if not buffer.startswith('['):
            raise ValueError(f'{path}: expected JSON array')
        buffer = buffer[1:]
        first = True
        while True:
            buffer = buffer.lstrip()
            while not buffer and not eof:
                refill()
                buffer = buffer.lstrip()
            if buffer.startswith(']'):
                buffer = buffer[1:]
                if buffer.strip() or f.read().strip():
                    raise ValueError(f'{path}: trailing data')
                return
            if not first:
                if not buffer.startswith(','):
                    raise ValueError(f'{path}: missing separator or truncated array')
                buffer = buffer[1:].lstrip()
            while True:
                try:
                    value, end = decoder.raw_decode(buffer)
                    break
                except json.JSONDecodeError:
                    if eof:
                        raise ValueError(f'{path}: invalid or truncated JSON') from None
                    refill()
                    buffer = buffer.lstrip()
            yield value
            buffer = buffer[end:]
            first = False


def snapshot(state, player, details=False):
    own = [e for e in state['entities'] if e['owner'] == player]
    complete = [e for e in own if e['lifecycle'] == 'complete']
    status = next(p for p in state['players'] if p['player_id'] == player)
    factories = []
    for e in own:
        p = e.get('production')
        if p is not None:
            factories.append(dict(id=e['id'], tile=e['tile'], stored_order=p['stored_order'],
                                  active=p['active_item'], pending=p['pending_items'],
                                  loop=p['loop_enabled']))
    result = dict(tick=state['tick'], bank=status['bank'], counters=status['counters'],
                  eliminated=status['currently_eliminated'],
                  elimination_reasons=status['elimination_reasons'],
                  units=dict(Counter(e['type_key'] for e in complete)),
                  orders=dict(Counter(e['type_key'] + ':' + e['action']['kind'] for e in complete)),
                  non_mining_miners=sum(e['type_key'] == 'miner' and e['action']['kind'] != 'mine'
                                        for e in complete),
                  empty_factories=sum(not f['active'] and not f['pending'] for f in factories),
                  queued=dict(Counter(q['type_key'] for f in factories
                                      for q in f['pending'] + ([f['active']] if f['active'] else []))))
    if details:
        result.update(entities=own, factories=factories,
                      blueprints=[b for b in state['blueprints'] if b['owner'] == player])
    return result


def analyze(match, revision=None, player=None, tick=None, events=False):
    manifest = read(match / 'manifest.json')
    players = [player] if player is not None else list(range(manifest['config']['player_count']))
    if any(p < 0 or p >= manifest['config']['player_count'] for p in players):
        raise ValueError('player is outside this match')
    result = dict(match=str(match), match_id=manifest['match_id'],
                  fingerprint=manifest['fingerprint'], profiles=manifest.get('profiles', []),
                  limits=['Checkpoint peaks and first-observed ticks are sampled, not exact.',
                          'Each revision is an alternate timeline; do not sum counters across revisions.',
                          'Drafts without accepted turn files are pending, not executed.',
                          'Read-only omniscient diagnostics; not input for bot decisions.'], revisions=[])
    dirs = sorted((p for p in (match / 'results').iterdir() if p.name.isdigit()), key=lambda p: int(p.name))
    if revision is not None:
        dirs = [p for p in dirs if int(p.name) == revision]
        if not dirs:
            raise ValueError(f'revision {revision} not found')
    for directory in dirs:
        rev = int(directory.name)
        if not (directory / 'complete.json').exists():
            result.setdefault('incomplete_revisions', []).append(rev)
            continue
        rounds = [read(p) for p in sorted((match / 'rounds').glob('*.json'))]
        record = next((r for r in rounds if r['revision'] == rev), {})
        report = dict(revision=rev, outcome=record.get('outcome'), score=record.get('score'),
                      base_tick=record.get('base_tick'), players={}, turns=[], skipped=[])
        for relative in record.get('turns', []):
            # Filenames only: do not follow paths supplied by an archive outside its turns folder.
            turn = read(match / 'turns' / Path(relative).name)
            if turn['player'] in players:
                report['turns'].append(turn)
        report['skipped'] = [o for o in record.get('command_outcomes', [])
                             if o.get('skipped') and o['command_id']['player'] in players]
        for state in array_records(directory / 'checkpoints.json'):
            for p in players:
                s = snapshot(state, p)
                entry = report['players'].setdefault(str(p), dict(peak_units={},
                    peak_non_mining_miners=0, first_observed={}, final=None))
                for key, count in s['units'].items():
                    entry['peak_units'][key] = max(entry['peak_units'].get(key, 0), count)
                    entry['first_observed'].setdefault(key, state['tick'])
                entry['peak_non_mining_miners'] = max(entry['peak_non_mining_miners'], s['non_mining_miners'])
                entry['final'] = s
                if tick is not None and ('snapshot' not in entry or
                    abs(state['tick'] - tick) < abs(entry['snapshot']['tick'] - tick)):
                    entry['snapshot'] = snapshot(state, p, details=True)
                    entry['requested_tick'] = tick
        if events:
            counts = Counter()
            for event in array_records(directory / 'events.json'):
                counts[event['event']['kind']] += 1
            report['event_counts_all_players'] = dict(counts)
        result['revisions'].append(report)
    result['pending_drafts'] = [dict(file=p.name, **read(p)) for p in sorted((match / 'turns').glob('*.draft'))
                               if not p.with_suffix('.json').exists()
                               and (player is None or p.stem.split('-')[-1] == str(player))]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('path', nargs='?', type=Path, default=Path('games'), help='games root, game directory, or match archive')
    parser.add_argument('--latest', type=int, default=2, help='number of most recently created game directories')
    parser.add_argument('--revision', type=int)
    parser.add_argument('--player', type=int)
    parser.add_argument('--tick', type=int, help='include full nearest checkpoint, labelled with its actual tick')
    parser.add_argument('--events', action='store_true', help='also stream event counts')
    parser.add_argument('--out', type=Path, help='write JSON here instead of stdout')
    args = parser.parse_args()
    try:
        if args.latest < 1:
            raise ValueError('--latest must be positive')
        if (args.path / 'manifest.json').exists():
            matches = [args.path]
        elif (args.path / 'replays').is_dir():
            matches = sorted((args.path / 'replays').glob('match-*'))
        else:
            games = sorted(args.path.glob('game-*'), key=lambda p: int(p.name.split('-')[1]))[-args.latest:]
            matches = [m for g in games for m in sorted((g / 'replays').glob('match-*'))]
        if not matches:
            raise ValueError(f'no match archives under {args.path}')
        reports = [analyze(m, args.revision, args.player, args.tick, args.events) for m in matches]
        output = json.dumps(reports, indent=2) + '\n'
        if args.out:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(output)
        else:
            print(output, end='')
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f'archive analysis failed: {error}\n')


if __name__ == '__main__':
    main()
