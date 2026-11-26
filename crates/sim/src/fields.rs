//! Shared destination BFS fields keyed by goal/neighbor rule; derived, never checkpointed or hashed.
use crate::world::{DIRS, Sim};
use crate::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

pub const UNREACHABLE: u16 = u16::MAX;

/// Ordered preference for a destination cell.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Occupancy {
    Free,
    Yielding,
    Settled,
    Hard,
}
const CAPACITY: usize = 256;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct FieldKey {
    goal: Tile,
    diagonals: bool,
    adjacent: bool,
    attacking_team: Option<u8>,
}

pub struct FieldCache {
    version: u32,
    capacity: usize,
    fields: BTreeMap<FieldKey, Arc<Vec<u16>>>,
}
impl Default for FieldCache {
    fn default() -> Self {
        Self {
            version: 0,
            capacity: CAPACITY,
            fields: BTreeMap::new(),
        }
    }
}

impl Sim {
    /// Distance grid toward `goal`. A non-traversable goal seeds its legal adjacent cells at zero.
    /// Bound the derived field cache; eviction may cost time but never changes decisions.
    pub fn set_field_cache_capacity(&mut self, capacity: usize) {
        self.fields.capacity = capacity.max(1);
        self.fields.fields.clear();
    }

    pub(crate) fn field(&mut self, goal: Tile, neighbors: Neighbors) -> Arc<Vec<u16>> {
        self.goal_field(goal, neighbors, false, None)
    }

    /// Armed attack-movers plan through hostile structures. Actual movement still requires
    /// a free step, so the unit fights the obstruction before advancing through its tile.
    pub(crate) fn movement_field(
        &mut self,
        mover: usize,
        goal: Tile,
        neighbors: Neighbors,
        work: bool,
    ) -> Arc<Vec<u16>> {
        let entity = &self.state.entities[mover];
        let attack = self.breach_enemy_structures
            && matches!(entity.action, Order::AttackMove { .. })
            && self
                .def(mover)
                .weapon
                .as_ref()
                .is_some_and(|weapon| weapon.damage > 0.0);
        let team = attack.then(|| self.team_of[usize::from(entity.owner)]);
        self.goal_field(goal, neighbors, work, team)
    }

    fn goal_field(
        &mut self,
        goal: Tile,
        neighbors: Neighbors,
        adjacent: bool,
        attacking_team: Option<u8>,
    ) -> Arc<Vec<u16>> {
        if self.fields.version != self.structure_version {
            self.fields.fields.clear();
            self.fields.version = self.structure_version;
        }
        let key = FieldKey {
            goal,
            diagonals: neighbors == Neighbors::Eight,
            adjacent,
            attacking_team,
        };
        if let Some(f) = self.fields.fields.get(&key) {
            return f.clone();
        }
        if self.fields.fields.len() >= self.fields.capacity {
            self.fields.fields.clear();
        }
        let field = Arc::new(self.build_field(goal, neighbors, adjacent, attacking_team));
        self.fields.fields.insert(key, field.clone());
        field
    }

    fn build_field(
        &self,
        goal: Tile,
        neighbors: Neighbors,
        adjacent: bool,
        attacking_team: Option<u8>,
    ) -> Vec<u16> {
        let mut dist = vec![UNREACHABLE; self.cells()];
        let mut queue = VecDeque::new();
        let mut seed = |t: Tile, dist: &mut Vec<u16>| {
            let i = self.idx(t);
            if dist[i] == UNREACHABLE {
                dist[i] = 0;
                queue.push_back(t);
            }
        };
        if adjacent {
            for (dx, dy, _) in &DIRS {
                if let Some(t) = self.offset(goal, *dx, *dy)
                    && self.route_traversable(t, attacking_team)
                {
                    seed(t, &mut dist);
                }
            }
        } else if self.route_traversable(goal, attacking_team) {
            seed(goal, &mut dist);
        } else {
            let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
            for (dx, dy, _) in &DIRS[..count] {
                if let Some(t) = self.offset(goal, *dx, *dy)
                    && self.route_traversable(t, attacking_team)
                    && (dx.abs() + dy.abs() == 1
                        || self
                            .route_step(t, -dx, -dy, neighbors, attacking_team)
                            .is_some())
                {
                    seed(t, &mut dist);
                }
            }
        }
        let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
        while let Some(t) = queue.pop_front() {
            let d = dist[self.idx(t)];
            if d == UNREACHABLE - 1 {
                continue;
            }
            for (dx, dy, _) in &DIRS[..count] {
                if let Some(n) = self.route_step(t, *dx, *dy, neighbors, attacking_team) {
                    let i = self.idx(n);
                    if dist[i] == UNREACHABLE {
                        dist[i] = d + 1;
                        queue.push_back(n);
                    }
                }
            }
        }
        dist
    }

