# Shared data contracts

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

These are language-neutral schemas to implement first in Rust with generated TypeScript declarations or checked JSON fixtures. They specify semantics rather than dependency-specific code. Wire integer components must fit JavaScript’s safe range; stable identities are fixed-size tuples, with revision-local compact indices for bulk rendering data. Use explicit tagged enums, reject unknown schema versions, and serialize state in canonical order for hashes. Internal binary checkpoint encoding may differ from browser JSON.

## Common records

```text
Tick = u32
PlayerId = u8
ControlGroupId = { owner: PlayerId, slot: u8 }  // slot 0–9
TeamId = string
SideId = Player{player_id} | Team{team_id}
Revision = u32
CommandId = { round: u32, player: u8, index: u32 }  // round 0 reserved for genesis
BirthCommandId = { command: CommandId, target_index: u16 }
EntityId = { birth_command: BirthCommandId, item_index: u16, occurrence: u32 }
QueueItemId = { birth_command: BirthCommandId, item_index: u16 }
BlueprintId = EntityId  // occurrence 0; keeps this ID when construction completes
EntityIndex = u16 | u32  // revision-local transport dictionary, never order identity
Tile = { x: u16, y: u16 }
Rect = { min: Tile, max: Tile }  // inclusive, normalized, map-clipped
Priority = high | medium | low
Reason = no_active_building | no_build_ability
Fingerprint = { schema_version, sim_build, target, config_hash, content_hash }

MatchConfig = {
  schema_version, seed, map_size, player_count, symmetric,
  multiplayer: FFA | Teams{assignments: [{player_id, team_id}]},
  objective: Timed{lock_ticks_per_round} | Scoreboard{
    victory_rule: FixedTarget{points} | Lead{margin},
    tie_policy: continue_until_unique | shared_victory,
    draw_scoring: none | all_players, time_penalty},
  control_limit: timestamp | single_order,
  future_orders: { window_ticks: positive u32 | null },  // null disables window choice
  max_tick, stall_ticks, ticks_per_second, starting_matter, ore_matter_per_start,
  simulation_threads, checkpoint_interval, snapshot_interval,
  transport: authoritative | inputs_only, replay_directory
}

TypeDefinition = {
  key, kind: unit | structure, shape, matter_cost, max_hp, counts_for_survival,
  provides_build_ability,
  movement?: { neighbors: four | eight, cooldown }, vision,
  weapon?: { range, damage, cooldown, indirect },
  mining?: { rate, cooldown }, construction?: { rate, cooldown },
  production?: { rate, recipes: TypeKey[] },
  healing?: { range, hp_per_matter, demand, cooldown }
}
```

Constructor mining throughput is 0.5 × dedicated miner throughput in the initial content. Proposed encoding uses the same cooldown and half the transfer amount; the simulation consumes generic mining capability data. A constructor’s Mine and Construct assignments are mutually exclusive.

Balance tags are validated capabilities, not arbitrary scripts. Distances/range tests use squared Euclidean tile distance for combat. Melee uses the configured movement-neighbor adjacency. Symmetric map generation must be version-pinned; persist the actual generated initial terrain/ore/start state as well as the seed.

## Orders and drafts

