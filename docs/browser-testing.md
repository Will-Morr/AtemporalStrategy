# Browser review for any agent

Both layers are project-owned and agent-neutral. The repeatable harness is ordinary Playwright Test; interactive control is a standard stdio Playwright MCP server. Neither requires Codex, an OpenAI API, a model-specific SDK, or a proprietary browser tool. Use the same commands from a terminal, CI, another coding agent, or an MCP-capable client.

## Install and run repeatable review

From the repository root:

```sh
npm ci --prefix client
npm run ui:install --prefix client
cargo build --release -p atemporal-server -p atemporal-runner
npm run build --prefix client
ATEMPORAL_UI_SERVER_COMMAND='target/release/atemporal-server --single-match --seed 42 --replays target/ui-replays' npm run ui:review --prefix client
```

Chromium is pinned by Playwright 1.63.0. On a Linux machine missing browser libraries, install Playwright's documented OS dependencies with `cd client && npx playwright install-deps chromium`; this can require administrator access. The current development host already runs the browser successfully.

Every default run allocates a local port, builds the browser assets and starts the real game server (which generates its guide from effective content), runs desktop (1920×1080) cases, and closes its server/browser. It refuses to reuse an unrelated server on its selected port. Separate worktrees and processes get separate artifact directories and browser profiles. A port collision after allocation is reported as a failed start rather than attaching to another agent's app. Use an explicit port when needed:

```sh
ATEMPORAL_UI_PORT=8090 npm run ui:review --prefix client
ATEMPORAL_UI_BASE_URL=http://127.0.0.1:8080 npm run ui:review --prefix client
```

The second command reviews an already running real game server, without launching/rebuilding or stopping it. `ATEMPORAL_UI_SERVER_COMMAND` can replace the default game-server command; it runs from the repository root and receives the selected `PORT`. The gameplay suite requires the release server and built browser assets; a custom server command skips the default build, so build the client explicitly after changing it. Most scenarios start isolated matches; the opening slice uses this base server. Use a fresh replay directory for each base-server run.

Playwright flags pass through normally:

```sh
npm run ui:review --prefix client -- --project=desktop-chromium
npm run ui:review --prefix client -- --grep "keyboard"
npm run ui:review --prefix client -- --headed
```

A headed run requires a display. Headless Chromium renders the actual page, executes JavaScript and handles canvas; it is not a DOM-only emulator. Adding Firefox/WebKit projects is optional and requires installing those browsers.

Mobile support is outside the required scope. The browser matrix uses a normal 1080p desktop viewport at device scale 1; older narrow-screen runs in the integration log are historical evidence.

## Review artifacts and iteration

The command prints an `artifacts/ui/<run-id>/` path. Override it with `ATEMPORAL_UI_ARTIFACTS` if your agent runner needs a known output location. Use a new or empty override directory; existing evidence is never overwritten. Each run contains:

- `run.json`: run ID, server URL, arguments, timestamps, server command, exit status and startup errors.
- `runner.log`: combined runner/build/server output, including failures before tests start.
- `results.json`, `junit.xml` and `report/index.html`: machine-readable and human-readable results (available once Playwright starts).
- `results/<test>/`: named PNG screenshots, accessibility snapshots, browser console/network errors, and `trace.zip`. Videos are retained on failure, including fixture-owned player/spectator contexts and pages that closed during the test. Accessibility snapshots and diagnostics are attached to the HTML report. Failure captures cover every open observed page before owned contexts close.

Open reports and traces locally:

```sh
cd client
npx playwright show-report ../artifacts/ui/<run-id>/report
npx playwright show-trace ../artifacts/ui/<run-id>/results/<test>/trace.zip
```

An agent should run a relevant scenario, **inspect the resulting PNGs**, diagnose visual/interaction problems, edit the UI, and repeat the same scenario. A passing selector assertion is not a visual review. Preserve failure evidence; tests do not retry automatically. The automatic review fixture captures observed contexts' errors even when a test does not explicitly request `review`, and fails on page errors, failed requests, HTTP responses with status 400 or higher, or console errors. Screenshot names describe the actual inspected state.

