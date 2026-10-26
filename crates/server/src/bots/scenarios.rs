use super::*;
use std::{collections::BTreeSet, sync::atomic::AtomicBool};

fn request(
    c: &Controller,
    state: &WorldState,
    events: Vec<AcceptedTurn>,
    round: u32,
    end: Tick,
) -> SimRequest {
    SimRequest {
        schema_version: Version::default(),
        job_id: "bot-scenario".into(),
        revision: round,
        fingerprint: c.fingerprint.clone(),
        config: c.config.clone(),
        content: c.content.clone(),
        checkpoint: state.clone(),
        events,
        precedence: vec![identity::round_precedence(round, 2).unwrap()],
        end_tick_exclusive: end,
        minimum_end_tick: end,
        entity_dictionary: vec![],
    }
}

fn play(styles: [Style; 2], single: bool, seed: u64) {
    let (mut c, _) = tests::setup();
    c.config.seed = seed.try_into().unwrap();
    c.config.stop_when_decided = false;
    c.config.max_tick = 4000;
    c.config.checkpoint_interval = 20;
    if single {
        c.config.control_limit = ControlLimit::SingleOrder;
    }
    let mut state = map::generate(&c.config, &c.content).unwrap();
    let mut knowledge = [Knowledge::new(&state), Knowledge::new(&state)];
    let mut kinds: [BTreeSet<String>; 2] = Default::default();
    let mut peak_miners = [0; 2];
    let mut peak_factories = [0; 2];
    let mut peak_army = [0; 2];
    for round in 1..=40 {
        let mut turns = vec![];
        for player in 0..2u8 {
            knowledge[player as usize].observe(&state, player, &c.config, &c.content);
            let view = visible_state(&state, player, &c.config, &c.content);
            let draft = planner::plan_known(
                &view,
                player,
                styles[player as usize],
                &c.config,
                &c.content,
                round - 1,
                &knowledge[player as usize],
            );
            if single {
                assert!(draft.commands.len() <= 1);
            }
            let commands = draft::resolve_local_references(&draft, round, player).unwrap();
            c.validate_against_state(player, &commands, &state)
                .unwrap_or_else(|e| panic!("round {round}: {e}: {draft:?}"));
            turns.push(AcceptedTurn {
                player,
                round,
                tick: state.tick,
                commands: commands
                    .into_iter()
                    .enumerate()
                    .map(|(i, command)| CommittedCommand {
                        id: identity::command_id(round, player, i as u32),
                        command,
                        future_orders: draft.commands[i].future_orders,
                    })
                    .collect(),
                duration_ms: 0u64.try_into().unwrap(),
            });
        }
        let end = state.tick + if single { 45 } else { 70 };
        let result = run(
            &request(&c, &state, turns, round, end),
            &AtomicBool::new(false),
            &mut |output| {
                if let Output::Checkpoint(s) = output {
                    for (p, knowledge) in knowledge.iter_mut().enumerate() {
                        knowledge.observe(&s, p as u8, &c.config, &c.content);
                    }
                }
            },
        )
        .unwrap();
        state = result.final_state;
        for p in 0..2 {
            let own: Vec<_> = state
                .entities
                .iter()
                .filter(|e| e.owner == p as u8 && e.lifecycle == Lifecycle::Complete)
                .collect();
            for e in &own {
                kinds[p].insert(e.type_key.clone());
            }
            peak_miners[p] =
                peak_miners[p].max(own.iter().filter(|e| e.type_key == "miner").count());
            peak_factories[p] =
                peak_factories[p].max(own.iter().filter(|e| e.type_key == "factory").count());
            peak_army[p] = peak_army[p].max(
                own.iter()
                    .filter(|e| {
                        ["grunt", "tank", "grinder", "artillery", "scout"]
                            .contains(&e.type_key.as_str())
                    })
                    .count(),
            );
        }
    }
    eprintln!(
        "{styles:?} single={single} seed={seed}: miners={peak_miners:?} factories={peak_factories:?} army={peak_army:?} kinds={kinds:?} damage={:?}",
        state
            .players
            .iter()
            .map(|p| p.counters.damage_dealt)
            .collect::<Vec<_>>()
    );
    for p in 0..2 {
        assert!(peak_miners[p] >= 4, "economy did not grow: {peak_miners:?}");
        assert!(
            peak_factories[p] >= 2,
            "production did not expand: {peak_factories:?}"
        );
        assert!(peak_army[p] >= 6, "no meaningful army: {peak_army:?}");
        assert!(state.players[p].counters.damage_dealt > 100.0, "no combat");
    }
    if !single {
        for key in [
            "miner",
            "constructor",
            "scout",
            "grunt",
            "grinder",
            "tank",
            "artillery",
            "factory",
            "turret",
        ] {
            assert!(
                kinds.iter().all(|k| k.contains(key)),
                "missing {key}: {kinds:?}"
            );
        }
    }
}
#[test]
fn self_play_grows_economies_and_uses_mixed_armies() {
    for (styles, seed) in [
        ([Style::Raider, Style::Builder], 42),
        ([Style::Raider; 2], 43),
        ([Style::Builder; 2], 44),
    ] {
        play(styles, false, seed);
    }
}
#[test]
fn single_order_builds_a_working_economy_and_army() {
    play([Style::Raider, Style::Builder], true, 42);
}

