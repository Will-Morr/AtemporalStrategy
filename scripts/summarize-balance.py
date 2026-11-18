#!/usr/bin/env python3
"""Summarize a balance_sweep JSONL file. Scores give half credit for a draw."""
import argparse
import csv
import gzip
import json
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from statistics import mean


@dataclass
class Duel:
    wins: int = 0
    draws: int = 0
    losses: int = 0
    survivor_margin: float = 0.0
    remaining: float = 0.0
    spent: float = 0.0
    winning_ticks: int = 0

    @property
    def count(self):
        return self.wins + self.draws + self.losses

    @property
    def score(self):
        return (self.wins + self.draws / 2) / self.count

    def add(self, winner, side, margin, result, ticks):
        self.remaining += result["health_weighted_unit_matter"]
        self.spent += result["unit_spend"]
        if winner is None:
            self.draws += 1
        elif winner == side:
            self.wins += 1
            self.winning_ticks += ticks
        else:
            self.losses += 1
        self.survivor_margin += margin


def read_results(path):
    seats = set()
    pairs = defaultdict(Duel)
    records = 0
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt") as source:
        meta = json.loads(next(source))
        if meta.get("kind") != "metadata" or meta.get("schema_version") != 1:
            raise ValueError("Expected a balance_sweep version 1 metadata record")
        for line in source:
            trial = json.loads(line)
            if trial["kind"] != "trial":
                raise ValueError("Expected a trial record")
            a, b = tuple(trial["a"]), tuple(trial["b"])
            identity = (a, b, trial["a_seat"])
            if identity in seats:
                raise ValueError(f"Duplicate trial {identity}")
            seats.add(identity)
            margin = (trial["a_result"]["health_weighted_unit_matter"]
                      - trial["b_result"]["health_weighted_unit_matter"])
            pairs[a, b].add(trial["winner"], "a", margin, trial["a_result"], trial["ticks"])
            pairs[b, a].add(trial["winner"], "b", -margin, trial["b_result"], trial["ticks"])
            records += 1
    return meta, pairs, records, seats


def row(*cells):
    print("|", " | ".join(str(c) for c in cells), "|")


def report(meta, pairs, count):
    pure = sorted({r for pair in pairs for r in pair if len(r) == 1})
    mixes = sorted({r for pair in pairs for r in pair if len(r) == 2})
    print(f"# {meta['mode'].title()} balance results\n")
    print(f"{count} completed trials on {meta['arena']} terrain, with a {meta['ticks']}-tick limit.")
    rosters = sorted({r for k in pairs for r in k})
    if len(rosters) <= 2:
        print("\n| Roster | Opponent | Wins | Draws | Losses | Mean surviving matter margin |")
        row("---", "---", "---:", "---:", "---:", "---:")
        for (a, b), d in sorted(pairs.items()):
            row(" + ".join(a), " + ".join(b), d.wins, d.draws, d.losses,
                f"{d.survivor_margin / d.count:+.1f}")
        if len(rosters) == 1 and rosters:
            print("\nSelf-play counts each seat’s outcome.")
        return
    print("\n## Pure matchups\n")
    print("Read each cell from the row unit’s perspective as wins/draws/losses. "
          "A dash indicates a matchup without a completed trial.\n")
    row("Unit", *(p[0] for p in pure))
    row(*(["---"] * (len(pure) + 1)))
    for unit in pure:
        cells = []
        for opp in pure:
            d = pairs.get((unit, opp))
            if unit == opp:
                cells.append("self" if d else "—")
            else:
                cells.append(f"{d.wins}/{d.draws}/{d.losses}" if d else "—")
        row(unit[0], *cells)

    if len(pure) > 1:
        print("\n## Pure unit efficiency\n")
        print("Against other pure units, retention is total health-weighted surviving unit "
              "matter divided by total matter spent on units. Mean win time counts base kills only.\n")
        row("Unit", "W/D/L", "Retained", "Mean ticks to win")
        row("---", "---:", "---:", "---:")
        for unit in pure:
            panel = [pairs[unit, p] for p in pure if unit != p and (unit, p) in pairs]
            wins, draws, losses = (sum(getattr(d, field) for d in panel)
                                   for field in ("wins", "draws", "losses"))
            spend = sum(d.spent for d in panel)
            retained = sum(d.remaining for d in panel) / spend if spend else None
            row(unit[0], f"{wins}/{draws}/{losses}",
                f"{retained:.1%}" if retained is not None else "—",
                f"{sum(d.winning_ticks for d in panel) / wins:.1f}" if wins else "—")

    if not mixes:
        return
    print("\n## Mixed queues against pure opponents\n")
    print("Score gives one point for a win and half for a draw. Each opponent gets equal weight. "
          "Lift is the mix’s score minus the mean score of its two pure components against the "
          "same opponents, in percentage points. Margin is the mix’s mean health-weighted "
          "surviving unit matter minus the opponent’s, in matter. Only opponents with data for "
          "all three comparisons count.\n")
    row("Mix", "Opponents", "Score", "Lift (pp)", "Margin")
    row("---", "---:", "---:", "---:", "---:")
    results = []
    for mix in mixes:
        a, b = (mix[0],), (mix[1],)
        common = [p for p in pure if all((r, p) in pairs for r in (a, b, mix))]
        if not common:
            continue
        score = mean(pairs[mix, p].score for p in common)
        baseline = mean((pairs[a, p].score + pairs[b, p].score) / 2 for p in common)
        margin = mean(pairs[mix, p].survivor_margin / pairs[mix, p].count for p in common)
        results.append((score - baseline, score, mix, len(common), margin))
    for lift, score, mix, opponents, margin in sorted(results, reverse=True):
        row(" + ".join(mix), opponents, f"{score:.1%}", f"{100 * lift:+.1f}", f"{margin:+.1f}")


