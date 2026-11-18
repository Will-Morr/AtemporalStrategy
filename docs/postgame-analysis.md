# Postgame analysis

Run the read-only Python 3 tool from the repository root. It requires no packages and never reads cookies, slot tokens, or request-token files.

```sh
# Most recently created two game directories; all completed revisions, both players.
python3 scripts/analyze-game.py --out target/postgame/latest.json
# One game, with event counts and only player 1's actions and economy.
python3 scripts/analyze-game.py games/game-ID --player 1 --events --out target/postgame/bot.json
# One match/revision with full unit and production details at the nearest saved checkpoint.
python3 scripts/analyze-game.py games/game-ID/replays/match-ID --revision 2 --player 1 --tick 200
python3 -B scripts/test-analyze-game.py
```

A game path includes all its archived matches; a match path selects exactly one. `--latest N` selects game directories by their creation timestamp in the ID, not by filesystem modification time. This avoids mistaking replay reads for newly played games. Use an explicit path when analyzing a rematch in an older game directory.

The JSON report contains archive paths and build fingerprints, each revision's authoritative outcome and score, that round's accepted commands, authoritative skipped-command results, sampled unit peaks and first appearances, final banks and spending/damage/loss counters, orders by unit type, non-mining miner peaks, empty factory counts and queued production. Pending drafts are separate from accepted turns. Optional event counts cover the whole timeline and all players, even with `--player`; they are labelled accordingly.

Checkpoints and optional events are streamed one record at a time. The tool avoids the much larger `samples.json` files. A missing completion marker is reported as an incomplete revision; malformed/truncated selected data produces an error instead of a success report. Live archives can change while being read; rerun after publication if a worker is writing. Use `--out` to keep full reports out of agent context and extract the relevant fields afterward.

## Interpreting evidence

Each revision is a replacement timeline. Never add its resource counters or kills to those of another revision. The round's accepted commands are additions; earlier rounds' commands may still execute in this timeline. A skipped command result is authoritative for that revision, including earlier turns whose targets disappeared during a rewrite.

Peak units and first appearances are observations at checkpoint spacing, not exact births/deaths. `--tick` explicitly includes both the requested tick and actual nearest checkpoint tick; ties choose the earlier checkpoint. It does not reconstruct an exact state or extrapolate past the terminal tick. Inspect the game's exact-state replay or simulation tools when exact timing matters.

A miner with a non-mining order or an empty factory is a diagnostic clue, not necessarily a bug. Check the accepted orders, bank, queued production, local threats and timeline outcome before assigning a cause. Reports are omniscient offline diagnostics; do not feed hidden opponent state to bot planning.

## Recorded opponent

The [six-turn pressure fixture](../fixtures/bots/sustained-pressure.json) preserves the human orders and opening world from the earlier bot experiment. The current replay evaluator accepts this file directly. See the [reproduction commands](balance-analysis.md#reproduction) to run a fresh Standard or Tactical opponent against it.
