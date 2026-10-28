# Browser integration verification — 2026-09-13

## Shared map choice, rematches and factory displacement

Implemented on `agent/rematch-map-choice` from main `7dd746e`. The lobby offers five public terrain/ore/start previews, with a shared selection controlled by the first occupied slot. The selected seed produces the actual initial battlefield. A game-complete menu lets the controller return everyone to five fresh maps; profiles, tokens and the server address survive, while replay/draft/scoring state resets and the old archive remains on disk. Map changes require the current lobby revision; rematches require the current completed/archived match ID. Peripheral replay cursors reset, jobs include match identity, and stale exact/regeneration results cannot populate the next match's cache.

Ready factory births now push mobile output blockers, including a chain of units into nearby available space. Orders and movement cooldowns survive displacement; walls, buildings and completely packed areas remain solid. Fogged ore is muted yellow on the map/minimap while numeric readouts remain available. Costs are now silo 300, Satellite 50, Cluster 125 and Tac nuke 300 matter. These simulation changes use rules stamp 6.

Verification:

- `scripts/check.sh` passed on the committed implementation and revised authored group-birth fixture: workspace tests, strict Clippy, serial simulator tests, client build, JavaScript checks, guide and generated-file drift. The prior fixture expected a blocked factory to wait for a scheduled move; its authored expectations now verify immediate displacement/birth and subsequent group-order inheritance.
- The final native-peripheral Chromium suite passed **26/26** at 1920×1080 (`artifacts/ui/2026-09-14T02-14-46.280Z-1006931`), with all 26 trace ZIPs intact. This includes rematching while another player watches the old replay, multiplayer/match variants, recovery, progressive replay, missiles, production, fog, selection and game-end results.
- Simulator displacement scenarios verify immediate birth despite a parked miner's long movement cooldown, no duplicate birth or extra spend, chain displacement through allied/enemy mobile units, immovable buildings, and checkpoint equivalence. Existing movement and all twelve missile scenarios pass with the new prices/build times.
- Controller tests verify five options, authorization, stale/invalid selection rejection, exact preview-to-world equality, rematch phase/identity checks, preserved tokens/profiles/old archive, and fresh maps/empty replay state.
- The focused authoritative browser run passed **2/2** map/rematch and factory-displacement scenarios. The initial peripheral run passed those scenarios, ore shading and the manual missile scenario; its automatic-targeting test failed because ordinary scouts died before the newly more expensive missile finished. Durable spotters in that test setup preserve the intended acquisition check; the corrected automatic scenario passed. Failure artifacts remain retained.
- Manually inspected full 1920×1080 frames for the five-map picker, shared selection, game-complete menu and muted fogged ore. The new menu leaves replay inspection available, and all five previews fit together on desktop.

Final full-frame review additionally confirmed the retained-player fresh-map lobby and factory displacement/spawn. Representative PNGs, metadata and logs are retained in `artifacts/review/2026-09-13-rematch` in the main checkout; full reports/traces remain in `/home/will/atemporal-rematch/artifacts/ui`.

The unrelated 2,000-live-entity controller/browser/peripheral cold-seek and peak-memory measurement remains open. Mobile support is outside the required scope.

## Missile silos and desktop review target

Implemented on `agent/missile-silos` from main `be32b99`. Silos use matter priority and per-item production loops, store missiles by type, and accept automatic or ordered manual launches while planned, under construction or complete. Manual launches are unlimited and take 1–30 ticks. Satellite vision follows flight and persists at landing; Cluster deals 100 damage within seven tiles; Tac nuke annihilates entities within four tiles, including allies and entities receiving simultaneous healing. Silo destruction loses stock but does not cancel launched missiles. See [missile rules and implementation defaults](missiles.md).

Replay checkpoints, compact samples, events and native peripheral replication include missile state. Range overlays show turret reach and silo automatic acquisition; launch previews show blast/vision area and flight time after launch. Stock counts and targeting buttons remain beside an expandable production section. One-tick flights between compact samples are reconstructed from launch events.

The user's latest display decision makes desktop the required target. The harness now uses **1920×1080 at device scale 1**, with no mobile project or forced phone-sized guide screenshots. Earlier 1440×1000 and 390×844 evidence below is retained as history, not a mobile support commitment.

Verification:

