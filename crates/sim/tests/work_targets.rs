//! Work acquisition uses legal walking distance, including blocked and unfunded sites.
mod common;
use atemporal_sim::*;
use common::*;

const DETOUR: [&str; 8] = [
    "...#......",
    "...#......",
    "...#......",
    "...#......",
    "...#......",
    "...#......",
    "..........",
    "..........",
];

#[test]
fn miners_choose_short_walk_instead_of_nearby_ore_across_wall() {
    for neighbors in [Neighbors::Four, Neighbors::Eight] {
        let mut w = World::new(&DETOUR);
        w.content
            .types
            .iter_mut()
            .find(|t| t.key == "miner")
            .unwrap()
            .movement
            .as_mut()
            .unwrap()
            .neighbors = neighbors;
        w.state.ore[14] = 100.0; // (4,1), close in space but around the wall.
        w.state.ore[42] = 100.0; // (2,4), three legal steps away.
        let miner = w.spawn_with(
            0,
            "miner",
            2,
            1,
            Order::Mine {
                area: rect(0, 0, 9, 7),
            },
        );
        let first = w.at(1);
        assert_eq!(
            entity(&first, &miner).resolved_destination,
            Some(tile(2, 4))
        );
        let later = w.at(20);
        assert!(later.ore[42] < 100.0);
        assert_eq!(later.ore[14], 100.0);
    }
}

#[test]
fn miners_skip_unreachable_ore_and_claim_distinct_reachable_tiles() {
    let mut w = World::new(&["...#......"; 8]);
    w.state.ore[14] = 100.0;
    w.state.ore[42] = 100.0;
    w.state.ore[51] = 100.0;
    let a = w.spawn_with(
        0,
        "miner",
        2,
        1,
        Order::Mine {
            area: rect(0, 0, 9, 7),
        },
    );
    let b = w.spawn_with(
        0,
        "miner",
        1,
        1,
        Order::Mine {
            area: rect(0, 0, 9, 7),
        },
    );
    // A stale destination must be released, including after a topology change.
    w.entity_mut(&a).resolved_destination = Some(tile(4, 1));
    let first = w.at(1);
    assert_eq!(entity(&first, &a).resolved_destination, Some(tile(2, 4)));
    assert_eq!(entity(&first, &b).resolved_destination, Some(tile(1, 5)));
    let later = w.at(30);
    assert!(later.ore[42] < 100.0 && later.ore[51] < 100.0);
    assert_eq!(later.ore[14], 100.0);
}

#[test]
fn a_newborn_miner_can_work_ore_claimed_from_afar_by_an_enemy() {
    let mut w = World::new(&["............"; 6]);
    let deposit = tile(4, 2);
    w.ore(deposit, 100.0);
    let order = Order::Mine {
        area: rect(4, 2, 4, 2),
    };
    let factory = w.spawn(0, "factory", 1, 2);
    w.bank(0, 30.0);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::SetStoredOrder {
                factories: vec![factory.clone()],
                order: order.clone(),
            },
            Command::EditProduction {
                factories: vec![factory],
                edit: ProductionEdit::Append {
                    items: vec!["miner".into()],
                },
            },
        ],
    );
    let enemy = w.spawn_with(1, "miner", 11, 2, order);
    let remote = w.entity_mut(&enemy);
    remote.resolved_destination = Some(deposit);
    remote.next_move_tick = 200;

    let mut previous = w.request();
    previous.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 15".into();
    let old = reconstruct(&previous, 35).unwrap();
    let newborn = old
        .entities
        .iter()
        .find(|e| e.owner == 0 && e.type_key == "miner")
        .unwrap();
    assert_eq!(newborn.tile, tile(2, 2));
    assert_eq!(newborn.resolved_destination, None);
    let now = w.at(35);
    assert!(now.players[0].counters.mined > 0.0);
    assert_eq!(entity(&now, &newborn.id).tile, deposit);
    assert_eq!(entity(&now, &enemy).tile, tile(11, 2));
}

