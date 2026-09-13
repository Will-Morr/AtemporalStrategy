# Adversarial review and unresolved risks

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

An independent review of the source prompt examined determinism, identity, time travel, termination, victory, and persistence. This document records the disposition, including proposals that still need user review.

| Finding | Disposition | Falsification scenario |
| --- | --- | --- |
| Zero factories makes victory true at initialization | Resolved by user: add starting turret, use active-building elimination | Publish the initial future without any orders; nobody should get a spurious point |
| Finite ore cannot guarantee termination | User-directed inactivity window with future-event/cooldown guards and generous backup horizon | Two scouts pursue/move forever with no ore |
| Timed advancement is ambiguous | User confirmed fixed ticks per completed planning round; pre-tick checkpoint representation remains proposed | Victory at tick 20 is provisional until `L >= 21` |
| Scoreboard ending/pass farming | User confirmed scoring each resolved round including passes, with configurable default target 5; user also confirmed that survival awards require at least one elimination | A player passes while the published future remains a loss; show exactly who scores |
| Sequential spawn identities misdirect historical orders | Stable causal IDs, missing references explicitly skipped | Earlier queue insertion must not retarget a future tank order to a grunt |
| Delayed birth can miss a historical command | Intentional no-op, visible diagnostic; no deferred rebinding | A factory builds the same causal unit after its old command timestamp |
| Checkpoint convention duplicates same-tick input | `S[t]` before tick input, all commands applied once | Edit exactly at a checkpoint tick |
| Floating sums depend on worker ordering | Parallel read-only intents, ordered serial reductions | Vary worker count and scheduling; compare hashes |
| Native floating point is not universally portable | Same supported build/target only until tested | Incompatible peripheral rejected before playback |
| Construction fraction accidentally heals damage | Preserve current HP, increment only funded health | Damage a half-built site, continue funding, verify damage remains |
| Even division can strand matter unnecessarily | Capped equal-share water filling per priority | Two consumers demand 1 and 9 with bank 6: receive 1 and 5 |
| Factory blocked output can duplicate/overcharge births | Paid pending item, one stable birth identity | Block/unblock output over checkpoint/replay |
| Crowd collision can jam behind idle allies | Proposed deterministic one-step allied displacement, legal swaps and bounded stuck fallback | Idle blocker, crowded output and mixed walker/vehicle traffic resolve without overlap |
| Infinite healing can prevent battle resolution | Horizon still applies; no initial healer | Configured free healer/tank stalemate reaches cap |
| Replay inputs alone lose scores and human time | Durable turn records plus round metadata | Crash after result save but before broadcast; no double score |
| Timeline data can dominate runtime/memory | Sampled chunks, overview aggregates and bounded caches | Long default run with many units does not buffer all full snapshots in RAM |
| Exact three-way square-grid rotational symmetry impossible | Reject symmetric three-player setup, allow asymmetric 3-player | Setup validation gives an actionable message |

The earlier review suggested scoring only nonempty legal turns. The user subsequently confirmed awarding each resolved round, including passes, and a configurable five-point target in scoreboard rules. This supersedes that suggestion. Optional multiplayer lead-N victory remains available; team stalemates award zero by subsequent user confirmation; tie handling is now user-required configuration, defaulting to continued play until a unique side qualifies.

The user resolved the opening problem with a starting turret and an active-building victory condition, and confirmed simultaneous turns plus fixed-tick timed advancement. The user confirmed “active” requires completion; the plan also checks that entities are alive and makes survival eligibility a content capability. Add tests for walls-only survivors, unfinished factory sites, same-tick turret loss/factory completion, and simultaneous last-building destruction. The user superseded inert remnants: multiplayer units continue their orders and can restore their owner during the same simulation. Score endpoint status, not whether elimination ever occurred.

Potential UX tradeoff: stable historical assignments may become no-ops, and later old assignments can override a newly issued early order. Show both on the timeline and in a command inspector. The user’s future-order replacement requirements now explicitly allows dropping all future orders or a configured following interval. Show the chosen policy and affected components; retain future orders when Keep is selected.

