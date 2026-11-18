use super::{FrameRules, OwnEntity, PlayerFrame, PolicyTimeline, allied, append, distance, ore_at};
use atemporal_contracts::{
    BlueprintSettings, CardinalDirection, Command, DraftCommand, DraftItemRef, Lifecycle,
    MissileLaunch, Order, PlayerId, Priority, ProductionEdit, Rect, SiloPlan, Terrain, TerrainCell,
    Tile, TurnDraft, TypeKey, TypeKind,
};
use atemporal_sim::map;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

pub(super) fn adjacent(t: Tile, direction: CardinalDirection, terrain: &Terrain) -> Option<Tile> {
    let (dx, dy) = match direction {
        CardinalDirection::N => (0, -1),
        CardinalDirection::S => (0, 1),
        CardinalDirection::E => (1, 0),
        CardinalDirection::W => (-1, 0),
    };
    let (x, y) = (t.x.checked_add_signed(dx)?, t.y.checked_add_signed(dy)?);
    (x < terrain.width && y < terrain.height).then_some(Tile { x, y })
}

/// Desired total of each structure and its route distance from home. Constructors choose
/// their next project by walking distance, so later outposts can be placed in the first draft.
const EXPANSION: &[(&str, usize, u16)] = &[
    ("factory", 2, 3),
    ("turret", 2, 4),
    ("silo", 1, 6),
    ("factory", 3, 10),
    ("turret", 3, 11),
    ("silo", 2, 13),
    ("factory", 4, 14),
    ("factory", 5, 18),
    ("turret", 4, 20),
    ("factory", 6, 23),
    ("silo", 3, 24),
    ("factory", 7, 30),
    ("turret", 5, 31),
    ("factory", 8, 35),
];

fn route_lengths(terrain: &Terrain, home: Tile) -> BTreeMap<Tile, u16> {
    // Route distance excludes disconnected pockets and sites on the far side of a wall.
    let mut routes = BTreeMap::from([(home, 0u16)]);
    let mut frontier = VecDeque::from([home]);
    while let Some(t) = frontier.pop_front() {
        let steps = routes[&t] + 1;
        if steps > terrain.width {
            continue;
        }
        for (dx, dy) in [(0, 1), (1, 0), (-1, 0), (0, -1)] {
            let (Some(x), Some(y)) = (t.x.checked_add_signed(dx), t.y.checked_add_signed(dy))
            else {
                continue;
            };
            let next = Tile { x, y };
            if x < terrain.width
                && y < terrain.height
                && terrain.cells[usize::from(y) * usize::from(terrain.width) + usize::from(x)]
                    == TerrainCell::Floor
                && !routes.contains_key(&next)
            {
                routes.insert(next, steps);
                frontier.push_back(next);
            }
        }
    }
    routes
}

fn factory_queue(recipes: &[TypeKey], desired: usize) -> Option<(Vec<TypeKey>, Vec<bool>)> {
    let unit = recipes
        .iter()
        .find(|key| key.as_str() == "tank")
        .or_else(|| recipes.first())?;
    let grunts = recipes.iter().any(|r| r == "grunt");
    let (queue, flags) = if desired == 2 && grunts {
        let mut queue = vec!["grunt".into(); 8];
        queue.push(unit.clone());
        let mut flags = vec![false; 8];
        flags.push(true);
        (queue, flags)
    } else if desired == 3 && recipes.iter().any(|r| r == "constructor") {
        (vec!["constructor".into(); 4], vec![false; 4])
    } else if desired == 5 && recipes.iter().any(|r| r == "artillery") {
        (vec!["artillery".into()], vec![true])
    } else {
        (
            vec![if desired.is_multiple_of(2) && grunts {
                "grunt".into()
            } else {
                unit.clone()
            }],
            vec![true],
        )
    };
    Some((queue, flags))
}

