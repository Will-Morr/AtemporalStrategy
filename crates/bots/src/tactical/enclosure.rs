//! An opening derived from recorded human games: build economy and constructors at home,
//! then extend covered production along two routes using only public terrain and player observations.
use super::{FrameRules, PlayerFrame, adjacent, append, distance, ore_at};
use atemporal_contracts::{
    BlueprintSettings, CardinalDirection, Command, DraftCommand, DraftItemRef, MissileLaunch,
    Order, PlayerId, Priority, SiloPlan, Terrain, Tile, TypeKey,
};
use atemporal_sim::placement::{is_floor, output_directions};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

struct Routes(BTreeMap<Tile, Option<Tile>>);

impl Routes {
    fn survey(terrain: &Terrain, blockers: &BTreeSet<Tile>, start: Tile) -> Self {
        let mut parents = BTreeMap::from([(start, None)]);
        let mut frontier = VecDeque::from([start]);
        while let Some(tile) = frontier.pop_front() {
            for next in
                output_directions(None).filter_map(|direction| adjacent(tile, direction, terrain))
            {
                if is_floor(terrain, next)
                    && !blockers.contains(&next)
                    && let std::collections::btree_map::Entry::Vacant(entry) = parents.entry(next)
                {
                    entry.insert(Some(tile));
                    frontier.push_back(next);
                }
            }
        }
        Self(parents)
    }

    fn path_to(&self, aim: Tile) -> Vec<Tile> {
        let mut current = self
            .0
            .keys()
            .min_by_key(|&&tile| distance(tile, aim))
            .copied();
        let mut path = Vec::new();
        while let Some(tile) = current {
            path.push(tile);
            current = self.0[&tile];
        }
        path.reverse();
        path
    }
}

fn approach_routes(
    terrain: &Terrain,
    blocked: &BTreeSet<Tile>,
    home: Tile,
    enemy: Tile,
) -> [Vec<Tile>; 2] {
    let edge_offset = |limit: u16, upper: bool| {
        if upper {
            limit - 1 - limit / 6
        } else {
            limit / 6
        }
    };
    let cross_x = if home.x.abs_diff(enemy.x) < terrain.width / 4 {
        edge_offset(terrain.width, home.x < terrain.width / 2)
    } else {
        enemy.x
    };
    let cross_y = if home.y.abs_diff(enemy.y) < terrain.height / 4 {
        edge_offset(terrain.height, home.y < terrain.height / 2)
    } else {
        enemy.y
    };
    let lanes = [
        Tile {
            x: cross_x,
            y: home.y,
        },
        Tile {
            x: home.x,
            y: cross_y,
        },
    ];
    let base = Routes::survey(terrain, blocked, home);
    lanes.map(|aim| {
        let mut route = base.path_to(aim);
        if let Some(&corner) = route.last() {
            let far = |a: u16, b: u16| ((u32::from(a) + 2 * u32::from(b)) / 3) as u16;
            let target = Tile {
                x: far(corner.x, enemy.x),
                y: far(corner.y, enemy.y),
            };
            route.extend(
                Routes::survey(terrain, blocked, corner)
                    .path_to(target)
                    .into_iter()
                    .skip(1),
            );
            route.dedup();
        }
        route
    })
}

struct Footprint<'a> {
    state: &'a PlayerFrame,
    terrain: &'a Terrain,
    occupied: BTreeSet<Tile>,
    outputs: BTreeSet<Tile>,
    routes: BTreeSet<Tile>,
}

