# Implementation and agent handoffs

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

## Execution policy

The vertical slice is implemented and Gate 2 is met: the real engine, a server running it on a dedicated thread, and a browser client play the opening, commit simultaneous turns, seek any tick and rerun history from checkpoints; see [development](development.md), [stabilized contracts](contracts-v2.md) and [Gate 2 measurements](gate2-measurements.md). Simulation, controller and browser breadth are implemented; final browser and integration verification close the checklist below. Mocks remain limited to targeted fault injection and protocol unit fixtures. All engineering defaults remain revisable when implementation provides evidence.

Complete the entire planned feature set—including roster, maps, game variants, groups, guide, graphs, replay and peripheral—before asking the user to play. Intermediate automated scenarios and agent-run browser checks are engineering verification, not a user playtest.

Shared contracts/content and integration remain coordinated across isolated worktrees. Merge handoffs sequentially and reconcile generated contracts together. The native input-only peripheral is assigned separately; the user explicitly made it non-blocking for UI implementation and verification.

## Nested implementation checklist

- [x] Establish the real vertical slice — one lead agent, before subsystem fan-out
  - [x] Scaffold the Rust workspace and browser build; pin toolchain/dependencies and document launch commands.
  - [x] Implement shared content validation and provisional contract types, causal tuple IDs and pre-tick state conventions.
  - [x] Use a small deterministic fixture map and enough real content for miner → constructor → factory → grunt production.
  - [x] Implement shared four/eight-neighbor destination BFS fields and bounded local stuck handling.
  - [x] Implement tick action/motion, mining/construction/production, basic combat and simple allied displacement.
  - [x] Run the actual sim library on a dedicated server thread with typed channels and per-tick cancellation.
  - [x] Serve a minimal browser with selection, attack-move, blueprint placement, queue, commit and timeline playback/seek.
  - [x] Stage/validate at any editable tick using exact in-process reconstruction, including non-sample ticks.
  - [x] Persist real accepted turns/results and rerun from a checkpoint after an earlier-tick rewrite.
  - [x] Measure Gate 2 round latency, cold seek and per-revision event/stat/snapshot size with export enabled.
  - [x] Stabilize versioned contracts and shared fixtures from the working slice, then assign breadth work.
- [x] Complete deterministic simulation — simulation agent after Gate 2
  - [x] Full capability-based roster and validated content, including constructor mining at 50% miner throughput.
  - [x] Connected cave rooms/corridors, rotational symmetry, start access and configurable ore budgets.
  - [x] Continuous bank spending, tier water filling, damaged partial construction and cancellation rules.
  - [x] Production queue editing/loops, output direction/blocking and stored-order fallback.
  - [x] Persistent groups 0–9, membership/bindings, saved orders and newborn inheritance.
  - [x] Entity/group order locks, overlapping intervals, same/newer-round overrides and missing-target diagnostics.
  - [x] Complete combat behavior table, target state, direct/indirect fire and capability-gated support/healing.
  - [x] Shared-field cold/warm equivalence, static invalidation, bounded local detours and goal-crowd settling.
  - [x] Ordered parallel intents/serial reductions, serial fallback and HashMap/HashSet prohibition.
  - [x] Checkpointed locks/cooldowns/facing/targeting/stuck state; exclude derived flow caches and cache version.
  - [x] Completed-only survival checks with recovery; inactivity/absolute horizon and configurable decided-side stop that preserves active recovery.
  - [x] Compact revision entity dictionaries, events/stats, canonical hashes and full/checkpoint replay equivalence.
  - [x] Release benchmarks at 100/500/2,000 entities and the full configured cap.
