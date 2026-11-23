use super::{
    FrameRules, OwnEntity, PlayerFrame, PolicyTimeline, TacticalParams, allied, append, distance,
    ore_at,
};
use atemporal_contracts::{
    BlueprintSettings, CardinalDirection, Command, ControlLimit, DraftCommand, DraftItemRef,
    Lifecycle, MissileLaunch, Order, PlayerId, Priority, ProductionEdit, Rect, SiloPlan, Terrain,
    TerrainCell, Tick, Tile, TurnDraft, TypeKey,
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
    ("factory", 3, 7),
    ("factory", 4, 10),
    ("turret", 3, 13),
    ("factory", 5, 15),
    ("silo", 1, 17),
    ("factory", 6, 23),
    ("factory", 7, 28),
    ("silo", 2, 30),
    ("turret", 4, 32),
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

fn factory_queue(
    recipes: &[TypeKey],
    desired: usize,
    opening_grunts: u8,
) -> Option<(Vec<TypeKey>, Vec<bool>)> {
    let has = |key| recipes.iter().any(|recipe| recipe == key);
    if desired == 3 && has("constructor") {
        return Some((
            vec!["constructor".into(); 5],
            vec![false, false, false, false, true],
        ));
    }
    if desired == 4 && has("artillery") && has("scout") {
        return Some((
            vec!["artillery".into(), "scout".into(), "artillery".into()],
            vec![true; 3],
        ));
    }
    if desired == 5 && has("artillery") {
        return Some((vec!["artillery".into()], vec![true]));
    }
    let prefix = if desired == 2 && has("grunt") {
        usize::from(opening_grunts)
    } else {
        0
    };
    let mut queue = vec!["grunt".into(); prefix];
    let cycle: &[&str] = if desired == 2 {
        &["scout", "artillery", "grunt", "grunt"]
    } else if desired.is_multiple_of(2) {
        &["artillery", "tank", "scout", "grunt"]
    } else {
        &["artillery", "grunt", "grunt", "scout"]
    };
    queue.extend(
        cycle
            .iter()
            .filter(|&&key| has(key))
            .map(|key| (*key).into()),
    );
    if queue.len() == prefix {
        queue.push(recipes.first()?.clone());
    }
    let flags = (0..queue.len()).map(|i| i >= prefix).collect();
    Some((queue, flags))
}

fn hostile_anchor(rules: FrameRules<'_>, player: PlayerId, home: Tile) -> Option<Tile> {
    map::starts(
        rules.config.map_size,
        rules.config.player_count,
        rules.content,
    )
    .ok()?
    .into_iter()
    .enumerate()
    .filter(|(seat, _)| !allied(rules.config, player, *seat as PlayerId))
    .map(|(_, start)| start.anchor)
    .min_by_key(|&anchor| distance(home, anchor))
}

fn flanking_target(terrain: &Terrain, home: Tile, target: Tile, x_axis: bool) -> Tile {
    let offset = (terrain.width / 5).max(3);
    let flank = |from: u16, to: u16| {
        if from < to {
            to.saturating_sub(offset)
        } else {
            to.saturating_add(offset)
        }
    };
    let aim = if x_axis {
        Tile {
            x: flank(home.x, target.x).min(terrain.width - 1),
            y: target.y,
        }
    } else {
        Tile {
            x: target.x,
            y: flank(home.y, target.y).min(terrain.height - 1),
        }
    };
    (0..terrain.height)
        .flat_map(|y| (0..terrain.width).map(move |x| Tile { x, y }))
        .filter(|&tile| atemporal_sim::placement::is_floor(terrain, tile))
        .min_by_key(|&tile| (distance(tile, aim), distance(tile, target)))
        .unwrap_or(target)
}

/// An economic opening alone gives the first wave of fighters a mining rally. Open a second
/// line with its own attack rally and build a silo away from the value concentrated at home.
pub(super) fn develop(
    state: &PlayerFrame,
    player: PlayerId,
    rules: FrameRules<'_>,
    home: Tile,
    target: Tile,
    params: TacticalParams,
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
    let mut outputs: BTreeSet<_> = super::reserved_factory_outputs(state, terrain).collect();
    let free = |tile: Tile, occupied: &BTreeSet<Tile>| {
        let i = usize::from(tile.y) * usize::from(terrain.width) + usize::from(tile.x);
        terrain.cells[i] == TerrainCell::Floor
            && ore_at(state, i) == 0.
            && !occupied.contains(&tile)
    };
    let routes = route_lengths(terrain, home);
    let area = Rect {
        min: Tile { x: 0, y: 0 },
        max: Tile {
            x: terrain.width - 1,
            y: terrain.height - 1,
        },
    };
    let anchor = hostile_anchor(rules, player, home).unwrap_or(target);
    let flank_targets = [
        flanking_target(terrain, home, anchor, false),
        flanking_target(terrain, home, anchor, true),
    ];
    let closer = |from: u16, to: u16| -> u16 { ((u32::from(from) * 5 + u32::from(to)) / 6) as u16 };
    let defended = Tile {
        x: closer(home.x, anchor.x),
        y: closer(home.y, anchor.y),
    };
    let screen = routes
        .iter()
        .filter(|(_, steps)| **steps <= terrain.width / 3)
        .min_by_key(|(tile, _)| distance(**tile, defended))
        .map_or(home, |(&tile, _)| tile);

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
                    outputs.insert(out);
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
        let headings = if target.x < home.x {
            [
                CardinalDirection::W,
                CardinalDirection::N,
                CardinalDirection::S,
                CardinalDirection::E,
            ]
        } else {
            [
                CardinalDirection::E,
                CardinalDirection::S,
                CardinalDirection::N,
                CardinalDirection::W,
            ]
        };
        let site = routes
            .iter()
            .filter(|(t, _)| free(**t, &occupied) && !outputs.contains(t))
            .filter_map(|(&tile, &steps)| {
                let exits = headings
                    .iter()
                    .filter_map(|&heading| {
                        adjacent(tile, heading, terrain)
                            .filter(|out| {
                                atemporal_sim::placement::is_floor(terrain, *out)
                                    && !occupied.contains(out)
                            })
                            .map(|out| (heading, out))
                    })
                    .count();
                let output = (structure == "factory")
                    .then(|| {
                        headings.iter().copied().find(|&heading| {
                            adjacent(tile, heading, terrain).is_some_and(|out| {
                                free(out, &occupied)
                                    && headings.iter().any(|&next| {
                                        adjacent(out, next, terrain).is_some_and(|exit| {
                                            exit != tile
                                                && atemporal_sim::placement::is_floor(terrain, exit)
                                                && !occupied.contains(&exit)
                                        })
                                    })
                            })
                        })
                    })
                    .flatten();
                if structure == "factory" && output.is_none() {
                    return None;
                }
                let distance_penalty = steps.abs_diff(ideal) + if exits <= 2 { 4 } else { 0 };
                Some((
                    (distance_penalty, distance(tile, direction), tile.y, tile.x),
                    tile,
                    output,
                ))
            })
            .min_by_key(|(score, _, _)| *score);
        let Some((_, tile, output)) = site else {
            continue;
        };
        let priority = if structure == "silo" {
            Priority::Low
        } else {
            Priority::Medium
        };
        let (queue, flags, plan) = if structure == "turret" {
            (vec![], vec![], None)
        } else if structure == "factory" {
            let Some((queue, flags)) =
                factory_queue(&factory.recipes, desired, params.opening_grunts)
            else {
                continue;
            };
            (queue, flags, None)
        } else {
            let recon = content.types.iter().any(|d| {
                d.key == "silo"
                    && d.production
                        .as_ref()
                        .is_some_and(|p| p.recipes.iter().any(|r| r == "satellite"))
            });
            let launches = if recon && desired == 1 {
                vec![MissileLaunch {
                    type_key: "satellite".into(),
                    target: anchor,
                }]
            } else {
                vec![]
            };
            let mut queue: Vec<_> = launches.iter().map(|shot| shot.type_key.clone()).collect();
            if desired == 1 {
                queue.push("tac_nuke".into());
            }
            let flags = vec![false; queue.len()];
            (
                queue,
                flags,
                Some(SiloPlan {
                    automatic: false,
                    launches,
                }),
            )
        };
        occupied.insert(tile);
        if let Some(out) = output.and_then(|heading| adjacent(tile, heading, terrain)) {
            outputs.insert(out);
        }
        *counts.entry(structure.into()).or_default() += 1;
        let id = append(
            commands,
            Command::PlaceBlueprints {
                type_key: structure.into(),
                tiles: vec![tile],
                priority,
                output_directions: output.map(|heading| vec![heading]),
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
                    order: match (structure, desired) {
                        ("factory", 3) => Order::Construct { area: area.clone() },
                        ("factory", 2 | 4) => Order::AttackMove {
                            destination: screen,
                        },
                        ("factory", 5..=7) => Order::AttackMove {
                            destination: flank_targets[desired % 2],
                        },
                        _ => Order::AttackMove {
                            destination: target,
                        },
                    },
                    priority,
                    loop_enabled: structure == "factory",
                    silo_plan: plan,
                },
            },
        );
    }
}

