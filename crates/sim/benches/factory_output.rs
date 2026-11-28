//! Saturated factory outputs. Run with `cargo bench -p atemporal-sim --bench factory_output`.
#[path = "../tests/common/mod.rs"]
mod common;

use atemporal_sim::*;
use common::World;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

const SIZE: usize = 256;
const LANES: u16 = 16;
const BLOCKERS: u16 = 8;
const TICKS: Tick = 1000;

fn request() -> SimRequest {
    // Each factory opens onto a single-file lane with eight idle units and two free
    // tiles. The first two births displace the chain. The third stays paid and
    // searches the full dead end every tick. Walls isolate the independent lanes.
    let wall = "#".repeat(SIZE);
    let lane = format!(
        "#{}{}",
        ".".repeat(usize::from(BLOCKERS) + 3),
        "#".repeat(SIZE - usize::from(BLOCKERS) - 4)
    );
    let mut rows = vec![wall.as_str(); SIZE];
    for index in 0..usize::from(LANES) {
        rows[1 + index * 2] = &lane;
    }
    let mut world = World::new(&rows);
    world.config.max_tick = TICKS;
    world.config.stall_ticks = TICKS;
    world.config.snapshot_interval = TICKS;
    world.config.checkpoint_interval = TICKS;
    world.config.objective = Objective::Timed {
        lock_ticks_per_round: 100,
    };
    world.bank(0, 10_000.0);

    let mut factories = vec![];
    for index in 0..LANES {
        let y = 1 + index * 2;
        factories.push(world.spawn(0, "factory", 1, y));
        for x in 2..2 + BLOCKERS {
            world.spawn(0, "tank", x, y);
        }
    }
    world.turn(
        1,
        0,
        0,
        vec![Command::EditProduction {
            factories,
            edit: ProductionEdit::Append {
                items: vec!["grunt".into(); 3],
            },
        }],
    );
    world.request()
}

fn main() {
    let request = request();
    println!(
        "request SHA-256 {}",
        identity::canonical_hash(&request).unwrap()
    );
    for iteration in 1..=7 {
        let mut events = vec![];
        let start = Instant::now();
        let result = run(&request, &AtomicBool::new(false), &mut |output| {
            if let Output::Events(batch) = output {
                events.extend(batch);
            }
        })
        .unwrap();
        let elapsed = start.elapsed();
        let births = result
            .final_state
            .entities
            .iter()
            .filter(|e| e.born_at_tick.is_some())
            .count();
        let waiting = result
            .final_state
            .entities
            .iter()
            .filter(|e| {
                e.production
                    .as_ref()
                    .and_then(|p| p.active_item.as_ref())
                    .is_some_and(|a| a.awaiting_output)
            })
            .count();
        let displacements = events
            .iter()
            .filter(|e| matches!(e.event, PresentationEvent::Displacement { .. }))
            .count();
        assert_eq!(result.final_state.tick, TICKS);
        assert_eq!(births, usize::from(LANES) * 2);
        assert_eq!(waiting, usize::from(LANES));
        assert_eq!(
            displacements,
            usize::from(LANES) * usize::from(2 * BLOCKERS + 1)
        );
        println!(
            "{iteration}: {:.3} ms, {births} births, {waiting} paid items still jammed, {displacements} displacements, state {}, events {}",
            elapsed.as_secs_f64() * 1000.0,
            identity::canonical_hash(&result.final_state).unwrap(),
            identity::canonical_hash(&events).unwrap(),
        );
    }
}
