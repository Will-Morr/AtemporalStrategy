//! Runtime world: authoritative `WorldState` plus derived indexes rebuilt each tick.
use crate::fields::FieldCache;
use crate::*;
use std::collections::BTreeMap;

pub const NONE: u32 = u32::MAX;
/// Tile claimed this tick by a first-funded site or a spawn; blocks like a structure.
pub const RESERVED: u32 = u32::MAX - 1;
/// Fixed neighbor order: axis steps first, then diagonals. Ties in field distance follow this order.
pub const DIRS: [(i32, i32, Direction); 8] = [
    (0, -1, Direction::N),
    (1, 0, Direction::E),
    (0, 1, Direction::S),
    (-1, 0, Direction::W),
    (1, -1, Direction::Ne),
    (1, 1, Direction::Se),
    (-1, 1, Direction::Sw),
    (-1, -1, Direction::Nw),
];

pub struct Sim {
    pub config: MatchConfig,
    pub content: Content,
    pub state: WorldState,
    pub(crate) events: Vec<AcceptedTurn>,
    pub(crate) precedence: Vec<RoundPrecedence>,
    pub(crate) ty: Vec<usize>,
    pub(crate) occ: Vec<u32>,
    pub(crate) fields: FieldCache,
    pub(crate) structure_version: u32,
    pub(crate) sequence: u32,
    pub(crate) dictionary: BTreeMap<EntityId, u32>,
    pub(crate) dictionary_len: u32,
    pub(crate) new_dictionary: Vec<EntityRef>,
    pub(crate) command_outcomes: Vec<CommandOutcome>,
    pub(crate) pending_events: Vec<WorldEvent>,
    pub(crate) activity: Vec<[u32; 5]>,
    pub(crate) acted: Vec<bool>,
    pub(crate) pending_sites: Vec<(EntityId, f64)>,
    pub(crate) progress: bool,
    pub(crate) bucket_acc: Vec<[u32; 5]>,
    pub(crate) bucket_from: Tick,
    pub(crate) checkpoint_tick: Tick,
    pub(crate) team_of: Vec<u8>,
    pub(crate) sim_start: std::time::Instant,
}

impl Sim {
    pub fn new(request: &SimRequest) -> Result<Self> {
        let content = normalize_content(request.content.clone())?;
        atemporal_content::validate_config(&request.config)?;
        let state = identity::canonical_world(&request.checkpoint)?;
        let players = usize::from(request.config.player_count);
        if state.players.len() != players {
            return Err("checkpoint player count disagrees with configuration".into());
        }
        let mut events = request.events.clone();
        events.sort_by_key(|t| (t.tick, t.round, t.player));
        let mut precedence = request.precedence.clone();
        precedence.sort_by_key(|p| p.round);
        let mut team_of: Vec<u8> = (0..request.config.player_count).collect();
        if let Multiplayer::Teams { assignments } = &request.config.multiplayer {
            let mut teams: Vec<&str> = assignments.iter().map(|a| a.team_id.as_str()).collect();
            teams.sort();
            teams.dedup();
            for a in assignments {
                team_of[usize::from(a.player_id)] =
                    teams.iter().position(|t| *t == a.team_id).unwrap() as u8;
            }
        }
        let mut dictionary = BTreeMap::new();
        for (index, entry) in request.entity_dictionary.iter().enumerate() {
            dictionary.insert(entry.id.clone(), index as u32);
        }
        let mut sim = Self {
            config: request.config.clone(),
            content,
            state,
            events,
            precedence,
            ty: vec![],
            occ: vec![],
            fields: FieldCache::default(),
            structure_version: 0,
            sequence: 0,
            dictionary_len: request.entity_dictionary.len() as u32,
            dictionary,
            new_dictionary: vec![],
            command_outcomes: vec![],
            pending_events: vec![],
            activity: vec![[0; 5]; players],
            acted: vec![],
            pending_sites: vec![],
            progress: false,
            bucket_acc: vec![[0; 5]; players],
            bucket_from: request.checkpoint.tick,
            checkpoint_tick: request.checkpoint.tick,
            team_of,
            sim_start: std::time::Instant::now(),
        };
        sim.reindex()?;
        for i in 0..sim.state.entities.len() {
            sim.intern(i);
        }
        Ok(sim)
    }

