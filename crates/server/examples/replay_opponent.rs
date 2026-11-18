//! Evaluate each bot against one recorded human seat. Reads public spectator responses locally.
use atemporal_server::{
    adapter::SimThread,
    bots::{self, Style},
    controller::{Controller, RevisionData},
};
use atemporal_sim::*;
use std::{path::Path, sync::atomic::AtomicBool};

fn read(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read spectator export"))
        .expect("spectator JSON")
}

fn bot_turn(
    c: &Controller,
    bot: PlayerId,
    style: Style,
    ledger: &[AcceptedTurn],
    precedence: &[RoundPrecedence],
) -> AcceptedTurn {
    let round = c.round;
    let tick = bots::intervention(c, bot, style);
    let at = c.revisions[&c.current]
        .checkpoints
        .range(..=tick)
        .next_back()
        .unwrap()
        .1
        .clone();
    let prior = c.build_request(c.current, at, ledger.to_vec(), precedence.to_vec(), 0);
    let state = reconstruct(&prior, tick).unwrap();
    let draft = bots::published_plan(c, &state, bot, style);
    let resolved = draft::resolve_local_references(&draft, round, bot).unwrap();
    if let Err(error) = c.validate_against_state(bot, &resolved, &state) {
        eprintln!("Round {round} bot passed after its draft was rejected: {error}");
        return AcceptedTurn {
            round,
            player: bot,
            tick,
            commands: vec![],
            duration_ms: 1000u64.try_into().unwrap(),
        };
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
    AcceptedTurn {
        round,
        player: bot,
        tick,
        commands,
        duration_ms: 1000u64.try_into().unwrap(),
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) || args.get(4).is_some_and(|m| m != "--frozen") {
        panic!(
            "usage: replay_opponent EXPORT_DIRECTORY|seed:INTEGER tactical|standard 0|1 [--frozen] (bot seat)"
        );
    }
    let frozen = args.get(4).is_some();
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
    // A published opening is the empty ledger. Both seats plan against that same revision.
    for round in 0..=recorded.as_ref().map_or(12, |turns| {
        turns.iter().map(|t| t.round).max().unwrap_or_default()
    }) {
        if round > 0 {
            c.round = round;
            let turn = bot_turn(&c, bot, style, &ledger, &precedence);
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
                    None => bot_turn(&c, 1 - bot, Style::Standard, &ledger, &precedence),
                };
                ledger.extend([turn, other]);
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
            if !resolved.match_winners.is_empty() {
                break;
            }
            score = Some(resolved);
        }
        data.outcome = Some(result.outcome);
        data.turns = ledger.clone();
        c.current = round;
        c.revisions.clear();
        c.revisions.insert(round, data);
    }
}