```text
Order = Idle
      | AttackMove{ destination: Tile }
      | Support{ target: EntityId }
      | Mine{ area: Rect }
      | Construct{ area: Rect }

StoredOrder = Idle | AttackMove{destination} | Mine{area} | Construct{area}
            | Support{target: EntityId}

FutureOrderPolicy = Keep | DropAll | DropWindow

Command = AssignOrder{ entities: EntityId[], order: Order }
        | AssignGroupOrder{ group: ControlGroupId, order: Order }
        | EditGroupMembers{ group: ControlGroupId, edit:
            Replace{entities: EntityId[]} | Add{entities: EntityId[]}
            | Remove{entities: EntityId[]} }
        | BindFactoryGroup{ factories: EntityId[], group: ControlGroupId | null }
        | SetPriority{ entities: EntityId[], priority }
        | PlaceBlueprints{ type_key, tiles: Tile[], priority, output_directions?: Direction[] }
        | CancelBlueprints{ blueprint_ids: BlueprintId[] }
        | EditProduction{ factories: EntityId[], edit:
            Append{items: TypeKey[]} | ReplacePending{items: TypeKey[]}
            | RemovePending{item_ids: QueueItemId[]} | CancelActive }
        | SetQueueLoop{ factories: EntityId[], enabled: bool }
        | SetStoredOrder{ factories: EntityId[], order: StoredOrder }

DraftCommand = { local_id: string, command: Command, future_orders: FutureOrderPolicy }
// Non-Keep is permitted only for AssignOrder and AssignGroupOrder.
// Every setting/membership/binding/general command requires Keep.
TurnDraft = { based_on_revision, tick, commands: DraftCommand[] }
CommittedCommand = { id, command: Command, future_orders: FutureOrderPolicy }
CommitRequest = { request_id, slot_token, draft: TurnDraft }
AcceptedTurn = { player, round, tick, commands: CommittedCommand[], duration_ms }
CommandOutcome = { command_id, applied_entities, skipped: [{entity_id?, reason}] }
```

A missing stored support target resolves to idle with a diagnostic when a newborn receives it. Explicit entity-list targets are frozen at draft creation; control-group commands resolve membership at execution. For a multi-factory queue edit, `target_index` is the factory's position in the full frozen sorted target list, never renumbered when some targets are missing. `item_index` identifies the recipe within that command. Looping preserves QueueItemId and advances occurrence only on successful spawn. ReplacePending preserves the active item's ID; explicit re-append creates a new item. Blueprint commands use target_index=0 and tile item_index; initial entities use genesis CommandId per player and initial spawn item_index. Command/item/target integer bounds are validated, never wrapped.

The tuple is fixed-size without recursive factory identity, digests or variable-length causal strings. A queue event belongs to its frozen target factory; it cannot migrate to a replacement factory. Intern EntityId into a per-revision dictionary for samples/events. Choose u16 if a conservative bound on total births (initial entities plus all available matter divided by cheapest positive creation cost) fits, otherwise u32. Include `index_width` and dictionary data in revision/range payloads; never reuse an index within a revision. Durable orders and hashes use causal tuples, not dictionary insertion order. Cross-revision references resolve to EntityId, never a prior revision's compact index.

Validate envelope shape, player ownership in exact `S[t]`, capability compatibility, bounds, command-count rule, revision, and editable interval on commit. Reject malformed/unauthorized commands atomically; show an actionable error without losing the draft. Retained historical commands are revalidated at execution and can become no-ops as history changes. In simultaneous mode a command valid when committed can become blocked by another player's same-round changes; accept this as a reported gameplay outcome.

No player may stage commands for different ticks within one turn. Changing the draft tick offers an explicit clear-or-rebase action; rebasing validates all targets again. The server may cap payload/entity counts to bounded map/population limits, but does not impose an arbitrary tactical command quota in timestamp mode.

### Future-order locks

```text
OrderLock = { from_tick: Tick, until_tick: Tick, issued_round: u32 }
OrderLocks = OrderLock[]  // small canonical collection; usually empty or one entry
```

AssignOrder and AssignGroupOrder may install locks after passing execution-time ownership/capability/lock checks at tick t. Keep installs none and does not clear existing locks. DropWindow uses `until_tick = min(t + W, max_tick - 1)` with checked arithmetic; DropAll uses max_tick-1. The affected interval is `from_tick < u <= until_tick`. Reject window mode if disabled. Each affected living entity receives a lock, and a valid group command also installs one on its group slot even when it has no living members. There is no controller-resolved removal set.

When applying an action assignment, group delivery, or entity-directed priority/production/loop/template setting at u, skip that component if any active target lock covers u and `event.round < lock.issued_round`. Report `locked_by_later_round`. Same-round/newer-round events execute normally. Membership/binding and general blueprint commands require Keep and are not blocked by this action/settings lock. Check a group slot before its saved-order write and member delivery; a blocked slot skips the whole group command. Then check individual members independently. A blocked/missing/incompatible assignment installs no new lock; earlier future orders may therefore stay live if the replacement target disappears after a rewrite. This is visible in command outcomes.

