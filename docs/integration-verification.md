# Browser integration verification — 2026-09-13

Implementation: `agent/ui-final`, rebased onto main's `6f35ebd` simulation fixes. This record covers the browser game and shared controller/archive integration. The native input-only peripheral is assigned separately and is non-blocking for UI work.

## Reproducible checks

Use the launch and installation instructions in [development](development.md) and the [browser review workflow](browser-testing.md). Local checks include `scripts/check.sh`, `node scripts/gate2-check.mjs`, `node scripts/match-check.mjs`, `node scripts/gate5-check.mjs`, `cargo test --release -p atemporal-sim --test equivalence`, and `cargo bench -p atemporal-sim`. No hosted checks or user playtest gate were introduced.

The browser scenarios drive real inputs and real servers: the normal opening, production, group inheritance, retroactive replacement, authoritative skipped reasons, historical comparison, graphs, timed boundaries, same-port restart, 3-player FFA, 4-player FFA, 2v2 teams, both control limits, partial-round resume, forced historical-cache regeneration, and temporary loss/recovery in both objectives. The focused playtest scenario verifies ghost queues/priorities/start orders, constructor order replacement and undo/redo, R rotation, bottom-edge access, fog, contextual actions, matter at the viewed tick, applied-order markers and Shift-wheel panning. Finished-match coverage checks WIN/LOSS colors, disabled commits and retained replay.

## Final verification and visual review

`scripts/check.sh` passed on the completed runtime source: workspace/default and serial simulation tests, strict Clippy, TypeScript/build, shared fixtures and generated-file drift. Gate 2, match-controller scenarios, all seven Gate 5 scenarios and release cold/warm/thread/checkpoint equivalence passed. The final cap benchmark reached 20,000 ticks with one and four threads. Logs are retained locally as `/tmp/ui-handoff-check.log`, `/tmp/ui-final-gate2.log`, `/tmp/ui-final-match.log`, `/tmp/ui-final-gate5.log`, `/tmp/ui-final-equivalence.log` and `/tmp/ui-final-bench.log`.

The final full desktop run passed 12 scenarios (`artifacts/ui/2026-09-13T08-21-52.158Z-560201`); the narrow run passed four (`2026-09-13T08-23-25.967Z-562551`). Finished-match copy and the default real-server review command were then rechecked in both viewports: two passed (`2026-09-13T08-26-44.687Z-566342`). These runs retain console/network diagnostics, PNGs, accessibility records, video on failures and traces. Earlier failed runs remain available and are not presented as passing evidence.

Full rendered game frames were manually inspected, including the bottom map edge, configured ghost factory, completed factory and newborn, fogged battlefield, combat destruction, selected-order timeline, restored historical round, loss/recovery history, final desktop LOSS, narrow WIN, and dense spectator/player views. The final frames have readable production emphasis, distinguishable directional shapes, no full-health bars, clear win/loss colors and usable replay after match end. Narrow command panels require scrolling. The recovery trace was also inspected for its action sequence and captured frames; transient polling assertions resolved successfully in the passing test.

Representative final frames:

- [Timed recovery and full command deck](../artifacts/ui/2026-09-13T08-21-52.158Z-560201/results/variants-recovery-2v2-time-906cc-n-delayed-factory-completes-desktop-chromium/recovered-player-and-survival-history.png)
- [Narrow ghost production](../artifacts/ui/2026-09-13T08-23-25.967Z-562551/results/playtest-playtest-fixes-re-c97ed-hosts-fog-and-applied-ticks-narrow-chromium/ghost-production-configured.png)
- [Finished match: desktop loss](../artifacts/ui/2026-09-13T08-26-44.687Z-566342/results/variants-result-finished-m-eccb9-ed-loss-and-retained-replay-desktop-chromium/finished-match-loss.png)
- [Finished match: narrow win](../artifacts/ui/2026-09-13T08-26-44.687Z-566342/results/variants-result-finished-m-eccb9-ed-loss-and-retained-replay-narrow-chromium/finished-match-win.png)

