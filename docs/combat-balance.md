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

The sweep uses artillery, grinders, grunts, scouts and tanks, with five pure queues, ten two-type queues and 225 seat assignments per arena and economy. The miner and constructor are excluded from every matchup, including mixed queues. Each distinct matchup runs in both seats. Each self-match needs one run because both seats already use the same queue. The default open map is 31×15 with no ore. Factory income starts at tick zero from an empty bank. A funded unit can finish in one simulation tick. The engine still has one output tile and can deliver at most one unit per factory per tick. At the baseline income, even a single grunt takes five ticks to fund.

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

The historical open-map reference sweeps used the player-timeline branch’s rules 8 at commit `0ef5125` with a 900-tick limit. The archives retain the 225 trials between combat rosters at each income. At 3 matter/tick there were 215 base kills and 10 timeouts. At 6 matter/tick there were 223 base kills and two timeouts. The compressed JSONL can be passed directly to the summarizer. [3 matter/tick data](balance-data/open-factory-income3-rules8.jsonl.gz) · [6 matter/tick data](balance-data/open-factory-income6-rules8.jsonl.gz)

| Matchup | At 3 matter/tick | At 6 matter/tick |
| --- | --- | --- |
| Tank vs. grunt | Tank won at ticks 426 and 429 | Grunt won at ticks 218 and 232 |
| Artillery + tank vs. tank | Mix won at ticks 596 and 649 | Mix won at ticks 351 and 378 |
| Grunt vs. artillery + tank | Grunt won twice at tick 107 | Grunt won at ticks 111 and 114 |

At 3 matter/tick, tanks won all eight fights against other pure queues and retained 72.3% of the total matter they spent in those fights, health-adjusted. Grunts won six of eight. Their wins averaged 111 ticks, versus 287 for tanks. In both artillery–tank wins against pure tanks, the mix kept its base at full health and finished with 460 or 520 health-weighted matter. In both losses to grunts it had zero surviving unit matter. These results illustrate the trade between a strong established force and how quickly the first defenders arrive.

Doubling income reversed both grunt–tank fights and both artillery–tank pure-unit fights. Grunts then swept all four other pure opponents in both seats. The production economy can change the unit rankings even on the same map.

## Local review page