The identity test verifies independent storage against isolated real lobbies. The slice, playtest and variant scenarios additionally claim slots, commit, build, fight and restore archived matches. New gameplay tests should use actual user inputs and server setup with a deterministic seed. Capture a known paused timeline tick before comparing images; never let changing simulation/playback time stand in for a visual regression. Prefer role/text locators for UI controls and coordinate input plus screenshots for canvas. `review.capture(name, page)` supports both. Use `await review.newContext("player-a")` for each additional identity; the fixture observes new pages/popups, saves failure evidence and closes these contexts. Do not close them early. Playwright includes these contexts in the test’s `trace.zip`; do not manually start/stop their tracing. External contexts must be explicitly observed and remain the caller’s cleanup responsibility. Pixel baselines can use Playwright's `toHaveScreenshot` once a meaningful scene exists; inspect baseline changes rather than blindly refreshing them.

Artifacts are git-ignored and retained per run for inspection. They are not game archives. Do not run concurrent build-backed reviews in the same worktree: their ports/profiles differ, but the build writes the same `client/dist` and generated contracts. Use separate worktrees or build once and target an external server.

## Local harness verification

Run `scripts/check.sh` locally for formatting, Rust tests, strict Clippy, browser build, shared fixtures and generated-file drift. Run the browser and MCP commands above for rendered review and transport verification. These commands do not install hosted jobs or run automatically when the branch is pushed.

The MCP artifact directory includes session output, protocol transcript, browser console/network logs and run/error metadata. Startup failures cannot produce a screenshot of a page that never loaded.

Run the failure-path check independently:

```sh
node scripts/check-ui-harness.mjs
```

Its five intentionally failing cases are excluded from normal review. The outer command succeeds only when assertions, HTTP errors, page-script errors, failed requests and console errors produce failures with diagnostic/report/trace/screenshot evidence, including all three identity videos and a video from a page closed before failure. Cleanup failures are reported while the remaining diagnostics are still collected. These injected errors validate automation plumbing; they do not validate gameplay or application error recovery.

## Gameplay scenario activation

Add real scenarios to `client/tests/ui/*.spec.mjs` as their controls land; normal review discovers them without a package change. Configure `ATEMPORAL_UI_SERVER_COMMAND` to launch the implemented game server with deterministic setup and the supplied `PORT`, or use `ATEMPORAL_UI_BASE_URL` for an existing instance. Each independently running match needs isolated server state; mark a shared-match suite serial or give it its own server. An absent required control should fail an enabled scenario, rather than silently skip it or fall back to fixture JSON.

| Scenario | Evidence required before claiming coverage | Current state |
| --- | --- | --- |
| Landing and guide | Keyboard link activation, generated stats, desktop layout, screenshot inspection | Real lobby and generated guide |
| Independent browser identities | Separate storage across reloads and context evidence | Real lobby storage isolation; gameplay scenarios claim slots |
| Lobby and spectator | Claim actual slots, update username/color/team and observe all clients; spectator restrictions | Real 2–4-player lobbies, profiles and team labels |
| Timeline and drafts | Pause at exact tick, enter actual orders, undo/replace future orders, capture selection and timeline | Slice and playtest scenarios |
| Simultaneous commit and uncommit | First commit waits; withdrawal restores editable moves; final commit publishes the same revision to all clients | Slice, shared-browser refresh, three-player partial-round restart, and objective/control variants |
| Groups and production | Keyboard groups, factory output membership and inherited order visible in replay | Slice, configurable ghosts and single-order variants |
| Restore | Restart real server at same port, refresh clients and compare persisted revision/orders | Same-port resume, partial-round resume and forced cache regeneration |

For each activation, document the seed/config, viewport, revision and paused tick, inputs exercised, assertions and inspected screenshots. Verify authoritative behavior as well as visible output. The rendered guide describes planned mechanics; its text alone is not evidence those mechanics work. No human playtest is required for unfinished slices.

## Interactive browser tools through MCP

`.mcp.json` configures `atemporal-browser` for clients that discover this conventional project file. It invokes the pinned local package through `node scripts/browser-mcp.mjs`. Start a dev server first and tell the agent its exact URL. Clients that do not read `.mcp.json` can register the same command with an absolute script path:

```json
{
  "mcpServers": {
    "atemporal-browser": {
      "command": "node",
      "args": ["/absolute/path/to/AtemporalStrategy/scripts/browser-mcp.mjs"],
      "env": { "ATEMPORAL_BROWSER_NO_SANDBOX": "1" }
    }
  }
}
```

