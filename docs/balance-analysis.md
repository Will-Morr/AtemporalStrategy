# November 2026 balance analysis

## Recorded play

The public server exposed one completed game when sampled on November 18 UTC. Both seats were human. The match had ten published rounds, twenty accepted turns and 817 committed commands. It ended 5–3 with two stalemates under its original five-point rules. The old coordinator omits games after six hours of inactivity. This sample covers two people on one map. Their successive timelines share orders and terrain.

| Recorded result | Count |
| --- | ---: |
| Seat A timeline wins | 5 |
| Seat B timeline wins | 3 |
| Stalemates | 2 |
| Timelines that reached the 20,000-tick limit | 2 |

Both people revised early history. Six of the eighteen follow-up turns moved to a tick earlier than that player's preceding turn. Of the 817 commands, 581 saved blueprint configurations, 90 placed blueprints, 67 edited live production and 53 issued unit orders. Many configuration edits replaced an earlier edit in the same draft. Summing their queue lengths would greatly overstate units produced. In the final timeline, 52 commands had at least one skipped recipient, including 54 missing targets across all commands.

The first timeline illustrates the cost of an idle production line. Seat B mined 4,174 matter, spent zero on units and ended with 4,354 in the bank after placing factories without assigning construction. Adding the builder order at tick 1 in round two coincided with 2,608 unit spend and a stalemate. Both players changed their plans that round. Later results also favored spending capacity. In seven of the eight decisive timelines the winner mined more matter, and in all eight the winner spent more on units. These results include missiles in the engine's unit spending category. They describe that match and cannot isolate a causal effect.

The repository also contains an [anonymized six-turn human-versus-bot recording](../fixtures/bots/sustained-pressure.json). The human originally won 5–0 against the then-current bot. In the original first timeline the bot banked 37,554 matter while spending 160 on units. These are two independent matches and three human seat scripts. The November players are distinct, and the October fixture has no identity with which to deduplicate its participant.

## Replaying the human opponents

Each trial replaces one seat from the opening while leaving that person's opponent on their original side of the map. Both bot generations use the same released engine, map, starting state, resources, artillery update and five-point historical scoreboard. Scores in this table list the **bot first**.

| Opponent script | Standard 1.0 | Tactical 2.0 | Rounds played by Tactical | Recorded commands with an absent or dead target in Tactical's final timeline |
| --- | ---: | ---: | ---: | ---: |
| November seat A | Lost 0–5 | Won 5–0 | 10 | 24/278 |
| November seat B | Lost 1–5 | Won 5–1 | 6 | 21/373 |
| October human, held out | Trailed 0–3 after all 6 turns | Won 5–0 | 5 | 0/30 |

The policy was locked before playing the October human's plan. Tactical survived alone in all five scoring rounds. The human’s sixth recorded turn was a pass and the five-point match had already finished. Four winning timelines still reached the 20,000-tick limit, so the improved policy can have long simulations. Standard played all six turns without a match winner. Tactical won against all three recorded scripts, including the held-out one. Two matches cannot establish a general win rate against people playing reactively.

The November game informed bot development. A second stress test installed all ten human turns at the start, then held the player's full plan fixed. Tactical again won 5–0 and 5–1 respectively. Its last timelines in these tests contained 24/278 and 32/539 human commands with an absent or dead target. These counts describe commands with at least one affected recipient, including commands that still applied to other recipients. Human players could react and reissue orders in a live match.

Standard's first factory queues two miners and a repeating grunt with a mining rally. A fighter cannot inherit mining, so this line needs another intervention before it sends a wave to the enemy. Its economy aims for three miners and one constructor and its expansion stops after two factories. During testing, an early wave could kill the builder before another factory came online. A large army could also finish its march at the old enemy base while a remote factory kept the other side alive.

Tactical separates economic and combat production. Its opening factory keeps producing workers under a mining rally. Other factories rally fighters forward, train additional constructors to work on expansion, and run artillery to break entrenched defenses. Most expansion receives medium spending priority, and the outlying silos use low priority so they can use spare income. Widely spaced outposts keep a single strike from ending all production. Against a stalemate Tactical divides a sufficiently large force into search parties and sends them toward known survivors or passable areas its own army has barely visited. It may retarget silos using infrastructure it saw on the published timeline. It preserves a uniquely winning timeline by passing. The original bot remains selectable as Standard 1.0.

Tactical also won all ten completed matches against Standard on seeds 13, 42, 97, 101 and 211, playing each seat on every map. These were full first-to-three matches. Seeds 13, 42 and 97 appeared during development. Seeds 101 and 211 were first run after the policy was locked. Across the ten matches Tactical won 30 scoring timelines and Standard won three. The counterfactual human results are reported separately above.

As a check on the historical data, the old-rules replay of November round nine reproduced the published full state at terminal tick 1,104, SHA-256 `b52757065aebfeb378aae262e35c15adf5fed2c85319434280acc4a495233871`.

The [reusable combat balance lab](combat-balance.md) adds income-limited factories, complete pure and mixed rosters, and downloadable results for comparing production economies.

## Equal-budget scenarios

The engine ran seven recipes against one another, swapping their sides on an open 31×15 map and the same map crossed by a three-tile chokepoint. Each seat got 600 matter. In field fights both armies started deployed, with unspent change kept in the bank. In queue trials both sides started with one factory, an attack rally and a repeating recipe. The simulation stopped by tick 900. Field victory required eliminating the other army. Queue victory required destroying the other factory. The net exchange below uses the opposing side's lost invested matter minus the tested side's losses, including the loss of a factory.