Overlapping lock windows retain their separate round thresholds and expiry. Do not overwrite a long lock with a short newer one or merge them by taking both maxima, which would incorrectly extend the newer round's restriction. Prune expired entries at tick boundaries and entries dominated for all remaining ticks by another entry with at least as late expiry and at least as high issued_round. Canonical sort is expiry then issued_round; locks with identical future coverage are merged. All retained entries are serialized/hashed. Removing/replacing an earlier cause by rewriting before it executes changes downstream locks naturally; there is no permanent deletion outside sim state.

Replacement plus installed locks counts as one command. Original ledger events remain for inspection. A client locally projects the ordered draft over exact S[t] and fetched scheduled commands to estimate skipped effects; label it non-authoritative because future births, targets and group membership may change. Return actual per-command no-op reasons after simulation, without persisting separate suppressed-event relationships. The simulator's inactivity lookahead can ignore a future command only if every effect is provably blocked at its timestamp; uncertain future members/births keep it eligible.

Protocol: use `GetCommands{revision, from_tick, to_tick}` → `{revision, turns}` for published-revision scheduled-command chunks shared by timeline and local lock preview. Never reveal another player’s unclosed-round inputs; own accepted-turn acknowledgement remains private until the shared round closes. No separate per-entity/group removal-history or server deletion-preview endpoints. Draft structure/local references still validate sequentially at commit using the common metadata reducer.

### Persistent control groups

```text
ControlGroupState = {
  id: ControlGroupId,
  members: EntityId[],  // canonical set; stable IDs, dead/absent members not selected
  latest_order?: { source_command_id, tick, order: Order },
  order_locks: OrderLocks
}
```

All ten groups exist initially with empty members and no saved order. EditGroupMembers affects only its selected player-owned slot; Replace/Add/Remove does not reset the saved order or retroactively command existing units. AssignGroupOrder resolves living owned members from simulation state at its event tick, stores the order and applies it once to compatible members. An empty group is a valid recipient. Same-tick group edits/orders use existing canonical event ordering. Individual overrides update only entity action state, leaving saved group state intact. Dead members can remain as stable references and are ignored; earlier replay can restore them. Factories may be selection-group members without being output-bound, and output binding need not add the factory itself.

Birth insertion occurs after this tick’s input events, so a same-tick group order is available to a newborn. A blocked paid unit consults membership binding and saved order only when it actually spawns. Newborns join and inherit without generating a new player command, consuming a turn, resetting unrelated cooldowns, or changing causal entity ID. Snapshot/checkpoint serialization includes group membership, saved order source and factory bindings. Simulated latest_order is derived by scheduled event precedence; a new earlier-time group order must not override a later retained group order.

A member's lock skips only that member's incoming older-round deliveries/settings; it does not alter group saved-order state. A group slot lock skips an older-round group order's saved write and all member delivery. At actual factory spawn, a newborn joins its current bound group and copies its effective latest order and still-active group lock entries. Existing units added to a group do not retroactively copy locks/orders; later group deliveries check the slot and each entity. Group lock checks therefore remain consistent for later births without replaying one player command per newborn. Retain the original source tick/round/expiry on inherited locks.

ExactState includes group states. Client selection tracks whether a group was recalled: an unchanged recalled selection sends AssignGroupOrder; manually changing selection sends AssignOrder unless the player explicitly retargets the group. The recipient mode is visible before commit. Local future-order estimates use GetCommands and full preceding draft context. Canonical event order, not wall-clock acceptance, determines latest effective group order.

## Simulation boundary