    /// Sort entities by identity and rebuild type/occupancy indexes. Call after births/deaths.
    pub(crate) fn reindex(&mut self) -> Result<()> {
        self.state.entities.sort_by(|a, b| a.id.cmp(&b.id));
        let cells = self.cells();
        self.occ.clear();
        self.occ.resize(cells, NONE);
        self.ty.clear();
        for (i, e) in self.state.entities.iter().enumerate() {
            let ty = self.type_index(&e.type_key)?;
            self.ty.push(ty);
            let at = self.idx(e.tile);
            if self.occ[at] != NONE {
                return Err(format!("duplicate occupancy at {:?}", e.tile));
            }
            self.occ[at] = i as u32;
        }
        self.acted = vec![false; self.state.entities.len()];
        for (id, _) in &self.pending_sites {
            if let Some(b) = self.state.blueprints.iter().find(|b| b.id == *id) {
                let at = self.idx(b.tile);
                self.occ[at] = RESERVED;
            }
        }
        Ok(())
    }

    pub(crate) fn intern(&mut self, i: usize) -> u32 {
        let e = &self.state.entities[i];
        if let Some(index) = self.dictionary.get(&e.id) {
            return *index;
        }
        let index = self.dictionary_len;
        self.dictionary_len += 1;
        self.dictionary.insert(e.id.clone(), index);
        self.new_dictionary.push(EntityRef {
            id: e.id.clone(),
            owner: e.owner,
            type_key: e.type_key.clone(),
        });
        index
    }

