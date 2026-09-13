# Architecture and deterministic simulation

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

## Processes and modules

Use one Rust workspace with `contracts`, `sim`, `server`, and `runner` crates, plus a small TypeScript browser client. Server serves static files, HTTP bootstrap/archive endpoints and a WebSocket for live state. It owns slots, planning phases, accepted commands, scores, timing, and durable match revisions. No database; one match per server process is sufficient.

The runner binary has worker and peripheral modes. The server launches a worker subprocess per resimulation, using framed messages on stdin/stdout and stderr for diagnostics. The worker receives a checkpoint, event suffix, and pinned content; emits progress, snapshot batches, then final result. It cannot mutate the authoritative archive. A failed worker leaves the last published revision intact and permits retrying the already committed inputs. Cancellation closes the process; stale revision results are discarded.

Use a bounded worker pool *inside* the simulation for expensive read-only intent calculations, not separate processes per entity. Small simulations use a serial threshold to avoid parallel overhead. Start with ordinary native Rust floating-point `f64`, with finite values enforced and no unordered reduction or fast-math. Promise replay consistency for the same pinned build/platform; cross-CPU and native/browser bit identity is not assumed. Peripheral handshake rejects an incompatible build/target fingerprint. Browser clients do not independently simulate.

## Tick meaning and checkpoints

`S[t]` is state immediately **before** tick `t`. Commands stamped `t` are applied before that tick's actions. Running tick `t` yields `S[t+1]`. Initial state is `S[0]`; `max_tick` is exclusive. Timeline display distinguishes selected pre-tick state from events occurring during that tick.

Store periodic complete checkpoints (initial proposal every 100 ticks), sampled browser snapshots (every 5 ticks), and tick-indexed events/statistics. The server can request exact `S[t]` from the worker starting at the nearest checkpoint. Never interpolate positions for order validation. Sending commands requires exact selected-tick state; playback can interpolate visually.

To revise at `e`, load latest checkpoint `c <= e`, keep commands before `e` unchanged, execute `[c,e)` to reconstruct `S[e]`, then regenerate the suffix. Checkpoint spacing means some unchanged work is replayed; no changed tick is skipped. Reuse immutable prefix data and discard stale suffix snapshots, statistics, derived outcomes, and caches. If a new early terminal side resolution invalidates later orders, retain them in the ledger with explicit non-executed status.

Checkpoints include every future-affecting value: tick, terrain/ore, bank, entities, order state, last successful movement direction, cooldowns, queues and active progress, sites/blueprints, control-group membership/latest orders/factory bindings, elimination state, last-progress tick/inactivity state, deterministic spawn identity data, and any RNG state. Derived occupancy/path caches are rebuilt canonically. Do not retain hidden path or targeting state across checkpoint loads unless serialized.

## Historical orders and identity

Each accepted command is an append-only event, identified independently of simulation ticks. It targets stable causal entity IDs. Initial IDs derive from player and initial spawn slot. A constructed structure derives its ID from the blueprint command and index. A factory-produced unit derives its ID from factory ID, production-item ID, and loop occurrence. Delaying a birth must not change identity; deleting its cause makes subsequent references dormant. Numeric “next entity ID” allocation would silently redirect orders and is forbidden.

On command execution, absent, dead, wrong-owner or incompatible targets are skipped individually with a reason in command outcomes. Never rebind to a nearby replacement. Existing future historical assignments remain effective unless an explicit future-order replacement suppresses their selected entity components. At a shared tick order events by accepted round, persisted player precedence rank, then command index; explicit member IDs are sorted. For simultaneous rounds, player precedence rotates by round for conflicting general orders; persist the resulting precedence, independent of arrival timing. Settings and assignments on the same entity use the final valid command in that order.

Before allocation, competing unstarted blueprints on one tile are ordered by their creation event's canonical precedence. Only the first eligible blueprint can request funding that tick; others demand zero. An unfunded winner reserves consideration only for that tick, not physical occupancy. Positive first funding creates the site and blocks the tile; no two sites can be funded onto the same tile.

