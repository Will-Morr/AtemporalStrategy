//! Export selected real-engine artillery retreat ticks for the local field viewer.
//! Usage: cargo run --release -p atemporal-sim --example artillery_retreat_review -- OUTPUT.json
#[path = "../tests/common/mod.rs"]
mod common;

use atemporal_sim::*;
use common::{World, attack_move, tile};
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[derive(Serialize)]
struct Review {
    schema_version: u8,
    sim_rules_version: u32,
    scenarios: Vec<ScenarioReview>,
}

#[derive(Serialize)]
struct ScenarioReview {
    id: &'static str,
    title: &'static str,
    setup: &'static str,
    map_seed: Option<u64>,
    map_rows: Vec<String>,
    ore: Vec<Ore>,
    actors: Vec<Actor>,
    frames: Vec<Frame>,
}

#[derive(Serialize)]
struct Actor {
    id: EntityId,
    role: &'static str,
}

#[derive(Serialize)]
struct Ore {
    tile: Tile,
    matter: f64,
}

#[derive(Serialize)]
struct Frame {
    label: &'static str,
    tick: Tick,
    selected_unit_id: EntityId,
    entities: Vec<Entity>,
    retreats: Vec<Retreat>,
    events: Vec<WorldEvent>,
}

#[derive(Serialize)]
struct Entity {
    #[serde(flatten)]
    state: EntityState,
    max_hp: f64,
}

#[derive(Clone, Serialize)]
struct Retreat {
    unit_id: EntityId,
    owner: PlayerId,
    from: Tile,
    threats: Vec<Tile>,
    scores: Vec<Score>,
    next: Option<Tile>,
    destination: Option<Tile>,
    firing: bool,
    move_cooldown: bool,
}

impl Retreat {
    fn plans_move(&self) -> bool {
        self.next.is_some() && !self.firing && !self.move_cooldown
    }

    fn nearby_corner_contrast(&self) -> u32 {
        let (min, max) = self
            .scores
            .iter()
            .filter(|score| score.reachable)
            .map(|score| score.corner)
            .fold((i32::MAX, i32::MIN), |(min, max), cost| {
                (min.min(cost), max.max(cost))
            });
        if min <= max { min.abs_diff(max) } else { 0 }
    }
}

#[derive(Clone, Serialize)]
struct Score {
    tile: Tile,
    threat: i32,
    corner: i32,
    combined: i32,
    reachable: bool,
}

fn open_approach() -> World {
    let rows = vec!["........................."; 15];
    let mut world = World::new(&rows);
    world.spawn_with(0, "artillery", 3, 7, attack_move(21, 7));
    world.spawn_with(1, "grinder", 21, 7, attack_move(3, 7));
    world.spawn_with(1, "grinder", 22, 8, attack_move(3, 7));
    world
}

fn dead_end(two_pursuers: bool) -> World {
    let mut rows = vec!["#".repeat(25); 15];
    for row in &mut rows[2..6] {
        row.replace_range(13..20, ".......");
    }
    for row in &mut rows[6..9] {
        row.replace_range(1..18, ".................");
    }
    rows[7].replace_range(18..24, "......");
    let mut world = World::new(&rows.iter().map(String::as_str).collect::<Vec<_>>());
    world.spawn_with(0, "artillery", 17, 7, attack_move(2, 7));
    world.spawn_with(1, "tank", 9, 7, attack_move(21, 7));
    if two_pursuers {
        world.spawn_with(1, "grinder", 10, 6, attack_move(21, 7));
    }
    world
}

fn formation() -> World {
    let rows = vec!["........................."; 15];
    let mut world = World::new(&rows);
    for y in [6, 8] {
        world.spawn_with(0, "artillery", 8, y, attack_move(21, y));
        world.spawn_with(1, "tank", 14, y, attack_move(2, y));
        world.spawn_with(1, "grinder", 13, y, attack_move(2, y));
    }
    world
}

fn generated(seed: u64) -> Result<World> {
    let mut world = World::new(&["."]);
    world.config.map_size = 48;
    world.config.seed = seed.try_into().map_err(|_| "invalid review seed")?;
    let starts = map::starts(world.config.map_size, 2, &world.content)?;
    world.state = map::generate(&world.config, &world.content)?;
    // The seed supplies the entire map and ore. Authored formations replace the
    // stock worker-only starting roster so this scenario can begin with combat.
    world.state.entities.clear();
    for seat in 0..2 {
        let start = starts[seat].anchor;
        let goal = starts[1 - seat].anchor;
        let seat = seat as PlayerId;
        world.spawn(seat, "factory", start.x, start.y);
        let lineup: &[&str] = if seat == 0 {
            &["artillery", "artillery", "scout"]
        } else {
            &["tank", "tank", "grinder", "grinder"]
        };
        let width = usize::from(world.state.terrain.width);
        let mut nearby: Vec<_> = world
            .state
            .terrain
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| **cell == TerrainCell::Floor)
            .map(|(index, _)| tile((index % width) as u16, (index / width) as u16))
            .filter(|position| *position != start)
            .collect();
        nearby.sort_by_key(|position| {
            let dx = position.x.abs_diff(start.x);
            let dy = position.y.abs_diff(start.y);
            (dx + dy, position.x, position.y)
        });
        for (key, position) in lineup.iter().zip(nearby) {
            world.spawn_with(
                seat,
                key,
                position.x,
                position.y,
                attack_move(goal.x, goal.y),
            );
        }
    }
    Ok(world)
}