The launcher finds the repository independently of the caller's directory. It uses an isolated in-memory browser profile per process, the installed Chromium executable, screenshots/session output in a unique `artifacts/browser-mcp/` directory, and the `vision` capability for canvas coordinate clicks/drags. For explicitly named screenshots, pass an absolute path inside that artifact directory; this pinned MCP version resolves relative filenames from the workspace root. It preserves stdout for MCP JSON-RPC; diagnostics go to stderr. It uses stdio rather than exposing a browser-debugging TCP port. Start separate MCP processes for independent agents; do not share a single exploratory browser between concurrent tasks. The repeatable harness provides separate contexts for multiple player identities.

This Linux host blocks Chromium's namespace sandbox, so the checked-in **local test browser** configuration explicitly sets `ATEMPORAL_BROWSER_NO_SANDBOX=1`, matching the test harness's launch environment. It affects only the disposable automation browser, not a personal browser or OS setting. The launcher itself leaves the sandbox enabled when that variable is omitted; omit it on hosts that support sandboxed Chromium. Use the configured instance for the local game under test.

`ATEMPORAL_BROWSER_HEADED=1` shows the browser when a display exists. `ATEMPORAL_MCP_ARTIFACTS` selects a known output directory. Extra launcher arguments pass to Playwright MCP. Client-specific registration syntax is optional; no global agent configuration is modified by this repository. For example, a Codex CLI client can register the absolute launcher path using `codex mcp add`, while other clients use their equivalent MCP settings.

Validate the actual project MCP configuration independently of any agent product:

```sh
npm run browser:check --prefix client
```

The smoke client speaks JSON-RPC directly: initializes MCP, discovers browser and coordinate tools, navigates the actual page, clicks the guide link, reads its snapshot and saves a screenshot. It closes its browser and game server afterward. The smoke check waits for its own game process to announce readiness, preserves protocol requests/responses and stderr on failure, and attempts a failure screenshot before cleanup. The explicit artifact override must be empty. For an already-built external server, use `ATEMPORAL_UI_BASE_URL=... node scripts/check-browser-mcp.mjs` to avoid the npm script’s build step. Environment variables explicitly set by the caller take precedence over `.mcp.json` defaults. A new/reloaded agent session may be necessary for its client to discover newly configured tools; adding a server does not retrofit this chat's advertised tool list.

The desktop app's built-in `@Browser` remains an optional separate connection. Neither layer depends on it. See [Playwright web-server setup](https://playwright.dev/docs/test-webserver), [traces](https://playwright.dev/docs/trace-viewer-intro), [visual comparisons](https://playwright.dev/docs/test-snapshots), and [Playwright MCP](https://github.com/microsoft/playwright-mcp) for upstream behavior.

## Final browser handoff

See [integration verification](integration-verification.md) for the tested variants, performance workload, manually inspected full-frame evidence and actual limits. `playtest.spec.mjs`, `variants.spec.mjs` and the real-lobby guide tests supplement `slice.spec.mjs`; the gameplay cases exercise the desktop command deck. Historical round metadata loads without eagerly regenerating old replay bodies; selecting a round or opening its detailed comparison loads those data on demand.

The playtest-polish scenarios additionally cover shared-browser player identities, explicit rejoin, uncommit with restored undo/redo, factory loop and queue editing during construction, hidden enemy blueprints at visible tiles, hybrid history advancement/recovery, and exact per-player latest-write markers. See the current pass in [integration verification](integration-verification.md); retain earlier failed runs as diagnosis evidence.


`progressive.spec.mjs` checks direct-controller and input-only-peripheral previews in the desktop viewport: exact non-sample seeking, typing while progress advances, playback at the frontier, refresh, disabled planning and equality with the final replay. Its isolated processes use `ATEMPORAL_PREVIEW_TEST_DELAY_MS` to keep the long tail observable on fast machines; the production default adds no delay. The project timeout also gives built-in trace finalization up to 120 seconds; the review fixture has its own cleanup budget. Revision helpers wait for publication, so existing scenarios cannot accidentally count a provisional state as a verified result.


`feedback.spec.mjs` covers factory/turret blueprint deletion, undo/redo and removal of dependent settings, shared/mixed priority and group feedback, rebasing before a blueprint existed without submitting an invalid commit, cancellation of an active turret construction site, exhausted ore and smoothed mining-rate plots. It uses the peripheral when `ATEMPORAL_UI_PERIPHERAL=1`; direct and replicated verification evidence is recorded in the integration log. The pure slope checks run with `npm test --prefix client` and cover irregular spacing, negative/zero rates, missing samples and singleton windows.

