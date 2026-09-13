//! Deterministic cave map and genesis world: seeded cellular-automaton rooms, start clearings
//! with ore patches, corridors carved to the center, pocket removal and start access validation.
//! Symmetric maps are invariant under 180° rotation (2 players) or 90° rotation (4 players);
//! asymmetric mode keeps the same start layout with unmirrored rock.
use crate::*;
use std::collections::VecDeque;

const FILL_PERCENT: u64 = 50;
const SMOOTHING_PASSES: usize = 4;
const START_CLEARING: i32 = 8;

fn mix(seed: u64, x: u64, y: u64) -> u64 {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    for v in [x, y] {
        h ^= v.wrapping_add(0x9E37_79B9_7F4A_7C15);
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 31;
    }
    h
}

pub struct Start {
    pub anchor: Tile,
    pub entities: Vec<(TypeKey, Tile, Direction)>,
    pub ore: Vec<Tile>,
}

/// Quarter turns clockwise about the square's center.
fn rotate(t: Tile, size: u16, quarters: u8) -> Tile {
    let mut t = t;
    for _ in 0..quarters % 4 {
        t = Tile {
            x: size - 1 - t.y,
            y: t.x,
        };
    }
    t
}

fn rotate_direction(d: Direction, quarters: u8) -> Direction {
    let all = [
        Direction::N,
        Direction::Ne,
        Direction::E,
        Direction::Se,
        Direction::S,
        Direction::Sw,
        Direction::W,
        Direction::Nw,
    ];
    let i = all.iter().position(|x| *x == d).unwrap();
    all[(i + 2 * usize::from(quarters % 4)) % 8]
}

/// Rotation applied to the player-0 start for each player, by player count.
fn start_turns(players: u8) -> Vec<u8> {
    match players {
        2 => vec![0, 2],
        3 => vec![0, 2, 1],
        _ => vec![0, 1, 2, 3],
    }
}

/// Symmetry group of the terrain: quarter turns that must leave it unchanged.
fn symmetry_turns(config: &MatchConfig) -> Vec<u8> {
    match (config.symmetric, config.player_count) {
        (false, _) => vec![0],
        (true, 4) => vec![0, 1, 2, 3],
        (true, _) => vec![0, 2],
    }
}

/// Player 0 keeps the slice's top-left layout; other starts are its rotations.
pub fn starts(size: u16, players: u8, content: &Content) -> Result<Vec<Start>> {
    if size < 16 {
        return Err("map generation needs at least 16 tiles".into());
    }
    let anchor = Tile { x: 4, y: 4 };
    let slots = [
        Tile { x: 4, y: 7 },
        Tile { x: 6, y: 5 },
        Tile { x: 3, y: 3 },
    ];
    let mut entities = vec![];
    for (i, key) in content.starting_roster.iter().enumerate() {
        let tile = slots
            .get(i)
            .copied()
            .ok_or("starting roster exceeds start slots")?;
        entities.push((key.clone(), tile, Direction::Se));
    }
    let ore: Vec<Tile> = (2..5)
        .flat_map(|x| (9..12).map(move |y| Tile { x, y }))
        .collect();
    Ok(start_turns(players)
        .into_iter()
        .map(|q| Start {
            anchor: rotate(anchor, size, q),
            entities: entities
                .iter()
                .map(|(k, t, d)| (k.clone(), rotate(*t, size, q), rotate_direction(*d, q)))
                .collect(),
            ore: ore.iter().map(|t| rotate(*t, size, q)).collect(),
        })
        .collect())
}

struct Grid {
    size: u16,
    cells: Vec<TerrainCell>,
}

