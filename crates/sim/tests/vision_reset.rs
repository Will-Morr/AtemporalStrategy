mod common;
use atemporal_sim::*;
use common::*;

#[test]
fn sight_reveals_wall_faces_and_corners_but_cannot_cross_a_partition() {
    let world = World::new(&[
        "........", "...#....", "...#....", "...#....", "...#....", "...#....", "...#....",
        "...#....",
    ]);
    let visible = |origin: Tile, built: Option<Tile>| {
        let mut tiles = std::collections::BTreeSet::new();
        vision::flood_vision(
            &world.state.terrain,
            origin,
            6.0,
            |tile| Some(tile) == built,
            |tile| {
                tiles.insert(tile);
            },
        );
        tiles
    };
    assert!(visible(tile(2, 1), None).contains(&tile(4, 1)));
    assert!(visible(tile(2, 4), None).contains(&tile(3, 4)));
    assert!(!visible(tile(2, 4), None).contains(&tile(4, 4)));
    assert!(!visible(tile(2, 1), Some(tile(3, 0))).contains(&tile(4, 1)));
}

#[test]
fn reset_only_removes_prior_round_future_inputs_for_its_owner() {
    let mut w = World::new(&["........"; 8]);
    let mine = w.spawn(0, "miner", 0, 0);
    let theirs = w.spawn(1, "miner", 7, 7);
    let order = |id: &EntityId, priority| Command::SetPriority {
        entities: vec![id.clone()],
        priority,
    };
    w.turn(1, 0, 5, vec![order(&mine, Priority::Low)]);
    w.turn(1, 1, 5, vec![order(&theirs, Priority::Low)]);
    let before = w.request();
    w.turn(2, 0, 2, vec![order(&mine, Priority::High)]);
    w.turn(
        3,
        0,
        2,
        vec![
            Command::ClearFutureOrders {},
            order(&mine, Priority::Medium),
        ],
    );
    let after = w.at(6);
    assert_eq!(entity(&after, &mine).priority, Priority::Medium);
    assert_eq!(entity(&after, &theirs).priority, Priority::Low);
    assert_eq!(
        entity(&reconstruct(&before, 6).unwrap(), &mine).priority,
        Priority::Low
    );
    w.turn(4, 0, 5, vec![order(&mine, Priority::Off)]);
    assert_eq!(entity(&w.at(6), &mine).priority, Priority::Off);
}
