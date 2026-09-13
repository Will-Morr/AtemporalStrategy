# Browser integration verification — 2026-09-13

## Production, turret repair and persistent scoreboard

Implemented on `agent/production-priority`, rebased onto `0b61078` without conflicts. Factories capture repeat flags per queue item, including blueprint settings and active production; Shift-click adds five. Mixed blueprint selections only receive compatible recipe/loop edits. A shared bottom priority dropdown supports High/Medium/Low/Off with no battlefield priority labels. Off pauses spending; complete turrets automatically repair at 0.5 HP/tick for 1 matter/HP, half their construction efficiency, independently of firing. Terrain is darkest for walls, medium for unseen floor, lightest for seen floor; miners have blocky owner-colored silhouettes.

The persistent scoreboard shows each player's completed timeline wins, the configured target, and Leading/Winner labels. Latest published match totals survive historical viewing and refresh, and remain unchanged during progressive replay. Team/adjusted points are separate from individual wins. Camera transforms reserve the responsive header height.

Verification:

- `scripts/check.sh` passed: workspace Rust tests, strict workspace/simulator Clippy, serial simulator tests, client build, JavaScript contract/plot/scoreboard checks, guide generation and generated-file drift. Final client build and tests also passed after the mixed-selection filter and short-viewport bound correction.
- New simulator tests cover per-item loops through checkpoint reconstruction, editing an existing loop, turret repair efficiency/rate/caps and simultaneous fire, Off on all three consumers, and repair priority. Contract tests reject misaligned blueprint flags. Archive tests reject the prior simulator rules stamp.
- Focused desktop/narrow production, blueprint and scoreboard runs passed. The full 42-scenario run at `artifacts/ui/2026-09-13T20-50-49.573Z-895219` passed 40; the two opening walkthroughs used outdated miner-priority/global-loop steps and the removed `top-score` locator. The intermediate opening retry (`2026-09-13T20-54-37.153Z-899679`) timed out on that locator and reported a trace cleanup stream error; it was stopped and is retained as failed evidence.
- After updating those walkthrough inputs/selectors, both opening/rewrite/archive scenarios and both expanded mixed-blueprint production scenarios passed in `artifacts/ui/2026-09-13T21-00-37.392Z-903777` (4/4). All four final traces passed ZIP integrity checks. This gives all 42 distinct scenarios passing coverage, not a claim of one uninterrupted clean 42-case run. The broad cases cover multiplayer, single-order, timed/hybrid, archive recovery, progressive replay, uncommit and mismatch handling across authoritative/peripheral transports.
- Scoreboard scenarios play an actual five-win match on both screen sizes, test history and refresh between wins, and verify the final winner and losing-player displays. Score aggregation checks separately cover teams, ties, draw credits, adjusted leadership, revision ancestry and missing history.

Manually reviewed full desktop/narrow frames for terrain, miner ownership/shape, shared/mixed priority, turret repair Off, blueprint/site/completed mixed-loop queues, scoreboard ties/leaders/five-win victory, and four-player team totals. Narrow command panels retain their existing scroll behavior; long player names truncate visually with the full name available in accessible labels/tooltips. Representative PNGs, final check logs and reports are retained under `artifacts/review/2026-09-13-production-scoreboard`; full run evidence remains in the worktree.

Compatibility: simulator rules stamp 3 intentionally requires the original binary to resume older-rule archives; same-rules archive restart and peripheral replication are verified. The separate 2,000-live-entity end-to-end stress measurement remains open; these browser scenarios do not close it.


## Blueprint and economy feedback pass

Implemented in `agent/ui-feedback` on main's central-clearing map baseline (`702f57c`), then rebased without conflicts onto the subsequently published side-lane map change (`af1a223`). Every unfinished structure has a visible cancellation action. Newly placed blueprints can be configured and cancelled in one draft; deleting their placement removes dependent settings. Rebased drafts check blueprint availability locally and preserve the draft with an actionable explanation before submission.

Selection priority, orders and factory looping distinguish shared and mixed values. Draft group membership is visible immediately; ungrouped units have no number badge. A shared bottom-panel priority dropdown replaces battlefield priority badges; the panel explains that building priority controls construction funding and Off pauses constructor spending. Ore is yellow until exhausted, then light gray; walls retain their dark gray through fog. Miners use their owner's color. Statistics add a five-sample local slope with signed axes and responsive labels.

Verification:

