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
| Crowd collision can create expensive cyclic resolution | Only empty moves and explicit allied swaps initially | Three-unit cycle stalls without overlap or recursion |
| Infinite healing can prevent battle resolution | Horizon still applies; no initial healer | Configured free healer/tank stalemate reaches cap |
| Replay inputs alone lose scores and human time | Durable turn records plus round metadata | Crash after result save but before broadcast; no double score |
| Timeline data can dominate runtime/memory | Sampled chunks, overview aggregates and bounded caches | Long default run with many units does not buffer all full snapshots in RAM |
| Exact three-way square-grid rotational symmetry impossible | Reject symmetric three-player setup, allow asymmetric 3-player | Setup validation gives an actionable message |

The earlier review suggested scoring only nonempty legal turns. The user subsequently confirmed awarding each resolved round, including passes, and a configurable five-point target in scoreboard rules. This supersedes that suggestion. Optional multiplayer lead-N victory remains available; team stalemates award zero by subsequent user confirmation; simultaneous fixed-target tie handling remains proposed.

The user resolved the opening problem with a starting turret and an active-building victory condition, and confirmed simultaneous turns plus fixed-tick timed advancement. The plan interprets “active” as completed and alive and makes survival eligibility a content capability. Add tests for walls-only survivors, unfinished factory sites, same-tick turret loss/factory completion, and simultaneous last-building destruction. Elimination makes remaining units inert for that branch, preventing a defeated army from changing later outcomes; earlier rewrites can reverse it.

Potential UX tradeoff: stable historical assignments may become no-ops, and later old assignments can override a newly issued early order. Show both on the timeline and in a command inspector. The user’s future-order replacement requirements now explicitly allows dropping all future orders or a configured following interval. Show the chosen policy and affected components; retain future orders when Keep is selected.

Potential performance tradeoff: current pathfinding design is intentionally simple and may queue badly in corridors. First playtest needs a choke-point stress scenario. Add local detours or canonical cached routes only after measuring the problem, with full/checkpoint replay equality checks.

The architecture is ready for contract review, but remaining scoreboard policies are still revisable. No implementation has been performed in this pass.

The user refined termination after the first review: inactivity is the normal cutoff; the absolute horizon is a generous backstop. This is accepted, with the explicit correction that finite population/commands do not exclude persistent-order cycles. Regression scenarios must include productive peaceful mining, a blocked factory, a future order after a long quiet interval, a weapon cooldown longer than the stall window, mutual support movement, and full/checkpoint replay across an inactivity deadline.

A second review of the written plan caught worker repositioning, conflicting event precedence, defeated-player participation, competing blueprint funding, queue occurrence timing, and peripheral loading-time accounting. These now have explicit rules in the decision/architecture/contracts files. Inactivity endpoints can accept a new order at the stopped state; terminal side-resolution endpoints cannot accept commands after that resolution. Timed runs carry a minimum end tick so inactivity does not freeze boundary advancement.

## Review of the user’s gameplay requirements

These are desk-review consequences of the new requirements, not a new independent agent review:

- The explicit OR predicate takes precedence over the broader “any player ... at least one constructor” motivation: a constructor still requires an active turret/factory. Record that distinction; do not silently switch to AND.
- Presence of a factory counts as building ability even when unfunded or output-blocked. Viability/path/resource analysis would add an unrequested loss rule.
- Three simulation results are independent of stopping cause. Empty survivors is draw; all survive is stalemate; partial survival is win. A partial FFA elimination can produce several scoring survivors.
- Stopping at the first elimination would undercount subsequent team casualties. The proposed simulator continues while opposing sides remain.
- The former single-winner result shape and factory-only survival assumptions are superseded. Replay and score reduction need survivor arrays, team IDs, and a vector of same-round deltas.
- Team scores use survivor count only when at least one player was eliminated, per team scoring rules; stalemates and all-eliminated draws award zero. Equal survival increments cannot break a tied lead, which is consistent with lead-N victory.
- Shared banks/control, unfinished-site eligibility, and inert defeated units remain proposals, not new user requirements.

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
