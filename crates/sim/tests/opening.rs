//! The Gate 2 opening on the generated map: mine, build a factory, produce a grunt, attack.
use atemporal_sim::*;
use std::sync::atomic::AtomicBool;

fn setup() -> (MatchConfig, Content) {
    let content = load_content(include_str!("../../../config/content.yaml")).unwrap();
    let mut config = atemporal_content::load_setup(include_str!("../../../config/game.yaml"))
        .unwrap()
        .match_defaults;
    config.max_tick = 3000;
    (config, content)
}

fn turn(round: u32, player: PlayerId, tick: Tick, commands: Vec<Command>) -> AcceptedTurn {
    AcceptedTurn {
        player,
        round,
        tick,
        commands: commands
            .into_iter()
            .enumerate()
            .map(|(index, command)| CommittedCommand {
                id: identity::command_id(round, player, index as u32),
                command,
                future_orders: FutureOrderPolicy::Keep,
            })
            .collect(),
        duration_ms: 1000.try_into().unwrap(),
    }
}

fn request(
    config: &MatchConfig,
    content: &Content,
    world: WorldState,
    events: Vec<AcceptedTurn>,
) -> SimRequest {
    let rounds: std::collections::BTreeSet<u32> = events.iter().map(|t| t.round).collect();
    SimRequest {
        schema_version: Version::default(),
        job_id: "opening".into(),
        revision: rounds.len() as u32,
        fingerprint: Fingerprint {
            schema_version: Version::default(),
            sim_build: "test".into(),
            target: "test".into(),
            config_hash: String::new(),
            content_hash: String::new(),
        },
        config: config.clone(),
        content: content.clone(),
        checkpoint: world,
        events,
        precedence: rounds
            .into_iter()
            .map(|r| identity::round_precedence(r, 2).unwrap())
            .collect(),
        end_tick_exclusive: config.max_tick,
        minimum_end_tick: 0,
        entity_dictionary: vec![],
    }
}

fn run_collect(request: &SimRequest) -> (RunResult, Vec<WorldEvent>, Vec<WorldState>) {
    let mut events = vec![];
    let mut checkpoints = vec![];
    let mut sink = |o: Output| match o {
        Output::Events(e) => events.extend(e),
        Output::Checkpoint(c) => checkpoints.push(c),
        _ => {}
    };
    let result = run(request, &AtomicBool::new(false), &mut sink).unwrap();
    (result, events, checkpoints)
}

/// Three-by-three area around the ore tile nearest `from`.
fn mine_area(world: &WorldState, from: Tile) -> Rect {
    let n = usize::from(world.terrain.width);
    let (i, _) = world
        .ore
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0.0)
        .min_by_key(|(i, _)| {
            let (x, y) = ((i % n) as i32, (i / n) as i32);
            (x - i32::from(from.x)).pow(2) + (y - i32::from(from.y)).pow(2)
        })
        .unwrap();
    let (x, y) = ((i % n) as u16, (i / n) as u16);
    Rect {
        min: Tile { x: x - 1, y: y - 1 },
        max: Tile { x: x + 1, y: y + 1 },
    }
}

fn ore_in(world: &WorldState, area: &Rect) -> f64 {
    let n = usize::from(world.terrain.width);
    world
        .ore
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let (x, y) = ((i % n) as u16, (i / n) as u16);
            x >= area.min.x && x <= area.max.x && y >= area.min.y && y <= area.max.y
        })
        .map(|(_, v)| v)
        .sum()
}

