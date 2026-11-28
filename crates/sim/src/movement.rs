//! Unit motion and displacement, including the push that clears blocked factory outputs.
use crate::fields::Occupancy;
use crate::tick::{Intent, chebyshev};
use crate::world::{DIRS, NONE, RESERVED, Sim, direction_of};
use crate::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub(crate) struct FactoryOutputSearch {
    parents: Vec<Option<Tile>>,
    // Discovery order is the BFS queue. Keep even dead ends here to reset only visited cells.
    discovered: Vec<(Tile, usize)>,
}

impl FactoryOutputSearch {
    fn reset(&mut self, cells: usize) {
        for (_, cell) in self.discovered.drain(..) {
            self.parents[cell] = None;
        }
        self.parents.resize(cells, None);
    }

    fn visit(&mut self, tile: Tile, cell: usize, from: Tile) {
        self.parents[cell] = Some(from);
        self.discovered.push((tile, cell));
    }
}

struct MoveProposal {
    rank: u64,
    mover: usize,
    to: Tile,
    direction: Direction,
}

impl Sim {
    pub(crate) fn motion(&mut self, intents: &[Intent]) {
        let mut moves = self.propose_moves(intents);
        moves.sort_by_key(|proposal| (proposal.rank, proposal.mover));
        self.resolve_moves(intents, moves);
    }

    fn propose_moves(&mut self, intents: &[Intent]) -> Vec<MoveProposal> {
        let t = self.state.tick;
        let mut moves = Vec::new();
        for (i, intent) in intents.iter().enumerate().take(self.state.entities.len()) {
            let e = &self.state.entities[i];
            let Some(m) = self.def(i).movement.clone() else {
                continue;
            };
            if e.lifecycle != Lifecycle::Complete
                || self.acted[i]
                || intent.hold
                || e.born_at_tick.is_some_and(|b| b >= t)
            {
                continue;
            }
            let from = e.tile;
            if let Some((next, goal)) = intent.retreat {
                let dx = i32::from(next.x) - i32::from(from.x);
                let dy = i32::from(next.y) - i32::from(from.y);
                let rank = self.entity_rank(i);
                let legal = self.step_legal(from, dx, dy, m.neighbors).is_some();
                let mover = &mut self.state.entities[i];
                mover.resolved_destination = Some(goal);
                mover.goal_settled = false;
                mover.local_detour.clear();
                if legal {
                    moves.push(MoveProposal {
                        rank,
                        mover: i,
                        to: next,
                        direction: direction_of(dx, dy),
                    });
                }
                continue;
            }
            let Some((goal, _)) = intent.goal else {
                continue;
            };
            let ready = e.next_move_tick <= t;
            let needs_exact_work = intent.work_seat.is_some()
                || (self.reroute_miners && matches!(e.action, Order::Mine { .. }));
            if self.state.entities[i].resolved_destination != Some(goal) {
                self.state.entities[i].local_detour.clear();
            }
            self.state.entities[i].resolved_destination = Some(goal);
            if let Some(next) = self.state.entities[i].local_detour.first().copied() {
                let (dx, dy) = (
                    i32::from(next.x) - i32::from(from.x),
                    i32::from(next.y) - i32::from(from.y),
                );
                if dx.abs() <= 1
                    && dy.abs() <= 1
                    && (!needs_exact_work || next != from)
                    && self.step_legal(from, dx, dy, m.neighbors).is_some()
                {
                    if ready {
                        moves.push(MoveProposal {
                            rank: self.entity_rank(i),
                            mover: i,
                            to: next,
                            direction: direction_of(dx, dy),
                        });
                    }
                    continue;
                }
                self.state.entities[i].local_detour.clear();
            }
            let (route, adjacent) = intent.route_destination(goal);
            let field = self.movement_field(i, route, m.neighbors, adjacent);
            let here = field[self.idx(from)];
            if here == 0 {
                self.state.entities[i].goal_settled = true;
                continue;
            }
            match self.descend(&field, from, i, m.neighbors) {
                Some((to, _, Occupancy::Settled))
                    if self.settled_same_goal(to, i) && !needs_exact_work =>
                {
                    // Every lower exit holds a settled same-goal ally: try one bounded local
                    // detour around the cluster, otherwise settle behind it. Settlement is
                    // evaluated even on cooldown so arrivals are never churned by displacement.
                    // Settled units re-check on a fixed rotation so cells freed later fill in.
                    if self.state.entities[i].goal_settled
                        && (!(t + i as u32).is_multiple_of(8)
                            || self.occupancy_changed_tick + 8 < t)
                    {
                        continue;
                    }
                    let detour = self.local_detour(&field, from, m.neighbors);
                    if detour.is_empty() {
                        self.state.entities[i].goal_settled = true;
                        continue;
                    }
                    if ready {
                        let (dx, dy) = (
                            i32::from(detour[0].x) - i32::from(from.x),
                            i32::from(detour[0].y) - i32::from(from.y),
                        );
                        moves.push(MoveProposal {
                            rank: self.entity_rank(i),
                            mover: i,
                            to: detour[0],
                            direction: direction_of(dx, dy),
                        });
                    }
                    self.state.entities[i].local_detour = detour;
                }
                Some((to, dir, _)) => {
                    self.state.entities[i].goal_settled = false;
                    if ready {
                        moves.push(MoveProposal {
                            rank: self.entity_rank(i),
                            mover: i,
                            to,
                            direction: dir,
                        });
                    }
                }
                None => {
                    self.state.entities[i].goal_settled = false;
                }
            }
        }
        moves
    }

