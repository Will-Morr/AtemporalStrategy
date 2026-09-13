# Complete browser experience handoff

Worktree: `../atemporal-client`, branch `agent/client`, based on main `6d760bd`.

The client exposes the input map in `docs/client.md` through the keyboard and visible controls. Existing slice tests were extended in place. No simulation mechanics were changed.

## Implementation and coverage

| Area | Implementation and real-browser evidence |
| --- | --- |
| Lobby and guide | Profile updates broadcast to another player, guide opens in a separate tab, generated reference and actual controls reviewed at desktop and 390px width. Team selection uses the server's available teams. |
| Groups | H/Shift+H replacement/addition, J binding/clearing, ten-slot panel with living members/bound factories/saved order tick, empty-group orders and direct/group recipient label. Real replay verifies membership and output-bound newborn membership. Spectator can choose the player whose groups to inspect. |
| Orders and production | R supports all five order kinds. Queue focus/arrow navigation/Delete, active cancellation, clearing pending recipes, blueprint cancellation, connected wall line placement, Z output preview, priority, loop, and future-order policies. Draft undo/redo keeps the original tick; rebase is undoable. |
| Timeline | Ruler/middle-button drag, pointer zoom, keyboard zoom/pan, 0.25–8× presets, tick entry/steps/jumps, event tooltips, hatched immutable history, white playhead and dashed yellow draft marker. Timed test verifies order rejection before the boundary. |
| Statistics | Canvas plots for bank/mined/spend/living army/infrastructure/attrition/thinking-time ratio; player/opponent and full/visible tick windows; hover values. Thinking-time plots use round history. Zero denominators remain undefined. Displayed scores distinguish raw/adjusted totals; top bar separates committed and live planning time. |
| Replay | Historical round picker is read-only and preserves a live draft. Accepted input tick links and authoritative applied/skipped diagnostics. Before/after outcome, survivors, spend, living counts, entity births (including sites), destruction/cancellation. Timed finalized losses remain separate from battlefield survival history. |
| Connections and focus | Real offline/online transition reconnects without losing historical inspection or a live draft. A real server process is restarted on the same address; stale instance prompts reload into the fresh lobby. Text fields keep typing keys, Shift chords handle both physical/browser key representations, and focused buttons retain native activation. |
| Rendering | Grayscale battlefield, player/team colors and owner numbers, separate site health/completion bars, facing and combat effects. Effects capped at 24–160 per frame depending on speed; snapshot/event chunks bounded to eight per viewed revision. Long draft lists scroll independently so Commit stays visible. |
| Archive control | Real stop-and-archive input produces an unfinished result, disables Commit, and preserves score. |

## Commands

Run from the worktree root:

```sh
scripts/check.sh
ATEMPORAL_UI_SERVER_COMMAND='cargo run --release -q -p atemporal-server -- --replays target/ui-replays' npm run ui:review --prefix client -- --grep slice --project=desktop-chromium
```

For a fresh checkout, first use `npm ci --prefix client` and `npm run build --prefix client`; the custom server command expects `client/dist` to exist. `scripts/check.sh` also builds it. Browser dependencies must be installed as described in `docs/browser-testing.md`.

The scoreboard test uses seed 42, the repository's 48×48 two-player configuration, desktop Chromium at 1440×1000, three independent browser identities, and revisions 0–3. Inspection includes exact ticks 40, 153, 154, and the 19,726-tick combat result. The second test launches an isolated real server with the same seed and a 100-tick timed boundary, advances through nonzero checkpoints, and restarts that process on its original port. It preserves artifacts under its test output directory.

## Integration contract

`49d7256` introduces `get_round{revision}` / `round_result`: round/parent metadata, outcome, score, timed adjudication, timeline, committed duration totals, simulation duration and command outcomes. It also retains command diagnostics before the reconstruction checkpoint. `58a982b` adds protocol fixtures and zero-time entries for every player. These server/contracts changes are separate from client work so they can be reconciled with the in-flight server branch. Regenerate schema and TypeScript together when rebasing.

The server integration points are `ClientMessage::GetRound`, `ServerMessage::RoundResult`, `Controller::round_result`, the WebSocket query handler, and prefix diagnostic retention in `start_job`/`on_complete`. Existing snapshot/event/stat/command queries remain in use. Replies without range/request IDs are serialized by the client to prevent overlapping inspection requests from matching the wrong chunk.

## Limits

