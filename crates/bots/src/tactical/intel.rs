use super::{OwnEntity, PlayerFrame, PolicyTimeline, allied, distance};
use atemporal_contracts::{
    Content, Direction, EntityId, Lifecycle, Neighbors, PlayerId, PresentationEvent, Terrain,
    TerrainCell, Tick, Tile, TypeKind,
};
use atemporal_sim::player_timeline::VisibleEntity;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

const ENTRY_WINDOW: Tick = 90;
const BACKTRACK_STEPS: u8 = 12;

pub(super) struct Structure {
    pub tile: Tile,
    pub worth: u32,
    pub direct: bool,
    sight_tick: Tick,
}

pub(super) struct Lead {
    pub tile: Tile,
    pub sources: u32,
    pub fresh_sources: u32,
    pub separated_approaches: bool,
    pub score: u32,
}

pub(super) struct Intel {
    pub structures: Vec<Structure>,
    pub leads: Vec<Lead>,
    pub latest_contact: Option<Tile>,
    last_checked: Vec<Option<Tick>>,
}

struct Entry {
    tile: Tile,
    behind: Tile,
    last: Tile,
    appear: Tick,
    steps: u8,
    owner: PlayerId,
    neighbors: Neighbors,
}

impl Entry {
    fn observe(&mut self, tick: Tick, tile: Tile) {
        if self.steps >= 2 || self.appear.saturating_add(ENTRY_WINDOW) < tick {
            return;
        }
        if tile == self.last {
            return;
        }
        let (dx, dy) = delta(self.last, tile);
        let (fx, fy) = delta(self.behind, self.tile);
        // A first contact moving away from the fog must continue forward in sight.
        // Do not mistake a displacement, sharp turn, or a separated sighting for a route.
        if dx.abs() <= 1 && dy.abs() <= 1 && dx * fx + dy * fy > 0 {
            self.steps += 1;
            self.last = tile;
        } else {
            self.steps = u8::MAX;
        }
    }
}

fn delta(from: Tile, to: Tile) -> (i32, i32) {
    (
        i32::from(to.x) - i32::from(from.x),
        i32::from(to.y) - i32::from(from.y),
    )
}

fn behind(tile: Tile, direction: Direction) -> Option<Tile> {
    let (dx, dy) = match direction {
        Direction::N => (0, 1),
        Direction::Ne => (-1, 1),
        Direction::E => (-1, 0),
        Direction::Se => (-1, -1),
        Direction::S => (0, -1),
        Direction::Sw => (1, -1),
        Direction::W => (1, 0),
        Direction::Nw => (1, 1),
    };
    Some(Tile {
        x: tile.x.checked_add_signed(dx)?,
        y: tile.y.checked_add_signed(dy)?,
    })
}

fn index(terrain: &Terrain, tile: Tile) -> usize {
    usize::from(tile.y) * usize::from(terrain.width) + usize::from(tile.x)
}

fn floor(terrain: &Terrain, t: Tile) -> bool {
    t.x < terrain.width
        && t.y < terrain.height
        && terrain.cells[index(terrain, t)] == TerrainCell::Floor
}

fn neighbors(terrain: &Terrain, tile: Tile, mode: Neighbors) -> impl Iterator<Item = Tile> + '_ {
    let offsets = [
        (0, 1),
        (1, 0),
        (0, -1),
        (-1, 0),
        (1, 1),
        (1, -1),
        (-1, -1),
        (-1, 1),
    ];
    let count = if mode == Neighbors::Four { 4 } else { 8 };
    offsets.into_iter().take(count).filter_map(move |(dx, dy)| {
        let x = tile.x.checked_add_signed(dx)?;
        let y = tile.y.checked_add_signed(dy)?;
        let next = Tile { x, y };
        (floor(terrain, next)
            && (dx == 0
                || dy == 0
                || floor(terrain, Tile { x, y: tile.y }) && floor(terrain, Tile { x: tile.x, y })))
        .then_some(next)
    })
}

#[derive(Default)]
struct Votes {
    sources: u32,
    fresh_sources: u32,
    fresh_ingress: BTreeSet<Tile>,
    score: u32,
}