impl Grid {
    fn n(&self) -> usize {
        usize::from(self.size)
    }
    fn idx(&self, t: Tile) -> usize {
        usize::from(t.y) * self.n() + usize::from(t.x)
    }
    fn tile(&self, x: i32, y: i32) -> Option<Tile> {
        (x >= 0 && y >= 0 && x < i32::from(self.size) && y < i32::from(self.size)).then_some(Tile {
            x: x as u16,
            y: y as u16,
        })
    }
    /// Rock cells in the 3×3 block around `t`, counting out-of-bounds as rock.
    fn wall_neighbors(&self, t: Tile) -> usize {
        let mut count = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                match self.tile(i32::from(t.x) + dx, i32::from(t.y) + dy) {
                    Some(n) if self.cells[self.idx(n)] == TerrainCell::Floor => {}
                    _ => count += 1,
                }
            }
        }
        count
    }
    /// Copy every cell from the lexicographically smallest tile of its rotation orbit.
    fn symmetrize(&mut self, turns: &[u8]) {
        let before = self.cells.clone();
        for y in 0..self.size {
            for x in 0..self.size {
                let t = Tile { x, y };
                let source = turns
                    .iter()
                    .map(|q| rotate(t, self.size, *q))
                    .min()
                    .unwrap();
                let (to, from) = (self.idx(t), self.idx(source));
                self.cells[to] = before[from];
            }
        }
    }
    fn carve(&mut self, t: Tile, turns: &[u8]) {
        for q in turns {
            let r = rotate(t, self.size, *q);
            let i = self.idx(r);
            self.cells[i] = TerrainCell::Floor;
        }
    }
    /// Shortest four-neighbor path through rock from the nearest disconnected floor cell into
    /// `main`, or none when every floor cell is already connected.
    fn corridor_to(&self, main: &[bool]) -> Option<Vec<Tile>> {
        let cells = self.n() * self.n();
        let mut dist = vec![u32::MAX; cells];
        let mut parent: Vec<Option<Tile>> = vec![None; cells];
        let mut queue = VecDeque::new();
        for y in 0..self.size {
            for x in 0..self.size {
                let t = Tile { x, y };
                if main[self.idx(t)] {
                    dist[self.idx(t)] = 0;
                    queue.push_back(t);
                }
            }
        }
        while let Some(t) = queue.pop_front() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let Some(n) = self.tile(i32::from(t.x) + dx, i32::from(t.y) + dy) else {
                    continue;
                };
                if n.x == 0 || n.y == 0 || n.x == self.size - 1 || n.y == self.size - 1 {
                    continue;
                }
                if dist[self.idx(n)] == u32::MAX {
                    dist[self.idx(n)] = dist[self.idx(t)] + 1;
                    parent[self.idx(n)] = Some(t);
                    queue.push_back(n);
                }
            }
        }
        let mut best: Option<(u32, Tile)> = None;
        for y in 0..self.size {
            for x in 0..self.size {
                let t = Tile { x, y };
                let i = self.idx(t);
                if self.cells[i] == TerrainCell::Floor
                    && !main[i]
                    && dist[i] != u32::MAX
                    && best.is_none_or(|b| (dist[i], t) < b)
                {
                    best = Some((dist[i], t));
                }
            }
        }
        let (_, mut t) = best?;
        let mut path = vec![t];
        while let Some(p) = parent[self.idx(t)] {
            path.push(p);
            t = p;
        }
        Some(path)
    }
    /// Four-neighbor floor component containing `from`.
    fn component(&self, from: Tile) -> Vec<bool> {
        let mut seen = vec![false; self.n() * self.n()];
        if self.cells[self.idx(from)] != TerrainCell::Floor {
            return seen;
        }
        let mut queue = VecDeque::from([from]);
        seen[self.idx(from)] = true;
        while let Some(t) = queue.pop_front() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let Some(n) = self.tile(i32::from(t.x) + dx, i32::from(t.y) + dy) else {
                    continue;
                };
                if self.cells[self.idx(n)] == TerrainCell::Floor && !seen[self.idx(n)] {
                    seen[self.idx(n)] = true;
                    queue.push_back(n);
                }
            }
        }
        seen
    }
}

