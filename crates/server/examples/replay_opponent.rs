//! Evaluate a bot against a recording or run a seeded head-to-head match.
use atemporal_bots::{TacticalParams, published_plan_with_params};
use atemporal_server::{
    adapter::SimThread,
    bots::{self, Style},
    controller::{Controller, RevisionData},
};
use atemporal_sim::*;
use std::{path::Path, sync::atomic::AtomicBool, time::Instant};

fn read(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read spectator export"))
        .expect("spectator JSON")
}

fn bot_turn(
    c: &Controller,
    bot: PlayerId,
    style: Style,
    params: TacticalParams,
) -> (AcceptedTurn, bool) {
    let round = c.round;
    let timeline = bots::published_timeline(c, bot).expect("published player timeline");
    let draft = published_plan_with_params(&timeline, style, params);
    c.precheck_commit(
        bot,
        &CommitRequest {
            request_id: format!("evaluation-{round}-{bot}"),
            slot_token: String::new(),
            draft: draft.clone(),
        },
    )
    .expect("bot draft precheck");
    let tick = draft.tick;
    let state = reconstruct(&c.exact_request(c.current, tick).unwrap(), tick).unwrap();
    let resolved = draft::resolve_local_references(&draft, round, bot).unwrap();
    if let Err(error) = c.validate_against_state(bot, &resolved, &state) {
        eprintln!("Round {round} bot passed after its draft was rejected: {error}");
        return (
            AcceptedTurn {
                round,
                player: bot,
                tick,
                commands: vec![],
                duration_ms: 1000u64.try_into().unwrap(),
            },
            true,
        );
    }
    let commands = resolved
        .into_iter()
        .zip(draft.commands)
        .enumerate()
        .map(|(i, (command, d))| CommittedCommand {
            id: identity::command_id(round, bot, i as u32),
            command,
            future_orders: d.future_orders,
        })
        .collect();
    (
        AcceptedTurn {
            round,
            player: bot,
            tick,
            commands,
            duration_ms: 1000u64.try_into().unwrap(),
        },
        false,
    )
}