- `scripts/check.sh` passed: workspace and serial simulation tests, strict Clippy, client build, shared fixtures and generated-file drift. The slope checks cover linear/irregular sample spacing, negative/flat rates, missing data and singleton windows.
- The controller regression accepts same-draft placement/configuration/cancellation while rejecting foreign ownership, missing items and forward references.
- Direct-controller focused Chromium runs passed all four desktop/narrow cases. Final focused run: `artifacts/ui/2026-09-13T19-27-56.885Z-841600`. Seed 42; blueprint turns use a 1,200-tick cap; the mining case uses 1,800 ticks and 200 matter per start, mines one deposit completely, then moves the miner away for inspection.
- The peripheral-enabled full run (`artifacts/ui/2026-09-13T19-32-23.235Z-848086`) received SIGTERM after 13 passes, without an assertion failure or final report. All 13 completed traces have valid ZIP integrity. All 19 narrow cases then passed (`artifacts/ui/2026-09-13T19-35-27.502Z-854165`) and all 8 desktop variants passed (`artifacts/ui/2026-09-13T19-35-28.730Z-854219`), giving all 38 distinct scenarios passing coverage before the final side-lane rebase. The interrupted artifacts are retained.
- Final compact mixed-value labels and focused-queue Delete handling passed TypeScript/build and both desktop/narrow blueprint scenarios (`artifacts/ui/2026-09-13T19-42-31.841Z-862842`), including undo restoring the removed queue item without deleting its factory. Both final traces have valid ZIP integrity.
- After rebasing onto `af1a223`, all 18 controller tests, strict workspace Clippy, release map tests (6 passed; the manual map dump remains ignored), release server/peripheral build and client build passed. The broader suite above predates this map-only integration; all six placement/mining and opening/rewrite browser scenarios passed on the integrated result (`artifacts/ui/2026-09-13T19-46-54.153Z-865492`). All six traces have valid ZIP integrity. Fresh integrated desktop/narrow map frames were manually reviewed.

Manual review includes full desktop/narrow frames for mixed priorities, assigned groups, turret deletion controls, player-colored miners, yellow and exhausted gray ore, constant wall shading and smoothed mining plots. Review of the first narrow plot exposed illegible scaled labels; the final chart renders at its actual width. Focused traces were inspected for cancellation/rebase interactions and have valid ZIP integrity. Evidence is retained locally under `artifacts/review/2026-09-13-ui-feedback`; complete run evidence remains in the implementation worktree.

Representative reviewed frames:

- [Mixed selection](../artifacts/review/2026-09-13-ui-feedback/mixed-desktop.png), [narrow selection](../artifacts/review/2026-09-13-ui-feedback/mixed-narrow.png)
- [Delete turret blueprint](../artifacts/review/2026-09-13-ui-feedback/blueprint-desktop.png), [cancel turret construction](../artifacts/review/2026-09-13-ui-feedback/construction-desktop.png)
- [Miner owner color](../artifacts/review/2026-09-13-ui-feedback/miner-owner-color.png), [exhausted deposit](../artifacts/review/2026-09-13-ui-feedback/depleted-desktop.png)
- [Mining slope](../artifacts/review/2026-09-13-ui-feedback/slope-desktop.png), [narrow slope](../artifacts/review/2026-09-13-ui-feedback/slope-narrow.png)

This pass does not close the separate 2,000-live-entity controller/browser/peripheral stress measurement. Firefox/WebKit, touchscreen-only and WAN behavior remain unverified.

## Progressive replay pass

Implemented in `agent/progressive-replay`, rebased onto main's seeded traffic-map changes (`aa8e7a6`). Completed prefixes are available during simulation from both the controller and native input-only peripheral. Generation checks isolate retries; exact provisional states stay outside published caches. Playback waits at the frontier, refresh restores access, and final verification preserves the inspected tick. Planning stays closed and the result remains explicitly pending until publication.

Verification:

- `scripts/check.sh` passed after the rebase: workspace and serial simulation tests, strict Clippy, client build, shared fixtures and generated-file drift. The final client refinements also passed TypeScript/build.
- New controller coverage checks an exact non-sample tick against the published replay, rejects future ticks and expired retry generations, and closes the preview on publication. Peripheral coverage checks replacement/reopened inputs and duplicate delivery without reusing stale previews.
- Release equivalence, Gate 2, match-controller scenarios, all seven Gate 5 recovery/fault scenarios and the native peripheral protocol suite passed.
- Full desktop/narrow Chromium run `artifacts/ui/2026-09-13T18-57-27.178Z-806566` passed 33 scenarios. The narrow opening scenario completed gameplay but exceeded the built-in 30-second trace teardown budget, leaving a truncated trace. The project now permits 120 seconds for that separate teardown, in addition to the review fixture's cleanup budget.
- Final focused run `artifacts/ui/2026-09-13T19-05-23.089Z-815191` passed all six cases: four progressive cases plus both opening scenarios after the camera correction. All six traces have valid ZIP integrity, bringing all 34 scenarios to passing coverage on the final runtime. Its progressive setup uses seed 42, a 6,000-tick cap and a test-only 150 ms batch delay, moves a constructor, plays across an advancing frontier, seeks exact tick 73, refreshes, and compares the complete provisional world with the final world. Production adds no artificial delay.
- The failure-artifact harness passed all five intentional probes with screenshots, traces, diagnostics and identity videos (`artifacts/harness/1789326336541-815556`).

