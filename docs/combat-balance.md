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

The full sweep includes every mobile factory recipe in the selected content, every two-type mix, and matches between mixed queues. Each distinct matchup runs in both seats. Each self-match needs one run because both seats already use the same queue. The default open map is 31×15 with no ore. Factory income starts at tick zero from an empty bank. A funded unit can finish in one simulation tick. The engine still has one output tile and can deliver at most one unit per factory per tick. At the baseline income, even a single grunt takes five ticks to fund.

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

The open-map reference sweeps used the player-timeline branch’s rules 8 at commit `0ef5125` with a 900-tick limit. The integrated dev simulator uses rules 9, which also includes alternating tread movement. Generate a fresh rules 9 baseline before comparing unit-stat edits on dev. Each income setting produced 784 trials across the 28 rosters, including 759 base kills and 25 timeouts. The compressed JSONL includes every trial and can be passed directly to the summarizer. [3 matter/tick data](balance-data/open-factory-income3-rules8.jsonl.gz) · [6 matter/tick data](balance-data/open-factory-income6-rules8.jsonl.gz)

| Matchup | At 3 matter/tick | At 6 matter/tick |
| --- | --- | --- |
| Tank vs. grunt | Tank won at ticks 426 and 429 | Grunt won at ticks 218 and 232 |
| Artillery + tank vs. tank | Mix won at ticks 596 and 649 | Mix won at ticks 351 and 378 |
| Grunt vs. artillery + tank | Grunt won twice at tick 107 | Grunt won at ticks 111 and 114 |

At 3 matter/tick, tanks won all 12 fights against other pure queues and retained 80.1% of the total matter they spent in those fights, health-adjusted. Grunts won 10 of 12. Their wins averaged 108 ticks, versus 265 for tanks. In both artillery–tank wins against pure tanks, the mix kept its base at full health and finished with 460 or 520 health-weighted matter. In both losses to grunts it had zero surviving unit matter. These results illustrate the trade between a strong established force and how quickly the first defenders arrive.

Doubling income reversed both grunt–tank fights and both artillery–tank pure-unit fights. Grunts then swept all six other pure opponents in both seats. A per-unit ranking on this map depends on the production economy. Utility-only queues expose another limit of the exercise: the attack-move controller gives miners and constructors no economic work.

## Local review page

A single static page reads the two result files in its directory. To recreate the reference review on localhost, run these commands from the repository root, then open [the review on port 8846](http://127.0.0.1:8846/).

```sh
mkdir -p artifacts/balance/review
cp scripts/balance-review.html artifacts/balance/review/index.html
gzip -dc docs/balance-data/open-factory-income3-rules8.jsonl.gz > artifacts/balance/review/factory.jsonl
gzip -dc docs/balance-data/open-factory-income6-rules8.jsonl.gz > artifacts/balance/review/factory6.jsonl
python3 -m http.server 8846 --bind 127.0.0.1 --directory artifacts/balance/review
```

The review was checked in Chromium at desktop and phone widths. Selecting a matrix cell opens its seat results, selecting a mix opens its matchup, and switching income preserves the selected rosters. The table scrolls horizontally on a phone. Screenshots and the interaction trace from the local verification are in `artifacts/balance/`.