fn plot_entry(
    entry: &Entry,
    terrain: &Terrain,
    mut can_cross: impl FnMut(Tile) -> bool,
    mut visit: impl FnMut(Tile, u32),
) {
    // Every route stays within this local window, independent of map size.
    const RADIUS: i32 = BACKTRACK_STEPS as i32;
    const WIDTH: usize = (RADIUS * 2 + 1) as usize;
    let local_index = |tile| {
        let (x, y) = delta(entry.tile, tile);
        (y + RADIUS) as usize * WIDTH + (x + RADIUS) as usize
    };
    let mut reached = [false; WIDTH * WIDTH];
    reached[local_index(entry.behind)] = true;
    let mut frontier = VecDeque::from([(entry.behind, 1_u8)]);
    let (fx, fy) = delta(entry.tile, entry.behind);
    while let Some((tile, steps)) = frontier.pop_front() {
        if steps >= 3 {
            let (dx, dy) = delta(entry.tile, tile);
            let lateral = (dx * fy - dy * fx).unsigned_abs();
            // Avoid voting for the fog boundary itself. Prefer following a corridor some
            // distance into fog, but let roads bend around public terrain.
            let score = 18 - u32::from(steps.abs_diff(7)) - lateral.min(7);
            visit(tile, score);
        }
        if steps == BACKTRACK_STEPS {
            continue;
        }
        for next in neighbors(terrain, tile, entry.neighbors) {
            let (dx, dy) = delta(entry.tile, next);
            let i = local_index(next);
            if dx * fx + dy * fy > 0 && !reached[i] && can_cross(next) {
                reached[i] = true;
                frontier.push_back((next, steps + 1));
            }
        }
    }
}

fn latest_visibility(input: &PolicyTimeline) -> Vec<Option<Tick>> {
    let terrain = &input.timeline.terrain;
    let mut last_checked = vec![None; terrain.cells.len()];
    let mut seen = vec![0; terrain.cells.len().div_ceil(64)];
    let mut remaining = terrain.cells.len();
    let mut previous = &[][..];
    for frame in input.timeline.frames().rev() {
        if input.cancelled() {
            break;
        }
        let words = frame.visible_tile_words();
        if std::ptr::eq(words, previous) {
            continue;
        }
        previous = words;
        for (word_index, (&word, seen_word)) in words.iter().zip(&mut seen).enumerate() {
            let mut added = word & !*seen_word;
            *seen_word |= added;
            while added != 0 {
                let bit = added.trailing_zeros() as usize;
                last_checked[word_index * 64 + bit] = Some(frame.tick);
                added &= added - 1;
                remaining -= 1;
            }
        }
        if remaining == 0 {
            break;
        }
    }
    last_checked
}

fn hostile_structure(content: &Content, entity: &VisibleEntity) -> Option<u32> {
    let def = content
        .types
        .iter()
        .find(|def| def.key == entity.type_key)?;
    if def.kind != TypeKind::Structure {
        return None;
    }
    match entity.type_key.as_str() {
        "silo" => Some(8),
        "factory" => Some(6),
        "turret" => Some(3),
        _ if def.counts_for_survival => Some(4),
        _ => None,
    }
}