    /// Next legal step strictly descending the field. Equal distances prefer a free cell, then a
    /// displaceable ally, then alternating axes for treads, then fixed neighbor order.
    /// Occupants stay soft obstacles for the pass.
    pub(crate) fn descend(
        &self,
        field: &[u16],
        from: Tile,
        mover: usize,
        neighbors: Neighbors,
    ) -> Option<(Tile, Direction, Occupancy)> {
        let here = field[self.idx(from)];
        if here == 0 || here == UNREACHABLE {
            return None;
        }
        let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
        let horizontal = matches!(
            self.state.entities[mover].last_move_direction,
            Direction::E | Direction::W
        );
        let alternate_axes = neighbors == Neighbors::Four && self.alternate_treads;
        let repeats_axis = |dx: i32| alternate_axes && (dx != 0) == horizontal;
        let mut best: Option<(u16, Occupancy, bool, Tile, Direction)> = None;
        for (dx, dy, dir) in &DIRS[..count] {
            if let Some(n) = self.step_legal(from, *dx, *dy, neighbors) {
                let d = field[self.idx(n)];
                let occupancy = self.occupancy(n, mover);
                if d < here
                    && best.is_none_or(|b| (d, occupancy, repeats_axis(*dx)) < (b.0, b.1, b.2))
                {
                    best = Some((d, occupancy, repeats_axis(*dx), n, *dir));
                }
            }
        }
        best.map(|b| (b.3, b.4, b.1))
    }

    /// How `to` blocks `mover` this tick: free, an ally that may yield, or a hard occupant.
    pub(crate) fn occupancy(&self, to: Tile, mover: usize) -> Occupancy {
        let o = self.occ[self.idx(to)];
        if o == world::NONE {
            return Occupancy::Free;
        }
        if o == world::RESERVED {
            return Occupancy::Hard;
        }
        let (b, m) = (
            &self.state.entities[o as usize],
            &self.state.entities[mover],
        );
        let mobile = b.lifecycle == Lifecycle::Complete
            && self.def(o as usize).movement.is_some()
            && !self.hostile(b.owner, m.owner)
            && b.born_at_tick.is_none_or(|t| t < self.state.tick);
        if !mobile {
            Occupancy::Hard
        } else if b.goal_settled {
            Occupancy::Settled
        } else {
            Occupancy::Yielding
        }
    }

    /// Bounded radius-6 BFS treating current occupants as hard obstacles; returns a path to a
    /// cell with lower shared-field distance than `from`, or none.
    pub(crate) fn local_detour(
        &self,
        field: &[u16],
        from: Tile,
        neighbors: Neighbors,
    ) -> Vec<Tile> {
        const RADIUS: i32 = 6;
        const SIDE: usize = (2 * RADIUS + 1) as usize;
        const CELLS: usize = SIDE * SIDE;
        let target_below = field[self.idx(from)];
        if target_below == 0 || target_below == UNREACHABLE {
            return vec![];
        }
        let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
        let index = |tile: Tile| {
            let x = (i32::from(tile.x) - i32::from(from.x) + RADIUS) as usize;
            let y = (i32::from(tile.y) - i32::from(from.y) + RADIUS) as usize;
            y * SIDE + x
        };
        let mut parent = [None; CELLS];
        parent[index(from)] = Some(from);
        // Each cell enters the FIFO once. Record its depth at discovery; the
        // predecessor chain is only needed when reconstructing the chosen path.
        let mut queue = [(from, 0); CELLS];
        let mut head = 0;
        let mut tail = 1;
        let mut best: Option<(u16, usize, Tile)> = None;
        while head < tail {
            let (t, path_len) = queue[head];
            head += 1;
            let key = (field[self.idx(t)], path_len, t);
            if key.0 < target_below && best.is_none_or(|b| key < b) {
                best = Some(key);
            }
            for (dx, dy, _) in &DIRS[..count] {
                let Some(n) = self.step_legal(t, *dx, *dy, neighbors) else {
                    continue;
                };
                if (i32::from(n.x) - i32::from(from.x)).abs() > RADIUS
                    || (i32::from(n.y) - i32::from(from.y)).abs() > RADIUS
                    || self.occ[self.idx(n)] != world::NONE
                {
                    continue;
                }
                let slot = &mut parent[index(n)];
                if slot.is_none() {
                    *slot = Some(t);
                    queue[tail] = (n, path_len + 1);
                    tail += 1;
                }
            }
        }
        let Some((_, path_len, mut tile)) = best else {
            return vec![];
        };
        let mut path = Vec::with_capacity(path_len);
        while tile != from {
            path.push(tile);
            tile = parent[index(tile)].expect("each queued tile has a predecessor");
        }
        path.reverse();
        path
    }
}
