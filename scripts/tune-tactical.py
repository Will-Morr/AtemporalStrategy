#!/usr/bin/env python3
"""Run a bounded, serial Tactical coordinate search against a frozen incumbent."""

import argparse
import hashlib
import json
from pathlib import Path
import resource
import subprocess


INCUMBENT = {"opening_grunts": 8, "fresh_entry_ticks": 600, "rally_distance_band": 8}


def cpu_seconds():
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return usage.ru_utime + usage.ru_stime


def evaluate(binary, cap, profile, seeds, stage, emit):
    matches = []
    for seed in seeds:
        for seat in (0, 1):
            before = cpu_seconds()
            result = subprocess.run(
                [
                    str(binary), f"seed:{seed}", "tactical", str(seat), "--self-play",
                    str(cap), json.dumps(profile), json.dumps(INCUMBENT),
                ],
                check=True,
                stdout=subprocess.PIPE,
                text=True,
            )
            match = json.loads(result.stdout)
            match["cpu_seconds"] = round(cpu_seconds() - before, 3)
            emit({"kind": "match", "stage": stage, "candidate": profile, **match})
            matches.append(match)
    return matches


def margin(matches):
    """Screen using scoreboard point margin, including partial capped matches."""
    return sum(
        m["score_by_seat"][m["candidate_seat"]] - m["score_by_seat"][1 - m["candidate_seat"]]
        for m in matches
    )


def seed_list(value):
    try:
        seeds = [int(seed) for seed in value.split(",")]
    except ValueError as error:
        raise argparse.ArgumentTypeError("use comma-separated seed integers") from error
    if len(seeds) != len(set(seeds)) or any(not 0 < seed < 2**53 for seed in seeds):
        raise argparse.ArgumentTypeError("seeds must be unique positive safe integers")
    return seeds


def round_cap(value):
    cap = int(value)
    if not 1 <= cap <= 1000:
        raise argparse.ArgumentTypeError("round cap must be 1..=1000")
    return cap


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/replay_opponent"))
    parser.add_argument("--training-seeds", type=seed_list, default=[42])
    parser.add_argument("--held-out-seeds", type=seed_list, default=[113, 907])
    parser.add_argument("--training-cap", type=round_cap, default=2)
    parser.add_argument("--held-out-cap", type=round_cap, default=6)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if set(args.training_seeds) & set(args.held_out_seeds):
        parser.error("training and held-out seeds must be disjoint")

    with args.output.open("x") as output:
        def emit(record):
            line = json.dumps(record, sort_keys=True)
            print(line, file=output, flush=True)
            print(line, flush=True)

        binary = args.binary.resolve()
        emit({
            "kind": "experiment",
            "format_version": 1,
            "binary_sha256": hashlib.file_digest(binary.open("rb"), "sha256").hexdigest(),
            "incumbent": INCUMBENT,
            "training_seeds": args.training_seeds,
            "held_out_seeds": args.held_out_seeds,
            "training_cap": args.training_cap,
            "held_out_cap": args.held_out_cap,
        })
        center = INCUMBENT
        best_margin = 0  # Swapping identical policies cancels the aggregate seat margin.
        screened = {center["opening_grunts"]}
        challengers = []
        for iteration, step in enumerate((6, 2), start=1):
            start = center
            for count in (start["opening_grunts"] - step, start["opening_grunts"] + step):
                if not 0 <= count <= 16 or count in screened:
                    continue
                screened.add(count)
                candidate = {**start, "opening_grunts": count}
                matches = evaluate(
                    binary, args.training_cap, candidate, args.training_seeds,
                    f"training-{iteration}", emit,
                )
                score = margin(matches)
                challengers.append((score, candidate))
                emit({
                    "kind": "screening", "iteration": iteration, "candidate": candidate,
                    "scoreboard_margin": score,
                    "round_capped": sum(m["termination"] == "round_cap" for m in matches),
                })
                if score > best_margin:
                    center, best_margin = candidate, score
            emit({"kind": "update", "iteration": iteration, "center": center, "scoreboard_margin": best_margin})

        # Assess a changed candidate even if the screen failed to beat zero. Held-out results
        # never feed the search or change which candidate receives this check.
        score, finalist = max(challengers, key=lambda trial: trial[0])
        held_out = evaluate(binary, args.held_out_cap, finalist, args.held_out_seeds, "held-out", emit)
        wins = sum(
            {"kind": "player", "player_id": m["candidate_seat"]} in m["match_winners"]
            for m in held_out
        )
        completed = sum(m["termination"] == "scoreboard" for m in held_out)
        emit({
            "kind": "conclusion", "finalist": finalist, "training_margin": score,
            "held_out_wins": wins, "held_out_losses": completed - wins,
            "held_out_capped": len(held_out) - completed,
            "held_out_scoreboard_margin": margin(held_out),
            "deployed_profile": INCUMBENT,
        })


if __name__ == "__main__":
    main()
