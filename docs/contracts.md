# Shared data contracts

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

These are language-neutral schemas to implement first in Rust with generated TypeScript declarations or checked JSON fixtures. They specify semantics rather than dependency-specific code. Wire integers must fit JavaScript's safe range; identities are strings. Use explicit tagged enums, reject unknown schema versions, and serialize state in canonical order for hashes. Internal binary checkpoint encoding may differ from browser JSON.

## Common records

```text
Tick = u32
PlayerId = u8
ControlGroupId = { owner: PlayerId, slot: u8 }  // slot 0–9
TeamId = string
SideId = Player{player_id} | Team{team_id}
Revision = u32
CommandId = "r{round}:p{player}:c{index}"
EntityId = canonical causal string or collision-checked 128-bit digest string
QueueItemId = CommandId + item_index
BlueprintId = CommandId + tile_index
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
  max_tick, stall_ticks, ticks_per_second, starting_matter,
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

Balance tags are validated capabilities, not arbitrary scripts. Distances/range tests use squared Euclidean tile distance except path heuristics. Melee uses the configured movement-neighbor adjacency. Symmetric map generation must be version-pinned; persist the actual generated initial terrain/ore/start state as well as the seed.

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
        | PlaceBlueprints{ type_key, tiles: Tile[], priority }
        | CancelBlueprints{ blueprint_ids: BlueprintId[] }
        | EditProduction{ factories: EntityId[], edit:
            Append{items: TypeKey[]} | ReplacePending{items: TypeKey[]}
            | RemovePending{item_ids: QueueItemId[]} | CancelActive }
        | SetQueueLoop{ factories: EntityId[], enabled: bool }
        | SetStoredOrder{ factories: EntityId[], order: StoredOrder }

DraftCommand = { command: Command, future_orders: FutureOrderPolicy }
// Non-Keep is permitted for entity-directed orders/settings and AssignGroupOrder;
// membership/binding/general edits currently require Keep.
TurnDraft = { based_on_revision, tick, commands: DraftCommand[] }
Suppression = { source_command_id, historical_command_id,
  target: EntityComponent{entity_id} | EntireGroupOrder{group: ControlGroupId} }
CommittedCommand = { id, command: Command, future_orders: FutureOrderPolicy,
                     suppressions: Suppression[] }
CommitRequest = { request_id, slot_token, draft: TurnDraft }
AcceptedTurn = { player, round, tick, commands: CommittedCommand[], duration_ms }
CommandOutcome = { command_id, applied_entities, skipped: [{entity_id?, reason}] }
```

A missing stored support target resolves to idle with a diagnostic when the newborn receives it. Explicit entity-list command targets are frozen at draft creation. Persistent control-group commands instead resolve membership at execution, as specified below. A production command applied to multiple factories generates distinct queue-item identity by factory ID plus command/item index. Looping preserves the item ID and increments its occurrence; explicit re-append generates a new item ID. ReplacePending never renames the active item. Avoid unbounded causal strings by using deterministic digest identities with collision assertion.

Validate envelope shape, player ownership in exact `S[t]`, capability compatibility, bounds, command-count rule, revision, and editable interval on commit. Reject malformed/unauthorized commands atomically; show an actionable error without losing the draft. Retained historical commands are revalidated at execution and can become no-ops as history changes. In simultaneous mode a command valid when committed can become blocked by another player's same-round changes; accept this as a reported gameplay outcome.

No player may stage commands for different ticks within one turn. Changing the draft tick offers an explicit clear-or-rebase action; rebasing validates all targets again. The server may cap payload/entity counts to bounded map/population limits, but does not impose an arbitrary tactical command quota in timestamp mode.

### Future-order suppression contract

For a draft at t, the controller resolves DropAll against effective historical entity-directed commands with tick > t; DropWindow restricts this to `t < tick <= min(t + W, max_tick - 1)` using checked arithmetic. W comes from pinned setup. Reject DropWindow when disabled. Inspect only commands visible in `based_on_revision`; newly committed commands in the same simultaneous round are never suppression targets. Match owned entity IDs individually, not whole group envelopes. Canonicalize and persist the exact `(historical_command_id, suppression_target)` set with the source command; clients cannot provide unchecked deletions. Repeated suppressions are idempotent.

