//! One generator for startup, resume and development preview.
use super::*;
use atemporal_contracts::identity::sha256;
use std::{collections::BTreeMap, fs, path::Path};
const GUIDE_STYLES: &str = include_str!("../../../client/guide/style.css");
const UNIT_ICONS: &str = include_str!("../../../client/unit-icons.json");
fn unit_icon(icons: &BTreeMap<String, String>, key: &str) -> String {
    let body = icons.get(key).unwrap_or(&icons["default"]);
    format!(
        r#"<svg class="unit-icon" viewBox="0 0 32 32" aria-hidden="true" focusable="false">{body}</svg>"#
    )
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
fn ticks(value: Tick) -> String {
    format!("{value} tick{}", if value == 1 { "" } else { "s" })
}
fn display_name(key: &str) -> String {
    let name = key.replace('_', " ");
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}
fn stat(stats: &mut String, label: &str, value: impl AsRef<str>) {
    stats.push_str(&format!(
        "<div><dt>{}</dt><dd data-stat=\"{}\">{}</dd></div>",
        escape(label),
        escape(label),
        escape(value.as_ref())
    ));
}
fn unit_group(t: &TypeDefinition) -> usize {
    match t.kind {
        TypeKind::Structure => 0,
        TypeKind::Missile => 3,
        TypeKind::Unit if t.mining.is_some() || t.construction.is_some() => 1,
        TypeKind::Unit => 2,
    }
}
fn unit_order(key: &str) -> usize {
    [
        "turret",
        "factory",
        "wall",
        "silo",
        "miner",
        "constructor",
        "scout",
        "grunt",
        "tank",
        "grinder",
        "artillery",
        "satellite",
        "cluster",
        "tac_nuke",
    ]
    .iter()
    .position(|name| *name == key)
    .unwrap_or(usize::MAX)
}
/// Prose is trusted repository-authored HTML. All content-derived labels are escaped.
pub fn render_guide(content: &Content, prose: &str) -> Result<String> {
    let content = normalize_content(content.clone())?;
    let icons: BTreeMap<String, String> =
        serde_json::from_str(UNIT_ICONS).map_err(|e| e.to_string())?;
    let mut prose = prose.to_owned();
    for key in icons.keys() {
        prose = prose.replace(
            &format!(r#"<span data-unit-icon="{key}"></span>"#),
            &unit_icon(&icons, key),
        );
    }
    let mut cards = String::new();
    for (group, name) in ["Buildings", "Utility", "Combat", "Missiles"]
        .iter()
        .enumerate()
    {
        let mut types: Vec<_> = content
            .types
            .iter()
            .filter(|t| unit_group(t) == group)
            .collect();
        types.sort_by_key(|t| (unit_order(&t.key), &t.key));
        if types.is_empty() {
            continue;
        }
        cards.push_str(&format!("<section class=\"stats-group\" aria-label=\"{name} stats\"><h3>{name}</h3><div class=\"unit-stats\">"));
        for t in types {
            let mut stats = String::new();
            stat(&mut stats, "Cost", t.matter_cost.to_string());
            if t.missile.is_none() {
                stat(&mut stats, "Health", t.max_hp.to_string());
                stat(&mut stats, "Vision", format!("{} tiles", t.vision));
                stat(
                    &mut stats,
                    "Movement",
                    t.movement.as_ref().map_or_else(
                        || "Stationary".into(),
                        |m| {
                            format!(
                                "One tile every {}; {}",
                                ticks(m.cooldown),
                                if m.neighbors == Neighbors::Four {
                                    "no diagonal movement"
                                } else {
                                    "diagonal movement allowed"
                                }
                            )
                        },
                    ),
                );
                stat(
                    &mut stats,
                    "Weapon",
                    t.weapon.as_ref().map_or_else(
                        || "None".into(),
                        |w| {
                            format!(
                                "{} damage every {}; {}-tile range; {} fire",
                                w.damage,
                                ticks(w.cooldown),
                                readable_range(w.range),
                                if w.indirect { "indirect" } else { "direct" }
                            )
                        },
                    ),
                );
            }
            if let Some(w) = &t.mining {
                stat(
                    &mut stats,
                    "Mining",
                    format!("{} matter every {}", w.rate, ticks(w.cooldown)),
                );
            }
            if let Some(w) = &t.construction {
                stat(
                    &mut stats,
                    "Construction",
                    format!(
                        "{} matter per work action; {}-tick cooldown",
                        w.rate, w.cooldown
                    ),
                );
            }
            if let Some(p) = &t.production {
                stat(
                    &mut stats,
                    "Production",
                    format!("Spends up to {} matter per tick", p.rate),
                );
                stat(
                    &mut stats,
                    "Builds",
                    p.recipes
                        .iter()
                        .map(|key| display_name(key))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            if let Some(h) = &t.healing {
                stat(
                    &mut stats,
                    "Healing",
                    format!(
                        "{} HP per matter; requests {} matter every {} ticks; {}-tile range",
                        h.hp_per_matter, h.demand, h.cooldown, h.range
                    ),
                );
            }
            if let Some(h) = &t.self_repair {
                stat(
                    &mut stats,
                    "Self-repair",
                    format!(
                        "{} HP per matter; spends up to {} matter per tick",
                        h.hp_per_matter, h.rate
                    ),
                );
            }
            if let Some(s) = &t.silo {
                stat(
                    &mut stats,
                    "Auto targeting",
                    format!("{}-tile range", s.auto_range),
                );
                stat(&mut stats, "Manual targeting", "Anywhere on the map");
                stat(
                    &mut stats,
                    "Launch cooldown",
                    format!("{} ticks", s.launch_cooldown),
                );
            }
            if let Some(m) = &t.missile {
                stat(
                    &mut stats,
                    "Storage",
                    "Ammunition inside a silo; no separate health",
                );
                stat(
                    &mut stats,
                    "Effect",
                    match m.effect {
                        MissileEffect::Satellite => "Reveals terrain and enemies",
                        MissileEffect::Cluster => "Area damage, including allies",
                        MissileEffect::TacNuke => "Destroys units and buildings, including allies",
                    },
                );
                stat(&mut stats, "Effect radius", format!("{} tiles", m.radius));
                if m.effect == MissileEffect::Cluster {
                    stat(&mut stats, "Damage", m.damage.to_string());
                }
                stat(
                    &mut stats,
                    "Flight speed",
                    format!("{} tiles per tick", m.speed),
                );
                stat(
                    &mut stats,
                    "Maximum flight",
                    format!("{} ticks after launch", m.max_flight_ticks),
                );
                if m.reveal_ticks > 0 {
                    stat(
                        &mut stats,
                        "Vision at destination",
                        format!("{} ticks", m.reveal_ticks),
                    );
                }
            } else {
                stat(
                    &mut stats,
                    "Surviving building",
                    if t.counts_for_survival {
                        "Yes, when complete"
                    } else {
                        "No"
                    },
                );
                stat(
                    &mut stats,
                    "Keeps building ability",
                    if t.provides_build_ability {
                        "Yes, when complete"
                    } else {
                        "No"
                    },
                );
            }
            cards.push_str(&format!(r#"<article class="stat-card" id="stats-{}" data-unit="{}"><h4>{}{}</h4><dl>{stats}</dl></article>"#, escape(&t.key), escape(&t.key), unit_icon(&icons, &t.key), escape(&display_name(&t.key))));
        }
        cards.push_str("</div></section>");
    }
    let stats = format!(
        r##"<section aria-labelledby="units"><h2 id="units">Unit stats</h2><p>These values come from this server’s game settings. Costs are in matter; health is in HP. A tick is one step of the simulation. Ranges are measured in map tiles; ≈ marks a rounded value. Work and production can slow down when matter is short.</p><p><strong>Survival:</strong> “Surviving building” means the completed building counts toward staying alive. “Keeps building ability” means the completed unit can preserve your ability to rebuild. The exact loss conditions depend on the selected game mode; see <a href="#survival">scoring and survival</a>.</p>{cards}</section>"##
    );
    prose = prose.replace("<!-- UNIT_STATS -->", &stats);
    let hash = content_hash(&content)?;
    let styles = GUIDE_STYLES;
    Ok(format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Atemporal — Game Docs</title><style>{styles}</style></head>
<body><main><aside class="doc-warning" aria-label="Documentation warning"><strong>Chatbot Docs Warning:</strong> Better than nothing but my apologies for the Claudelish</aside><a href="/">← Atemporal</a>{prose}

<footer><details><summary>Game content identifier</summary><code>{hash}</code></details></footer></main></body></html>"##
    ))
}
fn rules_build(prose: &str) -> String {
    sha256(format!("atemporal-guide-renderer-v8\n{GUIDE_STYLES}\n{UNIT_ICONS}\n{prose}").as_bytes())
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
