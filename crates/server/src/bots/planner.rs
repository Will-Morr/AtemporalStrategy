//! Bounded heuristics over player-visible observations, with no rollout/search or enemy secrets.
use super::{Style, allied, distance};
use atemporal_sim::*;
use std::collections::{BTreeMap, VecDeque};
type PlannedCommand = Command<DraftItemRef<BlueprintId>, DraftItemRef<QueueItemId>>;

pub struct Knowledge {
    seen: Vec<bool>,
    // Last observed enemy structures; cleared when their tile is visibly empty.
    targets: BTreeMap<(u16, u16), Tile>,
}
impl Knowledge {
    pub fn new(state: &WorldState) -> Self {
        Self {
            seen: vec![false; state.terrain.cells.len()],
            targets: BTreeMap::new(),
        }
    }
    pub fn observe(
        &mut self,
        state: &WorldState,
        player: PlayerId,
        config: &MatchConfig,
        content: &Content,
    ) {
        let mut visible = vec![false; self.seen.len()];
        let w = usize::from(state.terrain.width);
        for e in state
            .entities
            .iter()
            .filter(|e| allied(config, player, e.owner) && e.lifecycle == Lifecycle::Complete)
        {
            let Some(d) = content.types.iter().find(|d| d.key == e.type_key) else {
                continue;
            };
            let r = d.vision.ceil() as u16;
            for y in e.tile.y.saturating_sub(r)
                ..=e.tile.y.saturating_add(r).min(state.terrain.height - 1)
            {
                for x in e.tile.x.saturating_sub(r)
                    ..=e.tile.x.saturating_add(r).min(state.terrain.width - 1)
                {
                    if f64::from(distance(e.tile, Tile { x, y })) <= d.vision * d.vision {
                        visible[usize::from(y) * w + usize::from(x)] = true;
                    }
                }
            }
        }
        for (seen, now) in self.seen.iter_mut().zip(&visible) {
            *seen |= now;
        }
        self.targets
            .retain(|_, tile| !visible[usize::from(tile.y) * w + usize::from(tile.x)]);
        for e in state
            .entities
            .iter()
            .filter(|e| !allied(config, player, e.owner))
        {
            if visible[usize::from(e.tile.y) * w + usize::from(e.tile.x)]
                && content
                    .types
                    .iter()
                    .any(|d| d.key == e.type_key && d.kind == TypeKind::Structure)
            {
                self.targets.insert((e.tile.x, e.tile.y), e.tile);
            }
        }
    }
}

/// Path distances keep exploration and placement in reachable rooms. Mobile traffic is left
/// to the normal simulator; permanent structures are obstacles.
fn paths(state: &WorldState, start: Tile, content: &Content) -> Vec<u32> {
    let w = usize::from(state.terrain.width);
    let mut blocked: Vec<_> = state
        .terrain
        .cells
        .iter()
        .map(|c| *c == TerrainCell::Wall)
        .collect();
    for e in &state.entities {
        if content
            .types
            .iter()
            .any(|d| d.key == e.type_key && d.kind == TypeKind::Structure)
        {
            blocked[usize::from(e.tile.y) * w + usize::from(e.tile.x)] = true;
        }
    }
    let root = usize::from(start.y) * w + usize::from(start.x);
    let mut distances = vec![u32::MAX; blocked.len()];
    distances[root] = 0;
    let mut queue = VecDeque::from([root]);
    while let Some(i) = queue.pop_front() {
        for j in neighbors(i, w, blocked.len()) {
            if !blocked[j] && distances[j] == u32::MAX {
                distances[j] = distances[i] + 1;
                queue.push_back(j);
            }
        }
    }
    distances
}
fn neighbors(i: usize, w: usize, n: usize) -> Vec<usize> {
    let mut out = vec![];
    if !i.is_multiple_of(w) {
        out.push(i - 1);
    }
    if i % w + 1 < w {
        out.push(i + 1);
    }
    if i >= w {
        out.push(i - w);
    }
    if i + w < n {
        out.push(i + w);
    }
    out
}
fn tile(i: usize, width: u16) -> Tile {
    Tile {
        x: (i % usize::from(width)) as u16,
        y: (i / usize::from(width)) as u16,
    }
}
fn noise(state: &WorldState, player: PlayerId) -> usize {
    // Public terrain supplies stable per-map variety without depending on simulation RNG.
    state
        .terrain
        .cells
        .iter()
        .enumerate()
        .filter(|(_, c)| **c == TerrainCell::Wall)
        .fold(usize::from(player) * 97, |a, (i, _)| {
            a.wrapping_mul(31).wrapping_add(i)
        })
}

