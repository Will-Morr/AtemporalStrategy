//! Gate 3: identical hashes, outcomes and events with one or four intent threads and with
//! cold, warm and evicted field caches, on a battle that crosses the parallel threshold.
mod common;
use atemporal_sim::*;
use common::*;

fn battle(threads: u16) -> World {
    let mut rows: Vec<String> = (0..60)
        .map(|y| {
            (0..60)
                .map(|x| {
                    let blob = (x % 9 == 4 && y % 7 == 3) || (x % 11 == 7 && y % 5 == 2);
                    if blob && (26..34).contains(&x) {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect();
    rows[0] = "#".repeat(60);
    rows[59] = "#".repeat(60);
    let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut w = World::new(&refs);
    w.config.simulation_threads = threads;
    w.config.max_tick = 700;
    w.config.stall_ticks = 100;
    w.config.checkpoint_interval = 100;
    w.config.snapshot_interval = 25;
    w.bank(0, 3000.0);
    w.bank(1, 3000.0);
    let mut commands = vec![vec![], vec![]];
    for k in 0..3u16 {
        let f0 = w.spawn(0, "factory", 2, 4 + 12 * k);
        let f1 = w.spawn(1, "factory", 56, 6 + 12 * k);
        w.spawn(0, "turret", 6, 8 + 12 * k);
        w.spawn(1, "turret", 53, 10 + 12 * k);
        w.spawn(0, "tank", 5, 5 + 12 * k);
        w.spawn(1, "artillery", 54, 5 + 12 * k);
        for (player, f, target) in [
            (0u8, f0, tile(56, 6 + 12 * k)),
            (1, f1, tile(2, 4 + 12 * k)),
        ] {
            commands[usize::from(player)].extend([
                Command::SetStoredOrder {
                    factories: vec![f.clone()],
                    order: Order::AttackMove {
                        destination: target,
                    },
                },
                Command::SetQueueLoop {
                    factories: vec![f.clone()],
                    enabled: true,
                },
                Command::EditProduction {
                    factories: vec![f.clone()],
                    edit: ProductionEdit::Append {
                        items: vec![
                            "grunt".into(),
                            if k == 1 {
                                "scout".into()
                            } else {
                                "grinder".into()
                            },
                        ],
                    },
                },
            ]);
        }
    }
    // 540 marching grunts per side keep the armed population above the parallel threshold.
    for y in 3..57u16 {
        for x in 0..10u16 {
            w.spawn_with(0, "grunt", 8 + x, y, attack_move(56, 18));
            w.spawn_with(1, "grunt", 51 - x, y, attack_move(2, 16));
        }
    }
    let t0 = commands.remove(0);
    w.turn(1, 0, 0, t0);
    let t1 = commands.remove(0);
    w.turn(1, 1, 0, t1);
    w
}

/// One test shares the expensive runs: a serial replay, a pooled replay, three cold checkpoint
/// reruns and a stepwise replay with a capacity-1 evicting field cache. The 1,000-armed-entity
/// battle takes ~10 s in release and ~160 s unoptimized, so it runs only in release
/// (`cargo test -p atemporal-sim --release`); pass `--ignored` to force it in debug.
#[test]
#[cfg_attr(debug_assertions, ignore)]
fn threads_checkpoints_and_evicted_caches_replay_identically() {
    let serial = battle(1);
    let pooled = battle(4);
    let a = serial.run();
    let b = pooled.run();
    // The genesis population itself crosses the armed threshold (1,080 grunts plus turrets).
    let armed = serial
        .state
        .entities
        .iter()
        .filter(|e| serial.def(&e.type_key).weapon.is_some())
        .count();
    assert!(
        armed >= 1000,
        "battle must cross the parallel threshold: {armed} armed"
    );
    assert_eq!(a.result.final_hash, b.result.final_hash);
    assert_eq!(a.result.outcome, b.result.outcome);
    assert!(a.events == b.events, "event streams differ");
    assert_eq!(a.checkpoints.len(), b.checkpoints.len());
    for (x, y) in a.checkpoints.iter().zip(&b.checkpoints) {
        assert_eq!(
            identity::world_hash(x).unwrap(),
            identity::world_hash(y).unwrap(),
            "checkpoint {} differs between thread counts",
            x.tick
        );
    }
    assert!(
        a.events
            .iter()
            .any(|e| matches!(e.event, PresentationEvent::Attack { .. }))
    );
    assert!(a.result.outcome.terminal_state_tick > 300);

    for checkpoint in b.checkpoints.iter().filter(|c| c.tick.is_multiple_of(200)) {
        if checkpoint.tick == 0 || checkpoint.tick >= b.result.outcome.terminal_state_tick {
            continue;
        }
        let mut request = pooled.request();
        request.checkpoint = checkpoint.clone();
        let partial = run_request(&request);
        assert_eq!(
            partial.result.final_hash, b.result.final_hash,
            "cold checkpoint {} diverges from the warm full replay",
            checkpoint.tick
        );
        assert_eq!(partial.result.outcome, b.result.outcome);
    }

    let request = pooled.request();
    let mut evicted = Sim::new(&request).unwrap();
    evicted.set_field_cache_capacity(1);
    let mut sink = |_: Output| {};
    let mut compared = 0;
    while evicted.state.tick < request.end_tick_exclusive {
        evicted.step(&mut sink).unwrap();
        if let Some(c) = b.checkpoints.iter().find(|c| c.tick == evicted.state.tick) {
            assert_eq!(
                identity::world_hash(c).unwrap(),
                evicted.hash_state().unwrap(),
                "S[{}] differs with an evicting cache",
                c.tick
            );
            compared += 1;
        }
        if evicted.inactive() {
            break;
        }
    }
    assert!(compared >= 3);
    assert_eq!(evicted.hash_state().unwrap(), b.result.final_hash);
}