impl Footprint<'_> {
    fn can_build(&self, tile: Tile) -> bool {
        is_floor(self.terrain, tile)
            && ore_at(
                self.state,
                usize::from(tile.y) * usize::from(self.terrain.width) + usize::from(tile.x),
            ) == 0.
            && !self.occupied.contains(&tile)
            && !self.outputs.contains(&tile)
            && !self.routes.contains(&tile)
            && self.neighbors(tile).any(|next| self.routes.contains(&next))
    }

    fn neighbors(&self, tile: Tile) -> impl Iterator<Item = Tile> + '_ {
        output_directions(None).filter_map(move |direction| adjacent(tile, direction, self.terrain))
    }

    fn sites_near(&self, center: Tile) -> impl Iterator<Item = Tile> + '_ {
        let xs = center.x.saturating_sub(5)..=(center.x + 5).min(self.terrain.width - 1);
        let ys = center.y.saturating_sub(5)..=(center.y + 5).min(self.terrain.height - 1);
        ys.flat_map(move |y| xs.clone().map(move |x| Tile { x, y }))
            .filter(|&tile| self.can_build(tile))
    }

    fn place(
        &mut self,
        key: &str,
        tile: Tile,
        direction: Option<CardinalDirection>,
        priority: Priority,
        commands: &mut Vec<DraftCommand>,
    ) -> String {
        self.occupied.insert(tile);
        if let Some(output) = direction.and_then(|heading| adjacent(tile, heading, self.terrain)) {
            self.outputs.insert(output);
        }
        append(
            commands,
            Command::PlaceBlueprints {
                type_key: key.into(),
                tiles: vec![tile],
                priority,
                output_directions: direction.map(|heading| vec![heading]),
            },
        )
    }
}

fn terrain_shot(terrain: &Terrain, from: Tile, to: Tile) -> bool {
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    let (mut x, mut y) = (i32::from(from.x), i32::from(from.y));
    let (tx, ty) = (i32::from(to.x), i32::from(to.y));
    let (dx, dy) = ((tx - x).abs(), -(ty - y).abs());
    let (sx, sy) = ((tx - x).signum(), (ty - y).signum());
    let mut error = dx + dy;
    let open = |x, y| {
        is_floor(
            terrain,
            Tile {
                x: x as u16,
                y: y as u16,
            },
        )
    };
    while (x, y) != (tx, ty) {
        let (old_x, old_y) = (x, y);
        let doubled = error * 2;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
        if !open(x, y) || (x != old_x && y != old_y && (!open(x, old_y) || !open(old_x, y))) {
            return false;
        }
    }
    true
}

