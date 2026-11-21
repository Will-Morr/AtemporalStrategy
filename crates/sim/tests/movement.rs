//! Movement fixtures: corner rules, four/eight neighbors, allied displacement, packed corridors,
//! factory-output crowds, same-goal settling and radius-6 detours across checkpoints.
mod common;
use atemporal_sim::*;
use common::*;

#[test]
fn walkers_never_cut_corners() {
    let mut w = World::new(&["......", "..#...", "......", "......", "......", "......"]);
    let grunt = w.spawn_with(0, "grunt", 1, 1, attack_move(2, 2));
    assert_eq!(
        entity(&w.at(1), &grunt).tile,
        tile(1, 2),
        "rock at (2,1) forbids the diagonal"
    );
    assert_eq!(entity(&w.at(3), &grunt).tile, tile(2, 2));

    let mut open = World::new(&["......", "......", "......", "......", "......", "......"]);
    let grunt = open.spawn_with(0, "grunt", 1, 1, attack_move(2, 2));
    assert_eq!(
        entity(&open.at(1), &grunt).tile,
        tile(2, 2),
        "free corner: one diagonal step"
    );
}

#[test]
fn vehicles_walk_orthogonally_and_are_displaced_orthogonally() {
    let mut w = World::new(&["......", "......", "......", "......", "......", "......"]);
    let miner = w.spawn_with(0, "miner", 1, 1, attack_move(3, 3));
    let states = trace(&w);
    assert_motion_invariants(&w, &states);
    assert_eq!(
        entity(&states[1], &miner).tile,
        tile(2, 1),
        "initial north facing prefers a horizontal step"
    );
    assert_eq!(
        entity(&states[10], &miner).tile,
        tile(3, 3),
        "four steps at cooldown 3"
    );

    for seed in 0..12u64 {
        // The blocker's only diagonal exit (3,2) is legal for walkers but never for vehicles.
        let mut w = World::new(&["######", "#....#", "##...#", "######"]);
        w.config.seed = seed.try_into().unwrap();
        let grunt = w.spawn_with(0, "grunt", 1, 1, attack_move(3, 1));
        let blocker = w.spawn(0, "miner", 2, 1);
        let s1 = w.at(1);
        assert_eq!(
            entity(&s1, &grunt).tile,
            tile(2, 1),
            "seed {seed}: idle blocker yielded"
        );
        let b = entity(&s1, &blocker);
        let manhattan = (i32::from(b.tile.x) - 2).abs() + (i32::from(b.tile.y) - 1).abs();
        assert_eq!(
            manhattan, 1,
            "seed {seed}: displaced vehicle stays orthogonal: {:?}",
            b.tile
        );
        assert!(
            b.next_move_tick >= 3,
            "displacement charges the blocker's movement cooldown"
        );
    }
}