An edited old turn is **not** a mutation of an old archive: new turns append commands that affect earlier simulation ticks. Draft undo operates only before commit. The user-requested future-order removal is represented by append-only suppression records referencing prior command/entity components. Original archived events stay intact; full arbitrary branch editing remains outside the current proposal.

Future-order policies are resolved against the common published base revision at commit. The controller combines their exact suppression sets before generating the worker's effective event stream. An explicit multi-unit order may be partially suppressed without changing its IDs or other members. Persistent control-group commands additionally have independent saved-order updates; a member suppression preserves that update, while whole-group-order suppression removes it. Checkpoint selection and suffix invalidation start at the earliest replacement tick across the round, because removals occur strictly after their replacement tick. Bind checkpoints/results to revision; an older round's replay must not inherit newer suppressions. Future-event inactivity guards ignore suppressed events. Peripheral replication receives the same suppression records and derives the same effective events. Suppression resolution is a controller revision operation and does not depend on whether the replacement action later executes successfully.

Persistent control groups are part of the simulation rather than browser-only shortcuts. At command application, membership edits, factory bindings, group orders and individual orders follow canonical event order. Group commands resolve current living membership, save the latest order and write each compatible member’s action once; no per-tick group-order rebroadcast occurs. Each recipient applies through the same generic action assignment used by direct commands. Group slots and causal unit IDs remain stable when births move in time. UI previews of group membership are informative; actual replay membership can differ after earlier edits.

At successful factory output insertion, resolve the current output binding, register the newborn in that group, and assign its latest saved order (or proposed factory-template fallback). Existing member-specific overrides are unaffected. Checkpoint suffix replay restores saved group orders and bindings before new births; a changed earlier group command invalidates all later derived birth orders in the suffix. Both worker and peripheral derive these assignments locally from the same ledger/state, rather than emitting synthetic player commands for newborns.

## Two-phase tick

1. Apply command assignments and queue edits in canonical event order; expire invalid targets deterministically.
2. **Action:** take a read-only view; compute legal attacks, mining, construction and production demands in parallel into slots indexed by sorted entity ID. Build spatial occupancy/buckets once. Target choice orders candidates by current support target, distance, then ID. Grid line-of-sight uses one specified integer traversal with symmetric endpoint handling and a fixture for corner cases.
3. Apply mining transfers in stable tile/entity order; allocate each player's bank through priority tiers using equal-share capped water filling. Roundoff is clamped at zero and accounted within a documented epsilon. Allocation and reductions run serially in stable ID order. One funded site is one consumer regardless of how many constructors contribute; its demand is their summed legal build rates.
4. Resolve damage/healing/construction HP growth simultaneously from the action snapshot. `hp' = min(new_max_hp, old_hp + funded_health_growth + healing - damage)`. Mark deaths after summation; units alive at phase start still contribute their action. Complete sites/production and reserve possible births in stable causal-ID order. No healing creates matter; healing, if enabled, must use finite matter or be explicitly free in content.
5. **Motion:** surviving pre-existing mobile entities compute destinations from the post-action occupancy. A unit that performed attack/mining/construction waits this tick; factories/static attackers never move. Blocked or ineligible action attempts do not consume the movement opportunity. Newborn units first act/move on the next tick. Resolve movement deterministically, then materialize eligible factory births on vacant outputs. A spawn cannot displace an occupant. Successful spawn, not funding completion, advances the queue occurrence and re-adds a looping item to the queue tail.
6. Advance cooldowns/tick, evaluate both per-player elimination predicates, surviving sides and inactivity cutoff with future-event/cooldown guards, record statistics/events, and hash canonical state as scheduled.

Cooldown semantics are absolute `next_action_tick` / `next_move_tick`, advanced only on executed action/move (blocked moves retry with a small fixed configured delay). Command changes do not reset cooldowns. This prevents rewrite spam from bypassing speed limits.

## Motion and AI shortcuts

Use deterministic tile A* on terrain/static structures; keep neighbor order fixed. Cache terrain routes per entity/goal and invalidate on relevant structure changes. Cache is performance-only and cannot change decisions: either serialize the route or derive the same next step when restored. Start by deriving paths per movement opportunity; add caching only with equivalence tests. Pathfind to reachable adjacent work/attack tiles rather than an occupied entity's center. An unreachable objective waits and retries on a fixed tick schedule or relevant topology change.

