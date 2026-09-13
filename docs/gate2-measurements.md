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