fn opening_volley(
    state: &PlayerFrame,
    player: PlayerId,
    rules: FrameRules<'_>,
    home: Tile,
    target: Tile,
    desired: usize,
) -> Option<Vec<MissileLaunch>> {
    let FrameRules {
        config,
        content,
        terrain,
    } = rules;
    // Starting corners are public. Prefer a visible enemy structure; otherwise aim at
    // a hostile starting cluster, including the correct opposing corner in team games.
    let aim = state
        .visible_entities
        .iter()
        .filter(|e| {
            !allied(config, player, e.owner)
                && content
                    .types
                    .iter()
                    .any(|d| d.key == e.type_key && d.kind == TypeKind::Structure)
        })
        .min_by_key(|e| (e.type_key != "factory", distance(e.tile, target)))
        .map(|e| e.tile)
        .or_else(|| {
            map::starts(config.map_size, config.player_count, content)
                .ok()?
                .into_iter()
                .enumerate()
                .filter(|(p, _)| !allied(config, player, *p as u8))
                .map(|(_, start)| start.anchor)
                .min_by_key(|t| distance(*t, home))
        });
    let aim = aim?;
    let advance = |v: u16, size: u16, steps: u16| {
        if v < size / 2 {
            v.saturating_add(steps).min(size - 1)
        } else {
            v.saturating_sub(steps)
        }
    };
    let mut targets = [
        aim,
        Tile {
            x: advance(aim.x, terrain.width, 5),
            y: advance(aim.y, terrain.height, 5),
        },
        Tile {
            x: advance(aim.x, terrain.width, 8),
            y: aim.y,
        },
        Tile {
            x: aim.x,
            y: advance(aim.y, terrain.height, 8),
        },
    ];
    if desired == 2 {
        targets.rotate_left(1);
    }
    let launches: Vec<_> = targets
        .into_iter()
        .filter(|t| {
            !state
                .visible_entities
                .iter()
                .any(|e| allied(config, player, e.owner) && distance(e.tile, *t) <= 16)
                && !state
                    .own_entities
                    .iter()
                    .any(|e| distance(e.tile, *t) <= 16)
        })
        .map(|target| MissileLaunch {
            type_key: "tac_nuke".into(),
            target,
        })
        .collect();
    Some(launches)
}