```text
SimRequest = {
  job_id, revision, fingerprint, config, content,
  checkpoint: WorldState, events: AcceptedTurn[], end_tick_exclusive,
  minimum_end_tick  // inactivity cannot stop before this; no survivor-count early exit
}
WorldState = {
  tick, last_progress_tick, inactivity_deadline, terrain, ore, players: [{bank, spend counters, currently_eliminated, status_since_tick, elimination_reasons: Reason[]}],
  entities: [{id, owner, type_key, tile, last_move_direction, hp, paid_matter,
    lifecycle: site | complete, blueprint_id?, action, priority, engaged_target?,
    order_locks: OrderLocks, goal_settled, failed_move_attempts, blocked_step?, local_detour?: Tile[],
    next_action_tick, next_move_tick, production?, support_target?}],
  blueprints, control_groups: ControlGroupState[], survival_transitions, deterministic_identity_state
}
Production = { pending_items, active_item?, loop_enabled, stored_order,
               output_tile, occurrence_counters, spawn_group: ControlGroupId | null }
QueueItem = { item_id, type_key, loop_enabled }
ActiveItem = { item_id, occurrence, type_key, paid_matter, awaiting_output, loop_enabled }

WorkerMessage = Progress{job_id, revision, tick, end_tick}
              | Batch{job_id, revision, snapshots, checkpoints, stats, events}
              | Complete{job_id, revision, outcome, final_hash, sim_duration_ms,
                         command_outcomes}
              | Failed{job_id, revision, error_code, message}
Outcome = {
  kind: stalemate | win | draw,
  stop_reason: inactivity | absolute_horizon,
  terminal_state_tick, last_progress_tick,
  survivors: PlayerId[], eliminated: PlayerId[],
  surviving_sides: SideId[],
  survival_transitions: [{player_id, resolved_tick, status: alive | eliminated,
    reasons: (no_active_building | no_build_ability)[]}]
}
RoundScore = {
  entries: [{side_id, surviving_members: PlayerId[], credited_players: PlayerId[],
             award_reason: survival | draw | none, raw_delta, adjusted_delta,
             raw_total, adjusted_total}],
  victory_rule, match_winners: SideId[]
}
```

The server sim adapter exchanges these owned Rust values over bounded channels with a dedicated thread. A shared cancellation flag is checked every tick; tag all output with job_id/revision. A job error or caught recoverable panic leaves the previous published revision intact and discards that job's unpublished cache data. Fatal process failure is handled by durable archive recovery. Required output can throttle the sim thread through bounded channels, never the IO event loop; optional progress may be coalesced. No stdin/stdout protocol, wire framing or subprocess lifecycle.

Write result chunks as regenerable cache files and mark only complete chunks usable. Publish only after a complete result and durable atomic round record; there is no result-directory promotion step. Interrupted chunks/jobs are ignored or removed on recovery. This retains atomic accepted-turn/round persistence independently of in-process execution.

GetExactState reconstructs any legal tick from the nearest checkpoint in-process and caches by revision/tick in an LRU. There is no snapping to sample ticks. Published-range checkpoint interval 100 bounds ordinary reconstruction to 99 ticks; cache regeneration may take longer and has its own measured latency. A speculative interpolated display does not authorize command staging before exact state arrives.

An elimination from resolved tick `v` appears in `S[v+1]`. Evaluate both loss predicates for every player after deaths/completions/births; record both reasons when applicable. User-confirmed eligibility counts only completed living entities with the corresponding capability. Status is recomputed every tick and can return to alive; never gate entity actions or retained scheduled orders on owner survival status. A recovery transition has no failing reasons. Final survivors/eliminees are a partition of all players by their current status, not an ever-eliminated set. Individual ownership matters even in teams. Store the player/team mapping in pinned configuration, and derive hostility consistently in worker and peripheral.

Outcome classification uses the final survivor set over all original participants: empty → `draw`, all → `stalemate`, otherwise → `win`. It does not require a unique winning player. Continue through temporary elimination and recovery to inactivity/horizon; elimination-based early stopping is explicitly removed by user direction. Inactivity/horizon can yield any applicable classification. Team score entries retain exact survivor membership, making count-based scoring and later audits possible. Match winners are separate from simulation outcomes.

## Browser/controller protocol