Manual full-frame review covers desktop/narrow completed-prefix views, refreshed cameras with the player's units visible, pending-result/frontier presentation and verified handoff at tick 73. The desktop peripheral trace was inspected for play, typed seek, refresh and final seek, including recorded browser snapshots and ZIP integrity. Visual review found the refresh camera bug; the final cases additionally assert the constructor is within the usable viewport.

Representative evidence (git-ignored, retained locally):

- [Completed prefix](../artifacts/review/2026-09-13-progressive-replay/prefix-desktop.png)
- [Peripheral refresh](../artifacts/review/2026-09-13-progressive-replay/refresh-desktop.png), [narrow refresh](../artifacts/review/2026-09-13-progressive-replay/refresh-narrow.png)
- [Verified handoff](../artifacts/review/2026-09-13-progressive-replay/handoff-desktop.png), [narrow handoff](../artifacts/review/2026-09-13-progressive-replay/handoff-narrow.png)

Logs and the inspected peripheral trace are retained beside these frames; complete and earlier failed runs remain in the implementation worktree. This feature pass does not close the outstanding 2,000-live-entity controller/browser/peripheral stress measurement. Firefox/WebKit, touchscreen-only and WAN behavior remain unverified.

## Earlier player-facing polish pass

Implemented in `agent/playtest-polish`, including main's map changes through `aa61a9d`. This pass adds the shorter command deck, selection stats/icons, explicit factory plans and removal controls, independent tab identities/rejoin, uncommit, player action rows and latest-write labels, 16× replay speed, hybrid mode, constructor output clearance and attack-move retaliation. Progressive replay remains queued; it is not implemented by this pass.

Verification:

- `scripts/check.sh` passed: workspace tests, serial simulation tests, strict Clippy, client build, Rust/JavaScript shared fixtures and generated-file drift.
- The final full run passed **29 desktop/narrow Chromium scenarios**, including real peripheral play, shared-browser refresh/uncommit/undo/redo, three-player partial-round restart and withdrawal, hybrid history/score restoration, multiplayer, fog, recovery and game-end replay. Full run: `artifacts/ui/2026-09-13T18-07-14.135Z-756752` in the implementation worktree. The remaining narrow opening scenario completed gameplay but hit the default 30-second evidence-cleanup timeout; the review fixture now allows 120 seconds to flush recorded contexts. Its focused rerun passed with complete evidence (`artifacts/ui/2026-09-13T18-14-08.988Z-764206`), bringing all 30 scenarios to passing coverage on the final runtime.
- Before the final full run, the timeline-label refinement passed **6 focused desktop/narrow scenarios** covering factory play, seeking/zoom/pan, earlier-tick rewrites and four-player lanes. Run: `artifacts/ui/2026-09-13T17-52-05.924Z-738588`. Its client build/type check passed.
- The review fixture change passed all five intentional failure probes, including diagnostic screenshots, traces and independent identity videos (`artifacts/harness/1789323327774-764696`).
- Release thread/checkpoint/cache equivalence, Gate 2, match rules/restart, all seven Gate 5 fault cases and the inputs-only peripheral checks passed. The fault harness pins its seed and includes the extra editable-draft writes when positioning its disk-full injection.
- The latest terrain integration exposed a targeting priority regression: attack-movers now retain visible targets before considering distant incoming fire. A focused behavior regression and the opening scenario pass. Browser recovery now schedules its rescue after the observed elimination instead of assuming a fixed attack-arrival tick.
- A generated-map regression reproduced a one-bit JSON parse change in fractional ore. Enabling `serde_json/float_roundtrip` fixes it. The previously failing archive subsequently reproduced all three original hashes without modifying that archive.

Full rendered frames were manually reviewed for construction/loop/queue state in both layouts, visible-area enemy-blueprint hiding, restored drafts/rejoin, rewritten timeline markers, four-player lanes, and colored game-end results. The uncommit trace was inspected for the claim/reload/withdraw/resubmit/rejoin sequence and recorded frames. The timeline labels now occupy a separate area so early action markers do not obscure the latest written tick. Narrow panels use scrolling for secondary controls. Selection/factory inspection checks use paused exact ticks.

Representative reviewed evidence (git-ignored, retained locally):

