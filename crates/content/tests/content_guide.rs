use atemporal_content::*;
use atemporal_contracts::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};
const CONTENT: &str = include_str!("../../../config/content.yaml");
const PROSE: &str = include_str!("../../../client/guide/introduction.html");
static NEXT: AtomicU32 = AtomicU32::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "atemporal-guide-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn normalized_content_has_full_roster_and_half_constructor_throughput() {
    let mut content = load_content(CONTENT).unwrap();
    assert_eq!(content.types.len(), 14);
    let miner = content
        .types
        .iter()
        .find(|t| t.key == "miner")
        .unwrap()
        .mining
        .as_ref()
        .unwrap();
    let constructor = content
        .types
        .iter()
        .find(|t| t.key == "constructor")
        .unwrap()
        .mining
        .as_ref()
        .unwrap();
    assert_eq!(
        constructor.rate / f64::from(constructor.cooldown),
        0.5 * miner.rate / f64::from(miner.cooldown)
    );
    assert_eq!(content.starting_roster, ["miner", "constructor", "turret"]);
    let hash = content_hash(&content).unwrap();
    content.types.reverse();
    assert_eq!(hash, content_hash(&content).unwrap());
    for setup in [
        include_str!("../../../config/game.yaml"),
        include_str!("../../../config/teams.yaml"),
    ] {
        load_setup(setup).unwrap();
    }
}
#[test]
fn rejects_malformed_caps_recipes_and_setup() {
    let original = load_content(CONTENT).unwrap();
    for n in [f64::NAN, f64::INFINITY, -1., 0.] {
        let mut c = original.clone();
        c.types[0].max_hp = n;
        assert!(normalize_content(c).is_err());
    }
    let mut c = original.clone();
    c.types.push(c.types[0].clone());
    assert!(normalize_content(c).is_err());
    let mut c = original.clone();
    c.types
        .iter_mut()
        .find(|t| t.key == "factory")
        .unwrap()
        .production
        .as_mut()
        .unwrap()
        .recipes = vec!["wall".into()];
    assert!(normalize_content(c).is_err());
    let mut c = original.clone();
    c.types
        .iter_mut()
        .find(|t| t.key == "miner")
        .unwrap()
        .mining
        .as_mut()
        .unwrap()
        .cooldown = 0;
    assert!(normalize_content(c).is_err());
    let mut setup = load_setup(include_str!("../../../config/game.yaml")).unwrap();
    setup.match_defaults.future_orders.window_ticks = Some(0);
    assert!(validate_config(&setup.match_defaults).is_err());
    setup.match_defaults.future_orders.window_ticks = None;
    setup.match_defaults.player_count = 3;
    assert!(validate_config(&setup.match_defaults).is_err());
    setup.match_defaults.symmetric = false;
    assert!(validate_config(&setup.match_defaults).is_ok());
}
#[cfg(feature = "guide")]
#[test]
fn startup_override_resume_and_corruption_use_one_generator() {
    let temp = Temp::new();
    let original = load_content(CONTENT).unwrap();
    let mut changed = original.clone();
    changed
        .types
        .iter_mut()
        .find(|t| t.key == "miner")
        .unwrap()
        .max_hp = 12345.;
    write_guide(&original, PROSE, &temp.0).unwrap();
    write_guide(&changed, PROSE, &temp.0).unwrap();
    assert!(
        fs::read_to_string(temp.0.join("index.html"))
            .unwrap()
            .contains("12345")
    );
    fs::write(temp.0.join("index.html"), "corrupt").unwrap();
    let resumed = load_archived_content(CONTENT, &content_hash(&original).unwrap()).unwrap();
    write_guide(&resumed, PROSE, &temp.0).unwrap();
    let exported: Content =
        serde_json::from_slice(&fs::read(temp.0.join("content.json")).unwrap()).unwrap();
    assert_eq!(exported, original);
    assert!(load_archived_content(CONTENT, &content_hash(&changed).unwrap()).is_err());
    assert!(
        !fs::read_to_string(temp.0.join("index.html"))
            .unwrap()
            .contains("12345")
    );
}
#[cfg(feature = "guide")]
#[test]
fn content_schema_is_not_a_second_guide_catalog() {
    let content = load_content(CONTENT).unwrap();
    let temp = Temp::new();
    write_guide(&content, PROSE, &temp.0).unwrap();
    let exported: Content =
        serde_json::from_slice(&fs::read(temp.0.join("content.json")).unwrap()).unwrap();
    assert_eq!(exported, content);
}