fn make_frame(
    label: &'static str,
    tick: Tick,
    selected_unit_id: EntityId,
    entities: &[EntityState],
    world: &World,
    retreats: Vec<Retreat>,
    events: Vec<WorldEvent>,
) -> Frame {
    Frame {
        label,
        tick,
        selected_unit_id,
        entities: entities
            .iter()
            .map(|state| Entity {
                max_hp: world.def(&state.type_key).max_hp,
                state: state.clone(),
            })
            .collect(),
        retreats,
        events,
    }
}

fn inspect_scenario(
    id: &'static str,
    title: &'static str,
    setup: &'static str,
    map_seed: Option<u64>,
    world: World,
) -> Result<ScenarioReview> {
    let terrain = &world.state.terrain;
    let width = usize::from(terrain.width);
    let map_rows = terrain
        .cells
        .chunks_exact(width)
        .map(|row| {
            row.iter()
                .map(|cell| {
                    if *cell == TerrainCell::Floor {
                        '.'
                    } else {
                        '#'
                    }
                })
                .collect()
        })
        .collect();
    let ore = world
        .state
        .ore
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, matter)| matter > 0.0)
        .map(|(index, matter)| Ore {
            tile: tile((index % width) as u16, (index / width) as u16),
            matter,
        })
        .collect();
    let actors: Vec<_> = world
        .state
        .entities
        .iter()
        .map(|entity| Actor {
            id: entity.id.clone(),
            role: match entity.type_key.as_str() {
                "artillery" => "artillery",
                "factory" => "base",
                "scout" => "screen",
                _ => "pursuer",
            },
        })
        .collect();
    let primary = actors
        .iter()
        .find(|actor| actor.role == "artillery")
        .ok_or("review scenario needs artillery")?
        .id
        .clone();

    let mut request = world.request();
    request.fingerprint.sim_build = format!("retreat review rules {RULES_VERSION}");
    let mut sim = Sim::new(&request)?;
    sim.set_artillery_retreat_diagnostics(true);
    let mut first_advance = None;
    let mut first_escape = None;
    let mut corner_escape = None;
    let mut best_corner_contrast = 79;
    let mut entered_escape_room = None;
    let mut turned_toward_room = false;
    let mut reversal = None;
    let mut formation_escape = None;
    let mut firing_under_pressure = None;
    let mut first_artillery_loss = None;
    for _ in 0..900 {
        let tick = sim.state.tick;
        let entities = sim.state.entities.clone();
        let mut events = Vec::new();
        sim.step(&mut |output| {
            if let Output::Events(mut batch) = output {
                events.append(&mut batch);
            }
        })?;
        // Regular movement is represented by snapshots in engine output. Diff this tick's
        // authoritative positions so the review can distinguish proposal from execution.
        let mut sequence = events.last().map_or(0, |event| event.sequence + 1);
        for before in &entities {
            if let Some(after) = sim
                .state
                .entities
                .iter()
                .find(|after| after.id == before.id)
                && after.tile != before.tile
            {
                events.push(WorldEvent {
                    tick,
                    sequence,
                    event: PresentationEvent::Move {
                        entity_id: before.id.clone(),
                        from: before.tile,
                        to: after.tile,
                    },
                });
                sequence += 1;
            }
        }
        let retreats: Vec<_> = sim
            .last_artillery_retreats()
            .iter()
            .map(|(unit_id, report)| {
                let matching = entities
                    .iter()
                    .find(|entity| &entity.id == unit_id)
                    .ok_or("decision unit missing from its pre-step state")?;
                if matching.tile != report.from {
                    return Err("decision position differs from its pre-step state".to_owned());
                }
                Ok(Retreat {
                    unit_id: unit_id.clone(),
                    owner: matching.owner,
                    from: report.from,
                    threats: report.threats.clone(),
                    scores: report
                        .scores
                        .iter()
                        .map(|score| Score {
                            tile: score.tile,
                            threat: score.threat,
                            corner: score.corner,
                            combined: score.combined,
                            reachable: score.reachable,
                        })
                        .collect(),
                    next: report.next,
                    destination: report.destination,
                    firing: report.firing,
                    move_cooldown: report.move_cooldown,
                })
            })
            .collect::<Result<_>>()?;
        let capture = |label, unit, retreats, events| {
            make_frame(label, tick, unit, &entities, &world, retreats, events)
        };
        let retreating: Vec<_> = retreats
            .iter()
            .filter(|retreat| retreat.plans_move())
            .collect();
        if first_advance.is_none() && retreats.is_empty() && events.iter().any(|event| {
            matches!(&event.event, PresentationEvent::Move {entity_id, ..} if entity_id == &primary)
        }) {
            first_advance = Some(capture("Approach", primary.clone(), Vec::new(), events.clone()));
        }
        if let Some(decision) = retreating.first() {
            if first_escape.is_none() {
                first_escape = Some(capture(
                    "Retreat step proposed",
                    decision.unit_id.clone(),
                    retreats.clone(),
                    events.clone(),
                ));
            }
            if retreating.len() >= 2 && formation_escape.is_none() {
                formation_escape = Some(capture(
                    "Simultaneous retreats",
                    decision.unit_id.clone(),
                    retreats.clone(),
                    events.clone(),
                ));
            }
        }
        if matches!(id, "dead-end" | "counterpressure") {
            for decision in &retreating {
                let retreat_move = events.iter().find_map(|event| match &event.event {
                    PresentationEvent::Move {
                        entity_id,
                        from,
                        to,
                    } if entity_id == &decision.unit_id && Some(*to) == decision.next => {
                        Some((*from, *to))
                    }
                    _ => None,
                });
                let Some((from, to)) = retreat_move else {
                    continue;
                };
                if to.y < from.y {
                    turned_toward_room = true;
                    if id == "dead-end" {
                        let contrast = decision.nearby_corner_contrast();
                        if contrast > best_corner_contrast {
                            best_corner_contrast = contrast;
                            corner_escape = Some(capture(
                                "Escape near the dead end",
                                decision.unit_id.clone(),
                                retreats.clone(),
                                events.clone(),
                            ));
                        }
                        if entered_escape_room.is_none() && from.y >= 6 && to.y < 6 {
                            entered_escape_room = Some(capture(
                                "Entering the wider room",
                                decision.unit_id.clone(),
                                retreats.clone(),
                                events.clone(),
                            ));
                        }
                    }
                } else if id == "counterpressure"
                    && turned_toward_room
                    && reversal.is_none()
                    && to.y > from.y
                {
                    reversal = Some(capture(
                        "Retreat reverses under pressure",
                        decision.unit_id.clone(),
                        retreats.clone(),
                        events.clone(),
                    ));
                }
            }
        }
        if firing_under_pressure.is_none()
            && let Some(decision) = retreats.iter().find(|decision| decision.firing)
        {
            firing_under_pressure = Some(capture(
                "Firing under pressure",
                decision.unit_id.clone(),
                retreats.clone(),
                events.clone(),
            ));
        }
        if first_artillery_loss.is_none()
            && let Some(unit) = events.iter().find_map(|event| match &event.event {
                PresentationEvent::Destroyed { entity_id, .. }
                    if actors
                        .iter()
                        .any(|actor| actor.role == "artillery" && actor.id == *entity_id) =>
                {
                    Some(entity_id)
                }
                _ => None,
            })
        {
            first_artillery_loss = Some(capture(
                "Artillery destroyed",
                unit.clone(),
                retreats.clone(),
                events,
            ));
        }
        if sim.inactive()
            || !sim
                .state
                .entities
                .iter()
                .any(|entity| entity.type_key == "artillery")
        {
            break;
        }
    }
    if id == "formation" && formation_escape.is_none() {
        return Err("formation did not produce simultaneous ready retreat moves".into());
    }
    if id == "dead-end" && (corner_escape.is_none() || entered_escape_room.is_none()) {
        return Err(
            "dead-end fixture did not escape into the room with substantial reachable corner costs"
                .into(),
        );
    }
    let mut frames: Vec<_> = [
        corner_escape,
        entered_escape_room,
        reversal,
        formation_escape,
        first_advance,
        first_escape,
        firing_under_pressure,
        first_artillery_loss,
    ]
    .into_iter()
    .flatten()
    .collect();
    frames.sort_by_key(|frame| frame.tick);
    frames.dedup_by_key(|frame| frame.tick);
    if frames.is_empty() {
        return Err(format!("no real review frames for {id}"));
    }
    Ok(ScenarioReview {
        id,
        title,
        setup,
        map_seed,
        map_rows,
        ore,
        actors,
        frames,
    })
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/retreat-review/frames.json"));
    let scenarios = vec![
        inspect_scenario(
            "approach",
            "Open approach",
            "Authored field and starting units",
            None,
            open_approach(),
        )?,
        inspect_scenario(
            "dead-end",
            "Escape near a dead end",
            "Authored narrow lane, side pocket and starting units",
            None,
            dead_end(false),
        )?,
        inspect_scenario(
            "counterpressure",
            "Dead end under two pursuers",
            "Same authored dead end with an additional fast grinder",
            None,
            dead_end(true),
        )?,
        inspect_scenario(
            "formation",
            "Formation under pressure",
            "Authored field with simultaneous pressure against two guns",
            None,
            formation(),
        )?,
        inspect_scenario(
            "generated",
            "Generated map advance",
            "Generated terrain and ore, with authored combat formations at both starts",
            Some(42),
            generated(42)?,
        )?,
    ];
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = BufWriter::new(File::create(&path)?);
    serde_json::to_writer(
        &mut out,
        &Review {
            schema_version: 1,
            sim_rules_version: RULES_VERSION,
            scenarios,
        },
    )?;
    writeln!(out)?;
    out.flush()?;
    println!("{}", path.display());
    Ok(())
}
