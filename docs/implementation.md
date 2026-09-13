# Implementation and agent handoffs

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

## Execution policy

This pass produces specifications only. The decision register records user requirements, including constructor mining, expanded elimination, three outcomes, team play and lead-based scoring. Remaining scoring interpretations stay revisable. Work can proceed using clearly labeled, revisable defaults where needed. The first milestone is one authoritative end-to-end browser game; the second adds peripheral replication and polish. Every subsystem agent owns a long-lived worktree and should finish a cohesive subsystem, with integration fixtures rather than a pile of disconnected scaffolding.

One coordinating agent owns shared contracts and integration. After contracts freeze, create branches/worktrees such as `agent/sim` at `../atemporal-sim`, `agent/server` at `../atemporal-server`, and `agent/client` at `../atemporal-client`. Agents commit to their own branches and report commit IDs. Coordinator merges one handoff at a time, resolves shared contract changes first, then runs integration checks. Do not have multiple agents edit the same contract/config fixture. At most three subsystem agents run alongside the coordinator; assign peripheral work after the sim agent finishes.

## Nested implementation checklist

- [ ] Resolve product rules and establish contracts — coordinator
  - [ ] Implement confirmed per-round/pass scoring and configurable five-point default (scoreboard rules), preserving optional lead-N and team scoring; track remaining Q4 interpretations.
  - [ ] Freeze versioned event/state/protocol schemas and causal identity encoding.
  - [ ] Scaffold Rust workspace/browser build, pin dependencies/toolchain, document launch commands.
  - [ ] Add normalized setup/content examples and fixture protocol messages.
  - [ ] Shared content-loader guide generation at build; content-hash validation and runtime override/resume fallback.
  - [ ] Define golden tiny-world fixtures with expected actions and outcomes.
- [ ] Deterministic world and simulation — simulation agent
  - [ ] Implement validated capability-based content and full initial roster.
  - [ ] Seeded cave rooms/corridors, rotational terrain/ore symmetry, start validation.
  - [ ] Canonical state, genesis/blueprint/production identity, checkpoints and hashing.
  - [ ] Action assignment, effective historical events after per-entity suppression, dormant-target diagnostics.
  - [ ] Mining including constructor 50% throughput, bank, priority water filling, partial construction and HP.
  - [ ] Blueprint selection, production queues/loops/templates and blocked output.
  - [ ] Persistent groups 0–9, timed membership/binding edits, saved group orders and one-time member delivery.
  - [ ] Factory spawn membership/inheritance, individual overrides, checkpointed group state and suppression interaction.
  - [ ] Direct/indirect combat, vision/LOS, support and capability-gated healing.
  - [ ] Four/eight-neighbor pathfinding, cooldowns, deterministic collision/swaps.
  - [ ] Ordered parallel intent collection and identical serial fallback.
  - [ ] Completed-only active-building OR no-build-ability checks with recovery, persisting orders, three outcome kinds, inactivity cutoff/future-event guards, backup horizon, state/events/statistics export.
  - [ ] Team hostility/support/swaps with individual survival and ownership.
  - [ ] Checkpoint suffix replay equivalence and release benchmarks.
- [ ] Match controller, worker and archive — server agent
  - [ ] Static browser/guide hosting, live lobby profiles/colors/team selection, slots/tokens/spectators, phase machine.
  - [ ] Commit validation/idempotency, future-order policy/window validation, group ownership/commands and whole-group/member suppression resolution, configured control limits.
  - [ ] Implement confirmed simultaneous Q2 turn sequencing; choose earliest changed tick across round.
  - [ ] Worker process protocol, bounded progress/batches, stale-result rejection.
  - [ ] Exact state/range/stat endpoints and bounded cache/disk chunks.
  - [ ] Q3 timed boundary, FFA/team survivor scoring, fixed-target/lead thresholds, configurable ties and draw scoring, thinking time and configurable penalties.
  - [ ] Atomic accepted-turn and round-result persistence, replay/resume CLI.
  - [ ] Reconnect, worker-failure retry and pending-round crash recovery.
  - [ ] CLI port, same-origin routes, restart/resume at the same address with persisted roster.