    pub fn type_index(&self, key: &str) -> Result<usize> {
        self.content
            .types
            .binary_search_by(|t| t.key.as_str().cmp(key))
            .map_err(|_| format!("unknown type {key}"))
    }
    pub(crate) fn def(&self, i: usize) -> &TypeDefinition {
        &self.content.types[self.ty[i]]
    }
    pub(crate) fn width(&self) -> i32 {
        i32::from(self.state.terrain.width)
    }
    pub(crate) fn height(&self) -> i32 {
        i32::from(self.state.terrain.height)
    }
    pub(crate) fn cells(&self) -> usize {
        usize::from(self.state.terrain.width) * usize::from(self.state.terrain.height)
    }
    pub(crate) fn idx(&self, t: Tile) -> usize {
        usize::from(t.y) * usize::from(self.state.terrain.width) + usize::from(t.x)
    }
    pub(crate) fn set_occ(&mut self, t: Tile, value: u32) {
        let at = self.idx(t);
        self.occ[at] = value;
    }
    pub(crate) fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width() && y < self.height()
    }
    pub(crate) fn floor(&self, t: Tile) -> bool {
        self.state.terrain.cells[self.idx(t)] == TerrainCell::Floor
    }
    pub(crate) fn is_structure(&self, i: usize) -> bool {
        self.def(i).kind == TypeKind::Structure
    }
    /// Terrain walkable and not held by a structure or site; dynamic units are ignored.
    pub(crate) fn traversable(&self, t: Tile) -> bool {
        if !self.floor(t) {
            return false;
        }
        match self.occ[self.idx(t)] {
            NONE => true,
            RESERVED => false,
            o => !self.is_structure(o as usize),
        }
    }
    pub(crate) fn offset(&self, t: Tile, dx: i32, dy: i32) -> Option<Tile> {
        let (x, y) = (i32::from(t.x) + dx, i32::from(t.y) + dy);
        self.in_bounds(x, y).then_some(Tile {
            x: x as u16,
            y: y as u16,
        })
    }
    /// Legal step under a neighbor rule: no wall/structure destination and no diagonal corner cutting.
    pub(crate) fn step_legal(
        &self,
        from: Tile,
        dx: i32,
        dy: i32,
        neighbors: Neighbors,
    ) -> Option<Tile> {
        if dx != 0 && dy != 0 {
            if neighbors == Neighbors::Four {
                return None;
            }
            let a = self.offset(from, dx, 0)?;
            let b = self.offset(from, 0, dy)?;
            if !self.traversable(a) || !self.traversable(b) {
                return None;
            }
        }
        let to = self.offset(from, dx, dy)?;
        self.traversable(to).then_some(to)
    }
    pub(crate) fn dist2(a: Tile, b: Tile) -> f64 {
        let dx = f64::from(a.x) - f64::from(b.x);
        let dy = f64::from(a.y) - f64::from(b.y);
        dx * dx + dy * dy
    }
    pub(crate) fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        self.team_of[usize::from(a)] != self.team_of[usize::from(b)]
    }
    /// Symmetric integer line traversal; only rock blocks direct fire.
    pub(crate) fn line_of_sight(&self, a: Tile, b: Tile) -> bool {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let (mut x, mut y) = (i32::from(a.x), i32::from(a.y));
        let (x1, y1) = (i32::from(b.x), i32::from(b.y));
        let dx = (x1 - x).abs();
        let dy = -(y1 - y).abs();
        let sx = if x < x1 { 1 } else { -1 };
        let sy = if y < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if (x, y) == (x1, y1) {
                return true;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
            if (x, y) != (x1, y1)
                && self.state.terrain.cells[(y * self.width() + x) as usize] == TerrainCell::Wall
            {
                return false;
            }
        }
    }
    pub(crate) fn find(&self, id: &EntityId) -> Option<usize> {
        self.state.entities.binary_search_by(|e| e.id.cmp(id)).ok()
    }
    /// Deterministic counter hash; there is no shared RNG stream to checkpoint.
    pub(crate) fn hash(&self, salt: u64, a: u64, b: u64) -> u64 {
        let mut h = self.config.seed.get() ^ 0x9E37_79B9_7F4A_7C15;
        for v in [salt, a, b] {
            h ^= v.wrapping_add(0x9E37_79B9_7F4A_7C15);
            h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h ^= h >> 31;
        }
        h
    }
    pub(crate) fn entity_rank(&self, i: usize) -> u64 {
        let e = &self.state.entities[i];
        let id = &e.id;
        let a = (u64::from(id.birth_command.command.round) << 32)
            | (u64::from(id.birth_command.command.player) << 24)
            | u64::from(id.birth_command.command.index & 0xFF_FFFF);
        let b = (u64::from(id.birth_command.target_index) << 48)
            | (u64::from(id.item_index) << 32)
            | u64::from(id.occurrence);
        self.hash(u64::from(self.state.tick), a, b)
    }
    pub(crate) fn push_event(&mut self, event: PresentationEvent) {
        self.pending_events.push(WorldEvent {
            tick: self.state.tick,
            sequence: self.sequence,
            event,
        });
        self.sequence += 1;
    }
    pub(crate) fn note(&mut self, player: PlayerId, activity: Activity) {
        self.activity[usize::from(player)][activity as usize] += 1;
    }
    pub fn hash_state(&self) -> Result<String> {
        identity::world_hash(&self.state)
    }
}

pub(crate) fn direction_of(dx: i32, dy: i32) -> Direction {
    DIRS.iter()
        .find(|(x, y, _)| *x == dx.signum() && *y == dy.signum())
        .map(|d| d.2)
        .unwrap_or(Direction::N)
}
pub(crate) fn cardinal_offset(d: CardinalDirection) -> (i32, i32) {
    match d {
        CardinalDirection::N => (0, -1),
        CardinalDirection::E => (1, 0),
        CardinalDirection::S => (0, 1),
        CardinalDirection::W => (-1, 0),
    }
}