## Production, repair and scoreboard scenarios

`production-priority.spec.mjs` exercises mixed priority and Off, per-item loop flags, Shift-click batches, blueprint/site/live queues, local queue references, and spending pause/resume through actual commits. `scoreboard.spec.mjs` drives a real three-win match, checks leader/winner totals at past ticks and rounds, and refreshes between wins. Both run through authoritative or input-only transport and desktop viewports. `npm test` also checks score aggregation across revision ancestry, teams, draw credits, adjusted leadership and incomplete history. Full-frame screenshots still require manual inspection before claiming visual readiness.

`factory-queue-delete.spec.mjs` uses the authoritative server to check keyboard removal in one factory, shared pointer removal in two factories, undo and redo across reload, and the remaining production items after a commit. The storage scenarios fill Chromium's actual session quota. One verifies that a current draft saved without history restores on reload. The other verifies that in-tab edits and undo still work when neither write fits, that the warning stays separate from the orders banner, and that persistence resumes after storage is freed.

With Chromium 153.0.8010.12 at 1920 × 1080, a baseline delete at the storage limit threw `QuotaExceededError` before the queue could redraw. The in-memory count fell from five to four while the UI still showed five. The table shows one instrumented run of 30 removals per selection type. Panel time is accumulated inside `updatePanels` and excludes input dispatch and frame presentation.

| Selection | Draft writes before → after | Serialized characters before → after | Panel time before → after |
| --- | ---: | ---: | ---: |
| One planned factory | 30 → 30 | 4,985,992 → 391,498 | 362.6 → 178.3 ms |
| Two planned factories | 90 → 30 | 85,236,006 → 1,002,867 | 3,792.5 → 257.3 ms |

The reports, traces and full frames are under `artifacts/ui/factory-delete-baseline`, `artifacts/ui/factory-delete-fixed`, `artifacts/ui/factory-delete-final` and `artifacts/ui/factory-delete-toast`.

## Missile silo scenarios

`missiles.spec.mjs` builds and configures a silo through real inputs, checks all three production recipes, manual launch queues and cancellation, range/impact previews, satellite flight and destination vision, blast results, stockpiling, refresh and replay rewrites. A second scenario supplies automatic targeting with an allied spotter, then rewrites the automatic plan into stored ammunition. Both run through the controller or native peripheral on desktop Chromium. The tests compare preview flight times with resolved arrivals and play a one-tick satellite flight between compact replay samples. Capture and inspect the full placement, stockpile, targeting and flight frames; a passing control assertion alone does not establish visual quality.

## Map choice, rematches and factory displacement

`rematch.spec.mjs` checks that both players and an observer see the same five maps, that only the controlling player can choose, and that the selected preview matches the initial terrain and ore. It ends a real match, reviews/reloads the completion menu, opens a fresh lobby while another player is playing the old replay, keeps slot identities, selects another map and commits in the reset match. A second scenario parks a miner on a factory output and verifies displacement and the completed birth. The scenarios run through authoritative or native-peripheral transport and capture full 1080p frames.


## Compact planning review

`declutter.spec.mjs` covers grouped factory overrides (inline warnings without browser dialogs, mouse and keyboard, whole-edit undo, aligned additions and active production cancellation), missing-order feedback, selection markers, unit-reference access, and committed/uncommitted banners. It checks normal panels for overflow and captures 1920×1080 and 1366×768 frames. `missiles.spec.mjs` also checks the expanded silo editor at 1366×768. Secondary actions are reached through Groups and Planning menus; only command/production/launch lists scroll. The default browser harness builds both server and peripheral binaries to avoid testing a stale replica against current content.

On the current eight-core workstation, keep test workloads on six cores at low priority so the user can work concurrently: prefix commands with `taskset -c 0-5 nice -n 10 env CARGO_BUILD_JOBS=6 RUST_TEST_THREADS=6`. Browser scenarios use two workers within that shared CPU cap. Run the repository check and browser build phases sequentially because both write generated/client assets.

### Timeline controls beside the canvas

The timeline strip keeps its existing place above the three workspace sections, with transport controls on the right. Its two-player height is 60px (previously 82px); additional player lanes can increase the height. Verified with the real-server `order warnings, committed banner` scenario on 2026-10-18, including screenshot inspection at 1920×1080 and 1366×768, no-panel-overflow assertions, seeking, selection markers, reference access and commit/uncommit. Evidence: `artifacts/ui/2026-10-19T02-41-25.274Z-70270/`. Client generation and TypeScript checks passed during the browser build. This focused run did not recheck four-player layouts or the complete browser suite. Workload was capped to six cores at low priority.