```text
Client -> Hello{protocol_version, last_revision?}
          ClaimSlot{slot, username, color, team_id?}, ReleaseSlot
          UpdateLobbyProfile{request_id, username?, color?, team_id?}
          StartMatch{based_on_lobby_revision}
          Commit{CommitRequest}
          GetSnapshotRange{revision, from_tick, to_tick, stride}
          GetExactState{revision, tick}
          GetStats{revision, from_tick, to_tick, bucket_width}
          GetCommands{revision, from_tick, to_tick}

Server -> Welcome{server_instance_id, match_id, config_summary, phase, lobby: LobbyState, fingerprint, guide_url}
          SlotClaimed{slot, private_token}
          LobbyUpdated{lobby: LobbyState}
          LobbyUpdateRejected{request_id, code, message, lobby: LobbyState}
          PlanningOpened{round, revision, editable_from, available_through,
                         committed_players, time_totals}
          CommitAccepted{request_id, round}
          CommitRejected{request_id, code, message}
          SimulationProgress{revision, tick, end_tick}
          RevisionPublished{revision, outcome, timeline_index, score: RoundScore,
                            time_totals, time_ratios, sim_duration_ms}
          SnapshotRange{revision, index_width, entity_dictionary, samples}
          ExactState{revision, tick, snapshot}
          StatsRange{revision, buckets}
          Commands{revision, turns}
          MatchFinished{match_winners: SideId[], final_outcome: Outcome, reason}
```

Only lobby participants claim slots. First occupied slot may start once all configured slots are claimed, required profile fields are valid, and chosen teams satisfy setup constraints; spectators can join anytime. Start uses the current lobby revision. Team choices are editable in the lobby and become pinned assignments at start. Slot token saved in local browser storage permits reconnect; spectators cannot commit. No account/security system, but ordinary ownership and phase checks remain. No automatic turn timeout; a disconnect can stall planning and is displayed. Server operator can stop/archive the match; do not invent bot moves.

Client discards responses for stale revisions. `request_id` provides idempotency: a retry returns the original commit response. Simultaneous mode hides committed command payloads until the round closes while sharing readiness. Commit is final for that round; undo is available in the draft only. A late reconnect receives the current revision, metadata and its own accepted-commit state.

Define `available_through` as the inclusive latest command tick: `min(terminal_state_tick, max_tick-1)`, allowing new orders to resume from a quiet endpoint. Always require `editable_from <= tick <= available_through` and `tick < max_tick`. Timed workers use `minimum_end_tick = min(max_tick, previous_editable_from + lock_ticks_per_round)` so quiet history can still advance. Do not close a match merely because a past elimination tick becomes immutable. Timed closure checks constructor/factory absence in S[new_L], as specified below. Distinguish `simulation_end_tick` from `editable_from`; a run reaching max_tick does not itself finalize the match. A transient active-building loss alone never finalizes timed defeat.

Each connected client/peripheral sends `PlanningReady{round, revision}` when its required initial state is available. Its thinking timer starts at that acknowledgement; ordinary clients do not wait for all timeline chunks. No anti-cheating system is required. Disconnected players' timers begin when the controller opens planning; reconnect does not erase accrued time. In peripheral mode, initial local simulation catch-up before readiness is excluded and displayed as loading. A connected client withholding readiness can stall the prototype just as withholding a commit can; show readiness explicitly.

Inputs-only controller adds `ReplayBootstrap{fingerprint, config, content, initial_state, ledger}`, `RoundInputs{revision, turns, precedence, editable_from}`, and `ReferenceHash{revision, tick, hash}`. It omits snapshot/stat-range service; peripheral provides those locally. Initial-state disclosure is initialization data, not ongoing authoritative world streaming.

## Statistics and timeline

Per player/tick collect bank, mined matter, total spend, unit spend, structure spend, lost invested matter, living army value, living infrastructure value, entity count, damage dealt, and worker activity. Unit/infrastructure spend is historical spending, not living value. Destroyed partly built projects contribute only paid matter to losses. Attrition is explicitly `own lost invested matter / own total spend` plus a separate `own total spend / opponent total spend` metric matching the prompt's spend-ratio request. Display `—` for undefined zero denominators. For 3–4 players allow selecting an opponent.