Proposed removal scope includes AssignOrder, AssignGroupOrder, SetPriority, EditProduction, SetQueueLoop and SetStoredOrder. PlaceBlueprints, CancelBlueprints, EditGroupMembers and BindFactoryGroup carry Keep only and are not implicitly suppressed. Suppressions are not recursively deleted when their source's in-world command component is later suppressed: they represent already accepted revision edits. Original command/queue-item IDs are never renumbered. Replacement plus suppression counts as one command for control limits.

Build the effective event stream by applying all suppression records through the selected revision before simulation, worker inactivity lookahead or checkpoint suffix replay. Preserve group members that were not suppressed. Persisted suppression is independent of later execution-time target validity. Add `suppressed_by` references to historical command inspection, distinct from execution no-op reasons. A historical round's replay includes only suppressions accepted through that round. Future-order policy and resolved suppressions are included in archive/inputs-only payloads and revision identity.

Proposed protocol additions: `PreviewFutureOrders{based_on_revision, tick, draft_command}` → `{revision, affected_components, counts_by_kind, interval}` for review, and `GetEntityOrderHistory{revision, entity_ids}` → scheduled command components including suppression status. Server recomputes/validates the suppression set on commit. Local previews may use fetched history; they are not authoritative.

### Persistent control groups

```text
ControlGroupState = {
  id: ControlGroupId,
  members: EntityId[],  // canonical set; stable IDs, dead/absent members not selected
  latest_order?: { source_command_id, tick, order: Order }
}
```

All ten groups exist initially with empty members and no saved order. EditGroupMembers affects only its selected player-owned slot; Replace/Add/Remove does not reset the saved order or retroactively command existing units. AssignGroupOrder resolves living owned members from simulation state at its event tick, stores the order and applies it once to compatible members. An empty group is a valid recipient. Same-tick group edits/orders use existing canonical event ordering. Individual overrides update only entity action state, leaving saved group state intact. Dead members can remain as stable references and are ignored; earlier replay can restore them. Factories may be selection-group members without being output-bound, and output binding need not add the factory itself.

Birth insertion occurs after this tick’s input events, so a same-tick group order is available to a newborn. A blocked paid unit consults membership binding and saved order only when it actually spawns. Newborns join and inherit without generating a new player command, consuming a turn, resetting unrelated cooldowns, or changing causal entity ID. Snapshot/checkpoint serialization includes group membership, saved order source and factory bindings. Simulated latest_order is derived by scheduled event precedence; a new earlier-time group order must not override a later retained group order.

Future-order suppression extends to dynamic delivery: EntityComponent on an AssignGroupOrder skips only that entity at execution even if membership changed since the base revision; the group’s saved-order update remains. For individual removal, include historical group-order events in the requested interval for groups the selected entity belongs to at that event in the base revision, even if that base application was incompatible. EntireGroupOrder suppresses the event’s deliveries and saved-order write together. A new AssignGroupOrder with a drop policy includes whole future orders addressed to that slot and per-entity future effects for its base-revision current members. Resolve any earlier staged membership edits in the draft before previewing that current-member set. These removal targets are fixed at commit; new membership in the rewritten simulation does not silently expand them. Whole-event suppression dominates a redundant member mask. Revision-scoped suppression records remain effective even if their source’s later in-world delivery is itself suppressed.

Proposed protocol additions: `GetControlGroups{revision, tick, player}` returns group states; `GetGroupOrderHistory{revision, group}` returns scheduled orders and suppression status. PreviewFutureOrders accepts group commands and reports whole-group and individual effects separately. Client selection tracks whether the user recalled a group: an unchanged recalled selection sends AssignGroupOrder; manually changing the selection sends AssignOrder unless the player explicitly retargets the group. The stored recipient mode is visible before commit.

## Simulation boundary