- [x] Complete match controller and archive — server agent after Gate 2
  - [x] Lobby usernames/colors/team selection, live roster, slots/tokens/spectators and stale-start protection.
  - [x] Sim adapter scheduling, bounded channels, exact-state LRU, job errors/cancellation and stale-result rejection.
  - [x] Simultaneous commit validation/idempotency, sequential draft references, control limits and policy validation.
  - [x] Snapshot/command/stat ranges, exact states, bounded cache chunks and on-demand archive regeneration.
  - [x] Timed constructor/factory finalization and scoreboard survivor/lead/target/draw/tie/time-penalty reducers (checked end to end by `scripts/match-check.mjs`; the reducers live in `crates/contracts`).
  - [x] No scoreboard round cap; manual stop/archive with no invented result or extra score.
  - [x] Atomic turn/round publication, pending-round recovery, replay/resume CLI and timing persistence.
  - [x] CLI port, same-origin routes, stable-address restart and fresh bootstrap/server-instance detection (checked by the `port_routes_and_guide` scenario in `scripts/match-check.mjs`).
  - [x] One startup guide-generation invocation using loaded content, also used for archived-content guides (a resumed match regenerates the guide from its pinned content copy; same scenario).
- [x] Complete browser experience — client agent after Gate 2
  - [x] Landing-page guide link, profile/color/team inputs and full live roster.
  - [x] Concise guide prose/layout and generated readable unit/building stats.
  - [x] Grayscale terrain, contrasting entities/ore, zoom/pan/WASD and minimap.
  - [x] Health/completion bars, last-move facing and minimal movement/projectile/explosion playback.
  - [x] Selection/shift/box, groups 0–9, visible direct/group recipient mode and factory output bindings.
  - [x] Keyboard action modes, areas/lines, queue/loop/template/priority controls and placement output preview.
  - [x] Draft timestamp, local non-authoritative lock estimates, undo/redo/rebase and commit/pass.
  - [x] Any-tick exact-state loading, seek/play/rate/step/zoom/pan timeline and immutable region.
  - [x] Event bars, before/after result summaries, actual skipped-command reasons and tick links.
  - [x] Score/time/spend panels, graphs, recovery/final-loss distinctions and historical-round viewer.
  - [x] Reconnect, stale-instance/revision handling, browser-safe keys and text-input focus behavior.
- [x] Complete input-only peripheral — assigned agent after the slice, before user playtest
  - [x] `runner` native peripheral, shared sim adapter/library, controller bootstrap/fingerprint validation.
  - [x] Local browser/guide serving on configurable port and commit relay to the controller.
  - [x] Derive locks/IDs/groups from inputs; compare revision hashes without world-state streaming.
  - [x] Mismatch UI, reconnect, checkpoint/cache regeneration and multi-round retroactive replay (`scripts/peripheral-check.mjs`; the mismatch diagnostic reaches the browser through withheld planning and failed queries, not a dedicated panel).
- [x] Full integration and handoff — coordinator
  - [x] Real two-player tabs plus spectator cover opening, production, combat, rewriting and replay.
  - [x] Exercise both objectives/control limits, 3-player FFA, 4-player FFA and 2v2 teams.
  - [x] Verify behavioral, durability and performance checks; fix blockers and record measured limits.
  - [x] Document launch/setup/content/guide/archive usage, constraints and measured benchmark machine/results in [integration verification](integration-verification.md).
  - [x] Complete browser integration without an intermediate human-playtest gate; the separately assigned native peripheral retains its own handoff.

## Agent assignments and acceptance contracts

| Agent | Exclusive ownership after Gate 2 | Required handoff |
| --- | --- | --- |
| Coordinator | `crates/contracts`, `crates/content`, guide generator, build wiring, shared fixtures/config/docs | Stabilized slice, compatible schema changes, startup stat export and assembled game |
| Simulation | `crates/sim`, sim benches | Real library completing mechanics, cold/warm/cache/thread replay checks and measured performance |
| Server | `crates/server`, in-process sim adapter, archive code | Real lobby/commit/publication/resume flow, failure checks and launch command |
| Client | `client/`, guide prose/layout | Full interaction flow against real server, build output and agent-run browser checklist |
| Peripheral | `crates/runner` | Native controller connection, local endpoint, input relay and hash/reconnect demonstration |

