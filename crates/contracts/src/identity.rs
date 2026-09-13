//! Fixed-size causal identities and SHA-256 canonical JSON hashing (same pinned build/platform).
use crate::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub fn command_id(round: u32, player: PlayerId, index: u32) -> CommandId {
    CommandId {
        round,
        player,
        index,
    }
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Objects are key-sorted by serde_json's default BTreeMap; arrays retain semantic order.
/// Call the boundary's semantic validator first (in particular finite-float validation).
pub fn canonical_json(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    fn normalize(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Number(n) if n.is_f64() && n.as_f64() == Some(0.0) => {
                *v = serde_json::json!(0.0)
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(normalize),
            serde_json::Value::Object(m) => m.values_mut().for_each(normalize),
            _ => {}
        }
    }
    normalize(&mut value);
    serde_json::to_vec(&value).map_err(|e| e.to_string())
}
pub fn canonical_hash(value: &impl Serialize) -> Result<String> {
    Ok(sha256(&canonical_json(value)?))
}
pub fn genesis(player: PlayerId, slot: u32) -> Result<EntityId> {
    blueprint(&command_id(0, player, 0), slot)
}
pub fn blueprint(command: &CommandId, tile_index: u32) -> Result<BlueprintId> {
    Ok(EntityId {
        birth_command: BirthCommandId {
            command: command.clone(),
            target_index: 0,
        },
        item_index: u16::try_from(tile_index).map_err(|_| "blueprint index exceeds u16")?,
        occurrence: 0,
    })
}
pub fn structure(blueprint: &BlueprintId) -> EntityId {
    blueprint.clone()
}
/// target_index is in the complete sorted/frozen factory list, including absent factories.
pub fn queue_item(command: &CommandId, target_index: u32, item_index: u32) -> Result<QueueItemId> {
    if command.round == 0 {
        return Err("round zero is reserved for genesis".into());
    }
    Ok(QueueItemId {
        birth_command: BirthCommandId {
            command: command.clone(),
            target_index: u16::try_from(target_index).map_err(|_| "target index exceeds u16")?,
        },
        item_index: u16::try_from(item_index).map_err(|_| "item index exceeds u16")?,
    })
}
pub fn production(item: &QueueItemId, occurrence: u32) -> EntityId {
    EntityId {
        birth_command: item.birth_command.clone(),
        item_index: item.item_index,
        occurrence,
    }
}
/// Normalize sets only; never reorder queues, terrain cells, or command item/tile arrays.
pub fn canonical_world(state: &WorldState) -> Result<WorldState> {
    let mut state = state.clone();
    let cells = usize::from(state.terrain.width) * usize::from(state.terrain.height);
    if cells == 0 || state.terrain.cells.len() != cells || state.ore.len() != cells {
        return Err("terrain and ore must be matching nonempty row-major grids".into());
    }
    if state.ore.iter().any(|x| !x.is_finite() || *x < 0.0) {
        return Err("invalid ore".into());
    }
    state.players.sort_by_key(|p| p.player_id);
    state.entities.sort_by(|a, b| a.id.cmp(&b.id));
    state.blueprints.sort_by(|a, b| a.id.cmp(&b.id));
    state.control_groups.sort_by(|a, b| a.id.cmp(&b.id));
    if state
        .players
        .windows(2)
        .any(|p| p[0].player_id == p[1].player_id)
        || state.entities.windows(2).any(|p| p[0].id == p[1].id)
        || state.blueprints.windows(2).any(|p| p[0].id == p[1].id)
        || state.control_groups.windows(2).any(|p| p[0].id == p[1].id)
    {
        return Err("duplicate world identity".into());
    }
    for p in &mut state.players {
        let c = &p.counters;
        if [
            p.bank,
            c.mined,
            c.total_spend,
            c.unit_spend,
            c.structure_spend,
            c.lost_invested_matter,
            c.destroyed_replacement_value,
            c.damage_dealt,
        ]
        .iter()
        .any(|n| !n.is_finite() || *n < 0.0)
        {
            return Err("invalid player resources/statistics".into());
        }
        p.elimination_reasons.sort_by_key(|r| match r {
            Reason::NoActiveBuilding => 0,
            Reason::NoBuildAbility => 1,
        });
        p.elimination_reasons.dedup();
    }
    for e in &mut state.entities {
        crate::locks::canonicalize(&mut e.order_locks, state.tick)?;
        if let Some(production) = &mut e.production {
            production
                .occurrence_counters
                .sort_by(|a, b| a.item_id.cmp(&b.item_id));
            if production
                .occurrence_counters
                .windows(2)
                .any(|p| p[0].item_id == p[1].item_id)
            {
                return Err("duplicate occurrence counter".into());
            }
        }
        if !e.hp.is_finite() || !e.paid_matter.is_finite() || e.hp <= 0.0 || e.paid_matter < 0.0 {
            return Err("invalid entity health/payment".into());
        }
        if e.tile.x >= state.terrain.width || e.tile.y >= state.terrain.height {
            return Err("entity out of bounds".into());
        }
        if let Some(a) = e.production.as_ref().and_then(|p| p.active_item.as_ref())
            && (!a.paid_matter.is_finite() || a.paid_matter < 0.0)
        {
            return Err("invalid production payment".into());
        }
    }
    let mut occupied = BTreeSet::new();
    for e in &state.entities {
        if !occupied.insert(e.tile) {
            return Err("duplicate occupancy".into());
        }
    }
    for g in &mut state.control_groups {
        crate::locks::canonicalize(&mut g.order_locks, state.tick)?;
        g.members.sort();
        g.members.dedup();
    }
    state
        .survival_transitions
        .sort_by_key(|t| (t.resolved_tick, t.player_id));
    if state
        .survival_transitions
        .windows(2)
        .any(|t| t[0].resolved_tick == t[1].resolved_tick && t[0].player_id == t[1].player_id)
    {
        return Err("duplicate survival transition".into());
    }
    for transition in &mut state.survival_transitions {
        if transition.resolved_tick >= state.tick {
            return Err("survival transition is not in checkpoint history".into());
        }
        transition.reasons.sort_by_key(|r| match r {
            Reason::NoActiveBuilding => 0,
            Reason::NoBuildAbility => 1,
        });
        transition.reasons.dedup();
    }
    Ok(state)
}
pub fn world_hash(state: &WorldState) -> Result<String> {
    canonical_hash(&canonical_world(state)?)
}
/// Persist this vector with each round; arrival timing must never choose precedence.
pub fn round_precedence(round: u32, player_count: u8) -> Result<RoundPrecedence> {
    if player_count < 2 {
        return Err("at least two players required".into());
    }
    let offset = round % u32::from(player_count);
    Ok(RoundPrecedence {
        round,
        players: (0..player_count)
            .map(|p| ((u32::from(p) + offset) % u32::from(player_count)) as u8)
            .collect(),
    })
}
