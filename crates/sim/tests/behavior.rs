//! Roster capability fixtures from the behavioral acceptance list: combat table, support/healing,
//! walls, priority tiers, damaged construction, queue edits, order locks and control groups.
mod common;
use atemporal_sim::*;
use common::*;

const OPEN: [&str; 8] = [
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
];

fn item(command: &CommandId, target_index: u32, item_index: u32) -> QueueItemId {
    identity::queue_item(command, target_index, item_index).unwrap()
}
fn born(command: &CommandId, target_index: u32, item_index: u32, occurrence: u32) -> EntityId {
    identity::production(&item(command, target_index, item_index), occurrence)
}

#[test]
fn direct_fire_needs_line_of_sight_and_idle_units_hold() {
    let mut w = World::new(&OPEN);
    let grunt = w.spawn(0, "grunt", 1, 1);
    let miner = w.spawn(1, "miner", 4, 1);
    let run = w.run();
    assert_eq!(entity(&w.at(1), &miner).hp, 52.0);
    assert_eq!(run.attacks(&grunt)[..2], [0, 3]);
    assert_eq!(run.destroyed(&miner), Some(21));

    let mut blocked = World::new(&[
        "..............",
        "...#..........",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
    ]);
    let grunt = blocked.spawn(0, "grunt", 1, 1);
    let miner = blocked.spawn(1, "miner", 4, 1);
    let run = blocked.run();
    assert!(run.attacks(&grunt).is_empty(), "rock blocks direct fire");
    let last = run.result.final_state.clone();
    assert_eq!(entity(&last, &grunt).tile, tile(1, 1), "idle never chases");
    assert_eq!(entity(&last, &grunt).engaged_target, None);
    assert_eq!(entity(&last, &miner).hp, 60.0);
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
}

#[test]
fn indirect_fire_ignores_intervening_rock() {
    let mut w = World::new(&[
        "..............",
        "...#..........",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
    ]);
    let artillery = w.spawn(0, "artillery", 1, 1);
    let miner = w.spawn(1, "miner", 5, 1);
    let run = w.run();
    assert_eq!(run.attacks(&artillery)[0], 0);
    assert_eq!(entity(&w.at(1), &miner).hp, 25.0);
    assert_eq!(entity(&w.at(1), &artillery).tile, tile(1, 1));
}

#[test]
fn melee_reaches_diagonal_neighbors_only() {
    let mut w = World::new(&OPEN);
    let grinder = w.spawn(0, "grinder", 1, 1);
    let miner = w.spawn(1, "miner", 2, 2);
    let run = w.run();
    assert_eq!(run.attacks(&grinder)[0], 0);
    assert_eq!(entity(&w.at(1), &miner).hp, 30.0);

    let mut far = World::new(&OPEN);
    let grinder = far.spawn(0, "grinder", 1, 1);
    let miner = far.spawn(1, "miner", 3, 1);
    let run = far.run();
    assert!(
        run.attacks(&grinder).is_empty(),
        "two tiles is out of melee reach"
    );
    assert_eq!(entity(&run.result.final_state, &miner).hp, 60.0);
    assert_eq!(entity(&run.result.final_state, &grinder).tile, tile(1, 1));
}

#[test]
fn turret_fires_automatically_inside_range_and_never_moves() {
    let mut w = World::new(&OPEN);
    let turret = w.spawn(0, "turret", 0, 0);
    let near = w.spawn(1, "miner", 5, 4);
    let far = w.spawn(1, "miner", 5, 5);
    let run = w.run();
    assert_eq!(run.attacks(&turret)[..2], [0, 4]);
    let s1 = w.at(1);
    assert_eq!(entity(&s1, &near).hp, 45.0);
    assert_eq!(entity(&s1, &far).hp, 60.0);
    assert_eq!(run.destroyed(&near), Some(12));
    let last = run.result.final_state.clone();
    assert_eq!(
        entity(&last, &far).hp,
        60.0,
        "7.07 tiles is outside range 7"
    );
    assert_eq!(entity(&last, &turret).tile, tile(0, 0));
    assert_eq!(entity(&last, &turret).engaged_target, None);
}

