//! Pure controller reduction. Persist returned RoundScore atomically before publication.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn validate_rules(rules: &ScoreboardRules) -> Result<()> {
    let threshold = match rules.victory_rule {
        VictoryRule::FixedTarget { points } => points,
        VictoryRule::Lead { margin } => margin,
    };
    if !threshold.is_finite() || threshold <= 0.0 {
        return Err("score target/margin must be finite and positive".into());
    }
    Ok(())
}
pub fn sides(player_count: u8, mode: &Multiplayer) -> Result<BTreeMap<SideId, Vec<PlayerId>>> {
    if player_count < 2 {
        return Err("at least two participants required".into());
    }
    let mut sides = BTreeMap::<SideId, Vec<PlayerId>>::new();
    match mode {
        Multiplayer::Ffa {} => {
            for p in 0..player_count {
                sides.insert(SideId::Player { player_id: p }, vec![p]);
            }
        }
        Multiplayer::Teams { assignments } => {
            let mut seen = BTreeSet::new();
            for a in assignments {
                if a.player_id >= player_count
                    || a.team_id.trim().is_empty()
                    || !seen.insert(a.player_id)
                {
                    return Err("invalid/duplicate team assignment".into());
                }
                sides
                    .entry(SideId::Team {
                        team_id: a.team_id.clone(),
                    })
                    .or_default()
                    .push(a.player_id);
            }
            if seen.len() != usize::from(player_count) || sides.len() < 2 {
                return Err("assign every player to at least two nonempty teams".into());
            }
            for players in sides.values_mut() {
                players.sort();
            }
        }
    }
    Ok(sides)
}
pub fn classify(player_count: u8, survivors: &[PlayerId]) -> Result<OutcomeKind> {
    if player_count < 2
        || survivors.iter().any(|p| *p >= player_count)
        || survivors.iter().collect::<BTreeSet<_>>().len() != survivors.len()
    {
        return Err("invalid survivor set".into());
    }
    Ok(if survivors.is_empty() {
        OutcomeKind::Draw
    } else if survivors.len() == usize::from(player_count) {
        OutcomeKind::Stalemate
    } else {
        OutcomeKind::Win
    })
}
pub fn validate_outcome(player_count: u8, mode: &Multiplayer, outcome: &Outcome) -> Result<()> {
    if classify(player_count, &outcome.survivors)? != outcome.kind {
        return Err("outcome kind disagrees with final survivors".into());
    }
    let survivors: BTreeSet<_> = outcome.survivors.iter().copied().collect();
    let mut eliminated = outcome.eliminated.clone();
    eliminated.sort();
    let expected: Vec<_> = (0..player_count)
        .filter(|p| !survivors.contains(p))
        .collect();
    if eliminated != expected {
        return Err("survivors/eliminated must partition participants".into());
    }
    let expected: BTreeSet<_> = sides(player_count, mode)?
        .into_iter()
        .filter(|(_, ps)| ps.iter().any(|p| survivors.contains(p)))
        .map(|(side, _)| side)
        .collect();
    if outcome.surviving_sides.len() != expected.len()
        || outcome
            .surviving_sides
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            != expected
    {
        return Err("surviving sides disagree with participants".into());
    }
    if outcome.last_progress_tick > outcome.terminal_state_tick {
        return Err("progress lies after outcome".into());
    }
    Ok(())
}
fn timings(player_count: u8, times: &[PlayerTime]) -> Result<BTreeMap<PlayerId, f64>> {
    let mut result = BTreeMap::new();
    for t in times {
        if t.player_id >= player_count
            || result
                .insert(t.player_id, t.total_ms.get().max(1000) as f64)
                .is_some()
        {
            return Err("invalid/duplicate timing player".into());
        }
    }
    if result.len() != usize::from(player_count) {
        return Err("timings must include every player".into());
    }
    Ok(result)
}
pub fn time_ratios(player_count: u8, times: &[PlayerTime]) -> Result<Vec<PlayerRatio>> {
    let times = timings(player_count, times)?;
    let fastest = times.values().copied().fold(f64::INFINITY, f64::min);
    Ok(times
        .into_iter()
        .map(|(player_id, t)| PlayerRatio {
            player_id,
            ratio: t / fastest,
        })
        .collect())
}
/// Reduce a complete vector, then test victory. Passes intentionally follow the same path.
/// `previous` is the last durable result; round IDs start at 1 and must be consecutive.
/// A retry of that last round returns its original record. The archive owns payload/revision
/// idempotency and older-round lookup; this function does not claim durable storage.
pub fn resolve_round(
    round: u32,
    player_count: u8,
    mode: &Multiplayer,
    rules: &ScoreboardRules,
    outcome: &Outcome,
    times: &[PlayerTime],
    previous: Option<&RoundScore>,
) -> Result<RoundScore> {
    validate_rules(rules)?;
    let side_members = sides(player_count, mode)?;
    validate_outcome(player_count, mode, outcome)?;
    let times = timings(player_count, times)?;
    if let Some(prev) = previous {
        if prev.victory_rule != rules.victory_rule {
            return Err("victory rule changed in score history".into());
        }
        if prev.entries.len() != side_members.len()
            || prev
                .entries
                .iter()
                .map(|e| &e.side_id)
                .collect::<BTreeSet<_>>()
                != side_members.keys().collect()
        {
            return Err("score history sides changed".into());
        }
        if prev.entries.iter().any(|e| {
            [e.raw_total, e.adjusted_total]
                .iter()
                .any(|n| !n.is_finite() || *n < 0.0)
        }) {
            return Err("invalid prior totals".into());
        }
        if round == prev.round {
            return Ok(prev.clone());
        }
        if prev.round.checked_add(1) != Some(round) {
            return Err("round must follow last durable result".into());
        }
        if !prev.match_winners.is_empty() {
            return Err("scoreboard match already finished".into());
        }
    } else if round != 1 {
        return Err("first scored round must be 1".into());
    }
    let mut entries = Vec::new();
    for (side_id, members) in &side_members {
        let surviving_members: Vec<_> = members
            .iter()
            .copied()
            .filter(|p| outcome.survivors.contains(p))
            .collect();
        let (credited_players, award_reason) = match outcome.kind {
            OutcomeKind::Win => (
                surviving_members.clone(),
                if surviving_members.is_empty() {
                    AwardReason::None
                } else {
                    AwardReason::Survival
                },
            ),
            OutcomeKind::Draw if rules.draw_scoring == DrawScoring::AllPlayers => {
                (members.clone(), AwardReason::Draw)
            }
            _ => (vec![], AwardReason::None),
        };
        let raw_delta = credited_players.len() as f64;
        let adjusted_delta = credited_players
            .iter()
            .map(|p| {
                if rules.time_penalty == TimePenalty::None {
                    return 1.0;
                }
                let fastest_opponent = times
                    .iter()
                    .filter(|(id, _)| !members.contains(id))
                    .map(|(_, t)| *t)
                    .fold(f64::INFINITY, f64::min);
                (fastest_opponent / times[p]).min(1.0)
            })
            .sum::<f64>();
        let old = previous.and_then(|s| s.entries.iter().find(|e| e.side_id == *side_id));
        let raw_total = old.map_or(0.0, |e| e.raw_total) + raw_delta;
        let adjusted_total = old.map_or(0.0, |e| e.adjusted_total) + adjusted_delta;
        if !raw_total.is_finite() || !adjusted_total.is_finite() {
            return Err("score overflow".into());
        }
        entries.push(ScoreEntry {
            side_id: side_id.clone(),
            surviving_members,
            credited_players,
            award_reason,
            raw_delta,
            adjusted_delta,
            raw_total,
            adjusted_total,
        });
    }
    let total = |e: &ScoreEntry| {
        if rules.time_penalty == TimePenalty::None {
            e.raw_total
        } else {
            e.adjusted_total
        }
    };
    let highest = entries.iter().map(&total).fold(f64::NEG_INFINITY, f64::max);
    let leaders: Vec<_> = entries.iter().filter(|e| total(e) == highest).collect();
    let match_winners = match rules.victory_rule {
        VictoryRule::FixedTarget { points }
            if highest >= points
                && (leaders.len() == 1 || rules.tie_policy == TiePolicy::SharedVictory) =>
        {
            leaders.iter().map(|e| e.side_id.clone()).collect()
        }
        VictoryRule::Lead { margin } if leaders.len() == 1 => {
            let leader = leaders[0];
            let rival = entries
                .iter()
                .filter(|e| e.side_id != leader.side_id)
                .map(&total)
                .fold(f64::NEG_INFINITY, f64::max);
            if highest - rival >= margin {
                vec![leader.side_id.clone()]
            } else {
                vec![]
            }
        }
        _ => vec![],
    };
    Ok(RoundScore {
        round,
        entries,
        victory_rule: rules.victory_rule.clone(),
        match_winners,
    })
}
