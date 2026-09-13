//! Deterministic cave map and genesis world: seeded blob rooms on a jittered lattice joined by
//! wandering corridors with loops, start clearings, pocket removal, start access validation and
//! ore clusters of one to nine tiles grown along room walls, weighted toward small deposits.
//! Symmetric maps are invariant under 180° rotation (2 players) or 90° rotation (4 players);
//! asymmetric mode keeps the same start layout with unmirrored rock.
use crate::tick::chebyshev;
use crate::*;
use std::collections::VecDeque;

/// Share of lattice cells that hold a room, and of redundant lattice edges kept as loops.
const ROOM_PERCENT: u64 = 90;
const LOOP_PERCENT: u64 = 45;
/// Radius of the round open clearing around each start anchor.
const START_CLEARING: i32 = 7;
/// Chebyshev radius within which each start is guaranteed a first ore cluster.
const NEAR_START: i32 = 9;
/// Map cells per ore cluster; the seed stream retries placement until this many are placed.
const CELLS_PER_CLUSTER: usize = 96;
const MAX_CLUSTER: usize = 9;

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
    Ok(start_turns(players)
        .into_iter()
        .map(|q| Start {
            anchor: rotate(anchor, size, q),
            entities: entities
                .iter()
                .map(|(k, t, d)| (k.clone(), rotate(*t, size, q), rotate_direction(*d, q)))
                .collect(),
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

/// Cave rooms on a jittered lattice: seeded blob rooms in most lattice cells, a random
/// spanning tree of corridors between lattice neighbours plus extra loop corridors, then
/// rotational union, start clearings, border walls and pocket removal.
/// Deterministic for (seed, size, players, symmetric) and independent of content.
pub fn terrain(config: &MatchConfig, starts: &[Start]) -> Result<Terrain> {
    let size = config.map_size;
    let n = usize::from(size);
    let turns = symmetry_turns(config);
    let seed = config.seed.get();
    let mut draw = 0u64;
    let mut next = |salt: u64| {
        draw += 1;
        mix(seed, draw, salt)
    };
    let mut floor = vec![false; n * n];
    let carve = |floor: &mut Vec<bool>, x: i32, y: i32| {
        if x > 0 && y > 0 && x < i32::from(size) - 1 && y < i32::from(size) - 1 {
            floor[y as usize * n + x as usize] = true;
        }
    };
    // Lattice of cells; every start anchor lies in a room cell.
    let cell = (i32::from(size) / 4).clamp(8, 16);
    let cols = (i32::from(size) / cell).max(2);
    let pitch = i32::from(size) as f64 / f64::from(cols);
    let mut rooms: Vec<Option<(i32, i32)>> = vec![];
    for cy in 0..cols {
        for cx in 0..cols {
            let holds_start = starts.iter().any(|s| {
                (f64::from(s.anchor.x) / pitch) as i32 == cx
                    && (f64::from(s.anchor.y) / pitch) as i32 == cy
            });
            if !holds_start && next(1) % 100 >= ROOM_PERCENT {
                rooms.push(None);
                continue;
            }
            let jitter = (pitch * 0.15) as u64 * 2 + 1;
            let x = (f64::from(cx) * pitch + pitch / 2.0) as i32 + (next(2) % jitter) as i32
                - (jitter / 2) as i32;
            let y = (f64::from(cy) * pitch + pitch / 2.0) as i32 + (next(3) % jitter) as i32
                - (jitter / 2) as i32;
            rooms.push(Some((x, y)));
        }
    }
    for room in &rooms {
        let Some((x0, y0)) = *room else { continue };
        let rx = pitch * (0.28 + 0.14 * (next(4) % 100) as f64 / 100.0);
        let ry = pitch * (0.28 + 0.14 * (next(5) % 100) as f64 / 100.0);
        let blob = next(6);
        let r = rx.max(ry).ceil() as i32 + 1;
        for dy in -r..=r {
            for dx in -r..=r {
                let nx = f64::from(dx) / rx;
                let ny = f64::from(dy) / ry;
                let rough = (mix(blob, (dx + r) as u64, (dy + r) as u64) % 100) as f64 / 100.0;
                if nx * nx + ny * ny < 1.0 - 0.35 * rough {
                    carve(&mut floor, x0 + dx, y0 + dy);
                }
            }
        }
    }
    // Corridors: random spanning tree over lattice neighbours, then extra edges for loops.
    let mut edges: Vec<(u64, usize, usize)> = vec![];
    for cy in 0..cols {
        for cx in 0..cols {
            let a = (cy * cols + cx) as usize;
            if rooms[a].is_none() {
                continue;
            }
            for (nx, ny) in [(cx + 1, cy), (cx, cy + 1)] {
                if nx < cols && ny < cols {
                    let b = (ny * cols + nx) as usize;
                    if rooms[b].is_some() {
                        edges.push((next(7), a, b));
                    }
                }
            }
        }
    }
    edges.sort();
    let mut parent: Vec<usize> = (0..rooms.len()).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (_, a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        } else if next(8) % 100 >= LOOP_PERCENT {
            continue;
        }
        let (ax, ay) = rooms[a].unwrap();
        let (bx, by) = rooms[b].unwrap();
        // Bend through a random waypoint so corridors wander instead of running straight.
        let wx = ax + (bx - ax) * (25 + (next(9) % 50) as i32) / 100 + (next(10) % 5) as i32 - 2;
        let wy = ay + (by - ay) * (25 + (next(11) % 50) as i32) / 100 + (next(12) % 5) as i32 - 2;
        let wide = next(13) % 3 == 0;
        for (from, to) in [((ax, ay), (wx, wy)), ((wx, wy), (bx, by))] {
            // Straight line at any angle; the 2×2 stamp keeps diagonal legs walkable.
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let count = dx.abs().max(dy.abs());
            let steps: Vec<(i32, i32)> = (0..=count)
                .map(|k| {
                    let along = |d: i32| {
                        if count == 0 {
                            0
                        } else {
                            (d * k + count / 2).div_euclid(count)
                        }
                    };
                    (from.0 + along(dx), from.1 + along(dy))
                })
                .collect();
            for (x, y) in steps {
                for (ox, oy) in [(0, 0), (1, 0), (0, 1)] {
                    carve(&mut floor, x + ox, y + oy);
                }
                if wide {
                    carve(&mut floor, x + 1, y + 1);
                }
            }
        }
    }
    let mut grid = Grid {
        size,
        cells: vec![TerrainCell::Wall; n * n],
    };
    let near_start = |t: Tile| {
        starts.iter().any(|s| {
            let (dx, dy) = (
                i32::from(s.anchor.x) - i32::from(t.x),
                i32::from(s.anchor.y) - i32::from(t.y),
            );
            dx * dx + dy * dy <= START_CLEARING * START_CLEARING
        })
    };
    for y in 0..size {
        for x in 0..size {
            let t = Tile { x, y };
            let border = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            let open = turns.iter().any(|q| floor[grid.idx(rotate(t, size, *q))]);
            if !border && (open || near_start(t)) {
                let i = grid.idx(t);
                grid.cells[i] = TerrainCell::Floor;
            }
        }
    }
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
        let start_cells = s.entities.iter().map(|(_, t, _)| t);
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

/// Chebyshev distance from each floor cell to the nearest wall (walls and the border are 0).
fn wall_distance(grid: &Grid) -> Vec<u16> {
    let n = grid.n();
    let mut dist = vec![u16::MAX; n * n];
    let mut queue = VecDeque::new();
    for y in 0..grid.size {
        for x in 0..grid.size {
            let t = Tile { x, y };
            if grid.cells[grid.idx(t)] != TerrainCell::Floor {
                dist[grid.idx(t)] = 0;
                queue.push_back(t);
            }
        }
    }
    while let Some(t) = queue.pop_front() {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(o) = grid.tile(i32::from(t.x) + dx, i32::from(t.y) + dy) else {
                    continue;
                };
                if dist[grid.idx(o)] == u16::MAX {
                    dist[grid.idx(o)] = dist[grid.idx(t)] + 1;
                    queue.push_back(o);
                }
            }
        }
    }
    dist
}

/// Ore tiles: clusters grown along the walls of open rooms, never in corridors or room
/// interiors, each placed with its rotations and never touching another cluster. The first
/// cluster sits within reach of each start; sizes 1..=9 are weighted toward one.
pub fn ore_tiles(config: &MatchConfig, terrain: &Terrain, starts: &[Start]) -> Vec<Tile> {
    let size = config.map_size;
    let n = usize::from(size);
    let turns = symmetry_turns(config);
    let grid = Grid {
        size,
        cells: terrain.cells.clone(),
    };
    let dist = wall_distance(&grid);
    let seed = config.seed.get() ^ 0x5DEE_CE66_D1B4_2F0D;
    let keep_out = (i32::from(size) / 8).clamp(2, 5);
    let allowed = |t: Tile| {
        let d = dist[grid.idx(t)];
        (1..=2).contains(&d)
            && starts.iter().all(|s| {
                chebyshev(s.anchor, t) > keep_out && s.entities.iter().all(|(_, e, _)| *e != t)
            })
    };
    // Room edges: wall-hugging floor with a 5×5 open block nearby, so corridors are skipped.
    let edge = |t: Tile| {
        allowed(t)
            && dist[grid.idx(t)] == 1
            && (-3..=3).any(|dy| {
                (-3..=3).any(|dx| {
                    grid.tile(i32::from(t.x) + dx, i32::from(t.y) + dy)
                        .is_some_and(|o| dist[grid.idx(o)] >= 3)
                })
            })
    };
    let heads: Vec<Tile> = (0..size)
        .flat_map(|y| (0..size).map(move |x| Tile { x, y }))
        .filter(|t| edge(*t))
        .collect();
    if heads.is_empty() {
        return vec![];
    }
    let mut ore = vec![false; n * n];
    let touching = |ore: &[bool], t: Tile| {
        (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                grid.tile(i32::from(t.x) + dx, i32::from(t.y) + dy)
                    .is_some_and(|o| ore[grid.idx(o)])
            })
        })
    };
    let target = (n * n / CELLS_PER_CLUSTER).max(turns.len());
    let mut placed = 0;
    let mut tiles = vec![];
    let mut draw = 0u64;
    let mut next = |salt: u64| {
        draw += 1;
        mix(seed, draw, salt)
    };
    // Symmetric maps cover every start with rotations of the first; asymmetric ones need each.
    let near_starts: Vec<Tile> = if turns.len() > 1 {
        vec![starts[0].anchor]
    } else {
        starts.iter().map(|s| s.anchor).collect()
    };
    let mut near = near_starts.as_slice();
    for _ in 0..target * 40 {
        if placed >= target {
            break;
        }
        let head = match near.first() {
            Some(&anchor) => {
                let close: Vec<Tile> = heads
                    .iter()
                    .copied()
                    .filter(|t| chebyshev(anchor, *t) <= NEAR_START)
                    .collect();
                if close.is_empty() {
                    continue;
                }
                close[(next(1) % close.len() as u64) as usize]
            }
            None => heads[(next(1) % heads.len() as u64) as usize],
        };
        if touching(&ore, head) {
            continue;
        }
        // Weights 9..=1 for sizes 1..=9.
        let mut roll = (next(3) % 45) as usize;
        let mut want = 1;
        while roll >= MAX_CLUSTER - (want - 1) {
            roll -= MAX_CLUSTER - (want - 1);
            want += 1;
        }
        let mut cluster = vec![head];
        while cluster.len() < want {
            let mut frontier: Vec<Tile> = vec![];
            for c in &cluster {
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let Some(t) = grid.tile(i32::from(c.x) + dx, i32::from(c.y) + dy) else {
                        continue;
                    };
                    if allowed(t)
                        && !touching(&ore, t)
                        && !cluster.contains(&t)
                        && !frontier.contains(&t)
                    {
                        frontier.push(t);
                    }
                }
            }
            // Keep hugging the wall when any wall-adjacent frontier cell exists.
            if frontier.iter().any(|t| dist[grid.idx(*t)] == 1) {
                frontier.retain(|t| dist[grid.idx(*t)] == 1);
            }
            if frontier.is_empty() {
                break;
            }
            let pick = (next(4) % frontier.len() as u64) as usize;
            cluster.push(frontier[pick]);
        }
        let copies: Vec<Vec<Tile>> = turns
            .iter()
            .map(|q| cluster.iter().map(|t| rotate(*t, size, *q)).collect())
            .collect();
        let separate = copies.iter().enumerate().all(|(i, a)| {
            copies[..i]
                .iter()
                .all(|b| a.iter().all(|x| b.iter().all(|y| chebyshev(*x, *y) > 1)))
        });
        if !separate {
            continue;
        }
        for copy in copies {
            for t in copy {
                ore[grid.idx(t)] = true;
                tiles.push(t);
            }
            placed += 1;
        }
        near = &near[near.len().min(1)..];
    }
    tiles
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
    let tiles = ore_tiles(config, &terrain, &starts);
    if tiles.is_empty() {
        return Err("no room for ore clusters".into());
    }
    let budget = config.ore_matter_per_start * f64::from(config.player_count);
    for t in &tiles {
        ore[idx(*t)] = budget / tiles.len() as f64;
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