    fn resolve_moves(&mut self, intents: &[Intent], moves: Vec<MoveProposal>) {
        let t = self.state.tick;
        let mut touched = vec![false; self.state.entities.len()];
        let mut diagonals: BTreeSet<(Tile, Tile)> = BTreeSet::new();
        let crossing = |from: Tile, to: Tile| -> Option<(Tile, Tile)> {
            let (dx, dy) = (
                i32::from(to.x) - i32::from(from.x),
                i32::from(to.y) - i32::from(from.y),
            );
            if dx == 0 || dy == 0 {
                return None;
            }
            let a = Tile { x: to.x, y: from.y };
            let b = Tile { x: from.x, y: to.y };
            Some(if a < b { (a, b) } else { (b, a) })
        };
        for MoveProposal {
            mover: i,
            to,
            direction: dir,
            ..
        } in moves
        {
            if touched[i] {
                continue;
            }
            let from = self.state.entities[i].tile;
            let intent = &intents[i];
            let cooldown = self.def(i).movement.as_ref().unwrap().cooldown;
            let cross = crossing(from, to);
            if cross.is_some_and(|c| diagonals.contains(&c)) {
                self.fail_move(i, to, intent);
                continue;
            }
            let o = self.occ[self.idx(to)];
            if o == RESERVED {
                self.fail_move(i, to, intent);
                continue;
            }
            if o == NONE {
                self.commit_move(i, from, to, dir, cooldown);
                touched[i] = true;
                if let Some(c) = crossing(from, to) {
                    diagonals.insert((from.min(to), from.max(to)));
                    diagonals.insert(c);
                }
                continue;
            }
            let o = o as usize;
            let blocker = &self.state.entities[o];
            let mover = &self.state.entities[i];
            let mining_here = intents[o].hold && matches!(blocker.action, Order::Mine { .. });
            let work_anchor = self
                .avoid_working_units
                .then_some(intents[o].work_anchor)
                .flatten();
            let can_yield = blocker.lifecycle == Lifecycle::Complete
                && self.def(o).movement.is_some()
                && !self.hostile(mover.owner, blocker.owner)
                && !touched[o]
                && blocker.born_at_tick.is_none_or(|b| b < t);
            if !can_yield {
                self.fail_move(i, to, intent);
                continue;
            }
            if self.settled_same_goal(to, i)
                && !(self.reroute_miners && matches!(mover.action, Order::Mine { .. }))
                && intent.work_seat.is_none()
            {
                // Settled same-goal ally: wait behind it instead of perpetual displacement.
                self.state.entities[i].goal_settled = true;
                self.state.entities[i].local_detour.clear();
                continue;
            }
            if work_anchor.is_some()
                && let Some(goal) = mover.resolved_destination
            {
                let neighbors = self.def(i).movement.as_ref().unwrap().neighbors;
                let (route, adjacent) = intent.route_destination(goal);
                let field = self.movement_field(i, route, neighbors, adjacent);
                let detour = self.local_detour(&field, from, neighbors);
                if !detour.is_empty() {
                    self.state.entities[i].local_detour = detour;
                    continue;
                }
            }
            let blocker = &self.state.entities[o];
            if (blocker.goal_settled || mining_here || work_anchor.is_some())
                && !self.transits(i, to, intent)
            {
                // A settled ally or a miner holding its claimed ore tile yields only to traffic
                // passing through, never to a mover that would rest on its cell; otherwise two
                // goals contend for one cell forever.
                self.fail_move(i, to, intent);
                continue;
            }
            let b_neighbors = self.def(o).movement.as_ref().unwrap().neighbors;
            let b_cooldown = self.def(o).movement.as_ref().unwrap().cooldown;
            let count = if b_neighbors == Neighbors::Eight {
                8
            } else {
                4
            };
            let mut order = [0, 1, 2, 3, 4, 5, 6, 7];
            let seed = self.hash(u64::from(t), self.entity_rank(i), self.entity_rank(o));
            for k in (1..count).rev() {
                let j = (seed.rotate_left((k * 7) as u32) % (k as u64 + 1)) as usize;
                order.swap(k, j);
            }
            let construct_anchor = work_anchor.filter(|anchor| *anchor != to);
            let mut placed: Option<(Tile, Direction)> = None;
            for &k in &order[..count] {
                let (dx, dy, d) = DIRS[k];
                if let Some(nt) = self.step_legal(to, dx, dy, b_neighbors)
                    && nt != from
                    && self.occ[self.idx(nt)] == NONE
                    && crossing(to, nt).is_none_or(|c| !diagonals.contains(&c))
                {
                    placed.get_or_insert((nt, d));
                    if construct_anchor.is_none_or(|anchor| chebyshev(nt, anchor) == 1) {
                        placed = Some((nt, d));
                        break;
                    }
                }
            }
            if placed.is_none()
                // Swapping is preferable to moving a miner farther along a narrow lane where
                // the same traffic would immediately need to push it again.
                || (work_anchor == Some(to) && placed.is_some_and(|(_, direction)| direction == dir))
                || construct_anchor.is_some_and(|anchor| {
                    chebyshev(from, anchor) == 1
                        && placed.is_some_and(|(tile, _)| chebyshev(tile, anchor) != 1)
                })
            {
                let (dx, dy) = (
                    i32::from(from.x) - i32::from(to.x),
                    i32::from(from.y) - i32::from(to.y),
                );
                if self.step_legal(to, dx, dy, b_neighbors).is_some() {
                    placed = Some((from, direction_of(dx, dy)));
                }
            }
            let Some((bt, bd)) = placed else {
                self.fail_move(i, to, intent);
                continue;
            };
            // Apply both moves atomically: vacate, then place mover and blocker.
            self.occupancy_changed_tick = t;
            self.set_occ(from, NONE);
            self.set_occ(to, NONE);
            let blocker_id = self.state.entities[o].id.clone();
            let mover_id = self.state.entities[i].id.clone();
            {
                let b = &mut self.state.entities[o];
                b.tile = bt;
                b.last_move_direction = bd;
                b.next_move_tick = b.next_move_tick.max(t + b_cooldown);
                b.goal_settled = false;
            }
            self.set_occ(bt, o as u32);
            {
                let m = &mut self.state.entities[i];
                m.tile = to;
                m.last_move_direction = dir;
                m.next_move_tick = t + cooldown;
                m.failed_move_attempts = 0;
                m.blocked_step = None;
                if m.local_detour.first() == Some(&to) {
                    m.local_detour.remove(0);
                }
            }
            self.set_occ(to, i as u32);
            touched[i] = true;
            touched[o] = true;
            if let Some(c) = crossing(from, to) {
                diagonals.insert(c);
            }
            if let Some(c) = crossing(to, bt) {
                diagonals.insert(c);
            }
            self.progress = true;
            let owner = self.state.entities[i].owner;
            self.note(owner, Activity::Movement);
            self.push_event(PresentationEvent::Displacement {
                mover_id,
                blocker_id: blocker_id.clone(),
                mover_from: from,
                mover_to: to,
                blocker_from: to,
                blocker_to: bt,
                involuntary_entity_id: blocker_id,
            });
        }
    }

