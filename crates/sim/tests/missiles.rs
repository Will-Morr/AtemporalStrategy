mod common;
use atemporal_sim::*;
use common::*;

fn world() -> World {
    let mut w = World::new(&["................................................"; 24]);
    for t in &mut w.content.types {
        t.weapon = None;
    }
    w.config.max_tick = 300;
    w.bank(0, 2000.0);
    w
}
fn silo(w: &World, id: &EntityId, tick: Tick) -> SiloState {
    entity(&w.at(tick), id)
        .production
        .as_ref()
        .unwrap()
        .silo
        .clone()
        .unwrap()
}
fn stock(w: &mut World, id: &EntityId, key: &str, count: u32) {
    w.entity_mut(id)
        .production
        .as_mut()
        .unwrap()
        .silo
        .as_mut()
        .unwrap()
        .inventory
        .push(MissileStock {
            type_key: key.into(),
            count,
        });
}
fn launch(w: &mut World, id: &EntityId, key: &str, x: u16, y: u16) {
    w.turn(
        1,
        0,
        0,
        vec![Command::SetSiloPlan {
            silos: vec![id.clone()],
            plan: SiloPlan {
                automatic: false,
                launches: vec![MissileLaunch {
                    type_key: key.into(),
                    target: tile(x, y),
                }],
            },
        }],
    );
}