#[test]
fn a_constructor_group_uses_available_work_positions_at_multiple_sites() {
    let mut w = World::new(&[
        "###############",
        "#.............#",
        "#####.###.#####",
        "###############",
    ]);
    let builders: Vec<_> = [1, 2, 3, 4, 5, 6]
        .into_iter()
        .map(|x| {
            w.spawn_with(
                0,
                "constructor",
                x,
                1,
                Order::Construct {
                    area: rect(5, 2, 9, 2),
                },
            )
        })
        .collect();
    let near = w.site(0, "turret", 5, 2, 1.0);
    let far = w.site(0, "turret", 9, 2, 1.0);
    let off = w.spawn_with(
        0,
        "constructor",
        13,
        1,
        Order::Construct {
            area: rect(5, 2, 9, 2),
        },
    );
    w.entity_mut(&off).priority = Priority::Off;
    w.bank(0, 1000.0);
    let mut previous = w.request();
    previous.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 15".into();
    let old = reconstruct(&previous, 1).unwrap();
    assert!(
        builders
            .iter()
            .all(|id| entity(&old, id).resolved_destination != Some(tile(9, 2)))
    );

    let started = w.at(1);
    assert_eq!(entity(&started, &off).resolved_destination, None);
    assert_eq!(
        builders
            .iter()
            .filter(|id| entity(&started, id).resolved_destination == Some(tile(9, 2)))
            .count(),
        3
    );
    let complete = w.at(60);
    assert_eq!(entity(&complete, &near).lifecycle, Lifecycle::Complete);
    assert_eq!(entity(&complete, &far).lifecycle, Lifecycle::Complete);
    assert_checkpoint_equivalence(&w, &w.run());
}

#[test]
fn traffic_paths_around_a_working_miner_when_there_is_room() {
    let mut w = World::new(&["........"; 6]);
    let deposit = tile(3, 2);
    let worker = w.spawn_with(
        0,
        "miner",
        3,
        2,
        Order::Mine {
            area: rect(3, 2, 3, 2),
        },
    );
    let moving = w.spawn_with(0, "miner", 2, 2, attack_move(6, 2));
    w.ore(deposit, 1000.0);
    let mut previous = w.request();
    previous.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 15".into();
    assert_ne!(
        entity(&reconstruct(&previous, 1).unwrap(), &worker).tile,
        deposit
    );

    let through = w.at(20);
    assert_eq!(entity(&through, &moving).tile, tile(6, 2));
    assert_eq!(entity(&through, &worker).tile, deposit);
    assert_eq!(through.players[0].counters.mined, 40.0);
    assert_checkpoint_equivalence(&w, &w.run());
}

#[test]
fn traffic_leaves_a_constructor_at_work_when_another_route_exists() {
    let mut w = World::new(&["........"; 6]);
    let work_position = tile(3, 2);
    let worker = w.spawn_with(
        0,
        "constructor",
        3,
        2,
        Order::Construct {
            area: rect(3, 1, 3, 1),
        },
    );
    let site = w.site(0, "turret", 3, 1, 1.0);
    let moving = w.spawn_with(0, "miner", 2, 2, attack_move(6, 2));
    w.bank(0, 40.0);
    let mut previous = w.request();
    previous.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 15".into();
    assert_ne!(
        entity(&reconstruct(&previous, 1).unwrap(), &worker).tile,
        work_position
    );

    let through = w.at(20);
    assert_eq!(entity(&through, &moving).tile, tile(6, 2));
    assert_eq!(entity(&through, &worker).tile, work_position);
    assert_eq!(entity(&through, &site).paid_matter, 41.0);
}

#[test]
fn traffic_can_pass_a_worker_when_the_only_route_is_through_it() {
    let mut w = World::new(&["########", "#......#", "########"]);
    let deposit = tile(3, 1);
    let worker = w.spawn_with(
        0,
        "miner",
        3,
        1,
        Order::Mine {
            area: rect(3, 1, 3, 1),
        },
    );
    let moving = w.spawn_with(0, "miner", 2, 1, attack_move(6, 1));
    w.ore(deposit, 1000.0);

    let through = w.at(20);
    assert_eq!(entity(&through, &moving).tile, tile(6, 1));
    assert_eq!(entity(&through, &worker).tile, deposit);
    assert!(through.players[0].counters.mined > 0.0);
}