Free starting entities contribute recipe cost to living/replacement value, but zero to spend or lost invested matter. Track destroyed replacement value separately so destruction of the free opening turret remains visible in combat statistics. Never mix replacement cost with actual bank expenditure in conservation checks.

Timeline buckets retain per-activity counts, severity, and involved entity IDs or bounded event references. Choose displayed dominant activity by combat > construction/production > mining > movement > idle; bar height is number of affected entities, normalized to the visible range with labeled scale. Events about the local player's units count even if caused by an enemy. Spectators choose a player or all-player view.

## Durable files and crash recovery

```text
replays/<match_id>/
  manifest.json                 # schema/build/content/config, initial-state reference
  initial-state.bin
  config.yaml + content.yaml    # exact pinned copies
  turns/<round>-<player>.json   # durable accepted inputs + accumulated duration
  rounds/<round>.json          # turn references, precedence, parent/new revision,
                               # survivor/outcome record, per-side score deltas/totals, boundary, hashes, sim time
  results/<revision>/...        # compressed snapshots, stats and checkpoints
```

Each accepted player turn is atomically written and flushed before acknowledgement. In simultaneous mode a partial round survives restart; do not discard its committed player. Once all turns exist, simulation is retryable. Round result links every input turn to its result, satisfying per-turn replay inspection without copying the same large result into each turn file. Write results and a complete round record before publishing the revision; recovery uses complete round records as the commit point. Ignore incomplete temporary files, verify references/checksums, and regenerate missing cache data. Apply a score delta exactly once by round ID. Never replay archived input under changed content silently.

Persist thinking time periodically in a separate atomic planning record, with the interval documented. Record resume events and any operator action. Provide CLI replay verification/resume and browser replay loading as read-only spectator mode. Results are inspectable by round, not only the final rewritten timeline.

User-confirmed scoreboard cadence is once per resolved round, including passes and unchanged winning timelines. Default victory rule is `FixedTarget{points: 5}`, configurable; multiplayer may instead use `Lead{margin: N}`.

Proposed score reduction: construct the complete vector of player/team deltas, persist it once per round, then evaluate the configured threshold. In lead mode compare the selected total of each side with the maximum of all rivals; require a positive margin. For example, totals `[8, 8, 3]` have no leader by 2, while `[10, 8, 3]` do. FFA win deltas are proposed as one per survivor; team deltas equal survivor count. Per user confirmation, survival points require at least one eliminated player: team raw delta is final surviving-member count on `win` and always zero on `stalemate`. On `draw`, use configured `draw_scoring`: `none` gives zero, `all_players` credits every original player (proposed one raw point each, summed by team). Score reduction belongs in the controller, not combat AI or client calculations.

### Facing and cosmetic combat events

Proposed `last_move_direction` is one of eight grid direction vectors; four-neighbor units only acquire axis-aligned values through movement. Set it after each successful move/swap, preserve it on idle/blocked steps, and initialize deterministically at spawn. Serialize it in exact states, sampled snapshots, checkpoints and state hashes so a seek or partial replay cannot lose facing. Authoritative HP/max-HP and construction fraction are available independently of animation.

Proposed presentation events are `Move{tick, entity_id, from, to}`, `Attack{tick, attacker_id, source_tile, target_id, target_tile, visual_style}`, `Impact{tick, target_tile, visual_style}` and `Destroyed{tick, entity_id, tile, visual_style}`. Give events stable revision-scoped sequence IDs in canonical emission order. Freeze event positions so a dead/missing entity is not needed to draw its effect. Include events in revision-tagged range chunks; the client requests a short lookback when seeking, based on maximum configured effect lifetime. Weapon/content presentation fields select minimal shot/impact styles; effect lifetime and interpolation never feed back into combat or consume simulation RNG. Aggregate visual batches only after authoritative damage/statistics are resolved.

### Recovery and configurable score ties

`currently_eliminated` is derived from current completed entities each tick, and affects outcome accounting rather than whether surviving entities execute orders. A player failing survival at tick 20 and completing a factory at tick 40 has elimination and recovery transitions but belongs to final survivors if still alive at the endpoint. If everyone recovers, classify stalemate and award zero survival points. Retain status transitions in replay/timeline data and checkpoint the current status; do not treat a past transition as irreversible even in timed mode.