The recipes were pure grunt, tank, grinder and artillery, two grunts followed by artillery, tank followed by artillery, and scout followed by artillery. Each artillery sight candidate played 168 simulations. Each individual composition participated in 48 of them.

| Artillery sight | Pure artillery wins | Scout/artillery wins | Two grunts/artillery wins | Mean pure artillery exchange |
| --- | ---: | ---: | ---: | ---: |
| 5 tiles, old | 7/48 | 0/48 | 15/48 | −381 matter |
| 7 | 7/48 | 1/48 | 20/48 | −356 |
| **8, released** | **12/48** | **4/48** | **12/48** | **−284** |
| 9 | 12/48 | 4/48 | 16/48 | −247 |
| 10 | 17/48 | 6/48 | 17/48 | −188 |

Artillery had a ten-tile weapon but attack-moving guns first acquired unengaged enemies at five tiles, the same sight as a tank. Eight was the first tested distance that improved both pure artillery and scout/artillery wins over seven. The patch leaves its cost at 80, keeps its ten-tile weapon and preserves some advantage for enemy units outside its unaided sight. The mixed grunt/artillery recipe lost three wins at the selected setting. A formation and the arrival order of reinforcements can change the result. These curated fights have no mining, new construction or missiles.

Queue cadence reversed the grunt/tank result at both sight settings. In four mirrored field fights, 40 grunts defeated six tanks. In all four single-factory trials, the tank queue defeated the grunt queue. At five-tile artillery sight, the mean grunt exchange against tanks was +383 in the open field and +180 across the choke, versus −650 on either production map. The straight factory rally goes through the chokepoint in both layouts, producing identical results in those queue trials. Grunt and tank prices stay at 15 and 100 respectively.

A separate siege trial gave the defender six completed turrets in two rows, totaling the same 600 matter as the attackers. With old five-tile sight, a pure artillery loop destroyed one turret on average and lost 560 matter. At eight tiles the artillery loop destroyed all six and lost 120 matter on average. The tank loop destroyed three and lost 600, and the grunt loop lost 600 without destroying a turret. The loop results held in both directions on both layouts. This matchup explains why a factory dedicated to artillery can justify its cost alongside ordinary military production.

## Reproduction

Run from the repository root. The first command exercises the entire matchup matrix for the given artillery sight. The export command uses Node 22 and an unexpired, unlocked spectator replay. It writes the public starting state, config, turn ledger and round outcomes into a new local directory.

```sh
cargo run --release -p atemporal-sim --example balance_matrix -- 8
cargo run --release -p atemporal-sim --example balance_matrix -- 8 siege
node scripts/export-balance-replay.mjs "$REPLAY_PAGE_URL" /tmp/game-export
cargo run --release -p atemporal-server --example replay_opponent -- /tmp/game-export tactical 0
cargo run --release -p atemporal-server --example replay_opponent -- /tmp/game-export standard 1
cargo run --release -p atemporal-server --example replay_opponent -- /tmp/game-export tactical 1 --frozen
cargo run --release -p atemporal-server --example replay_opponent -- fixtures/bots/sustained-pressure.json tactical 1
cargo run --release -p atemporal-server --example replay_opponent -- seed:42 tactical 1
```

The replay evaluator retains the recorded human's seat, map, starting state, timestamps and command identities, replacing the other seat from round one. Each bot plans against the previous published state before the next human commands enter the simulation. Both policies use the release's game rules and unit stats. The evaluator prints the opponent's commands with a missing recipient alongside each timeline score. If a recorded order references a unit that failed to spawn in a changed timeline, the engine handles the missing target. The original scoreboard target stays pinned. Seeded runs use generated maps, the release defaults and Standard 1.0 in the opposite seat. The release adds only aggregate results. The raw November export stays outside the repository.

## Wall firing in the two November 21 matches

Both matches paired WillM (seat 0) with Tactical 2.0 (seat 1), first to three. The control reproduced all nine archived revisions under rules 13, including both opening states. Final state hashes, command outcomes, simulation outcomes and cumulative scores matched the archives. A second run reused the same requests and enabled only allied-wall fire and turret fire over constructed walls and rock. Both seats kept every recorded order and intervention tick. Factory output reservations and bot planning changes were disabled for this comparison.

| Match | Round | Archived result | Wall-fire result | Terminal tick, old → new |
| --- | ---: | --- | --- | ---: |
| First | 1 | WillM | WillM | 2,266 → 2,635 |
| First | 2 | WillM | WillM | 3,662 → 1,949 |
| First | 3 | WillM | WillM | 3,862 → 3,473 |
| Second | 1 | Draw | WillM | 20,000 → 20,000 |
| Second | 2 | WillM | WillM | 20,000 → 5,914 |
| Second | 3 | WillM | WillM | 20,000 → 20,000 |
| Second | 4 | WillM | Match already won | 5,828 in archive |

WillM won both matches 3–0. In the second match the wall change moved the deciding point from round four to round three. Its first combat divergence was Tactical’s turret at (41, 38) firing across map rock at WillM’s scout on tick 252. Tactical was then eliminated at tick 1,229 in the changed opening. The simulation continued to the 20,000-tick cap. The experiment measures the combined firing change against these fixed plans.