- Local lock estimates intentionally do not simulate a draft. They project sequential membership edits and scheduled deliveries from exact state, and may differ after future births/membership changes, missing or incompatible targets, and simultaneous replay. Authoritative results appear only after publication.
- Browser scenarios cover two-player scoreboard and timed matches. Four-player/2v2 behavior, a scripted loss-then-recovery scenario, full entity-cap stress, Firefox/WebKit, and mobile battlefield interaction were not verified in this handoff. The narrow guide was reviewed. Team colors and recovery rendering consume the existing server records.
- Historical inspection uses published revisions held by the running server. Archive restoration/regeneration is still the server owner's responsibility; restarting the current server creates a fresh match.
- Graphs use bounded sampled data; hover shows sampled values, not reconstructed statistics at every tick. Entity birth summaries include site creation; destroyed events also include site cancellation.
- No main-branch integration or human playtest was performed. Re-run these checks after reconciling the server agent's changes.

## Verification record — 2026-09-13

`scripts/check.sh` passed on the final source, including workspace/default-feature Rust tests, strict Clippy, TypeScript/build, 30 shared fixtures and 7 malformed boundary cases, and generated-file drift. The local command log is `target/client-check.log`.

The final prescribed real-server browser command passed **2 tests in 39.3 seconds**. Its artifact root is `artifacts/ui/2026-09-13T05-39-14.193Z-435917/`; `run.json`, `results.json`, the HTML report, per-test browser logs and traces are retained there. Screenshots were inspected directly across the iterations; fixes included reachable Commit with long drafts, historical-view labeling, Shift chords, diagonal walls, single-point thinking-time graphs, and archived timer labeling.

Representative final screenshots (paths relative to this document):

- [Long draft and group bindings](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-two-players-and-a-sp-34447--opening-rewrite-and-replay-desktop-chromium/group-binding-and-saved-order.png)
- [Connected diagonal wall](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-two-players-and-a-sp-34447--opening-rewrite-and-replay-desktop-chromium/diagonal-connected-wall.png)
- [Bank graph](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-two-players-and-a-sp-34447--opening-rewrite-and-replay-desktop-chromium/graph-bank.png)
- [Thinking-time graph](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-two-players-and-a-sp-34447--opening-rewrite-and-replay-desktop-chromium/graph-thinking.png)
- [Authoritative rewrite diagnostics](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-two-players-and-a-sp-34447--opening-rewrite-and-replay-desktop-chromium/rewrite-command-skipped-reasons.png)
- [Unfinished archive](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-two-players-and-a-sp-34447--opening-rewrite-and-replay-desktop-chromium/unfinished-archive.png)
- [Immutable history and draft marker](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-slice-timed-history-guide-and-server-restart-desktop-chromium/draft-marker-versus-playhead.png)
- [Stale server instance](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-slice-timed-history-guide-and-server-restart-desktop-chromium/stale-server-instance.png)
- [Narrow guide controls](../artifacts/ui/2026-09-13T05-39-14.193Z-435917/results/slice-slice-timed-history-guide-and-server-restart-desktop-chromium/guide-controls-narrow-viewport.png)

## Final UI playtest fixes — 2026-09-13

Implementation branch `agent/ui-final`, worktree `../atemporal-ui-final`. The actual desktop Chromium scenarios pass against the real server: two-player/spectator opening, production, rewrite and historical replay plus timed history/restart (artifact run `2026-09-13T07-23-50.358Z-488743`, 2 tests); focused playtest regressions (run `2026-09-13T07-27-31.646Z-497920`, 1 test). Screenshots inspected include the grouped command deck, ghost configuration, bottom-edge camera, and effective factory orders. Earlier failed runs are retained; they exposed clipped controls and the revised factory command expectation.

The focused test uses seed 42, a 48×48 map, 2,000 starting matter and a 2,000-tick cap; it configures an unfunded factory with a grunt queue, high priority and an attack order using actual buttons/keys, then verifies the completed factory and newborn in authoritative tick 153 state. It checks replacement/undo/Ctrl+U, R rotation, hidden incompatible buttons, fogged enemy omission, matter labels, selected-recipient timeline ticks, and Shift-wheel panning. The simulation regression separately checks immediate production after completion and inherited action, and caught/fixed a causal birth-ID collision. Full workspace tests and strict Clippy pass after fixture regeneration.

This record covers the playtest-fix batch; multiplayer variants, recovery and complete handoff verification follow in a separate batch. Fog is a presentation constraint over inspectable replay data, not server-side information secrecy.