fn main() {
    let started = Instant::now();
    let args: Vec<_> = std::env::args().collect();
    let self_play = args.get(4).is_some_and(|flag| flag == "--self-play");
    let (profiles, round_cap) = if self_play {
        assert_eq!(
            args.len(),
            8,
            "use --self-play ROUND_CAP BOT_PARAMS_JSON OPPONENT_PARAMS_JSON"
        );
        assert!(args[1].starts_with("seed:"), "self-play requires a seed");
        assert_eq!(args[2], "tactical", "self-play is Tactical versus Tactical");
        let cap = args[5].parse::<u32>().expect("round cap");
        assert!((1..=1000).contains(&cap), "round cap must be 1..=1000");
        let parse = |profile: &str| {
            let params = serde_json::from_str::<TacticalParams>(profile).expect("Tactical JSON");
            params.validate().expect("Tactical parameters");
            params
        };
        ([parse(&args[6]), parse(&args[7])], cap)
    } else {
        assert!(
            (4..=5).contains(&args.len()) && args.get(4).is_none_or(|m| m == "--frozen"),
            "usage: replay_opponent EXPORT_DIRECTORY|seed:INTEGER tactical|standard 0|1 [--frozen | --self-play ROUND_CAP BOT_PARAMS_JSON OPPONENT_PARAMS_JSON]"
        );
        ([TacticalParams::default(); 2], 12)
    };
    let frozen = args.get(4).is_some_and(|flag| flag == "--frozen");
    let dir = Path::new(&args[1]);
    let bot: u8 = args[3].parse().unwrap();
    assert!(bot < 2);
    let style = match args[2].as_str() {
        "standard" => Style::Standard,
        "tactical" => Style::Tactical,
        _ => panic!("unknown policy"),
    };
    let yaml = include_str!("../../../config/game.yaml");
    let content_yaml = include_str!("../../../config/content.yaml");
    let mut setup = atemporal_content::load_setup(yaml).unwrap();
    let content = load_content(content_yaml).unwrap();
    let (initial, recorded) = if let Some(seed) = args[1].strip_prefix("seed:") {
        setup.match_defaults.seed = seed.parse::<u64>().unwrap().try_into().unwrap();
        (
            map::generate(&setup.match_defaults, &content).unwrap(),
            None,
        )
    } else if dir.is_file() {
        let mut recording = read(dir);
        setup.match_defaults = serde_json::from_value(recording["config"].take()).unwrap();
        (
            serde_json::from_value(recording["initial"].take()).unwrap(),
            Some(serde_json::from_value(recording["human_turns"].take()).unwrap()),
        )
    } else {
        setup.match_defaults =
            serde_json::from_value(read(&dir.join("game.json"))["config"].take()).unwrap();
        let initial =
            serde_json::from_value(read(&dir.join("initial.json"))["snapshot"].take()).unwrap();
        let recorded: Vec<AcceptedTurn> =
            serde_json::from_value(read(&dir.join("commands.json"))["turns"].take()).unwrap();
        (initial, Some(recorded))
    };
    assert_eq!(setup.match_defaults.player_count, 2);
    assert!(matches!(
        setup.match_defaults.objective,
        Objective::Scoreboard { .. }
    ));
    if self_play {
        assert!(matches!(
            setup.match_defaults.multiplayer,
            Multiplayer::Ffa {}
        ));
        let Objective::Scoreboard { rules } = &setup.match_defaults.objective else {
            unreachable!()
        };
        assert_eq!(
            rules.time_penalty,
            TimePenalty::None,
            "self-play uses synthetic planning times"
        );
    }
    setup.match_defaults.simulation_threads = 1;
    let mut c = Controller::new(
        setup,
        content,
        yaml.into(),
        content_yaml.into(),
        String::new(),
        std::env::temp_dir(),
        SimThread::spawn(),
    )
    .unwrap();
    let mut ledger = vec![];
    let mut precedence = vec![];
    let mut score = None;
    let mut evaluated_rounds = vec![];
    let mut rejected_drafts = [0; 2];
    let mut match_winners = vec![];
    let mut final_scores = [0.; 2];
    let mut turn_hashes = [String::new(), String::new()];
    // A published opening is the empty ledger. Both seats plan against that same revision.
    for round in 0..=recorded.as_ref().map_or(round_cap, |turns| {
        turns.iter().map(|t| t.round).max().unwrap_or_default()
    }) {
        if round > 0 {
            c.round = round;
            c.phase = Phase::Planning;
            let (turn, rejected) = bot_turn(&c, bot, style, profiles[0]);
            rejected_drafts[usize::from(bot)] += u32::from(rejected);
            if self_play {
                turn_hashes[usize::from(bot)] = identity::canonical_hash(&turn).unwrap();
            }
            if frozen {
                let recorded = recorded
                    .as_ref()
                    .expect("frozen evaluation requires recorded commands");
                if round == 1 {
                    ledger.extend(recorded.iter().filter(|t| t.player != bot).cloned());
                    precedence.extend(
                        (1..=recorded.iter().map(|t| t.round).max().unwrap())
                            .map(|r| identity::round_precedence(r, 2).unwrap()),
                    );
                }
                ledger.push(turn);
            } else {
                let other = match &recorded {
                    Some(turns) => turns
                        .iter()
                        .find(|t| t.round == round && t.player != bot)
                        .expect("recorded human turn")
                        .clone(),
                    None => {
                        let other_style = if self_play {
                            Style::Tactical
                        } else {
                            Style::Standard
                        };
                        let (turn, rejected) = bot_turn(&c, 1 - bot, other_style, profiles[1]);
                        rejected_drafts[usize::from(1 - bot)] += u32::from(rejected);
                        if self_play {
                            turn_hashes[usize::from(1 - bot)] =
                                identity::canonical_hash(&turn).unwrap();
                        }
                        turn
                    }
                };
                if self_play && bot == 1 {
                    ledger.extend([other, turn]);
                } else {
                    ledger.extend([turn, other]);
                }
                precedence.push(identity::round_precedence(round, 2).unwrap());
            }
        }
        let request = c.build_request(
            round,
            initial.clone(),
            ledger.clone(),
            precedence.clone(),
            0,
        );
        let mut data = RevisionData::new(round, round, round.checked_sub(1), 0);
        data.checkpoints.insert(0, initial.clone());
        let result = run(&request, &AtomicBool::new(false), &mut |out| {
            if let Output::Checkpoint(s) = out {
                data.checkpoints.insert(s.tick, s);
            }
        })
        .unwrap();
        if std::env::var_os("ATEMPORAL_BALANCE_TRACE").is_some() {
            for (tick, s) in data
                .checkpoints
                .range(100..=1000)
                .step_by(2)
                .chain(data.checkpoints.last_key_value())
            {
                for player in &s.players {
                    let mut army = std::collections::BTreeMap::new();
                    for e in s.entities.iter().filter(|e| {
                        e.owner == player.player_id && e.lifecycle == Lifecycle::Complete
                    }) {
                        *army.entry(e.type_key.as_str()).or_insert(0) += 1;
                    }
                    eprintln!(
                        "round {round} tick {tick} player {} bank {:.0} mined {:.0} complete {army:?}",
                        player.player_id, player.bank, player.counters.mined
                    );
                }
            }
        }
        if round > 0 {
            let Objective::Scoreboard { rules } = &c.config.objective else {
                unreachable!()
            };
            let times = (0..2)
                .map(|player_id| PlayerTime {
                    player_id,
                    total_ms: 1000u64.try_into().unwrap(),
                })
                .collect::<Vec<_>>();
            let resolved = scoring::resolve_round(
                round,
                2,
                &c.config.multiplayer,
                rules,
                &result.outcome,
                &times,
                score.as_ref(),
            )
            .unwrap();
            let other_outcomes = result
                .command_outcomes
                .iter()
                .filter(|o| o.command_id.player != bot)
                .collect::<Vec<_>>();
            let absent = other_outcomes
                .iter()
                .filter(|o| {
                    o.skipped
                        .iter()
                        .any(|s| matches!(s.reason, SkipReason::Absent | SkipReason::Dead))
                })
                .count();
            if !self_play {
                println!(
                    "round={round},tick={},survivors={:?},bot_mined={},bot_unit_spend={},opponent_commands_with_absent_target={absent}/{},score={:?}",
                    result.final_state.tick,
                    result.outcome.survivors,
                    result.final_state.players[bot as usize].counters.mined,
                    result.final_state.players[bot as usize].counters.unit_spend,
                    other_outcomes.len(),
                    resolved
                        .entries
                        .iter()
                        .map(|e| e.raw_total)
                        .collect::<Vec<_>>()
                );
            } else {
                for entry in &resolved.entries {
                    let SideId::Player { player_id } = entry.side_id else {
                        panic!("evaluator requires FFA");
                    };
                    final_scores[usize::from(player_id)] = entry.raw_total;
                }
                evaluated_rounds.push(serde_json::json!({
                    "round": round,
                    "outcome": result.outcome.kind,
                    "stop_reason": result.outcome.stop_reason,
                    "terminal_tick": result.outcome.terminal_state_tick,
                    "state_hash": result.final_hash,
                    "accepted_turn_hashes_by_seat": turn_hashes,
                    "score_by_seat": final_scores,
                }));
            }
            if !resolved.match_winners.is_empty() {
                match_winners = resolved.match_winners;
                break;
            }
            score = Some(resolved);
        }
        data.outcome = Some(result.outcome);
        data.final_hash = result.final_hash;
        data.command_outcomes = result.command_outcomes;
        data.score = score.clone();
        data.precedence = precedence.clone();
        data.turns = ledger.clone();
        // Keep published ledgers, outcomes and the opening for historical analysis.
        // Only the latest revision needs its checkpoint body for validation.
        if round > 0 {
            let previous = c.revisions.get_mut(&c.current).unwrap();
            previous
                .checkpoints
                .retain(|tick, _| c.current == 0 && *tick == 0);
        }
        c.revisions.insert(round, data);
        c.current = round;
    }
    if self_play {
        println!(
            "{}",
            serde_json::json!({
                "rules_version": RULES_VERSION,
                "fingerprint": c.fingerprint,
                "seed": c.config.seed,
                "candidate_seat": bot,
                "policy_fingerprints_by_seat": if bot == 0 {
                    profiles.map(|params| identity::canonical_hash(&params).unwrap())
                } else {
                    [profiles[1], profiles[0]].map(|params| identity::canonical_hash(&params).unwrap())
                },
                "score_by_seat": final_scores,
                "match_winners": match_winners,
                "termination": if match_winners.is_empty() { "round_cap" } else { "scoreboard" },
                "round_cap": round_cap,
                "rounds": evaluated_rounds,
                "rejected_drafts_by_seat": rejected_drafts,
                "wall_seconds": started.elapsed().as_secs_f64(),
            })
        );
    }
}
