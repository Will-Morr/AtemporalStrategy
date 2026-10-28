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
        c.round = round;
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
                if std::env::var_os("ATEMPORAL_BOT_SINGLE_TRACE").is_some() {
                    eprintln!("single round={round} player={player} {:?}", draft.commands);
                }
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
    after
        .players
        .iter_mut()
        .find(|p| p.player_id == 1)
        .unwrap()
        .currently_eliminated = true;
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
    c.round = 4;
    assert_eq!(
        intervention(&c, 1, Style::Builder),
        70,
        "terminal losses need recovery on every round"
    );
    c.revisions.get_mut(&0).unwrap().turns.push(AcceptedTurn {
        player: 1,
        round: 3,
        tick: 0,
        commands: vec![],
        duration_ms: 0u64.try_into().unwrap(),
    });
    // Recovery is bounded by normal forward progress even if a distant future collapses.
    c.revisions
        .get_mut(&0)
        .unwrap()
        .checkpoints
        .get_mut(&200)
        .unwrap()
        .players[1]
        .currently_eliminated = false;
    assert_eq!(
        intervention(&c, 1, Style::Builder),
        90,
        "advance from the most recent turn, not the maximum timestamp"
    );
    c.revisions
        .get_mut(&0)
        .unwrap()
        .checkpoints
        .get_mut(&200)
        .unwrap()
        .players[1]
        .currently_eliminated = true;
    c.editable_from = 90;
    assert_eq!(intervention(&c, 1, Style::Builder), 90);
}