Content loader/exporter has one owner. The guide depends on content; the sim never depends on HTML generation. Server owns startup invocation/routing, not a second loader. Agents work against the integrated slice rather than waiting for separate mocked subsystems. Each handoff reports commit IDs, runnable commands, checks/results, limitations and any coordinated contract changes.

## Integration gates and meaningful checks

Gate 1: provisional Rust/TypeScript schema fixtures and tiny worlds establish tick semantics, tuple identities and serialized data. Do not freeze the whole contract or fan out subsystem agents yet.

Gate 2 (met): real miner → factory blueprint → funded factory → produced grunt → attack works from browser commit through the in-process sim and back to timeline. Seek to a non-sample tick, issue an order there and verify partial resimulation against full replay. Measure actual event/stat/snapshot bytes and commit-to-playable/cold-seek latency with default-cap export before selecting retention budgets. Verified by `crates/sim/tests`, `scripts/gate2-check.mjs` and `client/tests/ui/slice.spec.mjs`; numbers in [Gate 2 measurements](gate2-measurements.md). This was an automated engineering milestone, not an early user playtest.

Gate 3: full/checkpoint/peripheral replay has identical hashes with one/four threads and cold/warm/evicted flow caches. Restore lock windows, targeting, cooldowns and bounded local detours. Clippy prohibits HashMap/HashSet in the sim. Compact transport dictionaries must not affect gameplay order or IDs.

Gate 4: the behavioral fixtures below pass. Use numeric tolerance for conservation invariants, but exact equality for deterministic hashes. The real engine acceptance suite covers these fixtures.

Gate 5: inject failures after durable input, during a sim-thread job, on cancellation/channel backpressure, after result writes and before publication. Recover one score delta and the same accepted moves. A failed/canceled job cannot mutate a published revision. Process-kill recovery uses the archive; recoverable thread failure restarts a clean job. Duplicate commits do not append another turn. Disk-full leaves accepted inputs recoverable and the last published result intact.

Gate 6: agent-run multi-tab browser walkthrough covers all actions, exact-tick planning, replay, guide, graphs, variants, reconnect and immutable-history rules. Measure typical and cap-length 20,000-tick runs, dense chokepoints and repeated near-zero rewrites. Report end-to-end latency, seeks, memory and bytes/revision with export enabled. Cache eviction preserves access to every archived round through regeneration. Repeat checks after relevant changes/failures, not as unbounded busywork.

## Behavioral fixtures

