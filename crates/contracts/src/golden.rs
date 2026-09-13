//! Reusable acceptance comparator; callers supply states/results from the real engine.
use crate::*;
use std::collections::BTreeMap;
pub fn verify(
    fixture: &GoldenWorldFixture,
    states: &[WorldState],
    outcome: &Outcome,
    commands: &[CommandOutcome],
) -> Result<()> {
    let fail = |message: &str| format!("{}: {message}", fixture.name);
    if *outcome != fixture.expected.outcome {
        return Err(fail("endpoint outcome differs"));
    }
    let by_tick: BTreeMap<_, _> = states.iter().map(|state| (state.tick, state)).collect();
    if by_tick.len() != states.len() {
        return Err(fail("duplicate state ticks"));
    }
    let by_command: BTreeMap<_, _> = commands.iter().map(|c| (&c.command_id, c)).collect();
    if by_command.len() != commands.len()
        || commands.len() != fixture.expected.command_outcomes.len()
    {
        return Err(fail("unexpected or duplicate command outcomes"));
    }
    for expected in &fixture.expected.command_outcomes {
        if by_command.get(&expected.command_id).copied() != Some(expected) {
            return Err(fail(&format!("command {:?} differs", expected.command_id)));
        }
    }
    for expected in &fixture.expected.states {
        let state = by_tick
            .get(&expected.state_tick)
            .ok_or_else(|| fail(&format!("missing exact S[{}]", expected.state_tick)))?;
        for e in &expected.entities {
            let actual = state
                .entities
                .iter()
                .find(|actual| actual.id == e.entity_id);
            if actual.is_some() != e.present {
                return Err(fail(&format!(
                    "entity {:?} presence differs at S[{}]",
                    e.entity_id, state.tick
                )));
            }
            if let Some(actual) = actual
                && (e.tile.is_some_and(|v| actual.tile != v)
                    || e.hp.is_some_and(|v| actual.hp != v)
                    || e.paid_matter.is_some_and(|v| actual.paid_matter != v)
                    || e.action.as_ref().is_some_and(|v| actual.action != *v))
            {
                return Err(fail(&format!(
                    "entity {:?} state differs at S[{}]",
                    e.entity_id, state.tick
                )));
            }
        }
        for p in &expected.banks {
            let actual = state
                .players
                .iter()
                .find(|actual| actual.player_id == p.player_id)
                .ok_or_else(|| fail("missing player"))?;
            if actual.bank != p.bank || actual.counters.mined != p.mined {
                return Err(fail(&format!(
                    "player {} economy differs at S[{}]",
                    p.player_id, state.tick
                )));
            }
        }
        for g in &expected.groups {
            if state.control_groups.iter().find(|actual| actual.id == g.id) != Some(g) {
                return Err(fail(&format!("group differs at S[{}]", state.tick)));
            }
        }
    }
    if let Some(expected) = &fixture.expected.final_hash {
        let state = by_tick
            .get(&outcome.terminal_state_tick)
            .ok_or_else(|| fail("missing final state for hash assertion"))?;
        if identity::world_hash(state)? != *expected {
            return Err(fail("final hash differs"));
        }
    }
    Ok(())
}
