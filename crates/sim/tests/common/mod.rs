//! Hand-seeded tiny worlds for behavioral fixtures: ASCII terrain, genesis entities, turns.
#![allow(dead_code)]
use atemporal_sim::*;
use std::sync::atomic::AtomicBool;

pub fn tile(x: u16, y: u16) -> Tile {
    Tile { x, y }
}
pub fn rect(x0: u16, y0: u16, x1: u16, y1: u16) -> Rect {
    Rect {
        min: tile(x0, y0),
        max: tile(x1, y1),
    }
}
pub fn attack_move(x: u16, y: u16) -> Order {
    Order::AttackMove {
        destination: tile(x, y),
    }
}

pub struct Run {
    pub result: RunResult,
    pub events: Vec<WorldEvent>,
    pub checkpoints: Vec<WorldState>,
}
impl Run {
    pub fn attacks(&self, attacker: &EntityId) -> Vec<Tick> {
        self.events
            .iter()
            .filter(|e| matches!(&e.event, PresentationEvent::Attack { attacker_id, .. } if attacker_id == attacker))
            .map(|e| e.tick)
            .collect()
    }
    pub fn destroyed(&self, id: &EntityId) -> Option<Tick> {
        self.events
            .iter()
            .find(|e| matches!(&e.event, PresentationEvent::Destroyed { entity_id, .. } if entity_id == id))
            .map(|e| e.tick)
    }
    pub fn displacements(&self) -> Vec<Tick> {
        self.events
            .iter()
            .filter(|e| matches!(e.event, PresentationEvent::Displacement { .. }))
            .map(|e| e.tick)
            .collect()
    }
    pub fn outcome(&self, command: &CommandId) -> &CommandOutcome {
        self.result
            .command_outcomes
            .iter()
            .find(|o| o.command_id == *command)
            .expect("command outcome recorded")
    }
}

pub struct World {
    pub config: MatchConfig,
    pub content: Content,
    pub state: WorldState,
    pub events: Vec<AcceptedTurn>,
    slots: Vec<u32>,
}

impl World {
    /// `#` is rock, anything else is floor. Rows must share one width.
    pub fn new(rows: &[&str]) -> Self {
        let content = load_content(include_str!("../../../../config/content.yaml")).unwrap();
        Self::with_content(rows, content)
    }

    pub fn with_content(rows: &[&str], content: Content) -> Self {
        let height = rows.len() as u16;
        let width = rows[0].len() as u16;
        let mut config =
            atemporal_content::load_setup(include_str!("../../../../config/game.yaml"))
                .unwrap()
                .match_defaults;
        config.map_size = (width.max(height).max(8) + 1) & !1;
        config.max_tick = 2000;
        config.stall_ticks = 50;
        config.snapshot_interval = 1;
        config.checkpoint_interval = 10;
        config.simulation_threads = 1;
        config.starting_matter = 0.0;
        let cells = rows
            .iter()
            .flat_map(|r| {
                assert_eq!(r.len() as u16, width, "ragged fixture map");
                r.bytes().map(|b| {
                    if b == b'#' {
                        TerrainCell::Wall
                    } else {
                        TerrainCell::Floor
                    }
                })
            })
            .collect();
        let players = (0..config.player_count)
            .map(|player_id| PlayerState {
                player_id,
                bank: 0.0,
                counters: SpendCounters {
                    mined: 0.0,
                    total_spend: 0.0,
                    unit_spend: 0.0,
                    structure_spend: 0.0,
                    lost_invested_matter: 0.0,
                    destroyed_replacement_value: 0.0,
                    damage_dealt: 0.0,
                },
                currently_eliminated: false,
                status_since_tick: 0,
                elimination_reasons: vec![],
            })
            .collect();
        let control_groups = (0..config.player_count)
            .flat_map(|owner| {
                (0..10u8).map(move |slot| ControlGroupState {
                    id: ControlGroupId {
                        owner,
                        slot: slot.try_into().unwrap(),
                    },
                    members: vec![],
                    latest_order: None,
                    order_locks: vec![],
                })
            })
            .collect();
        let state = WorldState {
            schema_version: Version::default(),
            tick: 0,
            last_progress_tick: 0,
            inactivity_deadline: config.stall_ticks,
            terrain: Terrain {
                width,
                height,
                cells,
            },
            ore: vec![0.0; usize::from(width) * usize::from(height)],
            players,
            entities: vec![],
            blueprints: vec![],
            control_groups,
            survival_transitions: vec![],
            rng_state: "fixture:no_rng:v2".into(),
        };
        Self {
            slots: vec![0; usize::from(config.player_count)],
            config,
            content,
            state,
            events: vec![],
        }
    }

