mod common;

use atemporal_sim::{player_timeline::project, *};
use common::{World, tile};
use serde_json::Value;

fn uint(value: &Value) -> u64 {
    value.as_u64().unwrap()
}

fn radius(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

fn probe(key: &Value) -> Tile {
    let (x, y) = key.as_str().unwrap().split_once(',').unwrap();
    tile(x.parse().unwrap(), y.parse().unwrap())
}

#[test]
fn player_visibility_matches_browser_fixture() {
    // scripts/check-player-visibility.mjs uses these same exact snapshots and expectations.
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/gameplay/player-visibility.json"
    ))
    .unwrap();
    let rows: Vec<_> = fixture["terrain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_str().unwrap())
        .collect();
    let mut world = World::new(&rows);
    world.config.player_count = 4;
    for player_id in 2..4 {
        let mut player = world.state.players[0].clone();
        player.player_id = player_id;
        world.state.players.push(player);
    }
    world.config.multiplayer = serde_json::from_value(fixture["multiplayer"].clone()).unwrap();
    for def in &mut world.content.types {
        if let Some(value) = fixture["observer_radii"].get(&def.key) {
            if def.key == "satellite" {
                def.missile.as_mut().unwrap().radius = radius(value);
            } else {
                def.vision = radius(value);
            }
        }
    }
    for (index, observer) in fixture["entities"].as_array().unwrap().iter().enumerate() {
        let key = observer["type_key"].as_str().unwrap();
        let owner = uint(&observer["owner"]) as u8;
        world.spawn(
            0,
            key,
            uint(&observer["x"]) as u16,
            uint(&observer["y"]) as u16,
        );
        let entity = world.state.entities.last_mut().unwrap();
        entity.owner = owner;
        entity.id = identity::genesis(owner, index as u32).unwrap();
        entity.lifecycle = serde_json::from_value(observer["lifecycle"].clone()).unwrap();
    }
    let enemy_silo = world.state.entities[4].id.clone();
    let allied_site = world.state.entities[2].id.clone();
    let hidden_enemy = world.state.entities[5].id.clone();
    let own_miner = world.state.entities[0].id.clone();
    for (index, value) in fixture["missiles"].as_array().unwrap().iter().enumerate() {
        let mut flight = value.clone();
        flight["silo_id"] = serde_json::to_value(&enemy_silo).unwrap();
        flight["sequence"] = Value::from(index);
        world
            .state
            .missiles
            .push(serde_json::from_value(flight).unwrap());
    }
    world.state.recon = serde_json::from_value(fixture["recon"].clone()).unwrap();
    for frame in fixture["frames"].as_array().unwrap() {
        world.state.tick = uint(&frame["tick"]) as Tick;
        let observed = project(&world.state, 0, &world.config, &world.content).unwrap();
        for (key, expected) in [("visible", true), ("hidden", false)] {
            for value in frame[key].as_array().unwrap() {
                assert_eq!(
                    observed.sees(probe(value)),
                    expected,
                    "tick {} {key} tile {value}",
                    world.state.tick
                );
            }
        }
        assert!(
            !observed
                .visible_entities
                .iter()
                .any(|e| e.id == allied_site)
        );
        assert!(
            !observed
                .visible_entities
                .iter()
                .any(|e| e.id == hidden_enemy)
        );
        assert_eq!(observed.own_entities.len(), 1);
        assert_eq!(observed.own_entities[0].id, own_miner);
        assert_eq!(
            observed.visible_entities.iter().any(|e| e.id == enemy_silo),
            matches!(world.state.tick, 2 | 4)
        );
    }
}
