use atemporal_contracts::{scoring::*, *};
fn outcome(n: u8, mode: &Multiplayer, alive: &[u8]) -> Outcome {
    Outcome {
        kind: classify(n, alive).unwrap(),
        stop_reason: StopReason::Inactivity,
        terminal_state_tick: 40,
        last_progress_tick: 10,
        survivors: alive.to_vec(),
        eliminated: (0..n).filter(|p| !alive.contains(p)).collect(),
        surviving_sides: sides(n, mode)
            .unwrap()
            .into_iter()
            .filter(|(_, ps)| ps.iter().any(|p| alive.contains(p)))
            .map(|(s, _)| s)
            .collect(),
        survival_transitions: vec![],
    }
}
fn times(n: u8) -> Vec<PlayerTime> {
    (0..n)
        .map(|player_id| PlayerTime {
            player_id,
            total_ms: 1000.try_into().unwrap(),
        })
        .collect()
}
fn teams() -> Multiplayer {
    Multiplayer::Teams {
        assignments: (0..4)
            .map(|p| TeamAssignment {
                player_id: p,
                team_id: if p < 2 { "a" } else { "b" }.into(),
            })
            .collect(),
    }
}
#[test]
fn unchanged_pass_rounds_award_once_and_default_ends_at_five() {
    let rules = ScoreboardRules::default();
    let o = outcome(2, &Multiplayer::Ffa {}, &[0]);
    let mut prev = None;
    for round in 1..=5 {
        let result = resolve_round(
            round,
            2,
            &Multiplayer::Ffa {},
            &rules,
            &o,
            &times(2),
            prev.as_ref(),
        )
        .unwrap();
        assert_eq!(result.entries[0].raw_total, round as f64);
        assert_eq!(result.match_winners.len(), usize::from(round == 5));
        let retry = resolve_round(
            round,
            2,
            &Multiplayer::Ffa {},
            &rules,
            &o,
            &times(2),
            Some(&result),
        )
        .unwrap();
        assert_eq!(retry, result);
        prev = Some(result);
    }
    assert!(
        resolve_round(
            6,
            2,
            &Multiplayer::Ffa {},
            &rules,
            &o,
            &times(2),
            prev.as_ref()
        )
        .is_err()
    );
}
#[test]
fn teams_stalemate_partial_win_and_both_draw_policies() {
    let mode = teams();
    for (alive, draw, expected) in [
        (vec![0, 1, 2, 3], DrawScoring::AllPlayers, [0., 0.]),
        (vec![0, 2, 3], DrawScoring::None, [1., 2.]),
        (vec![], DrawScoring::None, [0., 0.]),
        (vec![], DrawScoring::AllPlayers, [2., 2.]),
    ] {
        let rules = ScoreboardRules {
            draw_scoring: draw,
            ..Default::default()
        };
        let s = resolve_round(
            1,
            4,
            &mode,
            &rules,
            &outcome(4, &mode, &alive),
            &times(4),
            None,
        )
        .unwrap();
        assert_eq!(
            s.entries.iter().map(|e| e.raw_delta).collect::<Vec<_>>(),
            expected
        );
        if alive.is_empty() {
            assert!(s.entries.iter().all(|e| e.surviving_members.is_empty()));
        }
    }
}
#[test]
fn simultaneous_threshold_ties_and_shared_victory() {
    let mode = Multiplayer::Ffa {};
    for policy in [TiePolicy::ContinueUntilUnique, TiePolicy::SharedVictory] {
        let rules = ScoreboardRules {
            tie_policy: policy,
            ..Default::default()
        };
        let o = outcome(3, &mode, &[0, 1]);
        let mut prev = None;
        for round in 1..=5 {
            prev =
                Some(resolve_round(round, 3, &mode, &rules, &o, &times(3), prev.as_ref()).unwrap());
        }
        let s = prev.unwrap();
        assert_eq!(
            s.match_winners.len(),
            if policy == TiePolicy::SharedVictory {
                2
            } else {
                0
            }
        );
        if policy == TiePolicy::ContinueUntilUnique {
            let next = resolve_round(
                6,
                3,
                &mode,
                &rules,
                &outcome(3, &mode, &[0]),
                &times(3),
                Some(&s),
            )
            .unwrap();
            assert_eq!(
                next.entries.iter().map(|e| e.raw_total).collect::<Vec<_>>(),
                [6., 5., 0.]
            );
            assert_eq!(next.match_winners, vec![SideId::Player { player_id: 0 }]);
        }
    }
}
#[test]
fn lead_compares_strongest_rival_and_ignores_shared_tie_policy() {
    let rules = ScoreboardRules {
        victory_rule: VictoryRule::Lead { margin: 2. },
        tie_policy: TiePolicy::SharedVictory,
        ..Default::default()
    };
    let mode = Multiplayer::Ffa {};
    let mut prev = None;
    for round in 1..=8 {
        prev = Some(
            resolve_round(
                round,
                3,
                &mode,
                &rules,
                &outcome(3, &mode, &[0, 1]),
                &times(3),
                prev.as_ref(),
            )
            .unwrap(),
        );
    }
    assert!(prev.as_ref().unwrap().match_winners.is_empty());
    for round in 9..=10 {
        prev = Some(
            resolve_round(
                round,
                3,
                &mode,
                &rules,
                &outcome(3, &mode, &[0]),
                &times(3),
                prev.as_ref(),
            )
            .unwrap(),
        );
        assert_eq!(
            prev.as_ref().unwrap().match_winners.len(),
            usize::from(round == 10)
        );
    }
}
#[test]
fn penalty_uses_opposing_players_and_totals_not_teammates() {
    let mode = teams();
    let rules = ScoreboardRules {
        time_penalty: TimePenalty::FastestOpponentRatio,
        ..Default::default()
    };
    let ts = vec![
        PlayerTime {
            player_id: 0,
            total_ms: 10000.try_into().unwrap(),
        },
        PlayerTime {
            player_id: 1,
            total_ms: 1000.try_into().unwrap(),
        },
        PlayerTime {
            player_id: 2,
            total_ms: 5000.try_into().unwrap(),
        },
        PlayerTime {
            player_id: 3,
            total_ms: 20000.try_into().unwrap(),
        },
    ];
    let result = resolve_round(
        1,
        4,
        &mode,
        &rules,
        &outcome(4, &mode, &[0, 2, 3]),
        &ts,
        None,
    )
    .unwrap();
    assert_eq!(result.entries[0].raw_delta, 1.);
    assert_eq!(result.entries[0].adjusted_delta, 0.5);
    assert_eq!(result.entries[1].adjusted_delta, 0.25);
    assert_eq!(time_ratios(4, &ts).unwrap()[3].ratio, 20.);
}
#[test]
fn recovery_stalemate_does_not_score_and_malformed_inputs_fail() {
    let mode = Multiplayer::Ffa {};
    let rules = ScoreboardRules::default();
    let mut o = outcome(2, &mode, &[0, 1]);
    o.survival_transitions = vec![
        SurvivalTransition {
            player_id: 0,
            resolved_tick: 1,
            status: SurvivalStatus::Eliminated,
            reasons: vec![Reason::NoActiveBuilding],
        },
        SurvivalTransition {
            player_id: 0,
            resolved_tick: 3,
            status: SurvivalStatus::Alive,
            reasons: vec![],
        },
    ];
    assert!(
        resolve_round(1, 2, &mode, &rules, &o, &times(2), None)
            .unwrap()
            .entries
            .iter()
            .all(|e| e.raw_delta == 0.)
    );
    o.survivors = vec![0, 0];
    assert!(resolve_round(1, 2, &mode, &rules, &o, &times(2), None).is_err());
    assert!(
        validate_rules(&ScoreboardRules {
            victory_rule: VictoryRule::FixedTarget { points: f64::NAN },
            ..rules.clone()
        })
        .is_err()
    );
    assert!(
        resolve_round(
            2,
            2,
            &mode,
            &rules,
            &outcome(2, &mode, &[0]),
            &times(2),
            None
        )
        .is_err()
    );
    assert!(resolve_round(1, 2, &mode, &rules, &outcome(2, &mode, &[0]), &[], None).is_err());
    assert!(
        sides(
            4,
            &Multiplayer::Teams {
                assignments: vec![]
            }
        )
        .is_err()
    );
}