```text
SimRequest = {
  job_id, revision, fingerprint, config, content,
  checkpoint: WorldState, events: AcceptedTurn[], suppressions: Suppression[], end_tick_exclusive,
  minimum_end_tick  // inactivity cannot stop before this; no survivor-count early exit
}
WorldState = {
  tick, last_progress_tick, inactivity_deadline, terrain, ore, players: [{bank, spend counters, currently_eliminated, status_since_tick, elimination_reasons: Reason[]}],
  entities: [{id, owner, type_key, tile, last_move_direction, hp, paid_matter,
    lifecycle: site | complete, blueprint_id?, action, priority,
    next_action_tick, next_move_tick, production?, support_target?}],
  blueprints, control_groups: ControlGroupState[], deterministic_identity_state
}
Production = { pending_items, active_item?, loop_enabled, stored_order,
               output_tile, occurrence_counters, spawn_group: ControlGroupId | null }
ActiveItem = { item_id, occurrence, type_key, paid_matter, awaiting_output }

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

Runner frames are length-prefixed with a configured maximum; never mix logs into stdout. Server writes a private temporary result directory, validates completion/hash/indexes, then atomically promotes the result. A worker's partial batches may be shown as provisional progress, never published as the next authoritative revision.

An elimination from resolved tick `v` appears in `S[v+1]`. Evaluate both loss predicates for every player after deaths/completions/births; record both reasons when applicable. User-confirmed eligibility counts only completed living entities with the corresponding capability. Status is recomputed every tick and can return to alive; never gate entity actions or retained scheduled orders on owner survival status. A recovery transition has no failing reasons. Final survivors/eliminees are a partition of all players by their current status, not an ever-eliminated set. Individual ownership matters even in teams. Store the player/team mapping in pinned configuration, and derive hostility consistently in worker and peripheral.

Outcome classification uses the final survivor set over all original participants: empty → `draw`, all → `stalemate`, otherwise → `win`. It does not require a unique winning player. Continue through temporary elimination and recovery to inactivity/horizon; elimination-based early stopping is explicitly removed by user direction. Inactivity/horizon can yield any applicable classification. Team score entries retain exact survivor membership, making count-based scoring and later audits possible. Match winners are separate from simulation outcomes.

## Browser/controller protocol

```text
Client -> Hello{protocol_version, last_revision?}
          ClaimSlot{slot}, ReleaseSlot, StartMatch
          Commit{CommitRequest}
          GetSnapshotRange{revision, from_tick, to_tick, stride}
          GetExactState{revision, tick}
          GetStats{revision, from_tick, to_tick, bucket_width}

Server -> Welcome{match_id, config_summary, phase, slots, fingerprint}
          SlotClaimed{slot, private_token}
          PlanningOpened{round, revision, editable_from, available_through,
                         committed_players, time_totals}
          CommitAccepted{request_id, round}
          CommitRejected{request_id, code, message}
          SimulationProgress{revision, tick, end_tick}
          RevisionPublished{revision, outcome, timeline_index, score: RoundScore,
                            time_totals, time_ratios, sim_duration_ms}
          SnapshotRange{revision, samples}
          ExactState{revision, tick, snapshot}
          StatsRange{revision, buckets}
          MatchFinished{match_winners: SideId[], final_outcome: Outcome, reason}