Treat dynamic occupants as soft obstacles for route selection, then arbitrate destinations. For multiple claims use a deterministic rotating rank derived from tick and entity ID, not thread completion order. Initially allow moves into empty tiles and mutual allied swaps only; reject longer dependency cycles and chains into still-occupied tiles. Enemy swaps are forbidden. Reject intersecting diagonal crossings and corner cuts. These deliberate simplifications may cause queues; show blocked units and measure before adding expensive crowd behavior.

Team membership defines hostility (proposed fixed alliances with no friendly fire). Ownership still governs orders and bank spending; allies may be followed/supported and participate in allied swaps. Player survival is evaluated individually, not through teammates’ capabilities.

Attack-move acquires legal enemies within vision/weapon constraints, moves toward the destination or acquired enemy, and attacks when in range. Support follows a target to an adjacent/legal trailing tile and assists its currently engaged target. Constructors/miners choose eligible jobs in their rectangle with stable distance/ID tie breaks. No strategic AI opponent is required.

## Performance and data delivery

Use compact arrays and tile-indexed occupancy; avoid per-tick JSON and full-world clones. Parallel intent jobs read shared immutable phase data; one ordered reducer commits mutations. Worker stream batches are bounded; browser slowness cannot block simulation. Send progress at most a few times per second. Measure wall simulation duration separately from simulated duration.

Initial measurement target, not a promise: default 48×48 map, 500 entities, 12,000 actively simulated ticks in under two seconds on the developer's recorded machine in release mode, peak server+worker memory under 512 MiB. Benchmark path-heavy and combat-heavy scenarios at 100/500/2,000 entities. If unmet, profile; first reduce snapshot frequency, unnecessary path work, and allocation. Do not weaken determinism to hit a benchmark.

Snapshot memory and browser delivery must be bounded: checkpoint interval configurable, compressed snapshot chunks on disk, bounded in-memory LRU, timeline overview fetched separately from selected-tick chunks. Browser requests revision-tagged tick ranges. Avoid broadcasting the entire history after each turn. Checkpoint pruning in timed mode retains `S[L]` and the replay archive needed for later audit.

## Peripheral mode

When configured for inputs-only delivery, main server still runs the authoritative simulation and owns scoring. A trusted native peripheral connects to its WebSocket, downloads pinned initial configuration/content plus the command ledger, and uses the exact same simulation library/build to reproduce revisions. It serves a local browser UI and local state snapshots; player commits relay to the main server using assigned slot credentials. Main server sends commands, phase metadata, scores/timing, and state hashes, not world snapshots in this mode.

Peripheral acknowledges completed revision/hash; mismatch stops local order entry and shows a diagnostic. It can reconstruct from the initial seed and ledger after reconnect; optional locally persisted checkpoints accelerate this. No automatic acceptance of client-computed wins, custom consensus, browser WASM port, or peer-to-peer synchronization. The ordinary authoritative mode remains the first playable milestone.

## Multiplayer result reduction

The worker returns all survivors and elimination reasons, not a single winner ID. A partial multiplayer elimination is not itself an early-stop condition: continue while opposing sides remain until inactivity/horizon or terminal side resolution. Classify the final survivor set as stalemate (everyone), win (some), or draw (none). The controller atomically applies all per-player/ per-team score deltas before checking a fixed target or lead over every rival. Checkpoints and replay outcomes preserve elimination timing; team assignments are pinned with match configuration. Constructor mining uses the ordinary mining action with half the dedicated miner rate, without an ID-specific engine branch.

## Minimal visual playback

The client renders user-requested grayscale terrain, high-contrast entities, health bars, facing, and lightweight movement/shot/impact effects. Movement resolution updates last-move direction only for successful moves, including allied swaps; attacks do not rotate this field. Emit compact canonical combat/movement events with frozen positions alongside snapshot chunks, retaining events that occur between samples. Cosmetic projectile travel does not change tick damage resolution. Effects are derived from selected replay time and revision, so seeking, pausing and suffix replacement do not replay stale explosions or change authoritative state.
