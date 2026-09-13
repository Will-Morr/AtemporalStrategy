# Decisions and scope

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

## Status

This is a proposed implementation specification for a short-lived playtest prototype. It preserves the requested mechanics while avoiding production infrastructure. The user resolved the core choices in Q1–Q3 below. Assistant interpretations attached to those choices remain provisional. Q4 records confirmed per-round scoring, a configurable five-point target, and optional lead-based multiplayer scoring; remaining details are proposals. Q5 records constructor mining and FFA/team support.

| ID | Decision requiring user input | Recommendation | Consequence |
| --- | --- | --- | --- |
| Q1 — resolved | Opening and elimination | Each player starts with miner, constructor, and turret. Eliminate a player with no active building OR no ability to build (no constructors or factories). Initially turrets and factories count as active buildings. | Assistant interpretation: active means completed and alive; walls and sites do not count. The user has not settled partial-site eligibility. |
| Q2 — resolved | Turn sequencing | All players plan simultaneously against the same published timeline; reveal orders and simulate only after everyone commits. Explicitly reconfirmed in [the user’s simultaneous-turn rules](../user_spec/gameplay.md#simultaneous-turns). | One round contains one turn per player; network arrival order never changes outcomes. |
| Q3 — resolved | Timed history advancement | A fixed number of ticks per completed planning round, confirmed in [the user’s timed-mode rules](../user_spec/gameplay.md#timed-mode). | The increment is configurable; advancing per completed round is a user decision. |
| Q4 — confirmed core rules | Scoreboard and outcomes | Award each resolved round, including passes; first to 5 points, configurable, per [the user’s scoreboard rules](../user_spec/gameplay.md#scoreboard-and-teams). Retain three outcome kinds, optional lead-N multiplayer victory, and team survivor scoring from gameplay requirements. | User confirmed survival points require at least one elimination (team scoring rules). Fixed-target ties, default lead margin N, and penalty details remain proposed. |
| Q5 — resolved | Mining and multiplayer | Constructors mine at 50% of dedicated miner rate; support FFA and team play. | Team ownership, resource sharing, and other alliance mechanics below remain proposals. |

Follow-up requirements are recorded in [the user’s gameplay requirements](../user_spec/gameplay.md). Keep interpretations distinct from those requirements; in particular, the explicit OR loss rule means a constructor alone without an active building does not prevent elimination.

## Revisable gameplay proposals

- Square tile world, one entity per tile, finite ore, no fog of war for the prototype. Vision limits acquisition, not information sent to clients. Cave rock blocks movement and direct fire; artillery uses an explicit indirect-fire tag. Projectile/explosion animations are user-required; authoritative projectile-flight mechanics are not currently proposed. Cosmetic shots visualize tick-resolved hits.
- Walkers move along eight directions without diagonal corner cutting. Vehicles use four directions. Integer tick cooldowns control speed and attack rates; each diagonal step costs one movement opportunity. A* uses Chebyshev or Manhattan heuristic respectively.
- Constructors build structures and mine at 50% of the dedicated miner’s rate, per user instruction. Both use the same mining capability and order; encode the rate in content, with equal mining cooldowns and half transfer amount proposed to guarantee the throughput ratio. A constructor performs its assigned mine or construct action, not both simultaneously. All mobile units can attack-move as a movement order; attacking is optional and capability-driven. All mobile units can support by following, with combat assistance available only to armed units. Support follows an ally, assists its target when legal, and heals only if the configuration grants a healing capability. The initial roster has no healer; do not add one implicitly.
- Build queues contain unit recipes, loop state, and a stored order template. One factory has one active build and a designated adjacent output tile. A complete unit waits internally when that tile is blocked. It occupies the output on a later legal spawn opportunity and receives its bound control group’s latest order if present, otherwise the factory’s stored order (proposed precedence). No extra matter is charged while waiting. An idle template is allowed with a visible blockage warning.
- Structure blueprints are player-owned queued requests on specific tiles. A constructor working a selected rectangle chooses the oldest reachable eligible blueprint, then the closest work tile with coordinates breaking ties. A blueprint reserves construction intent but does not block movement until work begins. Work begins only if its tile is empty. Constructors work from adjacent tiles; multiple constructors may contribute.
- Build sites are physical entities once funded. Their maximum/current health grows with paid fraction; existing damage is never reset by construction. Destruction removes paid matter; no refunds or automatic rebuild. Unstarted blueprints can be canceled freely. Canceling a started site removes it without refund. Factory queue edits preserve the active item unless explicitly canceled; canceling it loses its paid matter.
- Mining requires standing on ore; ore and entities can coexist. Mining transfers a configured amount every action opportunity, capped by remaining ore. No hauling, storage buildings, upkeep, or matter creation. Ore beneath structures cannot be mined.
- Resource priority is high/medium/low. It applies to consuming entities: construction-site priority for construction, factory priority for production, and healer priority if healing is configured. Spend within each tier is equal-share water filling, capped by each consumer's per-tick demand. Satisfy a tier before funding the next. Mining happens before allocation; stored bank is available immediately.
- Unit order lists are historical **assignments**, not movement command queues. A new assignment replaces the unit's current action when its event executes. Later historical assignments execute unless explicitly dropped using the user-requested future-order replacement option. Factories have the separate persistent production queue requested in the prompt.
- At one chosen tick per turn, a player may stage any number of assignments/settings/blueprints. Single-order mode permits exactly one command envelope; a group action or building line counts as one, while changing a queue and a rally template counts as two. A production-queue command may contain a whole queue. Empty turns are legal passes.
- Default 1v1; multiplayer supports FFA or configured teams per user instruction. A 2–4 player initial implementation remains a revisable scope proposal. Default symmetry is 180-degree rotational symmetry. For four players, use fourfold symmetry and equal start packages. Three-player rotational tile symmetry on a square grid cannot be exact: reject symmetric three-player configuration and explain how to use asymmetric mode. The 2–4 player bound and symmetry restriction are MVP proposals, not user-imposed limits; revisit them if a different map representation or larger playtest warrants it.
- Each start has one miner, one constructor, one completed turret, starting bank sufficient for a factory, reachable ore, and room for factory/output. The turret is placed without blocking initial workers or access corridors. Connected rooms are carved, joined by corridors, rotated as complete terrain/ore features, and validated for connectivity and start access. Generate asymmetrically from the same seeded pipeline when symmetry is disabled.

## Control groups

[The user’s control-group requirements](../user_spec/controls_and_visuals.md#control-groups) establishes groups selected with keys 0–9, factory binding so newborn units join the group and inherit its most recent order, and individual overrides that last until another group order arrives. A group order is one sent command for living members, plus stored state for future births; it is not reissued continuously.

Proposed representation: each player owns ten stable group slots, identified by `(player, digit)`. Membership and the latest group order are simulation state, because factory births and historical orders depend on them. A group command resolves membership when it executes at its scheduled tick in the rewritten simulation, applies once to living compatible members in deterministic ID order, and stores its order even when the group is empty. “Most recent” means scheduled tick plus canonical same-tick event precedence, not latest network arrival or latest planning round. Invalid member applications produce diagnostics without discarding the stored group order.

Individual orders never remove membership or change the group’s saved order. An existing member assigned a new individual target keeps it until a subsequent applicable group order. Newborns inherit the group’s saved order, even while an older member is individually overridden. Group membership may overlap; whichever applicable group/individual order executes last sets the unit’s current action. Adding an existing unit to a group is proposed to preserve its current action until a new group command; the user-required immediate inheritance applies to newborns.

Proposed factory binding is one output group per factory, independently of the factory’s own selection-group memberships. Several factories can feed the same group. At actual spawn into the output tile—not payment completion—read the current binding, add the newborn, and copy that group’s latest order. This takes precedence over the factory’s stored template; without a saved group order, use the factory template. A dead support target or incompatible inherited action falls back to idle with a warning. Changing/unbinding a factory affects later births, not existing group members. These precedence/cardinality choices remain revisable proposals.

Membership changes and factory bindings are proposed as timestamped, undoable draft commands so replay and peripheral simulation agree. Recalling a group is immediate local selection and never consumes a turn. Group-order submission is one command in single-order mode regardless of living member count or future births. Membership/binding edits are separate commands under the current control-limit proposal. Group state belongs to each player; team membership does not grant control of another player’s groups.

## Future-order replacement

[The user’s future-order replacement requirements](../user_spec/controls_and_visuals.md#future-order-replacement) requires an option to drop all future orders when giving new unit orders, plus a configured time-window option for rewriting a following span of ticks. The following details are implementation proposals, not additional locked decisions.

Each new entity-directed command carries a future-order policy: `keep`, `drop_all`, or `drop_window`. Proposed default is `keep`; the player chooses explicitly when drafting the new command. Setup YAML exposes `future_orders.window_ticks: null | positive integer`; null disables the window option, while W enables it. All-future removal remains available independently. Proposed window semantics are scheduled simulation timestamps `t < future_tick <= t + W`, clipped at the absolute horizon; this is not the wall-clock time or round in which an order was submitted. `drop_all` covers all scheduled ticks strictly after t. Existing same-tick commands retain normal precedence.

Scope removal to the new command's selected owned entities and previously committed commands visible in the draft's base revision. A historical explicit multi-unit command targeting A/B loses only A’s component when A alone is redirected. Persistent group commands use the additional rules below. Do not affect other players, unselected units, or new commands staged in the same round. An order later issued into a previously cleared interval is allowed: removal targets existing events rather than creating a lasting ban.

Proposed meaning of “all future orders” includes action assignments and entity-directed priority/production/loop/stored-order changes for the selected entities. General blueprint placement/cancellation commands are not implicitly removed. An active factory queue established at or before t remains running unless the new command edits it; deleting a later queue-edit event does not cancel already-active production. Expose the affected categories and counts in the preview so this scope can be revised based on playtesting.

Replacement plus its removals is one draft command and one single-order-mode action. Undo/redo restores both together before commit. Removal is a revision edit recorded with the committed command, not an in-world action conditional on the unit still existing when the rewritten simulation reaches t. Thus already-validated removal remains stable even if another simultaneous player's changes cause the replacement action to become a no-op. Preserve original archived events with explicit suppression records; historical round replays remain inspectable.

## End conditions, scoring, and time

The user specified dynamic stopping based on a configurable inactivity duration, with an ambitious absolute tick limit as backup. Set `stall_ticks` (proposed 300 at 10 ticks/second) and `max_tick` (proposed 100,000). Stop when **neither order progress nor a unit/building destruction** has occurred for `stall_ticks`. This interpretation allows peaceful construction/mining to continue without combat. Reset the activity timestamp for successful movement toward an assigned objective, positive mining transfer, funded construction/production, completed births, effective damage/healing, and destruction. Pure target selection, blocked movement, unaffordable demand and queue inspection do not count. Continuous costs must make a positive state change, not merely issue an intent.

Do not stop ahead of a future effective input (after future-order suppression): a scheduled command may resume activity. If inactivity is reached with a future command, continue (or fast-forward safely) to that event and give it a full activity window. A known pending cooldown or in-flight completion that can next progress later than the inactivity window also defers stopping. If no intervening state can change, jump to the earliest such tick; otherwise execute normally. This must produce the same result as ordinary ticks. Derived empty timeline ranges retain state and are exposed as inactive intervals, not missing history.

An inactivity stop is a gameplay cutoff, **not a proof that no possible future progress exists**; record `stop_reason: inactivity` separately from the stalemate/win/draw result. Finite units and finite player inputs do not alone prove finite simulation: persistent support/pursuit orders and movement cycles can keep changing state. The backup horizon bounds these cases. Keep a fixed absolute `max_tick` for the match so advancing the immutable boundary does not extend it indefinitely. Report the reason and last progress tick visibly. Proposed early stop: no surviving players, or only one surviving opposing side (one player in FFA, one team in team mode). An elimination alone does not stop a multiplayer run while opponents remain; continue to inactivity or the backup horizon so later eliminations affect the final survivor count. No sophisticated cycle detection is needed for the MVP.

Elimination is evaluated for every player after a whole tick resolves, using the user's predicate:

```text
has_active_building = at least one living active turret or factory
has_build_ability = at least one living constructor or factory
eliminated = NOT has_active_building OR NOT has_build_ability
```

Proposed capability mapping: `counts_for_survival` marks active buildings and `provides_build_ability` marks constructors/factories. Interpret active/ability-bearing entities as completed and alive; partial-site eligibility remains an assistant interpretation. This is a presence test, not a strategic solvability test: blocked output, empty bank, or exhausted ore do not remove a factory's ability flag. Hidden scouts, miners, walls, and other cheap combat units cannot independently prevent elimination. A turret plus constructor survives; a turret plus only a miner loses; a factory survives both tests without a constructor. All deaths, completions and births in the tick precede simultaneous elimination evaluation.

Proposed eliminated-entity handling remains inert blockers for that simulation branch. Earlier rewrites can restore the player; battlefield elimination does not remove their planning slot. All players keep committing until the match ends, including players whose published future is a loss. A player eliminated in immutable history can pass but cannot resurrect entities after the boundary.

There are exactly three simulation outcome kinds. In 1v1 these match the user's definitions directly; the proposed multiplayer extension is:

| Kind | Final survivor set | Scoring interpretation |
| --- | --- | --- |
| `stalemate` | Everyone survives; nobody was eliminated | Zero points; team survival awards require at least one elimination. |
| `win` | At least one player eliminated and at least one survives | Multiple survivors may win/score together, even on opposing FFA sides. |
| `draw` | All players eliminated | No survivor points. |

Stopping reason is independent: elimination resolution, inactivity, or absolute horizon. A three-player run where A is eliminated and B/C remain at inactivity is a `win` with B/C surviving, not a stalemate or draw. A capped run with everyone alive is a `stalemate`, without claiming progress is mathematically impossible. Draws are reserved for all-player elimination, including simultaneous eliminations on the final tick.

Proposed FFA scoring awards one raw point to every final surviving player on a `win`; eliminated players receive zero. The user requires team scores to reflect the number of surviving members: `team_delta = count(final survivors on that team)`. Per [the user’s team scoring rules](../user_spec/gameplay.md#scoreboard-and-teams), award these survival points only when at least one player was eliminated. Thus `win` awards each team its surviving-member count, `stalemate` awards zero, and `draw` awards zero because no members survive. Multiple teams can score in one round. Keep survivor counts separate from adjusted points. Score is meta-history: rewriting battlefield history does not revoke points from previous resolved rounds. Per user confirmation, award once per resolved round, including passes: an unchanged winning timeline continues earning points.

Scoreboard configuration offers `fixed_target{points}` or `lead{margin: N}`. Lead mode is user-required: player/team X wins the match only when `score[X] - max(score[other sides]) >= N`; N must be positive. Compare against every rival, not the sum or lowest score. In teams, the side is the configured team. All same-round score deltas are applied before checking this condition. The user-confirmed default is fixed-target mode with 5 points, configurable. Optional lead mode remains available for multiplayer; its default margin N is proposed as 5 and is not user-confirmed. Selecting raw versus time-adjusted totals follows the penalty setting. Proposed fixed-target tie behavior: declare a match winner only if a unique highest-scoring side has reached the target; tied leaders continue. A match win and a simulation `win` are separate records.

Team behavior beyond survivor scoring is proposed: fixed YAML player-to-team assignments, at least two nonempty opposing teams, friendly target filtering/support/swaps, no friendly fire, separate player banks and unit ownership, and no shared control/construction unless later requested. A teammate's factory does not satisfy another player's individual survival test. Equal-size teams are recommended for comparable survival scores but not mandated; show the advantage of unequal team sizes in setup. Symmetric map start assignment should give teams comparable layouts as well as individually fair starts.

For timed mode, the user requires boundary advancement by a fixed `lock_ticks_per_round` per completed planning round. Events at ticks `< L` are immutable and `S[L]` is the replay base. Individual eliminations becoming immutable do not finish a multiplayer match while multiple opposing sides remain. A sole surviving side or all-player elimination becomes final when its terminal tick enters the immutable prefix. At the absolute horizon, finalize with the simulation's survivor/result record; multiple survivors can remain, so do not relabel a stalemate as a draw or invent a unique winner. This timed multiplayer closure policy remains a proposal.

Timing uses server monotonic elapsed time from per-client planning readiness to accepted commit, following the readiness/reconnect contract. Disconnected thinking time counts; server simulation/loading time does not. Persist elapsed durations; replay never recomputes them from wall clock. Crash recovery restarts any incomplete planning window, retaining accumulated elapsed time through the last saved timing record. This prototype does not claim subsecond crash accounting.

Display every player's total thinking time and ratio to the fastest player (`max(total, 1 second)` denominator), regardless of penalty mode. Proposed penalty: on a scoring round, add `min(1, min_opponent_time / own_time)` using the same one-second floor; keep raw points and adjusted points separately. Faster players are not awarded more than one point. For multiple opponents use the fastest opponent; label the choice explicitly. For teams, proposed adjusted delta sums the per-survivor factors, comparing each survivor to the fastest opposing-team player; teammates are not time-ratio opponents. Raw team delta remains the required survivor count. Timing cannot alter battlefield simulation or determinism.

## Initial configuration roster

Keep all mechanics capability-driven: movement kind, vision, weapon, mining, construction, production, healing, and static traits. Type names are content keys, never branches in engine AI. Exact balance numbers are tuning values and can be selected in the content implementation.

| Type | Movement | Relative role | Capabilities |
| --- | --- | --- | --- |
| Constructor | Walker | Builder and fallback extractor | Move, construct, mine at 50% miner rate, provides_build_ability |
| Miner | Vehicle | Dedicated extractor | Move, mine |
| Tank | Vehicle | Expensive, durable, moderate damage | Move, direct attack |
| Artillery | Vehicle | Fragile, slow, long range | Move, indirect attack |
| Scout | Walker | Fast, high vision, weak damage | Move, direct attack |
| Grunt | Walker | Cheap general combat | Move, direct attack |
| Grinder | Walker | High melee damage | Move, adjacent attack |
| Factory | Static | Produces mobile roster | Produce, queue, stored order, counts_for_survival, provides_build_ability |
| Turret | Static | Long-range defense | Direct attack, counts_for_survival |
| Wall | Static | Durable obstacle | None |

Game setup is YAML loaded at restart, with seed, map size, players, FFA/team assignments, symmetry, objective, fixed-target/lead scoring, horizon, lock speed, control limit, future-order window, inactivity window, timing penalty, starting bank, simulation threads, transport mode, and replay directory. Unit content is separately versioned YAML. Validate finite positive costs/health, nonnegative damage/rates, positive cooldowns, valid recipes, and meaningful map sizes. Pin configuration and content snapshots into each replay; restarting with changed files affects new matches only.

No accounts, matchmaking, cloud services, tech tree, sophisticated animation, ranked security, cross-version replay migration, or unbounded game length. Input-only replication remains a secondary milestone after the authoritative playable path, not a reason to delay the first playtest.

For future-order replacement, proposed group targeting has two effects: suppress future order effects on the group’s current members (membership frozen from the draft base for this removal set), and suppress future commands explicitly addressed to that group within the chosen interval. Suppressing an entire group-order event prevents both its one-time delivery and its saved-order update, including for future newborns. Redirecting just one member can instead suppress that member’s delivery from a historical group order while preserving other members and the group’s saved order. No group removal policy automatically covers units joining later for unrelated individually addressed orders. The UI previews member-level versus whole-group removal separately. Membership and factory-binding changes are not implicitly deleted by an order-replacement policy; this is a disclosed scope proposal.

## Visual direction

The user’s visual requirements requires grayscale floor/wall terrain, contrasting gameplay objects, minimal movement/attack/projectile/explosion animations, health bars, and unit rotation toward the last movement direction. Exact palette, effect durations and facing interpolation are revisable client proposals. These requirements supersede any reading of “minimal art” as no animation. The current simulation still resolves damage per tick; visual projectile flight is proposed as presentation only.

## Remaining product questions

These can be answered while implementation planning proceeds; the recommendations are not locked decisions.

| Question | Current proposal | Why it matters |
| --- | --- | --- |
| Do unfinished turrets/factories satisfy active-building survival, and do unfinished factories provide build ability? | Only completed, living entities count. | Determines whether a last-second construction site can prevent elimination. |
| What happens to remaining units/buildings when their player is eliminated? | They stop acting and remain as inert blockers for that simulation branch. Alternatives include removal or continuing existing orders. | Changes subsequent casualties and team scores in multiplayer. |
| If several sides reach the fixed score target together with equal top scores, does the match have shared winners or continue? | Continue until one qualifying side has a unique lead; optional lead-N mode already has an explicit margin rule. | Multiple sides can score in the same round, so first-to-5 alone does not resolve simultaneous ties. |

Other undecided items (exact inactivity counters, group/template precedence, penalty formula, visual timing and balance defaults) have documented revisable proposals and need not be resolved through more user questions before further work.