#[test]
fn idle_defender_holds_position_and_drops_targets_leaving_range() {
    let mut w = World::new(&OPEN);
    let grunt = w.spawn(0, "grunt", 1, 1);
    let miner = w.spawn_with(1, "miner", 3, 1, attack_move(10, 1));
    let run = w.run();
    assert_eq!(run.attacks(&grunt), vec![0, 3]);
    assert_eq!(entity(&w.at(4), &grunt).engaged_target, Some(miner.clone()));
    assert_eq!(entity(&w.at(7), &grunt).engaged_target, None);
    let last = run.result.final_state.clone();
    assert_eq!(entity(&last, &miner).hp, 44.0);
    assert_eq!(entity(&last, &miner).tile, tile(10, 1));
    assert_eq!(entity(&last, &grunt).tile, tile(1, 1));
}

#[test]
fn attack_move_pursues_within_vision_then_continues_to_destination() {
    let mut w = World::new(&OPEN);
    let grunt = w.spawn_with(0, "grunt", 1, 1, attack_move(12, 1));
    let miner = w.spawn(1, "miner", 5, 4);
    let far_miner = w.spawn(1, "miner", 1, 7);
    let run = w.run();
    assert_eq!(
        run.attacks(&grunt).len(),
        8,
        "60 hp needs eight 8-damage shots"
    );
    assert!(run.destroyed(&miner).is_some());
    let last = run.result.final_state.clone();
    assert_eq!(entity(&last, &grunt).tile, tile(12, 1));
    assert!(entity(&last, &grunt).goal_settled);
    assert_eq!(
        entity(&last, &far_miner).hp,
        60.0,
        "out of vision: never chased"
    );
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn support_reads_the_ally_target_from_the_prior_snapshot() {
    let mut w = World::new(&OPEN);
    let ally = w.spawn(0, "grunt", 2, 2);
    let enemy = w.spawn(1, "miner", 4, 2);
    let supporter = w.spawn_with(
        0,
        "grunt",
        2,
        5,
        Order::Support {
            target: ally.clone(),
        },
    );
    let run = w.run();
    let s1 = w.at(1);
    assert_eq!(entity(&s1, &ally).engaged_target, Some(enemy.clone()));
    assert_eq!(entity(&s1, &supporter).engaged_target, None);
    assert_eq!(
        entity(&s1, &supporter).tile,
        tile(2, 4),
        "followed the ally first"
    );
    assert_eq!(
        entity(&w.at(2), &supporter).engaged_target,
        Some(enemy.clone())
    );
    assert_eq!(run.attacks(&supporter)[0], 1);
    assert_eq!(run.attacks(&ally)[0], 0);
}

#[test]
fn mutual_support_reads_committed_targets_without_recursion() {
    let mut w = World::new(&OPEN);
    let a = identity::genesis(0, 0).unwrap();
    let b = identity::genesis(0, 1).unwrap();
    w.spawn_with(0, "grunt", 2, 2, Order::Support { target: b.clone() });
    w.spawn_with(0, "grunt", 2, 5, Order::Support { target: a.clone() });
    let enemy = w.spawn(1, "miner", 4, 2);
    let run = w.run();
    let s2 = w.at(2);
    assert_eq!(entity(&s2, &a).engaged_target, Some(enemy.clone()));
    assert_eq!(entity(&s2, &b).engaged_target, Some(enemy.clone()));
    assert_eq!(run.attacks(&a)[0], 0);
    assert_eq!(run.attacks(&b)[0], 1);
}

#[test]
fn walls_block_movement_until_destroyed() {
    let mut w = World::new(&[
        "##########",
        "#........#",
        "#........#",
        "#........#",
        "##########",
    ]);
    let miner = w.spawn_with(0, "miner", 1, 2, attack_move(8, 2));
    let grunt = w.spawn(0, "grunt", 2, 2);
    let walls: Vec<EntityId> = (1..=3).map(|y| w.spawn(1, "wall", 4, y)).collect();
    let run = w.run();
    let first_fall = run.destroyed(&walls[1]).expect("the middle wall falls");
    assert_eq!(first_fall, 54, "150 hp at 8 damage every 3 ticks");
    assert_eq!(
        entity(&w.at(first_fall), &miner).tile,
        tile(1, 2),
        "unreachable: no movement"
    );
    let last = run.result.final_state.clone();
    assert_eq!(
        entity(&last, &miner).tile,
        tile(8, 2),
        "wall destruction invalidates fields"
    );
    assert!(walls.iter().all(|id| !present(&last, id)));
    assert!(
        entity(&last, &grunt).tile.x <= 3,
        "idle grunt only yielded to allied traffic"
    );
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn priority_tiers_fund_high_first_then_equal_capped_shares() {
    let mut w = World::new(&OPEN);
    w.bank(0, 3.0);
    let high = w.spawn(0, "factory", 1, 1);
    let low = w.spawn(0, "factory", 1, 3);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::SetPriority {
                entities: vec![high.clone()],
                priority: Priority::High,
            },
            Command::SetPriority {
                entities: vec![low.clone()],
                priority: Priority::Low,
            },
            Command::EditProduction {
                factories: vec![high.clone(), low.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
        ],
    );
    let s1 = w.at(1);
    let paid = |s: &WorldState, id: &EntityId| {
        entity(s, id)
            .production
            .as_ref()
            .unwrap()
            .active_item
            .as_ref()
            .unwrap()
            .paid_matter
    };
    assert_eq!(paid(&s1, &high), 3.0);
    assert_eq!(paid(&s1, &low), 0.0);
    assert_eq!(s1.players[0].bank, 0.0);

    let mut w = World::new(&OPEN);
    w.bank(0, 4.0);
    let a = w.spawn(0, "factory", 1, 1);
    let b = w.spawn(0, "factory", 1, 3);
    let c = w.spawn(0, "factory", 1, 5);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::EditProduction {
                factories: vec![a.clone(), b.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
            Command::SetPriority {
                entities: vec![c.clone()],
                priority: Priority::Low,
            },
            Command::EditProduction {
                factories: vec![c.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
        ],
    );
    let s1 = w.at(1);
    assert_eq!(paid(&s1, &a), 2.0, "medium tier splits four matter equally");
    assert_eq!(paid(&s1, &b), 2.0);
    assert_eq!(paid(&s1, &c), 0.0, "lower tier waits");
    assert_eq!(s1.players[0].counters.unit_spend, 4.0);
}

#[test]
fn damage_during_funding_stays_lost_after_completion() {
    let mut w = World::new(&OPEN);
    w.bank(0, 1000.0);
    let site = w.site(0, "factory", 3, 2, 100.0);
    w.spawn_with(
        0,
        "constructor",
        2,
        2,
        Order::Construct {
            area: rect(3, 2, 3, 2),
        },
    );
    let turret = w.spawn(1, "turret", 8, 2);
    let run = w.run();
    assert_eq!(run.attacks(&turret)[..7], [0, 4, 8, 12, 16, 20, 24]);
    let s24 = w.at(24);
    assert_eq!(entity(&s24, &site).lifecycle, Lifecycle::Site);
    let s25 = w.at(25);
    let done = entity(&s25, &site);
    assert_eq!(done.lifecycle, Lifecycle::Complete);
    assert_eq!(done.paid_matter, 200.0);
    assert!(
        (done.hp - 295.0).abs() < 1e-6,
        "400 max minus seven 15-damage hits: {}",
        done.hp
    );
    assert!(
        s25.blueprints.is_empty(),
        "completion removes the blueprint record"
    );
    assert_eq!(s25.players[0].counters.structure_spend, 100.0);
}

#[test]
fn queue_edits_loops_and_distinct_ids_per_factory() {
    let mut w = World::new(&OPEN);
    w.bank(0, 10_000.0);
    let f1 = w.spawn(0, "factory", 1, 1);
    let f2 = w.spawn(0, "factory", 1, 5);
    let ids = w.turn(
        1,
        0,
        0,
        vec![
            Command::SetStoredOrder {
                factories: vec![f1.clone(), f2.clone()],
                order: attack_move(12, 3),
            },
            Command::EditProduction {
                factories: vec![f1.clone(), f2.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into(), "scout".into(), "tank".into()],
                },
            },
        ],
    );
    let append = ids[1].clone();
    let ids2 = w.turn(
        2,
        0,
        1,
        vec![
            Command::EditProduction {
                factories: vec![f1.clone()],
                edit: ProductionEdit::RemovePending {
                    item_ids: vec![item(&append, 0, 1)],
                },
            },
            Command::SetQueueLoop {
                factories: vec![f1.clone()],
                enabled: true,
            },
            Command::EditProduction {
                factories: vec![f2.clone()],
                edit: ProductionEdit::ReplacePending {
                    items: vec!["scout".into()],
                },
            },
        ],
    );
    let s5 = w.at(5);
    assert_eq!(entity(&s5, &born(&append, 0, 0, 0)).tile, tile(2, 1));
    assert_eq!(
        entity(&s5, &born(&append, 1, 0, 0)).tile,
        tile(2, 5),
        "distinct tuple per factory"
    );
    let s200 = w.at(200);
    assert!(
        !present(&s200, &born(&append, 0, 1, 0)),
        "removed scout is never built"
    );
    assert!(
        present(&s200, &born(&append, 0, 2, 0)),
        "tank after the grunt"
    );
    assert!(
        present(&s200, &born(&append, 0, 0, 1)),
        "loop re-queued the grunt at spawn"
    );
    assert!(present(&s200, &born(&append, 1, 0, 0)));
    assert!(
        present(&s200, &born(&ids2[2], 0, 0, 0)),
        "replacement scout"
    );
    assert!(
        !present(&s200, &born(&append, 1, 1, 0)),
        "replaced scout identity is gone"
    );
    assert!(
        !present(&s200, &born(&append, 1, 2, 0)),
        "replaced tank is never built"
    );
    let prod = entity(&s200, &f2).production.as_ref().unwrap();
    assert!(prod.pending_items.is_empty() && prod.active_item.is_none());
    let built: f64 = s200
        .entities
        .iter()
        .filter(|e| e.born_at_tick.is_some())
        .map(|e| e.paid_matter)
        .sum();
    let active: f64 = s200
        .entities
        .iter()
        .filter_map(|e| e.production.as_ref()?.active_item.as_ref())
        .map(|a| a.paid_matter)
        .sum();
    assert!((s200.players[0].counters.unit_spend - built - active).abs() < 1e-9);
}

#[test]
fn cancelling_the_active_item_loses_its_matter() {
    let mut w = World::new(&OPEN);
    w.bank(0, 1000.0);
    let f = w.spawn(0, "factory", 1, 1);
    let ids = w.turn(
        1,
        0,
        0,
        vec![Command::EditProduction {
            factories: vec![f.clone()],
            edit: ProductionEdit::Append {
                items: vec!["tank".into(), "grunt".into()],
            },
        }],
    );
    w.turn(
        2,
        0,
        5,
        vec![Command::EditProduction {
            factories: vec![f.clone()],
            edit: ProductionEdit::CancelActive {},
        }],
    );
    let run = w.run();
    let last = run.result.final_state.clone();
    assert!(!present(&last, &born(&ids[0], 0, 0, 0)), "cancelled tank");
    assert!(
        present(&last, &born(&ids[0], 0, 1, 0)),
        "next item proceeds"
    );
    assert_eq!(last.players[0].counters.lost_invested_matter, 15.0);
    assert_eq!(last.players[0].bank, 1000.0 - 15.0 - 15.0);
}

#[test]
fn blocked_output_never_duplicates_or_double_charges_a_birth() {
    let mut w = World::new(&OPEN);
    w.bank(0, 1000.0);
    let f = w.spawn(0, "factory", 1, 1);
    let blocker = w.spawn(0, "miner", 2, 1);
    let ids = w.turn(
        1,
        0,
        0,
        vec![
            Command::SetStoredOrder {
                factories: vec![f.clone()],
                order: attack_move(10, 1),
            },
            Command::SetQueueLoop {
                factories: vec![f.clone()],
                enabled: true,
            },
            Command::EditProduction {
                factories: vec![f.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
        ],
    );
    w.turn(
        2,
        0,
        30,
        vec![Command::AssignOrder {
            entities: vec![blocker.clone()],
            order: attack_move(10, 4),
        }],
    );
    let grunt = born(&ids[2], 0, 0, 0);
    let s30 = w.at(30);
    assert!(!present(&s30, &grunt));
    let prod = entity(&s30, &f).production.as_ref().unwrap();
    assert!(prod.active_item.as_ref().unwrap().awaiting_output);
    assert!(
        prod.occurrence_counters.is_empty(),
        "occurrence advances only at spawn"
    );
    assert!(prod.pending_items.is_empty(), "loop re-adds only at spawn");
    assert_eq!(s30.players[0].counters.unit_spend, 15.0);
    let s31 = w.at(31);
    let g = entity(&s31, &grunt);
    assert_eq!(
        (g.tile, g.born_at_tick, &g.action),
        (tile(2, 1), Some(30), &attack_move(10, 1))
    );
    let prod = entity(&s31, &f).production.as_ref().unwrap();
    assert_eq!(prod.occurrence_counters[0].next_occurrence, 1);
    assert_eq!(prod.pending_items.len(), 1);
    assert_eq!(s31.players[0].counters.unit_spend, 15.0);
    assert!(present(&w.at(60), &born(&ids[2], 0, 0, 1)));
}

fn lock_world() -> (World, EntityId) {
    let mut w = World::new(&OPEN);
    w.config.future_orders.window_ticks = Some(10);
    w.config.max_tick = 200;
    let u = w.spawn(0, "grunt", 1, 1);
    (w, u)
}
fn order_u(u: &EntityId, x: u16) -> Vec<Command> {
    vec![Command::AssignOrder {
        entities: vec![u.clone()],
        order: attack_move(x, 7),
    }]
}

#[test]
fn drop_window_lock_skips_older_rounds_inside_its_interval() {
    let (mut w, u) = lock_world();
    let mut older = vec![];
    for (k, tick) in [20, 21, 30, 31].into_iter().enumerate() {
        older.push(
            w.turn_from(
                1,
                0,
                tick,
                FutureOrderPolicy::Keep,
                k as u32,
                order_u(&u, 2 + k as u16),
            )[0]
            .clone(),
        );
    }
    w.turn_with(2, 0, 20, FutureOrderPolicy::DropWindow, order_u(&u, 9));
    let same = w.turn_from(2, 0, 25, FutureOrderPolicy::Keep, 1, order_u(&u, 10))[0].clone();
    let newer = w.turn_with(3, 0, 26, FutureOrderPolicy::Keep, order_u(&u, 11))[0].clone();
    let run = w.run();
    let skipped = |id: &CommandId| {
        run.outcome(id)
            .skipped
            .iter()
            .map(|s| s.reason)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run.outcome(&older[0]).applied_entities,
        vec![u.clone()],
        "tick 20 remains"
    );
    assert_eq!(skipped(&older[1]), vec![SkipReason::LockedByLaterRound]);
    assert_eq!(skipped(&older[2]), vec![SkipReason::LockedByLaterRound]);
    assert_eq!(
        run.outcome(&older[3]).applied_entities,
        vec![u.clone()],
        "tick 31 remains"
    );
    assert_eq!(
        run.outcome(&same).applied_entities,
        vec![u.clone()],
        "same round executes"
    );
    assert_eq!(
        run.outcome(&newer).applied_entities,
        vec![u.clone()],
        "newer round executes"
    );
    assert_eq!(entity(&w.at(21), &u).action, attack_move(9, 7));
    assert_eq!(entity(&w.at(32), &u).action, attack_move(5, 7));
    assert_eq!(
        entity(&w.at(21), &u).order_locks,
        vec![OrderLock {
            from_tick: 20,
            until_tick: 30,
            issued_round: 2
        }]
    );
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn drop_all_lock_also_skips_the_tick_after_the_window() {
    let (mut w, u) = lock_world();
    let late = w.turn_with(1, 0, 31, FutureOrderPolicy::Keep, order_u(&u, 5))[0].clone();
    w.turn_with(2, 0, 20, FutureOrderPolicy::DropAll, order_u(&u, 9));
    let run = w.run();
    assert_eq!(
        run.outcome(&late).skipped[0].reason,
        SkipReason::LockedByLaterRound
    );
    assert_eq!(entity(&w.at(40), &u).action, attack_move(9, 7));
}

#[test]
fn short_newer_window_keeps_the_longer_prior_lock() {
    let (mut w, u) = lock_world();
    let old = w.turn_with(1, 0, 40, FutureOrderPolicy::Keep, order_u(&u, 5))[0].clone();
    let round2_late = w.turn_from(2, 0, 40, FutureOrderPolicy::Keep, 1, order_u(&u, 6))[0].clone();
    w.turn_with(2, 0, 20, FutureOrderPolicy::DropAll, order_u(&u, 9));
    w.turn_with(3, 0, 22, FutureOrderPolicy::DropWindow, order_u(&u, 8));
    let run = w.run();
    assert_eq!(
        run.outcome(&old).skipped[0].reason,
        SkipReason::LockedByLaterRound
    );
    assert_eq!(
        run.outcome(&round2_late).applied_entities,
        vec![u.clone()],
        "the round-3 window ended at 32; the round-2 lock never blocks round 2"
    );
    assert_eq!(
        entity(&w.at(23), &u).order_locks.len(),
        2,
        "both entries retained"
    );
}

#[test]
fn explicit_delivery_with_one_locked_target_still_reaches_the_other() {
    let (mut w, a) = lock_world();
    let b = w.spawn(0, "grunt", 1, 3);
    w.turn_with(2, 0, 20, FutureOrderPolicy::DropWindow, order_u(&a, 9));
    let both = w.turn(
        1,
        0,
        25,
        vec![Command::AssignOrder {
            entities: vec![a.clone(), b.clone()],
            order: attack_move(5, 7),
        }],
    )[0]
    .clone();
    let run = w.run();
    let outcome = run.outcome(&both);
    assert_eq!(outcome.applied_entities, vec![b.clone()]);
    assert_eq!(outcome.skipped[0].entity_id, Some(a.clone()));
    assert_eq!(entity(&w.at(26), &a).action, attack_move(9, 7));
    assert_eq!(entity(&w.at(26), &b).action, attack_move(5, 7));
}

#[test]
fn missing_replacement_target_installs_no_lock() {
    let (mut w, u) = lock_world();
    let ghost = identity::genesis(0, 7).unwrap();
    let missing = w.turn_with(
        2,
        0,
        20,
        FutureOrderPolicy::DropWindow,
        vec![Command::AssignOrder {
            entities: vec![ghost.clone()],
            order: attack_move(9, 7),
        }],
    )[0]
    .clone();
    let old = w.turn_with(1, 0, 25, FutureOrderPolicy::Keep, order_u(&u, 5))[0].clone();
    let run = w.run();
    assert_eq!(run.outcome(&missing).skipped[0].reason, SkipReason::Absent);
    assert_eq!(run.outcome(&old).applied_entities, vec![u.clone()]);
}

#[test]
fn group_slot_locks_block_older_saved_orders_and_newborns_inherit() {
    let (mut w, u) = lock_world();
    w.bank(0, 1000.0);
    let f = w.spawn(0, "factory", 1, 5);
    let g1 = w.group(0, 1);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::EditGroupMembers {
                group: g1.clone(),
                edit: MemberEdit::Add {
                    entities: vec![u.clone()],
                },
            },
            Command::BindFactoryGroup {
                factories: vec![f.clone()],
                group: Some(g1.clone()),
            },
        ],
    );
    let append = w.turn_from(
        1,
        0,
        18,
        FutureOrderPolicy::Keep,
        2,
        vec![Command::EditProduction {
            factories: vec![f.clone()],
            edit: ProductionEdit::Append {
                items: vec!["grunt".into()],
            },
        }],
    )[0]
    .clone();
    let v = born(&append, 0, 0, 0);
    w.turn_with(
        2,
        0,
        20,
        FutureOrderPolicy::DropWindow,
        vec![Command::AssignGroupOrder {
            group: g1.clone(),
            order: attack_move(9, 7),
        }],
    );
    let old_group = w.turn_from(
        1,
        0,
        25,
        FutureOrderPolicy::Keep,
        3,
        vec![Command::AssignGroupOrder {
            group: g1.clone(),
            order: Order::Idle {},
        }],
    )[0]
    .clone();
    let old_direct = w.turn_from(
        1,
        0,
        26,
        FutureOrderPolicy::Keep,
        4,
        vec![Command::AssignOrder {
            entities: vec![v.clone()],
            order: Order::Idle {},
        }],
    )[0]
    .clone();
    let after = w.turn_from(
        1,
        0,
        31,
        FutureOrderPolicy::Keep,
        5,
        vec![Command::AssignOrder {
            entities: vec![v.clone()],
            order: Order::Idle {},
        }],
    )[0]
    .clone();
    let run = w.run();
    let s23 = w.at(23);
    let newborn = entity(&s23, &v);
    assert_eq!(newborn.born_at_tick, Some(22));
    assert_eq!(
        newborn.action,
        attack_move(9, 7),
        "birth takes the saved group order"
    );
    assert_eq!(
        newborn.order_locks,
        entity(&s23, &u).order_locks,
        "inherited slot lock"
    );
    assert_eq!(
        run.outcome(&old_group).skipped[0].reason,
        SkipReason::LockedByLaterRound
    );
    assert_eq!(
        run.outcome(&old_direct).skipped[0].reason,
        SkipReason::LockedByLaterRound
    );
    assert_eq!(run.outcome(&after).applied_entities, vec![v.clone()]);
    let group = |s: &WorldState| {
        s.control_groups
            .iter()
            .find(|g| g.id == g1)
            .unwrap()
            .clone()
    };
    assert_eq!(group(&s23).members, {
        let mut m = vec![u.clone(), v.clone()];
        m.sort();
        m
    });
    assert_eq!(
        group(&w.at(26)).latest_order.as_ref().unwrap().order,
        attack_move(9, 7)
    );
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn member_locks_preserve_group_saved_order_writes() {
    let (mut w, u) = lock_world();
    let g1 = w.group(0, 1);
    w.turn(
        1,
        0,
        0,
        vec![Command::EditGroupMembers {
            group: g1.clone(),
            edit: MemberEdit::Add {
                entities: vec![u.clone()],
            },
        }],
    );
    w.turn_with(2, 0, 20, FutureOrderPolicy::DropWindow, order_u(&u, 9));
    let group_order = w.turn_from(
        1,
        0,
        25,
        FutureOrderPolicy::Keep,
        1,
        vec![Command::AssignGroupOrder {
            group: g1.clone(),
            order: attack_move(7, 7),
        }],
    )[0]
    .clone();
    let run = w.run();
    let outcome = run.outcome(&group_order);
    assert_eq!(outcome.skipped[0].reason, SkipReason::LockedByLaterRound);
    let s26 = w.at(26);
    let slot = s26.control_groups.iter().find(|g| g.id == g1).unwrap();
    assert_eq!(
        slot.latest_order.as_ref().unwrap().order,
        attack_move(7, 7),
        "saved"
    );
    assert_eq!(
        entity(&s26, &u).action,
        attack_move(9, 7),
        "member kept the newer order"
    );
}

#[test]
fn group_order_individual_override_then_spawn_and_later_group_override() {
    let mut w = World::new(&OPEN);
    w.bank(0, 1000.0);
    let f = w.spawn(0, "factory", 1, 1);
    let u = w.spawn(0, "grunt", 1, 3);
    let g1 = w.group(0, 1);
    let g2 = w.group(0, 2);
    let a = attack_move(8, 1);
    let b = attack_move(8, 4);
    let c = attack_move(1, 6);
    let ids = w.turn(
        1,
        0,
        0,
        vec![
            Command::EditGroupMembers {
                group: g1.clone(),
                edit: MemberEdit::Add {
                    entities: vec![u.clone()],
                },
            },
            Command::BindFactoryGroup {
                factories: vec![f.clone()],
                group: Some(g1.clone()),
            },
            Command::AssignGroupOrder {
                group: g1.clone(),
                order: a.clone(),
            },
            Command::AssignOrder {
                entities: vec![u.clone()],
                order: b.clone(),
            },
            Command::EditProduction {
                factories: vec![f.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
            Command::AssignGroupOrder {
                group: g2.clone(),
                order: c.clone(),
            },
        ],
    );
    w.turn(
        2,
        0,
        20,
        vec![Command::AssignGroupOrder {
            group: g1.clone(),
            order: c.clone(),
        }],
    );
    let v = born(&ids[4], 0, 0, 0);
    let s10 = w.at(10);
    assert_eq!(entity(&s10, &u).action, b);
    assert_eq!(entity(&s10, &v).action, a);
    let empty = s10.control_groups.iter().find(|g| g.id == g2).unwrap();
    assert!(empty.members.is_empty());
    assert_eq!(
        empty.latest_order.as_ref().unwrap().order,
        c,
        "empty groups store orders"
    );
    let s21 = w.at(21);
    assert_eq!(entity(&s21, &u).action, c);
    assert_eq!(entity(&s21, &v).action, c);
}

fn medic_content(weapon: bool) -> Content {
    let mut content = load_content(include_str!("../../../config/content.yaml")).unwrap();
    let mut medic = content
        .types
        .iter()
        .find(|t| t.key == "scout")
        .unwrap()
        .clone();
    medic.key = "medic".into();
    medic.weapon = weapon.then_some(Weapon {
        range: 3.0,
        damage: 5.0,
        cooldown: 2,
        indirect: false,
        visual_style: VisualStyle::Direct,
    });
    medic.healing = Some(Healing {
        range: 2.0,
        hp_per_matter: 2.0,
        demand: 3.0,
        cooldown: 1,
    });
    content.types.push(medic);
    normalize_content(content).unwrap()
}

#[test]
fn healing_spends_bank_matter_and_never_exceeds_max_hp() {
    let mut w = World::with_content(&OPEN, medic_content(false));
    w.bank(0, 100.0);
    let ally = w.spawn(0, "grunt", 2, 2);
    w.entity_mut(&ally).hp = 20.0;
    let medic = w.spawn_with(
        0,
        "medic",
        2,
        3,
        Order::Support {
            target: ally.clone(),
        },
    );
    let run = w.run();
    assert_eq!(entity(&w.at(1), &ally).hp, 26.0);
    let s5 = w.at(5);
    assert_eq!(entity(&s5, &ally).hp, 45.0);
    assert_eq!(s5.players[0].bank, 87.5);
    assert_eq!(s5.players[0].counters.total_spend, 12.5);
    assert_eq!(entity(&s5, &medic).tile, tile(2, 3), "held while healing");
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);

    let mut broke = World::with_content(&OPEN, medic_content(false));
    let ally = broke.spawn(0, "grunt", 2, 2);
    broke.entity_mut(&ally).hp = 20.0;
    broke.spawn_with(
        0,
        "medic",
        2,
        3,
        Order::Support {
            target: ally.clone(),
        },
    );
    let run = broke.run();
    assert_eq!(
        entity(&run.result.final_state, &ally).hp,
        20.0,
        "no matter, no healing"
    );
}

#[test]
fn healer_prefers_healing_over_firing() {
    let mut w = World::with_content(&OPEN, medic_content(true));
    w.bank(0, 100.0);
    let ally = w.spawn(0, "grunt", 2, 2);
    w.entity_mut(&ally).hp = 39.0;
    let medic = w.spawn_with(
        0,
        "medic",
        2,
        3,
        Order::Support {
            target: ally.clone(),
        },
    );
    w.spawn(1, "miner", 4, 4);
    let run = w.run();
    assert_eq!(entity(&w.at(1), &ally).hp, 45.0);
    let shots = run.attacks(&medic);
    assert!(shots[0] >= 1, "no shot while healing was legal: {shots:?}");
}

#[test]
fn long_cooldown_with_a_legal_target_defers_inactivity() {
    let mut content = load_content(include_str!("../../../config/content.yaml")).unwrap();
    content
        .types
        .iter_mut()
        .find(|t| t.key == "turret")
        .unwrap()
        .weapon
        .as_mut()
        .unwrap()
        .cooldown = 200;
    let mut w = World::with_content(&OPEN, content);
    let turret = w.spawn(0, "turret", 0, 0);
    let miner = w.spawn(1, "miner", 5, 0);
    let run = w.run();
    assert_eq!(run.attacks(&turret), vec![0, 200, 400, 600]);
    assert_eq!(run.destroyed(&miner), Some(600));
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_eq!(run.result.outcome.terminal_state_tick, 651);
}

#[test]
fn unfunded_production_does_not_defer_inactivity() {
    let mut w = World::new(&OPEN);
    let f = w.spawn(0, "factory", 1, 1);
    w.turn(
        1,
        0,
        0,
        vec![Command::EditProduction {
            factories: vec![f.clone()],
            edit: ProductionEdit::Append {
                items: vec!["grunt".into()],
            },
        }],
    );
    let run = w.run();
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_eq!(run.result.outcome.terminal_state_tick, w.config.stall_ticks);
}