User-confirmed tie default is continued play until one qualifying team leads. Proposed wire policy `continue_until_unique` requires a unique maximum score meeting FixedTarget; `shared_victory` instead returns all tied maximum qualifying sides in `match_winners`. With target 5, `[5,5]` continues by default and `[6,5]` ends; `[5,4]` meets the first-to-5 rule. In shared mode `[5,5]` returns both. Lead-N still requires a unique side at least N ahead of every rival. Apply all same-round deltas before this check. `match_winners` is empty while the match continues; multiple match winners do not change the separate simulation stalemate/win/draw taxonomy.

### Draw score reduction

The user requires `draw_scoring: none | all_players`; default `none` is proposed. Result classification precedes scoring. Stalemate always produces zero deltas, even if someone temporarily failed the survival test, and ignores draw policy. Win credits final survivors. Draw/none credits nobody; draw/all_players credits all original participants, with proposed raw team totals equal to original team size. Keep `surviving_members` empty on draws and use `credited_players` plus `award_reason: draw` to explain points correctly. Apply any configured time adjustments to the credited players as a revisable proposal, then evaluate target/lead and tie policy on the complete vector. Persist draw/tie policies with pinned config and per-round score records so replay, recovery and peripheral displays agree.

### Lobby profiles and guide identity

```text
PlayerProfile = { player_id, username, color: "#RRGGBB", team_id?: TeamId }
LobbyState = {
  revision, slots: [{slot, claimed, connected, profile?: PlayerProfile}],
  available_teams: [{team_id, label, capacity?}], can_start, rule_summary
}
GuideManifest = {
  content_hash, rules_build, locale,
  ticks_per_second?, generated_files
}
```

Profile updates are allowed before match start, validated by slot ownership, and broadcast to every connected client. Include the complete latest LobbyState on initial join/reconnect; do not depend on a client having received earlier updates. Team availability/capacity comes from YAML; MatchConfig's final assignments come from the accepted start roster. Start and profile updates serialize on the controller so stale starts cannot freeze an unseen roster. Profile values never determine entity ID, deterministic target ordering, or ownership; persist profiles in lobby storage and match/replay metadata for correct labels/colors after refresh/resume.

Guide generation consumes normalized TypeDefinition records through the same loader used by the game. Generate static guide files once at startup from the effective loaded content into a served writable directory, and expose guide_url when complete. The same generator serves resumed/replay content without a bundled artifact comparison. Intrinsic stats use ticks; any generated time conversions also key the artifact by effective tick rate. Every launch regenerates matching tables; generated metadata records content identity without selecting a second generation path. Unit descriptions reference capability data; exact mechanics prose is versioned alongside the sim code and reviewed at release.

Proposed server CLI contract: `--port <1..65535>` selects the single listener used by assets, HTTP and WebSocket, overrides a config default, and never falls back silently. Print the chosen address and resume/new-match mode at launch. Reconnecting to an old server instance requires a fresh bootstrap; revision numbers alone cannot distinguish two process runs. Keep tokens private in individual SlotClaimed messages; public lobby snapshots expose only profile/readiness data.

### Placement, draft projection, and content ownership

Factory output selection: proposed cardinal directions N/E/S/W, explicitly selectable during placement. If omitted, choose the first in that order whose output neighbor is in bounds, walkable for the producible roster, and not occupied by a static structure/site in the projected draft state. Reject a factory blueprint with no structurally legal output; moving allied occupants do not invalidate placement and can cause ordinary temporary blocking later. Persist the chosen direction on the blueprint/site/factory rather than reselecting it during replay. Validate against actual state when the historical blueprint executes; report a no-op if an earlier rewrite makes it invalid. Preview renders the factory and its output tile. No implicit output teleporting or newborn displacement.