#[test]
fn opening_produces_workers_that_mine_without_another_round() {
    for style in [Style::Raider, Style::Builder] {
        let (c, state) = tests::setup();
        let view = visible_state(&state, 1, &c.config, &c.content);
        let mut draft = plan(&view, 1, style, &c.config, &c.content, 0);
        retain_legal_actions(&c, 1, &mut draft, &state).unwrap();
        let commands = draft::resolve_local_references(&draft, 1, 1).unwrap();
        let turn = AcceptedTurn {
            player: 1,
            round: 1,
            tick: 0,
            commands: commands
                .into_iter()
                .enumerate()
                .map(|(i, command)| CommittedCommand {
                    id: identity::command_id(1, 1, i as u32),
                    command,
                    future_orders: draft.commands[i].future_orders,
                })
                .collect(),
            duration_ms: 0u64.try_into().unwrap(),
        };
        let final_state = run(
            &request(&c, &state, vec![turn], 1, 300),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap()
        .final_state;
        let miners: Vec<_> = final_state
            .entities
            .iter()
            .filter(|e| e.owner == 1 && e.type_key == "miner")
            .collect();
        assert!(
            miners.len() >= 4,
            "{style:?}: opening factory produced no economy"
        );
        assert!(
            miners
                .iter()
                .all(|e| matches!(e.action, Order::Mine { .. }))
        );
        assert!(final_state.players[1].counters.unit_spend > 0.0);
    }
}

/// Offline counterfactual: preserve the archived map/content/human opening, replace only the
/// bot's first turn. Never changes the archive or feeds opponent commands into the planner.
#[test]
#[ignore = "requires ATEMPORAL_BOT_ARCHIVE pointing to a local match archive"]
fn archived_human_opening_review() {
    let root = PathBuf::from(std::env::var("ATEMPORAL_BOT_ARCHIVE").unwrap());
    let read = |name: &str| std::fs::read_to_string(root.join(name)).unwrap();
    let manifest: crate::archive::Manifest = serde_json::from_str(&read("manifest.json")).unwrap();
    let initial: WorldState = serde_json::from_str(&read("initial-state.json")).unwrap();
    let human: AcceptedTurn = serde_json::from_str(&read("turns/1-0.json")).unwrap();
    let bots: BotConfig = serde_json::from_str(
        &std::fs::read_to_string(root.parent().unwrap().parent().unwrap().join("bots.json"))
            .unwrap(),
    )
    .unwrap();
    let style = bots.slots[1].unwrap();
    let (mut c, _) = tests::setup();
    c.config = manifest.config;
    c.config.simulation_threads = 1;
    c.content = atemporal_content::load_archived_content(
        &read("content.yaml"),
        &manifest.fingerprint.content_hash,
    )
    .unwrap();
    let view = visible_state(&initial, 1, &c.config, &c.content);
    let mut draft = plan(&view, 1, style, &c.config, &c.content, 0);
    retain_legal_actions(&c, 1, &mut draft, &initial).unwrap();
    let commands = draft::resolve_local_references(&draft, 1, 1).unwrap();
    let turn = AcceptedTurn {
        player: 1,
        round: 1,
        tick: 0,
        commands: commands
            .into_iter()
            .enumerate()
            .map(|(i, command)| CommittedCommand {
                id: identity::command_id(1, 1, i as u32),
                command,
                future_orders: draft.commands[i].future_orders,
            })
            .collect(),
        duration_ms: 0u64.try_into().unwrap(),
    };
    let mut checkpoints = vec![];
    let result = run(&request(&c, &initial, vec![human, turn.clone()], 1, 300),
        &AtomicBool::new(false), &mut |output| {
            if let Output::Checkpoint(s) = output {
                let own: Vec<_> = s.entities.iter().filter(|e| e.owner == 1).collect();
                checkpoints.push(serde_json::json!({
                    "tick": s.tick,
                    "miners": own.iter().filter(|e| e.type_key == "miner").count(),
                    "mining_miners": own.iter().filter(|e| e.type_key == "miner" && matches!(e.action, Order::Mine { .. })).count(),
                    "bank": s.players[1].bank,
                    "counters": s.players[1].counters,
                }));
            }
        }).unwrap();
    assert!(
        checkpoints
            .iter()
            .any(|s| s["tick"] == 100 && s["mining_miners"].as_u64().unwrap() >= 3)
            && checkpoints
                .iter()
                .any(|s| s["tick"] == 200 && s["mining_miners"].as_u64().unwrap() >= 5)
            && checkpoints
                .iter()
                .all(|s| s["miners"] == s["mining_miners"]),
        "opening must establish mining immediately and grow while funding military follow-up: {checkpoints:?}"
    );
    // Re-evaluate the intervention that originally skipped early economy work in round 3.
    // This uses the original published revision, independently of the opening counterfactual.
    let record: crate::archive::RoundRecord = serde_json::from_str(&read("rounds/2.json")).unwrap();
    let mut data = crate::controller::RevisionData::new(2, 2, Some(1), record.base_tick);
    data.outcome = Some(record.outcome);
    let states: Vec<WorldState> =
        serde_json::from_str(&read("results/2/checkpoints.json")).unwrap();
    data.checkpoints = states.into_iter().map(|s| (s.tick, s)).collect();
    for round in 1..=2 {
        data.turns
            .push(serde_json::from_str(&read(&format!("turns/{round}-1.json"))).unwrap());
    }
    let previous_tick = data.turns.last().unwrap().tick;
    c.revisions.insert(2, data);
    c.current = 2;
    c.round = 3;
    c.editable_from = record.editable_from;
    let recovery_tick = intervention(&c, 1, style);
    let original: AcceptedTurn = serde_json::from_str(&read("turns/3-1.json")).unwrap();
    assert!(recovery_tick <= previous_tick + 2 * style.interval());
    assert!(recovery_tick < original.tick);
    let report = serde_json::json!({
        "round_3_intervention": { "original": original.tick, "revised": recovery_tick },
        "archive": root, "style": style, "bot_turn": turn,
        "checkpoints": checkpoints, "outcome": result.outcome,
        "limit": "Counterfactual first 300 ticks only; original human opening, revised bot opening. Not a complete-match strength evaluation."
    });
    if let Ok(out) = std::env::var("ATEMPORAL_BOT_REVIEW_OUT") {
        std::fs::write(out, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    eprintln!("{report}");
}

#[derive(serde::Deserialize)]
struct RecordedOpponent {
    config: MatchConfig,
    content: Content,
    initial: WorldState,
    style: Style,
    human_turns: Vec<AcceptedTurn>,
}

#[test]
fn recorded_sustained_pressure_match() {
    let fixture: RecordedOpponent = serde_json::from_str(include_str!(
        "../../../../fixtures/bots/sustained-pressure.json"
    ))
    .unwrap();
    let (mut c, _) = tests::setup();
    c.config = fixture.config;
    c.config.simulation_threads = 1;
    c.content = fixture.content;
    let mut ledger = vec![];
    let mut precedence = vec![];
    let mut scores: Option<RoundScore> = None;
    let mut report = vec![];
    for round in 0..=24 {
        let mut bot_tick = 0;
        let mut bot_commands = vec![];
        if round > 0 {
            c.round = round;
            bot_tick = intervention(&c, 1, fixture.style);
            let state =
                reconstruct(&c.exact_request(c.current, bot_tick).unwrap(), bot_tick).unwrap();
            let view = visible_state(&state, 1, &c.config, &c.content);
            let mut knowledge = Knowledge::new(&state);
            for (_, checkpoint) in c.revisions[&c.current].checkpoints.range(..=bot_tick) {
                knowledge.observe(checkpoint, 1, &c.config, &c.content);
            }
            knowledge.observe(&state, 1, &c.config, &c.content);
            let mut draft = if preserve_winning_timeline(&c, 1) {
                TurnDraft {
                    based_on_revision: c.current,
                    tick: bot_tick,
                    commands: vec![],
                }
            } else {
                planner::plan_known(
                    &view,
                    1,
                    fixture.style,
                    &c.config,
                    &c.content,
                    c.current,
                    &knowledge,
                )
            };
            retain_legal_actions(&c, 1, &mut draft, &state).unwrap();
            let commands = draft::resolve_local_references(&draft, round, 1).unwrap();
            bot_commands = commands
                .iter()
                .map(|x| serde_json::to_value(x).unwrap())
                .collect();
            ledger.push(AcceptedTurn {
                player: 1,
                round,
                tick: bot_tick,
                commands: commands
                    .into_iter()
                    .enumerate()
                    .map(|(i, command)| CommittedCommand {
                        id: identity::command_id(round, 1, i as u32),
                        command,
                        future_orders: draft.commands[i].future_orders,
                    })
                    .collect(),
                duration_ms: 0u64.try_into().unwrap(),
            });
            // Opponent input is added only after the bot has planned against the previous revision.
            ledger.push(
                fixture
                    .human_turns
                    .get(round as usize - 1)
                    .cloned()
                    .unwrap_or(AcceptedTurn {
                        player: 0,
                        round,
                        tick: c.available_through().min(c.config.max_tick - 1),
                        commands: vec![],
                        duration_ms: 0u64.try_into().unwrap(),
                    }),
            );
            precedence.push(identity::round_precedence(round, 2).unwrap());
        }
        let req = c.build_request(
            round,
            fixture.initial.clone(),
            ledger.clone(),
            precedence.clone(),
            0,
        );
        let mut data = crate::controller::RevisionData::new(round, round, round.checked_sub(1), 0);
        data.checkpoints.insert(0, fixture.initial.clone());
        let result = run(&req, &AtomicBool::new(false), &mut |o| {
            if let Output::Checkpoint(s) = o {
                data.checkpoints.insert(s.tick, s);
            }
        })
        .unwrap();
        data.checkpoints
            .insert(result.final_state.tick, result.final_state.clone());
        data.outcome = Some(result.outcome.clone());
        data.turns = ledger.clone();
        data.precedence = precedence.clone();
        if round > 0 {
            let Objective::Scoreboard { rules } = &c.config.objective else {
                panic!("scoreboard fixture required")
            };
            scores = Some(
                scoring::resolve_round(
                    round,
                    2,
                    &c.config.multiplayer,
                    rules,
                    &result.outcome,
                    &(0..2)
                        .map(|player_id| PlayerTime {
                            player_id,
                            total_ms: 0u64.try_into().unwrap(),
                        })
                        .collect::<Vec<_>>(),
                    scores.as_ref(),
                )
                .unwrap(),
            );
            let peaks: Vec<_> = (0..2)
                .map(|p| {
                    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
                    for s in data.checkpoints.values() {
                        let mut counts = std::collections::BTreeMap::<String, usize>::new();
                        for e in s.entities.iter().filter(|e| e.owner == p) {
                            *counts.entry(e.type_key.clone()).or_default() += 1;
                        }
                        for (key, count) in counts {
                            let n = kinds.entry(key).or_default();
                            *n = (*n).max(count);
                        }
                    }
                    kinds
                })
                .collect();
            eprintln!(
                "round {round} bot@{bot_tick}: {:?} survivors={:?} terminal={} peaks={peaks:?}",
                result.outcome.kind, result.outcome.survivors, result.outcome.terminal_state_tick
            );
            report.push(serde_json::json!({"round":round,"bot_tick":bot_tick,"commands":bot_commands,"outcome":result.outcome,"score":scores,"peaks":peaks,"players":result.final_state.players,"final_entities":result.final_state.entities.iter().map(|e| serde_json::json!({"owner":e.owner,"type":e.type_key,"tile":e.tile,"order":e.action})).collect::<Vec<_>>(),"skipped":result.command_outcomes.iter().filter(|o| !o.skipped.is_empty()).collect::<Vec<_>>()}));
        }
        c.revisions.clear();
        c.revisions.insert(round, data);
        c.current = round;
        if round >= fixture.human_turns.len() as u32
            && scores.as_ref().is_some_and(|s| !s.match_winners.is_empty())
        {
            break;
        }
    }
    if let Ok(path) = std::env::var("ATEMPORAL_BOT_MATCH_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert_eq!(
        scores.unwrap().match_winners,
        vec![SideId::Player { player_id: 1 }],
        "bot must win against the recorded sequence followed by passes, before the opponent wins"
    );
}
