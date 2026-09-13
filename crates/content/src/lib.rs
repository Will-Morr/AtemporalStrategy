//! Shared normalized content, setup validation, and optional startup guide generation.
use atemporal_contracts::{identity::canonical_hash, scoring, *};
use std::collections::BTreeSet;
#[cfg(feature = "guide")]
mod guide;
#[cfg(feature = "guide")]
pub use guide::*;

fn nonnegative(n: f64, field: &str) -> Result<()> {
    if n.is_finite() && n >= 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be finite and nonnegative"))
    }
}
fn positive(n: f64, field: &str) -> Result<()> {
    if n.is_finite() && n > 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be finite and positive"))
    }
}
fn cooldown(n: Tick) -> Result<()> {
    if n > 0 {
        Ok(())
    } else {
        Err("cooldown must be positive".into())
    }
}
fn key_valid(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
/// Type and recipe arrays are sets. Initial roster order determines genesis slots.
pub fn normalize_content(mut content: Content) -> Result<Content> {
    if content.types.is_empty() {
        return Err("content roster is empty".into());
    }
    content.types.sort_by(|a, b| a.key.cmp(&b.key));
    let keys: BTreeSet<_> = content.types.iter().map(|t| t.key.clone()).collect();
    if keys.len() != content.types.len() {
        return Err("duplicate content key".into());
    }
    let unit_keys: BTreeSet<_> = content
        .types
        .iter()
        .filter(|t| t.kind == TypeKind::Unit)
        .map(|t| t.key.clone())
        .collect();
    for t in &mut content.types {
        if !key_valid(&t.key) {
            return Err(format!("invalid type key {}", t.key));
        }
        positive(t.matter_cost, "matter_cost")?;
        positive(t.max_hp, "max_hp")?;
        nonnegative(t.vision, "vision")?;
        if let Some(m) = &t.movement {
            cooldown(m.cooldown)?;
        }
        if t.kind == TypeKind::Structure && t.movement.is_some() {
            return Err(format!("structure {} cannot move", t.key));
        }
        if let Some(w) = &t.weapon {
            nonnegative(w.range, "weapon range")?;
            nonnegative(w.damage, "damage")?;
            cooldown(w.cooldown)?;
        }
        for w in [&t.mining, &t.construction].into_iter().flatten() {
            nonnegative(w.rate, "work rate")?;
            cooldown(w.cooldown)?;
        }
        if let Some(p) = &mut t.production {
            nonnegative(p.rate, "production rate")?;
            if p.recipes.is_empty() || p.recipes.iter().any(|key| !unit_keys.contains(key)) {
                return Err(format!(
                    "{} has empty or non-unit production recipes",
                    t.key
                ));
            }
            p.recipes.sort();
            if p.recipes.windows(2).any(|p| p[0] == p[1]) {
                return Err("duplicate recipe".into());
            }
        }
        if let Some(h) = &t.self_repair {
            positive(h.hp_per_matter, "self-repair efficiency")?;
            nonnegative(h.rate, "self-repair rate")?;
        }
        if let Some(h) = &t.healing {
            nonnegative(h.range, "healing range")?;
            positive(h.hp_per_matter, "healing efficiency")?;
            nonnegative(h.demand, "healing demand")?;
            cooldown(h.cooldown)?;
        }
    }
    if content.starting_roster.is_empty()
        || content
            .starting_roster
            .iter()
            .any(|key| !keys.contains(key))
    {
        return Err("starting roster references missing content".into());
    }
    Ok(content)
}
pub fn load_content(yaml: &str) -> Result<Content> {
    normalize_content(serde_yaml::from_str(yaml).map_err(|e| format!("content YAML: {e}"))?)
}
pub fn content_hash(content: &Content) -> Result<String> {
    canonical_hash(&normalize_content(content.clone())?)
}
/// Archived content must match its persisted fingerprint; never substitute current defaults.
pub fn load_archived_content(yaml: &str, expected_hash: &str) -> Result<Content> {
    let content = load_content(yaml)?;
    if content_hash(&content)? != expected_hash {
        return Err("archived content hash mismatch".into());
    }
    Ok(content)
}
pub fn validate_config(c: &MatchConfig) -> Result<()> {
    // Explicit v1 map-generator scope proposal, not a user-imposed player cap.
    if !(2..=4).contains(&c.player_count) || !(8..=512).contains(&c.map_size) {
        return Err(
            "current configuration supports 2–4 players and square maps of 8–512 tiles".into(),
        );
    }
    if c.symmetric && (c.player_count == 3 || !c.map_size.is_multiple_of(2)) {
        return Err("symmetric maps require 2 or 4 players and an even map size".into());
    }
    scoring::sides(c.player_count, &c.multiplayer)?;
    match &c.objective {
        Objective::Scoreboard { rules } => scoring::validate_rules(rules)?,
        Objective::Hybrid {
            rules,
            lock_ticks_per_round,
        } => {
            scoring::validate_rules(rules)?;
            cooldown(*lock_ticks_per_round)?;
        }
        Objective::Timed {
            lock_ticks_per_round,
        } => {
            cooldown(*lock_ticks_per_round)?;
        }
    }
    if let Some(w) = c.future_orders.window_ticks {
        cooldown(w)?;
    }
    for n in [
        c.max_tick,
        c.stall_ticks,
        c.ticks_per_second,
        c.checkpoint_interval,
        c.snapshot_interval,
        u32::from(c.simulation_threads),
    ] {
        cooldown(n)?;
    }
    nonnegative(c.starting_matter, "starting_matter")?;
    nonnegative(c.ore_matter_per_start, "ore_matter_per_start")?;
    if c.replay_directory.trim().is_empty() {
        return Err("replay directory is empty".into());
    }
    Ok(())
}
pub fn load_setup(yaml: &str) -> Result<Setup> {
    let mut setup: Setup = serde_yaml::from_str(yaml).map_err(|e| format!("setup YAML: {e}"))?;
    validate_config(&setup.match_defaults)?;
    if setup.default_port == 0 {
        return Err("port must be 1..65535".into());
    }
    setup
        .available_teams
        .sort_by(|a, b| a.team_id.cmp(&b.team_id));
    if setup
        .available_teams
        .windows(2)
        .any(|t| t[0].team_id == t[1].team_id)
    {
        return Err("duplicate available team".into());
    }
    for t in &setup.available_teams {
        if !key_valid(&t.team_id) || t.label.trim().is_empty() || t.capacity == Some(0) {
            return Err("invalid available team".into());
        }
    }
    if let Multiplayer::Teams { assignments } = &setup.match_defaults.multiplayer {
        for a in assignments {
            let t = setup
                .available_teams
                .iter()
                .find(|t| t.team_id == a.team_id)
                .ok_or("default assignment references unavailable team")?;
            if t.capacity.is_some_and(|cap| {
                assignments
                    .iter()
                    .filter(|a| a.team_id == t.team_id)
                    .count()
                    > usize::from(cap)
            }) {
                return Err("default team exceeds capacity".into());
            }
        }
    }
    Ok(setup)
}