- [Factory under construction](../artifacts/review/2026-09-13-playtest-polish/factory-desktop.png), [narrow factory](../artifacts/review/2026-09-13-playtest-polish/factory-narrow.png)
- [Restored uncommitted plan](../artifacts/review/2026-09-13-playtest-polish/uncommit-desktop.png), [explicit player rejoin](../artifacts/review/2026-09-13-playtest-polish/rejoin-desktop.png)
- [Latest rewrite tick](../artifacts/review/2026-09-13-playtest-polish/rewritten-timeline.png), [four-player lanes](../artifacts/review/2026-09-13-playtest-polish/four-player-timeline.png)
- [Desktop loss](../artifacts/review/2026-09-13-playtest-polish/loss-desktop.png), [narrow win](../artifacts/review/2026-09-13-playtest-polish/win-narrow.png)

Core logs and the inspected uncommit trace are retained beside those frames. Full reports/videos and earlier failure evidence remain in the implementation worktree. The 2,000-live-entity controller/browser/peripheral stress matrix is still open; performance figures below are earlier measurements, not a new stress measurement for this pass. Older partial-round archives without an original `.draft` can be withdrawn but cannot restore an editable draft; the UI states that limitation.

## Earlier integration baseline

Implementation: `agent/ui-final`, rebased onto `agent/peripheral` (`2f125b4`) while retaining main's simulation fixes. This record covers the browser, controller/archive and native inputs-only peripheral integration.

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


## Peripheral integration verification

The UI branch was rebased onto `agent/peripheral` (`2f125b4`). Conflict resolution retains the core simulation fixes, ghost settings and worker recovery while moving optional effects-only event filtering into the shared `RevisionData` queries used by both controller and runner. The updated protocol scenario compares the filtered stream with the rendered subset of the full event stream (54 effects in revision 2).

The assembled runtime passes `scripts/check.sh`, `scripts/peripheral-check.mjs`, `scripts/match-check.mjs`, all seven `scripts/gate5-check.mjs` scenarios, and release `atemporal-sim --test equivalence`. Peripheral revisions 0–3 match the controller archive; checkpoint replay, a tick-40 rewrite, exact tick 153, runner restart, controller resume, deliberately corrupted hash rejection, and a 1 MiB cache budget with regeneration all pass. These ordinary-sized rounds do not replace the outstanding 2,000-live-entity end-to-end scale measurement.

A real rendered mismatch check exposed an unhandled initial replay-query rejection. The browser now retains a visible red mismatch banner, disables orders and stops repeatedly requesting the failed revision. Desktop and narrow scenarios verify this with the real corrupted-hash runner, alongside the healthy configurable-ghost scenario. The failure evidence remains in `artifacts/ui/2026-09-13T15-15-24.476Z-625518`; later passing evidence uses the corrected runtime. Manual full-frame inspection covered the healthy ghost/production states and both mismatch viewports; the mismatch trace contains the actual claim/start/assert/capture sequence with no browser errors.

Local logs: `/tmp/peripheral-integration-check-final.log`, `/tmp/peripheral-integration-protocol.log`, `/tmp/peripheral-integration-match.log`, `/tmp/peripheral-integration-gate5.log`, `/tmp/peripheral-integration-equivalence.log`. Protocol summaries are under `target/`. The review checklist records the remaining scale-specific coverage rather than presenting the 1,264-live-unit browser workload as a 2,000-unit result.


The final clean command `ATEMPORAL_UI_PERIPHERAL=1 npm run ui:review --prefix client` passed **all 26 desktop/narrow Chromium cases** on the integrated source: `artifacts/ui/2026-09-13T15-24-39.535Z-635088`. This includes healthy peripheral ghost planning, visible mismatch rejection, both complete opening/rewrite walkthroughs, both objectives/control limits, FFA/team variants, recovery and final results. A prior combined run was interrupted after 25 passing cases because the narrow opening reused the desktop match; per-test servers now isolate it, and viewport-aware drag fitting prevents a mining gesture from crossing the minimap. Earlier failed/interrupted runs are retained as diagnostic evidence.

Full-frame review also covered narrow combat playback and historical replay (`2026-09-13T15-23-53.550Z-634552`). The final run retains matching healthy/mismatch frames, traces and browser diagnostics. Main's browser assets and release server/runner were rebuilt successfully. The integration merge preserves the tested rebased tree while retaining main's prior commit history; no force update of main is needed. Local evidence and logs are also copied into the main checkout, excluding regenerable test replay archives.

- [Peripheral ghost planning, desktop](../artifacts/ui/2026-09-13T15-24-39.535Z-635088/results/playtest-playtest-fixes-re-c97ed-hosts-fog-and-applied-ticks-desktop-chromium/ghost-production-configured.png)
- [Peripheral mismatch, narrow](../artifacts/ui/2026-09-13T15-24-39.535Z-635088/results/playtest-peripheral-mismat-74cf1-ning-without-browser-errors-narrow-chromium/peripheral-mismatch-blocks-planning.png)