#[test]
fn missiles_build_to_inventory_without_an_output_and_loop_per_item() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    w.spawn(0, "wall", 3, 10);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::SetQueueLoop {
                factories: vec![id.clone()],
                enabled: true,
            },
            Command::EditProduction {
                factories: vec![id.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["satellite".into()],
                },
            },
        ],
    );
    let state = w.at(30);
    let inventory = &entity(&state, &id)
        .production
        .as_ref()
        .unwrap()
        .silo
        .as_ref()
        .unwrap()
        .inventory;
    assert_eq!(
        inventory,
        &vec![MissileStock {
            type_key: "satellite".into(),
            count: 2
        }]
    );
    assert!(state.missiles.is_empty());
    assert_eq!(state.players[0].bank, 1940.0);
    assert!(!state.entities.iter().any(|e| e.type_key == "satellite"));
    w.entity_mut(&id).priority = Priority::Off;
    assert!(silo(&w, &id, 30).inventory.is_empty());
}
#[test]
fn queued_launch_waits_for_production_then_cluster_hits_the_radius_for_40_percent_of_tank_hp() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    let tank = w.spawn(1, "tank", 20, 10);
    let edge = w.spawn(1, "tank", 27, 10);
    let outside = w.spawn(1, "tank", 28, 10);
    let friendly = w.spawn(0, "constructor", 20, 12);
    w.turn(
        1,
        0,
        0,
        vec![
            Command::EditProduction {
                factories: vec![id.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["cluster".into()],
                },
            },
            Command::SetSiloPlan {
                silos: vec![id.clone()],
                plan: SiloPlan {
                    automatic: false,
                    launches: vec![MissileLaunch {
                        type_key: "cluster".into(),
                        target: tile(20, 10),
                    }],
                },
            },
        ],
    );
    assert_eq!(silo(&w, &id, 37).plan.launches.len(), 1);
    assert!(w.at(37).missiles.is_empty());
    let flying = w.at(38);
    assert_eq!(flying.missiles.len(), 1);
    assert_eq!(flying.missiles[0].launch_tick, 37);
    assert_eq!(flying.missiles[0].impact_tick, 43);
    let hit = w.at(44);
    assert_eq!(entity(&hit, &tank).hp, 150.0);
    assert_eq!(entity(&hit, &edge).hp, 150.0);
    assert_eq!(entity(&hit, &outside).hp, 250.0);
    assert!(!hit.entities.iter().any(|e| e.id == friendly));
    assert!(hit.missiles.is_empty());
    assert!(silo(&w, &id, 44).plan.launches.is_empty());
    let run = w.run();
    assert_checkpoint_equivalence(&w, &run);
}
#[test]
fn tactical_nuke_annihilates_units_and_buildings_including_friendlies() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "tac_nuke", 1);
    let tank = w.spawn(1, "tank", 20, 10);
    let wall = w.spawn(1, "wall", 24, 10);
    let ally = w.spawn(0, "factory", 20, 14);
    let outside = w.spawn(1, "tank", 25, 10);
    launch(&mut w, &id, "tac_nuke", 20, 10);
    let hit = w.at(7);
    for dead in [tank, wall, ally] {
        assert!(!hit.entities.iter().any(|e| e.id == dead));
    }
    assert_eq!(entity(&hit, &outside).hp, 250.0);
}
#[test]
fn satellite_moves_across_the_map_and_holds_temporary_destination_vision() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "satellite", 1);
    launch(&mut w, &id, "satellite", 44, 10);
    let flying = w.at(7);
    let flight = &flying.missiles[0];
    assert_eq!(flight.impact_tick, 14);
    assert_eq!(
        atemporal_contracts::missiles::flight_position(flight, 7.0),
        (23.0, 10.0)
    );
    let landed = w.at(15);
    assert!(landed.missiles.is_empty());
    assert_eq!(landed.recon[0].center, tile(44, 10));
    assert_eq!(landed.recon[0].radius, 9.0);
    assert_eq!(landed.recon[0].expires_at, 115);
    assert!(w.at(116).recon.is_empty());
    let run = w.run();
    assert_checkpoint_equivalence(&w, &run);
}
#[test]
fn launch_has_no_range_limit_and_flight_is_capped_at_thirty_ticks() {
    let row = ".".repeat(120);
    let rows = vec![row.as_str(); 24];
    let mut w = World::new(&rows);
    w.config.max_tick = 80;
    let id = w.spawn(0, "silo", 1, 10);
    stock(&mut w, &id, "satellite", 1);
    launch(&mut w, &id, "satellite", 118, 10);
    let first = w.at(1);
    assert_eq!(first.missiles[0].impact_tick, 30);
    assert!(!w.at(31).recon.is_empty());
}
#[test]
fn silo_death_destroys_stock_but_already_launched_missiles_land() {
    let mut w = world();
    w.config.stop_when_decided = true;
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "cluster", 3);
    launch(&mut w, &id, "cluster", 20, 10);
    w.content
        .types
        .iter_mut()
        .find(|t| t.key == "grunt")
        .unwrap()
        .weapon = Some(Weapon {
        range: 2.0,
        damage: 1000.0,
        cooldown: 1,
        indirect: false,
        visual_style: VisualStyle::Direct,
    });
    w.spawn(1, "grunt", 2, 11);
    let target = w.spawn(1, "tank", 20, 10);
    w.spawn(1, "factory", 40, 20);
    let first = w.at(1);
    assert!(!first.entities.iter().any(|e| e.id == id));
    assert_eq!(first.missiles.len(), 1);
    assert_eq!(first.players[0].counters.lost_invested_matter, 300.0); // 150 silo + two 75-matter missiles.
    let run = w.run();
    assert!(run.result.outcome.terminal_state_tick >= 7);
    assert_eq!(entity(&run.result.final_state, &target).hp, 150.0);
}
#[test]
fn automatic_fire_uses_shared_vision_and_manual_queue_can_be_cancelled() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "cluster", 2);
    w.spawn(1, "tank", 20, 10);
    let plan = SiloPlan {
        automatic: true,
        launches: vec![],
    };
    w.turn(
        1,
        0,
        0,
        vec![Command::SetSiloPlan {
            silos: vec![id.clone()],
            plan,
        }],
    );
    assert!(
        w.at(1).missiles.is_empty(),
        "automatic fire cannot acquire hidden enemies"
    );
    w.spawn(0, "scout", 18, 10);
    assert_eq!(w.at(1).missiles.len(), 1);
    w.turn(
        2,
        0,
        0,
        vec![Command::SetSiloPlan {
            silos: vec![id.clone()],
            plan: SiloPlan::default(),
        }],
    );
    assert!(w.at(1).missiles.is_empty());
    assert_eq!(silo(&w, &id, 1).inventory[0].count, 2);
}
#[test]
fn blueprint_silo_keeps_its_production_and_launch_plan_through_construction() {
    let mut w = world();
    w.config.max_tick = 220;
    w.spawn_with(
        0,
        "constructor",
        2,
        10,
        Order::Construct {
            area: rect(3, 10, 3, 10),
        },
    );
    let id = identity::blueprint(&identity::command_id(1, 0, 0), 0).unwrap();
    w.turn(
        1,
        0,
        0,
        vec![
            Command::PlaceBlueprints {
                type_key: "silo".into(),
                tiles: vec![tile(3, 10)],
                priority: Priority::High,
                output_directions: None,
            },
            Command::ConfigureBlueprints {
                blueprint_ids: vec![id.clone()],
                settings: BlueprintSettings {
                    queue: vec!["satellite".into()],
                    queue_loop_flags: vec![false],
                    order: Order::Idle {},
                    priority: Priority::High,
                    loop_enabled: false,
                    silo_plan: Some(SiloPlan {
                        automatic: false,
                        launches: vec![MissileLaunch {
                            type_key: "satellite".into(),
                            target: tile(40, 10),
                        }],
                    }),
                },
            },
        ],
    );
    assert_eq!(silo(&w, &id, 10).plan.launches.len(), 1);
    let run = w.run();
    assert!(
        run.events
            .iter()
            .any(|e| matches!(e.event, PresentationEvent::MissileLaunch { .. }))
    );
    assert_checkpoint_equivalence(&w, &run);
}