impl Intel {
    pub(super) fn scan(input: &PolicyTimeline, fresh_entry_ticks: Tick) -> Self {
        let terrain = &input.timeline.terrain;
        let last = input
            .timeline
            .frames()
            .next_back()
            .expect("published timeline");
        let last_checked = latest_visibility(input);
        let hostile = |e: &VisibleEntity| !allied(input.config(), input.player(), e.owner);
        let mut structures: Vec<_> = last
            .visible_entities
            .iter()
            .map(|e| (e.as_ref(), true, last.tick))
            .chain(
                input
                    .timeline
                    .remembered_buildings(last.tick)
                    .map(|s| (s.entity, false, s.tick)),
            )
            .filter(|(e, _, _)| hostile(e))
            .filter_map(|(e, direct, sight_tick)| {
                let worth = hostile_structure(input.content(), e)?;
                Some(Structure {
                    tile: e.tile,
                    worth,
                    direct,
                    sight_tick,
                })
            })
            .collect();
        let latest_contact = input
            .timeline
            .last_seen_entities(last.tick)
            .filter(|s| hostile(s.entity) && last.tick.saturating_sub(s.tick) <= 80)
            .filter(|s| {
                input
                    .content()
                    .types
                    .iter()
                    .any(|d| d.key == s.entity.type_key && d.movement.is_some())
            })
            .filter(|s| {
                last_checked[index(terrain, s.entity.tile)].is_none_or(|checked| checked <= s.tick)
            })
            .max_by_key(|s| s.tick)
            .map(|s| s.entity.tile);

        let recipes: BTreeMap<_, _> = input
            .content()
            .types
            .iter()
            .filter(|t| t.key == "factory")
            .filter_map(|t| t.production.as_ref())
            .flat_map(|p| &p.recipes)
            .filter_map(|key| {
                let movement = input
                    .content()
                    .types
                    .iter()
                    .find(|def| def.key == *key)?
                    .movement
                    .as_ref()?;
                Some((key.as_str(), movement.neighbors))
            })
            .collect();
        let mut sightings = BTreeMap::<&EntityId, Entry>::new();
        let mut previous: Option<&PlayerFrame> = None;
        let mut struck = vec![None; terrain.cells.len()];
        for frame in input.timeline.frames() {
            if input.cancelled() {
                break;
            }
            for event in &frame.events {
                if let PresentationEvent::MissileImpact {
                    owner,
                    type_key,
                    target,
                    radius,
                } = &event.event
                    && allied(input.config(), input.player(), *owner)
                    && type_key == "tac_nuke"
                {
                    for t in super::blast_tiles(*target, *radius)
                        .filter(|t| t.x < terrain.width && t.y < terrain.height)
                    {
                        struck[index(terrain, t)] = Some(frame.tick);
                    }
                }
            }
            if let Some(old) = previous
                && Arc::ptr_eq(&old.visible_entities, &frame.visible_entities)
            {
                previous = Some(frame);
                continue;
            }
            for e in frame.visible_entities.iter().filter(|e| hostile(e)) {
                let Some(&mode) = recipes.get(e.type_key.as_str()) else {
                    continue;
                };
                let Some(old) = previous else {
                    continue;
                };
                let row = sightings.entry(&e.id);
                if matches!(&row, std::collections::btree_map::Entry::Occupied(entry) if entry.get().steps == 2)
                {
                    continue;
                }
                if old
                    .visible_entities
                    .binary_search_by(|seen| seen.id.cmp(&e.id))
                    .is_ok()
                {
                    if let std::collections::btree_map::Entry::Occupied(mut entry) = row {
                        entry.get_mut().observe(frame.tick, e.tile);
                    }
                    continue;
                }
                let candidate = behind(e.tile, e.last_move_direction)
                    .filter(|&from| {
                        e.lifecycle == Lifecycle::Complete
                            && old.sees(e.tile)
                            && !old.sees(from)
                            && neighbors(terrain, from, mode).any(|next| next == e.tile)
                            && floor(terrain, from)
                    })
                    .map(|from| Entry {
                        tile: e.tile,
                        behind: from,
                        last: e.tile,
                        appear: frame.tick,
                        steps: 0,
                        owner: e.owner,
                        neighbors: mode,
                    });
                match (row, candidate) {
                    (std::collections::btree_map::Entry::Occupied(mut row), Some(entry)) => {
                        row.insert(entry);
                    }
                    (std::collections::btree_map::Entry::Vacant(row), Some(entry)) => {
                        row.insert(entry);
                    }
                    (std::collections::btree_map::Entry::Occupied(row), None) => {
                        row.remove();
                    }
                    _ => {}
                }
            }
            previous = Some(frame);
        }
        structures
            .retain(|s| struck[index(terrain, s.tile)].is_none_or(|tick| tick <= s.sight_tick));
        let mut votes = BTreeMap::<(PlayerId, Tile), Votes>::new();
        for entry in sightings.into_values().filter(|e| e.steps == 2) {
            if input.cancelled() {
                break;
            }
            let arrival = input
                .timeline
                .frame(entry.appear - 1)
                .expect("entry has a previous frame");
            plot_entry(
                &entry,
                terrain,
                |tile| !arrival.sees(tile),
                |tile, score| {
                    let i = index(terrain, tile);
                    if last_checked[i].is_some_and(|tick| tick >= entry.appear)
                        || struck[i].is_some_and(|tick| tick >= entry.appear)
                        || super::ore_at(last, i) > 0.
                    {
                        return;
                    }
                    let vote = votes.entry((entry.owner, tile)).or_default();
                    vote.sources += 1;
                    if last.tick - entry.appear <= fresh_entry_ticks {
                        vote.fresh_sources += 1;
                        vote.fresh_ingress.insert(entry.tile);
                    }
                    vote.score += score;
                },
            );
        }
        let mut leads: Vec<_> = votes
            .into_iter()
            .filter_map(|((_, tile), v)| {
                (v.sources >= 2).then_some(Lead {
                    tile,
                    sources: v.sources,
                    fresh_sources: v.fresh_sources,
                    separated_approaches: v
                        .fresh_ingress
                        .iter()
                        .any(|&a| v.fresh_ingress.iter().any(|&b| distance(a, b) >= 9)),
                    score: v.score,
                })
            })
            .collect();
        leads.sort_by_key(|lead| std::cmp::Reverse((lead.fresh_sources, lead.sources, lead.score)));
        Self {
            structures,
            leads,
            latest_contact,
            last_checked,
        }
    }