```

Only lobby participants claim slots. First occupied slot may start once all configured slots are claimed; spectators can join anytime. Slot token saved in local browser storage permits reconnect; spectators cannot commit. No account/security system, but ordinary ownership and phase checks remain. No automatic turn timeout; a disconnect can stall planning and is displayed. Server operator can stop/archive the match; do not invent bot moves.

Client discards responses for stale revisions. `request_id` provides idempotency: a retry returns the original commit response. Simultaneous mode hides committed command payloads until the round closes while sharing readiness. Commit is final for that round; undo is available in the draft only. A late reconnect receives the current revision, metadata and its own accepted-commit state.

Define `available_through` as the inclusive latest command tick: `min(terminal_state_tick, max_tick-1)`, allowing new orders to resume from a quiet endpoint. Always require `editable_from <= tick <= available_through` and `tick < max_tick`. Timed workers use `minimum_end_tick = min(max_tick, previous_editable_from + lock_ticks_per_round)` so quiet history can still advance. Do not close a match merely because a past elimination tick becomes immutable. Proposed timed closure requires the completed inactive win/draw endpoint to become immutable, or reaching the absolute horizon; preserve any recovery before that endpoint. Otherwise open the next planning round.

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

User-confirmed scoreboard cadence is once per resolved round, including passes and unchanged winning timelines. Default victory rule is `FixedTarget{points: 5}`, configurable; multiplayer may instead use the previously requested `Lead{margin: N}`.

Proposed score reduction: construct the complete vector of player/team deltas, persist it once per round, then evaluate the configured threshold. In lead mode compare the selected total of each side with the maximum of all rivals; require a positive margin. For example, totals `[8, 8, 3]` have no leader by 2, while `[10, 8, 3]` do. FFA win deltas are proposed as one per survivor; team deltas equal survivor count. Per user confirmation, survival points require at least one eliminated player: team raw delta is final surviving-member count on `win` and always zero on `stalemate`. On `draw`, use configured `draw_scoring`: `none` gives zero, `all_players` credits every original player (proposed one raw point each, summed by team). Score reduction belongs in the controller, not combat AI or client calculations.

### Facing and cosmetic combat events

Proposed `last_move_direction` is one of eight grid direction vectors; four-neighbor units only acquire axis-aligned values through movement. Set it after each successful move/swap, preserve it on idle/blocked steps, and initialize deterministically at spawn. Serialize it in exact states, sampled snapshots, checkpoints and state hashes so a seek or partial replay cannot lose facing. Authoritative HP/max-HP and construction fraction are available independently of animation.

Proposed presentation events are `Move{tick, entity_id, from, to}`, `Attack{tick, attacker_id, source_tile, target_id, target_tile, visual_style}`, `Impact{tick, target_tile, visual_style}` and `Destroyed{tick, entity_id, tile, visual_style}`. Give events stable revision-scoped sequence IDs in canonical emission order. Freeze event positions so a dead/missing entity is not needed to draw its effect. Include events in revision-tagged range chunks; the client requests a short lookback when seeking, based on maximum configured effect lifetime. Weapon/content presentation fields select minimal shot/impact styles; effect lifetime and interpolation never feed back into combat or consume simulation RNG. Aggregate visual batches only after authoritative damage/statistics are resolved.

### Recovery and configurable score ties

`currently_eliminated` is derived from current completed entities each tick, and affects outcome accounting rather than whether surviving entities execute orders. A player failing survival at tick 20 and completing a factory at tick 40 has elimination and recovery transitions but belongs to final survivors if still alive at the endpoint. If everyone recovers, classify stalemate and award zero survival points. Retain status transitions in replay/timeline data and checkpoint the current status; do not treat a past transition as irreversible even in timed mode.

User-confirmed tie default is continued play until one qualifying team leads. Proposed wire policy `continue_until_unique` requires a unique maximum score meeting FixedTarget; `shared_victory` instead returns all tied maximum qualifying sides in `match_winners`. With target 5, `[5,5]` continues by default and `[6,5]` ends; `[5,4]` meets the earlier first-to-5 rule. In shared mode `[5,5]` returns both. Lead-N still requires a unique side at least N ahead of every rival. Apply all same-round deltas before this check. `match_winners` is empty while the match continues; multiple match winners do not change the separate simulation stalemate/win/draw taxonomy.

### Draw score reduction

The user requires `draw_scoring: none | all_players`; default `none` is proposed. Result classification precedes scoring. Stalemate always produces zero deltas, even if someone temporarily failed the survival test, and ignores draw policy. Win credits final survivors. Draw/none credits nobody; draw/all_players credits all original participants, with proposed raw team totals equal to original team size. Keep `surviving_members` empty on draws and use `credited_players` plus `award_reason: draw` to explain points correctly. Apply any configured time adjustments to the credited players as a revisable proposal, then evaluate target/lead and tie policy on the complete vector. Persist draw/tie policies with pinned config and per-round score records so replay, recovery and peripheral displays agree.