### Global structure planning and compatible actions

On 2026-10-18, all three `declutter.spec.mjs` scenarios passed through the real server with six-core affinity and two browser workers. The new scenario places a turret without selecting a constructor, verifies unsupported mouse/keyboard orders stay unavailable for the turret and miner, and separately assigns constructor work. Shared factory edits verify inline warning/no browser dialog and undo. Inspected the full-frame turret-plan and mixed-factory-warning screenshots in `artifacts/ui/2026-10-19T02-59-55.134Z-72504/`; the existing feedback scenario also covers both desktop sizes. Client generation and TypeScript checks passed. The complete browser suite was not rerun.

### Content-sized workspace

The three workspace sections now size to content with a 126px minimum, rather than reserving 230px. Unit details open in an Info popover; health and orders remain in the selection summary. The map canvas and warning/minimap offsets follow actual panel height, preserving world positions on screen when the workspace resizes. Normal constructor selections use a 212px total bottom deck (previously 316px after the timeline-control change).

Five real-server scenarios passed in `artifacts/ui/2026-10-19T03-02-37.389Z-74535/`: shared factories, order feedback, global placement/action compatibility, map/production playtest, and silo production/launch/rewrite. The initial run exposed map-coordinate shifts during panel resize; these were fixed before the passing run. A final selection-packing change passed the feedback scenario again in `artifacts/ui/2026-10-19T03-03-44.921Z-75359/`, with a deck-height assertion and opening/closing unit information at both desktop sizes. Inspected full-frame constructor, unit-info, shared-factory and expanded silo screenshots, including 1366×768 and 1920×1080 constructor frames. Builds include TypeScript checks. All runs used six-core affinity and at most two browser workers. Four-player layouts and the full repository suite were not rerun for this client-only change.

## Coordinator, identities and basic bots

`coordinator.spec.mjs` launches isolated real coordinators, each with its own directory and worker processes. It covers first-visit unvalidated usernames, password entry, bot selection, shared map choice, two parallel games and per-game identities, open/active/My games filters, spectator access, same-port coordinator restart, bot-only budgets, recent read-only replays, and four-player bot teams with hybrid history and single-order limits. Standalone gameplay helpers now explicitly pass `--single-match`; this preserves their existing contract after the default server became a directory.

For the user's current busy-device constraint, run heavy checks sequentially with **two cores**, two Rust test/build threads, low priority, and **one browser worker**. This supersedes the earlier six-core recommendation while that constraint is in effect:

```sh
taskset -c 0-1 nice -n 15 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 scripts/check.sh
taskset -c 0-1 nice -n 15 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 npm run ui:review --prefix client -- coordinator.spec.mjs --workers=1
```

Coordinator scenarios own and close their workers. To reuse an already-built binary without the harness's default build/server phase, set `ATEMPORAL_UI_BASE_URL=http://127.0.0.1:1`; these scenarios navigate only to their own isolated servers. Do not use this shortcut for scenarios that rely on the shared base URL. Review the full-frame directory, creation dialog, human/bot lobby, live spectator and archived replay screenshots before declaring visual verification.

### Simple win game type

The focused `simple win game type can be selected and started` coordinator scenario passed on 2026-10-25. It selects Simple win through the creation form, creates a real human/bot lobby and starts the match with the authoritative `simple_win` configuration. Inspected the full-frame creation and lobby screenshots in `artifacts/ui/2026-10-25T21-47-31.729Z-11280/`; labels, rule summary and controls are readable. The build includes schema generation and TypeScript checks. Exact destruction-tick termination, continued building survival without constructors, unfinished sites and simultaneous losses are covered by Rust simulation tests; the browser scenario checks selection/startup, not a complete combat match. Used two CPU cores, low priority and one browser worker.

### Small-host lifecycle