Potential performance tradeoff: current pathfinding design is intentionally simple and may queue badly in corridors. Automated choke-point scenarios run during implementation; user playtesting waits for the full planned game. The displacement/stuck-response baseline below replaces reciprocal-only swaps, with full/checkpoint replay equality checks.

The earlier pass considered the architecture ready for contract review. The [2026-09-12 review](#review-2026-09-12) below identifies additional issues to settle before treating the contracts as ready for independent implementation. No implementation has been performed.

The user refined termination after the first review: inactivity is the normal cutoff; the absolute horizon is a generous backstop. This is accepted, with the explicit correction that finite population/commands do not exclude persistent-order cycles. Regression scenarios must include productive peaceful mining, a blocked factory, a future order after a long quiet interval, a weapon cooldown longer than the stall window, mutual support movement, and full/checkpoint replay across an inactivity deadline.

A second review of the written plan caught worker repositioning, conflicting event precedence, defeated-player participation, competing blueprint funding, queue occurrence timing, and peripheral loading-time accounting. These now have explicit rules in the decision/architecture/contracts files. Inactivity endpoints can accept a new order at the stopped state; elimination-based early endpoints have been removed. Timed runs carry a minimum end tick so inactivity does not freeze boundary advancement.

## Review of the user’s gameplay requirements

These are desk-review consequences of the new requirements, not a new independent agent review:

- The explicit OR predicate takes precedence over the broader “any player ... at least one constructor” motivation: a constructor still requires an active turret/factory. Record that distinction; do not silently switch to AND.
- Presence of a factory counts as building ability even when unfunded or output-blocked. Viability/path/resource analysis would add an unrequested loss rule.
- Three simulation results are independent of stopping cause. Empty survivors is draw; all survive is stalemate; partial survival is win. A partial FFA elimination can produce several scoring survivors.
- Stopping at the first elimination would undercount subsequent team casualties. The user requires continuing beyond elimination regardless of current surviving-side count, allowing further casualties and recovery.
- The former single-winner result shape and factory-only survival assumptions are superseded. Replay and score reduction need survivor arrays, team IDs, and a vector of same-round deltas.
- Team scores use survivor count only when at least one player was eliminated, per team scoring rules; stalemates always award zero, while draws now use configurable none/all-player awards. Equal survival increments cannot break a tied lead, which is consistent with lead-N victory.
- Shared banks/control remain proposals. Completed-only eligibility and continuing orders after elimination are now user decisions.

## Future-order replacement review notes

The future-order replacement requirements supersedes the earlier always-retain-future-orders proposal. Proposed suppression records preserve replay auditability without limiting the requested editing behavior. Critical cases are partial group removal, exclusive current-tick/inclusive window-end semantics, preserving same-round new commands, and evaluating inactivity against effective events. Suppression must be revision-scoped rather than a persistent prohibition on future commands. Removal of future factory settings versus only action assignments remains a disclosed scope proposal; active queues and general blueprint events are not implicitly erased. Original archived user inputs remain unchanged.

## Control groups and newborn inheritance

The control-group requirements turns control groups into simulation state, replacing the earlier browser-only/frozen-selection assumption for persistent group commands. Explicit multi-unit selections remain frozen ID lists. Desk-review cases:

- Reapplying saved orders every tick would erase individual overrides. Apply once on group-event delivery, and separately once at each actual birth.
- “Latest” must follow simulated tick/event precedence, not archive arrival, or time travel would give newborns orders from their future.
- Resolving group members only at draft time would omit units whose births move earlier after a rewrite. Resolve normal group delivery at execution; keep removal masks fixed to the reviewed base revision.
- Saved orders, membership and output bindings belong in checkpoints and hashes, including orders to currently empty groups.
- A blocked fully paid factory item must inherit at spawn, not funding completion. A changed binding does not move existing members.
- Suppressing one member’s delivery must not clear the saved group order for all newborns. Whole-group-event removal must suppress both components.
- Output-binding cardinality, factory-template precedence, membership-edit turn cost and overlapping-group behavior are transparent proposals, not user-locked restrictions.

## Visual design consistency

The user’s visual requirements adds minimal animation explicitly. Cosmetic projectiles fit the current tick-resolved combat proposal; do not silently convert them into authoritative delayed-hit mechanics. Facing must survive checkpoint/seek and cannot be inferred only from two adjacent sampled snapshots. Circular units need a directional detail for rotation to be visible. Short attack events need retained positions and playback-time lifetimes so effects remain meaningful after a target dies, on pause, and after a historical rewrite. “Last turn” is currently interpreted as last successful simulation movement, not planning-round camera state; this interpretation remains revisable.

## Recovery, draws, and score ties

The user explicitly removed the elimination early-stop shortcut. This resolves two failures: prematurely declaring a win before residual units cause mutual elimination, and freezing out a constructor that later completes a recovery factory. No player-status flag may gate orders, queues or group behavior. Even an all-eliminated intermediate state may later change; classify only at the actual inactivity/horizon endpoint. A fully recovered field is a non-scoring stalemate. Draw/all_players is a separate award path with an empty survivor set, not survivor scoring. Configurable score ties default to continued play; alternate shared winners require a result array. In timed mode immutable historical status failure is not proof of permanent defeat. The earlier inert-remnant and terminal-side-count rules are superseded throughout the plan.

## Player guide, lobby, and restart review

Compile-time static stats alone can drift when runtime YAML or an archived replay uses different content. The proposed shared loader/generator and content-keyed static fallback address that conflict without a second stat catalog. Shared data does not automatically validate authored mechanics prose; include it in gameplay acceptance. A lobby needs full snapshots for late joiners, serialized profile/start handling, and durable identities; names/colors are not player IDs. Team selection on the landing page supersedes the older fixed-YAML-assignment proposal. A CLI port must be used by both assets and WebSocket, with no silent alternate-port fallback; a fresh process requires fresh bootstrap even if revision numbers repeat. These are planning checks, not an implemented site/server.

## Review 2026-09-12

This is a desk review of the current specifications and all planning documents, not an implementation audit or performance measurement. The recommendations below are new assistant proposals; they do not amend confirmed gameplay requirements. Causal identities, ordered simulation reductions, pre-tick checkpoints, revision-scoped suppression and atomic round publication remain sound foundations. The original review highlighted rules at the match boundary, basic army behavior, and an early human test. The user subsequently rejected the early human test; see the disposition below for the other findings.

### R1 — High: timed match closure is not an executable rule yet

Evidence: [timed decisions](decisions.md#end-conditions-scoring-and-time) and [editable interval/minimum end](contracts.md#browsercontroller-protocol).

Let the old boundary be L and its increment be d. The next simulation must reach at least L+d, which is also the next boundary. Its newly computed endpoint therefore cannot fall strictly behind that boundary. A check requiring the new endpoint tick to become immutable can chase a moving endpoint until the cap. Conversely, treating equality as sufficient needs an explicit match rule: S[L] is fixed, but an order at tick L is legal and can still restore a player in a later state. Neither an inactive endpoint nor an immutable elimination proves permanent defeat under the recovery rules.

“Or reaching the absolute horizon” also needs to distinguish the simulation endpoint reaching max_tick from the immutable boundary reaching it. The former can happen on an early round while almost the entire battle is still editable. Closing then could remove the very opportunity to rewrite the result. At a capped multiplayer endpoint, several opposing sides may survive; there is no timed-mode match-winner reducer specified for that case.

Recommendation: define one controller transition with explicit old/new boundary, simulated endpoint, finalization tick and match-winner mapping. The simplest baseline to consider is closing only when the boundary reaches the fixed horizon. Earlier closure would be an additional gameplay adjudication rule, not a simulation optimization or a proof that recovery is impossible. This choice needs user input. Check a quiet recoverable loss, an endpoint equal to the boundary, a first-round capped moving cycle, and multiple surviving sides at final closure.

### R2 — High: movement shortcuts permit permanent jams, not just slower queues

Evidence: [motion and AI shortcuts](architecture.md#motion-and-ai-shortcuts).

The plan rejects movement chains into occupied tiles, allows only mutual swaps, and does not specify how soft-obstacle costs change after repeated failure. A unit can repeatedly choose a shortest route through an idle ally despite a slightly longer free route. The idle ally does not request a swap, so retries alone need not resolve it. A full movement cycle cannot advance under the explicit rule. This is especially consequential in cave corridors and at factory outputs: an army can lose because its navigation never recovers.

Recommendation: include a deterministic stuck-response rule in baseline movement. Start with occupancy-aware local detours after bounded failed attempts; consider resolving dependency chains ending in an empty tile in a single pass. Full crowd simulation is unnecessary. Specify what happens when no detour exists and when many selected units share one destination. Fixtures should include an idle blocker with a free bypass, a packed corridor following a moving leader, opposing allied traffic, and a factory output feeding a crowd. Treat failures as gameplay blockers before balancing units.

### R3 — High: bounded simulations do not imply bounded matches

Evidence: [scoring and tie rules](decisions.md#end-conditions-scoring-and-time) and [match configuration](contracts.md#common-records).

The fixed max_tick bounds each simulation, but scoreboard rounds have no corresponding bound. Example: in a three-player FFA, A is eliminated and B/C have equal scores. Repeated identical results award B/C one point each. Neither the default unique-leader rule nor lead-N will ever end that sequence. Repeated stalemates also award nothing forever; equal all-player draw awards cannot break equal-size-team ties. Players might change the battle through earlier inputs, but the rules do not guarantee they will or that an effective change exists.

This follows from confirmed pass scoring and continued-play ties; it is not a reason to silently override them. Recommendation: ask whether an explicit optional round limit, agreed draw, or manual end-match outcome should bound playtests, and document the experience when no limit is selected. Keep any such adjudication separate from battlefield draw, which already means all players eliminated. Also disclose unequal-team scoring advantages. Check indefinitely repeated win/stalemate/draw results independently of sim termination.

### R4 — High: ordinary combat behavior needs a small, complete specification

Evidence: [action/motion phases](architecture.md#two-phase-tick), [AI](architecture.md#motion-and-ai-shortcuts), and [order types](contracts.md#orders-and-drafts).

AttackMove is described, but there is no explicit engagement table for Idle, destination reached, mining/construction and static turrets. “Within vision/weapon constraints” does not settle whether an enemy visible outside weapon range is acquired and chased. Support references the target's currently engaged target without defining whether that is the previous state or a same-phase result. Movement is prohibited on ticks that actually fire, leaving unspecified whether an in-range unit holds position during weapon cooldown or resumes moving. Different reasonable implementations produce substantially different combat and potentially different parallel evaluation dependencies.

Recommendation: define automatic turret fire, idle-unit defense, target acquisition/retention/loss, chase/leash behavior, and movement while in range/on cooldown. Make Support read a specified snapshot field rather than recursively depending on another unit's unfinished intent. Use a few tiny fixtures: an idle turret sees an enemy; an attack-move unit reaches its destination; an enemy is visible but out of range; an in-range attacker is cooling down; two units mutually support. These are engine semantics to settle before agent handoff, not balance tuning.

### R5 — High: the first human test is too late for the experiment's purpose

Evidence: [implementation sequence](implementation.md#nested-implementation-checklist) and [planning flow](client.md#planning-flow).

The checklist puts broad mode coverage before the first explicit human playtest, which appears under final hardening. The riskiest product assumption is whether people can understand a long future, choose one useful earlier tick, and understand what their rewrite changed. Correct no-op diagnostics alone do not establish that this is enjoyable. There is no defined acceptance observation for planning duration, useful rewrites, or how often lost/delayed births make intended commands disappear.

Recommendation: move a two-human 1v1 test immediately after the first end-to-end factory/army/rewrite path. Use one hand-authored map and a reduced roster for this intermediate milestone; retain the requested full roster, procedural maps, variants, graphs, guide and peripheral in the overall plan. Include basic groups/newborn inheritance and future-order replacement early enough to test command usability. Observe several rewrites before expanding coverage. Add a compact previous-versus-new result summary (casualties, production changes, no-op commands and outcome), and links from changed/no-op orders to the relevant tick. No speculative live simulation is necessary. Milestone resequencing is a proposal, not a reduction of the user's requested final scope.

### R6 — High: performance acceptance excludes expensive parts of the actual round

Evidence: [performance and data delivery](architecture.md#performance-and-data-delivery), [simulation boundary](contracts.md#simulation-boundary), and [integration gates](implementation.md#integration-gates-and-meaningful-checks).

The two-second target covers 12,000 active ticks, whereas the proposed cap is 100,000. A moving cycle can require that much work, and an early rewrite invalidates nearly the whole future. Worker startup, checkpoint transfer, event export, compression, disk flush, exact-state reconstruction and browser readiness all add to the wait. Bounded snapshot caches do not bound per-tick event/stat storage or the disk space retained across arbitrarily many revisions. “Browser slowness cannot block simulation” also leaves the durable-output backpressure policy unspecified: required replay data must be drained, safely spooled, or allowed to throttle the worker; it cannot simply be discarded like progress updates.

Recommendation: benchmark commit-to-playable-publication and exact-tick seek latency, with realistic history export enabled, before full subsystem expansion. Include a cap-length cyclic run, a dense choke point, and repeated near-zero rewrites. Record peak memory, bytes per revision, disk use and latency, with explicit worker-output backpressure. Preserve inputs/results required by the user; distinguish regenerable caches from authoritative records before choosing retention. Do not assume that adding threads to AI will fix serialization, pathfinding or history costs.

### R7 — Medium: generic future-order removal has surprising cross-setting effects

Evidence: [removal scope](contracts.md#future-order-suppression-contract) and [group suppression](contracts.md#persistent-control-groups).

The current proposal allows a priority or queue-setting command carrying DropAll to remove future movement orders as well as future settings. A player trying to change spending priority could erase the army's future attack. Deleting a later suppression source does not restore what it previously removed; this is internally coherent as revision editing, but differs from causally replaying all commands as ordinary in-world actions. Member-level suppression also preserves the group's saved order, so future births can still inherit it. These choices are not user-locked.

Recommendation: default non-action settings to Keep and consider offering replacement only with action assignments, with advanced category removal exposed separately if useful. In all cases, preview deleted actions/settings and whole-group saved-order effects distinctly. Clarify that undo applies to the uncommitted draft and that later suppression of a replacement is not an undo of its earlier deletions. Validate the proposed UX with “change priority but retain march”, “redirect one group member”, and “replace a replacement” examples before locking schema behavior.

### R8 — Medium: several interface details still require incompatible guesses

Evidence: [contracts](contracts.md), [client controls](client.md#input-map), and [scope proposals](decisions.md#revisable-gameplay-proposals).

- A factory has a designated output tile, but no command/config/default rule chooses its direction or handles an output placed against rock. Specify deterministic selection and what the placement preview guarantees. A visible blocked-output warning alone does not make a structurally unusable factory playable.
- Group-removal preview must account for earlier staged membership edits, but PreviewFutureOrders carries only one draft_command. Send the preceding draft context, or specify an equivalent validated representation, so client preview and commit resolution agree.
- Commit validation reads exact S[t], but same-tick drafting can create blueprint IDs referenced by later cancellation/edit commands. Define whether sequential draft effects participate in validation and how temporary IDs become CommandId-based identities.
- Ctrl+digits and Alt+Left/Right conflict with browser tab/history navigation. Adopt browser-safe defaults and verify them in the actual browser rather than relying on global interception.
- The startup guide generator is shared infrastructure but content-loader ownership/build dependencies are not fully assigned. Choose one owner and avoid making generated documentation a prerequisite for bringing up the first simulation slice.

These are ordinary implementation decisions, not new permission gates. Resolve them in small contract fixtures before parallel subsystem work.

### R9 — Medium: inactivity guards need a precise eligibility predicate

Evidence: [dynamic stopping](decisions.md#end-conditions-scoring-and-time) and [cooldowns/retries](architecture.md#motion-and-ai-shortcuts).

The prose correctly warns that finite matter does not prove termination, but a “known pending cooldown” must not include every future retry. Otherwise an unreachable mover can keep scheduling its next retry and postpone inactivity until max_tick despite making no progress. Conversely, a legitimate slow attack or mining action must not be cut off while waiting. “Movement toward an assigned objective” also needs a concrete definition: a necessary path detour may temporarily increase geometric distance, while pursuit cycles can count as movement indefinitely.

Recommendation: distinguish pending work that can become executable through passage of time alone from retries that require an external state change. Specify the activity predicate and guard ordering in one reducer; compare normal ticks with fast-forward and checkpoint runs. Test an unreachable target retrying forever, an unaffordable blocked factory, a long legal cooldown, and a necessary detour. Leave cycle detection optional, but make cap-triggering behavior visible in playtests.

## Disposition after user review

The user explicitly discounted R5's proposed early human playtest and requested a simple allied displacement solution for R2. The original R1–R9 findings above remain as the review record, not the current implementation instructions. This pass reviewed each finding and revised the affected plans; no code or benchmark evidence exists yet.

| Finding | Assessment and action | Status |
| --- | --- | --- |
| R1 timed closure | Valid: an editable cap endpoint is not immutable history, and recovery defeats elimination-based proofs. User chose a different final rule: locked constructor/factory absence. Separate that from active-building battlefield elimination and reduce team eligibility explicitly. | User answered; candidate controller transition and fixtures recorded. |
| R2 jams | Valid, but a complex crowd solver is unnecessary. Adopt proposed adjacent allied displacement or legal source-tile swap without reciprocal orders, deterministic arbitration, one movement per entity, and bounded occupancy-aware fallback. | Concrete proposal in architecture; fixture verification required. |
| R3 unbounded matches | Correct consequence of confirmed pass/tie rules, not a bug that authorizes changing them. User chose no round limit and manual stop/archive. An administrative stop is never a battlefield draw. | User answered; no round cap. |
| R4 combat | Valid contract gap. Added an engagement table, hold-on-cooldown, target retention/loss and snapshot-based Support. | Concrete proposal; tiny-world fixtures required. |
| R5 early human test | Rejected by user. Full current scope precedes their first playtest. Retain useful automated checks and previous/new result summaries. | User decision recorded; no early-human gate. |
| R6 performance/storage | Valid. Measure full commit-to-playable and exact seeks with capped runs/export enabled. Bound required output queues, throttle on slow disk, fail safely on disk-full, evict only regenerable caches. | Concrete proposal; measurements remain implementation work. |
| R7 removal surprise | Valid. Only action assignments can initiate all/window removal; settings preserve future inputs. Previews still disclose every removal category and group saved-order effect. | Concrete proposal; no change to required optional future removal. |
| R8 interface gaps | Valid. Defined deterministic output direction/preview, full ordered draft context, temporary ID resolution, browser-safe chords and shared content ownership. | Concrete contracts; integration fixtures required. |
| R9 inactivity | Valid. Only otherwise-executable time-delayed work defers stopping; failed retries do not. Define activity once and compare normal/fast-forward/checkpoint execution. | Concrete proposal; cap remains necessary for productive movement cycles. |

The earlier assurance that no product questions remained was too strong. R1/R3 were explicitly raised and answered: constructor-based locked timed defeat and manual archive without a scoreboard cap. Remaining reducer edge cases are disclosed implementation interpretations, not another blanket approval gate. Displacement reduces avoidable jams but is not a promise of routing through physically sealed terrain or of starvation freedom in every arbitrary crowd configuration. Test the expected corridor/output cases before calling the mechanic adequate.