/// An economic opening alone gives the first wave of fighters a mining rally. Open a second
/// line with its own attack rally and build a silo away from the value concentrated at home.
pub(super) fn develop(
    state: &PlayerFrame,
    player: PlayerId,
    rules: FrameRules<'_>,
    home: Tile,
    target: Tile,
    commands: &mut Vec<DraftCommand>,
) {
    let FrameRules {
        content, terrain, ..
    } = rules;
    if !state
        .own_entities
        .iter()
        .any(|e| e.owner == player && e.type_key == "constructor")
    {
        return;
    }
    let Some(factory) = content
        .types
        .iter()
        .find(|d| d.key == "factory")
        .and_then(|d| d.production.as_ref())
    else {
        return;
    };
    let mut occupied: BTreeSet<_> = state
        .own_entities
        .iter()
        .map(|e| e.tile)
        .chain(state.visible_entities.iter().map(|e| e.tile))
        .chain(state.own_blueprints.iter().map(|b| b.tile))
        .collect();
    occupied.extend(state.own_blueprints.iter().filter_map(|b| {
        b.output_direction
            .and_then(|d| adjacent(b.tile, d, terrain))
    }));
    occupied.extend(
        state
            .own_entities
            .iter()
            .filter_map(|e| e.production.as_ref().map(|p| p.output_tile)),
    );
    let free = |tile: Tile, occupied: &BTreeSet<Tile>| {
        let i = usize::from(tile.y) * usize::from(terrain.width) + usize::from(tile.x);
        terrain.cells[i] == TerrainCell::Floor
            && ore_at(state, i) == 0.
            && !occupied.contains(&tile)
    };
    let routes = route_lengths(terrain, home);
    let mut counts = BTreeMap::<String, usize>::new();
    for key in state
        .own_entities
        .iter()
        .filter(|e| e.owner == player)
        .map(|e| &e.type_key)
        .chain(
            state
                .own_blueprints
                .iter()
                .filter(|b| b.owner == player && b.site_id.is_none())
                .map(|b| &b.type_key),
        )
    {
        *counts.entry(key.clone()).or_default() += 1;
    }
    for command in commands.iter() {
        if let Command::PlaceBlueprints {
            type_key,
            tiles,
            output_directions,
            ..
        } = &command.command
        {
            *counts.entry(type_key.clone()).or_default() += tiles.len();
            for (i, tile) in tiles.iter().enumerate() {
                occupied.insert(*tile);
                if let Some(out) = output_directions
                    .as_ref()
                    .and_then(|d| d.get(i))
                    .and_then(|d| adjacent(*tile, *d, terrain))
                {
                    occupied.insert(out);
                }
            }
        }
    }
    for &(structure, desired, ideal) in EXPANSION {
        if u32::from(ideal) > u32::from(terrain.width) * 3 / 4
            || !content.types.iter().any(|d| d.key == structure)
        {
            continue;
        }
        if counts.get(structure).copied().unwrap_or_default() >= desired {
            continue;
        }
        if structure == "silo"
            && !content.types.iter().any(|d| {
                d.key == "silo"
                    && d.production
                        .as_ref()
                        .is_some_and(|p| p.recipes.iter().any(|r| r == "tac_nuke"))
            })
        {
            continue;
        }
        let flank = if desired.is_multiple_of(2) {
            Tile {
                x: target.x,
                y: home.y,
            }
        } else {
            Tile {
                x: home.x,
                y: target.y,
            }
        };
        let direction = if ideal >= 10 { flank } else { target };
        let output = if target.x < home.x {
            CardinalDirection::W
        } else {
            CardinalDirection::E
        };
        let site = routes
            .iter()
            .filter(|(t, _)| {
                free(**t, &occupied)
                    && (structure != "factory"
                        || adjacent(**t, output, terrain).is_some_and(|out| free(out, &occupied)))
            })
            .min_by_key(|(t, steps)| (steps.abs_diff(ideal), distance(**t, direction), t.y, t.x))
            .map(|(t, _)| *t);
        let Some(tile) = site else {
            continue;
        };
        let priority = if structure == "silo" && desired > 1 {
            Priority::Low
        } else {
            Priority::Medium
        };
        let (queue, flags, plan) = if structure == "turret" {
            (vec![], vec![], None)
        } else if structure == "factory" {
            let Some((queue, flags)) = factory_queue(&factory.recipes, desired) else {
                continue;
            };
            (queue, flags, None)
        } else {
            let Some(launches) = opening_volley(state, player, rules, home, target, desired) else {
                continue;
            };
            let mut queue = vec!["tac_nuke".into(); launches.len()];
            let mut flags = vec![false; queue.len()];
            let automatic = desired >= 3;
            if automatic {
                queue.push("tac_nuke".into());
                flags.push(true);
            }
            (
                queue,
                flags,
                Some(SiloPlan {
                    automatic,
                    launches,
                }),
            )
        };
        occupied.insert(tile);
        if structure == "factory"
            && let Some(out) = adjacent(tile, output, terrain)
        {
            occupied.insert(out);
        }
        *counts.entry(structure.into()).or_default() += 1;
        let id = append(
            commands,
            Command::PlaceBlueprints {
                type_key: structure.into(),
                tiles: vec![tile],
                priority,
                output_directions: (structure == "factory").then(|| vec![output]),
            },
        );
        if structure == "turret" {
            continue;
        }
        append(
            commands,
            Command::ConfigureBlueprints {
                blueprint_ids: vec![DraftItemRef::Draft {
                    local_id: id,
                    item_index: 0,
                }],
                settings: BlueprintSettings {
                    queue_loop_flags: flags,
                    queue,
                    order: if desired == 3 && structure == "factory" {
                        Order::Construct {
                            area: Rect {
                                min: Tile { x: 0, y: 0 },
                                max: Tile {
                                    x: terrain.width - 1,
                                    y: terrain.height - 1,
                                },
                            },
                        }
                    } else {
                        Order::AttackMove {
                            destination: target,
                        }
                    },
                    priority,
                    loop_enabled: structure == "factory",
                    silo_plan: plan,
                },
            },
        );
    }
}

fn blast_tiles(center: Tile) -> impl Iterator<Item = Tile> {
    (-4_i16..=4).flat_map(move |dx| {
        (-4_i16..=4).filter_map(move |dy| {
            if dx * dx + dy * dy > 16 {
                return None;
            }
            Some(Tile {
                x: center.x.checked_add_signed(dx)?,
                y: center.y.checked_add_signed(dy)?,
            })
        })
    })
}

