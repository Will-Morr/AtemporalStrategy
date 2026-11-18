mod common;
use atemporal_sim::*;
use common::*;

fn limit_gun_vision(world: &mut World) {
    world
        .content
        .types
        .iter_mut()
        .find(|d| d.key == "artillery")
        .unwrap()
        .vision = 4.0;
}

fn next_sim(world: &World) -> Sim {
    let mut sim = Sim::new(&world.request()).unwrap();
    sim.set_artillery_retreat_diagnostics(true);
    sim.step(&mut |_| {}).unwrap();
    sim
}

#[test]
fn retreat_uses_a_strict_euclidean_threshold_and_shared_recon() {
    for (enemy, expected) in [
        (tile(13, 3), false),
        (tile(12, 6), true),
        (tile(13, 4), false),
    ] {
        let mut w = World::new(&["...................."; 11]);
        limit_gun_vision(&mut w);
        let gun = w.spawn(0, "artillery", 5, 3);
        w.entity_mut(&gun).next_action_tick = 2;
        w.spawn(1, "miner", enemy.x, enemy.y);
        assert!(next_sim(&w).last_artillery_retreats().is_empty());

        w.state.recon.push(ReconZone {
            owner: 0,
            center: enemy,
            radius: 1.0,
            starts_at: 0,
            expires_at: 20,
        });
        let sim = next_sim(&w);
        let plan = sim.last_artillery_retreats().get(&gun);
        assert_eq!(plan.is_some(), expected);
        let mut previous = w.request();
        previous.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 10".into();
        let mut old = Sim::new(&previous).unwrap();
        old.set_artillery_retreat_diagnostics(true);
        old.step(&mut |_| {}).unwrap();
        assert!(old.last_artillery_retreats().contains_key(&gun));
        if let Some(plan) = plan {
            assert_eq!(plan.threats, [enemy]);
            assert_eq!(Some(entity(&sim.state, &gun).tile), plan.next);
            assert!(plan.scores.iter().any(|score| !score.reachable));
        }
    }
}

#[test]
fn allied_spotters_share_sight_and_do_not_contribute_threat() {
    let mut w = World::new(&["...................."; 11]);
    limit_gun_vision(&mut w);
    let gun = w.spawn(0, "artillery", 5, 3);
    w.entity_mut(&gun).next_action_tick = 10;
    w.spawn(1, "scout", 12, 6);
    let enemy = w.spawn(1, "miner", 12, 5);
    let enemy_state = w.entity_mut(&enemy);
    enemy_state.id = identity::genesis(2, 0).unwrap();
    enemy_state.owner = 2;
    for player_id in 2..=3 {
        let mut player = w.state.players[1].clone();
        player.player_id = player_id;
        w.state.players.push(player);
    }
    w.config.player_count = 4;
    w.config.multiplayer = Multiplayer::Teams {
        assignments: (0..4)
            .map(|player_id| TeamAssignment {
                player_id,
                team_id: if player_id < 2 { "blue" } else { "red" }.into(),
            })
            .collect(),
    };
    let sim = next_sim(&w);
    assert_eq!(sim.last_artillery_retreats()[&gun].threats, [tile(12, 5)]);
    assert_ne!(entity(&sim.state, &gun).tile, tile(5, 3));
}

#[test]
fn ready_artillery_fires_then_retreats_without_changing_its_order() {
    let mut w = World::new(&["...................."; 11]);
    let order = attack_move(15, 5);
    let gun = w.spawn_with(0, "artillery", 7, 5, order.clone());
    let enemy = w.spawn(1, "miner", 12, 5);
    let mut sim = next_sim(&w);
    let plan = &sim.last_artillery_retreats()[&gun];
    assert!(plan.firing);
    assert!(plan.next.is_some());
    assert_eq!(entity(&sim.state, &gun).tile, tile(7, 5));
    assert!(entity(&sim.state, &enemy).hp < w.def("miner").max_hp);

    sim.step(&mut |_| {}).unwrap();
    let plan = &sim.last_artillery_retreats()[&gun];
    assert!(!plan.firing);
    let moved = entity(&sim.state, &gun);
    assert_eq!(Some(moved.tile), plan.next);
    assert_eq!(moved.action, order);
    assert_eq!(moved.tile.x.abs_diff(7) + moved.tile.y.abs_diff(5), 1);

    sim.step(&mut |_| {}).unwrap();
    assert!(sim.last_artillery_retreats()[&gun].move_cooldown);
    assert_eq!(sim.state, w.at(3), "diagnostics must not affect gameplay");
}