- `scripts/check.sh` passed after the final launch-cutoff fix: workspace tests, strict workspace/simulator Clippy, serial simulator tests, client generation/build, JavaScript fixture/plot/score tests, guide and generated-file drift. The release server and native peripheral also rebuilt successfully.
- The final peripheral-enabled desktop suite passed **24/24** at 1920×1080 (`artifacts/ui/2026-09-13T23-20-15.198Z-974253`), with all 24 trace ZIPs intact. This run includes the final launch-cutoff guard and covers multiplayer, timed/single-order/hybrid variants, recovery, progressive replay, queues and match-end results. Factory placement and the five-win scoreboard were also manually reviewed at this desktop size.
- Final 1080p controller silo coverage passed **2/2** (`artifacts/ui/2026-09-13T23-20-15.204Z-974260`), with both traces intact. Full 1920×1080 frames were manually inspected for silo placement, target flight/radius previews, stored inventory, satellite flight and Cluster impact.
- Twelve simulator missile regressions cover inventory/loops/Off, production and launch waiting, blast boundaries, friendly fire, simultaneous healing, satellite flight/expiry, unlimited range and the flight cap, silo death, automatic shared vision, cancelled plans, ghost configuration, invalid ownership, cold/evicted checkpoint replay, and early-termination/cooldown boundaries.
- The broad peripheral-enabled run passed **48/48** desktop/narrow scenarios (`artifacts/ui/2026-09-13T23-07-32.605Z-960325`), and all 48 trace ZIPs passed integrity checks. This run used the runtime before the final launch-cutoff guard. Its desktop silo trace contains actual production, map-target, commit, playback, rewrite and refresh inputs.
- Authoritative-controller silo coverage passed **4/4** in the old two-viewport matrix (`artifacts/ui/2026-09-13T23-07-32.612Z-960327`). The earlier automatic-targeting test sought state zero and incorrectly expected the tick-zero command to have applied. Seeking state one corrected that test; the original failure remains retained.
- Full rendered frames were manually inspected for stockpiles, targeting previews, turret reach, moving and landed satellite vision, nuclear impact, and automatic fire with an allied spotter. Initial review moved stock counts and launch targets out of the expanded production list. Narrow panels required scrolling; mobile is now outside the required scope.

Representative frames, result metadata, trace integrity summaries and check logs are retained in `artifacts/review/2026-09-13-missile-silos` in the main checkout; full reports and traces remain in the implementation worktree.

The separate 2,000-live-entity controller/browser/peripheral cold-seek and peak-memory measurement remains open. This feature pass does not claim that workload, Firefox/WebKit or WAN coverage.

## Work targeting, fogged ore and selection

Implemented on `agent/pathing-selection` from main `e79ec1b`. Mining acquires reachable ore by walking distance with exclusive claims. Construction ranks usable adjacent work positions by legal walking distance for both planned and funded buildings; creation precedence only breaks equal paths. Source fields rank candidates without generating a separate BFS for each candidate. Dedicated work-goal fields guide the selected constructor, while factory-output clearing retains ordinary movement. Grinders now cost 100 matter with 275 HP, 25 damage per five ticks and movement every two ticks; tanks retain 250 HP, the same cost/damage rate and movement every four ticks.

Ore appears through fog on the field and minimap, with a remaining-matter hover readout; enemy entities stay hidden. All army and All units select owned entities across the viewed map. Type chips retain/drop an owner/type, and clear recalled-group routing before later commands.

Verification:

- `scripts/check.sh` passed on the committed implementation and refreshed authored content fixtures: workspace and serial simulator tests, strict Clippy, client generation/build, JavaScript fixtures, plot/score checks, guide and generated-file drift. The first check detected the expected grinder content changes in generated tiny-world fixtures; those were committed before the clean run.
- Five new simulator tests cover four/eight-neighbor wall detours, unreachable and claimed ore, planned/funded site choice, grinder relationships, and cold-checkpoint/evicting-cache equivalence. Existing factory-output clearance caught and prevented a work-field regression.
- Both focused desktop/narrow scenarios passed through a real native peripheral (`artifacts/ui/2026-09-13T22-09-42.536Z-925060`). Inputs cover fogged numeric ore, canvas ore color, hidden enemies, owned army/all-unit selection, type removal/retention, group recall filtering, and authoritative attack-order recipients after commit. Both traces passed ZIP integrity checks.
- The final full Chromium run passed **44/44** desktop/narrow scenarios with peripheral mode enabled (`artifacts/ui/2026-09-13T22-13-23.277Z-929525`), covering multiplayer, single-order/hybrid modes, restore/recovery, production, fog, progressive replay and game-end results. All 44 traces passed ZIP integrity checks. Full factory frames were also manually reviewed on both viewport sizes.
- Manually inspected full 1440×1000 and 390×844 frames for fogged ore readouts, all-type selection and army selection. Narrow panels scroll to expose longer selection/control lists. The initial scenario used an invalid seven-unit starting roster; the lobby rejected it visibly. The corrected valid three-slot custom roster is miner/factory/grunt; failure evidence is retained in `artifacts/ui/2026-09-13T22-06-03.391Z-920073`.

