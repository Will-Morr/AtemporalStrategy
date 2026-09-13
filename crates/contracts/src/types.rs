use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Tick = u32;
pub type PlayerId = u8;
pub type Revision = u32;
pub type EntityId = String;
pub type CommandId = String;
pub type BlueprintId = String;
pub type QueueItemId = String;
pub type TypeKey = String;
pub type TeamId = String;

/// A JSON integer that survives a browser round trip without precision loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct SafeInt(pub(crate) u64);
impl TryFrom<u64> for SafeInt {
    type Error = String;
    fn try_from(n: u64) -> Result<Self, String> {
        if n <= 9_007_199_254_740_991 {
            Ok(Self(n))
        } else {
            Err("integer exceeds JavaScript safe range".into())
        }
    }
}
impl From<SafeInt> for u64 {
    fn from(n: SafeInt) -> Self {
        n.0
    }
}
impl SafeInt {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Version(u32);
impl Default for Version {
    fn default() -> Self {
        Self(1)
    }
}
impl TryFrom<u32> for Version {
    type Error = String;
    fn try_from(n: u32) -> Result<Self, String> {
        if n == 1 {
            Ok(Self(n))
        } else {
            Err(format!("unsupported schema version {n}"))
        }
    }
}
impl From<Version> for u32 {
    fn from(n: Version) -> Self {
        n.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct GroupSlot(u8);
impl TryFrom<u8> for GroupSlot {
    type Error = String;
    fn try_from(n: u8) -> Result<Self, String> {
        if n < 10 {
            Ok(Self(n))
        } else {
            Err("group slot must be 0–9".into())
        }
    }
}
impl From<GroupSlot> for u8 {
    fn from(n: GroupSlot) -> Self {
        n.0
    }
}

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
macro_rules! choices {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }
    };
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Tile {
    pub x: u16,
    pub y: u16,
}
record!(Rect {
    min: Tile,
    max: Tile
});
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControlGroupId {
    pub owner: PlayerId,
    pub slot: GroupSlot,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SideId {
    Player { player_id: PlayerId },
    Team { team_id: TeamId },
}
choices!(Priority { High, Medium, Low });
choices!(Reason {
    NoActiveBuilding,
    NoBuildAbility
});
choices!(ControlLimit {
    Timestamp,
    SingleOrder
});
choices!(Transport {
    Authoritative,
    InputsOnly
});
choices!(TiePolicy {
    ContinueUntilUnique,
    SharedVictory
});
choices!(DrawScoring { None, AllPlayers });
choices!(TimePenalty {
    None,
    FastestOpponentRatio
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VictoryRule {
    FixedTarget { points: f64 },
    Lead { margin: f64 },
}
record!(ScoreboardRules {
    victory_rule: VictoryRule,
    tie_policy: TiePolicy,
    draw_scoring: DrawScoring,
    time_penalty: TimePenalty
});
impl Default for ScoreboardRules {
    fn default() -> Self {
        Self {
            victory_rule: VictoryRule::FixedTarget { points: 5.0 },
            tie_policy: TiePolicy::ContinueUntilUnique,
            draw_scoring: DrawScoring::None,
            time_penalty: TimePenalty::None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Objective {
    Timed { lock_ticks_per_round: Tick },
    Scoreboard { rules: ScoreboardRules },
}
record!(TeamAssignment {
    player_id: PlayerId,
    team_id: TeamId
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Multiplayer {
    Ffa {},
    Teams { assignments: Vec<TeamAssignment> },
}
record!(FutureOrdersConfig { window_ticks: Option<Tick> });
record!(MatchConfig {
    schema_version: Version,
    seed: SafeInt,
    map_size: u16,
    player_count: u8,
    symmetric: bool,
    multiplayer: Multiplayer,
    objective: Objective,
    control_limit: ControlLimit,
    future_orders: FutureOrdersConfig,
    max_tick: Tick,
    stall_ticks: Tick,
    ticks_per_second: u32,
    starting_matter: f64,
    simulation_threads: u16,
    checkpoint_interval: Tick,
    snapshot_interval: Tick,
    transport: Transport,
    replay_directory: String
});
record!(AvailableTeam { team_id: TeamId, label: String, capacity: Option<u8> });
// Setup permits lobby-selected teams; MatchConfig is pinned only at Start.
record!(Setup { schema_version: Version, match_defaults: MatchConfig, available_teams: Vec<AvailableTeam>, default_port: u16 });
record!(Fingerprint {
    schema_version: Version,
    sim_build: String,
    target: String,
    config_hash: String,
    content_hash: String
});
choices!(TypeKind { Unit, Structure });
choices!(Shape { Circle, Rectangle });
choices!(Neighbors { Four, Eight });
choices!(VisualStyle {
    Direct,
    Artillery,
    Melee
});
record!(Movement {
    neighbors: Neighbors,
    cooldown: Tick
});
record!(Weapon {
    range: f64,
    damage: f64,
    cooldown: Tick,
    indirect: bool,
    visual_style: VisualStyle
});
record!(WorkRate {
    rate: f64,
    cooldown: Tick
});
record!(ProductionCapability { rate: f64, recipes: Vec<TypeKey> });
record!(Healing {
    range: f64,
    hp_per_matter: f64,
    demand: f64,
    cooldown: Tick
});
record!(TypeDefinition { key: TypeKey, kind: TypeKind, shape: Shape, matter_cost: f64, max_hp: f64,
    counts_for_survival: bool, provides_build_ability: bool, movement: Option<Movement>, vision: f64,
    weapon: Option<Weapon>, mining: Option<WorkRate>, construction: Option<WorkRate>, production: Option<ProductionCapability>, healing: Option<Healing> });
record!(Content { schema_version: Version, types: Vec<TypeDefinition>, starting_roster: Vec<TypeKey> });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Order {
    Idle {},
    AttackMove { destination: Tile },
    Support { target: EntityId },
    Mine { area: Rect },
    Construct { area: Rect },
}
choices!(FutureOrderPolicy {
    Keep,
    DropAll,
    DropWindow
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemberEdit {
    Replace { entities: Vec<EntityId> },
    Add { entities: Vec<EntityId> },
    Remove { entities: Vec<EntityId> },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProductionEdit {
    Append { items: Vec<TypeKey> },
    ReplacePending { items: Vec<TypeKey> },
    RemovePending { item_ids: Vec<QueueItemId> },
    CancelActive {},
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    AssignOrder {
        entities: Vec<EntityId>,
        order: Order,
    },
    AssignGroupOrder {
        group: ControlGroupId,
        order: Order,
    },
    EditGroupMembers {
        group: ControlGroupId,
        edit: MemberEdit,
    },
    BindFactoryGroup {
        factories: Vec<EntityId>,
        group: Option<ControlGroupId>,
    },
    SetPriority {
        entities: Vec<EntityId>,
        priority: Priority,
    },
    PlaceBlueprints {
        type_key: TypeKey,
        tiles: Vec<Tile>,
        priority: Priority,
    },
    CancelBlueprints {
        blueprint_ids: Vec<BlueprintId>,
    },
    EditProduction {
        factories: Vec<EntityId>,
        edit: ProductionEdit,
    },
    SetQueueLoop {
        factories: Vec<EntityId>,
        enabled: bool,
    },
    SetStoredOrder {
        factories: Vec<EntityId>,
        order: Order,
    },
}
record!(DraftCommand {
    command: Command,
    future_orders: FutureOrderPolicy
});
record!(TurnDraft { based_on_revision: Revision, tick: Tick, commands: Vec<DraftCommand> });
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SuppressionTarget {
    EntityComponent { entity_id: EntityId },
    EntireGroupOrder { group: ControlGroupId },
}
record!(Suppression {
    source_command_id: CommandId,
    historical_command_id: CommandId,
    target: SuppressionTarget
});
record!(CommittedCommand { id: CommandId, command: Command, future_orders: FutureOrderPolicy, suppressions: Vec<Suppression> });
record!(CommitRequest {
    request_id: String,
    slot_token: String,
    draft: TurnDraft
});
record!(AcceptedTurn { player: PlayerId, round: u32, tick: Tick, commands: Vec<CommittedCommand>, duration_ms: SafeInt });
record!(SkippedTarget { entity_id: Option<EntityId>, reason: SkipReason });
choices!(SkipReason {
    Absent,
    Dead,
    WrongOwner,
    Incompatible,
    Blocked,
    MissingSupportTarget
});
record!(CommandOutcome { command_id: CommandId, applied_entities: Vec<EntityId>, skipped: Vec<SkippedTarget> });
record!(SavedOrder {
    source_command_id: CommandId,
    tick: Tick,
    order: Order
});
record!(ControlGroupState { id: ControlGroupId, members: Vec<EntityId>, latest_order: Option<SavedOrder> });
choices!(Lifecycle { Site, Complete });
choices!(Direction {
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Nw
});
record!(QueueItem {
    item_id: QueueItemId,
    type_key: TypeKey
});
record!(ActiveItem {
    item_id: QueueItemId,
    occurrence: u32,
    type_key: TypeKey,
    paid_matter: f64,
    awaiting_output: bool
});
record!(Production { pending_items: Vec<QueueItem>, active_item: Option<ActiveItem>, loop_enabled: bool, stored_order: Order,
    output_tile: Tile, occurrence_counters: BTreeMap<QueueItemId, u32>, spawn_group: Option<ControlGroupId> });
record!(EntityState { id: EntityId, owner: PlayerId, type_key: TypeKey, tile: Tile, last_move_direction: Direction,
    hp: f64, paid_matter: f64, lifecycle: Lifecycle, blueprint_id: Option<BlueprintId>, action: Order, priority: Priority,
    next_action_tick: Tick, next_move_tick: Tick, production: Option<Production>, support_target: Option<EntityId> });
record!(Blueprint { id: BlueprintId, owner: PlayerId, type_key: TypeKey, tile: Tile, priority: Priority,
    source_command_id: CommandId, precedence: EventKey, site_id: Option<EntityId> });
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventKey {
    pub tick: Tick,
    pub round: u32,
    pub player_rank: u8,
    pub command_index: u32,
}
record!(SpendCounters {
    mined: f64,
    total_spend: f64,
    unit_spend: f64,
    structure_spend: f64,
    lost_invested_matter: f64,
    destroyed_replacement_value: f64,
    damage_dealt: f64
});
record!(PlayerState { player_id: PlayerId, bank: f64, counters: SpendCounters, currently_eliminated: bool,
    status_since_tick: Tick, elimination_reasons: Vec<Reason> });
choices!(TerrainCell { Floor, Wall });
record!(Terrain { width: u16, height: u16, cells: Vec<TerrainCell> });
/// Digest => canonical preimage, persisted for collision detection across checkpoints.
pub type IdentityRegistry = BTreeMap<EntityId, String>;
record!(WorldState { schema_version: Version, tick: Tick, last_progress_tick: Tick, inactivity_deadline: Tick,
    terrain: Terrain, ore: Vec<f64>, players: Vec<PlayerState>, entities: Vec<EntityState>, blueprints: Vec<Blueprint>,
    control_groups: Vec<ControlGroupState>, survival_transitions: Vec<SurvivalTransition>, deterministic_identity_state: IdentityRegistry, rng_state: String });
choices!(OutcomeKind {
    Stalemate,
    Win,
    Draw
});
choices!(StopReason {
    Inactivity,
    AbsoluteHorizon
});
choices!(SurvivalStatus { Alive, Eliminated });
record!(SurvivalTransition { player_id: PlayerId, resolved_tick: Tick, status: SurvivalStatus, reasons: Vec<Reason> });
record!(Outcome { kind: OutcomeKind, stop_reason: StopReason, terminal_state_tick: Tick, last_progress_tick: Tick,
    survivors: Vec<PlayerId>, eliminated: Vec<PlayerId>, surviving_sides: Vec<SideId>, survival_transitions: Vec<SurvivalTransition> });
choices!(AwardReason {
    Survival,
    Draw,
    None
});
record!(ScoreEntry { side_id: SideId, surviving_members: Vec<PlayerId>, credited_players: Vec<PlayerId>, award_reason: AwardReason,
    raw_delta: f64, adjusted_delta: f64, raw_total: f64, adjusted_total: f64 });
record!(RoundScore { round: u32, entries: Vec<ScoreEntry>, victory_rule: VictoryRule, match_winners: Vec<SideId> });
record!(PlayerTime {
    player_id: PlayerId,
    total_ms: SafeInt
});
record!(PlayerRatio {
    player_id: PlayerId,
    ratio: f64
});
record!(SimRequest { schema_version: Version, job_id: String, revision: Revision, fingerprint: Fingerprint, config: MatchConfig,
    content: Content, checkpoint: WorldState, events: Vec<AcceptedTurn>, precedence: Vec<RoundPrecedence>,
    suppressions: Vec<Suppression>, end_tick_exclusive: Tick, minimum_end_tick: Tick });
record!(RoundPrecedence { round: u32, players: Vec<PlayerId> });
record!(PlayerStats {
    player_id: PlayerId,
    bank: f64,
    counters: SpendCounters,
    living_army_value: f64,
    living_infrastructure_value: f64,
    entity_count: u32,
    active_workers: u32
});
record!(StatsSample { tick: Tick, players: Vec<PlayerStats> });
choices!(Activity {
    Combat,
    Construction,
    Mining,
    Movement,
    Idle
});
record!(TimelineBucket {
    from_tick: Tick,
    to_tick_exclusive: Tick,
    player_id: PlayerId,
    activity: Activity,
    affected_entities: u32,
    severity: f64
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresentationEvent {
    Move {
        entity_id: EntityId,
        from: Tile,
        to: Tile,
    },
    Attack {
        attacker_id: EntityId,
        source_tile: Tile,
        target_id: EntityId,
        target_tile: Tile,
        visual_style: VisualStyle,
    },
    Impact {
        target_tile: Tile,
        visual_style: VisualStyle,
    },
    Destroyed {
        entity_id: EntityId,
        tile: Tile,
        visual_style: VisualStyle,
    },
    Survival {
        transition: SurvivalTransition,
    },
}
record!(WorldEvent {
    tick: Tick,
    sequence: u32,
    event: PresentationEvent
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Progress {
        job_id: String,
        revision: Revision,
        tick: Tick,
        end_tick: Tick,
    },
    Batch {
        job_id: String,
        revision: Revision,
        snapshots: Vec<WorldState>,
        checkpoints: Vec<WorldState>,
        stats: Vec<StatsSample>,
        events: Vec<WorldEvent>,
    },
    Complete {
        job_id: String,
        revision: Revision,
        outcome: Outcome,
        final_hash: String,
        sim_duration_ms: SafeInt,
        command_outcomes: Vec<CommandOutcome>,
    },
    Failed {
        job_id: String,
        revision: Revision,
        error_code: String,
        message: String,
    },
}
record!(PlayerProfile { player_id: PlayerId, username: String, color: String, team_id: Option<TeamId> });
record!(LobbySlot { slot: PlayerId, claimed: bool, connected: bool, profile: Option<PlayerProfile> });
record!(LobbyState { revision: Revision, slots: Vec<LobbySlot>, available_teams: Vec<AvailableTeam>, can_start: bool, rule_summary: String });
choices!(Phase {
    Lobby,
    Planning,
    Simulating,
    Finished
});
record!(GuideManifest { schema_version: Version, content_hash: String, rules_build: String, locale: String,
    generated_files: BTreeMap<String, String> });
record!(HistoryComponent { tick: Tick, command_id: CommandId, command: Command, suppressed_by: Vec<Suppression> });
record!(PreviewInterval {
    after_tick: Tick,
    through_tick: Tick
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Hello {
        protocol_version: Version,
        last_revision: Option<Revision>,
        slot_token: Option<String>,
    },
    ClaimSlot {
        slot: PlayerId,
        username: String,
        color: String,
        team_id: Option<TeamId>,
    },
    ReleaseSlot {
        slot_token: String,
    },
    UpdateLobbyProfile {
        request_id: String,
        slot_token: String,
        username: Option<String>,
        color: Option<String>,
        team_id: Option<TeamId>,
    },
    StartMatch {
        slot_token: String,
        based_on_lobby_revision: Revision,
    },
    PlanningReady {
        round: u32,
        revision: Revision,
    },
    Commit {
        request: CommitRequest,
    },
    GetSnapshotRange {
        revision: Revision,
        from_tick: Tick,
        to_tick: Tick,
        stride: Tick,
    },
    GetExactState {
        revision: Revision,
        tick: Tick,
    },
    GetStats {
        revision: Revision,
        from_tick: Tick,
        to_tick: Tick,
        bucket_width: Tick,
    },
    PreviewFutureOrders {
        based_on_revision: Revision,
        tick: Tick,
        draft_command: DraftCommand,
        preceding_commands: Vec<DraftCommand>,
    },
    GetEntityOrderHistory {
        revision: Revision,
        entity_ids: Vec<EntityId>,
    },
    GetControlGroups {
        revision: Revision,
        tick: Tick,
        player: PlayerId,
    },
    GetGroupOrderHistory {
        revision: Revision,
        group: ControlGroupId,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerMessage {
    Welcome {
        match_id: String,
        config: MatchConfig,
        phase: Phase,
        lobby: LobbyState,
        fingerprint: Fingerprint,
        guide_url: String,
    },
    SlotClaimed {
        slot: PlayerId,
        private_token: String,
    },
    LobbyUpdated {
        lobby: LobbyState,
    },
    LobbyUpdateRejected {
        request_id: String,
        code: String,
        message: String,
        lobby: LobbyState,
    },
    PlanningOpened {
        round: u32,
        revision: Revision,
        editable_from: Tick,
        available_through: Tick,
        committed_players: Vec<PlayerId>,
        time_totals: Vec<PlayerTime>,
    },
    CommitAccepted {
        request_id: String,
        round: u32,
    },
    CommitRejected {
        request_id: String,
        code: String,
        message: String,
    },
    SimulationProgress {
        revision: Revision,
        tick: Tick,
        end_tick: Tick,
    },
    RevisionPublished {
        revision: Revision,
        outcome: Outcome,
        timeline_index: Vec<TimelineBucket>,
        score: Option<RoundScore>,
        time_totals: Vec<PlayerTime>,
        time_ratios: Vec<PlayerRatio>,
        sim_duration_ms: SafeInt,
    },
    SnapshotRange {
        revision: Revision,
        samples: Vec<WorldState>,
    },
    ExactState {
        revision: Revision,
        tick: Tick,
        snapshot: WorldState,
    },
    StatsRange {
        revision: Revision,
        buckets: Vec<StatsSample>,
    },
    MatchFinished {
        match_winners: Vec<SideId>,
        final_outcome: Outcome,
        reason: String,
    },
    FutureOrdersPreview {
        revision: Revision,
        affected_components: Vec<Suppression>,
        counts_by_kind: BTreeMap<String, u32>,
        interval: Option<PreviewInterval>,
    },
    OrderHistory {
        revision: Revision,
        components: Vec<HistoryComponent>,
    },
    ControlGroups {
        revision: Revision,
        tick: Tick,
        groups: Vec<ControlGroupState>,
    },
    ReplayBootstrap {
        fingerprint: Fingerprint,
        config: MatchConfig,
        content: Content,
        initial_state: Box<WorldState>,
        ledger: Vec<AcceptedTurn>,
        precedence: Vec<RoundPrecedence>,
    },
    RoundInputs {
        revision: Revision,
        turns: Vec<AcceptedTurn>,
        precedence: RoundPrecedence,
        editable_from: Tick,
    },
    ReferenceHash {
        revision: Revision,
        tick: Tick,
        hash: String,
    },
}
// Every frame carries a version, including worker frames and messages without a Hello.
record!(ClientEnvelope {
    schema_version: Version,
    message: ClientMessage
});
record!(ServerEnvelope {
    schema_version: Version,
    server_instance_id: String,
    message: ServerMessage
});
record!(WorkerEnvelope {
    schema_version: Version,
    message: WorkerMessage
});
// Export root: all shared boundary types are reachable from this schema.
record!(ContractCatalog {
    client: ClientEnvelope,
    server: ServerEnvelope,
    worker: WorkerEnvelope,
    simulation: SimRequest,
    setup: Setup,
    guide: GuideManifest,
    score: RoundScore,
    golden: GoldenWorldFixture
});

macro_rules! bounded_schema {
    ($ty:ty, $min:expr, $max:expr) => {
        impl JsonSchema for $ty {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($ty).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type":"integer", "minimum":$min, "maximum":$max})
            }
        }
    };
}
bounded_schema!(Version, 1, 1);
bounded_schema!(GroupSlot, 0, 9);
bounded_schema!(SafeInt, 0, 9007199254740991_u64);

record!(ExpectedEntity { entity_id: EntityId, present: bool, tile: Option<Tile>, hp: Option<f64>, paid_matter: Option<f64>, action: Option<Order> });
record!(ExpectedBank {
    player_id: PlayerId,
    bank: f64,
    mined: f64
});
record!(TickExpectation { state_tick: Tick, entities: Vec<ExpectedEntity>, banks: Vec<ExpectedBank>, groups: Vec<ControlGroupState> });
record!(GoldenExpectation { final_hash: Option<String>, outcome: Outcome, states: Vec<TickExpectation>, command_outcomes: Vec<CommandOutcome> });
record!(GoldenWorldFixture {
    schema_version: Version,
    name: String,
    description: String,
    request: SimRequest,
    initial_hash: String,
    expected: GoldenExpectation
});