#[test]
fn chooses_escape_from_a_pocket_and_never_uses_hidden_positions() {
    let mut w = World::new(&[
        "#################",
        "#...............#",
        "#...............#",
        "#...............#",
        "###.#############",
        "###.....#########",
        "#################",
        "#...............#",
        "#################",
    ]);
    let gun = w.spawn(0, "artillery", 5, 5);
    w.entity_mut(&gun).next_action_tick = 10;
    w.spawn(1, "miner", 1, 1);
    w.state.recon.push(ReconZone {
        owner: 0,
        center: tile(1, 1),
        radius: 0.5,
        starts_at: 0,
        expires_at: 20,
    });
    let first = next_sim(&w);
    let before = &first.last_artillery_retreats()[&gun];
    assert_eq!(before.next, Some(tile(4, 5)));
    let corner = |at| {
        before
            .scores
            .iter()
            .find(|score| score.tile == at)
            .unwrap()
            .corner
    };
    assert!(corner(tile(7, 5)) > corner(tile(6, 2)));
    w.spawn(1, "miner", 5, 7);
    w.spawn(1, "wall", 6, 7);
    let after = next_sim(&w);
    assert_eq!(before, &after.last_artillery_retreats()[&gun]);
}

#[test]
fn rules_nine_keeps_previous_artillery_motion() {
    let mut w = World::new(&["................"; 9]);
    let gun = w.spawn(0, "artillery", 6, 4);
    w.entity_mut(&gun).next_action_tick = 10;
    w.spawn(1, "miner", 10, 4);
    assert_ne!(entity(&w.at(1), &gun).tile, tile(6, 4));
    let mut request = w.request();
    request.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 9".into();
    let result = reconstruct(&request, 1).unwrap();
    assert_eq!(entity(&result, &gun).tile, tile(6, 4));
}

#[test]
fn moving_a_visible_enemy_changes_threat_and_routes_but_not_corridor_geometry() {
    let mut w = World::new(&[
        "#####################",
        "#...................#",
        "#####################",
    ]);
    let gun = w.spawn(0, "artillery", 10, 1);
    w.entity_mut(&gun).next_action_tick = 10;
    let enemy = w.spawn(1, "miner", 11, 1);
    let before = next_sim(&w);
    let mut legacy = w.request();
    legacy.fingerprint.sim_build = "atemporal-sim 0.1.0 rules 11".into();
    let mut legacy = Sim::new(&legacy).unwrap();
    legacy.set_artillery_retreat_diagnostics(true);
    legacy.step(&mut |_| {}).unwrap();
    let old = &legacy.last_artillery_retreats()[&gun].scores;
    assert_eq!(
        old.iter().find(|s| s.tile == tile(10, 1)).unwrap().corner,
        188
    );

    w.entity_mut(&enemy).tile = tile(14, 1);
    w.spawn(0, "wall", 8, 1);
    let after = next_sim(&w);
    let a = &before.last_artillery_retreats()[&gun].scores;
    let b = &after.last_artillery_retreats()[&gun].scores;
    assert_eq!(a.len(), 19);
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!((a.tile, a.corner), (b.tile, b.corner));
    }
    let doorway = tile(11, 1);
    let a = a.iter().find(|s| s.tile == doorway).unwrap();
    let b = b.iter().find(|s| s.tile == doorway).unwrap();
    assert!(a.threat > b.threat);
    assert!(!a.reachable && b.reachable);
    assert_eq!(
        before.artillery_corner_costs(),
        after.artillery_corner_costs()
    );
    let wall_tile = tile(8, 1);
    let after = &after.last_artillery_retreats()[&gun].scores;
    assert!(
        !after
            .iter()
            .find(|s| s.tile == wall_tile)
            .unwrap()
            .reachable
    );
}

#[test]
fn terrain_potential_shades_a_wide_pocket_beyond_the_first_bend() {
    let rows: Vec<String> = (0..19)
        .map(|y| {
            (0..56)
                .map(|x| {
                    let through = (2..54).contains(&x) && (3..8).contains(&y);
                    let elbow = (5..10).contains(&x) && (3..17).contains(&y);
                    let pocket = (5..44).contains(&x) && (12..17).contains(&y);
                    if through || elbow || pocket { '.' } else { '#' }
                })
                .collect()
        })
        .collect();
    let row_refs: Vec<_> = rows.iter().map(String::as_str).collect();
    let w = World::new(&row_refs);
    let sim = Sim::new(&w.request()).unwrap();
    let scores = sim.artillery_corner_costs().unwrap();
    let score = |x: usize, y: usize| scores[y * 56 + x];
    for x in [30, 36, 41] {
        assert!(score(x, 14) > score(x, 5), "pocket and through at x={x}");
    }
    assert!(score(30, 14) < score(36, 14));
    assert!(score(36, 14) < score(41, 14));
    assert!(
        score(7, 9) < score(41, 14),
        "turn remains less constrained than pocket tip"
    );
    assert_eq!(score(30, 10), 0, "blur does not cross rock");

    let repeated = Sim::new(&w.request()).unwrap();
    assert!(std::ptr::eq(
        scores,
        repeated.artillery_corner_costs().unwrap()
    ));
}
