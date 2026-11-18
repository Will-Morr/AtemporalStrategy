//! Local artillery retreat. Derived state and optional review data never affect a checkpoint.
use crate::player_timeline::{Sight, VisionCache};
use crate::world::{DIRS, NONE, RESERVED, Sim};
use crate::{EntityId, Lifecycle, PlayerId, Tile};
use std::collections::{BTreeMap, VecDeque};

pub(crate) mod terrain_corner;

const LOOKAHEAD: i32 = 6;
const FIELD_WIDTH: usize = (2 * LOOKAHEAD + 1) as usize;
const FIELD_CELLS: usize = FIELD_WIDTH * FIELD_WIDTH;
const STEP_COST: i32 = 12;
const DEAD_END_COST: i32 = 176;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtilleryRetreatScore {
    pub tile: Tile,
    pub threat: i32,
    pub corner: i32,
    pub combined: i32,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtilleryRetreat {
    pub from: Tile,
    pub threats: Vec<Tile>,
    pub scores: Vec<ArtilleryRetreatScore>,
    pub next: Option<Tile>,
    pub destination: Option<Tile>,
    pub firing: bool,
    pub move_cooldown: bool,
}

#[derive(Default)]
pub(crate) struct RetreatState {
    pub diagnostics: bool,
    pub last: BTreeMap<EntityId, ArtilleryRetreat>,
    vision: VisionCache,
}

impl Sim {
    /// Static row-major corner costs for the map, with zero for rock cells.
    /// Older rules scored corners from the live board and have no static field.
    pub fn artillery_corner_costs(&self) -> Option<&[i32]> {
        self.terrain_corner_scores.as_deref()
    }

    pub fn set_artillery_retreat_diagnostics(&mut self, enabled: bool) {
        self.retreat.diagnostics = enabled;
        self.retreat.last.clear();
    }

    /// Proposed plans from the last tick, before movement arbitration.
    pub fn last_artillery_retreats(&self) -> &BTreeMap<EntityId, ArtilleryRetreat> {
        &self.retreat.last
    }

    pub(crate) fn artillery_sights(&mut self, players: &[PlayerId]) -> BTreeMap<u8, Sight> {
        if players.is_empty() {
            return BTreeMap::new();
        }
        self.retreat.vision.update_walls(&self.state);
        let types = self
            .content
            .types
            .iter()
            .map(|d| (d.key.as_str(), d))
            .collect();
        let mut sights = BTreeMap::new();
        for &player in players {
            sights
                .entry(self.team_of[usize::from(player)])
                .or_insert_with(|| {
                    self.retreat
                        .vision
                        .sight(&self.state, player, &types, &self.team_of, None)
                });
        }
        sights
    }

    pub(crate) fn could_retreat(&self, i: usize) -> bool {
        let e = &self.state.entities[i];
        let def = self.def(i);
        if def.key != "artillery"
            || def.movement.is_none()
            || e.lifecycle != Lifecycle::Complete
            || e.born_at_tick.is_some_and(|born| born >= self.state.tick)
        {
            return false;
        }
        let Some(w) = &def.weapon else {
            return false;
        };
        let threshold = w.range - self.artillery_retreat_margin;
        let r = threshold.ceil() as i32;
        if r <= 0 {
            return false;
        }
        if self.hostile_prefix.is_empty() {
            return self.state.entities.iter().enumerate().any(|(j, enemy)| {
                self.hostile(e.owner, enemy.owner)
                    && enemy.lifecycle == Lifecycle::Complete
                    && self.def(j).movement.is_some()
                    && Self::dist2(e.tile, enemy.tile) < threshold * threshold
            });
        }
        let (x, y) = (i32::from(e.tile.x), i32::from(e.tile.y));
        self.hostiles_in_box(
            e.owner,
            x.saturating_sub(r).max(0),
            y.saturating_sub(r).max(0),
            x.saturating_add(r).min(self.width() - 1),
            y.saturating_add(r).min(self.height() - 1),
        ) > 0
    }

    pub(crate) fn plan_artillery_retreat(
        &self,
        i: usize,
        sight: &Sight,
        firing: bool,
        move_cooldown: bool,
    ) -> Option<ArtilleryRetreat> {
        let e = &self.state.entities[i];
        let from = e.tile;
        let threshold = self.def(i).weapon.as_ref()?.range - self.artillery_retreat_margin;
        if threshold <= 0.0 {
            return None;
        }
        let limit = threshold * threshold;
        let reach = threshold + f64::from(LOOKAHEAD);
        let map_radius = self.state.terrain.width.max(self.state.terrain.height);
        let radius = if self.retreat.diagnostics {
            map_radius
        } else {
            (reach.ceil() as u16).min(map_radius)
        };
        let mut threats = Vec::new();
        let mut urgent = false;
        for y in from.y.saturating_sub(radius)
            ..=from
                .y
                .saturating_add(radius)
                .min(self.state.terrain.height - 1)
        {
            for x in from.x.saturating_sub(radius)
                ..=from
                    .x
                    .saturating_add(radius)
                    .min(self.state.terrain.width - 1)
            {
                let tile = Tile { x, y };
                let idx = self.occ[self.idx(tile)] as usize;
                if let Some(enemy) = self.state.entities.get(idx)
                    && self.hostile(e.owner, enemy.owner)
                    && enemy.lifecycle == Lifecycle::Complete
                    && self.def(idx).movement.is_some()
                    && sight.sees(tile)
                {
                    let distance = Self::dist2(from, tile);
                    urgent |= distance < limit;
                    if self.retreat.diagnostics || distance < reach * reach {
                        threats.push(tile);
                    }
                }
            }
        }
        if !urgent {
            return None;
        }
        let known_occupant = |tile| {
            let occupant = self.occ[self.idx(tile)];
            if occupant != NONE
                && !sight.sees(tile)
                && (occupant == RESERVED
                    || self
                        .state
                        .entities
                        .get(occupant as usize)
                        .is_some_and(|other| other.owner != e.owner))
            {
                return NONE;
            }
            occupant
        };
        let walkable = |tile| {
            if !self.floor(tile) {
                return false;
            }
            let occupant = known_occupant(tile);
            occupant == NONE
                || (occupant != RESERVED
                    && !self.is_structure(occupant as usize)
                    && !self.hostile(e.owner, self.state.entities[occupant as usize].owner))
        };
        let neighbors = |tile| {
            DIRS[..4]
                .iter()
                .filter_map(move |&(dx, dy, _)| self.offset(tile, dx, dy).filter(|&n| walkable(n)))
        };
        let corner_cost = |tile| {
            if let Some(scores) = &self.terrain_corner_scores {
                return scores[self.idx(tile)];
            }
            let mut exits = 0;
            let mut escape_arms = 0;
            for next in neighbors(tile) {
                exits += 1;
                let (mut previous, mut current) = (tile, next);
                for depth in 0..=LOOKAHEAD {
                    let mut onward = neighbors(current).filter(|&n| n != previous);
                    let Some(next) = onward.next() else {
                        break;
                    };
                    if onward.next().is_some() || depth == LOOKAHEAD {
                        escape_arms += 1;
                        break;
                    }
                    (previous, current) = (current, next);
                }
            }
            // A short side pocket may look safe by distance alone. Penalize its single exit,
            // including along the inward approach, and prefer room to turn in close ties.
            (4 - exits) * 4 + (2_i32 - escape_arms).max(0) * DEAD_END_COST
        };
        let field_index = |tile: Tile| {
            let x = (i32::from(tile.x) - i32::from(from.x) + LOOKAHEAD) as usize;
            let y = (i32::from(tile.y) - i32::from(from.y) + LOOKAHEAD) as usize;
            y * FIELD_WIDTH + x
        };
        let evaluate = |tile, reachable| {
            let threat = threats.iter().fold(0_i32, |cost, &enemy| {
                cost.saturating_add(((limit - Self::dist2(tile, enemy)).max(0.0) * 4.0) as i32)
            });
            let corner = corner_cost(tile);
            ArtilleryRetreatScore {
                tile,
                threat,
                corner,
                combined: threat.saturating_add(corner),
                reachable,
            }
        };
        let mut visited = [false; FIELD_CELLS];
        let mut queue = VecDeque::from([(from, from, 0)]);
        visited[field_index(from)] = true;
        let current_danger = evaluate(from, true).combined;
        let mut best = None;
        while let Some((tile, first, steps)) = queue.pop_front() {
            let score = evaluate(tile, true);
            let occupied = first != from && known_occupant(first) != NONE;
            let cost = score
                .combined
                .saturating_add(steps * STEP_COST + if occupied { STEP_COST } else { 0 });
            if score.combined < current_danger {
                let option = (cost, steps, first, tile);
                best = Some(best.map_or(option, |prior| option.min(prior)));
            }
            if steps == LOOKAHEAD {
                continue;
            }
            for next in neighbors(tile) {
                let index = field_index(next);
                if !visited[index] {
                    visited[index] = true;
                    queue.push_back((next, if steps == 0 { next } else { first }, steps + 1));
                }
            }
        }
        let mut scores = Vec::new();
        if self.retreat.diagnostics {
            for y in 0..self.state.terrain.height {
                for x in 0..self.state.terrain.width {
                    let tile = Tile { x, y };
                    if self.terrain_corner_scores.is_some() && self.floor(tile) || walkable(tile) {
                        let reachable = from.x.abs_diff(x) <= LOOKAHEAD as u16
                            && from.y.abs_diff(y) <= LOOKAHEAD as u16
                            && visited[field_index(tile)];
                        scores.push(evaluate(tile, reachable));
                    }
                }
            }
        }
        Some(ArtilleryRetreat {
            from,
            threats,
            scores,
            next: best.map(|(_, _, first, _)| first),
            destination: best.map(|(_, _, _, goal)| goal),
            firing,
            move_cooldown,
        })
    }
}