    pub fn def(&self, key: &str) -> &TypeDefinition {
        self.content.types.iter().find(|t| t.key == key).unwrap()
    }
    pub fn bank(&mut self, player: PlayerId, matter: f64) {
        self.state.players[usize::from(player)].bank = matter;
    }
    pub fn ore(&mut self, t: Tile, matter: f64) {
        let i = usize::from(t.y) * usize::from(self.state.terrain.width) + usize::from(t.x);
        self.state.ore[i] = matter;
    }
    pub fn group(&self, owner: PlayerId, slot: u8) -> ControlGroupId {
        ControlGroupId {
            owner,
            slot: slot.try_into().unwrap(),
        }
    }

    /// Complete genesis entity at full health, idle.
    pub fn spawn(&mut self, player: PlayerId, key: &str, x: u16, y: u16) -> EntityId {
        self.spawn_with(player, key, x, y, Order::Idle {})
    }
    pub fn spawn_with(
        &mut self,
        player: PlayerId,
        key: &str,
        x: u16,
        y: u16,
        action: Order,
    ) -> EntityId {
        let slot = self.slots[usize::from(player)];
        self.slots[usize::from(player)] += 1;
        let id = identity::genesis(player, slot).unwrap();
        let def = self.def(key).clone();
        let production = def.production.as_ref().map(|_| Production {
            pending_items: vec![],
            active_item: None,
            loop_enabled: false,
            stored_order: Order::Idle {},
            output_tile: tile(x + 1, y),
            occurrence_counters: vec![],
            spawn_group: None,
            output_direction: CardinalDirection::E,
        });
        let support_target = match &action {
            Order::Support { target } => Some(target.clone()),
            _ => None,
        };
        self.state.entities.push(EntityState {
            id: id.clone(),
            owner: player,
            type_key: key.into(),
            tile: tile(x, y),
            last_move_direction: Direction::N,
            hp: def.max_hp,
            paid_matter: def.matter_cost,
            lifecycle: Lifecycle::Complete,
            blueprint_id: None,
            action,
            priority: Priority::Medium,
            next_action_tick: 0,
            next_move_tick: 0,
            production,
            support_target,
            engaged_target: None,
            resolved_destination: None,
            failed_move_attempts: 0,
            blocked_step: None,
            born_at_tick: None,
            order_locks: vec![],
            goal_settled: false,
            local_detour: vec![],
        });
        id
    }

    /// A funded construction site with its blueprint record, `paid` matter already invested.
    pub fn site(&mut self, player: PlayerId, key: &str, x: u16, y: u16, paid: f64) -> EntityId {
        let id = self.spawn(player, key, x, y);
        let def = self.def(key).clone();
        let e = self.state.entities.last_mut().unwrap();
        e.lifecycle = Lifecycle::Site;
        e.paid_matter = paid;
        e.hp = def.max_hp * paid / def.matter_cost;
        e.blueprint_id = Some(id.clone());
        e.production = None;
        self.state.blueprints.push(Blueprint {
            id: id.clone(),
            owner: player,
            type_key: key.into(),
            tile: tile(x, y),
            priority: Priority::Medium,
            source_command_id: identity::command_id(0, player, 0),
            precedence: EventKey {
                tick: 0,
                round: 0,
                player_rank: player,
                command_index: 0,
            },
            site_id: Some(id.clone()),
            output_direction: None,
        });
        id
    }

    pub fn entity_mut(&mut self, id: &EntityId) -> &mut EntityState {
        self.state
            .entities
            .iter_mut()
            .find(|e| e.id == *id)
            .unwrap()
    }