/// Direct silos at infrastructure seen anywhere in the published timeline.
pub(super) fn target_observed_bases(
    input: &PolicyTimeline,
    view: &PlayerFrame,
    draft: &mut TurnDraft,
) {
    let silos: Vec<_> = view
        .own_entities
        .iter()
        .filter(|e| {
            e.lifecycle == Lifecycle::Complete
                && e.production.as_ref().is_some_and(|p| p.silo.is_some())
        })
        .collect();
    let mut opening_plans = draft
        .commands
        .iter_mut()
        .filter_map(|row| {
            if let Command::ConfigureBlueprints { settings, .. } = &mut row.command {
                settings.silo_plan.as_mut()
            } else {
                None
            }
        })
        .peekable();
    if silos.is_empty() && opening_plans.peek().is_none() {
        return;
    }
    let mut known = BTreeMap::<Tile, u32>::new();
    let Some(last) = input.timeline.frames().next_back() else {
        return;
    };
    let visible = last.visible_entities.iter().map(Arc::as_ref);
    let remembered = input
        .timeline
        .remembered_buildings(last.tick)
        .map(|sighting| sighting.entity);
    for e in visible.chain(remembered) {
        if allied(input.config(), input.player(), e.owner) {
            continue;
        }
        let worth = match e.type_key.as_str() {
            "silo" => 8,
            "factory" => 6,
            "turret" => 3,
            _ => continue,
        };
        known.insert(e.tile, worth);
    }
    if known.is_empty() {
        return;
    }
    let friendly_tiles: BTreeSet<_> = view
        .own_entities
        .iter()
        .map(|e| e.tile)
        .chain(
            view.visible_entities
                .iter()
                .filter(|e| allied(input.config(), input.player(), e.owner))
                .map(|e| e.tile),
        )
        .collect();
    let mut targets = vec![];
    while !known.is_empty() && targets.len() < 4 {
        let Some(center) = known
            .keys()
            .copied()
            .filter(|pos| !blast_tiles(*pos).any(|tile| friendly_tiles.contains(&tile)))
            .max_by_key(|pos| {
                blast_tiles(*pos)
                    .filter_map(|tile| known.get(&tile))
                    .sum::<u32>()
            })
        else {
            break;
        };
        targets.push(center);
        for tile in blast_tiles(center) {
            known.remove(&tile);
        }
    }
    if targets.is_empty() {
        return;
    }
    // Reuse observed targets in opening silo blueprints while preserving their queue sizes.
    for (index, command) in opening_plans.enumerate() {
        for (offset, launch) in command.launches.iter_mut().take(targets.len()).enumerate() {
            launch.target = targets[(index + offset) % targets.len()];
        }
    }
    let key = "tac_nuke";
    for (index, e) in silos.iter().enumerate() {
        let p = e.production.as_ref().unwrap();
        let silo = p.silo.as_ref().unwrap();
        let launches: Vec<_> = (0..targets.len())
            .map(|i| MissileLaunch {
                type_key: key.into(),
                target: targets[(i + index) % targets.len()],
            })
            .collect();
        if silo.plan.launches != launches {
            append(
                &mut draft.commands,
                Command::SetSiloPlan {
                    silos: vec![e.id.clone()],
                    plan: SiloPlan {
                        launches: launches.clone(),
                        automatic: silo.plan.automatic,
                    },
                },
            );
        }
        let ready = silo
            .inventory
            .iter()
            .find(|s| s.type_key == key)
            .map_or(0, |s| s.count as usize);
        let queued = p.pending_items.iter().filter(|i| i.type_key == key).count()
            + usize::from(p.active_item.as_ref().is_some_and(|i| i.type_key == key));
        let missing = launches.len().saturating_sub(ready + queued);
        if missing > 0 {
            append(
                &mut draft.commands,
                Command::EditProduction {
                    factories: vec![e.id.clone()],
                    edit: ProductionEdit::Append {
                        items: vec![key.into(); missing],
                    },
                },
            );
        }
    }
}