    /// After stepping onto `to`, the mover could keep descending into a free or yielding cell.
    fn transits(&mut self, mover: usize, to: Tile, intent: &Intent) -> bool {
        let entity = &self.state.entities[mover];
        let Some(goal) = entity.resolved_destination else {
            return false;
        };
        if self.reroute_miners
            && matches!(entity.action, Order::Mine { .. })
            && let [first, next, ..] = entity.local_detour.as_slice()
            && *first == to
            && self.occupancy(*next, mover) != Occupancy::Hard
        {
            return true;
        }
        let neighbors = self.def(mover).movement.as_ref().unwrap().neighbors;
        let (route, adjacent) = intent.route_destination(goal);
        let field = self.movement_field(mover, route, neighbors, adjacent);
        matches!(
            self.descend(&field, to, mover, neighbors),
            Some((_, _, Occupancy::Free | Occupancy::Yielding))
        )
    }

    /// `to` holds an allied mobile entity settled at the mover's own destination.
    fn settled_same_goal(&self, to: Tile, mover: usize) -> bool {
        let o = self.occ[self.idx(to)];
        if o == NONE || o == RESERVED {
            return false;
        }
        let (blocker, m) = (
            &self.state.entities[o as usize],
            &self.state.entities[mover],
        );
        blocker.goal_settled
            && self.def(o as usize).movement.is_some()
            && !self.hostile(blocker.owner, m.owner)
            && blocker.resolved_destination == m.resolved_destination
    }

