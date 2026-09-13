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

pub struct FieldCache {
    version: u32,
    capacity: usize,
    fields: BTreeMap<(u32, bool), Arc<Vec<u16>>>,
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
        if self.fields.version != self.structure_version {
            self.fields.fields.clear();
            self.fields.version = self.structure_version;
        }
        let key = (self.idx(goal) as u32, neighbors == Neighbors::Eight);
        if let Some(f) = self.fields.fields.get(&key) {
            return f.clone();
        }
        if self.fields.fields.len() >= self.fields.capacity {
            self.fields.fields.clear();
        }
        let field = Arc::new(self.build_field(goal, neighbors));
        self.fields.fields.insert(key, field.clone());
        field
    }

    fn build_field(&self, goal: Tile, neighbors: Neighbors) -> Vec<u16> {
        let mut dist = vec![UNREACHABLE; self.cells()];
        let mut queue = VecDeque::new();
        let mut seed = |t: Tile, dist: &mut Vec<u16>| {
            let i = self.idx(t);
            if dist[i] == UNREACHABLE {
                dist[i] = 0;
                queue.push_back(t);
            }
        };
        if self.traversable(goal) {
            seed(goal, &mut dist);
        } else {
            let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
            for (dx, dy, _) in &DIRS[..count] {
                if let Some(t) = self.offset(goal, *dx, *dy)
                    && self.traversable(t)
                    && (dx.abs() + dy.abs() == 1
                        || self.step_legal(t, -dx, -dy, neighbors).is_some())
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
                if let Some(n) = self.step_legal(t, *dx, *dy, neighbors) {
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
    /// displaceable ally, then fixed neighbor order; occupants stay soft obstacles for the pass.
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
        let mut best: Option<(u16, Occupancy, Tile, Direction)> = None;
        for (dx, dy, dir) in &DIRS[..count] {
            if let Some(n) = self.step_legal(from, *dx, *dy, neighbors) {
                let d = field[self.idx(n)];
                let occupancy = self.occupancy(n, mover);
                if d < here && best.is_none_or(|b| (d, occupancy) < (b.0, b.1)) {
                    best = Some((d, occupancy, n, *dir));
                }
            }
        }
        best.map(|b| (b.2, b.3, b.1))
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
        let target_below = field[self.idx(from)];
        if target_below == 0 || target_below == UNREACHABLE {
            return vec![];
        }
        let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
        let mut parent: BTreeMap<Tile, Tile> = BTreeMap::new();
        let mut queue = VecDeque::from([from]);
        parent.insert(from, from);
        let mut best: Option<(u16, usize, Tile)> = None;
        while let Some(t) = queue.pop_front() {
            let path_len = {
                let mut n = 0;
                let mut c = t;
                while c != from {
                    c = parent[&c];
                    n += 1;
                }
                n
            };
            if t != from {
                let d = field[self.idx(t)];
                if d < target_below {
                    let key = (d, path_len, t);
                    if best.is_none_or(|b| key < (b.0, b.1, b.2)) {
                        best = Some(key);
                    }
                }
            }
            for (dx, dy, _) in &DIRS[..count] {
                let Some(n) = self.step_legal(t, *dx, *dy, neighbors) else {
                    continue;
                };
                if (i32::from(n.x) - i32::from(from.x)).abs() > RADIUS
                    || (i32::from(n.y) - i32::from(from.y)).abs() > RADIUS
                    || self.occ[self.idx(n)] != world::NONE
                    || parent.contains_key(&n)
                {
                    continue;
                }
                parent.insert(n, t);
                queue.push_back(n);
            }
        }
        let Some((_, _, end)) = best else {
            return vec![];
        };
        let mut path = vec![end];
        let mut c = end;
        while parent[&c] != from {
            c = parent[&c];
            path.push(c);
        }
        path.reverse();
        path
    }
}