#[test]
fn lost_miner_uses_constructor_income_and_replaces_workers() {
    let (c, mut state) = tests::setup();
    state
        .entities
        .retain(|e| !(e.owner == 1 && e.type_key == "miner"));
    state
        .players
        .iter_mut()
        .find(|p| p.player_id == 1)
        .unwrap()
        .bank = 0.0;
    for style in [Style::Raider, Style::Builder] {
        let view = visible_state(&state, 1, &c.config, &c.content);
        let draft = plan(&view, 1, style, &c.config, &c.content, 0);
        let constructor = state
            .entities
            .iter()
            .find(|e| e.owner == 1 && e.type_key == "constructor")
            .unwrap();
        assert!(draft.commands.iter().any(|d| matches!(&d.command, Command::AssignOrder { entities, order: Order::Mine {..} } if entities.contains(&constructor.id))));
    }
}

#[test]
fn legacy_rusher_maps_to_one_of_only_two_policies() {
    assert_eq!(
        serde_json::from_str::<Style>("\"rusher\"").unwrap(),
        Style::Raider
    );
    assert_eq!(serde_json::to_string(&Style::Raider).unwrap(), "\"raider\"");
}

#[tokio::test]
async fn published_timeline_self_play_uses_real_controller() {
    let (mut c, _) = tests::setup();
    c.config.max_tick = 1800;
    c.config.checkpoint_interval = 50;
    c.config.objective = Objective::Scoreboard {
        rules: ScoreboardRules {
            victory_rule: VictoryRule::FixedTarget { points: 100.0 },
            ..ScoreboardRules::default()
        },
    };
    let root = std::env::temp_dir().join(format!(
        "atemporal-bot-rounds-{}-{}",
        std::process::id(),
        crate::controller::now_ms()
    ));
    let mut setup = atemporal_content::load_setup(&c.config_yaml).unwrap();
    setup.match_defaults = c.config.clone();
    let mut c = Controller::new(
        setup,
        c.content.clone(),
        c.config_yaml.clone(),
        c.content_yaml.clone(),
        "/guide/".into(),
        root.clone(),
        crate::adapter::SimThread::spawn(),
    )
    .unwrap();
    let bots = BotConfig {
        slots: vec![Some(Style::Raider), Some(Style::Builder)],
        playtest_rounds: 24,
    };
    bots.install(&mut c).unwrap();
    let app = ws::App {
        controller: std::sync::Arc::new(std::sync::Mutex::new(c)),
        client_dir: PathBuf::new(),
        guide_dir: PathBuf::new(),
        inputs_only: false,
    };
    let mut peaks = [0; 2];
    let mut damage = [0.0f64; 2];
    let mut rewind = false;
    let mut previous = [0; 2];
    tokio::time::timeout(Duration::from_secs(150), async {
        loop {
            step(&app, &bots).await.unwrap();
            {
                let c = app.controller.lock().unwrap();
                if let Some(data) = c.revisions.get(&c.current) {
                    for s in data.checkpoints.values() {
                        for p in 0..2 {
                            peaks[p] = peaks[p].max(
                                s.entities
                                    .iter()
                                    .filter(|e| e.owner == p as u8 && e.type_key == "miner")
                                    .count(),
                            );
                            damage[p] = damage[p].max(s.players[p].counters.damage_dealt);
                        }
                    }
                    for (p, previous) in previous.iter_mut().enumerate() {
                        if let Some(t) = data
                            .turns
                            .iter()
                            .filter(|t| t.player == p as u8)
                            .max_by_key(|t| t.round)
                        {
                            rewind |= t.tick < *previous;
                            *previous = t.tick;
                        }
                    }
                }
                if matches!(c.phase, Phase::Archived | Phase::Finished) {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("bot rounds stalled");
    eprintln!(
        "real controller: peak miners={peaks:?}, damage={damage:?}, rewound={rewind}, archives={}",
        root.display()
    );
    assert!(
        rewind,
        "published losses should trigger an earlier intervention"
    );
    assert!(peaks.iter().all(|n| *n >= 4));
    assert!(damage.iter().all(|n| *n > 100.0));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejection_keeps_independent_mining_order() {
    let (c, state) = tests::setup();
    let view = visible_state(&state, 1, &c.config, &c.content);
    let mut draft = plan(&view, 1, Style::Builder, &c.config, &c.content, 0);
    let turret = state
        .entities
        .iter()
        .find(|e| e.owner == 1 && e.type_key == "turret")
        .unwrap();
    for d in &mut draft.commands {
        if let Command::PlaceBlueprints { tiles, .. } = &mut d.command {
            *tiles = vec![turret.tile];
        }
    }
    retain_legal_actions(&c, 1, &mut draft, &state).unwrap();
    assert!(
        !draft
            .commands
            .iter()
            .any(|d| matches!(d.command, Command::PlaceBlueprints { .. }))
    );
    assert!(draft.commands.iter().any(|d| matches!(
        d.command,
        Command::AssignOrder {
            order: Order::Mine { .. },
            ..
        }
    )));
}

#[test]
fn intervention_rewinds_before_losses_and_respects_locked_history() {
    let (mut c, state) = tests::setup();
    let mut data = crate::controller::RevisionData::new(0, 2, None, 0);
    let result = run(
        &request(&c, &state, vec![], 1, 400),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    data.outcome = Some(result.outcome);
    let mut before = state.clone();
    before.tick = 100;
    let mut after = before.clone();
    after.tick = 200;
    after.entities.retain(|e| e.owner != 1);
    data.checkpoints.insert(100, before);
    data.checkpoints.insert(200, after);
    data.turns.push(AcceptedTurn {
        player: 1,
        round: 2,
        tick: 150,
        commands: vec![],
        duration_ms: 0u64.try_into().unwrap(),
    });
    c.revisions.insert(0, data);
    c.current = 0;
    c.round = 3;
    assert_eq!(intervention(&c, 1, Style::Builder), 70);
    c.editable_from = 90;
    assert_eq!(intervention(&c, 1, Style::Builder), 90);
}