`coordinator.spec.mjs` also covers Show inactive games, an update restarting the server despite a live player (whose tab reloads onto the new server), same-address recovery, and 24-hour expiry blocking direct access to an abandoned incomplete game across restart. The expiry test ages the persisted activity timestamp. The replay scenarios cover the separate Replays tab and read-only playback. See the [2026-11-10 verification record](integration-verification.md#small-host-updates-and-game-lifetime--2026-11-10) for screenshots and actual test limits.

### Home page and quick play

`landing.spec.mjs` covers the first-visit landing page, one-click default-bot startup, real commits and refresh, lobby/guide navigation, existing names, retry after a failed creation response and generated stats from modified server content. It captures desktop, laptop and narrow landing layouts. Coordinator scenarios now open `/lobbies/` to test the directory; standalone workers still open directly into their game lobby. The guide assertions use the Game Docs heading. See the [verification record](integration-verification.md#home-page-quick-play-and-explainer--2026-11-10) for results and screenshot review.

The info-page checks also verify the exact chatbot warning at the top, 14 shared unit-role and stats icons, expanded static SVG markup, and no horizontal page overflow at 390px. The quick-play scenario selects all starting units to exercise the same icons in game controls. See the [shared-icon verification record](integration-verification.md#info-page-warning-and-shared-icons--2026-11-10).

The Game Docs review also checks grouped role/stat cards, links from named units to their generated stats, the current warning, five main actions and positional construction explanation. Stats are checked against the real server’s content (including a changed miner health value). Inspect the top, construction, group-control and stat-card viewport captures as well as the full-page screenshots.

## Order path previews

`path-preview.spec.mjs` uses real mouse/keyboard orders to show actual movement routes for selected miners and constructors, aiming and saved orders, persisted straight-line settings, rotated factory outputs, tank-only/mixed draft queues, and economic work areas. `scripts/check-path-preview.mjs` checks route lengths for both movement rules, shared field caching, blocked targets, work adjacency, unreachable regions and forbidden diagonal corner cutting, retained mining reservations, distinct claims, alternating tread steps and inset target-marker geometry. Mining scenarios compare rendered goals with authoritative retained destinations. Tread previews show no walker alternatives. The preview intentionally excludes hidden obstacles and future traffic/combat decisions; those are not execution guarantees.

## Artillery visual review

`artillery.spec.mjs` constructs a factory, produces artillery and a tank through real orders, seeks a paused tick, and captures their battlefield appearance at 18/26/40 pixels per tile and 1366×768. It also captures queue icons and the generated guide, including a viewport screenshot of the unit cards. Recenter after changing camera scale so screenshots actually include both units.

## DEBUG_TEST handoff

`debug-test.spec.mjs` checks that normal coordinators hide the button and reject debug creation, while `--test-server` enables a one-click bot opening. It verifies both players' real opening commands, human ownership and an editable developed state at turn 2, refresh/restart persistence, and continued human-versus-bot play. Capture and inspect the homescreen button and turn-2 handoff frames.

## Fog memory, launch supply and timeline planning

On November 18, 2026, five isolated real-server Chromium scenarios passed across the initial run and a focused retest. The two scenarios in `planning-release.spec.mjs` cover an expired satellite’s remembered building after a seek and refresh, matching-unit selection, the Game info link, atomic supply for a batch of five launches in single-order mode, the opponent-waiting banner, and staging, undoing and committing a timeline reset. Both existing missile scenarios passed, as did the three-win scoreboard scenario.

The initial run exposed a hidden Game info menu. The focused retest followed the repaired link through its popup and inspected the exact patch entry. Full rendered frames were inspected for grayed buildings, opponent waiting, the staged reset and the patch note. Artifacts are under `artifacts/ui/orders-release-initial` and `artifacts/ui/orders-release-verified`. The initial 1366px silo screenshot also caught a cleared canvas during resize. The resize handler now repaints immediately; that change passed TypeScript checks after this browser run.

## Timeline inspection and practice replays

The desktop Chromium release pass used seed 42 at 1920 × 1080. The two `timeline-review.spec.mjs` scenarios passed on the authoritative server and on the native inputs-only peripheral. In the first scenario a constructor's recorded attack led to elimination at tick 123. The inspector showed its command at tick 0, focused and outlined the recipient, and reported attack, impact, destruction and survival events at tick 123. The player opened the earlier timeline, returned to the lost timeline, then submitted an idle order in practice. The alternate timeline ended in stalemate, while exiting practice returned to the recorded winner. The second scenario inspected another player's completed factory and its active and pending grunts at tick 153, started practice while a player was disconnected, then submitted two practice ticks from the manually archived game.

The coordinator recent-replay scenario also passed. It opened an archived bot match through the directory relay, submitted an alternate tick and compared the original round file byte-for-byte before reopening it. The first run caught a scoring error when branching from later rounds. The corrected scorer starts the private score at one while retaining unique historical command IDs.

Inspected full frames and traces are under `artifacts/ui/timeline-review-2`, `artifacts/ui/timeline-review-peripheral`, and `artifacts/ui/timeline-review-event`. Frames included the focused gold recipient outline, the tick-123 event counts, the enemy factory queue and the alternate outcome. A separate compatibility check reproduced the exact public rules 7 round-9 terminal state at tick 1,104 from its initial snapshot, pinned content and submitted turns. Its canonical SHA-256 was `b52757065aebfeb378aae262e35c15adf5fed2c85319434280acc4a495233871`.

## Player visibility and bot observations

The exact-sighting scenario in `planning-release.spec.mjs` uses a one-tick satellite recon zone and a 60-tick presentation stride. It checks that seeking to an exact sighting creates building memory, rewinding before the sighting hides it, a new revision starts with its own memory, and an active player cannot inspect an enemy silo's production on the older revision.

Set `ATEMPORAL_PLAYER_REPLAY_BIN` to the absolute path of the `player_replay` example binary to compare the Chromium player's tile mask and visible entity rows to the bot's observation export from the same match archive. This comparison covers a presentation sample, a sighting between samples, a later fogged state, and the original and replacement revisions. The browser remembers the frames it loads through ordinary playback and exact seeks. The bot can process every exact frame of a published revision. The shared Rust and JavaScript fixture also covers team sight, wall occlusion, and flight and recon boundaries.

On November 18, 2026, all three planning-release scenarios passed with a real local server and one Chromium worker. The export-enabled retest matched the full tile mask and the visible entities, including facing and activity. It also matched the remembered silo’s identity, health and lifecycle to the bot’s last observed silo. A stationary miner retained its attack-move order while showing idle activity. Inspected the full frames at tick 54, when the enemy silo appeared in green, and tick 100, when its gray silhouette remained. Artifacts and traces are under `artifacts/ui/player-visibility-ui-chromium151` and `artifacts/ui/player-visibility-activity`. The run used installed Chromium 151.0.7922.34 because the pinned Chromium 153 executable was unavailable. The comparison did not track animation between snapshots or a full match with four players.

A final authoritative-server run passed the export comparison and both timeline review scenarios. An active player could select a satellite-revealed silo on an older revision while its queue stayed private. The spectator and the opposing player after archival both had a read-only view of the factory's active grunt and pending queue. A separate four-player opening selected teams different from the setup defaults and matched the browser's full visibility mask and entity rows to the same archive's bot export. The shared fixture checks that allied sight can reveal a site, while an unfinished allied site outside that sight stays hidden. Inspected the opening team map and both factory queue frames. Artifacts and traces are under `artifacts/ui/player-visibility-final` and `artifacts/ui/player-visibility-teams`. This run covered the authoritative server and the four-player opening at tick 0.

## Scouting, production orders and round review

The November 21 Chromium pass exercised single-launch Shift targeting, automatic supply, a configured blueprint removed and replaced on the same tile, partial wall-line deletion, and counted queues shared by multiple factories. Full frames included two missile destinations, the remaining wall pieces and a shared queue of 30 grunts. The reports are under `artifacts/ui/nov21-orders-{browser,wall,atomic,guide}`.

The scouting scenarios show a gray last-seen miner and buildings after satellite coverage expires, and two lit bases separated by dark terrain in an archived replay. They also cover an exact sighting between presentation samples, revision isolation and private enemy plans. The screenshots are under `artifacts/ui/scout-memory-2` and `artifacts/ui/scout-memory-centred`. These runs did not use the optional bot observation exporter.

The scoreboard and rematch scenarios exercised both players’ result colors, three selectable rounds, reopening the review on a historical round, and starting a rematch. The archive scenario opened a read-only bot game through the coordinator and branched into practice. Full-frame screenshots and traces are under `artifacts/ui/nov21-rematch-review` and `artifacts/ui/nov21-archive-review`. The browser tests run against local game servers.

The rules 13 release passed `combat-facing.spec.mjs`. Inspected frames in `artifacts/ui/combat-rules13` show the route reaching the enemy turret, independently aimed tank and turret barrels, and the artillery barrel aligned with the outgoing shell arc. The scenario also verifies live playback rotation and seeking back to the idle pose. Six integrated Chromium scenarios passed in `artifacts/ui/nov21-integrated-review`, including a three-round review that seeks to round 2’s final tick, stable workspace and button positions across successive steps, and combined replay fog.

### Replay retention verification, November 21, 2026

The desktop Chromium coordinator and landing suites passed all nine scenarios. The retention scenario opens a real bot recording, seeds six legacy winning ledgers in a tombstoned lobby, and restarts the coordinator. A fresh browser sees the five newest completion times and opens the newest replay read-only. The sixth replay and the live lobby return 404. The inactivity scenario ages the saved activity timestamp to test the 24-hour boundary without waiting a day. Screenshot review covered the lobby list, replay cards at desktop and 390px widths, and read-only playback. Other browser engines were not exercised.

## Factory placement and output direction

`factory-placement.spec.mjs` ran against the authoritative server with seed 42
in desktop Chromium 153 at 1920×1080 and 40 pixels per tile. At revision 0,
tick 0, a staged turret blocked a factory's east output. Chromium checked that
the red marker outlined the output tile, the message named the turret
blueprint, and the attempt left the existing draft intact.

A second scenario used R and mouse clicks to stage all four orientations. It
checked the canvas markers in each placement preview and across the four draft
ghosts, then assigned a constructor and queued a grunt per factory. In revision
1 it verified orientation on a construction site at tick 5 and on all four
completed factories at tick 45. It resumed playback and checked the rendered
bays and markers using compact samples between exact seeks. At tick 85 the
authoritative state placed each newborn on the marked tile. Full-frame preview,
blocked-placement, ghost, site, completed-factory and newborn screenshots were
inspected. Screenshots and traces are under
`artifacts/ui/factory-placement-chromium`. The scenario used one browser worker
and exercised the authoritative transport.

An adjacent authoritative run passed 19 scenarios covering factory queues and
storage fallback, path previews, wall editing, construction spending, replay
visibility, allied sight, repeat placement, rematches and output displacement.
It used one pinned Chromium worker. The initial rebase scenario continued by
clicking Commit after the checker had preserved the draft at its original
tick. The corrected test verifies the immediate refusal and the unchanged
draft, then commits deliberately at the original tick. It passed with all six
coordinator scenarios, all three landing scenarios and both draft-ID
scenarios. Inspected frames included the mixed factory routes, remaining wall
pieces, Shift repeat placement, last-seen buildings, combined replay sight,
lobby creation, landing actions and the rebase notice. The two runs are under
`artifacts/ui/factory-placement-adjacent` and
`artifacts/ui/factory-placement-coordinator-landing`. They used the release
server built for the placement changes and did not verify later Tactical
policy changes.

## Checker incidents and guarded continuation

All four `checker-incidents.spec.mjs` scenarios passed in desktop Chromium 153
at 1920×1080 with seed 42 and one worker per run. The tests used live player
inputs and injected checker disagreements in the controlled browser session.
A submitted draft with conflicting local IDs and an exact-state placement
mismatch each saved a distinct incident at revision 0, tick 0. The returned
identifiers matched the saved files. The tests compared the saved world state
with the exact snapshot, checked that local aliases retained their meaning
without retaining their original strings, and verified that the incident files
were unavailable through browser asset URLs.

An ordinary occupied tile produced a placement reason and created no incident.
When the local checker wrongly refused a legal placement, rebase, or commit
preflight, the authoritative checker confirmed the original draft and recorded
the fault. The browser staged the placement, rebased the draft to tick 5, and
submitted it through normal validation. The published revision had both
blueprints at tick 6. Escape cancelled an action before a deliberately delayed
approval arrived, leaving the draft untouched. An injected filesystem failure
produced an operator log entry with an attempted report ID and a player message
that the report could not be saved.

Inspected the full rendered rejection, resumed commit, cancelled retry and
failed-write notices. Reports and traces are in
`artifacts/ui/checker-incidents-continuation-verified` for direct transport and
`artifacts/ui/checker-incidents-continuation-peripheral` for the inputs-only
relay. The focused checks did not repeat the full browser suite or inject
discrepancies into a live deployment.

The landing-page Chromium scenario opens Patch Notes in its own tab and checks all five category filters. It clears every checkbox, selects Docs, then adds Bot update and verifies individual notes and commit links. Desktop and 390-pixel-wide screenshots were inspected. The filtered mobile frame is in `artifacts/ui/category-filters-final`.
