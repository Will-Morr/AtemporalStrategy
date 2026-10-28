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

## Re-run the two reported openings

An opt-in Rust scenario preserves a saved human opening, map and pinned content, then simulates the revised bot opening for 300 ticks. It also compares round 3's intervention time on the original published revision 2. This regression review currently expects the reported archive layout: two players, human in slot 0, bot in slot 1, checkpoints every 100 ticks, and at least three accepted rounds. It is ignored in ordinary tests because private archives are not repository fixtures.

```sh
ATEMPORAL_BOT_ARCHIVE="$PWD/games/game-ID/replays/match-ID" \
ATEMPORAL_BOT_REVIEW_OUT="$PWD/target/opening-review.json" \
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 \
nice -n 10 taskset -c 0,1 cargo test -p atemporal-server archived_human_opening_review --locked -- --ignored
```

The current opening check requires at least three miners mining at tick 100, five by tick 200, and all surviving miners on mining orders while military follow-up is funded. The report includes the replacement bot commands, working-miner counts and resource counters at ticks 100/200/300, the bounded-run outcome, and old/new intervention ticks. Opponent commands enter only the simulator, never the planner. This is an opening regression and timing check, not a claim that a changed bot wins the original full game. Neither original archive nor running worker is modified.

## Full recorded-opponent match

The repository fixture `fixtures/bots/sustained-pressure.json` contains the latest reported match's initial world, pinned content/rules and six accepted human turns, with profiles and session metadata omitted. Run the full bot regression without needing a local game archive:

```sh
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 \
ATEMPORAL_BOT_MATCH_REPORT="$PWD/target/sustained-pressure-report.json" \
nice -n 10 taskset -c 0,1 cargo test -p atemporal-server --release --lib recorded_sustained_pressure_match --locked -- --nocapture
```

The bot plans against the previous published revision using ordinary visibility, knowledge reconstruction, intervention selection and command validation. Only afterward does the harness add that round's recorded human turn. It preserves human command IDs, ticks and future-order policies and reruns the real simulation from the original world. Scoring uses the normal scoreboard reducer and the fixture's unchanged five-point target. After all six recorded turns, the opponent passes; the scenario must produce a bot match win before a human match win, within 24 rounds. Reports distinguish timeline outcomes from the accumulating match score and include bot commands, spending, sampled unit peaks, final unit positions/orders and authoritative skipped commands. This tests a fixed opponent sequence; it does not predict how a human would adapt to the revised bot.

For a verbose action-by-action single-order diagnostic, set `ATEMPORAL_BOT_SINGLE_TRACE=1` when running the `single_order_builds_a_working_economy_and_army` test with `--nocapture`.
