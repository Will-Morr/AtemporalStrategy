/* Generated from Rust contracts by npm run generate. Do not edit. */

/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ClientMessage".
 */
export type ClientMessage =
  | {
      kind: "hello";
      last_revision?: number | null;
      protocol_version: Version;
      slot_token?: string | null;
    }
  | {
      color: string;
      kind: "claim_slot";
      slot: number;
      team_id?: string | null;
      username: string;
    }
  | {
      kind: "release_slot";
      slot_token: string;
    }
  | {
      color?: string | null;
      kind: "update_lobby_profile";
      request_id: string;
      slot_token: string;
      team_id?: string | null;
      username?: string | null;
    }
  | {
      based_on_lobby_revision: number;
      kind: "start_match";
      slot_token: string;
    }
  | {
      kind: "planning_ready";
      revision: number;
      round: number;
    }
  | {
      kind: "commit";
      request: CommitRequest;
    }
  | {
      from_tick: number;
      kind: "get_snapshot_range";
      revision: number;
      stride: number;
      to_tick: number;
    }
  | {
      kind: "get_exact_state";
      revision: number;
      tick: number;
    }
  | {
      bucket_width: number;
      from_tick: number;
      kind: "get_stats";
      revision: number;
      to_tick: number;
    }
  | {
      from_tick: number;
      kind: "get_commands";
      revision: number;
      to_tick: number;
    }
  | {
      kind: "get_round";
      revision: number;
    }
  | {
      /**
       * Rendered effects only; omit movement diagnostics already represented by samples.
       */
      effects_only?: boolean | null;
      from_tick: number;
      kind: "get_events";
      revision: number;
      to_tick: number;
    }
  | {
      based_on_revision: number;
      kind: "stop_and_archive";
      request_id: string;
      slot_token: string;
    }
  | {
      kind: "get_control_groups";
      player: number;
      revision: number;
      tick: number;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Version".
 */
export type Version = number;
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Command".
 */
export type Command =
  | {
      entities: EntityId[];
      kind: "assign_order";
      order: Order;
    }
  | {
      group: ControlGroupId;
      kind: "assign_group_order";
      order: Order;
    }
  | {
      edit: MemberEdit;
      group: ControlGroupId;
      kind: "edit_group_members";
    }
  | {
      factories: EntityId[];
      group?: ControlGroupId | null;
      kind: "bind_factory_group";
    }
  | {
      entities: EntityId[];
      kind: "set_priority";
      priority: Priority;
    }
  | {
      kind: "place_blueprints";
      output_directions?: CardinalDirection[] | null;
      priority: Priority;
      tiles: Tile[];
      type_key: string;
    }
  | {
      blueprint_ids: DraftItemRef[];
      kind: "configure_blueprints";
      settings: BlueprintSettings;
    }
  | {
      blueprint_ids: DraftItemRef[];
      kind: "cancel_blueprints";
    }
  | {
      edit: ProductionEdit;
      factories: EntityId[];
      kind: "edit_production";
    }
  | {
      enabled: boolean;
      factories: EntityId[];
      kind: "set_queue_loop";
    }
  | {
      factories: EntityId[];
      kind: "set_stored_order";
      order: Order;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Order".
 */
export type Order =
  | {
      kind: "idle";
    }
  | {
      destination: Tile;
      kind: "attack_move";
    }
  | {
      kind: "support";
      target: EntityId;
    }
  | {
      area: Rect;
      kind: "mine";
    }
  | {
      area: Rect;
      kind: "construct";
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "GroupSlot".
 */
export type GroupSlot = number;
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "MemberEdit".
 */
export type MemberEdit =
  | {
      entities: EntityId[];
      kind: "replace";
    }
  | {
      entities: EntityId[];
      kind: "add";
    }
  | {
      entities: EntityId[];
      kind: "remove";
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Priority".
 */
export type Priority = "high" | "medium" | "low";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "CardinalDirection".
 */
export type CardinalDirection = "n" | "e" | "s" | "w";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "DraftItemRef".
 */
export type DraftItemRef =
  | {
      id: EntityId;
      kind: "persistent";
    }
  | {
      item_index: number;
      kind: "draft";
      local_id: string;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ProductionEdit".
 */
export type ProductionEdit =
  | {
      items: string[];
      kind: "append";
    }
  | {
      items: string[];
      kind: "replace_pending";
    }
  | {
      item_ids: DraftItemRef2[];
      kind: "remove_pending";
    }
  | {
      kind: "cancel_active";
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "DraftItemRef2".
 */
export type DraftItemRef2 =
  | {
      id: QueueItemId;
      kind: "persistent";
    }
  | {
      item_index: number;
      kind: "draft";
      local_id: string;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "FutureOrderPolicy".
 */
export type FutureOrderPolicy = "keep" | "drop_all" | "drop_window";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SkipReason".
 */
export type SkipReason =
  "absent" | "dead" | "wrong_owner" | "incompatible" | "blocked" | "missing_support_target" | "locked_by_later_round";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "OutcomeKind".
 */
export type OutcomeKind = "stalemate" | "win" | "draw";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "StopReason".
 */
export type StopReason = "inactivity" | "absolute_horizon" | "elimination";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Reason".
 */
export type Reason = "no_active_building" | "no_build_ability";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SurvivalStatus".
 */
export type SurvivalStatus = "alive" | "eliminated";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SideId".
 */
export type SideId =
  | {
      kind: "player";
      player_id: number;
    }
  | {
      kind: "team";
      team_id: string;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Direction".
 */
export type Direction = "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Lifecycle".
 */
export type Lifecycle = "site" | "complete";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TerrainCell".
 */
export type TerrainCell = "floor" | "wall";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ControlLimit".
 */
export type ControlLimit = "timestamp" | "single_order";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Multiplayer".
 */
export type Multiplayer =
  | {
      kind: "ffa";
    }
  | {
      assignments: TeamAssignment[];
      kind: "teams";
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Objective".
 */
export type Objective =
  | {
      kind: "timed";
      lock_ticks_per_round: number;
    }
  | {
      kind: "scoreboard";
      rules: ScoreboardRules;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "DrawScoring".
 */
export type DrawScoring = "none" | "all_players";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TiePolicy".
 */
export type TiePolicy = "continue_until_unique" | "shared_victory";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TimePenalty".
 */
export type TimePenalty = "none" | "fastest_opponent_ratio";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "VictoryRule".
 */
export type VictoryRule =
  | {
      kind: "fixed_target";
      points: number;
    }
  | {
      kind: "lead";
      margin: number;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SafeInt".
 */
export type SafeInt = number;
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Transport".
 */
export type Transport = "authoritative" | "inputs_only";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TypeKind".
 */
export type TypeKind = "unit" | "structure";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Neighbors".
 */
export type Neighbors = "four" | "eight";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Shape".
 */
export type Shape = "circle" | "rectangle";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "VisualStyle".
 */
export type VisualStyle = "direct" | "artillery" | "melee";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Command2".
 */
export type Command2 =
  | {
      entities: EntityId[];
      kind: "assign_order";
      order: Order;
    }
  | {
      group: ControlGroupId;
      kind: "assign_group_order";
      order: Order;
    }
  | {
      edit: MemberEdit;
      group: ControlGroupId;
      kind: "edit_group_members";
    }
  | {
      factories: EntityId[];
      group?: ControlGroupId | null;
      kind: "bind_factory_group";
    }
  | {
      entities: EntityId[];
      kind: "set_priority";
      priority: Priority;
    }
  | {
      kind: "place_blueprints";
      output_directions?: CardinalDirection[] | null;
      priority: Priority;
      tiles: Tile[];
      type_key: string;
    }
  | {
      blueprint_ids: EntityId[];
      kind: "configure_blueprints";
      settings: BlueprintSettings;
    }
  | {
      blueprint_ids: EntityId[];
      kind: "cancel_blueprints";
    }
  | {
      edit: ProductionEdit2;
      factories: EntityId[];
      kind: "edit_production";
    }
  | {
      enabled: boolean;
      factories: EntityId[];
      kind: "set_queue_loop";
    }
  | {
      factories: EntityId[];
      kind: "set_stored_order";
      order: Order;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ProductionEdit2".
 */
export type ProductionEdit2 =
  | {
      items: string[];
      kind: "append";
    }
  | {
      items: string[];
      kind: "replace_pending";
    }
  | {
      item_ids: QueueItemId[];
      kind: "remove_pending";
    }
  | {
      kind: "cancel_active";
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "AwardReason".
 */
export type AwardReason = "survival" | "draw" | "none";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ServerMessage".
 */
export type ServerMessage =
  | {
      archive: ArchiveRecord;
      kind: "match_archived";
    }
  | {
      config: MatchConfig;
      fingerprint: Fingerprint;
      guide_url: string;
      kind: "welcome";
      lobby: LobbyState;
      match_id: string;
      phase: Phase;
      timed?: TimedAdjudication | null;
    }
  | {
      kind: "slot_claimed";
      private_token: string;
      slot: number;
    }
  | {
      kind: "lobby_updated";
      lobby: LobbyState;
    }
  | {
      code: string;
      kind: "lobby_update_rejected";
      lobby: LobbyState;
      message: string;
      request_id: string;
    }
  | {
      available_through: number;
      committed_players: number[];
      editable_from: number;
      kind: "planning_opened";
      revision: number;
      round: number;
      time_totals: PlayerTime[];
    }
  | {
      kind: "commit_accepted";
      request_id: string;
      round: number;
    }
  | {
      code: string;
      kind: "commit_rejected";
      message: string;
      request_id: string;
    }
  | {
      end_tick: number;
      kind: "simulation_progress";
      revision: number;
      tick: number;
    }
  | {
      kind: "revision_published";
      outcome: Outcome;
      revision: number;
      score?: RoundScore | null;
      sim_duration_ms: SafeInt;
      time_ratios: PlayerRatio[];
      time_totals: PlayerTime[];
      timed?: TimedAdjudication | null;
      timeline_index: TimelineBucket[];
    }
  | {
      entity_dictionary: EntityRef[];
      index_width: number;
      kind: "snapshot_range";
      revision: number;
      samples: Sample[];
    }
  | {
      events: WorldEvent[];
      kind: "events";
      revision: number;
    }
  | {
      kind: "exact_state";
      revision: number;
      snapshot: WorldState;
      tick: number;
    }
  | {
      buckets: StatsSample[];
      kind: "stats_range";
      revision: number;
    }
  | {
      final_outcome: Outcome;
      kind: "match_finished";
      match_winners: SideId[];
      reason: string;
    }
  | {
      kind: "commands";
      revision: number;
      turns: AcceptedTurn[];
    }
  | {
      command_outcomes: CommandOutcome[];
      kind: "round_result";
      outcome: Outcome;
      parent_revision?: number | null;
      revision: number;
      round: number;
      score?: RoundScore | null;
      sim_duration_ms: SafeInt;
      time_totals: PlayerTime[];
      timed?: TimedAdjudication | null;
      timeline_index: TimelineBucket[];
    }
  | {
      groups: ControlGroupState[];
      kind: "control_groups";
      revision: number;
      tick: number;
    }
  | {
      config: MatchConfig;
      content: Content;
      fingerprint: Fingerprint;
      initial_state: WorldState;
      kind: "replay_bootstrap";
      ledger: AcceptedTurn[];
      precedence: RoundPrecedence[];
    }
  | {
      editable_from: number;
      kind: "round_inputs";
      precedence: RoundPrecedence;
      revision: number;
      turns: AcceptedTurn[];
    }
  | {
      hash: string;
      kind: "reference_hash";
      revision: number;
      tick: number;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ArchiveReason".
 */
export type ArchiveReason = "manual_stop" | "history_exhausted";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ArchiveStatus".
 */
export type ArchiveStatus = "unfinished";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Phase".
 */
export type Phase = "lobby" | "planning" | "simulating" | "finished" | "archived";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TimedStatus".
 */
export type TimedStatus = "planning" | "finished" | "history_exhausted";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Activity".
 */
export type Activity = "combat" | "construction" | "mining" | "movement" | "idle";
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "PresentationEvent".
 */
export type PresentationEvent =
  | {
      blocker_from: Tile;
      blocker_id: EntityId;
      blocker_to: Tile;
      involuntary_entity_id: EntityId;
      kind: "displacement";
      mover_from: Tile;
      mover_id: EntityId;
      mover_to: Tile;
    }
  | {
      entity_id: EntityId;
      from: Tile;
      kind: "move";
      to: Tile;
    }
  | {
      attacker_id: EntityId;
      kind: "attack";
      source_tile: Tile;
      target_id: EntityId;
      target_tile: Tile;
      visual_style: VisualStyle;
    }
  | {
      kind: "impact";
      target_tile: Tile;
      visual_style: VisualStyle;
    }
  | {
      entity_id: EntityId;
      kind: "destroyed";
      tile: Tile;
      visual_style: VisualStyle;
    }
  | {
      kind: "survival";
      transition: SurvivalTransition;
    };
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "WorkerMessage".
 */
export type WorkerMessage =
  | {
      end_tick: number;
      job_id: string;
      kind: "progress";
      revision: number;
      tick: number;
    }
  | {
      checkpoints: WorldState[];
      dictionary: EntityRef[];
      events: WorldEvent[];
      job_id: string;
      kind: "batch";
      revision: number;
      samples: Sample[];
      stats: StatsSample[];
      timeline: TimelineBucket[];
    }
  | {
      command_outcomes: CommandOutcome[];
      final_hash: string;
      job_id: string;
      kind: "complete";
      outcome: Outcome;
      revision: number;
      sim_duration_ms: SafeInt;
    }
  | {
      error_code: string;
      job_id: string;
      kind: "failed";
      message: string;
      revision: number;
    };

export interface ContractCatalog {
  client: ClientEnvelope;
  golden: GoldenWorldFixture;
  guide: GuideManifest;
  score: RoundScore;
  server: ServerEnvelope;
  setup: Setup;
  simulation: SimRequest;
  worker: WorkerEnvelope;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ClientEnvelope".
 */
export interface ClientEnvelope {
  message: ClientMessage;
  schema_version: Version;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "CommitRequest".
 */
export interface CommitRequest {
  draft: TurnDraft;
  request_id: string;
  slot_token: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TurnDraft".
 */
export interface TurnDraft {
  based_on_revision: number;
  commands: DraftCommand[];
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "DraftCommand".
 */
export interface DraftCommand {
  command: Command;
  future_orders: FutureOrderPolicy;
  local_id: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "EntityId".
 */
export interface EntityId {
  birth_command: BirthCommandId;
  item_index: number;
  occurrence: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "BirthCommandId".
 */
export interface BirthCommandId {
  command: CommandId;
  target_index: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "CommandId".
 */
export interface CommandId {
  index: number;
  player: number;
  round: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Tile".
 */
export interface Tile {
  x: number;
  y: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Rect".
 */
export interface Rect {
  max: Tile;
  min: Tile;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ControlGroupId".
 */
export interface ControlGroupId {
  owner: number;
  slot: GroupSlot;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "BlueprintSettings".
 */
export interface BlueprintSettings {
  loop_enabled: boolean;
  order: Order;
  priority: Priority;
  queue: string[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "QueueItemId".
 */
export interface QueueItemId {
  birth_command: BirthCommandId;
  item_index: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "GoldenWorldFixture".
 */
export interface GoldenWorldFixture {
  description: string;
  expected: GoldenExpectation;
  initial_hash: string;
  name: string;
  request: SimRequest;
  schema_version: Version;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "GoldenExpectation".
 */
export interface GoldenExpectation {
  command_outcomes: CommandOutcome[];
  final_hash?: string | null;
  outcome: Outcome;
  states: TickExpectation[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "CommandOutcome".
 */
export interface CommandOutcome {
  applied_entities: EntityId[];
  command_id: CommandId;
  skipped: SkippedTarget[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SkippedTarget".
 */
export interface SkippedTarget {
  entity_id?: EntityId | null;
  reason: SkipReason;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Outcome".
 */
export interface Outcome {
  eliminated: number[];
  kind: OutcomeKind;
  last_progress_tick: number;
  stop_reason: StopReason;
  survival_transitions: SurvivalTransition[];
  surviving_sides: SideId[];
  survivors: number[];
  terminal_state_tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SurvivalTransition".
 */
export interface SurvivalTransition {
  player_id: number;
  reasons: Reason[];
  resolved_tick: number;
  status: SurvivalStatus;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TickExpectation".
 */
export interface TickExpectation {
  banks: ExpectedBank[];
  entities: ExpectedEntity[];
  groups: ControlGroupState[];
  state_tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ExpectedBank".
 */
export interface ExpectedBank {
  bank: number;
  mined: number;
  player_id: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ExpectedEntity".
 */
export interface ExpectedEntity {
  action?: Order | null;
  entity_id: EntityId;
  hp?: number | null;
  paid_matter?: number | null;
  present: boolean;
  tile?: Tile | null;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ControlGroupState".
 */
export interface ControlGroupState {
  id: ControlGroupId;
  latest_order?: SavedOrder | null;
  members: EntityId[];
  order_locks: OrderLock[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SavedOrder".
 */
export interface SavedOrder {
  order: Order;
  source_command_id: CommandId;
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "OrderLock".
 */
export interface OrderLock {
  from_tick: number;
  issued_round: number;
  until_tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SimRequest".
 */
export interface SimRequest {
  checkpoint: WorldState;
  config: MatchConfig;
  content: Content;
  end_tick_exclusive: number;
  entity_dictionary: EntityRef[];
  events: AcceptedTurn[];
  fingerprint: Fingerprint;
  job_id: string;
  minimum_end_tick: number;
  precedence: RoundPrecedence[];
  revision: number;
  schema_version: Version;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "WorldState".
 */
export interface WorldState {
  blueprints: Blueprint[];
  control_groups: ControlGroupState[];
  entities: EntityState[];
  inactivity_deadline: number;
  last_progress_tick: number;
  ore: number[];
  players: PlayerState[];
  rng_state: string;
  schema_version: Version;
  survival_transitions: SurvivalTransition[];
  terrain: Terrain;
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Blueprint".
 */
export interface Blueprint {
  id: EntityId;
  output_direction?: CardinalDirection | null;
  owner: number;
  precedence: EventKey;
  priority: Priority;
  settings?: BlueprintSettings | null;
  settings_command?: BirthCommandId | null;
  site_id?: EntityId | null;
  source_command_id: CommandId;
  tile: Tile;
  type_key: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "EventKey".
 */
export interface EventKey {
  command_index: number;
  player_rank: number;
  round: number;
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "EntityState".
 */
export interface EntityState {
  action: Order;
  blocked_step?: Tile | null;
  blueprint_id?: EntityId | null;
  born_at_tick?: number | null;
  engaged_target?: EntityId | null;
  failed_move_attempts: number;
  goal_settled: boolean;
  hp: number;
  id: EntityId;
  last_move_direction: Direction;
  lifecycle: Lifecycle;
  local_detour: Tile[];
  next_action_tick: number;
  next_move_tick: number;
  order_locks: OrderLock[];
  owner: number;
  paid_matter: number;
  priority: Priority;
  production?: Production | null;
  resolved_destination?: Tile | null;
  support_target?: EntityId | null;
  tile: Tile;
  type_key: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Production".
 */
export interface Production {
  active_item?: ActiveItem | null;
  loop_enabled: boolean;
  occurrence_counters: OccurrenceCounter[];
  output_direction: CardinalDirection;
  output_tile: Tile;
  pending_items: QueueItem[];
  spawn_group?: ControlGroupId | null;
  stored_order: Order;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ActiveItem".
 */
export interface ActiveItem {
  awaiting_output: boolean;
  item_id: QueueItemId;
  occurrence: number;
  paid_matter: number;
  type_key: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "OccurrenceCounter".
 */
export interface OccurrenceCounter {
  item_id: QueueItemId;
  next_occurrence: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "QueueItem".
 */
export interface QueueItem {
  item_id: QueueItemId;
  type_key: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "PlayerState".
 */
export interface PlayerState {
  bank: number;
  counters: SpendCounters;
  currently_eliminated: boolean;
  elimination_reasons: Reason[];
  player_id: number;
  status_since_tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SpendCounters".
 */
export interface SpendCounters {
  damage_dealt: number;
  destroyed_replacement_value: number;
  lost_invested_matter: number;
  mined: number;
  structure_spend: number;
  total_spend: number;
  unit_spend: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Terrain".
 */
export interface Terrain {
  cells: TerrainCell[];
  height: number;
  width: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "MatchConfig".
 */
export interface MatchConfig {
  checkpoint_interval: number;
  control_limit: ControlLimit;
  future_orders: FutureOrdersConfig;
  map_size: number;
  max_tick: number;
  multiplayer: Multiplayer;
  objective: Objective;
  ore_matter_per_start: number;
  player_count: number;
  replay_directory: string;
  schema_version: Version;
  seed: SafeInt;
  simulation_threads: number;
  snapshot_interval: number;
  stall_ticks: number;
  starting_matter: number;
  stop_when_decided: boolean;
  symmetric: boolean;
  ticks_per_second: number;
  transport: Transport;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "FutureOrdersConfig".
 */
export interface FutureOrdersConfig {
  window_ticks?: number | null;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TeamAssignment".
 */
export interface TeamAssignment {
  player_id: number;
  team_id: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ScoreboardRules".
 */
export interface ScoreboardRules {
  draw_scoring: DrawScoring;
  tie_policy: TiePolicy;
  time_penalty: TimePenalty;
  victory_rule: VictoryRule;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Content".
 */
export interface Content {
  schema_version: Version;
  starting_roster: string[];
  types: TypeDefinition[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TypeDefinition".
 */
export interface TypeDefinition {
  construction?: WorkRate | null;
  counts_for_survival: boolean;
  healing?: Healing | null;
  key: string;
  kind: TypeKind;
  matter_cost: number;
  max_hp: number;
  mining?: WorkRate | null;
  movement?: Movement | null;
  production?: ProductionCapability | null;
  provides_build_ability: boolean;
  shape: Shape;
  vision: number;
  weapon?: Weapon | null;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "WorkRate".
 */
export interface WorkRate {
  cooldown: number;
  rate: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Healing".
 */
export interface Healing {
  cooldown: number;
  demand: number;
  hp_per_matter: number;
  range: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Movement".
 */
export interface Movement {
  cooldown: number;
  neighbors: Neighbors;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ProductionCapability".
 */
export interface ProductionCapability {
  rate: number;
  recipes: string[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Weapon".
 */
export interface Weapon {
  cooldown: number;
  damage: number;
  indirect: boolean;
  range: number;
  visual_style: VisualStyle;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "EntityRef".
 */
export interface EntityRef {
  id: EntityId;
  owner: number;
  type_key: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "AcceptedTurn".
 */
export interface AcceptedTurn {
  commands: CommittedCommand[];
  duration_ms: SafeInt;
  player: number;
  round: number;
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "CommittedCommand".
 */
export interface CommittedCommand {
  command: Command2;
  future_orders: FutureOrderPolicy;
  id: CommandId;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Fingerprint".
 */
export interface Fingerprint {
  config_hash: string;
  content_hash: string;
  schema_version: Version;
  sim_build: string;
  target: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "RoundPrecedence".
 */
export interface RoundPrecedence {
  players: number[];
  round: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "GuideManifest".
 */
export interface GuideManifest {
  content_hash: string;
  generated_files: {
    [k: string]: string;
  };
  locale: string;
  rules_build: string;
  schema_version: Version;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "RoundScore".
 */
export interface RoundScore {
  entries: ScoreEntry[];
  match_winners: SideId[];
  round: number;
  victory_rule: VictoryRule;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ScoreEntry".
 */
export interface ScoreEntry {
  adjusted_delta: number;
  adjusted_total: number;
  award_reason: AwardReason;
  credited_players: number[];
  raw_delta: number;
  raw_total: number;
  side_id: SideId;
  surviving_members: number[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ServerEnvelope".
 */
export interface ServerEnvelope {
  message: ServerMessage;
  schema_version: Version;
  server_instance_id: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "ArchiveRecord".
 */
export interface ArchiveRecord {
  actor?: number | null;
  reason: ArchiveReason;
  request_id: string;
  revision: number;
  status: ArchiveStatus;
  stopped_at_unix_ms: SafeInt;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "LobbyState".
 */
export interface LobbyState {
  available_teams: AvailableTeam[];
  can_start: boolean;
  revision: number;
  rule_summary: string;
  slots: LobbySlot[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "AvailableTeam".
 */
export interface AvailableTeam {
  capacity?: number | null;
  label: string;
  team_id: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "LobbySlot".
 */
export interface LobbySlot {
  claimed: boolean;
  connected: boolean;
  profile?: PlayerProfile | null;
  slot: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "PlayerProfile".
 */
export interface PlayerProfile {
  color: string;
  player_id: number;
  team_id?: string | null;
  username: string;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TimedAdjudication".
 */
export interface TimedAdjudication {
  boundary: number;
  eligible_sides: SideId[];
  match_winners: SideId[];
  status: TimedStatus;
  timed_lost_players: number[];
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "PlayerTime".
 */
export interface PlayerTime {
  player_id: number;
  total_ms: SafeInt;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "PlayerRatio".
 */
export interface PlayerRatio {
  player_id: number;
  ratio: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "TimelineBucket".
 */
export interface TimelineBucket {
  activity: Activity;
  affected_entities: number;
  from_tick: number;
  player_id: number;
  severity: number;
  to_tick_exclusive: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Sample".
 */
export interface Sample {
  entities: SampleEntity[];
  ore: OreCell[];
  players: SamplePlayer[];
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SampleEntity".
 */
export interface SampleEntity {
  activity: Activity;
  engaged?: number | null;
  facing: Direction;
  hp: number;
  index: number;
  lifecycle: Lifecycle;
  tile: Tile;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "OreCell".
 */
export interface OreCell {
  index: number;
  remaining: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "SamplePlayer".
 */
export interface SamplePlayer {
  bank: number;
  currently_eliminated: boolean;
  player_id: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "WorldEvent".
 */
export interface WorldEvent {
  event: PresentationEvent;
  sequence: number;
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "StatsSample".
 */
export interface StatsSample {
  players: PlayerStats[];
  tick: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "PlayerStats".
 */
export interface PlayerStats {
  active_workers: number;
  bank: number;
  counters: SpendCounters;
  entity_count: number;
  living_army_value: number;
  living_infrastructure_value: number;
  player_id: number;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "Setup".
 */
export interface Setup {
  available_teams: AvailableTeam[];
  default_port: number;
  match_defaults: MatchConfig;
  schema_version: Version;
}
/**
 * This interface was referenced by `ContractCatalog`'s JSON-Schema
 * via the `definition` "WorkerEnvelope".
 */
export interface WorkerEnvelope {
  message: WorkerMessage;
  schema_version: Version;
}
