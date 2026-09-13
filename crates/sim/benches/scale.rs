//! Release benchmarks: march (caves, no weapons) and battle (open field) at 100/500/2,000
//! entities with one and four intent threads, plus the 20,000-tick default-cap opening with
//! export enabled. Run with `cargo bench -p atemporal-sim`.
use atemporal_sim::*;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn setup() -> (MatchConfig, Content) {
    let content = load_content(include_str!("../../../config/content.yaml")).unwrap();
    let config = atemporal_content::load_setup(include_str!("../../../config/game.yaml"))
        .unwrap()
        .match_defaults;
    (config, content)
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
        job_id: "bench".into(),
        revision: rounds.len() as u32,
        fingerprint: Fingerprint {
            schema_version: Version::default(),
            sim_build: "bench".into(),
            target: "bench".into(),
            config_hash: String::new(),
            content_hash: String::new(),
        },
        config: config.clone(),
        content: content.clone(),
        checkpoint: world,
        events,
        precedence: rounds
            .into_iter()
            .map(|r| identity::round_precedence(r, config.player_count).unwrap())
            .collect(),
        end_tick_exclusive: config.max_tick,
        minimum_end_tick: 0,
        entity_dictionary: vec![],
    }
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

/// Fill `count` free floor cells nearest `from` (four-neighbor flood order) with units.
fn populate(
    world: &mut WorldState,
    player: PlayerId,
    from: Tile,
    count: usize,
    keys: &[&str],
    content: &Content,
    order: Order,
) {
    let w = usize::from(world.terrain.width);
    let idx = |t: Tile| usize::from(t.y) * w + usize::from(t.x);
    let mut occupied = vec![false; w * w];
    for e in &world.entities {
        occupied[idx(e.tile)] = true;
    }
    let mut seen = vec![false; w * w];
    let mut queue = VecDeque::from([from]);
    seen[idx(from)] = true;
    let mut placed = 0;
    while let Some(t) = queue.pop_front() {
        if placed == count {
            break;
        }
        if !occupied[idx(t)] {
            let key = keys[placed % keys.len()];
            let def = content.types.iter().find(|d| d.key == key).unwrap();
            world.entities.push(EntityState {
                id: identity::genesis(player, 1000 + placed as u32).unwrap(),
                owner: player,
                type_key: key.into(),
                tile: t,
                last_move_direction: Direction::N,
                hp: def.max_hp,
                paid_matter: def.matter_cost,
                lifecycle: Lifecycle::Complete,
                blueprint_id: None,
                action: order.clone(),
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
            });
            occupied[idx(t)] = true;
            placed += 1;
        }
        for (dx, dy) in [(0i32, -1i32), (1, 0), (0, 1), (-1, 0)] {
            let (x, y) = (i32::from(t.x) + dx, i32::from(t.y) + dy);
            if x < 1 || y < 1 || x >= w as i32 - 1 || y >= w as i32 - 1 {
                continue;
            }
            let n = Tile {
                x: x as u16,
                y: y as u16,
            };
            if world.terrain.cells[idx(n)] == TerrainCell::Floor && !seen[idx(n)] {
                seen[idx(n)] = true;
                queue.push_back(n);
            }
        }
    }
    assert_eq!(placed, count, "not enough floor to place {count} units");
}

struct Measured {
    entities: usize,
    ticks: Tick,
    wall_ms: f64,
    stop: StopReason,
    peak: usize,
}

/// Best of three repetitions, since laptop clocks and background load add noise.
fn measure(request: &SimRequest) -> Measured {
    (0..3)
        .map(|_| measure_once(request))
        .min_by(|a, b| a.wall_ms.partial_cmp(&b.wall_ms).unwrap())
        .unwrap()
}

fn measure_once(request: &SimRequest) -> Measured {
    let mut peak = 0;
    let mut sink = |o: Output| {
        if let Output::Sample(s) = o {
            peak = peak.max(s.entities.len());
        }
    };
    let start = Instant::now();
    let result = run(request, &AtomicBool::new(false), &mut sink).unwrap();
    Measured {
        entities: request.checkpoint.entities.len(),
        ticks: result.outcome.terminal_state_tick - request.checkpoint.tick,
        wall_ms: start.elapsed().as_secs_f64() * 1000.0,
        stop: result.outcome.stop_reason,
        peak,
    }
}

