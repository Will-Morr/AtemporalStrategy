# Browser review for any agent

Both layers are project-owned and agent-neutral. The repeatable harness is ordinary Playwright Test; interactive control is a standard stdio Playwright MCP server. Neither requires Codex, an OpenAI API, a model-specific SDK, or a proprietary browser tool. Use the same commands from a terminal, CI, another coding agent, or an MCP-capable client.

## Install and run repeatable review

From the repository root:

```sh
npm ci --prefix client
npm run ui:install --prefix client
npm run ui:review --prefix client
```

Chromium is pinned by Playwright 1.63.0. On a Linux machine missing browser libraries, install Playwright's documented OS dependencies with `cd client && npx playwright install-deps chromium`; this can require administrator access. The current development host already runs the browser successfully.

Every run allocates a local port, builds the browser assets and starts the preview server (which generates its guide from effective content), runs desktop (1440×1000) and narrow (390×844) cases, and closes its server/browser. It refuses to reuse an unrelated server on its selected port. Separate worktrees and processes get separate artifact directories and browser profiles. A port collision after allocation is reported as a failed start rather than attaching to another agent's app. Use an explicit port when needed:

```sh
ATEMPORAL_UI_PORT=8090 npm run ui:review --prefix client
ATEMPORAL_UI_BASE_URL=http://127.0.0.1:8080 npm run ui:review --prefix client
```

The second command reviews an already running real game server, without launching/rebuilding or stopping it. `ATEMPORAL_UI_SERVER_COMMAND` can replace the default preview-server command; it runs from the repository root and receives the selected `PORT`. When the server handoff lands, point it at that real server and extend the tests for gameplay. The current tests deliberately cover the existing scaffold/guide, not an invented battlefield.

Playwright flags pass through normally:

```sh
npm run ui:review --prefix client -- --project=desktop-chromium
npm run ui:review --prefix client -- --grep "keyboard"
npm run ui:review --prefix client -- --headed
```

A headed run requires a display. Headless Chromium renders the actual page, executes JavaScript and handles canvas; it is not a DOM-only emulator. Adding Firefox/WebKit projects is optional and requires installing those browsers.

## Review artifacts and iteration

The command prints an `artifacts/ui/<run-id>/` path. Override it with `ATEMPORAL_UI_ARTIFACTS` if your agent runner needs a known output location. Each run contains:

- `run.json`: run ID, server URL and arguments.
- `results.json` and `report/index.html`: machine-readable and human-readable results.
- `results/<test>/`: named PNG screenshots, accessibility snapshots, browser console/network errors, and `trace.zip`. Videos are retained on failure.

Open reports and traces locally:

```sh
cd client
npx playwright show-report ../artifacts/ui/<run-id>/report
npx playwright show-trace ../artifacts/ui/<run-id>/results/<test>/trace.zip
```

An agent should run a relevant scenario, **inspect the resulting PNGs**, diagnose visual/interaction problems, edit the UI, and repeat the same scenario. A passing selector assertion is not a visual review. Preserve failure evidence; tests do not retry automatically. The harness captures all reviewed contexts' errors and fails on page errors, failed requests, or console errors. Screenshot names describe the actual inspected state.

The current multi-context test verifies independent player-A/player-B/spectator storage and page loads. It does not claim slots, commits, or combat are implemented. New gameplay tests should use actual user inputs and server setup with a deterministic seed. Capture a known paused timeline tick before comparing images; never let changing simulation/playback time stand in for a visual regression. Prefer role/text locators for UI controls and coordinate input plus screenshots for canvas. `review.capture(name, page)` supports both. Pixel baselines can use Playwright's `toHaveScreenshot` once a meaningful scene exists; inspect baseline changes rather than blindly refreshing them.

Artifacts are git-ignored and retained per run for inspection. Long-running CI should upload relevant artifacts and apply its own retention policy. They are not game archives.

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

The smoke client speaks JSON-RPC directly: initializes MCP, discovers browser and coordinate tools, navigates the actual page, clicks the guide link, reads its snapshot and saves a screenshot. It closes its browser and preview server afterward. A new/reloaded agent session may be necessary for its client to discover newly configured tools; adding a server does not retrofit this chat's advertised tool list.

The desktop app's built-in `@Browser` remains an optional separate connection. Neither layer depends on it. See [Playwright web-server setup](https://playwright.dev/docs/test-webserver), [traces](https://playwright.dev/docs/trace-viewer-intro), [visual comparisons](https://playwright.dev/docs/test-snapshots), and [Playwright MCP](https://github.com/microsoft/playwright-mcp) for upstream behavior.