/// Terrain only; deterministic for (seed, size, players, symmetric) and independent of content.
pub fn terrain(config: &MatchConfig, starts: &[Start]) -> Result<Terrain> {
    let size = config.map_size;
    let turns = symmetry_turns(config);
    let mut grid = Grid {
        size,
        cells: vec![TerrainCell::Floor; usize::from(size) * usize::from(size)],
    };
    let seed = config.seed.get();
    for y in 0..size {
        for x in 0..size {
            let t = Tile { x, y };
            let i = grid.idx(t);
            if mix(seed, u64::from(t.x), u64::from(t.y)) % 100 < FILL_PERCENT {
                grid.cells[i] = TerrainCell::Wall;
            }
        }
    }
    grid.symmetrize(&turns);
    let near_start = |t: Tile| {
        starts.iter().any(|s| {
            (i32::from(s.anchor.x) - i32::from(t.x)).abs() <= START_CLEARING
                && (i32::from(s.anchor.y) - i32::from(t.y)).abs() <= START_CLEARING
        })
    };
    for _ in 0..SMOOTHING_PASSES {
        let mut next = grid.cells.clone();
        for y in 0..size {
            for x in 0..size {
                let t = Tile { x, y };
                next[grid.idx(t)] = if grid.wall_neighbors(t) >= 5 {
                    TerrainCell::Wall
                } else {
                    TerrainCell::Floor
                };
            }
        }
        grid.cells = next;
    }
    for y in 0..size {
        for x in 0..size {
            let t = Tile { x, y };
            let border = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            let i = grid.idx(t);
            if border {
                grid.cells[i] = TerrainCell::Wall;
            } else if near_start(t) {
                grid.cells[i] = TerrainCell::Floor;
            }
        }
    }
    // Corridor from the player-0 anchor to the center block; its rotations serve the others.
    let center = i32::from(size / 2);
    let a = starts[0].anchor;
    let steps = (center - i32::from(a.x))
        .abs()
        .max((center - i32::from(a.y)).abs());
    for k in 0..=steps {
        let x = i32::from(a.x) + (center - i32::from(a.x)) * k / steps;
        let y = i32::from(a.y) + (center - i32::from(a.y)) * k / steps;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)] {
            if let Some(t) = grid.tile(x + dx, y + dy)
                && t.x > 0
                && t.y > 0
                && t.x < size - 1
                && t.y < size - 1
            {
                grid.carve(t, &turns);
            }
        }
    }
    for dy in -1..=0 {
        for dx in -1..=0 {
            if let Some(t) = grid.tile(center + dx, center + dy) {
                grid.carve(t, &turns);
            }
        }
    }
    grid.symmetrize(&turns);
    // Dig the shortest corridor from each remaining room to the connected body, with its
    // rotations, until every floor cell is reachable from the first start.
    loop {
        let main = grid.component(starts[0].anchor);
        let Some(path) = grid.corridor_to(&main) else {
            break;
        };
        for t in path {
            grid.carve(t, &turns);
        }
    }
    let main = grid.component(starts[0].anchor);
    for (i, reachable) in main.iter().enumerate() {
        if !reachable {
            grid.cells[i] = TerrainCell::Wall;
        }
    }
    for s in starts {
        let start_cells = s.ore.iter().chain(s.entities.iter().map(|(_, t, _)| t));
        for t in std::iter::once(&s.anchor).chain(start_cells) {
            if !main[grid.idx(*t)] {
                return Err(format!(
                    "start at {:?} cannot reach the first start",
                    s.anchor
                ));
            }
        }
    }
    Ok(Terrain {
        width: size,
        height: size,
        cells: grid.cells,
    })
}

pub fn generate(config: &MatchConfig, content: &Content) -> Result<WorldState> {
    let content = normalize_content(content.clone())?;
    atemporal_content::validate_config(config)?;
    let size = config.map_size;
    let n = usize::from(size);
    let idx = |t: Tile| usize::from(t.y) * n + usize::from(t.x);
    let starts = starts(size, config.player_count, &content)?;
    let terrain = terrain(config, &starts)?;
    let mut ore = vec![0.0; n * n];
    for s in &starts {
        let per_tile = config.ore_matter_per_start / s.ore.len() as f64;
        for t in &s.ore {
            ore[idx(*t)] = per_tile;
        }
    }
    let mut entities = vec![];
    for (player, s) in starts.iter().enumerate() {
        for (slot, (key, tile, facing)) in s.entities.iter().enumerate() {
            let def = content
                .types
                .iter()
                .find(|t| t.key == *key)
                .ok_or("unknown roster type")?;
            entities.push(EntityState {
                id: identity::genesis(player as u8, slot as u32)?,
                owner: player as u8,
                type_key: key.clone(),
                tile: *tile,
                last_move_direction: *facing,
                hp: def.max_hp,
                paid_matter: 0.0,
                lifecycle: Lifecycle::Complete,
                blueprint_id: None,
                action: Order::Idle {},
                priority: Priority::Medium,
                next_action_tick: 0,
                next_move_tick: 0,
                production: None,
                support_target: None,
                engaged_target: None,
                resolved_destination: None,
                failed_move_attempts: 0,
                blocked_step: None,
                born_at_tick: None,
                order_locks: vec![],
                goal_settled: false,
                local_detour: vec![],
            });
        }
    }
    let players = (0..config.player_count)
        .map(|player_id| PlayerState {
            player_id,
            bank: config.starting_matter,
            counters: SpendCounters {
                mined: 0.0,
                total_spend: 0.0,
                unit_spend: 0.0,
                structure_spend: 0.0,
                lost_invested_matter: 0.0,
                destroyed_replacement_value: 0.0,
                damage_dealt: 0.0,
            },
            currently_eliminated: false,
            status_since_tick: 0,
            elimination_reasons: vec![],
        })
        .collect();
    let control_groups = (0..config.player_count)
        .flat_map(|owner| {
            (0..10u8).map(move |slot| ControlGroupState {
                id: ControlGroupId {
                    owner,
                    slot: slot.try_into().unwrap(),
                },
                members: vec![],
                latest_order: None,
                order_locks: vec![],
            })
        })
        .collect();
    identity::canonical_world(&WorldState {
        schema_version: Version::default(),
        tick: 0,
        last_progress_tick: 0,
        inactivity_deadline: config.stall_ticks,
        terrain,
        ore,
        players,
        entities,
        blueprints: vec![],
        control_groups,
        survival_transitions: vec![],
        rng_state: "counter-hash:v1".into(),
    })
}