fn report(name: &str, threads: u16, m: &Measured) {
    println!(
        "| {name} | {threads} | {} | {} | {} | {:.0} | {:.3} | {:.0} | {:?} |",
        m.entities,
        m.peak,
        m.ticks,
        m.wall_ms,
        m.wall_ms / f64::from(m.ticks.max(1)),
        f64::from(m.ticks) / (m.wall_ms / 1000.0),
        m.stop
    );
}

/// Caves, no weapons: every unit crosses the map to the enemy start and settles.
fn march(count: usize, threads: u16) -> SimRequest {
    let (mut config, mut content) = setup();
    for t in &mut content.types {
        t.weapon = None;
    }
    config.map_size = 96;
    config.max_tick = 3000;
    config.stall_ticks = 100;
    config.simulation_threads = threads;
    config.snapshot_interval = 25;
    let mut world = map::generate(&config, &content).unwrap();
    let a = Tile { x: 4, y: 4 };
    let b = Tile { x: 91, y: 91 };
    populate(
        &mut world,
        0,
        a,
        count / 2,
        &["grunt", "miner", "tank"],
        &content,
        Order::AttackMove { destination: b },
    );
    populate(
        &mut world,
        1,
        b,
        count - count / 2,
        &["grunt", "miner", "tank"],
        &content,
        Order::AttackMove { destination: a },
    );
    request(
        &config,
        &content,
        identity::canonical_world(&world).unwrap(),
        vec![],
    )
}

/// Open field: two armies charge each other's start; turrets and artillery add line-of-sight work.
fn battle(count: usize, threads: u16) -> SimRequest {
    let (mut config, content) = setup();
    config.map_size = 96;
    config.max_tick = 3000;
    config.stall_ticks = 100;
    config.simulation_threads = threads;
    config.snapshot_interval = 25;
    let n = 96usize;
    let mut cells = vec![TerrainCell::Floor; n * n];
    for i in 0..n {
        for j in [i, i * n, i * n + n - 1, (n - 1) * n + i] {
            cells[j] = TerrainCell::Wall;
        }
    }
    let mut world = map::generate(&config, &content).unwrap();
    world.terrain.cells = cells;
    world.entities.clear();
    let a = Tile { x: 20, y: 48 };
    let b = Tile { x: 75, y: 48 };
    let mix = [
        "grunt",
        "grunt",
        "grinder",
        "scout",
        "tank",
        "artillery",
        "turret",
    ];
    populate(
        &mut world,
        0,
        a,
        count / 2,
        &mix,
        &content,
        Order::AttackMove { destination: b },
    );
    populate(
        &mut world,
        1,
        b,
        count - count / 2,
        &mix,
        &content,
        Order::AttackMove { destination: a },
    );
    request(
        &config,
        &content,
        identity::canonical_world(&world).unwrap(),
        vec![],
    )
}

/// The real opening on the default map with looping production to the 20,000-tick cap.
fn cap_run(threads: u16) -> SimRequest {
    let (mut config, content) = setup();
    config.simulation_threads = threads;
    // Exercise the absolute cap even when the opening becomes decided sooner.
    config.stop_when_decided = false;
    config.stall_ticks = config.max_tick;
    // Enough ore for three miners' worth of income to outlast the 20,000-tick horizon.
    config.ore_matter_per_start = 80_000.0;
    let world = map::generate(&config, &content).unwrap();
    let mut events = vec![];
    for player in 0..2u8 {
        let miner = identity::genesis(player, 0).unwrap();
        let constructor = identity::genesis(player, 1).unwrap();
        let ore: Vec<Tile> = world
            .ore
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0.0)
            .map(|(i, _)| Tile {
                x: (i % 48) as u16,
                y: (i / 48) as u16,
            })
            .filter(|t| {
                let m = world.entities.iter().find(|e| e.id == miner).unwrap().tile;
                (f64::from(t.x) - f64::from(m.x)).hypot(f64::from(t.y) - f64::from(m.y)) < 12.0
            })
            .collect();
        let area = Rect {
            min: Tile {
                x: ore.iter().map(|t| t.x).min().unwrap(),
                y: ore.iter().map(|t| t.y).min().unwrap(),
            },
            max: Tile {
                x: ore.iter().map(|t| t.x).max().unwrap(),
                y: ore.iter().map(|t| t.y).max().unwrap(),
            },
        };
        let c = world
            .entities
            .iter()
            .find(|e| e.id == constructor)
            .unwrap()
            .tile;
        let site = if player == 0 {
            Tile {
                x: c.x + 2,
                y: c.y - 1,
            }
        } else {
            Tile {
                x: c.x - 2,
                y: c.y + 1,
            }
        };
        let direction = if player == 0 {
            CardinalDirection::E
        } else {
            CardinalDirection::W
        };
        let enemy_miner = world
            .entities
            .iter()
            .find(|e| e.owner != player && e.type_key == "miner")
            .unwrap()
            .tile;
        events.push(turn(
            1,
            player,
            0,
            vec![
                Command::AssignOrder {
                    entities: vec![miner],
                    order: Order::Mine { area: area.clone() },
                },
                Command::PlaceBlueprints {
                    type_key: "factory".into(),
                    tiles: vec![site],
                    priority: Priority::High,
                    output_directions: Some(vec![direction]),
                },
                Command::AssignOrder {
                    entities: vec![constructor.clone()],
                    order: Order::Construct {
                        area: Rect {
                            min: site,
                            max: site,
                        },
                    },
                },
            ],
        ));
        let factory = identity::blueprint(&identity::command_id(1, player, 1), 0).unwrap();
        events.push(turn(
            2,
            player,
            150,
            vec![
                Command::SetStoredOrder {
                    factories: vec![factory.clone()],
                    order: Order::AttackMove {
                        destination: enemy_miner,
                    },
                },
                Command::SetQueueLoop {
                    factories: vec![factory.clone()],
                    enabled: true,
                },
                Command::EditProduction {
                    factories: vec![factory.clone()],
                    edit: ProductionEdit::Append {
                        items: vec!["grunt".into()],
                    },
                },
                Command::AssignOrder {
                    entities: vec![constructor],
                    order: Order::Mine { area },
                },
            ],
        ));
    }
    request(&config, &content, world, events)
}