def compare(baseline, current):
    old_meta, old, _, old_seats = baseline
    meta, new, _, seats = current
    if old_meta["mode"] != meta["mode"]:
        raise ValueError("Choose baseline and candidate runs in the same mode")
    if seats != old_seats:
        raise ValueError("Baseline and candidate must have the same roster and seat matchups")
    print("\n## Change from baseline\n")
    for key in ("ticks", "income_per_tick", "army_budget", "arena", "sim_rules_version",
                "effective_content_hash"):
        if meta.get(key) != old_meta.get(key):
            print(f"{key}: {old_meta.get(key)} → {meta.get(key)}.\n")
    print("Each roster is scored against all other tested rosters, with equal weight per "
          "opponent. Margin change is in health-weighted surviving matter.\n")
    row("Roster", "Opponents", "Score change (pp)", "Margin change")
    row("---", "---:", "---:", "---:")
    changes = []
    for roster in {r for r, _ in new}:
        panel = [(r, p) for r, p in new if r == roster and r != p]
        if not panel:
            continue
        delta = mean(new[k].score - old[k].score for k in panel)
        margin = mean(new[k].survivor_margin / new[k].count
                      - old[k].survivor_margin / old[k].count for k in panel)
        changes.append((delta, margin, roster, len(panel)))
    for delta, margin, roster, n in sorted(changes, reverse=True):
        row(" + ".join(roster), n, f"{100 * delta:+.1f}", f"{margin:+.1f}")


def write_csv(path, pairs):
    with path.open("w", newline="") as out:
        w = csv.writer(out)
        w.writerow(["roster", "opponent", "seat_observations", "wins", "draws", "losses",
                    "score", "mean_health_weighted_survivor_margin"])
        for (a, b), d in sorted(pairs.items()):
            w.writerow(["+".join(a), "+".join(b), d.count, d.wins, d.draws, d.losses,
                        f"{d.score:.6f}", f"{d.survivor_margin / d.count:.6f}"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("--csv", type=Path, help="write all directed matchups, including mixes")
    parser.add_argument("--baseline", type=Path, help="compare the same roster and seat matchups from another run")
    args = parser.parse_args()
    try:
        current = read_results(args.results)
        metadata, matchups, completed, _ = current
        report(metadata, matchups, completed)
        if args.baseline:
            compare(read_results(args.baseline), current)
        if args.csv:
            write_csv(args.csv, matchups)
    except (OSError, ValueError, KeyError, StopIteration) as error:
        sys.exit(f"Cannot summarize {args.results}: {error}")
