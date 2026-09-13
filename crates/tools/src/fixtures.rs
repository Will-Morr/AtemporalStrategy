//! Authored tiny-world acceptance data; deliberately no simulation implementation.
use atemporal_contracts::{identity::*, scoring::*, *};
use serde_json::json;
use std::{fs, path::Path};
fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("authored fixture has valid shape")
}
fn tile(x: u16, y: u16) -> Tile {
    Tile { x, y }
}
fn entity(player: u8, slot: u32, key: &str, position: Tile, hp: f64) -> EntityState {
    EntityState {
        id: genesis(player, slot).unwrap(),
        owner: player,
        type_key: key.into(),
        tile: position,
        last_move_direction: if player == 0 {
            Direction::Se
        } else {
            Direction::Nw
        },
        hp,
        paid_matter: 0.,
        lifecycle: Lifecycle::Complete,
        blueprint_id: None,
        action: Order::Idle {},
        priority: Priority::Medium,
        next_action_tick: 0,
        next_move_tick: 0,
        production: None,
        support_target: None,
        engaged_target: None,
        resolved_destination: None,
        failed_move_attempts: 0,
        blocked_step: None,
        born_at_tick: None,
        order_locks: vec![],
        goal_settled: false,
        local_detour: vec![],
    }
}
fn base(name: &str, description: &str, max_tick: u32) -> GoldenWorldFixture {
    let mut content =
        atemporal_content::load_content(include_str!("../../../config/content.yaml")).unwrap();
    // Economic fixtures remove weapon capabilities, not engine type-name branches.
    for t in &mut content.types {
        t.weapon = None;
    }
    let mut config = atemporal_content::load_setup(include_str!("../../../config/game.yaml"))
        .unwrap()
        .match_defaults;
    config.map_size = 8;
    config.max_tick = max_tick;
    config.stall_ticks = 3;
    config.snapshot_interval = 1;
    config.checkpoint_interval = 2;
    config.simulation_threads = 1;
    config.starting_matter = 0.;
    let entities = vec![
        entity(0, 0, "miner", tile(1, 1), 60.),
        entity(0, 1, "constructor", tile(1, 2), 80.),
        entity(0, 2, "turret", tile(0, 0), 200.),
        entity(1, 0, "miner", tile(6, 6), 60.),
        entity(1, 1, "constructor", tile(6, 5), 80.),
        entity(1, 2, "turret", tile(7, 7), 200.),
    ];
    let players=(0..2).map(|p|decode(json!({"player_id":p,"bank":0.,"counters":{"mined":0.,"total_spend":0.,"unit_spend":0.,"structure_spend":0.,"lost_invested_matter":0.,"destroyed_replacement_value":0.,"damage_dealt":0.},"currently_eliminated":false,"status_since_tick":0,"elimination_reasons":[]}))).collect();
    let control_groups = (0..2)
        .flat_map(|owner| {
            (0..10).map(move |slot| ControlGroupState {
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
    let checkpoint = WorldState {
        schema_version: Version::default(),
        tick: 0,
        last_progress_tick: 0,
        inactivity_deadline: 3,
        terrain: Terrain {
            width: 8,
            height: 8,
            cells: vec![TerrainCell::Floor; 64],
        },
        ore: vec![0.; 64],
        players,
        entities,
        blueprints: vec![],
        control_groups,
        survival_transitions: vec![],
        rng_state: "fixture:no_rng:v2".into(),
    };
    let request = SimRequest {
        schema_version: Version::default(),
        job_id: name.into(),
        revision: 1,
        fingerprint: Fingerprint {
            schema_version: Version::default(),
            sim_build: "tiny-world-contract-v2".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            config_hash: String::new(),
            content_hash: String::new(),
        },
        config,
        content,
        checkpoint,
        events: vec![],
        precedence: vec![],
        end_tick_exclusive: max_tick,
        minimum_end_tick: 0,
        entity_dictionary: vec![],
    };
    GoldenWorldFixture {
        schema_version: Version::default(),
        name: name.into(),
        description: description.into(),
        request,
        initial_hash: String::new(),
        expected: GoldenExpectation {
            final_hash: None,
            outcome: result(3, 0, &[0, 1], StopReason::Inactivity),
            states: vec![],
            command_outcomes: vec![],
        },
    }
}
fn result(end: u32, last: u32, alive: &[u8], reason: StopReason) -> Outcome {
    Outcome {
        kind: classify(2, alive).unwrap(),
        stop_reason: reason,
        terminal_state_tick: end,
        last_progress_tick: last,
        survivors: alive.to_vec(),
        eliminated: (0..2).filter(|p| !alive.contains(p)).collect(),
        surviving_sides: alive
            .iter()
            .map(|p| SideId::Player { player_id: *p })
            .collect(),
        survival_transitions: vec![],
    }
}
fn id(f: &GoldenWorldFixture, p: u8, key: &str) -> EntityId {
    f.request
        .checkpoint
        .entities
        .iter()
        .find(|e| e.owner == p && e.type_key == key)
        .unwrap()
        .id
        .clone()
}
fn get<'a>(f: &'a mut GoldenWorldFixture, id: &EntityId) -> &'a mut EntityState {
    f.request
        .checkpoint
        .entities
        .iter_mut()
        .find(|e| &e.id == id)
        .unwrap()
}
fn add_turn(
    f: &mut GoldenWorldFixture,
    round: u32,
    tick: u32,
    commands: Vec<Command>,
) -> Vec<CommandId> {
    let ids: Vec<_> = (0..commands.len())
        .map(|i| command_id(round, 0, i as u32))
        .collect();
    let committed = commands
        .into_iter()
        .zip(&ids)
        .map(|(command, id)| CommittedCommand {
            id: id.clone(),
            command,
            future_orders: FutureOrderPolicy::Keep,
        })
        .collect();
    f.request.events.push(AcceptedTurn {
        player: 0,
        round,
        tick,
        commands: committed,
        duration_ms: 1000.try_into().unwrap(),
    });
    f.request.events.push(AcceptedTurn {
        player: 1,
        round,
        tick,
        commands: vec![],
        duration_ms: 1000.try_into().unwrap(),
    });
    f.request
        .precedence
        .push(round_precedence(round, 2).unwrap());
    ids
}
fn entity_expect(id: &EntityId, action: Option<Order>, position: Option<Tile>) -> ExpectedEntity {
    ExpectedEntity {
        entity_id: id.clone(),
        present: true,
        tile: position,
        hp: None,
        paid_matter: None,
        action,
    }
}
fn state(tick: u32, entities: Vec<ExpectedEntity>, banks: Vec<ExpectedBank>) -> TickExpectation {
    TickExpectation {
        state_tick: tick,
        entities,
        banks,
        groups: vec![],
    }
}
fn write(path: &str, value: &impl serde::Serialize) -> Result<()> {
    let path = Path::new(path);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|e| e.to_string())
}
pub fn generate() -> Result<()> {
    let mut worlds = vec![];
    let mut quiet = base(
        "quiet",
        "No commands or weapons: S[3] is the first inactive endpoint, not a missing range.",
        12,
    );
    quiet.expected.states.push(state(
        3,
        vec![],
        vec![
            ExpectedBank {
                player_id: 0,
                bank: 0.,
                mined: 0.,
            },
            ExpectedBank {
                player_id: 1,
                bank: 0.,
                mined: 0.,
            },
        ],
    ));
    let mut final_quiet = quiet.request.checkpoint.clone();
    final_quiet.tick = 3;
    quiet.expected.final_hash = Some(world_hash(&final_quiet)?);
    worlds.push(quiet);

    let mut mining = base(
        "mining-half",
        "Commands at t=0 first affect S[1]. Four equal mining opportunities transfer 8 matter from the miner and 4 from the constructor.",
        4,
    );
    let miner = id(&mining, 0, "miner");
    let constructor = id(&mining, 0, "constructor");
    mining.request.checkpoint.ore[9] = 20.;
    mining.request.checkpoint.ore[17] = 20.;
    let order = Order::Mine {
        area: Rect {
            min: tile(1, 1),
            max: tile(1, 2),
        },
    };
    let mut recipients = vec![miner.clone(), constructor.clone()];
    recipients.sort();
    let commands = add_turn(
        &mut mining,
        1,
        0,
        vec![Command::AssignOrder {
            entities: recipients.clone(),
            order: order.clone(),
        }],
    );
    mining.expected.command_outcomes.push(CommandOutcome {
        command_id: commands[0].clone(),
        applied_entities: recipients,
        skipped: vec![],
    });
    mining.expected.states = vec![
        state(
            1,
            vec![
                entity_expect(&miner, Some(order.clone()), Some(tile(1, 1))),
                entity_expect(&constructor, Some(order), Some(tile(1, 2))),
            ],
            vec![ExpectedBank {
                player_id: 0,
                bank: 3.,
                mined: 3.,
            }],
        ),
        state(
            4,
            vec![],
            vec![ExpectedBank {
                player_id: 0,
                bank: 12.,
                mined: 12.,
            }],
        ),
    ];
    mining.expected.outcome = result(4, 3, &[0, 1], StopReason::AbsoluteHorizon);
    worlds.push(mining);

    let mut future = base(
        "future-input",
        "Inactivity at S[3] must defer to an effective mining command at tick 5; ticks 5,6,7 transfer six matter.",
        8,
    );
    let miner = id(&future, 0, "miner");
    future.request.checkpoint.ore[9] = 20.;
    let order = Order::Mine {
        area: Rect {
            min: tile(1, 1),
            max: tile(1, 1),
        },
    };
    let commands = add_turn(
        &mut future,
        1,
        5,
        vec![Command::AssignOrder {
            entities: vec![miner.clone()],
            order: order.clone(),
        }],
    );
    future.expected.command_outcomes.push(CommandOutcome {
        command_id: commands[0].clone(),
        applied_entities: vec![miner.clone()],
        skipped: vec![],
    });
    future.expected.states = vec![
        state(
            5,
            vec![entity_expect(&miner, Some(Order::Idle {}), None)],
            vec![ExpectedBank {
                player_id: 0,
                bank: 0.,
                mined: 0.,
            }],
        ),
        state(
            6,
            vec![entity_expect(&miner, Some(order), None)],
            vec![ExpectedBank {
                player_id: 0,
                bank: 2.,
                mined: 2.,
            }],
        ),
        state(
            8,
            vec![],
            vec![ExpectedBank {
                player_id: 0,
                bank: 6.,
                mined: 6.,
            }],
        ),
    ];
    future.expected.outcome = result(8, 7, &[0, 1], StopReason::AbsoluteHorizon);
    worlds.push(future);

    let mut draw = base(
        "mutual-elimination",
        "Adjacent one-HP turrets fire simultaneously on tick 0. Both sides are eliminated with only idle constructors and no future commands, so the run is decided at S[1].",
        12,
    );
    let a = id(&draw, 0, "turret");
    let b = id(&draw, 1, "turret");
    draw.request
        .content
        .types
        .iter_mut()
        .find(|t| t.key == "turret")
        .unwrap()
        .weapon = Some(Weapon {
        range: 1.,
        damage: 1.,
        cooldown: 1,
        indirect: false,
        visual_style: VisualStyle::Direct,
    });
    for (id, position) in [(&a, tile(3, 3)), (&b, tile(4, 3))] {
        let e = get(&mut draw, id);
        e.hp = 1.;
        e.tile = position;
        e.action = Order::Idle {};
    }
    draw.expected.outcome = result(1, 0, &[], StopReason::Elimination);
    draw.expected.outcome.survival_transitions = (0..2)
        .map(|player_id| SurvivalTransition {
            player_id,
            resolved_tick: 0,
            status: SurvivalStatus::Eliminated,
            reasons: vec![Reason::NoActiveBuilding],
        })
        .collect();
    draw.expected.states.push(state(
        1,
        vec![
            ExpectedEntity {
                present: false,
                ..entity_expect(&a, None, None)
            },
            ExpectedEntity {
                present: false,
                ..entity_expect(&b, None, None)
            },
            entity_expect(&id(&draw, 0, "constructor"), Some(Order::Idle {}), None),
            entity_expect(&id(&draw, 1, "constructor"), Some(Order::Idle {}), None),
        ],
        vec![],
    ));
    worlds.push(draw);

    let mut recovery = base(
        "factory-recovery",
        "A previously eliminated owner keeps constructing. Four remaining matter completes a factory at tick 0, restoring the owner in S[1]. Both players alive at the endpoint is a non-scoring stalemate.",
        12,
    );
    let old_turret = id(&recovery, 0, "turret");
    recovery
        .request
        .checkpoint
        .entities
        .retain(|e| e.id != old_turret);
    recovery.request.checkpoint.players[0].currently_eliminated = true;
    recovery.request.checkpoint.players[0].elimination_reasons = vec![Reason::NoActiveBuilding];
    recovery.request.checkpoint.players[0].bank = 4.;
    recovery.request.checkpoint.players[0].counters.total_spend = 4.;
    recovery.request.checkpoint.players[0]
        .counters
        .structure_spend = 4.;
    let factory_type = recovery
        .request
        .content
        .types
        .iter_mut()
        .find(|t| t.key == "factory")
        .unwrap();
    factory_type.matter_cost = 8.;
    factory_type.max_hp = 16.;
    let bp = blueprint(&command_id(1, 0, 88), 0)?;
    let factory_id = structure(&bp);
    let template_id = id(&recovery, 0, "constructor");
    let mut factory = get(&mut recovery, &template_id).clone();
    // Template is an existing owned entity; all authoritative fields below are explicit.
    factory.id = factory_id.clone();
    factory.type_key = "factory".into();
    factory.tile = tile(2, 2);
    factory.hp = 8.;
    factory.paid_matter = 4.;
    factory.lifecycle = Lifecycle::Site;
    factory.blueprint_id = Some(bp.clone());
    factory.action = Order::Idle {};
    factory.production = Some(Production {
        pending_items: vec![],
        active_item: None,
        loop_enabled: false,
        stored_order: Order::Idle {},
        output_tile: tile(3, 2),
        occurrence_counters: Default::default(),
        spawn_group: None,
        output_direction: CardinalDirection::E,
    });
    recovery.request.checkpoint.entities.push(factory);
    recovery.request.checkpoint.blueprints.push(Blueprint {
        id: bp,
        owner: 0,
        type_key: "factory".into(),
        tile: tile(2, 2),
        priority: Priority::Medium,
        source_command_id: command_id(1, 0, 88),
        precedence: EventKey {
            tick: 0,
            round: 0,
            player_rank: 0,
            command_index: 0,
        },
        site_id: Some(factory_id.clone()),
        output_direction: Some(CardinalDirection::E),
    });
    let constructor = id(&recovery, 0, "constructor");
    get(&mut recovery, &constructor).action = Order::Construct {
        area: Rect {
            min: tile(2, 2),
            max: tile(2, 2),
        },
    };
    recovery.expected.states.push(state(
        1,
        vec![ExpectedEntity {
            hp: Some(16.),
            paid_matter: Some(8.),
            ..entity_expect(&factory_id, None, Some(tile(2, 2)))
        }],
        vec![ExpectedBank {
            player_id: 0,
            bank: 0.,
            mined: 0.,
        }],
    ));
    recovery.expected.outcome = result(4, 0, &[0, 1], StopReason::Inactivity);
    recovery.expected.outcome.survival_transitions = vec![SurvivalTransition {
        player_id: 0,
        resolved_tick: 0,
        status: SurvivalStatus::Alive,
        reasons: vec![],
    }];
    worlds.push(recovery);

    let mut dormant = base(
        "dormant-cause",
        "A historical assignment to an absent causal entity skips it; it never redirects an existing miner or constructor.",
        12,
    );
    let absent = genesis(0, 99)?;
    let commands = add_turn(
        &mut dormant,
        1,
        0,
        vec![Command::AssignOrder {
            entities: vec![absent.clone()],
            order: Order::AttackMove {
                destination: tile(4, 4),
            },
        }],
    );
    dormant.expected.command_outcomes.push(CommandOutcome {
        command_id: commands[0].clone(),
        applied_entities: vec![],
        skipped: vec![SkippedTarget {
            entity_id: Some(absent.clone()),
            reason: SkipReason::Absent,
        }],
    });
    dormant.expected.states.push(state(
        1,
        vec![
            ExpectedEntity {
                present: false,
                ..entity_expect(&absent, None, None)
            },
            entity_expect(
                &id(&dormant, 0, "miner"),
                Some(Order::Idle {}),
                Some(tile(1, 1)),
            ),
        ],
        vec![],
    ));
    worlds.push(dormant);

    let mut inheritance = base(
        "group-birth",
        "A group assignment at t=0 is overridden individually at t=1. Blocked factory output opens at t=2; the newborn inherits the saved group order while the existing constructor remains idle. Newborns cannot move until tick 3.",
        3,
    );
    let constructor = id(&inheritance, 0, "constructor");
    let blocker = id(&inheritance, 0, "miner");
    get(&mut inheritance, &blocker).tile = tile(3, 4);
    let group = ControlGroupId {
        owner: 0,
        slot: 0.try_into().unwrap(),
    };
    inheritance
        .request
        .checkpoint
        .control_groups
        .iter_mut()
        .find(|g| g.id == group)
        .unwrap()
        .members = vec![constructor.clone()];
    let factory_id = genesis(0, 3)?;
    let item = queue_item(&command_id(1, 0, 99), 0, 0)?;
    let newborn = production(&item, 0);
    let mut factory = get(&mut inheritance, &constructor).clone();
    factory.id = factory_id.clone();
    factory.type_key = "factory".into();
    factory.tile = tile(2, 4);
    factory.hp = 400.;
    factory.action = Order::Idle {};
    factory.production = Some(Production {
        pending_items: vec![],
        active_item: Some(ActiveItem {
            item_id: item.clone(),
            occurrence: 0,
            type_key: "grunt".into(),
            paid_matter: 15.,
            awaiting_output: true,
        }),
        loop_enabled: false,
        stored_order: Order::Idle {},
        output_tile: tile(3, 4),
        occurrence_counters: vec![OccurrenceCounter {
            item_id: item,
            next_occurrence: 0,
        }],
        spawn_group: Some(group.clone()),
        output_direction: CardinalDirection::E,
    });
    inheritance.request.checkpoint.players[0]
        .counters
        .total_spend = 15.;
    inheritance.request.checkpoint.players[0]
        .counters
        .unit_spend = 15.;
    inheritance.request.checkpoint.entities.push(factory);
    let saved = Order::AttackMove {
        destination: tile(1, 2),
    };
    let group_commands = add_turn(
        &mut inheritance,
        1,
        0,
        vec![Command::AssignGroupOrder {
            group: group.clone(),
            order: saved.clone(),
        }],
    );
    let individual = add_turn(
        &mut inheritance,
        2,
        1,
        vec![Command::AssignOrder {
            entities: vec![constructor.clone()],
            order: Order::Idle {},
        }],
    );
    let moved = add_turn(
        &mut inheritance,
        3,
        2,
        vec![Command::AssignOrder {
            entities: vec![blocker.clone()],
            order: Order::AttackMove {
                destination: tile(4, 4),
            },
        }],
    );
    inheritance.request.revision = 3;
    inheritance.expected.command_outcomes = vec![
        CommandOutcome {
            command_id: group_commands[0].clone(),
            applied_entities: vec![constructor.clone()],
            skipped: vec![],
        },
        CommandOutcome {
            command_id: individual[0].clone(),
            applied_entities: vec![constructor.clone()],
            skipped: vec![],
        },
        CommandOutcome {
            command_id: moved[0].clone(),
            applied_entities: vec![blocker.clone()],
            skipped: vec![],
        },
    ];
    inheritance.expected.states.push(state(
        2,
        vec![
            entity_expect(&constructor, Some(Order::Idle {}), None),
            ExpectedEntity {
                present: false,
                ..entity_expect(&newborn, None, None)
            },
        ],
        vec![],
    ));
    let mut members = vec![constructor.clone(), newborn.clone()];
    members.sort();
    let mut expected = state(
        3,
        vec![
            entity_expect(&constructor, Some(Order::Idle {}), Some(tile(1, 2))),
            entity_expect(&newborn, Some(saved.clone()), Some(tile(3, 4))),
            entity_expect(&blocker, None, Some(tile(4, 4))),
        ],
        vec![],
    );
    expected.groups.push(ControlGroupState {
        order_locks: vec![],
        id: group.clone(),
        members,
        latest_order: Some(SavedOrder {
            source_command_id: group_commands[0].clone(),
            tick: 0,
            order: saved,
        }),
    });
    inheritance.expected.states.push(expected);
    inheritance.expected.outcome = result(3, 2, &[0, 1], StopReason::AbsoluteHorizon);
    worlds.push(inheritance);

    let mut locked_group = base(
        "partial-group-lock",
        "A successful t=0 individual assignment locks only the constructor delivery of a historical t=5 group event. The miner receives the retained order and the group still saves it for future births.",
        6,
    );
    let miner = id(&locked_group, 0, "miner");
    let constructor = id(&locked_group, 0, "constructor");
    let mut members = vec![miner.clone(), constructor.clone()];
    members.sort();
    locked_group
        .request
        .checkpoint
        .control_groups
        .iter_mut()
        .find(|g| g.id == group)
        .unwrap()
        .members = members.clone();
    let order = Order::AttackMove {
        destination: tile(1, 1),
    };
    let historical = add_turn(
        &mut locked_group,
        1,
        5,
        vec![Command::AssignGroupOrder {
            group: group.clone(),
            order: order.clone(),
        }],
    );
    let replacement = add_turn(
        &mut locked_group,
        2,
        0,
        vec![Command::AssignOrder {
            entities: vec![constructor.clone()],
            order: Order::Idle {},
        }],
    );
    let committed = &mut locked_group
        .request
        .events
        .iter_mut()
        .find(|t| t.round == 2 && t.player == 0)
        .unwrap()
        .commands[0];
    committed.future_orders = FutureOrderPolicy::DropAll;
    locked_group.request.revision = 2;
    locked_group.expected.command_outcomes = vec![
        CommandOutcome {
            command_id: replacement[0].clone(),
            applied_entities: vec![constructor.clone()],
            skipped: vec![],
        },
        CommandOutcome {
            command_id: historical[0].clone(),
            applied_entities: vec![miner.clone()],
            skipped: vec![SkippedTarget {
                entity_id: Some(constructor.clone()),
                reason: SkipReason::LockedByLaterRound,
            }],
        },
    ];
    let mut expected = state(
        6,
        vec![
            entity_expect(&miner, Some(order.clone()), Some(tile(1, 1))),
            entity_expect(&constructor, Some(Order::Idle {}), Some(tile(1, 2))),
        ],
        vec![],
    );
    expected.groups.push(ControlGroupState {
        order_locks: vec![],
        id: group,
        members,
        latest_order: Some(SavedOrder {
            source_command_id: historical[0].clone(),
            tick: 5,
            order,
        }),
    });
    locked_group.expected.states.push(expected);
    locked_group.expected.outcome = result(6, 0, &[0, 1], StopReason::AbsoluteHorizon);
    worlds.push(locked_group);

    let mut manifest: Vec<serde_json::Value> = serde_json::from_str(
        &fs::read_to_string("fixtures/manifest.json").map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    manifest.retain(|entry| {
        !entry["file"]
            .as_str()
            .unwrap()
            .starts_with("fixtures/worlds/")
            && entry["file"] != "fixtures/protocol/exact-state.json"
            && entry["file"] != "fixtures/protocol/worker-complete.json"
            && entry["file"] != "fixtures/protocol/get-round.json"
            && entry["file"] != "fixtures/protocol/round-result.json"
    });
    for f in &mut worlds {
        f.request.checkpoint = canonical_world(&f.request.checkpoint)?;
        f.request.content = atemporal_content::normalize_content(f.request.content.clone())?;
        f.request.fingerprint.config_hash = canonical_hash(&f.request.config)?;
        f.request.fingerprint.content_hash = atemporal_content::content_hash(&f.request.content)?;
        f.initial_hash = world_hash(&f.request.checkpoint)?;
        let file = format!("fixtures/worlds/{}.json", f.name);
        write(&file, f)?;
        manifest.push(json!({"file":file,"type":"GoldenWorldFixture"}));
    }
    let exact = ServerEnvelope {
        schema_version: Version::default(),
        server_instance_id: "fixture-instance".into(),
        message: ServerMessage::ExactState {
            revision: 0,
            tick: 0,
            snapshot: worlds[0].request.checkpoint.clone(),
        },
    };
    write("fixtures/protocol/exact-state.json", &exact)?;
    manifest.push(json!({"file":"fixtures/protocol/exact-state.json","type":"ServerEnvelope"}));
    let complete = WorkerEnvelope {
        schema_version: Version::default(),
        message: WorkerMessage::Complete {
            job_id: "quiet".into(),
            revision: 1,
            outcome: worlds[0].expected.outcome.clone(),
            final_hash: worlds[0].expected.final_hash.clone().unwrap(),
            sim_duration_ms: 0.try_into().unwrap(),
            command_outcomes: vec![],
        },
    };
    write("fixtures/protocol/worker-complete.json", &complete)?;
    manifest.push(json!({"file":"fixtures/protocol/worker-complete.json","type":"WorkerEnvelope"}));
    let request = ClientEnvelope {
        schema_version: Version::default(),
        message: ClientMessage::GetRound { revision: 2 },
    };
    write("fixtures/protocol/get-round.json", &request)?;
    manifest.push(json!({"file":"fixtures/protocol/get-round.json","type":"ClientEnvelope"}));
    let round = ServerEnvelope {
        schema_version: Version::default(),
        server_instance_id: "fixture-instance".into(),
        message: ServerMessage::RoundResult {
            revision: 2,
            round: 2,
            parent_revision: Some(1),
            outcome: worlds[0].expected.outcome.clone(),
            timeline_index: vec![],
            score: None,
            timed: None,
            time_totals: vec![
                PlayerTime {
                    player_id: 0,
                    total_ms: 2000.try_into().unwrap(),
                },
                PlayerTime {
                    player_id: 1,
                    total_ms: 3000.try_into().unwrap(),
                },
            ],
            sim_duration_ms: 10.try_into().unwrap(),
            command_outcomes: vec![CommandOutcome {
                command_id: CommandId {
                    round: 1,
                    player: 0,
                    index: 0,
                },
                applied_entities: vec![],
                skipped: vec![SkippedTarget {
                    entity_id: None,
                    reason: SkipReason::LockedByLaterRound,
                }],
            }],
        },
    };
    write("fixtures/protocol/round-result.json", &round)?;
    manifest.push(json!({"file":"fixtures/protocol/round-result.json","type":"ServerEnvelope"}));
    manifest.sort_by(|a, b| a["file"].as_str().cmp(&b["file"].as_str()));
    write("fixtures/manifest.json", &manifest)?;
    println!(
        "Wrote {} authored tiny-world fixtures (no simulation executed).",
        worlds.len()
    );
    Ok(())
}