#[test]
fn constructors_choose_shortest_working_route_for_funded_and_unfunded_sites() {
    for funded in [false, true] {
        for neighbors in [Neighbors::Four, Neighbors::Eight] {
            let mut w = World::new(&DETOUR);
            w.content
                .types
                .iter_mut()
                .find(|t| t.key == "constructor")
                .unwrap()
                .movement
                .as_mut()
                .unwrap()
                .neighbors = neighbors;
            w.state.players[0].bank = 100.0;
            let builder = w.spawn_with(
                0,
                "constructor",
                2,
                1,
                Order::Construct {
                    area: rect(0, 0, 9, 7),
                },
            );
            if funded {
                w.site(0, "factory", 4, 1, 1.0);
                w.site(0, "factory", 2, 4, 1.0);
            } else {
                w.turn(
                    1,
                    0,
                    0,
                    vec![Command::PlaceBlueprints {
                        type_key: "factory".into(),
                        tiles: vec![tile(4, 1), tile(2, 4)],
                        priority: Priority::Medium,
                        output_directions: None,
                    }],
                );
            }
            let first = w.at(1);
            assert_eq!(
                entity(&first, &builder).resolved_destination,
                Some(tile(2, 4))
            );
            let later = w.at(8);
            let near = later
                .entities
                .iter()
                .find(|e| e.tile == tile(2, 4))
                .expect("near site funded first");
            assert!(near.paid_matter > 1.0);
            let far = later.entities.iter().find(|e| e.tile == tile(4, 1));
            assert_eq!(
                far.map_or(0.0, |e| e.paid_matter),
                if funded { 1.0 } else { 0.0 }
            );
        }
    }
}

#[test]
fn grinders_trade_range_for_speed_and_a_small_health_advantage() {
    let w = World::new(&DETOUR);
    let tank = w.def("tank");
    let grinder = w.def("grinder");
    assert_eq!(grinder.matter_cost - tank.matter_cost, 10.0);
    assert!(grinder.max_hp > tank.max_hp && grinder.max_hp <= tank.max_hp * 1.2);
    let (g, t) = (
        grinder.weapon.as_ref().unwrap(),
        tank.weapon.as_ref().unwrap(),
    );
    assert_eq!(
        g.damage / f64::from(g.cooldown),
        t.damage / f64::from(t.cooldown)
    );
    assert!(g.range <= 1.415 && g.range < t.range);
    assert!(grinder.movement.as_ref().unwrap().cooldown < tank.movement.as_ref().unwrap().cooldown);
}

#[test]
fn work_routes_replay_identically_with_cold_or_evicting_fields() {
    let mut w = World::new(&DETOUR);
    w.config.max_tick = 60;
    w.state.ore[42] = 100.0;
    w.spawn_with(
        0,
        "miner",
        2,
        1,
        Order::Mine {
            area: rect(0, 0, 9, 7),
        },
    );
    w.spawn_with(
        0,
        "constructor",
        1,
        1,
        Order::Construct {
            area: rect(0, 0, 9, 7),
        },
    );
    w.site(0, "factory", 4, 1, 1.0);
    w.site(0, "factory", 1, 4, 1.0);
    w.bank(0, 100.0);
    let run = w.run();
    assert_checkpoint_equivalence(&w, &run);
    let request = w.request();
    let mut evicted = Sim::new(&request).unwrap();
    evicted.set_field_cache_capacity(1);
    let mut sink = |_: Output| {};
    while evicted.state.tick < run.result.outcome.terminal_state_tick {
        evicted.step(&mut sink).unwrap();
    }
    assert_eq!(evicted.hash_state().unwrap(), run.result.final_hash);
}