mod intel;

/// Give a new opening time to develop, then intervene before an observed
/// infrastructure collapse or when the army and missile capacity can act together.
pub(super) fn intervention(input: &PolicyTimeline, previous: Tick) -> Tick {
    let editable = input.editable_ticks();
    let (first, last) = (*editable.start(), *editable.end());
    let delay = if input.stalemate() { 500 } else { 160 };
    let wanted = previous.saturating_add(delay).clamp(first, last);
    let armed_types: BTreeSet<_> = input
        .content()
        .types
        .iter()
        .filter(|def| def.movement.is_some() && def.weapon.is_some())
        .map(|def| def.key.as_str())
        .collect();
    // The first response corrects the opening while armies and factories are still developing.
    // After that, look for an earlier chance to prevent a later infrastructure collapse.
    if previous > first && input.loss_tick().is_some() {
        let mut peak_factories = 0;
        for frame in input
            .timeline
            .frames()
            .filter(|frame| editable.contains(&frame.tick))
            .step_by(20)
        {
            let factories = frame
                .own_entities
                .iter()
                .filter(|e| e.type_key == "factory" && e.lifecycle == Lifecycle::Complete)
                .count();
            if peak_factories >= 2 && factories * 3 <= peak_factories * 2 {
                let preparation = u32::from(input.timeline.terrain.width) * 8 + 100;
                let rescue = frame.tick.saturating_sub(preparation).max(first);
                let recently_revised = input
                    .timeline
                    .turns
                    .iter()
                    .any(|turn| turn.tick.abs_diff(rescue) < 50);
                if !recently_revised {
                    return rescue;
                }
                break;
            }
            peak_factories = peak_factories.max(factories);
        }
    }
    let through = wanted.saturating_add(260).min(last);
    let has_silo = input.content().types.iter().any(|def| def.key == "silo");
    let ready = |frame: &PlayerFrame| {
        let army = frame
            .own_entities
            .iter()
            .filter(|e| {
                e.lifecycle == Lifecycle::Complete && armed_types.contains(e.type_key.as_str())
            })
            .count();
        let silo = frame
            .own_entities
            .iter()
            .any(|e| e.type_key == "silo" && e.lifecycle == Lifecycle::Complete);
        if input.stalemate() {
            army >= 32
        } else {
            army >= 20 || army >= 7 && (silo || !has_silo)
        }
    };
    (wanted..=through)
        .find(|&tick| input.timeline().frame(tick).is_some_and(ready))
        .unwrap_or(wanted)
}