- [ ] Browser play experience — client agent
  - [ ] Landing-page guide link, username/color input, team selection and full live roster.
  - [ ] Concise player guide prose, generated unit tables and responsive static layout.
  - [ ] Grayscale canvas floor/walls, contrasting units/ore/structures, zoom/pan/WASD and minimap.
  - [ ] Unit health bars, last-move facing and minimal movement/attack/projectile/explosion playback.
  - [ ] Selection/shift/box, groups 0–9 with visible recipient mode, factory output binding and capability-aware panel.
  - [ ] Keyboard action modes, area/line placement, construction orders.
  - [ ] Factory queue/loop/stored-order UI and blocked-output visibility.
  - [ ] Draft timestamp, future-order keep/all/window controls and removal previews, atomic undo/redo, clear/rebase and commit/pass.
  - [ ] Seek/play/rate/step/zoom/pan timeline, event bars, immutable region.
  - [ ] Revision-aware exact-state fetching and progress/reconnect handling.
  - [ ] Score/timing/spend panels, team/survivor outcomes, lead margin display, graph overlay and round replay viewer.
  - [ ] Keyboard-only action workflow with mouse used for map selection.
- [ ] End-to-end first playtest — coordinator
  - [ ] Run two player tabs plus spectator, complete an opening factory/army fight.
  - [ ] Rewrite before production and verify outcomes, IDs, overlays and undo.
  - [ ] Exercise timed/scoreboard, both control limits, asymmetric 3-player FFA, symmetric 4-player FFA and 2v2 team setup.
  - [ ] Inspect prototype usability and performance; fix actual blocking friction.
- [ ] Input-only peripheral — available simulation agent, after first playable milestone
  - [ ] Runner peripheral mode, bootstrap/fingerprint verification, local browser serving.
  - [ ] Relay commits to controller; reproduce revisions using native shared sim.
  - [ ] Hash verification, mismatch UI, reconnect/replay and cache rebuild.
  - [ ] Compare controller/peripheral hashes across retroactive multi-round fixture.
- [ ] Final hardening and handoff — coordinator
  - [ ] Adversarial scenarios below pass with recorded results.
  - [ ] Document setup YAML, content tuning, replay/resume, constraints and benchmark machine.
  - [ ] Run one human playtest; record findings without expanding mechanics prematurely.

## Agent assignments and acceptance contracts

| Agent | Exclusive ownership | Inputs | Required handoff |
| --- | --- | --- | --- |
| Coordinator | `crates/contracts`, root build files, shared fixtures/config, documentation | Settled decisions | Versioned schemas, compilable skeleton, integration harness, final assembled game |
| Simulation | `crates/sim`, content validation implementation, sim benches | Contracts and content fixtures | Library implementing `SimRequest` to deterministic batches/result, replay tests, measured release performance |
| Server | `crates/server`, worker-mode shell in `crates/runner`, archive code | Contracts; fake sim adapter until sim lands | Playable transport/lobby/commit flow, durable revisions, failure/recovery tests, launch command |
| Client | `client/` | Protocol fixtures and a local mocked transport | Full interaction flow and guide prose/layout against fixtures and then server, build output and manual keyboard checklist |
| Peripheral (later) | Peripheral module in `crates/runner` | Integrated server/sim and frozen native fingerprint | Local endpoint, input relay, verification/reconnect demonstration |

Server agent initially owns the runner entry point. Transfer runner ownership explicitly before peripheral implementation. Simulation agent must not independently change schema or fixture meanings; propose a contract change to the coordinator, who updates version/fixtures and informs all agents. Use mocks at process/library boundaries only; avoid maintaining two game engines.

## Integration gates and meaningful checks

Gate 1: schemas serialize/deserialize identically in Rust and TypeScript; tiny-world golden fixtures settle tick/state conventions. No subsystem waits for a polished UI.

Gate 2: miner → factory blueprint → funded factory → looped grunt production → attack order works from a browser commit through the worker and back to timeline. Do this before adding all graphs/peripheral mode.

Gate 3: replay hashes match with one/four threads, full replay/checkpoint replay, and controller/peripheral on the supported build. Inputs that delay or remove births never redirect old orders. Checkpoints include cooldown/production state.

Gate 4: targeted conservation and conflict tests cover water filling, simultaneous death, damaged construction, ore exhaustion, blocked spawns, swaps/corner cuts, equal-time edits, inactivity windows, capped moving cycles, long cooldowns and future scheduled commands. Use tolerance only for resource invariants; deterministic hash equality is exact.

Gate 5: fault injection after durable input, during worker run, after result writes and before publication recovers exactly one score delta and the same accepted moves. Duplicate commit requests do not append another turn. Malformed commands do not crash server/worker.

Gate 6: two-player/spectator browser walkthrough covers every action, timeline seeking, draft undo/rebase, stale responses, reconnect, graph selection and immutable-history restrictions. Run release benchmarks once stable; repeat only after relevant changes or failures.

Each handoff reports scope, command(s) to run, meaningful tests and results, known limitations, and any unmerged contract requests. Do not call a mocked subsystem complete until it is exercised with its real neighbor.

