//! Shared normalized content, setup validation, and content-keyed static guides.
use atemporal_contracts::{
    identity::{canonical_hash, sha256},
    scoring, *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

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
        return Err("v1 supports 2–4 players and square maps of 8–512 tiles".into());
    }
    if c.symmetric && (c.player_count == 3 || !c.map_size.is_multiple_of(2)) {
        return Err("symmetric v1 maps require 2 or 4 players and an even map size".into());
    }
    scoring::sides(c.player_count, &c.multiplayer)?;
    match &c.objective {
        Objective::Scoreboard { rules } => scoring::validate_rules(rules)?,
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
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn readable_range(value: f64) -> String {
    if (value * 1000.0).round() / 1000.0 == value {
        return value.to_string();
    }
    format!("≈{value:.3}")
}
fn work(rate: &Option<WorkRate>) -> String {
    rate.as_ref().map_or_else(
        || "—".into(),
        |w| format!("{} / {} ticks", w.rate, w.cooldown),
    )
}
/// Prose is trusted repository-authored HTML. All content-derived labels are escaped.
pub fn render_guide(content: &Content, prose: &str) -> Result<String> {
    let content = normalize_content(content.clone())?;
    let mut rows = String::new();
    for t in &content.types {
        let movement = t.movement.as_ref().map_or_else(
            || "Static".into(),
            |m| {
                format!(
                    "{} neighbors / {} ticks",
                    if m.neighbors == Neighbors::Four { 4 } else { 8 },
                    m.cooldown
                )
            },
        );
        let weapon = t.weapon.as_ref().map_or_else(
            || "—".into(),
            |w| {
                format!(
                    "{} damage / {} range / {} ticks; {}",
                    w.damage,
                    readable_range(w.range),
                    w.cooldown,
                    if w.indirect { "indirect" } else { "direct" }
                )
            },
        );
        let production = t.production.as_ref().map_or_else(
            || "—".into(),
            |p| format!("{} / tick; {}", p.rate, p.recipes.join(", ")),
        );
        let healing = t.healing.as_ref().map_or_else(
            || "—".into(),
            |h| {
                format!(
                    "{} HP/matter; {} demand / {} ticks; range {}",
                    h.hp_per_matter, h.demand, h.cooldown, h.range
                )
            },
        );
        let values = vec![
            t.key.clone(),
            format!("{:?}", t.kind),
            t.matter_cost.to_string(),
            t.max_hp.to_string(),
            t.vision.to_string(),
            movement,
            weapon,
            work(&t.mining),
            work(&t.construction),
            production,
            healing,
            format!("{} / {}", t.counts_for_survival, t.provides_build_ability),
        ];
        rows.push_str("<tr>");
        for value in values {
            rows.push_str(&format!("<td>{}</td>", escape(&value)));
        }
        rows.push_str("</tr>\n");
    }
    let hash = content_hash(&content)?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Atemporal Strategy — How to play</title>
<style>body{{font:17px/1.6 system-ui;margin:2rem auto;max-width:1000px;padding:0 1rem;color:#eee;background:#222}}a{{color:#79d7ff}}.table-scroll{{overflow:auto}}table{{border-collapse:collapse;font-size:.85rem}}th,td{{border:1px solid #666;padding:.5rem;text-align:left;vertical-align:top}}th{{background:#333}}code{{overflow-wrap:anywhere}}</style></head>
<body><a href="/">Back to lobby</a>{prose}<h2 id="units">Unit reference</h2><p>Rates and cooldowns use simulation ticks. Matter is paid continuously. The last column shows active-building / build-ability survival flags for completed entities. On a narrow screen, scroll the table sideways. Approximate ranges are marked ≈.</p>
<div class="table-scroll"><table><thead><tr><th>Type</th><th>Kind</th><th>Cost</th><th>HP</th><th>Vision</th><th>Movement</th><th>Weapon</th><th>Mining</th><th>Construction</th><th>Production</th><th>Healing</th><th>Survival flags</th></tr></thead><tbody>{rows}</tbody></table></div>
<footer><p>Content: <code>{hash}</code></p></footer></body></html>
"#
    ))
}
fn rules_build(prose: &str) -> String {
    sha256(format!("atemporal-guide-renderer-v2\n{prose}").as_bytes())
}
fn artifacts(content: &Content, prose: &str) -> Result<(GuideManifest, BTreeMap<String, Vec<u8>>)> {
    let content = normalize_content(content.clone())?;
    let files: BTreeMap<String, Vec<u8>> = BTreeMap::from([
        (
            "index.html".into(),
            render_guide(&content, prose)?.into_bytes(),
        ),
        (
            "content.json".into(),
            serde_json::to_vec_pretty(&content).map_err(|e| e.to_string())?,
        ),
    ]);
    let manifest = GuideManifest {
        schema_version: Version::default(),
        content_hash: content_hash(&content)?,
        rules_build: rules_build(prose),
        locale: "en".into(),
        generated_files: files
            .iter()
            .map(|(name, bytes)| (name.clone(), sha256(bytes)))
            .collect(),
    };
    Ok((manifest, files))
}
pub fn write_guide(content: &Content, prose: &str, directory: &Path) -> Result<GuideManifest> {
    let (manifest, files) = artifacts(content, prose)?;
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    for (name, bytes) in files {
        atomic_write(&directory.join(name), &bytes)?;
    }
    atomic_write(
        &directory.join("manifest.json"),
        &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )?;
    Ok(manifest)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}
pub fn guide_matches(content: &Content, prose: &str, directory: &Path) -> Result<bool> {
    let (expected, _) = artifacts(content, prose)?;
    let actual = fs::read(directory.join("manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<GuideManifest>(&b).ok());
    if actual.as_ref() != Some(&expected) {
        return Ok(false);
    }
    Ok(expected.generated_files.iter().all(|(name, hash)| {
        fs::read(directory.join(name)).is_ok_and(|bytes| sha256(&bytes) == *hash)
    }))
}
/// Server calls before publishing guide_url. Works for runtime overrides and pinned archives.
/// Returns a verified directory to mount, not an HTTP route or a publication side effect.
pub fn select_guide(
    content: &Content,
    prose: &str,
    bundled: &Path,
    cache: &Path,
) -> Result<PathBuf> {
    if guide_matches(content, prose, bundled)? {
        return Ok(bundled.to_path_buf());
    }
    let (manifest, _) = artifacts(content, prose)?;
    let directory = cache.join(canonical_hash(&manifest)?);
    if !guide_matches(content, prose, &directory)? {
        write_guide(content, prose, &directory)?;
    }
    if !guide_matches(content, prose, &directory)? {
        return Err("generated guide failed content validation".into());
    }
    Ok(directory)
}