#[test]
fn miner_factory_grunt_attack_opening() {
    let (config, content) = setup();
    let world = map::generate(&config, &content).unwrap();
    let miner = identity::genesis(0, 0).unwrap();
    let constructor = identity::genesis(0, 1).unwrap();
    let enemy_turret = identity::genesis(1, 2).unwrap();
    let enemy_miner = identity::genesis(1, 0).unwrap();
    let enemy_miner_tile = world
        .entities
        .iter()
        .find(|e| e.id == enemy_miner)
        .unwrap()
        .tile;
    let factory_tile = Tile { x: 8, y: 4 };
    let round1 = turn(
        1,
        0,
        0,
        vec![
            Command::AssignOrder {
                entities: vec![miner.clone()],
                order: Order::Mine {
                    area: mine_area(&world, Tile { x: 4, y: 7 }),
                },
            },
            Command::PlaceBlueprints {
                type_key: "factory".into(),
                tiles: vec![factory_tile],
                priority: Priority::High,
                output_directions: Some(vec![CardinalDirection::E]),
            },
            Command::AssignOrder {
                entities: vec![constructor.clone()],
                order: Order::Construct {
                    area: Rect {
                        min: factory_tile,
                        max: factory_tile,
                    },
                },
            },
        ],
    );
    let factory = identity::blueprint(&identity::command_id(1, 0, 1), 0).unwrap();
    let first = request(
        &config,
        &content,
        world.clone(),
        vec![round1.clone(), turn(1, 1, 0, vec![])],
    );
    let (result, _, checkpoints) = run_collect(&first);
    assert_eq!(result.outcome.kind, OutcomeKind::Stalemate);
    // Find the completion tick of the factory by exact reconstruction (non-sample ticks allowed).
    let mut completed_at = None;
    for tick in 1..400 {
        let state = reconstruct(&first, tick).unwrap();
        if state
            .entities
            .iter()
            .any(|e| e.id == factory && e.lifecycle == Lifecycle::Complete)
        {
            completed_at = Some(tick);
            break;
        }
    }
    let completed_at = completed_at.expect("factory completes within 400 ticks");
    let at_completion = reconstruct(&first, completed_at).unwrap();
    assert!(at_completion.players[0].counters.mined > 0.0, "miner mined");
    assert_eq!(at_completion.players[0].counters.structure_spend, 200.0);

    // Round 2 rewrites an earlier tick than the last revision end: queue a grunt with a stored attack-move.
    let round2_tick = (completed_at + 3).max(150);
    let round2 = turn(
        2,
        0,
        round2_tick,
        vec![
            Command::SetStoredOrder {
                factories: vec![factory.clone()],
                order: Order::AttackMove {
                    destination: enemy_miner_tile,
                },
            },
            Command::EditProduction {
                factories: vec![factory.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
        ],
    );
    let checkpoint = checkpoints
        .iter()
        .filter(|c| c.tick <= round2_tick)
        .max_by_key(|c| c.tick)
        .unwrap()
        .clone();
    assert!(
        checkpoint.tick > 0,
        "resume from a later checkpoint, not genesis"
    );
    let events = vec![
        round1,
        turn(1, 1, 0, vec![]),
        round2.clone(),
        turn(2, 1, round2_tick, vec![]),
    ];
    let partial = request(&config, &content, checkpoint, events.clone());
    let (partial_result, partial_events, _) = run_collect(&partial);
    let full = request(&config, &content, world, events);
    let (full_result, _, _) = run_collect(&full);
    assert_eq!(
        partial_result.final_hash, full_result.final_hash,
        "checkpoint rerun equals full replay"
    );
    assert_eq!(partial_result.outcome, full_result.outcome);

    let grunt = identity::production(
        &identity::queue_item(&identity::command_id(2, 0, 1), 0, 0).unwrap(),
        0,
    );
    // Which worker is nearest on arrival depends on the cave layout around the enemy base.
    let enemy_constructor = identity::genesis(1, 1).unwrap();
    let attacks: Vec<(Tick, EntityId)> = partial_events
        .iter()
        .filter_map(|e| match &e.event {
            PresentationEvent::Attack {
                attacker_id,
                target_id,
                ..
            } if *attacker_id == grunt
                && (*target_id == enemy_miner || *target_id == enemy_constructor) =>
            {
                Some((e.tick, target_id.clone()))
            }
            _ => None,
        })
        .collect();
    assert!(
        !attacks.is_empty(),
        "the produced grunt attacked an enemy worker"
    );
    let (first_attack, target) = attacks[0].clone();
    let before = reconstruct(&full, first_attack).unwrap();
    let after = reconstruct(&full, first_attack + 1).unwrap();
    let hp = |s: &WorldState| s.entities.iter().find(|e| e.id == target).unwrap().hp;
    assert!(hp(&after) < hp(&before), "worker took damage");
    assert!(full_result.outcome.terminal_state_tick > first_attack);
    let turret_damage = partial_events
        .iter()
        .filter(|e| matches!(&e.event, PresentationEvent::Attack { attacker_id, .. } if *attacker_id == enemy_turret))
        .count();
    assert!(
        turret_damage > 0,
        "the turret automatically defended its miner"
    );
    let destroyed = partial_events
        .iter()
        .any(|e| matches!(&e.event, PresentationEvent::Destroyed { entity_id, .. } if *entity_id == grunt));
    assert!(destroyed, "a lone grunt under turret fire is destroyed");
}

#[test]
fn peaceful_mining_reaches_inactivity_after_ore_exhaustion() {
    let (mut config, content) = setup();
    config.max_tick = 20000;
    config.ore_matter_per_start = 600.0;
    let world = map::generate(&config, &content).unwrap();
    let miner = identity::genesis(0, 0).unwrap();
    let area = mine_area(&world, Tile { x: 4, y: 7 });
    let expected = ore_in(&world, &area);
    let events = vec![
        turn(
            1,
            0,
            0,
            vec![Command::AssignOrder {
                entities: vec![miner],
                order: Order::Mine { area },
            }],
        ),
        turn(1, 1, 0, vec![]),
    ];
    let request = request(&config, &content, world, events);
    let start = std::time::Instant::now();
    let (result, _, _) = run_collect(&request);
    eprintln!(
        "peaceful opening: terminal S[{}], last progress {}, {:?} wall",
        result.outcome.terminal_state_tick,
        result.outcome.last_progress_tick,
        start.elapsed()
    );
    assert_eq!(result.outcome.stop_reason, StopReason::Inactivity);
    assert!(
        (result.final_state.players[0].counters.mined - expected).abs() < 1e-6,
        "mined the whole area: {expected}"
    );
    assert_eq!(
        result.outcome.terminal_state_tick,
        result.outcome.last_progress_tick + 1 + config.stall_ticks
    );
}
