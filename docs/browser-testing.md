# Browser review for any agent

Both layers are project-owned and agent-neutral. The repeatable harness is ordinary Playwright Test; interactive control is a standard stdio Playwright MCP server. Neither requires Codex, an OpenAI API, a model-specific SDK, or a proprietary browser tool. Use the same commands from a terminal, CI, another coding agent, or an MCP-capable client.

## Install and run repeatable review

From the repository root:

```sh
npm ci --prefix client
npm run ui:install --prefix client
cargo build --release -p atemporal-server
npm run build --prefix client
ATEMPORAL_UI_SERVER_COMMAND='target/release/atemporal-server --seed 42 --replays target/ui-replays' npm run ui:review --prefix client
```

Chromium is pinned by Playwright 1.63.0. On a Linux machine missing browser libraries, install Playwright's documented OS dependencies with `cd client && npx playwright install-deps chromium`; this can require administrator access. The current development host already runs the browser successfully.

Every default run allocates a local port, builds the browser assets and starts the real game server (which generates its guide from effective content), runs desktop (1440×1000) and narrow (390×844) cases, and closes its server/browser. It refuses to reuse an unrelated server on its selected port. Separate worktrees and processes get separate artifact directories and browser profiles. A port collision after allocation is reported as a failed start rather than attaching to another agent's app. Use an explicit port when needed:

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
| Landing and guide | Keyboard link activation, generated stats, narrow layout, screenshot inspection | Real lobby and generated guide |
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

See [integration verification](integration-verification.md) for the tested variants, performance workload, manually inspected full-frame evidence and actual limits. `playtest.spec.mjs`, `variants.spec.mjs` and the real-lobby guide tests supplement `slice.spec.mjs`; the narrow gameplay cases exercise a scrolling command deck. Historical round metadata loads without eagerly regenerating old replay bodies; selecting a round or opening its detailed comparison loads those data on demand.

The playtest-polish scenarios additionally cover shared-browser player identities, explicit rejoin, uncommit with restored undo/redo, factory loop and queue editing during construction, hidden enemy blueprints at visible tiles, hybrid history advancement/recovery, and exact per-player latest-write markers. See the current pass in [integration verification](integration-verification.md); retain earlier failed runs as diagnosis evidence.


`progressive.spec.mjs` checks direct-controller and input-only-peripheral previews in both viewports: exact non-sample seeking, typing while progress advances, playback at the frontier, refresh, disabled planning and equality with the final replay. Its isolated processes use `ATEMPORAL_PREVIEW_TEST_DELAY_MS` to keep the long tail observable on fast machines; the production default adds no delay. The project timeout also gives built-in trace finalization up to 120 seconds; the review fixture has its own cleanup budget. Revision helpers wait for publication, so existing scenarios cannot accidentally count a provisional state as a verified result.


`feedback.spec.mjs` covers factory/turret blueprint deletion, undo/redo and removal of dependent settings, shared/mixed priority and group feedback, rebasing before a blueprint existed without submitting an invalid commit, cancellation of an active turret construction site, exhausted ore and smoothed mining-rate plots. It uses the peripheral when `ATEMPORAL_UI_PERIPHERAL=1`; direct and replicated verification evidence is recorded in the integration log. The pure slope checks run with `npm test --prefix client` and cover irregular spacing, negative/zero rates, missing samples and singleton windows.

## Production, repair and scoreboard scenarios

`production-priority.spec.mjs` exercises mixed priority and Off, per-item loop flags, Shift-click batches, blueprint/site/live queues, local queue references, and spending pause/resume through actual commits. `scoreboard.spec.mjs` drives a real five-win match, checks leader/winner totals at past ticks and rounds, and refreshes between wins. Both run through authoritative or input-only transport and desktop/narrow viewports. `npm test` also checks score aggregation across revision ancestry, teams, draw credits, adjusted leadership and incomplete history. Full-frame screenshots still require manual inspection before claiming visual readiness.
