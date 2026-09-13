//! Locked-state timed adjudication; never infer a timed loss from a mutable sim endpoint.
use crate::{scoring, *};
use std::collections::BTreeSet;
pub fn adjudicate(
    config: &MatchConfig,
    content: &Content,
    old_boundary: Tick,
    locked: &WorldState,
    previously_lost: &[PlayerId],
) -> Result<TimedAdjudication> {
    let lock_ticks_per_round = match config.objective {
        Objective::Timed {
            lock_ticks_per_round,
        }
        | Objective::Hybrid {
            lock_ticks_per_round,
            ..
        } => lock_ticks_per_round,
        _ => return Err("timed adjudication requires an advancing-history configuration".into()),
    };
    if lock_ticks_per_round == 0 || old_boundary > config.max_tick {
        return Err("invalid timed boundary/increment".into());
    }
    let boundary = old_boundary
        .saturating_add(lock_ticks_per_round)
        .min(config.max_tick);
    if locked.tick != boundary {
        return Err("adjudication requires exact S[new_L]".into());
    }
    let sides = scoring::sides(config.player_count, &config.multiplayer)?;
    if matches!(config.objective, Objective::Hybrid { .. }) {
        return Ok(TimedAdjudication {
            boundary,
            timed_lost_players: vec![],
            eligible_sides: sides.into_keys().collect(),
            match_winners: vec![],
            status: if boundary == config.max_tick {
                TimedStatus::HistoryExhausted
            } else {
                TimedStatus::Planning
            },
        });
    }
    let mut lost: BTreeSet<_> = previously_lost.iter().copied().collect();
    if lost.len() != previously_lost.len() || lost.iter().any(|p| *p >= config.player_count) {
        return Err("invalid previous timed losses".into());
    }
    let mut capable = BTreeSet::new();
    for entity in &locked.entities {
        if entity.owner >= config.player_count || !entity.hp.is_finite() {
            return Err("invalid locked entity".into());
        }
        let definition = content
            .types
            .iter()
            .find(|t| t.key == entity.type_key)
            .ok_or("unknown locked entity type")?;
        if entity.hp > 0.
            && entity.lifecycle == Lifecycle::Complete
            && definition.provides_build_ability
        {
            capable.insert(entity.owner);
        }
    }
    if capable.iter().any(|p| lost.contains(p)) {
        return Err(
            "locked history restores a previously timed-lost player; finalization premise changed"
                .into(),
        );
    }
    for p in 0..config.player_count {
        if !capable.contains(&p) {
            lost.insert(p);
        }
    }
    let eligible_sides: Vec<_> = sides
        .into_iter()
        .filter(|(_, players)| players.iter().any(|p| !lost.contains(p)))
        .map(|(side, _)| side)
        .collect();
    let status = if eligible_sides.len() <= 1 {
        TimedStatus::Finished
    } else if boundary == config.max_tick {
        TimedStatus::HistoryExhausted
    } else {
        TimedStatus::Planning
    };
    let match_winners = if eligible_sides.len() == 1 {
        eligible_sides.clone()
    } else {
        vec![]
    };
    Ok(TimedAdjudication {
        boundary,
        timed_lost_players: lost.into_iter().collect(),
        eligible_sides,
        match_winners,
        status,
    })
}
