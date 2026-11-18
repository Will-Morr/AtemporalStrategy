mod common;

use atemporal_sim::{player_timeline::build_verified, *};
use common::{World, tile};
use std::sync::atomic::AtomicBool;

#[test]
fn exact_satellite_sighting_survives_between_samples_until_tile_is_cleared() {
    let mut world = World::new(&["..........", "..........", ".........."]);
    world.config.objective = Objective::Timed {
        lock_ticks_per_round: 1,
    };
    world.config.snapshot_interval = 4;
    let site = world.site(1, "wall", 8, 1, 1.0);
    let satellite = world
        .content
        .types
        .iter_mut()
        .find(|d| d.key == "satellite")
        .unwrap()
        .missile
        .as_mut()
        .unwrap();
    satellite.radius = 0.4;
    let silo_id = identity::genesis(0, 0).unwrap();
    world.state.missiles = [(7, 9, 2), (3, 8, 5)]
        .into_iter()
        .enumerate()
        .map(|(sequence, (origin, target, impact_tick))| MissileFlight {
            silo_id: silo_id.clone(),
            sequence: sequence as u32,
            owner: 0,
            type_key: "satellite".into(),
            origin: tile(origin, 1),
            target: tile(target, 1),
            launch_tick: 0,
            impact_tick,
        })
        .collect();
    world.turn(
        1,
        1,
        2,
        vec![Command::CancelBlueprints {
            blueprint_ids: vec![site.clone()],
        }],
    );
    world.turn(1, 0, 0, vec![]);
    let mut request = world.request();
    request.end_tick_exclusive = 7;
    request.minimum_end_tick = 7;
    let cancel = AtomicBool::new(false);
    let result = run(&request, &cancel, &mut |_| {}).unwrap();
    let timelines = build_verified(&request, &[0, 1], &cancel, 7, &result.final_hash).unwrap();
    let timeline = &timelines[&0];

    assert_eq!(timeline.frames().count(), 8);
    assert_eq!(
        timeline.public_stats.keys().copied().collect::<Vec<_>>(),
        [0, 4, 7]
    );
    assert!(timeline.public_stats[&4].players[1].entity_count.is_none());
    assert!(timeline.public_stats[&7].players[1].entity_count.is_some());
    assert!(timeline.last_seen(0, &site).is_none());
    assert!(
        timeline
            .frame(1)
            .unwrap()
            .visible_entities
            .iter()
            .any(|e| e.id == site)
    );
    assert!(timeline.frame(2).unwrap().visible_entities.is_empty());
    assert_eq!(timeline.remembered_buildings(4).next().unwrap().tick, 1);
    assert_eq!(timeline.last_seen(7, &site).unwrap().tick, 1);
    assert!(timeline.remembered_buildings(6).next().is_none());
    assert!(!timeline.frame(3).unwrap().events.iter().any(|e| {
        matches!(&e.event, PresentationEvent::Destroyed { entity_id, .. } if *entity_id == site)
    }));
    assert!(timeline.frame(3).unwrap().command_outcomes.is_empty());
    assert_eq!(timelines[&1].frame(3).unwrap().command_outcomes.len(), 1);
    assert_eq!(timeline.turns.len(), 1);
    assert_eq!(timeline.turns[0].player, 0);
    assert!(build_verified(&request, &[0], &cancel, 7, "wrong digest").is_err());
}

#[test]
fn projected_flight_and_own_state_omit_hidden_identifiers_and_combat_totals() {
    let mut world = World::new(&["............", "............", "............"]);
    let observer = world.spawn(0, "miner", 5, 1);
    let hidden = world.spawn(1, "wall", 11, 1);
    let miner = world
        .content
        .types
        .iter_mut()
        .find(|def| def.key == "miner")
        .unwrap();
    miner.vision = 1.0;
    world.entity_mut(&observer).engaged_target = Some(hidden.clone());
    world.state.players[0].counters.damage_dealt = 777.0;
    world.state.tick = 5;
    world.state.missiles.push(MissileFlight {
        silo_id: hidden.clone(),
        sequence: 50,
        owner: 1,
        type_key: "satellite".into(),
        origin: tile(0, 1),
        target: tile(10, 1),
        launch_tick: 0,
        impact_tick: 10,
    });

    let frame = player_timeline::project(&world.state, 0, &world.config, &world.content).unwrap();
    assert!(frame.own_entities[0].engaged_target.is_none());
    assert!(frame.visible_entities.is_empty());
    assert!(frame.missiles.is_empty());
    let [flight] = &*frame.hostile_missiles else {
        panic!("visible hostile flight expected")
    };
    assert_eq!((flight.x, flight.y), (5.0, 1.0));

    let frame = serde_json::to_value(frame).unwrap();
    assert!(frame["player"].get("counters").is_none());
    let flight = &frame["hostile_missiles"][0];
    for key in [
        "silo_id",
        "target",
        "origin",
        "impact_tick",
        "launch_tick",
        "sequence",
    ] {
        assert!(flight.get(key).is_none(), "{key}");
    }
}