    fn commit_move(&mut self, i: usize, from: Tile, to: Tile, dir: Direction, cooldown: Tick) {
        let t = self.state.tick;
        self.occupancy_changed_tick = t;
        self.set_occ(from, NONE);
        self.set_occ(to, i as u32);
        let e = &mut self.state.entities[i];
        e.tile = to;
        e.last_move_direction = dir;
        e.next_move_tick = t + cooldown;
        e.failed_move_attempts = 0;
        e.blocked_step = None;
        if e.local_detour.first() == Some(&to) {
            e.local_detour.remove(0);
        }
        let owner = e.owner;
        self.progress = true;
        self.note(owner, Activity::Movement);
    }

    fn fail_move(&mut self, i: usize, to: Tile, intent: &Intent) {
        let t = self.state.tick;
        let goal = self.state.entities[i].resolved_destination;
        {
            let e = &mut self.state.entities[i];
            e.failed_move_attempts = e.failed_move_attempts.saturating_add(1);
            e.blocked_step = Some(to);
            e.next_move_tick = t + 1;
        }
        if self.state.entities[i].failed_move_attempts >= 3
            && let Some(goal) = goal
        {
            let neighbors = self.def(i).movement.as_ref().unwrap().neighbors;
            let (route, adjacent) = intent.route_destination(goal);
            let field = self.movement_field(i, route, neighbors, adjacent);
            let from = self.state.entities[i].tile;
            let detour = self.local_detour(&field, from, neighbors);
            let e = &mut self.state.entities[i];
            e.local_detour = detour;
            e.failed_move_attempts = 0;
        }
    }

    /// Push a chain of mobile units one legal step each into the nearest available space.
    /// This is involuntary displacement: orders survive, movement cooldown cannot block output.
    pub(crate) fn push_factory_output(&mut self, factory: usize, out: Tile) {
        let root = self.idx(out);
        let occupied = self.occ[root];
        if occupied == NONE || occupied == RESERVED {
            return;
        }
        self.factory_output_search.reset(self.occ.len());
        self.factory_output_search.visit(out, root, out);
        let mut next = 0;
        let mut vacancy = None;
        while let Some(&(from, cell)) = self.factory_output_search.discovered.get(next) {
            next += 1;
            let index = self.occ[cell];
            if index == NONE || index == RESERVED {
                continue;
            }
            let i = index as usize;
            if self.state.entities[i].lifecycle != Lifecycle::Complete {
                continue;
            }
            let Some(movement) = &self.def(i).movement else {
                continue;
            };
            let neighbors = movement.neighbors;
            let count = if neighbors == Neighbors::Eight { 8 } else { 4 };
            for (dx, dy, _) in &DIRS[..count] {
                let Some(to) = self.step_legal(from, *dx, *dy, neighbors) else {
                    continue;
                };
                let cell = self.idx(to);
                if self.factory_output_search.parents[cell].is_some() {
                    continue;
                }
                self.factory_output_search.visit(to, cell, from);
                if self.occ[cell] == NONE {
                    // Do not trade one blocked output for another.
                    if self.state.entities.iter().any(|e| {
                        e.production
                            .as_ref()
                            .is_some_and(|p| p.silo.is_none() && p.output_tile == to)
                    }) {
                        continue;
                    }
                    vacancy = Some(to);
                    break;
                }
            }
            if vacancy.is_some() {
                break;
            }
        }
        let Some(mut to) = vacancy else {
            return;
        };
        while to != out {
            let from = self.factory_output_search.parents[self.idx(to)].unwrap();
            let i = self.occ[self.idx(from)] as usize;
            let cooldown = self.def(i).movement.as_ref().unwrap().cooldown;
            let next_tick = self.state.entities[i]
                .next_move_tick
                .max(self.state.tick + cooldown);
            let blocker_id = self.state.entities[i].id.clone();
            let direction = direction_of(
                i32::from(to.x) - i32::from(from.x),
                i32::from(to.y) - i32::from(from.y),
            );
            self.commit_move(i, from, to, direction, cooldown);
            let e = &mut self.state.entities[i];
            e.next_move_tick = next_tick;
            e.goal_settled = false;
            e.local_detour.clear();
            self.push_event(PresentationEvent::Displacement {
                mover_id: self.state.entities[factory].id.clone(),
                blocker_id: blocker_id.clone(),
                mover_from: self.state.entities[factory].tile,
                mover_to: self.state.entities[factory].tile,
                blocker_from: from,
                blocker_to: to,
                involuntary_entity_id: blocker_id,
            });
            to = from;
        }
    }
}