    pub(super) fn checked_after(&self, terrain: &Terrain, tile: Tile, tick: Tick) -> bool {
        tile.x < terrain.width
            && tile.y < terrain.height
            && self.last_checked[index(terrain, tile)].is_some_and(|checked| checked >= tick)
    }

    pub(super) fn rally(
        &self,
        input: &PolicyTimeline,
        view: &PlayerFrame,
        distance_band: u8,
    ) -> Option<Tile> {
        let is_armed = |e: &OwnEntity| {
            input
                .content()
                .types
                .iter()
                .any(|d| d.key == e.type_key && d.movement.is_some() && d.weapon.is_some())
        };
        let army: Vec<_> = view
            .own_entities
            .iter()
            .filter(|e| e.lifecycle == Lifecycle::Complete && is_armed(e))
            .map(|e| e.tile)
            .collect();
        let home = view
            .own_entities
            .iter()
            .find(|e| e.type_key == "factory")
            .or_else(|| view.own_entities.first())?
            .tile;
        let near = |tile: Tile| {
            army.iter()
                .map(|&p| distance(p, tile))
                .min()
                .unwrap_or_else(|| distance(home, tile))
        };
        if let Some(hostile) = view
            .visible_entities
            .iter()
            .filter(|e| !allied(input.config(), input.player(), e.owner))
            .filter(|e| {
                input
                    .content()
                    .types
                    .iter()
                    .any(|d| d.key == e.type_key && d.weapon.is_some())
            })
            .filter(|e| near(e.tile) <= 36 || distance(home, e.tile) <= 49)
            .min_by_key(|e| near(e.tile))
        {
            return Some(hostile.tile);
        }
        if self.structures.is_empty() && self.leads.is_empty() && self.latest_contact.is_none() {
            return None;
        }
        let terrain = &input.timeline.terrain;
        let blocked: BTreeSet<_> = view
            .own_entities
            .iter()
            .filter(|e| e.type_key == "wall")
            .map(|e| e.tile)
            .chain(
                view.visible_entities
                    .iter()
                    .filter(|e| e.type_key == "wall")
                    .map(|e| e.tile),
            )
            .collect();
        let mut routes = vec![u32::MAX; terrain.cells.len()];
        let mut frontier = VecDeque::new();
        for start in army.iter().copied().chain(std::iter::once(home)) {
            routes[index(terrain, start)] = 0;
            frontier.push_back(start);
        }
        while let Some(tile) = frontier.pop_front() {
            let steps = routes[index(terrain, tile)] + 1;
            for next in neighbors(terrain, tile, Neighbors::Four) {
                let i = index(terrain, next);
                if routes[i] == u32::MAX && !blocked.contains(&next) {
                    routes[i] = steps;
                    frontier.push_back(next);
                }
            }
        }
        let route_to = |tile| routes[index(terrain, tile)];
        self.structures
            .iter()
            .filter(|s| route_to(s.tile) != u32::MAX)
            .min_by_key(|s| {
                (
                    !s.direct,
                    route_to(s.tile) / u32::from(distance_band.max(1)),
                    std::cmp::Reverse(s.worth),
                    route_to(s.tile),
                )
            })
            .map(|s| s.tile)
            .or_else(|| {
                self.leads
                    .iter()
                    .find(|lead| route_to(lead.tile) != u32::MAX)
                    .map(|lead| lead.tile)
            })
            .or_else(|| {
                self.latest_contact
                    .filter(|&tile| route_to(tile) != u32::MAX)
            })
    }
}
