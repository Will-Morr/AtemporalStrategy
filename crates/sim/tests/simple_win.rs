mod common;
use atemporal_sim::*;
use common::*;

const MAP: [&str; 8] = ["................"; 8];
fn world() -> World {
    let mut w = World::new(&MAP);
    w.config.objective = Objective::SimpleWin {
        rules: ScoreboardRules::default(),
    };
    w
}

#[test]
fn stops_on_last_building_destruction_despite_active_units_and_future_orders() {
    let mut w = world();
    w.spawn(0, "turret", 1, 1);
    let target = w.spawn(1, "turret", 5, 1);
    w.entity_mut(&target).hp = 1.0;
    let constructor = w.spawn_with(
        1,
        "constructor",
        12,
        6,
        Order::AttackMove {
            destination: tile(13, 6),
        },
    );
    w.turn(
        1,
        1,
        100,
        vec![Command::AssignOrder {
            entities: vec![constructor],
            order: Order::AttackMove {
                destination: tile(10, 6),
            },
        }],
    );
    let mut request = w.request();
    request.minimum_end_tick = 200;
    let run = run_request(&request);
    let destroyed = run.destroyed(&target).expect("last turret destroyed");
    assert_eq!(run.result.outcome.terminal_state_tick, destroyed + 1);
    assert_eq!(run.result.outcome.stop_reason, StopReason::Elimination);
    assert_eq!(run.result.outcome.survivors, vec![0]);
    assert_eq!(run.result.outcome.eliminated, vec![1]);
}

#[test]
fn completed_buildings_survive_without_constructors_including_silos() {
    let mut w = world();
    w.spawn(0, "turret", 1, 1);
    w.spawn(1, "silo", 12, 6);
    let run = w.run();
    assert_eq!(run.result.outcome.kind, OutcomeKind::Stalemate);
    assert!(run.result.outcome.eliminated.is_empty());
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
}

#[test]
fn unfinished_buildings_do_not_prevent_loss_and_simultaneous_losses_draw() {
    let mut w = world();
    let site = w.spawn(0, "turret", 1, 1);
    w.entity_mut(&site).lifecycle = Lifecycle::Site;
    w.entity_mut(&site).paid_matter = 1.0;
    w.entity_mut(&site).hp = 1.0;
    w.spawn(0, "constructor", 1, 6);
    w.spawn(1, "constructor", 12, 6);
    let run = w.run();
    assert_eq!(run.result.outcome.terminal_state_tick, 1);
    assert_eq!(run.result.outcome.kind, OutcomeKind::Draw);
    assert_eq!(run.result.outcome.eliminated, vec![0, 1]);
}