Rules stamp 4 prevents replaying older work-targeting rules as if they were identical. The separate 2,000-live-entity end-to-end measurement remains open. Missile silos are the next requested feature.


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

## Compact planning UI pass

The requested UI checklist is implemented: contextual command menus, a full-width timeline, one shared factory editor, accepted overrides for differing factory configurations, missing-order feedback, a committed/waiting banner, prominent selected-recipient ticks, and a direct generated unit-reference link. The guide and thematic user requirements are updated.

Local verification:

- `scripts/check.sh` completed successfully: formatting, workspace tests, Clippy, simulation tests without default features, client generation/typecheck/build, JavaScript checks, authored fixtures and generated-file drift. Later layout adjustments were rebuilt and typechecked by the browser harness.
- All 27 browser scenarios have passing coverage across the full run and focused corrections. `artifacts/ui/2026-10-19T01-12-01.903Z-46856` passed 22; its five failures exposed old menu-navigation test steps, a stale peripheral executable, a missing acceptance in the expanded factory scenario, and silo-panel overflow. Those failure artifacts are retained.
- `artifacts/ui/2026-10-19T01-21-14.841Z-53407` passed seven of eight focused scenarios, including active factory cancellation after publication, production priorities, peripheral previews, both opening/history slices, four-player layout and the new feedback checks. Its remaining silo overflow was corrected by removing the empty mode-options row and showing mode choices in place of production controls.
- `artifacts/ui/2026-10-19T01-24-00.699Z-55199` passed all three final factory/feedback/silo scenarios. This includes 1366×768 panel overflow assertions, accepted/dismissed mixed-factory overrides, aligned five-item additions, whole-edit undo, blueprint/site/live editing, committed/uncommitted banner lifetime, draft and published selection markers, guide access and completed missile scenarios.

Full rendered PNGs were manually inspected, including the final `silo-production-no-scroll-1366`, `factory-no-scroll-1366` and `shared-aligned-factory-editor` frames; the preceding run's `planning-menu-1366`, `hotkeys-no-scroll-1366`, `active-factories-overridden-together`, committed-waiting frame and four-player `group-single-delivery-and-timeline` frame were also inspected. The screenshots show accessible bottom controls, readable menus, isolated long command lists and unclipped timeline rows. The reviewed display sizes are 1920×1080 and 1366×768, device scale 1, Chromium; mobile and other browser engines were not reviewed.

The warning system describes projected orders at the viewed tick, not guaranteed future funding or reachability. Equivalent shared operations combine disjoint recipients into one command; a mixed-factory reset that needs several protocol commands is rejected atomically in single-order mode. It remains available in the default timestamp mode. No simulation/protocol behavior was changed.

After the user's resource request, running test threads and all subsequent runs were restricted to CPU cores 0–5 with scheduling priority 10. The browser retained two workers inside that cap. Test processes are stopped after verification; generated build phases were serialized. The pre-existing untracked `TMP_NOTES.md` was left untouched.

## Game coordinator, users and basic bots — 2026-10-24

The default server now coordinates persistent games with separate workers, casual passwords, unvalidated browser usernames, parallel player identities, spectators and recent read-only replays. Creation exposes common settings and the full setup editor. Three deterministic bot policies use visibility-filtered published states and the ordinary draft-validation/commit path. Bot-only playtests have a round budget. Standalone server/peripheral tooling remains available through `--single-match` or the existing inputs-only/resume/verify modes.

Verification:

- `scripts/check.sh` passed: workspace tests, strict Clippy, simulator no-default-features checks, client build/tests and generated fixtures. The intentional guide regeneration was supplied as the baseline through a temporary alternate Git index; the user's real index was untouched. Existing explicitly ignored expensive tests remain ignored.
- Six Chromium scenarios passed in `artifacts/ui/2026-10-24T22-55-21.054Z-80020`: all three new coordinator scenarios, existing same-browser identity/uncommit coverage and both rematch regressions. Following final game-title and read-only-entry presentation changes, client build/type checking and all three coordinator scenarios passed again in `artifacts/ui/2026-10-24T22-57-56.634Z-80878`.
- Coordinator coverage includes arbitrary long markup-looking names rendered safely, password rejection/admission, spectator defaults, two parallel games, directory filters, saved rejoin across tabs, active/open worker restart and map retention, normal human/bot turns, bot-only archiving, read-only token masking, and four-bot teams with single-order/hybrid-history constraints. Rust policy tests cover valid deterministic openings, order limits, hidden-enemy independence and visible-enemy order privacy.
- Manually inspected rendered screenshots for first-visit username entry, creation at 1920×1080 and 1366×768, directory cards, shared maps, live spectating, resumed play, four-bot teams and read-only replay entry/playback. Inspected failure traces/screenshots from the first run; corrected an initial-handler click race and hidden-field layout issue. Failure artifacts remain retained.
- All three archives from the final coordinator run reproduced every recorded round hash through the release server's `--verify` path (11 recorded rounds total). Results are in that artifact directory's `replay-verification.log`.
- Builds/tests used two CPU cores, low scheduling priority and at most two Rust build/test workers; browser scenarios used one worker. Heavy checks ran sequentially.

Limits: browser verification used desktop Chromium on localhost, not Firefox/WebKit or WAN/high-load deployment. This feature pass did not repeat the entire native-peripheral browser matrix or the separate outstanding 2,000-live-entity measurement. Open-lobby snapshots run every 750ms, so an abrupt crash can lose a very recent roster/map edit; accepted game turns retain existing archive durability. Usernames and lobby passwords deliberately provide casual identity/access rather than secure accounts.

## Lobby interaction and tick-scoped warnings — 2026-10-24

Game lobbies inherit the directory username without a second visible input, including intentionally empty names. Open slot bars support direct clicks and keyboard activation; release appears within the occupied local slot. Standalone lobbies retain their username field. “Needs orders” appears only for a nonempty current draft while viewing its intervention tick.

Client build and TypeScript checks passed. All six coordinator and compact-planning Chromium scenarios passed with two-core affinity, low priority and one worker in `artifacts/ui/2026-10-25T03-27-20.409Z-108279`. Scenarios exercise inherited names, bar claims, release/reclaim, existing coordinator recovery/replays, no initial warning, warning at the draft tick, hiding on another tick and after publication, and ordinary construction/production feedback. Manually inspected the inherited-name lobby with its inline release button, warning-free initial planning at 1920×1080, and active-draft warnings at 1366×768. No Rust changes were needed; the full repository/native-peripheral/browser matrix was not repeated for this client-only correction.

## Switch/rejoin pointer input and mixed production — 2026-10-25

The Switch / rejoin player button now explicitly accepts pointer events instead of inheriting the noninteractive HUD style. Factory warning checks no longer flag a mining or construction newborn order merely because some queued unit types cannot use it. Empty queues and missing newborn/work/construction orders retain their existing warnings.

Client build/type checks and three real-server Chromium scenarios passed in `artifacts/ui/2026-10-25T04-43-03.035Z-557321`: coordinator switch/rejoin and recovery, standalone switching to another saved player, and mixed miner/grunt production with mining orders. The scenarios click the actual switch control rather than opening another tab to bypass it, and verify the warning disappears after assigning the remaining work orders. Manually inspected the coordinator chooser and the mixed-queue factory with no warning at 1366×768. Checks used two CPU cores, low priority and one browser worker. This focused client-only pass did not repeat the complete Rust or native-peripheral suites.

## Two heuristic opponents — 2026-10-26

Raider and Builder replace the three narrow policies. Both grow mining economies, replenish workers, expand factories, produce every existing mobile unit type, construct turrets, explore using observed-space memory, and react to visible threats. Coarse loss-based rewinds and horizon revisits use the published timeline; unit orders replace their own later orders. Independent command validation preserves useful orders when a placement fails. Single-order priorities reserve periodic opportunities for army movement and scouting. Legacy `rusher` configurations load as Raider. Walls and silos are intentionally outside these policies.