    pub fn turn(
        &mut self,
        round: u32,
        player: PlayerId,
        tick: Tick,
        commands: Vec<Command>,
    ) -> Vec<CommandId> {
        self.turn_with(round, player, tick, FutureOrderPolicy::Keep, commands)
    }
    pub fn turn_with(
        &mut self,
        round: u32,
        player: PlayerId,
        tick: Tick,
        policy: FutureOrderPolicy,
        commands: Vec<Command>,
    ) -> Vec<CommandId> {
        self.turn_from(round, player, tick, policy, 0, commands)
    }
    /// Several turns of one round at different ticks need distinct command indices.
    pub fn turn_from(
        &mut self,
        round: u32,
        player: PlayerId,
        tick: Tick,
        policy: FutureOrderPolicy,
        first_index: u32,
        commands: Vec<Command>,
    ) -> Vec<CommandId> {
        let ids: Vec<CommandId> = (0..commands.len() as u32)
            .map(|i| identity::command_id(round, player, first_index + i))
            .collect();
        self.events.push(AcceptedTurn {
            player,
            round,
            tick,
            commands: commands
                .into_iter()
                .zip(ids.iter().cloned())
                .map(|(command, id)| CommittedCommand {
                    id,
                    command,
                    future_orders: policy,
                })
                .collect(),
            duration_ms: 1000.try_into().unwrap(),
        });
        ids
    }

    pub fn request(&self) -> SimRequest {
        let rounds: std::collections::BTreeSet<u32> = self.events.iter().map(|t| t.round).collect();
        SimRequest {
            schema_version: Version::default(),
            job_id: "fixture".into(),
            revision: rounds.len() as u32,
            fingerprint: Fingerprint {
                schema_version: Version::default(),
                sim_build: "test".into(),
                target: "test".into(),
                config_hash: String::new(),
                content_hash: String::new(),
            },
            config: self.config.clone(),
            content: self.content.clone(),
            checkpoint: self.state.clone(),
            events: self.events.clone(),
            precedence: rounds
                .into_iter()
                .map(|r| identity::round_precedence(r, self.config.player_count).unwrap())
                .collect(),
            end_tick_exclusive: self.config.max_tick,
            minimum_end_tick: 0,
            entity_dictionary: vec![],
        }
    }

    pub fn run(&self) -> Run {
        run_request(&self.request())
    }
    pub fn at(&self, tick: Tick) -> WorldState {
        reconstruct(&self.request(), tick).unwrap()
    }
}

pub fn run_request(request: &SimRequest) -> Run {
    let mut events = vec![];
    let mut checkpoints = vec![];
    let mut sink = |o: Output| match o {
        Output::Events(e) => events.extend(e),
        Output::Checkpoint(c) => checkpoints.push(c),
        _ => {}
    };
    let result = run(request, &AtomicBool::new(false), &mut sink).unwrap();
    Run {
        result,
        events,
        checkpoints,
    }
}

pub fn entity<'a>(state: &'a WorldState, id: &EntityId) -> &'a EntityState {
    state
        .entities
        .iter()
        .find(|e| e.id == *id)
        .unwrap_or_else(|| panic!("entity present at S[{}]", state.tick))
}
pub fn present(state: &WorldState, id: &EntityId) -> bool {
    state.entities.iter().any(|e| e.id == *id)
}

/// Every checkpoint rerun must reproduce the full replay's final hash and stop tick.
pub fn assert_checkpoint_equivalence(world: &World, full: &Run) {
    for checkpoint in &full.checkpoints {
        if checkpoint.tick == 0 || checkpoint.tick >= full.result.outcome.terminal_state_tick {
            continue;
        }
        let mut request = world.request();
        request.checkpoint = checkpoint.clone();
        let partial = run_request(&request);
        assert_eq!(
            partial.result.final_hash, full.result.final_hash,
            "checkpoint {} diverges from full replay",
            checkpoint.tick
        );
        assert_eq!(
            partial.result.outcome.terminal_state_tick,
            full.result.outcome.terminal_state_tick
        );
    }
}