The static reviewer reads measured trial files in its directory. To reproduce the current release and the historical comparisons, run these commands from the repository root, then open [the review on port 8846](http://127.0.0.1:8846/).

```sh
review=artifacts/balance/review
mkdir -p "$review"
cp scripts/balance-review.html "$review/index.html"
gzip -dc docs/balance-data/open-factory-income3-rules8.jsonl.gz > "$review/factory.jsonl"
gzip -dc docs/balance-data/open-factory-income6-rules8.jsonl.gz > "$review/factory6.jsonl"
for path in docs/balance-data/corridors-rules9/*.jsonl.gz; do
  name=$(basename "$path" .gz)
  gzip -dc "$path" > "$review/$name"
done

# Prepare data, downloadable PNGs and a combined PDF (requires Matplotlib).
prepare_review() {
  source="docs/balance-data/$1"
  destination="$review/$2"
  mkdir -p "$destination"
  for path in "$source"/*.jsonl.gz; do
    name=$(basename "$path" .gz)
    gzip -dc "$path" > "$destination/$name"
  done
  python3 scripts/summarize-balance.py "$source" --heatmap "$destination/survival-charts.pdf"
}
prepare_review survival-rules12 rules12
prepare_review grunt-balance-patch grunt-patch
prepare_review artillery-tank-mobility artillery-tank-mobility
prepare_review artillery-factory-patch artillery-factory-patch
python3 -m http.server 8846 --bind 127.0.0.1 --directory "$review"
```

The review was checked in Chromium at desktop and phone widths. Selecting a matrix cell opens its seat results, selecting a mix opens its matchup, and switching income preserves the selected rosters. The survival chart fits the available width. On narrow screens, it uses abbreviated roster labels and color-only cells. Select a cell to read its exact value and both roster names. The fit check covered viewport widths from 320 to 1,920 pixels. Screenshots and browser traces are in `artifacts/ui/combat-chart-fit/`.

For the artillery factory patch, the reviewer opens the winding-corridor factory chart at 6 matter/tick. Live Chromium checks loaded all four arenas and three economies, opened both seat results, preserved the selected rosters across income changes and opened all five historical datasets. The full chart fit at 390px; desktop and mobile screenshots, the trace and inspected PNG/PDF exports are in `artifacts/ui/artillery-factory-final/`. Exact zero is solid black across the reviewer and both export formats.

### Surviving matter by matchup

The rules-12 chart uses all 15 combat rosters (five singles and ten pairs) for each arena and economy. Rows are the measured force and columns its opponent. Count only surviving units, multiplying each unit’s cost by its remaining health fraction. A wipeout contributes zero even if the base survives. A cell averages the two starting seats, including both forces in the single mirror duel on the diagonal. Each opponent has equal weight in the roster average used to sort both axes. Colors run from zero to the highest cell on the selected chart.

Each of the [twelve datasets](balance-data/survival-rules12/) contains 225 trials. Survivors are measured at base destruction or tick 1,800, whichever comes first. Both sides can retain troops at the cutoff. Factory forces accumulate spending while a duel lasts, so a long fight can produce a larger surviving army. The army setting uses a fixed 600-matter starting budget.

To export an individual chart with Matplotlib installed, use:

```sh
python3 scripts/summarize-balance.py docs/balance-data/survival-rules12/army-snake.jsonl.gz --heatmap army-snake.png
```

### Grunt damage patch

Before the patch, pure grunts won all 28 non-mirror trials on each army map and in the 6-matter/tick factories on open and choke. The patch reduces grunt damage per shot from 8 to 3. Grunts still cost 15 matter and deploy 40 units with the 600-matter army budget.

The [patch archive](balance-data/grunt-balance-patch/) contains 2,700 trials across the same four maps and three economies as the [baseline](balance-data/survival-rules12/). The reviewer shows both sets. The highest win count for any roster was 27 of 28 in armies and 26 of 28 in factories. Artillery and some other rosters completed individual corridor factory panels without a loss because some opponents held to the 1,800-tick cutoff.

| Arena | Army | Factory at 3 matter/tick | Factory at 6 matter/tick |
| --- | ---: | ---: | ---: |
| Open | 28 → 26 | 26 → 24 | 28 → 21 |
| Choke | 28 → 14 | 26 → 23 | 28 → 8 |
| Straight | 28 → 14 | 20 → 8 | 16 → 0 |
| Snake | 28 → 17 | 20 → 8 | 19 → 1 |

The table shows pure grunt wins out of 28 non-mirror trials before and after the patch. Pure tanks beat pure grunts from both seats on all four army maps. At 6 matter/tick, 18 straight-corridor pure grunt games and 17 snake-corridor pure grunt games ended at the cutoff with both bases standing.

### Artillery and tank mobility

The tank movement cooldown increases from four to six ticks, matching artillery. Grunts and grinders still take one step every two ticks. The [mobility patch archive](balance-data/artillery-tank-mobility/) repeats all 2,700 trials on the four arenas. The strongest roster in any panel won 26 of its 28 non-mirror trials.

| Pure matchup | Army wins out of 8 | Factory wins out of 16 |
| --- | ---: | ---: |
| Artillery over tank | 3 → 8 | 14 → 14 |
| Grunt over artillery | 8 → 8 | 8 → 8 |
| Grinder over artillery | 8 → 8 | 4 → 4 |

The arrows compare the grunt damage patch with the mobility patch. Grunts beat artillery in both seats on open and choke at both factory incomes. Grinders beat artillery in both seats on open and choke at 3 matter/tick. Both straight and snake corridors still give factory artillery room to build up against those faster units.

Both 6-matter/tick snake artillery–tank duels reached the 1,800-tick matrix cutoff. Artillery held over eleven times as much health-weighted surviving unit matter in each seat. A separate run with a 3,600-tick cap ended in artillery wins at ticks 2,778 and 2,660, with full base health in both seats.

```sh
taskset -c 0 nice -n 15 "$lab" factory --arena snake --income 6 --ticks 3600 --a artillery --b tank
```

The [extended snake duel log](balance-data/artillery-tank-snake-3600.jsonl.gz) records both seats through base destruction.

### Factory artillery rework

Artillery costs 120 matter, up from 80. At an income of 6 matter per tick, a factory needs 20 ticks of income to pay for a gun, up from about 13. Grinders move every tick instead of every two ticks, and their cost rises from 100 to 110. They keep their 275 HP and melee weapon. In a 600-matter army the new prices buy five guns or five grinders. Testing the faster grinder at the old price produced 27 wins in 28 matchups on three army maps.

The [factory rework](balance-data/artillery-factory-patch/) and [mobility baseline](balance-data/artillery-tank-mobility/) each contain 2,700 mirrored trials. The factory comparison below uses four maps, two incomes and a 1,800-tick cutoff. Each matchup was played from both starting seats.

| Factory result | Mobility baseline | Factory rework |
| --- | ---: | ---: |
| Artillery roster vs artillery-free roster, wins / losses / cutoffs (800 games) | 538 / 80 / 182 | 321 / 351 / 128 |
| Pure artillery vs 14 other rosters, wins / losses / cutoffs (224 games) | 155 / 19 / 50 | 72 / 115 / 37 |
| Grunt wins against pure artillery (16 games) | 8 | 16 |
| Grinder wins against pure artillery (16 games) | 4 | 16 |
| Factory panels led by pure artillery in average surviving matter | 8 of 8 | 2 of 8 |

In 600-matter army duels, artillery beat tanks, tanks beat grinders, and both fast units beat artillery from both seats on all four maps. Every army roster had an opponent that beat it from both seats. The most wins achieved by any roster were 26 of 28 in an army panel and 24 of 28 in a factory panel.

The two high-income corridors still favor artillery in surviving matter. On the winding map at 6 matter/tick, 105 of 210 non-mirror games reached the cutoff, down from 126. Artillery destroyed the tank factory in 12 of 16 standard factory duels. Extending the four winding-map cutoffs produced four more artillery wins, at ticks 2,165 and 2,431 at income 3 and 3,871 and 3,847 at income 6. Income also changes how reinforcements arrive. At the intermediate income of 5 on the straight corridor, both grunt–artillery games reached tick 3,600 with both bases standing and artillery holding over 17,700 surviving matter in each seat. The [extended duel logs](balance-data/artillery-factory-checks/) record these outcomes.

### Artillery + grunt in factory combat

The factory rework’s comparison chart ranks artillery + grunt in the top three for average surviving matter in all eight factory settings. It leads that measure on open ground at income 6. Its strongest base-kill result is the straight corridor at income 3 (22 wins, six losses). Across 224 non-mirror factory trials it finished seventh of 15 rosters by base kills, with 106 wins, 58 losses and 60 cutoffs. Pure grinders beat it from both seats on all four maps at both incomes, and in all eight corresponding deployed-army trials.

On the winding corridor at income 6, pure grinders destroyed its factory at tick 210 from either seat. All 26 other trials reached tick 1,800 with both factories at full health. Artillery + grunt led in surviving matter in 22 of those 26. The [7,200-tick follow-ups](balance-data/artillery-grunt-checks/) tested three of those opponents from both seats. Artillery + grunt eventually destroyed both grinder + grunt factories (ticks 5,030 and 5,352). Against pure grunts and against artillery + tank, both bases still had full health at tick 7,200 in both seats. At that point artillery + grunt led pure grunts in surviving matter and trailed artillery + tank in both seats.

## Calibrating corridor width

Run `cargo run --release --locked -p atemporal-sim --example corridor_widths` to measure the shipped generator on 32 default two-player 48×48 maps (seeds 0–31). For each tile the tool finds the largest axis-aligned square of walkable terrain containing it. This gives a consistent width even when a route hugs one wall, though diagonal passages can measure narrower. The sample skips spawn and center clearings and treats widths of eight tiles or more as rooms.

| Measurement outside rooms | Samples | Width ≤3 tiles | Median width |
| --- | ---: | ---: | ---: |
| Tiles on any shortest orthogonal base-to-base route | 7,648 | 17.8% | 5 |
| Visits on the generator’s sampled traffic routes | 132,989 | 28.8% | 5 |

Three tiles represents a narrow passage that occurs regularly in generated play. The traffic estimate uses sampled routes from the map generator. It measures terrain geometry independently of where players chose to send units in actual matches.

### What changed in the winding corridor

The snake has two hairpin turns, three traverses across the center, and one three-tile-wide passage between its bases. Closing any traverse disconnects them. Its straight control uses the same corridor width, identical starting formations in 6×7 staging rooms, and the same 66-step shortest cardinal path. The open and single-choke maps are shorter, at 26 steps. Cardinal distance controls for tread travel distance; eight-direction units can take different lines through turns. Bends also change sight lines, traffic and the space between opposing formations.

The rules 9 corridor archives contain **2,700 combat duels** across four geometries and three economies, each with 225 seat assignments. Rules 9 includes alternating tread movement. The endpoint is a destroyed base or a 900-tick timeout. Each table entry counts trials that reached the time limit with both bases standing, out of 225 in that cell. [All trial records](balance-data/corridors-rules9/)

| Economy | Open | Single choke | Straight corridor | Snake |
| --- | ---: | ---: | ---: | ---: |
| 600-matter starting army | 3 | 2 | 0 | 1 |
| Factory, 3 matter/tick | 12 | 12 | 34 | 42 |
| Factory, 6 matter/tick | 2 | 8 | 50 | 67 |

- **Artillery vs. grinder** shows a winner reversal caused by the corridor layout. At 3 matter/tick, grinders won both straight duels at tick 620. Artillery won both snake duels, at ticks 808 and 825. Both comparisons finished within the same limit.
- **Grunts** won every 600-matter army duel against the other armed rosters on both long maps (28/28 per map). Factory results depended on geometry and income. At 6 matter/tick they beat pure tanks on open ground in both seats. On both long corridors all four matchups reached tick 900. Extending the limit to 1,800 let tanks win all four, at ticks 1,014 and 1,022 in the snake and 1,178 and 1,477 in the straight.
- **Artillery + tank vs. tank** initially timed out in both snake duels at both incomes. With 1,800 ticks the mixed queue won all four. The 900-tick counts describe a time limit, and a longer firefight can still have a decisive winner.
- Comparing the snake directly to the length-matched straight, among the 210 seat assignments with distinct opposing rosters, the army result changed in 16 cases (15 outright winner reversals). Factory results changed in 39 cases at income 3 (11 outright reversals) and 31 at income 6 (four outright reversals). The remaining changes involved a timeout on one geometry. These are paired deterministic matchups on fixed layouts.

For the long-horizon checks, both seats were run for each combination of straight or snake, income 3 or 6, and grunt, grinder or artillery+tank versus tank. One invocation follows.

```sh
taskset -c 0 nice -n 15 "$lab" factory --arena snake --income 6 --ticks 1800 --a grunt --b tank
```

## Artillery retreat review

Since rules 11, artillery reacts to spotted enemy units strictly inside its gun range minus two tiles. For the default gun, an enemy at exactly eight tiles does not trigger retreat. A ready gun fires first. Between shots, it can retreat on its normal movement cooldown and keeps its underlying order.

The planner scores candidate tiles using the sum of distance-based pressure from spotted enemy units and a penalty for tight exits and short dead ends. It searches up to six orthogonal steps for a better endpoint and gives shorter routes preference. That allows a sideways first step toward open ground. Allies, satellites and recon can supply the sighting that triggers retreat.

Rules 12 computes one corner field from the map's floor and rock before play. It measures how far apart the reachable floor tiles spread after 4, 8, 12 and 20 cardinal steps. A long through-corridor can spread in both directions. The same-width pocket loses that span as its end approaches. Three smoothing passes blend scores along connected floor tiles. A separate short-corridor check adds a sharp penalty where fewer than two exits branch or continue beyond the local lookahead, so smoothing preserves the entrance to a shallow blind pocket. A tightly folded U-shaped through-route can also score as constrained because its reachable tiles stay geographically close. Every artillery unit reads the same field. Units, buildings, reserved construction tiles and fog never change its values. Enemy sightings contribute to a separate threat field. Buildings and occupied enemy tiles can still block proposed retreat routes.

To review the decisions, run these commands from the repository root. The exporter advances the real simulator with diagnostics enabled. It writes the map, entities, pre-movement plans and resolved events for selected ticks.

```sh
mkdir -p artifacts/retreat-review
CARGO_BUILD_JOBS=2 taskset -c 0,1 nice -n 12 cargo run --release --locked -p atemporal-sim --example artillery_retreat_review -- artifacts/retreat-review/frames.json
cp scripts/retreat-review.html artifacts/retreat-review/index.html
python3 -m http.server 8847 --bind 127.0.0.1 --directory artifacts/retreat-review
```

The review shows snapshot S[t] before the tick is resolved. A solid arrow is a proposed immediate cardinal step and the endpoint marker is the lookahead destination. The event record reports the move that actually happened. A ready shot takes its tick; a movement cooldown or another unit winning an occupied tile can keep the gun in place. Replanning on the next eligible tick lets the destination change as enemies move.

The [saved rules-12 review frames](balance-data/artillery-retreat-rules12.json.gz) contain one static corner grid per map. On the generated map, compare ticks 0 and 73 to see how enemy threat changes over the same terrain field. In the dead-end fixture, the gun turns toward the wider room at tick 13 and enters it at tick 19.

The [saved rules-10 review frames](balance-data/artillery-retreat-rules10.json.gz) can also be decompressed directly to `artifacts/retreat-review/frames.json`. The 18 frames cover five staged engagements. The full-map case uses generated terrain and an authored force placement. In the dead-end case, compare ticks 1 and 7 for the sideways step and entry into the room. In the generated case, tick 73 shows two guns retreating on the same tick.

A paired smoke test ran artillery against grunts, grinders and tanks, in both seats on open and snake maps, with a 600-matter army budget or 3-matter factory income and a 900-tick cap. Open factory artillery flipped from two losses against tanks to two wins. All six snake factory games hit the cap with retreat enabled, up from two. With a longer horizon artillery destroyed all six opposing factories, winning against grinders at ticks 910 and 986, tanks at 1,831 and 1,919, and grunts at 5,752 and 5,776 (artillery seats 0 and 1 respectively). Both grunt factories still had full health at tick 5,400, while each side fielding artillery had at least 200 guns facing at most four grunts. Base health alone missed the growing attrition advantage. See the [checkpoint results](balance-data/artillery-corridor-progress-rules10.csv).

The change also costs simulation time. In three single-core runs of the 2,000-entity, 150-tick mixed battle benchmark, median wall time rose from 387 ms to 542 ms.

Rules 11 reduced the artillery retreat threshold from nine to eight tiles. In the same six snake factory duels (three matter per tick, both seats per opponent), count ticks on which at least two artillery fired an attack. Over the first 750 ticks of every game the total rose from 358 to 392. The grinder pair rose from 83 to 99, the grunt pair from 208 to 226 and the tank pair stayed at 67. All six opposing factories fell under both rules. Tank games finished 556 and 531 ticks sooner across the two seats; one grunt game ran 142 ticks longer. The [per-seat firing counts and win ticks](balance-data/artillery-retreat-range.csv) cover one arena and income setting.

With the rules-12 terrain field, artillery won the same six seat-swapped snake factory games. Grinder games ended at ticks 847 and 920, tank games at 1,287 and 1,664, and grunt games at 5,738 and 6,097 (artillery in seat 0 and 1 respectively). All six rules-11 results also reproduced their archived terminal hashes when replayed with their original fingerprint.