pub(super) fn plan(
    input: &PolicyTimeline,
    view: &PlayerFrame,
    rules: FrameRules<'_>,
    params: TacticalParams,
) -> TurnDraft {
    let intel = intel::Intel::scan(input, params.fresh_entry_ticks);
    let rally = intel.rally(input, view, params.rally_distance_band);
    let mut draft = super::plan_frame_with_rally(
        view,
        input.player(),
        super::Style::Tactical,
        rules,
        input.revision(),
        rally,
        params,
    );
    if input.config().control_limit != ControlLimit::SingleOrder {
        direct_silos(input, view, &intel, &mut draft);
        search_stalemate(input, view, rally, &intel, &mut draft);
    }
    draft
}

fn blast_tiles(center: Tile, radius: f64) -> impl Iterator<Item = Tile> {
    let extent = radius.ceil() as i16;
    (-extent..=extent).flat_map(move |dx| {
        (-extent..=extent).filter_map(move |dy| {
            if f64::from(dx).powi(2) + f64::from(dy).powi(2) > radius * radius {
                return None;
            }
            Some(Tile {
                x: center.x.checked_add_signed(dx)?,
                y: center.y.checked_add_signed(dy)?,
            })
        })
    })
}

fn direct_silos(
    input: &PolicyTimeline,
    view: &PlayerFrame,
    intel: &intel::Intel,
    draft: &mut TurnDraft,
) {
    let key = "tac_nuke";
    let Some(missile) = input
        .content()
        .types
        .iter()
        .find(|d| d.key == key)
        .and_then(|d| d.missile.as_ref())
    else {
        return;
    };
    let terrain = &input.timeline.terrain;
    let last = input
        .timeline
        .frames()
        .next_back()
        .expect("published timeline");
    let mut known: BTreeMap<_, _> = intel
        .structures
        .iter()
        .map(|s| (s.tile, if s.direct { s.worth * 2 } else { s.worth }))
        .collect();
    let ready_for_mobile_target = view.own_entities.iter().any(|entity| {
        entity.lifecycle == Lifecycle::Complete
            && entity
                .production
                .as_ref()
                .and_then(|production| production.silo.as_ref())
                .is_some_and(|silo| {
                    silo.inventory
                        .iter()
                        .any(|item| item.type_key == key && item.count > 0)
                })
    });
    let forecast = ready_for_mobile_target
        .then(|| input.timeline().frame(view.tick.saturating_add(12)))
        .flatten();
    if let Some(forecast) = forecast {
        let mobile_worth: BTreeMap<_, _> = input
            .content()
            .types
            .iter()
            .filter(|d| d.movement.is_some() && d.weapon.is_some())
            .map(|d| (d.key.as_str(), (d.matter_cost / 12.) as u32))
            .collect();
        for enemy in forecast
            .visible_entities
            .iter()
            .filter(|e| !allied(input.config(), input.player(), e.owner))
        {
            if let Some(&worth) = mobile_worth.get(enemy.type_key.as_str()) {
                *known.entry(enemy.tile).or_insert(0) += worth;
            }
        }
    }
    let friendly_tiles: BTreeSet<_> = view
        .own_entities
        .iter()
        .map(|e| e.tile)
        .chain(
            forecast
                .into_iter()
                .flat_map(|frame| frame.own_entities.iter())
                .map(|e| e.tile),
        )
        .chain(
            view.visible_entities
                .iter()
                .filter(|e| allied(input.config(), input.player(), e.owner))
                .map(|e| e.tile),
        )
        .collect();
    let safe =
        |center| !blast_tiles(center, missile.radius).any(|tile| friendly_tiles.contains(&tile));
    let already_in_flight = |center| {
        view.missiles.iter().chain(last.missiles.iter()).any(|f| {
            f.type_key == key && f64::from(distance(f.target, center)) <= missile.radius.powi(2)
        })
    };
    // Do not reward a new blast for targets the existing projectile already covers.
    known.retain(|&tile, _| !already_in_flight(tile));
    let viable = |center| safe(center) && !already_in_flight(center);
    let mut targets = Vec::new();
    while targets.len() < 4 {
        let candidates: BTreeSet<_> = if forecast.is_some() {
            known
                .keys()
                .flat_map(|&pos| blast_tiles(pos, missile.radius))
                .filter(|tile| tile.x < terrain.width && tile.y < terrain.height)
                .collect()
        } else {
            known.keys().copied().collect()
        };
        let best = candidates
            .into_iter()
            .filter(|&pos| viable(pos))
            .map(|pos| {
                let score = blast_tiles(pos, missile.radius)
                    .filter_map(|tile| known.get(&tile))
                    .sum::<u32>();
                (pos, score)
            })
            .max_by_key(|(pos, score)| (*score, known.get(pos).copied().unwrap_or(0)));
        let Some((center, _)) = best.filter(|(_, score)| forecast.is_none() || *score >= 6) else {
            break;
        };
        targets.push(center);
        for tile in blast_tiles(center, missile.radius) {
            known.remove(&tile);
        }
    }
    // A single lane can be a chokepoint. Spend a missile on a lead only when multiple
    // approaches corroborate one area and no better observed target is available.
    let lead = (targets.is_empty() && intel.structures.is_empty())
        .then(|| {
            intel
                .leads
                .iter()
                .find(|lead| {
                    lead.fresh_sources >= 3 && lead.separated_approaches && viable(lead.tile)
                })
                .map(|lead| lead.tile)
        })
        .flatten();
    if let Some(lead) = lead {
        targets.push(lead);
    }

    let mut silos: Vec<_> = view
        .own_entities
        .iter()
        .filter_map(|e| {
            let production = e.production.as_ref()?;
            let silo = production.silo.as_ref()?;
            Some((e, production, silo))
        })
        .collect();
    silos.sort_by_key(|(entity, _, _)| entity.lifecycle == Lifecycle::Site);
    let silos_count = silos.len();
    let planned = view
        .own_blueprints
        .iter()
        .find(|b| b.type_key == "silo" && b.site_id.is_none());
    let fallback_allowed = |target| {
        viable(target)
            && lead.is_none_or(|tile| f64::from(distance(tile, target)) > missile.radius.powi(2))
            && (!intel.checked_after(terrain, target, view.tick)
                || intel
                    .structures
                    .iter()
                    .any(|s| f64::from(distance(s.tile, target)) <= missile.radius.powi(2)))
    };
    let strike = |&target: &Tile| MissileLaunch {
        type_key: key.into(),
        target,
    };
    if silos.is_empty()
        && !targets.is_empty()
        && let Some(planned) = planned
    {
        let mut settings = planned.settings.clone().unwrap_or(BlueprintSettings {
            queue: vec![],
            queue_loop_flags: vec![],
            order: Order::Idle {},
            priority: planned.priority,
            loop_enabled: false,
            silo_plan: None,
        });
        let plan = settings.silo_plan.get_or_insert_with(|| SiloPlan {
            automatic: false,
            launches: vec![],
        });
        plan.launches.retain(|launch| launch.type_key != key);
        plan.launches.extend(targets.iter().map(strike));
        settings.queue = plan
            .launches
            .iter()
            .map(|launch| launch.type_key.clone())
            .collect();
        settings.queue_loop_flags = vec![false; settings.queue.len()];
        append(
            &mut draft.commands,
            Command::ConfigureBlueprints {
                blueprint_ids: vec![DraftItemRef::Persistent {
                    id: planned.id.clone(),
                }],
                settings,
            },
        );
    }
    for (index, silo_plan) in draft
        .commands
        .iter_mut()
        .filter_map(|row| {
            let Command::ConfigureBlueprints {
                blueprint_ids,
                settings,
            } = &mut row.command
            else {
                return None;
            };
            if !blueprint_ids
                .iter()
                .any(|id| matches!(id, DraftItemRef::Draft { .. }))
            {
                return None;
            }
            settings.silo_plan.as_ref()?;
            Some(settings)
        })
        .enumerate()
    {
        let plan = silo_plan.silo_plan.as_mut().unwrap();
        if targets.is_empty() {
            continue;
        }
        if silos.is_empty() && planned.is_none() && index == 0 {
            plan.launches.extend(targets.iter().map(strike));
            silo_plan.queue = plan
                .launches
                .iter()
                .map(|launch| launch.type_key.clone())
                .collect();
            silo_plan.queue_loop_flags = vec![false; plan.launches.len()];
        }
    }

    for (index, (entity, production, silo)) in silos.into_iter().enumerate() {
        let mut launches: Vec<_> = silo
            .plan
            .launches
            .iter()
            .filter(|launch| {
                launch.type_key != key || targets.is_empty() && fallback_allowed(launch.target)
            })
            .cloned()
            .collect();
        launches.extend(targets.iter().skip(index).step_by(silos_count).map(strike));
        if silo.plan.launches == launches {
            continue;
        }
        let ready = silo
            .inventory
            .iter()
            .find(|s| s.type_key == key)
            .map_or(0, |s| s.count as usize);
        let queued = production
            .pending_items
            .iter()
            .filter(|i| i.type_key == key)
            .count()
            + usize::from(
                production
                    .active_item
                    .as_ref()
                    .is_some_and(|i| i.type_key == key),
            );
        let missing = launches
            .iter()
            .filter(|l| l.type_key == key)
            .count()
            .saturating_sub(ready + queued);
        append(
            &mut draft.commands,
            Command::SetSiloPlan {
                silos: vec![entity.id.clone()],
                plan: SiloPlan {
                    launches,
                    automatic: silo.plan.automatic,
                },
            },
        );
        if missing > 0 && !targets.is_empty() {
            append(
                &mut draft.commands,
                Command::EditProduction {
                    factories: vec![entity.id.clone()],
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
fn search_stalemate(
    input: &PolicyTimeline,
    view: &PlayerFrame,
    rally: Option<Tile>,
    intel: &intel::Intel,
    draft: &mut TurnDraft,
) {
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
    let mut structures: Vec<_> = intel.structures.iter().filter(|s| s.worth >= 6).collect();
    structures.sort_by_key(|s| std::cmp::Reverse((s.direct, s.worth)));
    targets.sort();
    targets.dedup();
    targets.splice(
        0..0,
        structures
            .iter()
            .map(|s| s.tile)
            .filter(|tile| !targets.contains(tile))
            .collect::<Vec<_>>(),
    );
    if let Some(rally) = rally {
        targets.retain(|t| *t != rally);
        targets.insert(0, rally);
    }
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
    let party_count = if structures.is_empty() {
        (army.len() / 6).clamp(2, 8)
    } else {
        targets.len().max(2)
    };
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