pub fn plan(
    state: &WorldState,
    player: PlayerId,
    style: Style,
    config: &MatchConfig,
    content: &Content,
    revision: Revision,
) -> TurnDraft {
    let mut knowledge = Knowledge::new(state);
    knowledge.observe(state, player, config, content);
    plan_known(state, player, style, config, content, revision, &knowledge)
}

pub fn plan_known(
    state: &WorldState,
    player: PlayerId,
    style: Style,
    config: &MatchConfig,
    content: &Content,
    revision: Revision,
    knowledge: &Knowledge,
) -> TurnDraft {
    let mut draft = TurnDraft {
        based_on_revision: revision,
        tick: state.tick,
        commands: vec![],
    };
    let own: Vec<_> = state
        .entities
        .iter()
        .filter(|e| e.owner == player && e.lifecycle == Lifecycle::Complete)
        .collect();
    let definition = |e: &EntityState| content.types.iter().find(|d| d.key == e.type_key);
    let Some(home) = own
        .iter()
        .find(|e| e.type_key == "factory")
        .or_else(|| own.iter().find(|e| e.type_key == "constructor"))
        .or_else(|| own.first())
        .map(|e| e.tile)
    else {
        return draft;
    };
    let routes = paths(state, home, content);
    let w = usize::from(state.terrain.width);
    let variety = noise(state, player);
    let aggressive = style == Style::Raider || (state.tick < 350 && variety.is_multiple_of(4));
    let area = Rect {
        min: Tile { x: 0, y: 0 },
        max: Tile {
            x: state.terrain.width - 1,
            y: state.terrain.height - 1,
        },
    };
    let enemies: Vec<_> = state
        .entities
        .iter()
        .filter(|e| !allied(config, player, e.owner))
        .collect();
    let threat = enemies
        .iter()
        .filter(|e| {
            distance(e.tile, home) < 225 && definition(e).is_some_and(|d| d.weapon.is_some())
        })
        .min_by_key(|e| distance(e.tile, home))
        .map(|e| e.tile);
    let explore = |salt: usize| {
        // Prefer unseen reachable rooms, then sweep known space again after exploration.
        routes
            .iter()
            .enumerate()
            .filter(|(i, d)| {
                **d != u32::MAX && **d > 4 && state.terrain.cells[*i] == TerrainCell::Floor
            })
            .max_by_key(|(i, d)| {
                let unseen = !knowledge.seen[*i];
                (
                    unseen,
                    own.iter()
                        .map(|e| distance(tile(*i, state.terrain.width), e.tile))
                        .min()
                        .unwrap_or(0)
                        / 32,
                    (i.wrapping_mul(2654435761)
                        .wrapping_add(salt.wrapping_mul(1013904223)))
                        % 4096,
                    **d,
                )
            })
            .map_or(home, |(i, _)| tile(i, state.terrain.width))
    };
    // Reinforcements need a durable strategic destination: an observed structure or a
    // distant unexplored region. A transient scout must not pin all future production at home.
    let strategic_target = enemies
        .iter()
        .filter(|e| definition(e).is_some_and(|d| d.kind == TypeKind::Structure))
        .min_by_key(|e| distance(e.tile, home))
        .map(|e| e.tile)
        .or_else(|| {
            knowledge
                .targets
                .values()
                .min_by_key(|t| distance(**t, home))
                .copied()
        })
        .unwrap_or_else(|| explore(variety));
    let target = threat.unwrap_or(strategic_target);
    let count = |key: &str| own.iter().filter(|e| e.type_key == key).count();
    let bank = state
        .players
        .iter()
        .find(|p| p.player_id == player)
        .map_or(0.0, |p| p.bank);
    let miners = count("miner");
    let factories = count("factory");
    let desired_miners = (if aggressive { 6 } else { 9 }) + factories.min(4) * 2;
    let desired_builders = 2 + factories / 2;
    let desired_factories = (1 + miners / 2).min(6);
    let army: Vec<_> = own
        .iter()
        .filter(|e| {
            definition(e).is_some_and(|d| d.movement.is_some() && d.weapon.is_some())
                && e.type_key != "scout"
        })
        .collect();
    let enemy_power: f64 = enemies
        .iter()
        .filter(|e| distance(e.tile, target) < 144)
        .filter_map(|e| definition(e))
        .filter(|d| d.weapon.is_some())
        .map(|d| d.matter_cost)
        .sum();
    let our_power: f64 = army
        .iter()
        .filter_map(|e| definition(e))
        .map(|d| d.matter_cost)
        .sum();
    let rally = routes
        .iter()
        .enumerate()
        .filter(|(_, d)| **d >= 4 && **d <= 8)
        .min_by_key(|(i, _)| distance(tile(*i, state.terrain.width), target))
        .map_or(home, |(i, _)| tile(i, state.terrain.width));
    let attack = threat.is_some()
        || (army.len() >= if aggressive { 3 } else { 6 }
            && our_power >= enemy_power * if aggressive { 0.7 } else { 1.1 });
    let destination = if attack { target } else { rally };
    // Ranked independent actions work in single-order mode as well as timestamp mode.
    let mut actions: Vec<(u16, PlannedCommand)> = vec![];
    let mut add = |priority, command| actions.push((priority, command));
    let mining: Vec<_> = own
        .iter()
        .filter(|e| e.type_key == "miner" && e.action != (Order::Mine { area: area.clone() }))
        .map(|e| e.id.clone())
        .collect();
    if !mining.is_empty() {
        add(
            100,
            Command::AssignOrder {
                entities: mining,
                order: Order::Mine { area: area.clone() },
            },
        );
    }

    // Account for queued workers across every factory so simultaneous factories do not each
    // order an entire economy. Finite batches permit recovery and composition changes.
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    for e in &own {
        *totals.entry(e.type_key.clone()).or_default() += 1;
    }
    for e in &own {
        if let Some(p) = &e.production {
            for q in &p.pending_items {
                *totals.entry(q.type_key.clone()).or_default() += 1;
            }
            if let Some(q) = &p.active_item {
                *totals.entry(q.type_key.clone()).or_default() += 1;
            }
        }
    }
    let mix = if aggressive {
        [
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "tank",
            "artillery",
            "grinder",
            "grunt",
            "scout",
        ]
    } else {
        [
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "grunt",
            "tank",
            "grunt",
            "artillery",
            "tank",
            "grunt",
            "grinder",
            "scout",
        ]
    };
    for (fi, factory) in own.iter().filter(|e| e.type_key == "factory").enumerate() {
        let Some(p) = &factory.production else {
            continue;
        };
        let recipes = definition(factory)
            .and_then(|d| d.production.as_ref())
            .map(|p| &p.recipes);
        let emergency = (miners == 0 && !p.pending_items.iter().any(|q| q.type_key == "miner"))
            || (count("constructor") == 0
                && !p.pending_items.iter().any(|q| q.type_key == "constructor"));
        let queued_workers = p
            .pending_items
            .iter()
            .map(|q| q.type_key.as_str())
            .chain(p.active_item.iter().map(|q| q.type_key.as_str()))
            .any(|key| key == "miner" || key == "constructor");
        let mut worker_batch = queued_workers;
        if p.pending_items.is_empty() && p.active_item.is_none() || emergency {
            let mut items = vec![];
            worker_batch = totals.get("miner").copied().unwrap_or(0) < desired_miners
                || totals.get("constructor").copied().unwrap_or(0) < desired_builders;
            let batch_size = if config.control_limit == ControlLimit::SingleOrder && !worker_batch {
                mix.len()
            } else {
                8
            };
            for slot in 0..batch_size {
                let n = |key: &str| totals.get(key).copied().unwrap_or(0);
                let key = if worker_batch {
                    if n("constructor") < desired_builders && (slot == 1 || n("constructor") == 0) {
                        "constructor"
                    } else if n("miner") < desired_miners {
                        "miner"
                    } else {
                        break;
                    }
                } else if n("scout") == 0 {
                    "scout"
                } else {
                    mix[(slot + fi + state.tick as usize / 70 + variety % 6) % mix.len()]
                };
                if recipes.is_some_and(|r| r.iter().any(|r| r == key)) {
                    items.push(key.to_string());
                    *totals.entry(key.to_string()).or_default() += 1;
                }
            }
            if !items.is_empty() {
                add(
                    if emergency {
                        105
                    } else if config.control_limit == ControlLimit::SingleOrder && !worker_batch {
                        101
                    } else {
                        85
                    },
                    Command::EditProduction {
                        factories: vec![factory.id.clone()],
                        edit: if emergency {
                            ProductionEdit::ReplacePending { items }
                        } else {
                            ProductionEdit::Append { items }
                        },
                    },
                );
            }
        }
        // Combat queues must keep fighting through the whole published simulation. Worker
        // queues stay finite so an idle opponent cannot turn every bit of ore into miners.
        let sustain = !worker_batch;
        let default_repeat = sustain;
        if p.loop_enabled != default_repeat {
            add(
                107,
                Command::SetQueueLoop {
                    factories: vec![factory.id.clone()],
                    enabled: default_repeat,
                },
            );
        }
        {
            let looping = p
                .pending_items
                .iter()
                .filter(|q| q.loop_enabled != sustain)
                .map(|q| DraftItemRef::Persistent {
                    id: q.item_id.clone(),
                })
                .chain(
                    p.active_item
                        .iter()
                        .filter(|q| q.loop_enabled != sustain)
                        .map(|q| DraftItemRef::Persistent {
                            id: q.item_id.clone(),
                        }),
                )
                .collect::<Vec<_>>();
            if !looping.is_empty() {
                add(
                    99,
                    Command::EditProduction {
                        factories: vec![factory.id.clone()],
                        edit: ProductionEdit::SetItemLoop {
                            item_ids: looping,
                            enabled: sustain,
                        },
                    },
                );
            }
        }
        // A factory has one inherited order. Never mix workers into a combat batch:
        // miners and constructors must start earning immediately, without another human round.
        let order = if worker_batch {
            Order::Mine { area: area.clone() }
        } else {
            Order::AttackMove {
                destination: strategic_target,
            }
        };
        if p.stored_order != order {
            add(
                if worker_batch {
                    106
                } else if !matches!(p.stored_order, Order::AttackMove { .. }) {
                    if config.control_limit == ControlLimit::SingleOrder {
                        108
                    } else {
                        86
                    }
                } else {
                    35
                },
                Command::SetStoredOrder {
                    factories: vec![factory.id.clone()],
                    order,
                },
            );
        }
    }

    let projects: Vec<_> = state
        .blueprints
        .iter()
        .filter(|b| b.owner == player)
        .collect();
    let pending_factories = projects.iter().filter(|b| b.type_key == "factory").count();
    let has_builder = count("constructor") > 0;
    // With one action per turn, activate existing capacity before spending more turns
    // placing empty factories. Timestamp plans can configure and expand together.
    let idle_capacity = config.control_limit == ControlLimit::SingleOrder
        && factories >= 2
        && own.iter().any(|e| {
            e.type_key == "factory"
                && e.production
                    .as_ref()
                    .is_some_and(|p| p.pending_items.is_empty() && p.active_item.is_none())
        });
    let build_type = if factories + pending_factories < desired_factories
        && !idle_capacity
        && has_builder
        && (factories == 0 || miners >= 3 || bank > 500.0)
    {
        Some("factory")
    } else if has_builder
        && factories > 0
        && count("turret") + projects.iter().filter(|b| b.type_key == "turret").count()
            < if aggressive { 2 } else { 4 }
        && (threat.is_some() || miners >= 5)
    {
        Some("turret")
    } else {
        None
    };
    let mut placing = false;
    if let Some(key) = build_type.filter(|_| projects.len() < 2) {
        let occupied = |t: Tile| {
            state.entities.iter().any(|e| e.tile == t)
                || state.blueprints.iter().any(|b| b.tile == t)
                || state
                    .entities
                    .iter()
                    .any(|e| e.production.as_ref().is_some_and(|p| p.output_tile == t))
        };
        let site = routes
            .iter()
            .enumerate()
            .filter(|(i, d)| {
                **d >= 2
                    && **d <= if key == "turret" { 6 } else { 18 }
                    && state.ore[*i] == 0.0
                    && !occupied(tile(*i, state.terrain.width))
            })
            .filter(|(i, _)| {
                neighbors(*i, w, routes.len())
                    .iter()
                    .filter(|j| {
                        routes[**j] != u32::MAX && !occupied(tile(**j, state.terrain.width))
                    })
                    .count()
                    >= 3
            })
            .min_by_key(|(i, d)| {
                if key == "turret" {
                    distance(tile(*i, state.terrain.width), target)
                } else {
                    **d * 10 + (*i % 7) as u32
                }
            });
        if let Some((i, _)) = site {
            let t = tile(i, state.terrain.width);
            let output = neighbors(i, w, routes.len())
                .into_iter()
                .find(|j| routes[*j] != u32::MAX && !occupied(tile(*j, state.terrain.width)));
            if let Some(j) = output {
                let o = tile(j, state.terrain.width);
                let direction = if o.x > t.x {
                    CardinalDirection::E
                } else if o.x < t.x {
                    CardinalDirection::W
                } else if o.y > t.y {
                    CardinalDirection::S
                } else {
                    CardinalDirection::N
                };
                let mut tiles = vec![t];
                let mut directions = vec![direction];
                // Plan the economic opening and its follow-up military production together.
                // A timestamp turn spans the whole simulation, not only the next 90 ticks.
                if key == "factory"
                    && factories == 0
                    && pending_factories == 0
                    && config.control_limit != ControlLimit::SingleOrder
                {
                    let second = routes
                        .iter()
                        .enumerate()
                        .filter(|(k, d)| {
                            **d >= 2
                                && **d <= 12
                                && state.ore[*k] == 0.0
                                && !occupied(tile(*k, state.terrain.width))
                                && distance(tile(*k, state.terrain.width), t) >= 4
                        })
                        .filter(|(k, _)| {
                            neighbors(*k, w, routes.len())
                                .iter()
                                .filter(|j| {
                                    routes[**j] != u32::MAX
                                        && !occupied(tile(**j, state.terrain.width))
                                })
                                .count()
                                >= 3
                        })
                        .min_by_key(|(k, d)| **d * 10 + (*k % 7) as u32);
                    if let Some((k, _)) = second {
                        let second_tile = tile(k, state.terrain.width);
                        if let Some(j) = neighbors(k, w, routes.len()).into_iter().find(|j| {
                            routes[*j] != u32::MAX
                                && !occupied(tile(*j, state.terrain.width))
                                && tile(*j, state.terrain.width) != t
                                && tile(*j, state.terrain.width) != o
                        }) {
                            let output = tile(j, state.terrain.width);
                            tiles.push(second_tile);
                            directions.push(if output.x > second_tile.x {
                                CardinalDirection::E
                            } else if output.x < second_tile.x {
                                CardinalDirection::W
                            } else if output.y > second_tile.y {
                                CardinalDirection::S
                            } else {
                                CardinalDirection::N
                            });
                        }
                    }
                }
                add(
                    if factories == 0 { 95 } else { 92 },
                    Command::PlaceBlueprints {
                        type_key: key.into(),
                        tiles,
                        priority: Priority::High,
                        output_directions: if key == "factory" {
                            Some(directions)
                        } else {
                            None
                        },
                    },
                );
                placing = true;
            }
        }
    }
    let work = !projects.is_empty() || placing;
    for (i, builder) in own
        .iter()
        .filter(|e| e.type_key == "constructor")
        .enumerate()
    {
        let order = if work && (miners > 0 || i > 0 || bank >= 200.0) {
            Order::Construct { area: area.clone() }
        } else {
            Order::Mine { area: area.clone() }
        };
        if builder.action != order {
            add(
                if work { 96 } else { 98 },
                Command::AssignOrder {
                    entities: vec![builder.id.clone()],
                    order,
                },
            );
        }
    }
    for (i, scout) in own.iter().filter(|e| e.type_key == "scout").enumerate() {
        // Keep travelling until arrival; change rooms when explored or blocked.
        if matches!(scout.action, Order::AttackMove { destination } if destination != rally && !scout.goal_settled && scout.failed_move_attempts < 10)
        {
            continue;
        }
        let destination = explore(
            variety
                .wrapping_add(i * 17)
                .wrapping_add(state.tick as usize / 100),
        );
        add(
            if revision.is_multiple_of(5) { 103 } else { 75 },
            Command::AssignOrder {
                entities: vec![scout.id.clone()],
                order: Order::AttackMove { destination },
            },
        );
    }
    let mut groups: BTreeMap<(u16, u16), Vec<EntityId>> = BTreeMap::new();
    for (i, e) in army.iter().enumerate() {
        let mut goal = destination;
        if aggressive && attack && threat.is_none() && i % 4 == 0 && enemies.is_empty() {
            goal = explore(variety.wrapping_add(1));
        }
        // Wounded units regroup rather than feeding an already stronger visible force.
        if definition(e).is_some_and(|d| e.hp < d.max_hp * 0.25) && enemy_power > our_power {
            goal = rally;
        }
        if e.action != (Order::AttackMove { destination: goal }) {
            groups
                .entry((goal.x, goal.y))
                .or_default()
                .push(e.id.clone());
        }
    }
    for ((x, y), entities) in groups {
        add(
            if threat.is_some() {
                110
            } else if revision.is_multiple_of(4) {
                102
            } else {
                70
            },
            Command::AssignOrder {
                entities,
                order: Order::AttackMove {
                    destination: Tile { x, y },
                },
            },
        );
    }
    actions.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));
    for (_, command) in
        actions
            .into_iter()
            .take(if config.control_limit == ControlLimit::SingleOrder {
                1
            } else {
                64
            })
    {
        let future_orders = if matches!(command, Command::AssignOrder { .. }) {
            FutureOrderPolicy::DropAll
        } else {
            FutureOrderPolicy::Keep
        };
        let factory_placements = match &command {
            Command::PlaceBlueprints {
                type_key, tiles, ..
            } if type_key == "factory" => tiles.len(),
            _ => 0,
        };
        let local_id = format!("bot-{}", draft.commands.len());
        draft.commands.push(DraftCommand {
            local_id: local_id.clone(),
            command,
            future_orders,
        });
        for item_index in 0..factory_placements {
            if config.control_limit == ControlLimit::SingleOrder {
                break;
            }
            let economic = miners < 3 && item_index == 0;
            draft.commands.push(DraftCommand {
                local_id: format!("bot-{}", draft.commands.len()),
                command: Command::ConfigureBlueprints {
                    blueprint_ids: vec![DraftItemRef::Draft {
                        local_id: local_id.clone(),
                        item_index: item_index as u32,
                    }],
                    settings: BlueprintSettings {
                        queue: if economic {
                            ["miner", "miner", "miner", "constructor", "miner"]
                                .into_iter()
                                .map(String::from)
                                .collect()
                        } else {
                            mix.into_iter().map(String::from).collect()
                        },
                        queue_loop_flags: if economic {
                            vec![false; 5]
                        } else {
                            vec![true; mix.len()]
                        },
                        order: if economic {
                            Order::Mine { area: area.clone() }
                        } else {
                            Order::AttackMove {
                                destination: strategic_target,
                            }
                        },
                        priority: if !economic && factories == 0 {
                            Priority::Low
                        } else {
                            Priority::High
                        },
                        loop_enabled: !economic,
                        silo_plan: None,
                    },
                },
                future_orders: FutureOrderPolicy::Keep,
            });
        }
    }
    draft
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exploration_tracks_observation_and_forgets_destroyed_structures() {
        let (c, mut state) = crate::bots::tests::setup();
        let mut knowledge = Knowledge::new(&state);
        knowledge.observe(&state, 1, &c.config, &c.content);
        assert!(knowledge.seen.iter().any(|v| *v));
        assert!(knowledge.seen.iter().any(|v| !*v));
        assert!(knowledge.targets.is_empty());
        let home = state.entities.iter().find(|e| e.owner == 1).unwrap().tile;
        state
            .entities
            .iter_mut()
            .find(|e| e.owner == 0 && e.type_key == "turret")
            .unwrap()
            .tile = home;
        knowledge.observe(&state, 1, &c.config, &c.content);
        assert!(knowledge.targets.contains_key(&(home.x, home.y)));
        state
            .entities
            .retain(|e| !(e.owner == 0 && e.type_key == "turret"));
        knowledge.observe(&state, 1, &c.config, &c.content);
        assert!(knowledge.targets.is_empty());
        // Observations from a discarded revision are never carried into a fresh knowledge set.
        assert!(Knowledge::new(&state).seen.iter().all(|v| !*v));
    }
}
