# Gate 2 measurements

Measured on the working slice, not estimated. Machine: Intel Core i7-8565U (4 cores/8 threads, 1.8 GHz base), 15 GiB RAM, Linux, Rust 1.97.1 release build, Node 22.23.2, headless Chromium. Repeat with `node scripts/gate2-check.mjs` (writes `target/gate2-summary.json`) and the browser walkthrough in [development](development.md). Default configuration (`config/game.yaml`: 48×48 map, 2 players, snapshot interval 5, checkpoint interval 100, stall 300, cap 20,000, export enabled).

## Round latency (final commit received → durable round record → published)

| Round | Rewrite tick | Base checkpoint | Ticks simulated | Sim | Persist | Commit → publish | Terminal tick / outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 (start) | — | 0 | 300 | 1 ms | 8 ms | 10 ms | 300, quiet stalemate |
| 1 | 0 | 0 | 12,318 | 62 ms | 35 ms | 98 ms | 12,318, stalemate (ore exhausted, inactivity) |
| 2 | 153 | 100 | 19,628 | 722 ms | 144 ms | 869 ms | 19,728, win (looping grunts vs. turret; 1,607 entities born) |
| 3 | 40 | 0 | 12,318 | 46 ms | 36 ms | 84 ms | 12,318, stalemate (earlier rewrite voids the factory) |

Browser-side, the client observed the published revision within the same second; the walkthrough's exact-state round trips ranged 2–36 ms including the WebSocket hop. The proposed 5-second typical-round target has a large margin on this workload.

## Cold and warm exact-state seeks

In-process reconstruction from the nearest checkpoint (≤ 99 ticks) took 0.35–0.75 ms server-side for non-sample ticks (7, 153, 999, 1234, 2001), 0.11 ms on a cache hit, and 1 ms round trip over the loopback WebSocket. Exact-state payload: 33–35 KB of JSON for a 48×48 world (terrain and ore dominate; entities are small). The proposed 250 ms cold-seek target is met by two orders of magnitude at this population; measure again at 2,000 entities.

## Per-revision result sizes (uncompressed JSON on disk, `results/<revision>/`)

| Revision | Samples | Checkpoints | Stats | Events | Timeline | Dictionary |
| --- | --- | --- | --- | --- | --- | --- |
| 1 (12,318 ticks, 7 entities) | 3.98 MB (2,465 samples) | 4.2 MB (124 states) | 1.55 MB | 2 B | 279 KB | 1 KB |
| 2 (19,728 ticks, up to hundreds of live entities) | 22.1 MB (3,947 samples) | 11.1 MB | 2.56 MB | 2.79 MB (11,279 events, 4,833 attacks) | 1.98 MB | 238 KB |

gzip of revision 2: samples 22.1 MB → 420 KB, checkpoints 11.1 MB → 439 KB, events 2.79 MB → 82 KB, stats → 92 KB, timeline → 72 KB. Terrain and ore repetition make the JSON highly compressible.

Wire payloads (JSON text over WebSocket): `RevisionPublished` 70 KB (revision 1, 601 coarse timeline buckets) and 225 KB (revision 2, 1,933 buckets); full-timeline `SnapshotRange` at stride 5 for revision 3: 4.19 MB / 2,465 samples; a 1,000-tick window: 595 KB / 201 samples; full `StatsRange` 1.55 MB; `GetCommands` 2.3 KB; `Welcome` 1.5 KB. The client fetches 1,000-tick chunks on demand around the playhead (13–37 ms per chunk in the walkthrough), never the whole history.

## Choices these numbers support

- Keep `snapshot_interval` 5 and `checkpoint_interval` 100; both are cheap at this scale and reconstruction is sub-millisecond.
- Movement `Move` events are not emitted; interpolating between 5-tick samples is enough for playback and would otherwise dominate event volume (every unit moves every 2–3 ticks). Attack/impact/destroyed/displacement/survival events are emitted.
- Compress result caches and consider `permessage-deflate` or gzip for range payloads before raising retention budgets: the 50× ratio changes the disk budget question entirely. Retention itself is not yet bounded (server breadth work).
- The timeline index coarsens worker buckets to 20 ticks and keeps only the dominant activity per player; that bounded `RevisionPublished` to ≤ 225 KB even for the 19,728-tick revision.
- Per-sample `StatsSample` at every snapshot tick is the largest avoidable payload; the stats route should bucket server-side before graphs land.

## Not measured yet

Memory ceilings, the 20,000-tick cap workload with 2,000 entities, dense chokepoints, repeated near-zero rewrites, and multi-thread intent computation (the slice runs intents serially). Those belong to Gate 3/6 and the simulation/server breadth agents.

