use atemporal_contracts::{locks::*, *};
#[test]
fn interval_rounds_overlap_and_expiry_preserve_exact_restrictions() {
    let mut locks = vec![];
    install(
        &mut locks,
        20,
        2,
        FutureOrderPolicy::DropWindow,
        Some(10),
        100,
    )
    .unwrap();
    for (tick, expected) in [(20, false), (21, true), (30, true), (31, false)] {
        assert_eq!(blocks(&locks, tick, 1), expected);
    }
    assert!(!blocks(&locks, 25, 2));
    assert!(!blocks(&locks, 25, 3));
    install(
        &mut locks,
        22,
        3,
        FutureOrderPolicy::DropWindow,
        Some(2),
        100,
    )
    .unwrap();
    assert_eq!(locks.len(), 2);
    assert!(blocks(&locks, 24, 2));
    assert!(!blocks(&locks, 25, 2));
    assert!(blocks(&locks, 30, 1));
    let saved = serde_json::to_string(&locks).unwrap();
    let mut restored: Vec<OrderLock> = serde_json::from_str(&saved).unwrap();
    canonicalize(&mut restored, 25).unwrap();
    assert_eq!(restored.len(), 1);
    canonicalize(&mut restored, 31).unwrap();
    assert!(restored.is_empty());
}
#[test]
fn keep_preserves_locks_and_dominated_entries_prune_without_erasing_all() {
    let mut locks = vec![];
    install(&mut locks, 20, 2, FutureOrderPolicy::DropAll, None, 100).unwrap();
    install(&mut locks, 21, 2, FutureOrderPolicy::DropAll, None, 100).unwrap();
    assert_eq!(locks.len(), 1);
    let before = locks.clone();
    install(&mut locks, 22, 3, FutureOrderPolicy::Keep, None, 100).unwrap();
    assert_eq!(locks, before);
    install(&mut locks, 23, 3, FutureOrderPolicy::DropAll, None, 100).unwrap();
    assert!(blocks(&locks, 23, 1));
    assert!(!blocks(&locks, 23, 2));
    canonicalize(&mut locks, 24).unwrap();
    assert_eq!(locks.len(), 1);
    assert!(blocks(&locks, 99, 2));
}
#[test]
fn checkpoint_hash_tracks_locks_and_local_movement_state() {
    let fixture: GoldenWorldFixture =
        serde_json::from_str(include_str!("../../../fixtures/worlds/quiet.json")).unwrap();
    let mut state = fixture.request.checkpoint;
    state.tick = 21;
    let original = identity::world_hash(&state).unwrap();
    install(
        &mut state.entities[0].order_locks,
        20,
        2,
        FutureOrderPolicy::DropAll,
        None,
        100,
    )
    .unwrap();
    assert_ne!(original, identity::world_hash(&state).unwrap());
    let locked = identity::world_hash(&state).unwrap();
    state.entities[0].local_detour.push(Tile { x: 2, y: 1 });
    assert_ne!(locked, identity::world_hash(&state).unwrap());
    let detour = identity::world_hash(&state).unwrap();
    state.entities[0].goal_settled = true;
    assert_ne!(detour, identity::world_hash(&state).unwrap());
}
