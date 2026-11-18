# Combat balance lab

The lab answers a narrow balance question. If two players send everything down the same lane and spend the same budget, which production queue can destroy the other base, and what survives? Both sides use ordinary engine movement, vision, targeting and damage. The controllers always order units to attack-move toward the opposing base. This makes the experiment repeatable and allows a queue that acquires targets for long-range units, screens a more fragile unit, or simply arrives sooner to differ from either component alone.

## Running an experiment

Build the evaluator once. These Linux commands limit the build to two cores and run the simulations on one low-priority core. Choose CPU IDs available on your host.

```sh
CARGO_BUILD_JOBS=2 taskset -c 0,1 nice -n 12 cargo build --release --locked -p atemporal-sim --example balance_sweep
mkdir -p artifacts/balance
lab=target/release/examples/balance_sweep
taskset -c 0 nice -n 15 "$lab" factory --income 3 --ticks 900 --output artifacts/balance/factory.jsonl
python3 scripts/summarize-balance.py artifacts/balance/factory.jsonl --csv artifacts/balance/factory.csv
```

The default sweep excludes miners and includes the other mobile factory recipes, every two-type mix among them, and matches between mixed queues. With shipped content there are six pure queues, 15 two-type queues and 441 seat assignments per arena and economy. Each distinct matchup runs in both seats. Each self-match needs one run because both seats already use the same queue. The default open map is 31×15 with no ore. Factory income starts at tick zero from an empty bank. A funded unit can finish in one simulation tick. The engine still has one output tile and can deliver at most one unit per factory per tick. At the baseline income, even a single grunt takes five ticks to fund.

Each finished trial is flushed to the JSONL file. Restart the same command to resume. The header pins the effective game content, engine rules version, experiment settings and expected trial count. Use a new file for a changed configuration. For a specific matchup or a different map, use a separate output file.

```sh
taskset -c 0 nice -n 15 "$lab" army --budget 600 --a grunt+tank --b artillery --arena choke --output artifacts/balance/army.jsonl
```

For a tuning experiment, copy `config/content.yaml`, edit the unit statistics, then pass the copy with `--content PATH`. Run the baseline and candidate with the same arena, seats, income or budget, tick limit and roster selection. Use `python3 scripts/summarize-balance.py candidate.jsonl --baseline baseline.jsonl` to see changes in score and surviving value per roster. A mirrored pair measures sensitivity to seating and deterministic tie breaking. It does not give two independent estimates of a population win rate.

## Reading results

A base kill decides the winner. A timeout can leave two very different surviving forces. Each trial therefore records the reason it ended, elapsed ticks, spending, base HP and survivors by unit type. A tank costing 100 matter at 25% health contributes 25 surviving matter. Base health is reported separately. For army mode, unused budget from indivisible unit costs stays separate from surviving unit value.

The Markdown report breaks out pure-unit fights and compares each mix with the mean of its two component queues against the same pure opponents. The CSV includes every observed roster against every observed opponent, including mixed opponents and mirrors. Each row is oriented toward the named roster. A win counts as one point, a draw as half. A positive lift means mixing helped against that panel. A queue with high lift can still lose against most opponents.

The equal allocation applies over a complete queue cycle. A 15-matter grunt and a 100-matter tank require 20 grunts and three tanks per cycle to allocate 300 matter to each type. An early base kill can interrupt a cycle, so actual counts and spending at the end of a duel can differ.

## Reference runs

The historical open-map reference sweeps included miners and used the player-timeline branch’s rules 8 at commit `0ef5125` with a 900-tick limit. The integrated dev simulator uses rules 9, which also includes alternating tread movement. The corridor calibration below includes fresh rules 9 baselines. Each income setting produced 784 trials across the 28 rosters, including 759 base kills and 25 timeouts. The compressed JSONL includes every trial and can be passed directly to the summarizer. [3 matter/tick data](balance-data/open-factory-income3-rules8.jsonl.gz) · [6 matter/tick data](balance-data/open-factory-income6-rules8.jsonl.gz)

