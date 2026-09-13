//! SHA-256 causal identities and canonical JSON hashing (same pinned build/platform).
use crate::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub fn command_id(round: u32, player: PlayerId, index: u32) -> CommandId {
    format!("r{round}:p{player}:c{index}")
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
fn register(
    registry: &mut IdentityRegistry,
    kind: &str,
    parts: serde_json::Value,
) -> Result<String> {
    let preimage = String::from_utf8(canonical_json(&serde_json::json!([
        "atemporal",
        1,
        kind,
        parts
    ]))?)
    .map_err(|e| e.to_string())?;
    let id = format!("{kind}:{}", sha256(preimage.as_bytes()));
    if let Some(old) = registry.get(&id) {
        if old != &preimage {
            return Err("causal identity collision".into());
        }
    } else {
        registry.insert(id.clone(), preimage);
    }
    Ok(id)
}
pub fn genesis(registry: &mut IdentityRegistry, player: PlayerId, slot: u32) -> Result<EntityId> {
    register(
        registry,
        "entity",
        serde_json::json!(["genesis", player, slot]),
    )
}
pub fn blueprint(
    registry: &mut IdentityRegistry,
    command: &str,
    tile_index: u32,
) -> Result<BlueprintId> {
    register(
        registry,
        "blueprint",
        serde_json::json!([command, tile_index]),
    )
}
pub fn structure(registry: &mut IdentityRegistry, blueprint: &str) -> Result<EntityId> {
    register(
        registry,
        "entity",
        serde_json::json!(["structure", blueprint]),
    )
}
pub fn queue_item(
    registry: &mut IdentityRegistry,
    factory: &str,
    command: &str,
    item_index: u32,
) -> Result<QueueItemId> {
    register(
        registry,
        "queue",
        serde_json::json!([factory, command, item_index]),
    )
}
pub fn production(
    registry: &mut IdentityRegistry,
    factory: &str,
    item: &str,
    occurrence: u32,
) -> Result<EntityId> {
    register(
        registry,
        "entity",
        serde_json::json!(["production", factory, item, occurrence]),
    )
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
    for e in &state.entities {
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
    for (id, preimage) in &state.deterministic_identity_state {
        let prefix = id.split(':').next().ok_or("invalid identity")?;
        if !["entity", "blueprint", "queue"].contains(&prefix)
            || *id != format!("{prefix}:{}", sha256(preimage.as_bytes()))
        {
            return Err("invalid causal identity registry".into());
        }
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