fn main() {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    println!(
        "machine: {cpu}, {} logical cores",
        std::thread::available_parallelism().map_or(0, |n| n.get())
    );
    println!();
    println!(
        "| Scenario | Threads | Entities | Peak | Ticks | Wall ms | ms/tick | Ticks/s | Stop |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for count in [100usize, 500, 2000] {
        for threads in [1u16, 4] {
            report(
                &format!("march {count}"),
                threads,
                &measure(&march(count, threads)),
            );
            report(
                &format!("battle {count}"),
                threads,
                &measure(&battle(count, threads)),
            );
        }
    }
    println!();
    // Full cap with export enabled: measure sim wall time and serialized output sizes.
    for threads in [1u16, 4] {
        let request = cap_run(threads);
        let (mut samples, mut checkpoints, mut events, mut stats, mut timeline, mut dictionary) =
            (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
        let (mut sample_count, mut event_count, mut checkpoint_count, mut peak, mut births) =
            (0usize, 0usize, 0usize, 0usize, 0usize);
        let mut sink = |o: Output| match o {
            Output::Sample(s) => {
                peak = peak.max(s.entities.len());
                sample_count += 1;
                samples += serde_json::to_vec(&s).unwrap().len();
            }
            Output::Checkpoint(c) => {
                checkpoint_count += 1;
                checkpoints += serde_json::to_vec(&c).unwrap().len();
            }
            Output::Events(e) => {
                event_count += e.len();
                events += serde_json::to_vec(&e).unwrap().len();
            }
            Output::Stats(s) => stats += serde_json::to_vec(&s).unwrap().len(),
            Output::Timeline(t) => timeline += serde_json::to_vec(&t).unwrap().len(),
            Output::Dictionary(d) => {
                births += d.len();
                dictionary += serde_json::to_vec(&d).unwrap().len();
            }
            Output::Progress { .. } => {}
        };
        let start = Instant::now();
        let result = run(&request, &AtomicBool::new(false), &mut sink).unwrap();
        let wall = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "cap run ({threads} threads): S[{}] {:?} {:?}, {} entities born, peak {peak} alive, sim {:.0} ms ({:.3} ms/tick)",
            result.outcome.terminal_state_tick,
            result.outcome.stop_reason,
            result.outcome.kind,
            births,
            wall,
            wall / f64::from(result.outcome.terminal_state_tick)
        );
        println!(
            "  export JSON: samples {:.1} MB ({sample_count}), checkpoints {:.1} MB ({checkpoint_count}), events {:.2} MB ({event_count}), stats {:.2} MB, timeline {:.2} MB, dictionary {:.0} KB",
            samples as f64 / 1e6,
            checkpoints as f64 / 1e6,
            events as f64 / 1e6,
            stats as f64 / 1e6,
            timeline as f64 / 1e6,
            dictionary as f64 / 1e3
        );
    }
}