`scripts/check.sh` passed with all 36 server tests, including 12 bot tests, workspace tests, strict Clippy, simulator no-default-features tests, client build/tests and generated-file consistency. The intended guide regeneration was staged before the generated-file drift check. Existing explicitly ignored expensive tests remain ignored. Log: `artifacts/bots/2026-10-26/check.log`.

The bot tests include three 40-step generated-map matchups (Raider mirror, Builder mirror and mixed), a single-order matchup, and 24 rounds through the actual controller with published revisions, validated commits and loss-triggered rewinds. Continuous simulation scenarios require both sides to grow at least four miners, two factories and six combat/scout units and deal damage. Timestamp scenarios additionally require every supported unit/building type to appear for both players. The actual-controller scenario verifies economic growth, combat by both sides and an earlier intervention. Separate regressions cover constructor mining after miner loss, exploration memory, loss rewinds bounded by locked history, partial validation failure, legacy configuration and hidden-state isolation.

All four coordinator browser scenarios passed; inspected creation/lobby screenshots at both desktop sizes. See [browser testing](browser-testing.md#two-capable-bot-choices) for artifacts. All heavy work ran sequentially with two-core affinity and low priority; Rust tests and browser scenarios used one worker.

A separate release-server run exercised all three pairings with the normal five-point scoreboard, 48×48 generated maps, a 3,000-tick horizon and a 32-round budget. Archives, runner source, server logs and a machine-readable summary are retained in `artifacts/bots/2026-10-26/matchups/`:

| Pairing | Seed | Result | Peak armed units by side | Peak miners by side | Peak factories by side |
| --- | --- | --- | --- | --- | --- |
| Raider / Raider | 43 | Budget archive after 32 rounds; first timeline wins at rounds 31–32 | 223 / 76 | 23 / 21 | 6 / 6 |
| Builder / Builder | 44 | Budget archive after 32 rounds; win, stalemate and draw timelines appeared | 112 / 101 | 22 / 32 | 7 / 6 |
| Raider / Builder | 42 | Builder reached five points on round 22 | 62 / 123 | 16 / 20 | 5 / 6 |

These peaks are measured across the final published timeline, and armed counts include scouts. Every supported unit/building type appeared for both sides in every pairing. In the mixed match, winning timelines reverted to stalemates at rounds 15 and 19 before the eventual win. Planning targets are not hard population caps: rewrites can preserve previously queued production and later construction, producing counts above the desired workforce/factory targets.

These are bounded heuristic checks rather than proof of balanced or consistently entertaining play across all seeds. The full native-peripheral/browser matrix, WAN performance and tournament-strength evaluation were not repeated. The policies do not perform tactical search, simulate alternative plans or inspect pending human orders.

## AI opening and postgame diagnostics — 2026-10-27

Inspected the two most recently created local games, `game-1793152676456-3` (Raider, match `1793152676500`) and `game-1793154125641-4` (Builder, match `1793154125690`). Evidence comes from accepted turns, authoritative round outcomes and saved checkpoints, not visual impressions or worker-log timing alone.

Both bots placed a factory on their first turn without configuring production. Their first published timelines spent **zero matter on units** and retained only one miner. On round 2, Raider queued seven miners and one scout; Builder queued eight miners. Both factories gave newborn miners attack-move orders. Revision 2's checkpoints showed seven and eight non-mining newborn miners respectively. Raider's round 3 intervention jumped to tick 470; rounds 4 and 5 submitted empty drafts, and revisions 3–6 ended at tick 687 with no Raider units. Builder's round 3 intervention was tick 270, after substantial early workforce losses. These are archived failures, not evidence of a transport or commit crash.

Factory blueprints now carry first-turn production settings. The opening queues three miners, a constructor and another miner with mining inheritance. Later worker and combat batches receive suitable stored orders; single-order mode sets the needed order before enqueueing, without allowing ordinary rally updates to starve production. Rejected placements discard their dependent configuration while preserving independent legal commands. Intervention timing advances from the most recent turn, checks terminal losses on every round, selects the earliest substantial loss, and avoids jumping over early economic work toward a distant future collapse.

The opt-in archived-opening scenario ran against each original map and pinned content, keeping the human's accepted first turn unchanged. Both revised openings had **four miners mining at tick 100 and five at ticks 200 and 300**. At tick 300 the revised Raider had mined 2,258 matter and Builder 1,133. On the original revision 2 timelines, the revised round 3 intervention moves from tick 470 to 200 for Raider and from tick 270 to 170 for Builder. This comparison covers the opening and intervention selection only; it does not establish that either revised bot wins the original full match.

Verification: all 37 server tests passed (one archive-dependent test intentionally ignored by the ordinary suite), including mixed-army self-play, both personalities, single-order growth, controller-driven published timelines, visibility restrictions, legal-command retention, and opening production without another round. The archive-dependent scenario was run explicitly for both reported games. The Python analyzer's two tests passed, covering streaming chunk boundaries, malformed input, revision metrics, nearest-checkpoint labelling, skipped commands and pending drafts. Server all-target Clippy passed with warnings denied. Builds/tests ran sequentially at low priority on two cores. No browser UI changed; no visual review was performed or claimed.

Reusable commands and interpretation limits are in [postgame analysis](postgame-analysis.md). Local generated reports are `target/latest-ai-analysis.json`, `target/raider-opening-review.json` and `target/builder-opening-review.json`. Original archives and live workers were not modified; the running server must be restarted with a rebuilt binary to use the new bot policy.

## Sustained scout pressure and complete-match bot regression — 2026-10-27

Investigated `game-1793156527569-6`, match `match-1793156527621`, against Builder. The original match ended 5–0 for the human. The frozen-opponent harness reproduced all six original timeline outcomes and terminal ticks with the previous policy before tuning.

The original first timeline's bot mined 37,714 matter but spent only 160 on units and banked 37,554. Later, after the human enabled the economic factory's mining inheritance, sustained scout/grunt production destroyed the bot's sparse defenses. In the original final revision the bot still had about 11,404 banked while having spent only 2,020 on units. These observations identify severe spending, production and army-control failures; they do not establish that scouts are balanced in every matchup. No unit costs, stats, simulation rules or starting resources were changed.

The revised policy plans low-priority military follow-up alongside the miner opening, scales factory capacity with the workforce, repeats combat items explicitly, screens heavy units with inexpensive grunts, places defensive turrets near its base, and distinguishes reinforcement destinations from immediate defensive targets. Exploration considers where friendly units already are, and interventions advance to substantial forces that have finished their previous advance. After follow-up development, an already winning timeline is preserved by passing. Single-order planning activates existing production before further expansion and enables repeat before enqueueing combat items.

The saved fixture retains the original terrain, ore, starting state, content, match rules and all six human turns with their exact timestamps, command IDs and replacement policies. Fresh bot turns use the preceding published revision, visibility filtering and ordinary command validation; the current human turn enters the simulator only afterward. Human play becomes passes after its recorded sequence. The original five-point objective remains in force, and the regression fails if the human wins or if the bot fails to win within 24 rounds. This is a fixed-transcript opponent, not a model of how the human would respond to changed bot behavior.

The revised Builder wins **5–1 on round 8**. Round 1 favors the bot, round 2 favors the human, rounds 3–4 stalemate, and rounds 5–8 favor the bot. The last two human inputs are additional passes after the six saved turns. On the winning round-5 timeline the bot reaches 17 miners and six factories, with sampled peaks of 154 grunts, 47 tanks, 23 artillery, 21 grinders and 16 scouts; it spends roughly 16,003 matter on units. The detailed local report is `target/sustained-final.json`; [postgame analysis](postgame-analysis.md#full-recorded-opponent-match) documents reproduction.

No browser UI changed, so no visual review was performed or claimed. Original game archives and running workers remain unchanged; rebuild and restart the server to load the revised policy.

Final checks: 38 server library tests passed in release mode, including the full recorded match, controller self-play, both personalities, single-order growth, ownership/visibility restrictions and restart/replay behavior; one local-archive-dependent test is intentionally ignored in that ordinary suite. It was then run explicitly against both older reported archives. The two analyzer tests and all-target server Clippy with warnings denied passed. The older Builder opening now has three mining miners at tick 100 and five by tick 200 while funding the second factory; its previous four-at-100 benchmark described the earlier one-factory policy. The updated opening regression checks immediate mining, growth to five by tick 200, and no non-mining surviving miners. Heavy checks were sequential and limited to two cores at low priority.
