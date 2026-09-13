mod common;
use atemporal_sim::*;
use common::*;
const OPEN: [&str; 8] = ["................"; 8];

#[test]
fn loop_mode_is_captured_per_item_and_survives_checkpoint_reconstruction() {
    let mut w = World::new(&OPEN);
    w.bank(0, 1000.0);
    let f = w.spawn(0, "factory", 1, 1);
    let ids = w.turn(
        1,
        0,
        0,
        vec![
            Command::SetStoredOrder {
                factories: vec![f.clone()],
                order: attack_move(14, 1),
            },
            Command::EditProduction {
                factories: vec![f.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["scout".into()],
                },
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
            Command::SetQueueLoop {
                factories: vec![f.clone()],
                enabled: false,
            },
        ],
    );
    let scout = identity::queue_item(&ids[1], 0, 0).unwrap();
    let grunt = identity::queue_item(&ids[3], 0, 0).unwrap();
    let s = w.at(40);
    let p = entity(&s, &f).production.as_ref().unwrap();
    assert!(!p.loop_enabled, "enqueue switch stays off");
    assert_eq!(
        p.occurrence_counters
            .iter()
            .find(|c| c.item_id == scout)
            .unwrap()
            .next_occurrence,
        1
    );
    assert!(
        p.occurrence_counters
            .iter()
            .find(|c| c.item_id == grunt)
            .unwrap()
            .next_occurrence
            > 1
    );
    assert!(p.pending_items.iter().all(|i| i.loop_enabled));
    w.turn(
        2,
        0,
        40,
        vec![Command::EditProduction {
            factories: vec![f.clone()],
            edit: ProductionEdit::SetItemLoop {
                item_ids: vec![grunt],
                enabled: false,
            },
        }],
    );
    let s = w.at(100);
    let p = entity(&s, &f).production.as_ref().unwrap();
    assert!(
        p.pending_items.is_empty() && p.active_item.is_none(),
        "turning off the existing item drains the loop"
    );
    let run = w.run();
    let mut request = w.request();
    request.checkpoint = run
        .checkpoints
        .iter()
        .find(|s| s.tick == 40)
        .unwrap()
        .clone();
    assert_eq!(
        reconstruct(&request, 100).unwrap(),
        s,
        "loop flags survive checkpoint replay"
    );
}

#[test]
fn turret_self_repair_cost_rate_cap_and_off_are_independent_of_firing() {
    let mut w = World::new(&OPEN);
    w.bank(0, 100.0);
    let t = w.spawn(0, "turret", 1, 1);
    let enemy = w.spawn(1, "tank", 5, 1);
    w.state.entities.iter_mut().find(|e| e.id == t).unwrap().hp = 100.0;
    let s = w.at(1);
    assert_eq!(
        entity(&s, &t).hp,
        75.5,
        "25 damage and 0.5 repair in the same tick"
    );
    assert_eq!(
        entity(&s, &enemy).hp,
        235.0,
        "repair does not suppress the turret shot"
    );
    assert_eq!(
        s.players[0].bank, 99.5,
        "one matter per healed HP: half of construction's two HP/matter"
    );
    let construction = w.def("constructor").construction.as_ref().unwrap();
    assert!(
        0.5 < construction.rate / construction.cooldown as f64 * w.def("turret").max_hp
            / w.def("turret").matter_cost
    );
    w.turn(
        1,
        0,
        0,
        vec![Command::SetPriority {
            entities: vec![t.clone()],
            priority: Priority::Off,
        }],
    );
    let s = w.at(1);
    assert_eq!(entity(&s, &t).hp, 75.0);
    assert_eq!(s.players[0].bank, 100.0);
    assert_eq!(entity(&s, &enemy).hp, 235.0);
    let mut w = World::new(&OPEN);
    w.bank(0, 2.0);
    let t = w.spawn(0, "turret", 1, 1);
    w.state.entities[0].hp = 199.75;
    let s = w.at(10);
    assert_eq!(entity(&s, &t).hp, 200.0);
    assert_eq!(s.players[0].bank, 1.75);
    w.state.entities[0].hp = 100.0;
    w.bank(0, 0.25);
    let s = w.at(10);
    assert_eq!(entity(&s, &t).hp, 100.25);
    assert_eq!(s.players[0].bank, 0.0);
}

#[test]
fn off_disables_factory_and_constructor_spend_and_repair_obeys_tiers() {
    let mut w = World::new(&OPEN);
    w.bank(0, 100.0);
    let f = w.spawn(0, "factory", 1, 1);
    let c = w.spawn_with(
        0,
        "constructor",
        5,
        3,
        Order::Construct {
            area: rect(6, 3, 6, 3),
        },
    );
    w.turn(
        1,
        0,
        0,
        vec![
            Command::SetPriority {
                entities: vec![f.clone(), c.clone()],
                priority: Priority::Off,
            },
            Command::PlaceBlueprints {
                type_key: "turret".into(),
                tiles: vec![tile(6, 3)],
                priority: Priority::High,
                output_directions: None,
            },
            Command::EditProduction {
                factories: vec![f.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
        ],
    );
    let s = w.at(20);
    assert_eq!(s.players[0].bank, 100.0);
    assert_eq!(s.entities.len(), 2);
    w.turn(
        2,
        0,
        20,
        vec![Command::SetPriority {
            entities: vec![c],
            priority: Priority::Medium,
        }],
    );
    let s = w.at(21);
    assert!(s.players[0].bank < 100.0);
    assert_eq!(
        entity(&s, &f)
            .production
            .as_ref()
            .unwrap()
            .active_item
            .as_ref()
            .unwrap()
            .paid_matter,
        0.0
    );
    let mut w = World::new(&OPEN);
    w.bank(0, 0.5);
    let f = w.spawn(0, "factory", 1, 1);
    let t = w.spawn(0, "turret", 5, 3);
    w.state.entities[1].hp = 100.0;
    w.turn(
        1,
        0,
        0,
        vec![
            Command::SetPriority {
                entities: vec![t.clone()],
                priority: Priority::High,
            },
            Command::EditProduction {
                factories: vec![f.clone()],
                edit: ProductionEdit::Append {
                    items: vec!["grunt".into()],
                },
            },
        ],
    );
    let s = w.at(1);
    assert_eq!(entity(&s, &t).hp, 100.5);
    assert_eq!(
        entity(&s, &f)
            .production
            .as_ref()
            .unwrap()
            .active_item
            .as_ref()
            .unwrap()
            .paid_matter,
        0.0
    );
}