pub(super) fn extend(
    state: &PlayerFrame,
    player: PlayerId,
    rules: FrameRules<'_>,
    [home, enemy]: [Tile; 2],
    recipes: &[TypeKey],
    commands: &mut Vec<DraftCommand>,
) -> bool {
    let FrameRules {
        content, terrain, ..
    } = rules;
    let Some(range) = content
        .types
        .iter()
        .find(|kind| kind.key == "turret")
        .and_then(|kind| kind.weapon.as_ref())
        .map(|weapon| weapon.range.powi(2))
    else {
        return false;
    };
    let shell_wall = content.types.iter().any(|kind| kind.key == "wall");
    let army: Vec<_> = ["artillery", "tank", "grunt", "scout"]
        .into_iter()
        .filter(|&kind| recipes.iter().any(|recipe| recipe == kind))
        .map(str::to_owned)
        .collect();
    if army.is_empty() {
        return false;
    }

    let mut occupied: BTreeSet<_> = state
        .own_entities
        .iter()
        .map(|unit| unit.tile)
        .chain(state.visible_entities.iter().map(|unit| unit.tile))
        .chain(state.own_blueprints.iter().map(|blueprint| blueprint.tile))
        .collect();
    let mut outputs: BTreeSet<_> = super::super::reserved_factory_outputs(state, terrain).collect();
    for draft in commands.iter() {
        if let Command::PlaceBlueprints {
            tiles,
            output_directions: dirs,
            ..
        } = &draft.command
        {
            occupied.extend(tiles.iter().copied());
            if let Some(dirs) = dirs {
                outputs.extend(
                    tiles
                        .iter()
                        .zip(dirs)
                        .filter_map(|(&tile, &dir)| adjacent(tile, dir, terrain)),
                );
            }
        }
    }
    let lanes = approach_routes(terrain, &occupied, home, enemy);
    let routes = lanes.iter().flatten().copied().collect();
    let mut ground = Footprint {
        state,
        terrain,
        occupied,
        outputs,
        routes,
    };
    let mut anchors = Vec::new();
    let first_command = commands.len();
    let mut factory_count = 0;
    for route in &lanes {
        // Small or trapped pockets cannot support a linked chain of new construction.
        if route.len() < 18 {
            continue;
        }
        for numerator in [2, 3, 4] {
            let index = route.len() * numerator / 5;
            let center = route[index];
            if anchors.iter().any(|&other| distance(center, other) < 36) {
                continue;
            }
            let approach = route[(index + 5).min(route.len() - 1)];
            let mut guns = Vec::new();
            for _ in 0..3 {
                let next = ground
                    .sites_near(center)
                    .filter(|&tile| {
                        f64::from(distance(tile, approach)) <= range
                            && terrain_shot(terrain, tile, approach)
                            && guns.iter().all(|&previous| distance(previous, tile) >= 4)
                    })
                    .min_by_key(|&tile| (distance(tile, approach), distance(tile, center), tile));
                let Some(tile) = next else { break };
                ground.place("turret", tile, None, Priority::Medium, commands);
                guns.push(tile);
            }
            if guns.is_empty() {
                continue;
            }
            anchors.push(center);
            if shell_wall {
                for turret in guns.into_iter().take(2) {
                    let cover = ground
                        .neighbors(turret)
                        .filter(|&tile| {
                            ground.can_build(tile)
                                && distance(tile, enemy) < distance(turret, enemy)
                        })
                        .min_by_key(|&tile| distance(tile, enemy));
                    if let Some(tile) = cover {
                        ground.place("wall", tile, None, Priority::Medium, commands);
                    }
                }
            }
            // Keep bay tiles open. Spawn along a traversable neighbor towards the opponent,
            // then give the mixed wave a real attack destination beyond the entrenchment.
            let factory = ground
                .sites_near(center)
                .filter_map(|tile| {
                    let (direction, output) = output_directions(None)
                        .filter_map(|direction| {
                            adjacent(tile, direction, terrain).map(|out| (direction, out))
                        })
                        .filter(|&(_, out)| {
                            ground.routes.contains(&out) && !ground.outputs.contains(&out)
                        })
                        .min_by_key(|&(_, out)| distance(out, enemy))?;
                    Some((tile, direction, output))
                })
                .min_by_key(|&(tile, _, output)| {
                    (distance(tile, center), distance(output, enemy), tile)
                });
            let Some((tile, direction, _)) = factory else {
                continue;
            };
            let id = ground.place("factory", tile, Some(direction), Priority::Medium, commands);
            factory_count += 1;
            append(
                commands,
                Command::ConfigureBlueprints {
                    blueprint_ids: vec![DraftItemRef::Draft {
                        local_id: id,
                        item_index: 0,
                    }],
                    settings: BlueprintSettings {
                        queue: army.clone(),
                        queue_loop_flags: vec![true; army.len()],
                        order: Order::AttackMove { destination: enemy },
                        priority: Priority::Medium,
                        loop_enabled: true,
                        silo_plan: None,
                    },
                },
            );
        }
    }

    if factory_count < 2 {
        commands.truncate(first_command);
        return false;
    }
    // Add reconnaissance and a siege option behind the most advanced position when stocked.
    // Known targets for later launches remain handled by Tactical's published-intel logic.
    if state
        .own_entities
        .iter()
        .any(|unit| unit.owner == player && unit.type_key == "silo")
        || !content
            .types
            .iter()
            .find(|kind| kind.key == "silo")
            .and_then(|kind| kind.production.as_ref())
            .is_some_and(|production| {
                ["satellite", "tac_nuke"]
                    .iter()
                    .all(|key| production.recipes.iter().any(|recipe| recipe == key))
            })
    {
        return true;
    }
    let silo = anchors
        .into_iter()
        .min_by_key(|&tile| distance(tile, enemy))
        .and_then(|center| {
            ground
                .sites_near(center)
                .max_by_key(|&tile| distance(tile, enemy))
        });
    if let Some(tile) = silo {
        let id = ground.place("silo", tile, None, Priority::Low, commands);
        append(
            commands,
            Command::ConfigureBlueprints {
                blueprint_ids: vec![DraftItemRef::Draft {
                    local_id: id,
                    item_index: 0,
                }],
                settings: BlueprintSettings {
                    queue: vec!["satellite".into(), "tac_nuke".into()],
                    queue_loop_flags: vec![false; 2],
                    order: Order::Idle {},
                    priority: Priority::Low,
                    loop_enabled: false,
                    silo_plan: Some(SiloPlan {
                        automatic: false,
                        launches: vec![MissileLaunch {
                            type_key: "satellite".into(),
                            target: enemy,
                        }],
                    }),
                },
            },
        );
    }
    true
}