The five intentional failure probes passed their outer harness checks, preserving diagnostics, screenshots, traces and independent identity videos (`artifacts/harness/1789288015241-566765`). The agent-neutral MCP smoke passed initialization, discovery, real-server navigation, guide popup selection, snapshot and screenshot (`artifacts/browser-mcp/smoke-1789288002246-566135`). Both review entry points now launch a real game server by default.

## Performance

Machine: Intel Core i7-8565U, four cores/eight logical CPUs, 15.3 GiB RAM, Linux; Rust 1.97.1 and Node 22.23.2. Measurements are local engineering runs, not dedicated-hardware guarantees.

| Authored simulation workload | One thread | Four threads | Simulated ticks |
| --- | ---: | ---: | ---: |
| Cave march, 2,006 entities | 2,138 ms | 2,163 ms | 3,000 |
| Battle, 2,000 entities | 630 ms | 616 ms | 1,430 |
| Production with export, peak 603 live / 744 births | 8,503 ms | 8,493 ms | 20,000 |

The production cap benchmark explicitly disables the decided-side shortcut and sets inactivity to the absolute cap; ordinary matches retain their configured stop rules. Its exported JSON totals approximately 267.5 MB samples, 73.8 MB checkpoints, 3.80 MB events, 2.66 MB stats, 0.68 MB timeline and 111 KB dictionary. The benchmark also exercises 100- and 500-entity workloads.

`node scripts/integration-performance.mjs` uses the real server, a 96×96 cave map, 20,000 ticks, weapons disabled and factory/grunt cost 1. Two factories each queue 1,000 units. Congestion blocks the remaining output at 1,264 live entities (376 and 366 pending, plus active items); this is not claimed as a 2,000-live-unit browser run. The separate authored benchmarks above cover 2,000 entities.

The dense revision took 32.8 seconds from last commit to publication; two tick-zero rewrites took 41.8 and 32.6 seconds. Cold protocol exact-state queries at ticks 19,003, 19,997 and 12,347 took 15, 74 and 46 ms. Peak process RSS across publication/rewrites was 2,079,372 KiB. The revision retains 6,601,495 events and about 4.3 GiB of JSON cache; current revisions are exempt from retention budgets. This deliberately congested cap-length case is substantially heavier than ordinary opening/playtest matches.

The stress run prompted streamed archive serialization and an optional effects-only event query for the browser. The previous export allocated large JSON buffers and peaked at 5,649,588 KiB for the first dense publication, which took 40.7 seconds. Streaming retained identical event counts/cache bytes while reducing that allocation overhead. Full movement diagnostics remain available through ordinary event queries.

The final dense spectator overview fits the whole map. Browser cold seek took 742 ms. Paused spectator, playing spectator and playing player frames all had 16.7 ms medians and 33.4 ms p95. Player playback with fog improved from a 50 ms median / 66.7 ms p95 after sharing vision calculations, culling off-screen drawing and removing the comparison panel's complete-history download. The player advanced 48 ticks during the measured playback window; its measured JS heap was about 92 MB. These are dense-workload measurements, not a universal 60 fps claim.

Resuming this extreme 4.3 GiB cache took approximately 59 seconds. Final high-water RSS for archive resume plus browser review was 1,121,132 KiB (Linux `/proc` high-water sampling, retained separately from the 2,079,372 KiB simulation/publication run). An intermediate review exposed an unnecessary multi-gigabyte event download by the comparison panel; it now uses effects-only queries and loads ancestor metadata without regenerating their full replay bodies. Detailed historical comparison loads on demand. The remaining heavy-cache restore time is a measured limit; ordinary variant browser tests use much smaller revisions.

Raw measurements and rendered evidence are in `target/integration-performance/summary.json`, `dense-battlefield.png`, `dense-playback.png`, `dense-player.png` and `trace.zip`. `--review-only` resumes the existing stress archive and repeats browser review without rerunning the simulation workload.

## Verification limits

Chromium is the verified browser. Firefox/WebKit, touchscreen-only play and WAN behavior were not tested. Narrow layouts support keyboard/mouse input with scrolling command panels; the primary play surface is desktop. Fog limits battlefield rendering and selection, while replay/statistical data remain inspectable. Local draft lock estimates are deliberately non-authoritative; published command outcomes are the source of truth. Heavy current revisions can exceed configured retention budgets as measured above.