| Matchup | At 3 matter/tick | At 6 matter/tick |
| --- | --- | --- |
| Tank vs. grunt | Tank won at ticks 426 and 429 | Grunt won at ticks 218 and 232 |
| Artillery + tank vs. tank | Mix won at ticks 596 and 649 | Mix won at ticks 351 and 378 |
| Grunt vs. artillery + tank | Grunt won twice at tick 107 | Grunt won at ticks 111 and 114 |

At 3 matter/tick, tanks won all 12 fights against other pure queues and retained 80.1% of the total matter they spent in those fights, health-adjusted. Grunts won 10 of 12. Their wins averaged 108 ticks, versus 265 for tanks. In both artillery–tank wins against pure tanks, the mix kept its base at full health and finished with 460 or 520 health-weighted matter. In both losses to grunts it had zero surviving unit matter. These results illustrate the trade between a strong established force and how quickly the first defenders arrive.

Doubling income reversed both grunt–tank fights and both artillery–tank pure-unit fights. Grunts then swept all six other pure opponents in both seats. A per-unit ranking on this map depends on the production economy. Utility-only queues expose another limit of the exercise: the attack-move controller gives miners and constructors no economic work.

## Local review page

A single static page reads the result files in its directory. To recreate the geometry review and its historical comparison on localhost, run these commands from the repository root, then open [the review on port 8846](http://127.0.0.1:8846/).

```sh
mkdir -p artifacts/balance/review
cp scripts/balance-review.html artifacts/balance/review/index.html
gzip -dc docs/balance-data/open-factory-income3-rules8.jsonl.gz > artifacts/balance/review/factory.jsonl
gzip -dc docs/balance-data/open-factory-income6-rules8.jsonl.gz > artifacts/balance/review/factory6.jsonl
for path in docs/balance-data/corridors-rules9/*.jsonl.gz; do
  name=$(basename "$path" .gz)
  gzip -dc "$path" > "artifacts/balance/review/$name"
done
python3 -m http.server 8846 --bind 127.0.0.1 --directory artifacts/balance/review
```

The review was checked in Chromium at desktop and phone widths. Selecting a matrix cell opens its seat results, selecting a mix opens its matchup, and switching income preserves the selected rosters. The table scrolls horizontally on a phone. Screenshots and the interaction trace from the local verification are in `artifacts/balance/`.

## Calibrating corridor width

Run `cargo run --release --locked -p atemporal-sim --example corridor_widths` to measure the shipped generator on 32 default two-player 48×48 maps (seeds 0–31). For each tile the tool finds the largest axis-aligned square of walkable terrain containing it. This gives a consistent width even when a route hugs one wall, though diagonal passages can measure narrower. The sample skips spawn and center clearings and treats widths of eight tiles or more as rooms.

| Measurement outside rooms | Samples | Width ≤3 tiles | Median width |
| --- | ---: | ---: | ---: |
| Tiles on any shortest orthogonal base-to-base route | 7,648 | 17.8% | 5 |
| Visits on the generator’s sampled traffic routes | 132,989 | 28.8% | 5 |

Three tiles represents a narrow passage that occurs regularly in generated play. The traffic estimate uses sampled routes from the map generator. It measures terrain geometry independently of where players chose to send units in actual matches.

### What changed in the winding corridor

The snake has two hairpin turns, three traverses across the center, and one three-tile-wide passage between its bases. Closing any traverse disconnects them. Its straight control uses the same corridor width, identical starting formations in 6×7 staging rooms, and the same 66-step shortest cardinal path. The open and single-choke maps are shorter, at 26 steps. Cardinal distance controls for tread travel distance; eight-direction units can take different lines through turns. Bends also change sight lines, traffic and the space between opposing formations.

The rules 9 experiment ran **5,292 duels**: 441 seat assignments × four geometries × three economies. The 21 rosters are six pure queues and all 15 two-unit mixes. Miners were excluded; constructors were included. The endpoint is a destroyed base or a 900-tick timeout. Each table entry counts trials that reached the time limit with both bases standing, out of 441 in that cell. [All trial records](balance-data/corridors-rules9/)

| Economy | Open | Single choke | Straight corridor | Snake |
| --- | ---: | ---: | ---: | ---: |
| 600-matter starting army | 12 | 11 | 4 | 3 |
| Factory, 3 matter/tick | 14 | 14 | 39 | 52 |
| Factory, 6 matter/tick | 3 | 18 | 96 | 129 |

- **Artillery vs. grinder** shows a winner reversal caused by the corridor layout. At 3 matter/tick, grinders won both straight duels at tick 620. Artillery won both snake duels, at ticks 808 and 825. Both comparisons finished within the same limit.
- **Grunts** won every 600-matter army duel against the other armed rosters on both long maps (28/28 per map). Factory results depended on geometry and income. At 6 matter/tick they beat pure tanks on open ground in both seats. On both long corridors all four matchups reached tick 900. Extending the limit to 1,800 let tanks win all four, at ticks 1,014 and 1,022 in the snake and 1,178 and 1,477 in the straight.
- **Artillery + tank vs. tank** initially timed out in both snake duels at both incomes. With 1,800 ticks the mixed queue won all four. The 900-tick counts describe a time limit, and a longer firefight can still have a decisive winner.
- Comparing the snake directly to the length-matched straight, among the 420 seat assignments with distinct opposing rosters, the army result changed in 35 cases (31 outright winner reversals). Factory results changed in 51 cases at income 3 (18 outright reversals) and 49 at income 6 (4 outright reversals). The remaining changes involved a timeout on one geometry. These are paired deterministic matchups on fixed layouts.

For the long-horizon checks, both seats were run for each combination of straight or snake, income 3 or 6, and grunt, grinder or artillery+tank versus tank. One invocation follows.

```sh
taskset -c 0 nice -n 15 "$lab" factory --arena snake --income 6 --ticks 1800 --a grunt --b tank
```

## Artillery retreat review

In rules 10, artillery reacts to spotted enemy units strictly inside its gun range minus one tile. For the default gun, an enemy at exactly nine tiles does not trigger retreat. A ready gun fires first. Between shots, it can retreat on its normal movement cooldown and keeps its underlying order.

The planner scores candidate tiles using the sum of distance-based pressure from spotted enemy units and a penalty for tight exits and short dead ends. It searches up to six orthogonal steps for a better endpoint and gives shorter routes preference. That allows a sideways first step toward open ground. Allies, satellites and recon can supply the sighting that triggers retreat.

To review the decisions, run these commands from the repository root. The exporter advances the real simulator with diagnostics enabled. It writes the map, entities, pre-movement plans and resolved events for selected ticks.

```sh
mkdir -p artifacts/retreat-review
CARGO_BUILD_JOBS=2 taskset -c 0,1 nice -n 12 cargo run --release --locked -p atemporal-sim --example artillery_retreat_review -- artifacts/retreat-review/frames.json
cp scripts/retreat-review.html artifacts/retreat-review/index.html
python3 -m http.server 8847 --bind 127.0.0.1 --directory artifacts/retreat-review
```

The review shows snapshot S[t] before the tick is resolved. A solid arrow is a proposed immediate cardinal step and the endpoint marker is the lookahead destination. The event record reports the move that actually happened. A ready shot takes its tick; a movement cooldown or another unit winning an occupied tile can keep the gun in place. Replanning on the next eligible tick lets the destination change as enemies move.

The [saved rules-10 review frames](balance-data/artillery-retreat-rules10.json.gz) can also be decompressed directly to `artifacts/retreat-review/frames.json`. The 18 frames cover five staged engagements. The full-map case uses generated terrain and an authored force placement. In the dead-end case, compare ticks 1 and 7 for the sideways step and entry into the room. In the generated case, tick 73 shows two guns retreating on the same tick.

A paired smoke test ran artillery against grunts, grinders and tanks, in both seats on open and snake maps, with a 600-matter army budget or 3-matter factory income and a 900-tick cap. Open factory artillery flipped from two losses against tanks to two wins. All six snake factory games hit the cap with retreat enabled, up from two. Longer trials would be needed to determine their eventual winners. The change also costs simulation time. In three single-core runs of the 2,000-entity, 150-tick mixed battle benchmark, median wall time rose from 387 ms to 542 ms.