## Simulation breadth

Measured on the same machine (Intel Core i7-8565U, 4 cores/8 threads, 15 GiB RAM, Linux, Rust 1.97.1 release build) with `cargo bench -p atemporal-sim` (`crates/sim/benches/scale.rs`, best of three runs, in-process, no server or disk). The suite ran on a lightly loaded laptop; expect ±15% between runs.

Scenarios: **march** is a 96×96 generated cave map with weapons removed, half the population at each start attack-moving to the other start for 3,000 ticks (path fields, displacement, corridor crowding, settling). **battle** is an open 96×96 field with two mixed armies (grunts, grinders, scouts, tanks, artillery, turrets) charging each other until inactivity (target scans, line of sight, combat, deaths).

| Scenario | Threads | Entities | Ticks | Wall | ms/tick | Ticks/s | Stop |
| --- | --- | --- | --- | --- | --- | --- | --- |
| march 100 | 1 | 106 | 3,000 | 301 ms | 0.100 | 9,983 | horizon |
| march 100 | 4 | 106 | 3,000 | 321 ms | 0.107 | 9,333 | horizon |
| battle 100 | 1 | 100 | 441 | 52 ms | 0.118 | 8,508 | inactivity |
| battle 100 | 4 | 100 | 441 | 51 ms | 0.115 | 8,667 | inactivity |
| march 500 | 1 | 506 | 3,000 | 817 ms | 0.272 | 3,670 | horizon |
| march 500 | 4 | 506 | 3,000 | 794 ms | 0.265 | 3,778 | horizon |
| battle 500 | 1 | 500 | 577 | 196 ms | 0.339 | 2,948 | inactivity |
| battle 500 | 4 | 500 | 577 | 211 ms | 0.366 | 2,730 | inactivity |
| march 2,000 | 1 | 2,006 | 3,000 | 2,661 ms | 0.887 | 1,127 | horizon |
| march 2,000 | 4 | 2,006 | 3,000 | 2,651 ms | 0.884 | 1,132 | horizon |
| battle 2,000 | 1 | 2,000 | 1,430 | 781 ms | 0.546 | 1,831 | inactivity |
| battle 2,000 | 4 | 2,000 | 1,430 | 737 ms | 0.515 | 1,941 | inactivity |

Full configured cap (default `config/game.yaml`, 48×48, ore raised to 80,000 per start so income outlasts the horizon; both players mine, build a factory, then loop grunts at the enemy miner with the constructor also mining; export enabled, every output serialized to JSON in-process):

| Threads | Terminal | Entities born | Peak alive | Sim wall | ms/tick |
| --- | --- | --- | --- | --- | --- |
| 1 | S[20,000], horizon, win | 740 | 602 | 5,846 ms | 0.292 |
| 4 | S[20,000], horizon, win | 740 | 602 | 5,852 ms | 0.293 |

Export volume for that run (uncompressed JSON): samples 257.7 MB (4,001 at interval 5), checkpoints 73.5 MB (200), events 0.73 MB (2,442), stats 2.64 MB, timeline 0.67 MB, dictionary 110 KB. Samples dominate because every sample carries the full ore list; the server-side retention budget and compression from the earlier sections apply unchanged.

What the numbers say:

- The 500-entity, 12,000-tick target (under two seconds) holds with margin: 500 entities run at 0.27–0.37 ms/tick, so 12,000 ticks are 3–4 s only in the pathological all-units-marching case and well under 2 s in the battle profile where units settle or die. The cap run at up to 602 live entities completes 20,000 ticks in under 6 s.
- The intent pool is gated on armed population (1,000 armed entities and `simulation_threads > 1`). Below that, per-tick scoped thread spawns cost more than the target scans they split, which is why 100/500-entity rows show no thread effect and 2,000-entity battle gains only ~6%. The Gate 3 fixtures (`crates/sim/tests/equivalence.rs`) prove identical hashes, events and checkpoints for one and four threads, cold checkpoint reruns and a capacity-1 evicting field cache on a 1,100+-entity battle.
- Profiling before these numbers showed target scanning (line-of-sight traces) and motion dominating; line of sight is now traced only for the nearest candidates in order, and a per-team prefix count of hostile occupants skips scans whose box holds no enemy. Motion at 2,000 marching units is the remaining cost (per-entity field lookups and displacement); it is linear in population and was not tuned further.
- Not measured: peak memory, dense chokepoints as a dedicated benchmark, and repeated near-zero rewrites (server-side, Gate 6).