/// Split a stalled field army into search parties. Survey the public terrain for passable
/// destinations far from the paths our army already followed in the published timeline.
pub(super) fn search_stalemate(input: &PolicyTimeline, view: &PlayerFrame, draft: &mut TurnDraft) {
    if !input.stalemate() {
        return;
    }
    let armed = |e: &OwnEntity| {
        e.lifecycle == Lifecycle::Complete
            && input
                .content()
                .types
                .iter()
                .any(|d| d.key == e.type_key && d.movement.is_some() && d.weapon.is_some())
    };
    let army: Vec<_> = view
        .own_entities
        .iter()
        .filter(|e| armed(e))
        .map(|e| e.id.clone())
        .collect();
    if army.len() < 12 {
        return;
    }
    let Some(last) = input.timeline.frames().next_back() else {
        return;
    };
    let mut targets: Vec<_> = last
        .visible_entities
        .iter()
        .filter(|e| {
            !allied(input.config(), input.player(), e.owner)
                && input
                    .content()
                    .types
                    .iter()
                    .any(|d| d.key == e.type_key && d.counts_for_survival)
        })
        .map(|e| e.tile)
        .collect();
    targets.sort();
    targets.dedup();
    targets.truncate(4);
    let terrain = &input.timeline.terrain;
    let index = |tile: Tile| usize::from(tile.y) * usize::from(terrain.width) + usize::from(tile.x);
    let mut distances = vec![u16::MAX; terrain.cells.len()];
    let mut flood = VecDeque::new();
    let mut previous: Option<&PlayerFrame> = None;
    for frame in input.timeline.frames() {
        let own_changed =
            previous.is_none_or(|old| !Arc::ptr_eq(&old.own_entities, &frame.own_entities));
        let visible_changed =
            previous.is_none_or(|old| !Arc::ptr_eq(&old.visible_entities, &frame.visible_entities));
        let own_tiles = own_changed
            .then_some(&frame.own_entities)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .map(|e| e.tile);
        let allied_tiles = visible_changed
            .then_some(&frame.visible_entities)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .filter(|e| allied(input.config(), input.player(), e.owner))
            .map(|e| e.tile);
        for tile in own_tiles.chain(allied_tiles) {
            if distances[index(tile)] > 0 {
                distances[index(tile)] = 0;
                flood.push_back(tile);
            }
        }
        previous = Some(frame);
    }
    let party_count = (army.len() / 6).clamp(2, 8);
    while targets.len() < party_count {
        while let Some(tile) = flood.pop_front() {
            let d = distances[index(tile)] + 1;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (Some(x), Some(y)) =
                    (tile.x.checked_add_signed(dx), tile.y.checked_add_signed(dy))
                else {
                    continue;
                };
                if x >= terrain.width || y >= terrain.height {
                    continue;
                }
                let next = Tile { x, y };
                let i = index(next);
                if terrain.cells[i] == TerrainCell::Floor && distances[i] > d {
                    distances[i] = d;
                    flood.push_back(next);
                }
            }
        }
        let Some((i, _)) = distances
            .iter()
            .enumerate()
            .filter(|(_, d)| **d != u16::MAX && **d > 2)
            .max_by_key(|(_, d)| **d)
        else {
            break;
        };
        let t = Tile {
            x: (i % usize::from(terrain.width)) as u16,
            y: (i / usize::from(terrain.width)) as u16,
        };
        targets.push(t);
        distances[i] = 0;
        flood.push_back(t);
    }
    if targets.is_empty() {
        return;
    }
    let mut existing = draft.commands.iter().position(|d| {
        matches!(
            &d.command,
            Command::AssignOrder {
                order: Order::AttackMove { .. },
                ..
            }
        )
    });
    for (i, target) in targets.iter().enumerate() {
        let command = Command::AssignOrder {
            entities: army
                .iter()
                .skip(i)
                .step_by(targets.len())
                .cloned()
                .collect(),
            order: Order::AttackMove {
                destination: *target,
            },
        };
        if let Some(index) = existing.take() {
            draft.commands[index].command = command;
        } else {
            append(&mut draft.commands, command);
        }
    }
}