Draft projection: the client uses the full ordered TurnDraft to estimate future lock effects locally. Server commit validation walks earlier metadata edits against exact S[t] (group membership, queue/priority/template edits and blueprints), without executing economic/combat ticks. Add DraftRef{local_id, item_index} for references to earlier draft-created blueprints/queue items alongside persistent ID references. Reject forward/cyclic/unknown references. Assign canonical command IDs after validation and resolve local references into birth tuples deterministically. New production targets do not exist merely because a queue was drafted. Actual future action/lock eligibility is decided in the sim, not inferred as durable deletion at commit.

Motion fields: `failed_move_attempts`, `blocked_step`, `goal_settled` and bounded `local_detour` are future-affecting state, so serialize/hash them. Displacement events identify mover and blocker, both old/new positions and which move was involuntary. Guard against moving either participant twice in a tick. Legal movement is capability-based for each participant; do not diagonally swap a vehicle merely because the other unit is a walker. Tests compare threads/checkpoints under randomized intent scheduling.

Content ownership: coordinator owns `crates/content` (normalized types, validation, serialization, fingerprint and stat export), `crates/contracts`, build wiring and the small static guide generator. Simulation depends on content, not on guide generation; the guide generator depends on content, not on server/sim startup. Client owns authored guide/layout and consumes generated tables. Server owns the single startup guide-generation invocation and historical-content routing. Headless sim tests compile without HTML generation or browser tooling. No duplicated content loader in the simulation crate.

### Controller adjudication

Timed mode: compute `new_L = min(old_L + lock_ticks_per_round, max_tick)` after a resolved round and preserve S[new_L]. Inspect completed living constructor/factory capabilities in that locked state, not the future sim outcome and not all-active-buildings absence. Persist `timed_lost_players` in controller metadata, separately from sim `currently_eliminated`. Proposed multiplayer reducer: a side remains eligible if any of its players is not timed-lost; finish with the sole eligible side or no winner if all are lost. Evaluate the entire locked-state vector before declaring a winner, so simultaneous final losses have no order bias. If several opposing sides remain eligible, open the next planning round. At boundary=max_tick with no constructor-based decision, proposed `history_exhausted` archives/halts as unfinished without a manufactured game win/draw or score delta. This last edge behavior and team reducer are disclosed interpretations.

A temporarily building-less player with a constructor at the boundary is not timed-lost. A constructor/factory absence only in the mutable suffix is not final. Under the current ownership rules there is no ability to create a new constructor/factory after both sources are absent in locked history; if future mechanics permit allied construction/rescue or resurrection, revisit this finalization premise explicitly.

Scoreboard has no maximum-round configuration, per user decision. Add proposed `StopAndArchive{request_id, based_on_revision}` as a controller operation available through a visible lobby/match control (first occupied slot/operator under the current host-role proposal). Preserve every accepted turn, including partial-round commits, and the last complete published result; optionally allow resume under existing archive semantics. Record `unfinished` and the stopping actor/time, never a fabricated sim Outcome, winner, or extra score delta. Repeated tie/stalemate/pass sequences otherwise continue. Manual-stop permission details remain revisable; no automatic round cap is introduced.

### Derived movement fields and tuning

Flow-field key is `(goal_tile, neighbor_rule, structure_version)`. The local version increments on blueprint/site/structure placement, completion and destruction, and resets with the cache on checkpoint load. Neither version nor field grids enter WorldState, snapshots or hashes; all decisions must be identical for an empty versus warm cache. Walkers and vehicles use separate BFS grids; blocked static targets have virtual adjacent seeds, not passable structure cells. Bounded local stuck BFS uses radius 6 and current occupancy; no per-unit whole-map route state is required.

Proposed defaults: max_tick=20,000, checkpoint_interval=100, snapshot_interval=5, stall_ticks=300 at 10 ticks/second. `ore_matter_per_start / miner_rate_per_tick` estimates 10,000–20,000 full-throughput mining ticks (initial target about 12,000). Show this estimate and its assumptions in LobbyState.rule_summary; ore, starting bank and horizon remain YAML tuning. Use all map ore for aggregate estimates in asymmetric/FFA layouts, clearly labeling per-start allocation as a generation reference rather than exclusive ownership.
