use atemporal_contracts::{draft::*, identity::*, *};
use serde_json::json;
use std::{fs, path::PathBuf};
fn quiet() -> GoldenWorldFixture {
    serde_json::from_slice(
        &fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/worlds/quiet.json"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn local_draft() -> TurnDraft {
    serde_json::from_value(json!({"based_on_revision":1,"tick":20,"commands":[
        {"local_id":"place","future_orders":"keep","command":{"kind":"place_blueprints","type_key":"factory","tiles":[{"x":2,"y":2}],"priority":"high","output_directions":["e"]}},
        {"local_id":"cancel","future_orders":"keep","command":{"kind":"cancel_blueprints","blueprint_ids":[{"kind":"draft","local_id":"place","item_index":0}]}}
    ]})).unwrap()
}
#[test]
fn action_only_lock_and_exact_interval_edges() {
    let command = DraftCommand {
        local_id: "priority".into(),
        command: Command::SetPriority {
            entities: vec![],
            priority: Priority::High,
        },
        future_orders: FutureOrderPolicy::DropAll,
    };
    assert!(validate_policy(&command, Some(10)).is_err());
    assert!(
        validate_policy(
            &DraftCommand {
                future_orders: FutureOrderPolicy::Keep,
                ..command
            },
            Some(10)
        )
        .is_ok()
    );
    let interval = lock_interval(20, FutureOrderPolicy::DropWindow, Some(10), 100)
        .unwrap()
        .unwrap();
    for (tick, removed) in [(20, false), (21, true), (30, true), (31, false)] {
        assert_eq!(
            tick > interval.after_tick && tick <= interval.through_tick,
            removed
        );
    }
    assert_eq!(
        lock_interval(20, FutureOrderPolicy::DropAll, None, 100)
            .unwrap()
            .unwrap()
            .through_tick,
        99
    );
    assert!(lock_interval(20, FutureOrderPolicy::DropWindow, None, 100).is_err());
    assert!(lock_interval(20, FutureOrderPolicy::DropWindow, Some(0), 100).is_err());
    assert_eq!(
        lock_interval(
            u32::MAX - 2,
            FutureOrderPolicy::DropWindow,
            Some(10),
            u32::MAX
        )
        .unwrap()
        .unwrap()
        .through_tick,
        u32::MAX - 1
    );
}
#[test]
fn earlier_local_references_resolve_to_causal_ids_and_fail_atomically() {
    let mut draft = local_draft();
    let commands = resolve_local_references(&draft, 2, 0).unwrap();
    let expected = blueprint(&command_id(2, 0, 0), 0).unwrap();
    assert_eq!(
        commands[1],
        Command::CancelBlueprints {
            blueprint_ids: vec![expected]
        }
    );
    draft.commands.swap(0, 1);
    assert!(resolve_local_references(&draft, 2, 0).is_err());
    let mut draft = local_draft();
    draft.commands[1].local_id = "place".into();
    assert!(resolve_local_references(&draft, 2, 0).is_err());
    let mut draft = local_draft();
    draft.commands[1].command = Command::CancelBlueprints {
        blueprint_ids: vec![DraftItemRef::Draft {
            local_id: "place".into(),
            item_index: 1,
        }],
    };
    assert!(resolve_local_references(&draft, 2, 0).is_err());
}
#[test]
fn local_queue_items_are_scoped_by_factory() {
    let draft:TurnDraft=serde_json::from_value(json!({"based_on_revision":0,"tick":0,"commands":[
        {"local_id":"queue","future_orders":"keep","command":{"kind":"edit_production","factories":[genesis(0, 3).unwrap(),genesis(0, 4).unwrap()],"edit":{"kind":"append","items":["grunt"]}}},
        {"local_id":"remove","future_orders":"keep","command":{"kind":"edit_production","factories":[genesis(0, 3).unwrap(),genesis(0, 4).unwrap()],"edit":{"kind":"remove_pending","item_ids":[{"kind":"draft","local_id":"queue","item_index":0}]}}}
    ]})).unwrap();
    let result = resolve_local_references(&draft, 1, 0).unwrap();
    let Command::EditProduction {
        edit: ProductionEdit::RemovePending { item_ids },
        ..
    } = &result[1]
    else {
        panic!("wrong command")
    };
    assert_eq!(item_ids.len(), 2);
    assert_ne!(item_ids[0], item_ids[1]);
    assert_eq!(item_ids[1].birth_command.target_index, 1);
    let mut subset = draft.clone();
    if let Command::EditProduction { factories, .. } = &mut subset.commands[1].command {
        factories.remove(0);
    }
    let subset_result = resolve_local_references(&subset, 1, 0).unwrap();
    if let Command::EditProduction {
        edit: ProductionEdit::RemovePending {
            item_ids: subset_ids,
        },
        ..
    } = &subset_result[1]
    {
        assert_eq!(subset_ids, &vec![item_ids[1].clone()]);
    } else {
        panic!("wrong command");
    }
    if let Command::EditProduction { factories, .. } = &mut subset.commands[0].command {
        factories.reverse();
    }
    assert!(resolve_local_references(&subset, 1, 0).is_err());
}
#[test]
fn timed_uses_locked_build_ability_not_sim_elimination_or_future_endpoint() {
    let mut fixture = quiet();
    fixture.request.config.objective = Objective::Timed {
        lock_ticks_per_round: 2,
    };
    let config = &fixture.request.config;
    let content = &fixture.request.content;
    let mut locked = fixture.request.checkpoint.clone();
    locked.tick = 2;
    locked
        .entities
        .retain(|e| !(e.owner == 0 && e.type_key == "turret"));
    locked.players[0].currently_eliminated = true;
    let result = timed::adjudicate(config, content, 0, &locked, &[]).unwrap();
    assert_eq!(result.status, TimedStatus::Planning);
    assert!(result.timed_lost_players.is_empty());
    locked
        .entities
        .retain(|e| !(e.owner == 0 && e.type_key == "constructor"));
    let result = timed::adjudicate(config, content, 0, &locked, &[]).unwrap();
    assert_eq!(result.match_winners, vec![SideId::Player { player_id: 1 }]);
    assert_eq!(result.timed_lost_players, vec![0]);
    locked.entities.retain(|e| e.type_key != "constructor");
    let result = timed::adjudicate(config, content, 0, &locked, &[]).unwrap();
    assert_eq!(result.status, TimedStatus::Finished);
    assert!(result.match_winners.is_empty());
    locked.tick = 3;
    assert!(timed::adjudicate(config, content, 0, &locked, &[]).is_err());
}
#[test]
fn exhausted_history_is_unfinished_and_sites_do_not_satisfy_timed_survival() {
    let mut fixture = quiet();
    fixture.request.config.objective = Objective::Timed {
        lock_ticks_per_round: 2,
    };
    fixture.request.config.max_tick = 2;
    let mut locked = fixture.request.checkpoint.clone();
    locked.tick = 2;
    let result = timed::adjudicate(
        &fixture.request.config,
        &fixture.request.content,
        0,
        &locked,
        &[],
    )
    .unwrap();
    assert_eq!(result.status, TimedStatus::HistoryExhausted);
    assert!(result.match_winners.is_empty());
    locked
        .entities
        .iter_mut()
        .filter(|e| e.owner == 0 && e.type_key == "constructor")
        .for_each(|e| e.lifecycle = Lifecycle::Site);
    let result = timed::adjudicate(
        &fixture.request.config,
        &fixture.request.content,
        0,
        &locked,
        &[],
    )
    .unwrap();
    assert_eq!(result.timed_lost_players, vec![0]);
}