#[test]
fn invalid_launch_plans_and_ownership_do_not_change_stock() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "cluster", 1);
    let wrong = SiloPlan {
        automatic: false,
        launches: vec![MissileLaunch {
            type_key: "tank".into(),
            target: tile(20, 10),
        }],
    };
    assert!(validate_silo_plan(w.def("silo"), &wrong, 48, 24).is_err());
    assert!(validate_silo_plan(w.def("factory"), &SiloPlan::default(), 48, 24).is_err());
    let outside = SiloPlan {
        automatic: false,
        launches: vec![MissileLaunch {
            type_key: "cluster".into(),
            target: tile(48, 10),
        }],
    };
    assert!(validate_silo_plan(w.def("silo"), &outside, 48, 24).is_err());
    let ids = w.turn(
        1,
        0,
        0,
        vec![Command::SetSiloPlan {
            silos: vec![id.clone()],
            plan: wrong,
        }],
    );
    let enemy = w.turn(
        1,
        1,
        0,
        vec![Command::SetSiloPlan {
            silos: vec![id.clone()],
            plan: SiloPlan {
                automatic: true,
                launches: vec![],
            },
        }],
    );
    let run = w.run();
    assert_eq!(
        run.outcome(&ids[0]).skipped[0].reason,
        SkipReason::Incompatible
    );
    assert_eq!(
        run.outcome(&enemy[0]).skipped[0].reason,
        SkipReason::WrongOwner
    );
    assert_eq!(silo(&w, &id, 1).inventory[0].count, 1);
    assert!(!silo(&w, &id, 1).plan.automatic);
}

#[test]
fn flight_and_inventory_replay_with_an_evicting_cache_matches_the_full_run() {
    let mut w = world();
    w.config.max_tick = 100;
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "satellite", 1);
    launch(&mut w, &id, "satellite", 44, 10);
    let request = w.request();
    let full = w.run();
    let mut sim = Sim::new(&request).unwrap();
    sim.set_field_cache_capacity(1);
    let mut sink = |_: Output| {};
    while sim.state.tick < full.result.outcome.terminal_state_tick {
        sim.step(&mut sink).unwrap();
    }
    assert_eq!(sim.hash_state().unwrap(), full.result.final_hash);
    let mut corrupt = w.at(1);
    corrupt.missiles[0].impact_tick = corrupt.missiles[0].launch_tick + 31;
    assert!(identity::canonical_world(&corrupt).is_err());
}

#[test]
fn a_nuke_cannot_be_survived_by_simultaneous_overhealing() {
    let mut w = world();
    let id = w.spawn(0, "silo", 2, 10);
    stock(&mut w, &id, "tac_nuke", 1);
    launch(&mut w, &id, "tac_nuke", 20, 10);
    let target = w.spawn(0, "tank", 20, 10);
    w.content
        .types
        .iter_mut()
        .find(|t| t.key == "constructor")
        .unwrap()
        .healing = Some(Healing {
        range: 100.0,
        hp_per_matter: 1000.0,
        demand: 1.0,
        cooldown: 1,
    });
    for x in 5..15 {
        w.spawn_with(
            0,
            "constructor",
            x,
            1,
            Order::Support {
                target: target.clone(),
            },
        );
    }
    w.content
        .types
        .iter_mut()
        .find(|t| t.key == "grunt")
        .unwrap()
        .weapon = Some(Weapon {
        range: 2.0,
        damage: 249.0,
        cooldown: 100,
        indirect: false,
        visual_style: VisualStyle::Direct,
    });
    let attacker = w.spawn(1, "grunt", 20, 11);
    w.entity_mut(&attacker).next_action_tick = 5;
    assert_eq!(entity(&w.at(6), &target).hp, 1.0);
    assert!(!w.at(7).entities.iter().any(|e| e.id == target));
}
