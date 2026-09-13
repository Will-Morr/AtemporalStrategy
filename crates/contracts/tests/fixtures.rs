use atemporal_contracts::*;
use serde::{Serialize, de::DeserializeOwned};
use std::{fs, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: serde_json::Value,
    js: Option<serde_json::Value>,
) {
    let typed: T = serde_json::from_value(value).unwrap();
    let restored: T = serde_json::from_slice(&serde_json::to_vec(&typed).unwrap()).unwrap();
    assert_eq!(typed, restored);
    if let Some(js) = js {
        let restored: T = serde_json::from_value(js).unwrap();
        assert_eq!(
            typed, restored,
            "JavaScript roundtrip changed a typed value"
        );
    }
}
#[test]
fn shared_fixtures_roundtrip_through_rust_and_optional_javascript_output() {
    let root = root();
    let manifest: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(root.join("fixtures/manifest.json")).unwrap()).unwrap();
    for (index, entry) in manifest.iter().enumerate() {
        let value =
            serde_json::from_slice(&fs::read(root.join(entry["file"].as_str().unwrap())).unwrap())
                .unwrap();
        let js = std::env::var_os("ATEMPORAL_JS_FIXTURES").map(|directory| {
            serde_json::from_slice(
                &fs::read(PathBuf::from(directory).join(format!("{index}.json"))).unwrap(),
            )
            .unwrap()
        });
        match entry["type"].as_str().unwrap() {
            "ClientEnvelope" => roundtrip::<ClientEnvelope>(value, js),
            "ServerEnvelope" => roundtrip::<ServerEnvelope>(value, js),
            "WorkerEnvelope" => roundtrip::<WorkerEnvelope>(value, js),
            "Setup" => roundtrip::<Setup>(value, js),
            "Content" => roundtrip::<Content>(value, js),
            "GuideManifest" => roundtrip::<GuideManifest>(value, js),
            "GoldenWorldFixture" => roundtrip::<GoldenWorldFixture>(value, js),
            other => panic!("unhandled fixture type {other}"),
        }
    }
}
#[test]
fn canonical_world_hash_ignores_set_order_but_tracks_future_state() {
    let f: GoldenWorldFixture =
        serde_json::from_slice(&fs::read(root().join("fixtures/worlds/quiet.json")).unwrap())
            .unwrap();
    let state = f.request.checkpoint;
    let hash = identity::world_hash(&state).unwrap();
    assert_eq!(hash, f.initial_hash);
    let mut permuted = state.clone();
    permuted.entities.reverse();
    permuted.players.reverse();
    permuted.control_groups.reverse();
    assert_eq!(identity::world_hash(&permuted).unwrap(), hash);
    permuted.entities[0].last_move_direction = Direction::N;
    assert_ne!(identity::world_hash(&permuted).unwrap(), hash);
    let mut changed = state.clone();
    changed.entities[0].next_move_tick += 1;
    assert_ne!(identity::world_hash(&changed).unwrap(), hash);
    let mut changed = state.clone();
    changed.control_groups[0].latest_order = Some(SavedOrder {
        source_command_id: "r1:p0:c0".into(),
        tick: 0,
        order: Order::Idle {},
    });
    assert_ne!(identity::world_hash(&changed).unwrap(), hash);
    let mut changed = state.clone();
    changed.entities[0].hp = f64::NAN;
    assert!(identity::world_hash(&changed).is_err());
    let mut changed = state.clone();
    changed.ore[0] = f64::INFINITY;
    assert!(identity::world_hash(&changed).is_err());
    let mut changed = state.clone();
    changed.entities[0].tile = changed.entities[1].tile;
    assert!(identity::world_hash(&changed).is_err());
}

#[test]
fn golden_comparator_requires_real_samples_and_checks_expected_economy() {
    let fixture: GoldenWorldFixture =
        serde_json::from_slice(&fs::read(root().join("fixtures/worlds/quiet.json")).unwrap())
            .unwrap();
    assert!(golden::verify(&fixture, &[], &fixture.expected.outcome, &[]).is_err());
    let mut final_state = fixture.request.checkpoint.clone();
    final_state.tick = 3;
    golden::verify(
        &fixture,
        &[final_state.clone()],
        &fixture.expected.outcome,
        &[],
    )
    .unwrap();
    final_state.players[0].bank = 1.;
    assert!(golden::verify(&fixture, &[final_state], &fixture.expected.outcome, &[]).is_err());
}