#[test]
fn allied_displacement_never_moves_an_entity_twice() {
    let mut w = World::new(&[
        "############",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        "############",
    ]);
    for y in 1..=4 {
        for x in 1..=2 {
            w.spawn_with(0, "grunt", x, y, attack_move(10, 5 - y));
        }
        for x in 4..=7 {
            w.spawn(0, if y % 2 == 0 { "miner" } else { "grunt" }, x, y);
        }
    }
    let states = trace(&w);
    assert_motion_invariants(&w, &states);
    let run = w.run();
    assert!(
        !run.displacements().is_empty(),
        "the crowd forced displacement"
    );
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    let last = &states[states.len() - 1];
    for e in last.entities.iter().filter(|e| e.action != Order::Idle {}) {
        assert!(
            e.tile.x >= 8,
            "{:?} crossed the idle crowd: {:?}",
            e.id,
            e.tile
        );
    }
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn packed_opposing_corridor_resolves_without_overlap() {
    let mut w = World::new(&["############", "#..........#", "############"]);
    let east: Vec<EntityId> = (1..=3)
        .map(|x| w.spawn_with(0, "grunt", x, 1, attack_move(10, 1)))
        .collect();
    let west: Vec<EntityId> = (8..=10)
        .map(|x| w.spawn_with(0, "grunt", x, 1, attack_move(1, 1)))
        .collect();
    let states = trace(&w);
    assert_motion_invariants(&w, &states);
    let last = &states[states.len() - 1];
    for id in &east {
        assert!(
            entity(last, id).tile.x >= 8,
            "{:?} reached the east end",
            id
        );
    }
    for id in &west {
        assert!(
            entity(last, id).tile.x <= 3,
            "{:?} reached the west end",
            id
        );
    }
    let run = w.run();
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn factory_output_crowd_clears_through_ordinary_traffic() {
    let mut w = World::new(&[
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
    ]);
    w.bank(0, 180.0);
    let f = w.spawn(0, "factory", 1, 1);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::SetStoredOrder {
                factories: vec![f.clone()],
                order: attack_move(12, 6),
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
    let states = trace(&w);
    assert_motion_invariants(&w, &states);
    let last = &states[states.len() - 1];
    let born: Vec<&EntityState> = last
        .entities
        .iter()
        .filter(|e| e.born_at_tick.is_some())
        .collect();
    assert_eq!(born.len(), 12, "180 matter buys twelve grunts");
    for g in &born {
        let d = (i32::from(g.tile.x) - 12)
            .abs()
            .max((i32::from(g.tile.y) - 6).abs());
        assert!(
            d <= 2 && g.goal_settled,
            "{:?} settled near the goal: {:?}",
            g.id,
            g.tile
        );
    }
    let run = w.run();
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn same_goal_group_settles_without_perpetual_displacement() {
    let mut w = World::new(&[
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
        "..............",
    ]);
    let ids: Vec<EntityId> = (1..=3)
        .flat_map(|y| (1..=3).map(move |x| (x, y)))
        .map(|(x, y)| w.spawn_with(0, "grunt", x, y, attack_move(10, 4)))
        .collect();
    let run = w.run();
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    let last = &run.result.final_state;
    for id in &ids {
        let e = entity(last, id);
        let d = (i32::from(e.tile.x) - 10)
            .abs()
            .max((i32::from(e.tile.y) - 4).abs());
        assert!(
            e.goal_settled && d <= 1,
            "{:?} settled in the 3×3 around the goal: {:?}",
            id,
            e.tile
        );
    }
    assert!(
        run.result.outcome.last_progress_tick < 120,
        "no displacement churn after arrival"
    );
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn static_targets_seed_adjacent_cells_and_stay_solid() {
    let mut w = World::new(&[
        "........", "........", "........", "........", "........", "........", "........",
        "........",
    ]);
    let miner = w.spawn_with(0, "miner", 1, 2, attack_move(5, 2));
    let wall = w.spawn(1, "wall", 5, 2);
    let states = trace(&w);
    assert_motion_invariants(&w, &states);
    let last = &states[states.len() - 1];
    assert!(present(last, &wall));
    let m = entity(last, &miner);
    assert_eq!((m.tile, m.goal_settled), (tile(4, 2), true));
    assert!(states.iter().all(|s| entity(s, &miner).tile != tile(5, 2)));
}

#[test]
fn radius_six_detour_keeps_its_uphill_first_step_across_checkpoints() {
    let mut w = World::new(&[
        "##############",
        "###.......####",
        "###.#####.####",
        "#............#",
        "##############",
    ]);
    let miner = w.spawn_with(0, "miner", 1, 3, attack_move(12, 3));
    let blocker = w.spawn(1, "miner", 6, 3);
    let run = w.run();
    let states = trace(&w);
    assert_motion_invariants(&w, &states);
    assert_eq!(entity(&states[13], &miner).tile, tile(5, 3));
    let detour = &entity(&states[15], &miner).local_detour;
    assert_eq!(
        detour.first(),
        Some(&tile(4, 3)),
        "necessary uphill first step"
    );
    assert_eq!(
        detour.last(),
        Some(&tile(11, 3)),
        "lowest reachable field distance in radius 6"
    );
    assert!(
        states.iter().any(|s| entity(s, &miner).tile == tile(3, 1)),
        "took the upper corridor"
    );
    let last = &states[states.len() - 1];
    assert_eq!(entity(last, &miner).tile, tile(12, 3));
    assert_eq!(
        entity(last, &blocker).tile,
        tile(6, 3),
        "enemy blocker was never displaced"
    );
    assert!(
        run.checkpoints.iter().any(|c| (20..=50).contains(&c.tick)),
        "checkpoints mid-detour"
    );
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn unreachable_goal_waits_without_progress() {
    let mut w = World::new(&["########", "#..#...#", "#..#...#", "########"]);
    let miner = w.spawn_with(0, "miner", 1, 1, attack_move(6, 2));
    let run = w.run();
    assert_eq!(run.result.outcome.stop_reason, StopReason::Inactivity);
    assert_eq!(run.result.outcome.terminal_state_tick, w.config.stall_ticks);
    assert_eq!(entity(&run.result.final_state, &miner).tile, tile(1, 1));
}

#[test]
fn tread_vehicles_alternate_axes_without_diagonal_steps() {
    for kind in ["tank", "artillery", "miner"] {
        for (dx, dy) in [(1i32, 1i32), (1, -1), (-1, 1), (-1, -1)] {
            let mut w = World::new(&["......."; 7]);
            let goal = tile((3 + 2 * dx) as u16, (3 + 2 * dy) as u16);
            let id = w.spawn_with(0, kind, 3, 3, attack_move(goal.x, goal.y));
            assert_eq!(
                w.content
                    .types
                    .iter()
                    .find(|t| t.key == kind)
                    .unwrap()
                    .movement
                    .as_ref()
                    .unwrap()
                    .neighbors,
                Neighbors::Four
            );
            let states = trace(&w);
            assert_motion_invariants(&w, &states);
            let mut path: Vec<Tile> = states.iter().map(|s| entity(s, &id).tile).collect();
            path.dedup();
            assert_eq!(
                path,
                vec![
                    tile(3, 3),
                    tile((3 + dx) as u16, 3),
                    tile((3 + dx) as u16, (3 + dy) as u16),
                    tile(goal.x, (3 + dy) as u16),
                    goal
                ],
                "{kind} {dx},{dy}"
            );
            assert_checkpoint_equivalence(&w, &w.run());
        }
    }
}

#[test]
fn rules_seven_keeps_fixed_tread_axis_precedence() {
    let mut w = World::new(&["......."; 7]);
    let id = w.spawn_with(0, "tank", 3, 3, attack_move(5, 5));
    let mut request = w.request();
    request.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 7".into();
    let mut sim = Sim::new(&request).unwrap();
    let mut path = vec![entity(&sim.state, &id).tile];
    for _ in 0..request.end_tick_exclusive {
        sim.step(&mut |_| {}).unwrap();
        let current = entity(&sim.state, &id).tile;
        if Some(&current) != path.last() {
            path.push(current);
        }
        if current == tile(5, 5) {
            break;
        }
    }
    assert_eq!(
        path,
        vec![tile(3, 3), tile(4, 3), tile(5, 3), tile(5, 4), tile(5, 5)]
    );
}

#[test]
fn stock_public_rules_eight_keeps_fixed_axes_and_artillery_holds() {
    let mut world = World::new(&["...................."; 9]);
    world.content = load_content(include_str!(
        "../../../fixtures/compatibility/public-rules8-content.json"
    ))
    .unwrap();
    let tank = world.spawn_with(0, "tank", 3, 3, attack_move(5, 5));
    let gun = world.spawn(0, "artillery", 10, 5);
    world.entity_mut(&gun).next_action_tick = 20;
    world.spawn(1, "miner", 15, 5);

    let mut request = world.request();
    request.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 8".into();
    request.fingerprint.content_hash = atemporal_content::content_hash(&world.content).unwrap();
    assert!(is_public_rules8(&request.fingerprint));
    let mut sim = Sim::new(&request).unwrap();
    sim.set_artillery_retreat_diagnostics(true);
    for _ in 0..5 {
        sim.step(&mut |_| {}).unwrap();
    }
    assert_eq!(entity(&sim.state, &tank).tile, tile(5, 3));
    assert_eq!(entity(&sim.state, &gun).tile, tile(10, 5));
    assert!(sim.last_artillery_retreats().is_empty());

    request.content.types[0].matter_cost += 1.0;
    assert!(
        Sim::new(&request).is_err(),
        "a claimed stock hash is checked"
    );
    request.fingerprint.content_hash = atemporal_content::content_hash(&request.content).unwrap();
    assert!(Sim::new(&request).is_err(), "custom rules 8 is ambiguous");
}

#[test]
fn stock_public_rules_eight_uses_wall_aware_indirect_targeting() {
    let content = load_content(include_str!(
        "../../../fixtures/compatibility/public-rules8-content.json"
    ))
    .unwrap();
    let mut world = World::with_content(&["....#......."; 7], content);
    world.spawn(0, "artillery", 2, 3);
    let target = world.spawn(1, "miner", 6, 3);
    let mut request = world.request();
    request.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 8".into();
    request.fingerprint.content_hash = atemporal_content::content_hash(&world.content).unwrap();
    let mut sim = Sim::new(&request).unwrap();
    sim.step(&mut |_| {}).unwrap();
    assert_eq!(entity(&sim.state, &target).hp, world.def("miner").max_hp);

    request.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 7".into();
    let mut legacy_vision = Sim::new(&request).unwrap();
    legacy_vision.step(&mut |_| {}).unwrap();
    assert!(entity(&legacy_vision.state, &target).hp < world.def("miner").max_hp);
}

#[test]
fn armed_attack_movers_breach_an_enemy_wall_then_continue_to_destination() {
    let mut w = World::new(&["###############", "#.............#", "###############"]);
    let grunt = w.spawn_with(0, "grunt", 1, 1, attack_move(13, 1));
    let wall = w.spawn(1, "wall", 11, 1);
    w.entity_mut(&wall).hp = 1.0;
    let run = w.run();
    assert!(run.destroyed(&wall).is_some());
    assert_eq!(entity(&run.result.final_state, &grunt).tile, tile(13, 1));
    assert_checkpoint_equivalence(&w, &run);

    let mut old = w.request();
    old.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 12".into();
    let mut old_sim = Sim::new(&old).unwrap();
    old_sim.step(&mut |_| {}).unwrap();
    assert_eq!(entity(&old_sim.state, &grunt).tile, tile(1, 1));

    w.entity_mut(&wall).owner = 0;
    assert_eq!(entity(&w.at(10), &grunt).tile, tile(1, 1));
}

#[test]
fn attack_movers_strike_an_intervening_building_while_pursuing_a_unit() {
    let mut w = World::new(&["#########", "#.......#", "#########"]);
    let grinder = w.spawn_with(0, "grinder", 2, 1, attack_move(7, 1));
    let factory = w.spawn(1, "factory", 3, 1);
    let opponent = w.spawn(1, "miner", 5, 1);
    w.entity_mut(&grinder).engaged_target = Some(opponent);
    w.entity_mut(&factory).hp = 1.0;
    assert_eq!(w.run().destroyed(&factory), Some(0));
}
