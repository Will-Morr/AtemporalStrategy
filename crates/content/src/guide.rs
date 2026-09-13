//! One generator for startup, resume and development preview.
use super::*;
use atemporal_contracts::identity::sha256;
use std::{collections::BTreeMap, fs, path::Path};
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
        let healing = if let Some(h) = &t.self_repair {
            format!(
                "Automatic self-repair: {} HP/matter; {} matter/tick",
                h.hp_per_matter, h.rate
            )
        } else {
            healing
        };
        let missile = t.missile.as_ref().map_or_else(||t.silo.as_ref().map_or_else(||"—".into(),|s|format!("Auto target {} tiles; manual unlimited; launch cooldown {} ticks",s.auto_range,s.launch_cooldown)),|m|format!("{:?}: radius {}; damage {}; speed {} tiles/tick; flight ≤ {} ticks; destination reveal {} ticks",m.effect,m.radius,if m.effect==MissileEffect::TacNuke {"annihilation".into()} else {m.damage.to_string()},m.speed,m.max_flight_ticks,m.reveal_ticks));
        let values = vec![
            t.key.clone(),
            format!("{:?}", t.kind),
            t.matter_cost.to_string(),
            if t.missile.is_some() {
                "— (inventory)".into()
            } else {
                t.max_hp.to_string()
            },
            t.vision.to_string(),
            movement,
            weapon,
            work(&t.mining),
            work(&t.construction),
            production,
            healing,
            missile,
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
<div class="table-scroll"><table><thead><tr><th>Type</th><th>Kind</th><th>Cost</th><th>HP</th><th>Vision</th><th>Movement</th><th>Weapon</th><th>Mining</th><th>Construction</th><th>Production</th><th>Healing</th><th>Missiles</th><th>Survival flags</th></tr></thead><tbody>{rows}</tbody></table></div>
<footer><p>Content: <code>{hash}</code></p></footer></body></html>
"#
    ))
}
fn rules_build(prose: &str) -> String {
    sha256(format!("atemporal-guide-renderer-v3\n{prose}").as_bytes())
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