- **Economy:** equal-duration constructor mining is half dedicated-miner throughput; high/medium/low allocation obeys capped equal shares; damage remains lost during funding; blocked paid factory output never duplicates or double-charges a birth. Default peaceful mine-and-loop opening exhausts its ore and reaches inactivity within the target wall time; estimate versus measured depletion is recorded.
- **Movement:** cold/warm fields select identical legal next steps; walkers cannot cut corners and vehicles never move diagonally, including forced displacement. Idle blockers yield; packed/opposing corridors and factory crowds move without overlap or moving an entity twice. Group arrivals settle without perpetual displacement. Static target fields do not make structures traversable. Radius-6 stuck detours retain a necessary uphill first step across checkpoints; unreachable fields do not repeatedly launch full searches.
- **Identity:** one queue command targeting two factories produces distinct tuple IDs. Delayed/removed births never retarget orders. Site completion preserves ID. Loop occurrences advance only at actual spawn. Compact u16/u32 dictionaries decode old revision events independently of the current revision; crossing the conservative u16 bound chooses wider encoding, not truncation.
- **Order locks:** at t=20/W=10, older events at 21 and 30 skip, 20 and 31 remain; DropAll also skips 31. Explicit A/B delivery with only A locked still applies to B. Same-round/newer commands execute. Group slot locks block older saved-order writes; member locks preserve those writes. Newborns inherit active slot locks. A missing replacement target installs no lock. A short newer window does not erase a longer prior one or extend newer-round restrictions beyond its window. Local estimates may differ from actual no-op results after rewriting. Fully locked distant events need not postpone inactivity; uncertain future targets do.
- **Control groups:** group order A, individual override B on U, then spawn V leaves U on B and V on A. Later group C overrides both once. Empty groups store orders; overlapping groups follow canonical event order. Blocked output inherits at actual spawn. Single-order mode counts one group assignment plus its locks as one command.
- **Combat/inactivity:** automatic turret fire, idle defense, target acquisition/loss, reached destinations, hold-on-cooldown and mutual Support reading prior snapshot. Unreachable retries/unfunded production stop; otherwise-legal long cooldowns defer inactivity. Voluntary detours count as progress; forced displacement alone does not. Normal/fast-forward/checkpoint stop ticks agree.
- **Survival/scoring:** unfinished entities do not prevent elimination; surviving constructors continue work and can restore a player. Residual armies can produce mutual elimination; survivor count alone never stops active recovery; the configurable decided-side stop requires opposition unable to act. All recovered → stalemate/zero points. Partial survival → win/survivor awards. All eliminated → configured draw/none or all-player awards, with an empty survivor list. Apply all deltas before target/lead/tie checks; default 5/5 continues and 6/5 ends, shared-victory mode can return both.
- **Match adjudication:** locked active-building absence with a constructor does not finalize timed loss; locked constructor/factory absence does. Mutable future losses remain editable. Reduce simultaneous losses together. Scoreboard never auto-stops after repeated ties/stalemates; manual archive records unfinished without another award or fabricated battlefield draw and retains partial-round inputs.
- **Browser/guide/port:** live usernames/colors/teams appear for both players and spectators; stale Start refreshes roster. Non-sample exact ticks never snap. Local lock estimates account for preceding draft edits. Temporary blueprint references resolve consistently. Startup and resumed-content guides use actual loaded stats through one generation function. Nondefault-port restart/resume works by refreshing existing tabs; occupied ports fail without silently changing URLs.
- **Visuals:** grayscale terrain keeps entities/ore/orders legible; health/facing restore at arbitrary ticks. Pause/seek/revision changes handle cosmetic shots/explosions without stale effects or shifted damage timing. Short combat between samples remains visible through events; dense playback bounds cosmetic counts.


## Subsystem handoff status

The integrated engine supports bounded parallel intents, 2–4-player maps, allied traffic and checkpoint-safe detours. The browser includes group editing, graphs, per-round replay and before/after comparison. The server supports resume/verification, retention budgets and regeneration. Final verification exercises these together; the separately assigned native peripheral has independent ownership.

| Timing | Bounded assignment | Ownership and handoff |
| --- | --- | --- |
| Complete | Simulation breadth | `crates/sim`: full roster/mechanics fixtures, parallel intents with cold/warm/thread equivalence, multi-player symmetric maps, benchmarks at 100/500/2,000 entities. |
| Complete | Controller/archive breadth | `crates/server`: teams/capacity in the lobby, bounded byte-accounted channels, retention/eviction with regeneration, archive resume/replay CLI, stats bucketing, failure injection. |
| Complete | Browser breadth | `client`: group-edit/binding chords, graphs, per-round replay viewer, before/after summaries, lock-effect previews, reconnect polish and multi-player rendered review. |
| Complete | Local browser harness | Extend `client/tests/ui/slice.spec.mjs` rather than the scaffold test; keep screenshots as review evidence. |
| Done | Native peripheral | `crates/runner` over the server library's sim adapter and revision store; `--inputs-only` controller route; see [development](development.md). |

The coordinator retains shared content/contracts and merges changes sequentially. Compact transport, exact-state reconstruction and real lock application should be settled in the slice before separate owners depend on them. These assignments do not authorize an early user playtest.