Additional acceptance cases from the user’s gameplay requirements: constructor mines exactly half miner throughput over equal active ticks; turret+constructor survives, turret+miner loses without a factory, lone constructor loses without an active building, and factory-only survives both presence tests. Hidden scouts never postpone elimination. A/B/C with A eliminated and B/C surviving reports win with two proposed FFA score deltas; no eliminations is stalemate; all eliminated is draw. A 2v2 result with one and two survivors yields team survivor counts 1 and 2; verify stalemates award zero and a partial-elimination win awards each team its survivor count. Lead-N checks handle tied leaders, simultaneous score increments and team totals. Replaying a partial elimination must still reach later eliminations and reproduce the final survivor set.

Scoring acceptance: an unchanged winning timeline continues awarding points on pass rounds; default match target is configurable 5. A 2v2 run with all four alive awards 0/0, one eliminated awards 1/2 (by surviving team membership), and all eliminated awards 0/0 in draw/none or proposed 2/2 in draw/all_players. Apply each round’s complete delta vector once before evaluating target/lead rules.

Future-order replacement acceptance: at t=20 and W=10, remove selected entities’ events at 21 and 30, preserve events at 20 and 31; DropAll also removes 31. For an A/B group event, redirecting A preserves B. Same-round new orders and other owners are untouched. Window mode rejects disabled/nonpositive configuration. Single-order mode accepts replacement+removals as one command. Undo/rebase previews restore/recompute precisely. Later new commands inside an old cleared window execute normally. Original round playback retains its original events; new revisions and peripherals share the same suppression set/hash. Suppressing the only distant future order removes its inactivity-wait obligation. Suppressing a future factory queue edit preserves prior active production and causal spawn identity. Test that target disappearance after simultaneous rewrite does not undo the accepted suppression.

Control-group acceptance: issue group order A, individually redirect member U to B, then spawn V; U stays on B and V joins/inherits A. Later group order C overrides both once, while a subsequent individual override remains until another group order. Empty-group orders are valid. Several factories feed one group; blocked output inherits the latest order/binding at actual spawn. Group command plus any future removals counts once in single-order mode. Overlapping group deliveries obey canonical order. Full/checkpoint/peripheral replay agrees after moving a birth or group order earlier. Suppressing U’s component leaves saved order and V’s behavior intact; suppressing the whole group event removes both delivery and saved-order write. Plain digit recall changes selection without consuming a turn; changing a recalled selection clearly switches to individual targeting.

Visual acceptance: units/ore/orders contrast clearly against both gray terrain tones; health bars show damage at exact ticks; successful axis/diagonal movement updates facing while idle, blocked moves and attacks retain it. Seek/checkpoint replay restores facing after a long idle interval. Pause freezes effects, scrubbing and revision changes remove stale effects, and combat between sampled snapshots remains visible through events. Visual projectile duration never shifts damage timing or sim hashes. Dense combat/high playback rate remains readable with bounded effect counts. Inspect a replay containing death, artillery fire and allied swaps.

Latest survival/scoring acceptance: an unfinished factory does not prevent elimination, but its surviving constructor continues building and restores the player on completion. A temporarily eliminated army can eliminate the opponent later, producing a draw instead of an early win. Continue even at zero current survivors if actions/future inputs can change state. Recovery followed by all players alive is stalemate and never scores. Full/checkpoint/peripheral replay preserves both transitions and final status; an immutable past elimination does not disable future recovery. For 2v2 all-eliminated endpoints, draw/none gives 0/0 and proposed draw/all_players gives 2/2; survivors remain empty in both. Default target tie at 5/5 continues, 6/5 ends; configured shared victory returns both at 5/5. Test complete same-round score reduction with draw/tie policies, and no duplicate awards after crash recovery.

Guide/lobby handoff: coordinator owns shared loader/exporter and build wiring; client agent owns player-facing prose/layout; server agent owns guide selection, live roster persistence/broadcast and CLI port. Do not independently maintain a second stat schema or move fixed team selection back into YAML-only assignments.

Acceptance: changing a unit stat rebuilds both sim content and guide table; runtime overrides and resumed archived stats serve a matching guide rather than bundled stale numbers. Validate guide links, readable roster tables and walkthrough against a real opening turn. In two player tabs plus spectator, username/color/team edits appear everywhere; a late join gets the complete roster and a stale Start is refreshed. Start freezes the visible accepted roster. Launch on a nondefault port, restart/resume on that port, and refresh existing tabs to recover identities/phase; reject an occupied port without changing URLs. Guide and WebSocket work at the same chosen origin. Profiles and documentation must not alter simulation hashes.
