//! One tick: commands → action intents → economy → damage/deaths/completions → motion → births →
//! survival/inactivity. Every loop runs in sorted entity-ID order or an explicit deterministic rank.
use crate::fields::{Occupancy, UNREACHABLE};
use crate::output::Output;
use crate::world::{DIRS, NONE, RESERVED, Sim, cardinal_offset, direction_of};
use crate::*;
use std::collections::BTreeSet;

const EPS: f64 = 1e-9;

#[derive(Clone, Default)]
struct Intent {
    attack: Option<usize>,
    heal: Option<usize>,
    mine: bool,
    construct: Option<usize>,
    goal: Option<(Tile, bool)>,
    work_goal: bool,
    hold: bool,
}

/// Results of the serial field-touching prepass, consumed by the read-only intent pass.
#[derive(Clone, Default)]
struct Prep {
    ore: Option<Tile>,
    blueprint: Option<usize>,
}

#[derive(Clone, Copy)]
enum ConsumerKind {
    Site(usize),
    Blueprint(usize),
    Factory(usize),
    SelfRepair(usize),
    Heal { healer: usize, ally: usize },
}
struct Consumer {
    kind: ConsumerKind,
    priority: Priority,
    demand: f64,
    workers: Vec<usize>,
}

impl Sim {
    pub fn step(&mut self, emit: &mut dyn FnMut(Output)) -> Result<()> {
        let t = self.state.tick;
        self.progress = false;
        for a in &mut self.activity {
            *a = [0; 5];
        }
        self.apply_commands()?;
        self.reindex()?;
        self.index_hostiles();
        let mut intents = self.intents()?;
        self.mining(&intents);
        let consumers = self.consumers(&intents);
        let (growth, healing) = self.allocate(consumers)?;
        self.stock_missiles()?;
        self.launch_missiles()?;
        let mut damage = self.combat(&intents);
        let annihilated = self.missile_impacts(&mut damage)?;
        self.resolve(&growth, &healing, &damage, &annihilated, &mut intents)?;
        self.motion(&intents)?;
        self.births()?;
        self.create_sites()?;
        self.survival();
        if self.progress {
            self.state.last_progress_tick = t;
            self.state.inactivity_deadline = t + 1 + self.config.stall_ticks;
        }
        self.state.tick = t + 1;
        self.emit_state_outputs(emit, false);
        Ok(())
    }

    fn ready_action(&self, i: usize) -> bool {
        let e = &self.state.entities[i];
        e.next_action_tick <= self.state.tick && e.born_at_tick.is_none_or(|b| b < self.state.tick)
    }

    /// Nearest hostile living entity within `radius`, by squared distance then identity. Line of
    /// sight is traced in that order and only until the first visible candidate.
    fn nearest_hostile(&self, i: usize, radius: f64, need_los: bool) -> Option<usize> {
        self.nearest_hostile_matching(i, radius, need_los, false)
    }
    fn nearest_hostile_matching(
        &self,
        i: usize,
        radius: f64,
        need_los: bool,
        incoming_only: bool,
    ) -> Option<usize> {
        let e = &self.state.entities[i];
        let r = radius.floor() as i32;
        let (cx, cy) = (i32::from(e.tile.x), i32::from(e.tile.y));
        let (x0, x1) = ((cx - r).max(0), (cx + r).min(self.width() - 1));
        let (y0, y1) = ((cy - r).max(0), (cy + r).min(self.height() - 1));
        if !self.hostile_prefix.is_empty() && self.hostiles_in_box(e.owner, x0, y0, x1, y1) == 0 {
            return None;
        }
        let limit = radius * radius + EPS;
        let mut candidates: Vec<(f64, &EntityId, usize)> = vec![];
        for y in y0..=y1 {
            let row = y as usize * self.width() as usize;
            for x in x0..=x1 {
                let o = self.occ[row + x as usize];
                if o == NONE || o == RESERVED {
                    continue;
                }
                let o = o as usize;
                let other = &self.state.entities[o];
                if !self.hostile(e.owner, other.owner)
                    || (incoming_only && other.engaged_target.as_ref() != Some(&e.id))
                {
                    continue;
                }
                let d = Self::dist2(e.tile, other.tile);
                if d <= limit {
                    candidates.push((d, &other.id, o));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(b.1)));
        candidates
            .into_iter()
            .find(|(_, _, o)| !need_los || self.line_of_sight(e.tile, self.state.entities[*o].tile))
            .map(|c| c.2)
    }

    fn target_valid(&self, i: usize, j: usize, radius: f64, need_los: bool) -> bool {
        let (a, b) = (&self.state.entities[i], &self.state.entities[j]);
        self.hostile(a.owner, b.owner)
            && Self::dist2(a.tile, b.tile) <= radius * radius + EPS
            && (!need_los || self.line_of_sight(a.tile, b.tile))
    }

    /// Fewer armed entities than this run intents serially: per-tick scoped spawns cost more
    /// than the target scans they split below roughly this population (measured, see docs).
    const PARALLEL_THRESHOLD: usize = 1000;

    /// Serial prepass that touches the shared field cache, then read-only intents computed
    /// in ordered slots by a bounded scoped pool (or serially below the threshold).
    fn intents(&mut self) -> Result<Vec<Intent>> {
        let n = self.state.entities.len();
        self.index_claims();
        let mut prep: Vec<Prep> = Vec::with_capacity(n);
        for i in 0..n {
            let e = &self.state.entities[i];
            let mut p = Prep::default();
            if e.lifecycle == Lifecycle::Site {
                prep.push(p);
                continue;
            }
            let def = self.def(i);
            match &e.action {
                Order::Mine { area } if def.mining.is_some() => {
                    let area = area.clone();
                    p.ore = self.ore_target(i, &area);
                    if let Some(old) = self.state.entities[i].resolved_destination {
                        let at = self.idx(old);
                        if Some(old) != p.ore && self.claims[at] == i as u32 {
                            self.claims[at] = NONE;
                        }
                    }
                    if p.ore.is_none() {
                        self.state.entities[i].resolved_destination = None;
                    }
                    if let Some(target) = p.ore {
                        let at = self.idx(target);
                        self.claims[at] = i as u32;
                    }
                }
                Order::Construct { area } if def.construction.is_some() => {
                    let area = area.clone();
                    p.blueprint = self.choose_blueprint(i, &area);
                }
                _ => {}
            }
            prep.push(p);
        }
        let threads = usize::from(self.config.simulation_threads).max(1);
        // Target scanning dominates intent cost, so the pool is gated on armed entities.
        let armed = (0..n).filter(|i| self.def(*i).weapon.is_some()).count();
        let intents: Vec<(Intent, Option<EntityId>)> =
            if threads == 1 || armed < Self::PARALLEL_THRESHOLD {
                (0..n).map(|i| self.intent(i, &prep[i])).collect()
            } else {
                let chunk = n.div_ceil(threads);
                let sim: &Self = self;
                std::thread::scope(|scope| {
                    let workers: Vec<_> = prep
                        .chunks(chunk)
                        .enumerate()
                        .map(|(k, slice)| {
                            scope.spawn(move || {
                                slice
                                    .iter()
                                    .enumerate()
                                    .map(|(j, p)| sim.intent(k * chunk + j, p))
                                    .collect::<Vec<_>>()
                            })
                        })
                        .collect();
                    workers
                        .into_iter()
                        .flat_map(|w| w.join().expect("intent worker panicked"))
                        .collect()
                })
            };
        let mut out = Vec::with_capacity(n);
        for (i, (intent, engaged)) in intents.into_iter().enumerate() {
            let e = &mut self.state.entities[i];
            if e.lifecycle == Lifecycle::Complete {
                e.engaged_target = engaged;
            }
            out.push(intent);
        }
        Ok(out)
    }

    /// Read-only intent for one entity from the pre-tick snapshot and its prepass result.
    fn intent(&self, i: usize, prep: &Prep) -> (Intent, Option<EntityId>) {
        let e = &self.state.entities[i];
        let mut intent = Intent::default();
        let mut engaged = None;
        if e.lifecycle == Lifecycle::Site {
            return (intent, engaged);
        }
        let def = self.def(i);
        let ready = self.ready_action(i);
        // A healing-capable supporter prioritizes legal healing of its ally over firing.
        if let (Order::Support { target }, Some(h)) = (&e.action, &def.healing)
            && let Some(ally) = self.find(target)
            && self.state.entities[ally].lifecycle == Lifecycle::Complete
            && self.state.entities[ally].hp < self.def(ally).max_hp - EPS
            && Self::dist2(e.tile, self.state.entities[ally].tile) <= h.range * h.range + EPS
        {
            intent.hold = true;
            if ready {
                intent.heal = Some(ally);
            }
        }
        if let Some(w) = &def.weapon
            && !intent.hold
        {
            let direct = !w.indirect;
            let range = w.range;
            let zone_defend = range.min(def.vision);
            let mut candidate: Option<usize> = None;
            let mut zone = 0.0;
            match &e.action {
                Order::Idle {} => zone = zone_defend,
                Order::AttackMove { .. } => {
                    // Attack-move remains vigilant after arrival: visible longer-ranged
                    // enemies must not be able to fire on a parked unit indefinitely.
                    zone = if def.movement.is_some() {
                        def.vision
                    } else {
                        zone_defend
                    };
                }
                Order::Support { target } => {
                    zone = zone_defend;
                    if let Some(ally) = self.find(target)
                        && let Some(enemy) = self.state.entities[ally].engaged_target.as_ref()
                        && let Some(j) = self.find(enemy)
                        && self.target_valid(i, j, def.vision, direct)
                    {
                        candidate = Some(j);
                    }
                }
                Order::Mine { .. } | Order::Construct { .. } => {}
            }
            if candidate.is_none() && zone > 0.0 {
                let retained = e
                    .engaged_target
                    .as_ref()
                    .and_then(|id| self.find(id))
                    .filter(|j| self.target_valid(i, *j, zone, direct));
                candidate = retained.or_else(|| self.nearest_hostile(i, zone, direct));
            }
            if candidate.is_none()
                && matches!(e.action, Order::AttackMove { .. })
                && def.movement.is_some()
            {
                // With no visible target, prior-snapshot attackers reveal a threat beyond vision.
                // Do not abandon an available target to chase a farther attacker.
                let threat_range = self
                    .content
                    .types
                    .iter()
                    .filter_map(|d| d.weapon.as_ref().map(|w| w.range))
                    .fold(def.vision, f64::max);
                candidate = self
                    .nearest_hostile_matching(i, threat_range, direct, true)
                    .or(candidate);
            }
            if let Some(j) = candidate {
                engaged = Some(self.state.entities[j].id.clone());
                let in_range =
                    Self::dist2(e.tile, self.state.entities[j].tile) <= range * range + EPS;
                if in_range {
                    intent.hold = true;
                    if ready && w.damage > 0.0 {
                        intent.attack = Some(j);
                    }
                } else if def.movement.is_some() {
                    intent.goal = Some((self.state.entities[j].tile, false));
                }
            }
        }
        if !intent.hold && intent.goal.is_none() {
            match &e.action {
                Order::Mine { .. } if def.mining.is_some() => {
                    if self.mines_here(i) {
                        intent.hold = true;
                        intent.mine = ready;
                    } else if let Some(goal) = prep.ore {
                        intent.goal = Some((goal, false));
                    }
                }
                Order::Construct { .. } if def.construction.is_some() => {
                    if let Some(b) = prep.blueprint {
                        let target = self.state.blueprints[b].tile;
                        if chebyshev(e.tile, target) == 1 {
                            intent.hold = true;
                            if ready {
                                intent.construct = Some(b);
                            }
                        } else {
                            intent.goal = Some((target, true));
                            intent.work_goal = true;
                        }
                    }
                }
                Order::AttackMove { destination } if def.movement.is_some() => {
                    intent.goal = Some((*destination, false));
                }
                Order::Support { target } if def.movement.is_some() => {
                    if let Some(j) = self.find(target) {
                        let tile = self.state.entities[j].tile;
                        if chebyshev(e.tile, tile) > 1 {
                            intent.goal = Some((tile, true));
                        }
                    }
                }
                _ => {}
            }
        }
        (intent, engaged)
    }

    /// Each mining unit claims the ore tile it is heading for, or the one it stands on; the
    /// lowest entity index wins a contested tile, so later miners target elsewhere.
    fn index_claims(&mut self) {
        self.claims.clear();
        self.claims.resize(self.cells(), NONE);
        for i in 0..self.state.entities.len() {
            let e = &self.state.entities[i];
            let Order::Mine { area } = &e.action else {
                continue;
            };
            if e.lifecycle != Lifecycle::Complete || self.def(i).mining.is_none() {
                continue;
            }
            let ore_in = |t: Tile| in_rect(t, area) && self.state.ore[self.idx(t)] > 0.0;
            let target = e
                .resolved_destination
                .filter(|d| ore_in(*d))
                .or_else(|| ore_in(e.tile).then_some(e.tile));
            if let Some(t) = target {
                let at = self.idx(t);
                if self.claims[at] == NONE {
                    self.claims[at] = i as u32;
                }
            }
        }
    }

    fn claimed_by_other(&self, t: Tile, i: usize) -> bool {
        let c = self.claims[self.idx(t)];
        c != NONE && c != i as u32
    }

    /// Standing on unclaimed ore inside the mine area.
    fn mines_here(&self, i: usize) -> bool {
        let e = &self.state.entities[i];
        let Order::Mine { area } = &e.action else {
            return false;
        };
        self.def(i).mining.is_some()
            && in_rect(e.tile, area)
            && self.state.ore[self.idx(e.tile)] > 0.0
            && !self.claimed_by_other(e.tile, i)
    }

    /// Keep a reachable claimed destination; new targets minimize actual walking steps.
    fn ore_target(&mut self, i: usize, area: &Rect) -> Option<Tile> {
        let from = self.state.entities[i].tile;
        let neighbors = self
            .def(i)
            .movement
            .as_ref()
            .map_or(Neighbors::Eight, |m| m.neighbors);
        let valid = |sim: &Self, t: Tile| {
            in_rect(t, area)
                && sim.state.ore[sim.idx(t)] > 0.0
                && sim.traversable(t)
                && !sim.claimed_by_other(t, i)
        };
        if valid(self, from) {
            return Some(from);
        }
        if let Some(t) = self.state.entities[i].resolved_destination
            && valid(self, t)
            && self.field(t, neighbors)[self.idx(from)] != UNREACHABLE
        {
            return Some(t);
        }
        // A cached distance grid from the source is symmetric with destination fields. This
        // search happens only when acquiring/replacing a target, not on every walking tick.
        let distance = self.field(from, neighbors);
        let mut best: Option<(u16, Tile)> = None;
        for y in area.min.y..=area.max.y.min(self.state.terrain.height - 1) {
            for x in area.min.x..=area.max.x.min(self.state.terrain.width - 1) {
                let t = Tile { x, y };
                let d = distance[self.idx(t)];
                if d != UNREACHABLE && valid(self, t) && best.is_none_or(|old| (d, t) < old) {
                    best = Some((d, t));
                }
            }
        }
        best.map(|(_, t)| t)
    }

    /// Nearest reachable working position; equal paths use creation precedence then identity.
    fn choose_blueprint(&mut self, i: usize, area: &Rect) -> Option<usize> {
        let e = &self.state.entities[i];
        let (owner, from) = (e.owner, e.tile);
        let neighbors = self
            .def(i)
            .movement
            .as_ref()
            .map_or(Neighbors::Eight, |m| m.neighbors);
        let candidates: Vec<_> = self
            .state
            .blueprints
            .iter()
            .enumerate()
            .filter(|(_, b)| b.owner == owner && in_rect(b.tile, area))
            .map(|(i, b)| {
                (
                    i,
                    b.tile,
                    b.site_id.is_some(),
                    b.precedence.clone(),
                    b.id.clone(),
                )
            })
            .collect();
        // One source field ranks every candidate; only the chosen target needs a work field
        // for motion. Large blueprint sets therefore cannot churn one BFS per site per worker.
        let distances = self.field(from, neighbors);
        let mut best = None;
        for (b, tile, started, precedence, id) in candidates {
            if !started && self.occ[self.idx(tile)] != NONE {
                continue;
            }
            let distance = DIRS
                .iter()
                .filter_map(|(dx, dy, _)| self.offset(tile, *dx, *dy))
                .filter(|t| self.traversable(*t))
                .map(|t| distances[self.idx(t)])
                .min()
                .unwrap_or(UNREACHABLE);
            if distance == UNREACHABLE {
                continue;
            }
            let candidate = (distance, precedence, id, b);
            if best.as_ref().is_none_or(|old| &candidate < old) {
                best = Some(candidate);
            }
        }
        best.map(|(_, _, _, b)| b)
    }

    fn mining(&mut self, intents: &[Intent]) {
        let t = self.state.tick;
        for (i, intent) in intents.iter().enumerate() {
            if !intent.mine {
                continue;
            }
            let m = self.def(i).mining.clone().unwrap();
            let e = &self.state.entities[i];
            let here = self.idx(e.tile);
            let amount = m.rate.min(self.state.ore[here]);
            if amount <= 0.0 {
                continue;
            }
            let owner = usize::from(e.owner);
            self.state.ore[here] -= amount;
            self.state.players[owner].bank += amount;
            self.state.players[owner].counters.mined += amount;
            self.state.entities[i].next_action_tick = t + m.cooldown;
            self.acted[i] = true;
            self.progress = true;
            self.note(owner as u8, Activity::Mining);
        }
    }

    fn consumers(&mut self, intents: &[Intent]) -> Vec<Consumer> {
        let mut consumers: Vec<Consumer> = vec![];
        // Construction: one consumer per blueprint/site with summed legal worker rates.
        let mut by_blueprint: Vec<(usize, Vec<usize>)> = vec![];
        for (i, intent) in intents.iter().enumerate() {
            if let Some(b) = intent.construct
                && self.state.entities[i].priority != Priority::Off
            {
                match by_blueprint.iter_mut().find(|(bp, _)| *bp == b) {
                    Some((_, workers)) => workers.push(i),
                    None => by_blueprint.push((b, vec![i])),
                }
            }
        }
        by_blueprint.sort_by_key(|(b, _)| *b);
        for (b, workers) in by_blueprint {
            let bp = &self.state.blueprints[b];
            let ty = self.type_index(&bp.type_key).unwrap();
            let cost = self.content.types[ty].matter_cost;
            let site = bp.site_id.as_ref().and_then(|id| self.find(id));
            let paid = site.map_or(0.0, |s| self.state.entities[s].paid_matter);
            let mut rate: f64 = workers
                .iter()
                .map(|w| self.def(*w).construction.as_ref().unwrap().rate)
                .sum();
            rate = rate.min(cost - paid).max(0.0);
            if site.is_none() {
                // Only the oldest competing unstarted blueprint on a tile may request funding.
                let first = self
                    .state
                    .blueprints
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.tile == bp.tile && o.site_id.is_none())
                    .min_by(|(_, a), (_, b)| (&a.precedence, &a.id).cmp(&(&b.precedence, &b.id)))
                    .map(|(idx, _)| idx);
                if first != Some(b) || self.occ[self.idx(bp.tile)] != NONE {
                    continue;
                }
            }
            consumers.push(Consumer {
                kind: site.map_or(ConsumerKind::Blueprint(b), ConsumerKind::Site),
                priority: site.map_or(bp.priority, |s| self.state.entities[s].priority),
                demand: rate,
                workers,
            });
        }
        for (i, intent) in intents.iter().enumerate() {
            let Some(ally) = intent.heal else {
                continue;
            };
            let h = self.def(i).healing.clone().unwrap();
            let missing = self.def(ally).max_hp - self.state.entities[ally].hp;
            consumers.push(Consumer {
                kind: ConsumerKind::Heal { healer: i, ally },
                priority: self.state.entities[i].priority,
                demand: h.demand.min(missing / h.hp_per_matter).max(0.0),
                workers: vec![],
            });
        }
        // Automatic repair is independent of weapons and order cooldowns.
        for (i, e) in self.state.entities.iter().enumerate() {
            if e.lifecycle != Lifecycle::Complete
                || e.born_at_tick.is_some_and(|t| t >= self.state.tick)
            {
                continue;
            }
            if let Some(h) = &self.def(i).self_repair {
                consumers.push(Consumer {
                    kind: ConsumerKind::SelfRepair(i),
                    priority: e.priority,
                    demand: h
                        .rate
                        .min((self.def(i).max_hp - e.hp) / h.hp_per_matter)
                        .max(0.0),
                    workers: vec![],
                });
            }
        }
        // Production: activate the next pending item, then demand at most the production rate.
        for i in 0..self.state.entities.len() {
            if self.state.entities[i].lifecycle != Lifecycle::Complete {
                continue;
            }
            let Some(p) = self.def(i).production.clone() else {
                continue;
            };
            let priority = self.state.entities[i].priority;
            let cost_of = |key: &str| self.content.types[self.type_index(key).unwrap()].matter_cost;
            let costs: Vec<(TypeKey, f64)> =
                p.recipes.iter().map(|k| (k.clone(), cost_of(k))).collect();
            let Some(prod) = self.state.entities[i].production.as_mut() else {
                continue;
            };
            if prod.active_item.is_none() && !prod.pending_items.is_empty() {
                let item = prod.pending_items.remove(0);
                let occurrence = prod
                    .occurrence_counters
                    .iter()
                    .find(|c| c.item_id == item.item_id)
                    .map_or(0, |c| c.next_occurrence);
                prod.active_item = Some(ActiveItem {
                    item_id: item.item_id,
                    occurrence,
                    type_key: item.type_key,
                    paid_matter: 0.0,
                    awaiting_output: false,
                    loop_enabled: item.loop_enabled,
                });
            }
            let Some(active) = prod.active_item.as_ref() else {
                continue;
            };
            if active.awaiting_output {
                continue;
            }
            let cost = costs
                .iter()
                .find(|(k, _)| *k == active.type_key)
                .map_or(0.0, |c| c.1);
            let demand = p.rate.min(cost - active.paid_matter).max(0.0);
            consumers.push(Consumer {
                kind: ConsumerKind::Factory(i),
                priority,
                demand,
                workers: vec![],
            });
        }
        consumers
    }

    /// Equal-share capped water filling per player and tier. Returns per-entity HP growth and healing.
    fn allocate(&mut self, consumers: Vec<Consumer>) -> Result<(Vec<f64>, Vec<f64>)> {
        let t = self.state.tick;
        let mut growth = vec![0.0; self.state.entities.len()];
        let mut healing = vec![0.0; self.state.entities.len()];
        let mut sites_to_create: Vec<(usize, f64)> = vec![];
        for player in 0..self.state.players.len() {
            let mut bank = self.state.players[player].bank;
            for tier in [Priority::High, Priority::Medium, Priority::Low] {
                let mut tier_items: Vec<(f64, usize)> = consumers
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        c.priority == tier
                            && c.demand > 0.0
                            && self.consumer_owner(c) == player as u8
                    })
                    .map(|(idx, c)| (c.demand, idx))
                    .collect();
                if tier_items.is_empty() {
                    continue;
                }
                tier_items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
                let mut remaining = bank;
                let mut left = tier_items.len();
                for (demand, idx) in tier_items {
                    let share = remaining / left as f64;
                    let give = demand.min(share).max(0.0);
                    remaining -= give;
                    left -= 1;
                    if give <= 0.0 {
                        continue;
                    }
                    bank -= give;
                    let c = &consumers[idx];
                    match c.kind {
                        ConsumerKind::Site(s) => {
                            let cost = self.def(s).matter_cost;
                            let max_hp = self.def(s).max_hp;
                            self.state.entities[s].paid_matter += give;
                            growth[s] += give / cost * max_hp;
                            self.state.players[player].counters.total_spend += give;
                            self.state.players[player].counters.structure_spend += give;
                            for w in &c.workers {
                                let cd = self.def(*w).construction.as_ref().unwrap().cooldown;
                                self.state.entities[*w].next_action_tick = t + cd;
                                self.acted[*w] = true;
                            }
                            self.note(player as u8, Activity::Construction);
                        }
                        ConsumerKind::Blueprint(b) => {
                            sites_to_create.push((b, give));
                            self.state.players[player].counters.total_spend += give;
                            self.state.players[player].counters.structure_spend += give;
                            for w in &c.workers {
                                let cd = self.def(*w).construction.as_ref().unwrap().cooldown;
                                self.state.entities[*w].next_action_tick = t + cd;
                                self.acted[*w] = true;
                            }
                            self.note(player as u8, Activity::Construction);
                        }
                        ConsumerKind::Factory(f) => {
                            let key = self.state.entities[f]
                                .production
                                .as_ref()
                                .unwrap()
                                .active_item
                                .as_ref()
                                .unwrap()
                                .type_key
                                .clone();
                            let cost = self.content.types[self.type_index(&key)?].matter_cost;
                            let prod = self.state.entities[f].production.as_mut().unwrap();
                            let active = prod.active_item.as_mut().unwrap();
                            active.paid_matter += give;
                            if active.paid_matter >= cost - EPS {
                                active.paid_matter = cost;
                                active.awaiting_output = true;
                            }
                            self.state.players[player].counters.total_spend += give;
                            self.state.players[player].counters.unit_spend += give;
                            self.note(player as u8, Activity::Construction);
                        }
                        ConsumerKind::SelfRepair(i) => {
                            healing[i] +=
                                give * self.def(i).self_repair.as_ref().unwrap().hp_per_matter;
                            self.state.players[player].counters.total_spend += give;
                            self.note(player as u8, Activity::Construction);
                        }
                        ConsumerKind::Heal { healer, ally } => {
                            let h = self.def(healer).healing.clone().unwrap();
                            healing[ally] += give * h.hp_per_matter;
                            self.state.entities[healer].next_action_tick = t + h.cooldown;
                            self.acted[healer] = true;
                            self.state.players[player].counters.total_spend += give;
                            self.note(player as u8, Activity::Construction);
                        }
                    }
                    self.progress = true;
                }
            }
            self.state.players[player].bank = bank.max(0.0);
        }
        for (b, give) in sites_to_create {
            let tile = self.state.blueprints[b].tile;
            let at = self.idx(tile);
            self.occ[at] = RESERVED;
            self.pending_sites
                .push((self.state.blueprints[b].id.clone(), give));
            self.structure_version += 1;
        }
        Ok((growth, healing))
    }

    fn consumer_owner(&self, c: &Consumer) -> PlayerId {
        match c.kind {
            ConsumerKind::Site(s)
            | ConsumerKind::Factory(s)
            | ConsumerKind::SelfRepair(s)
            | ConsumerKind::Heal { healer: s, .. } => self.state.entities[s].owner,
            ConsumerKind::Blueprint(b) => self.state.blueprints[b].owner,
        }
    }

    fn combat(&mut self, intents: &[Intent]) -> Vec<f64> {
        let t = self.state.tick;
        let mut damage = vec![0.0; self.state.entities.len()];
        for (i, intent) in intents.iter().enumerate() {
            let Some(j) = intent.attack else {
                continue;
            };
            let w = self.def(i).weapon.clone().unwrap();
            damage[j] += w.damage;
            let (attacker, target) = (&self.state.entities[i], &self.state.entities[j]);
            let (owner, victim) = (attacker.owner, target.owner);
            let (source, target_tile, attacker_id, target_id) = (
                attacker.tile,
                target.tile,
                attacker.id.clone(),
                target.id.clone(),
            );
            self.state.entities[i].next_action_tick = t + w.cooldown;
            self.acted[i] = true;
            self.state.players[usize::from(owner)].counters.damage_dealt += w.damage;
            self.progress = true;
            self.note(owner, Activity::Combat);
            self.note(victim, Activity::Combat);
            self.push_event(PresentationEvent::Attack {
                attacker_id,
                source_tile: source,
                target_id,
                target_tile,
                visual_style: w.visual_style,
            });
            self.push_event(PresentationEvent::Impact {
                target_tile,
                visual_style: w.visual_style,
            });
        }
        damage
    }

    /// hp' = min(new_max, hp + growth + healing − damage); then deaths and site completions.
    fn resolve(
        &mut self,
        growth: &[f64],
        healing: &[f64],
        damage: &[f64],
        annihilated: &[bool],
        intents: &mut Vec<Intent>,
    ) -> Result<()> {
        let n = self.state.entities.len();
        let mut keep = vec![true; n];
        let mut structure_changed = false;
        for (i, keep) in keep.iter_mut().enumerate() {
            let def = self.def(i).clone();
            let e = &mut self.state.entities[i];
            let new_max = if e.lifecycle == Lifecycle::Site {
                (def.max_hp * e.paid_matter / def.matter_cost).min(def.max_hp)
            } else {
                def.max_hp
            };
            let g = growth[i] + healing[i];
            let d = damage[i];
            e.hp = if annihilated[i] {
                0.0
            } else {
                (e.hp + g - d).min(new_max)
            };
            if e.lifecycle == Lifecycle::Site && e.paid_matter >= def.matter_cost - EPS {
                e.paid_matter = def.matter_cost;
                e.lifecycle = Lifecycle::Complete;
                let id = e.id.clone();
                self.state
                    .blueprints
                    .retain(|b| b.site_id.as_ref() != Some(&id));
                self.progress = true;
            }
            if e.hp <= 0.0 {
                *keep = false;
            }
        }
        for i in (0..n).rev() {
            if keep[i] {
                continue;
            }
            let e = self.state.entities.remove(i);
            self.occupancy_changed_tick = self.state.tick;
            let def = self.content.types[self.ty[i]].clone();
            let owner = usize::from(e.owner);
            let stored = e
                .production
                .as_ref()
                .and_then(|p| p.silo.as_ref().map(|s| (p, s)))
                .map_or(0.0, |(p, s)| {
                    s.inventory
                        .iter()
                        .map(|stock| {
                            self.content.types[self.type_index(&stock.type_key).unwrap()]
                                .matter_cost
                                * f64::from(stock.count)
                        })
                        .sum::<f64>()
                        + p.active_item.as_ref().map_or(0.0, |a| a.paid_matter)
                });
            self.state.players[owner].counters.lost_invested_matter += e.paid_matter + stored;
            self.state.players[owner]
                .counters
                .destroyed_replacement_value += def.matter_cost;
            if def.kind == TypeKind::Structure {
                structure_changed = true;
            }
            self.state
                .blueprints
                .retain(|b| b.site_id.as_ref() != Some(&e.id));
            self.push_event(PresentationEvent::Destroyed {
                entity_id: e.id,
                tile: e.tile,
                visual_style: def.weapon.map_or(VisualStyle::Direct, |w| w.visual_style),
            });
            intents.remove(i);
            self.acted.remove(i);
            self.progress = true;
        }
        if keep.iter().any(|k| !k) {
            let acted = self.acted.clone();
            self.reindex()?;
            self.acted = acted;
        }
        if structure_changed {
            self.structure_version += 1;
        }
        Ok(())
    }

    fn motion(&mut self, intents: &[Intent]) -> Result<()> {
        let t = self.state.tick;
        let n = self.state.entities.len();
        let mut moves: Vec<(u64, usize, Tile, Direction)> = vec![];
        for (i, intent) in intents.iter().enumerate().take(n) {
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
            let Some((goal, _)) = intent.goal else {
                continue;
            };
            let from = e.tile;
            let ready = e.next_move_tick <= t;
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
                    && self.step_legal(from, dx, dy, m.neighbors).is_some()
                {
                    if ready {
                        moves.push((self.entity_rank(i), i, next, direction_of(dx, dy)));
                    }
                    continue;
                }
                self.state.entities[i].local_detour.clear();
            }
            let field = if intent.work_goal {
                self.work_field(goal, m.neighbors)
            } else {
                self.field(goal, m.neighbors)
            };
            let here = field[self.idx(from)];
            if here == 0 {
                self.state.entities[i].goal_settled = true;
                continue;
            }
            match self.descend(&field, from, i, m.neighbors) {
                Some((to, _, Occupancy::Settled)) if self.settled_same_goal(to, i) => {
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
                        moves.push((self.entity_rank(i), i, detour[0], direction_of(dx, dy)));
                    }
                    self.state.entities[i].local_detour = detour;
                }
                Some((to, dir, _)) => {
                    self.state.entities[i].goal_settled = false;
                    if ready {
                        moves.push((self.entity_rank(i), i, to, dir));
                    }
                }
                None => {
                    self.state.entities[i].goal_settled = false;
                }
            }
        }
        moves.sort_by_key(|m| (m.0, m.1));
        let mut touched = vec![false; n];
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
        for (_, i, to, dir) in moves {
            if touched[i] {
                continue;
            }
            let from = self.state.entities[i].tile;
            let cooldown = self.def(i).movement.as_ref().unwrap().cooldown;
            let cross = crossing(from, to);
            if cross.is_some_and(|c| diagonals.contains(&c)) {
                self.fail_move(i, to, intents[i].work_goal);
                continue;
            }
            let o = self.occ[self.idx(to)];
            if o == RESERVED {
                self.fail_move(i, to, intents[i].work_goal);
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
            let can_yield = blocker.lifecycle == Lifecycle::Complete
                && self.def(o).movement.is_some()
                && !self.hostile(mover.owner, blocker.owner)
                && !touched[o]
                && blocker.born_at_tick.is_none_or(|b| b < t);
            if !can_yield {
                self.fail_move(i, to, intents[i].work_goal);
                continue;
            }
            if self.settled_same_goal(to, i) {
                // Settled same-goal ally: wait behind it instead of perpetual displacement.
                self.state.entities[i].goal_settled = true;
                self.state.entities[i].local_detour.clear();
                continue;
            }
            if (blocker.goal_settled || mining_here) && !self.transits(i, to, intents[i].work_goal)
            {
                // A settled ally or a miner holding its claimed ore tile yields only to traffic
                // passing through, never to a mover that would rest on its cell; otherwise two
                // goals contend for one cell forever.
                self.fail_move(i, to, intents[i].work_goal);
                continue;
            }
            let b_neighbors = self.def(o).movement.as_ref().unwrap().neighbors;
            let b_cooldown = self.def(o).movement.as_ref().unwrap().cooldown;
            let count = if b_neighbors == Neighbors::Eight {
                8
            } else {
                4
            };
            let mut order: Vec<usize> = (0..count).collect();
            let seed = self.hash(u64::from(t), self.entity_rank(i), self.entity_rank(o));
            for k in (1..count).rev() {
                let j = (seed.rotate_left((k * 7) as u32) % (k as u64 + 1)) as usize;
                order.swap(k, j);
            }
            let mut placed: Option<(Tile, Direction)> = None;
            for k in order {
                let (dx, dy, d) = DIRS[k];
                if let Some(nt) = self.step_legal(to, dx, dy, b_neighbors)
                    && nt != from
                    && self.occ[self.idx(nt)] == NONE
                    && crossing(to, nt).is_none_or(|c| !diagonals.contains(&c))
                {
                    placed = Some((nt, d));
                    break;
                }
            }
            if placed.is_none() {
                let (dx, dy) = (
                    i32::from(from.x) - i32::from(to.x),
                    i32::from(from.y) - i32::from(to.y),
                );
                if self.step_legal(to, dx, dy, b_neighbors).is_some() {
                    placed = Some((from, direction_of(dx, dy)));
                }
            }
            let Some((bt, bd)) = placed else {
                self.fail_move(i, to, intents[i].work_goal);
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
        Ok(())
    }

    /// After stepping onto `to`, the mover could keep descending into a free or yielding cell.
    fn transits(&mut self, mover: usize, to: Tile, work_goal: bool) -> bool {
        let Some(goal) = self.state.entities[mover].resolved_destination else {
            return false;
        };
        let neighbors = self.def(mover).movement.as_ref().unwrap().neighbors;
        let field = if work_goal {
            self.work_field(goal, neighbors)
        } else {
            self.field(goal, neighbors)
        };
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

    fn fail_move(&mut self, i: usize, to: Tile, work_goal: bool) {
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
            let field = if work_goal {
                self.work_field(goal, neighbors)
            } else {
                self.field(goal, neighbors)
            };
            let from = self.state.entities[i].tile;
            let detour = self.local_detour(&field, from, neighbors);
            let e = &mut self.state.entities[i];
            e.local_detour = detour;
            e.failed_move_attempts = 0;
        }
    }

    /// Push a chain of mobile units one legal step each into the nearest available space.
    /// This is involuntary displacement: orders survive, movement cooldown cannot block output.
    fn push_factory_output(&mut self, factory: usize, out: Tile) {
        let occupied = self.occ[self.idx(out)];
        if occupied == NONE || occupied == RESERVED {
            return;
        }
        let mut parents = vec![None; self.occ.len()];
        let mut seen = vec![false; self.occ.len()];
        let mut queue = std::collections::VecDeque::from([out]);
        seen[self.idx(out)] = true;
        let mut vacancy = None;
        while let Some(from) = queue.pop_front() {
            let index = self.occ[self.idx(from)];
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
            let count = if movement.neighbors == Neighbors::Eight {
                8
            } else {
                4
            };
            for (dx, dy, _) in &DIRS[..count] {
                let Some(to) = self.step_legal(from, *dx, *dy, movement.neighbors) else {
                    continue;
                };
                let cell = self.idx(to);
                if seen[cell] {
                    continue;
                }
                seen[cell] = true;
                parents[cell] = Some(from);
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
                if self.occ[cell] != RESERVED {
                    queue.push_back(to);
                }
            }
            if vacancy.is_some() {
                break;
            }
        }
        let Some(mut to) = vacancy else {
            return;
        };
        while let Some(from) = parents[self.idx(to)] {
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

    /// Materialize paid births on vacant output tiles; spawn advances occurrence and loops.
    fn births(&mut self) -> Result<()> {
        let t = self.state.tick;
        let mut newborns: Vec<EntityState> = vec![];
        let mut joins: Vec<(ControlGroupId, EntityId)> = vec![];
        for f in 0..self.state.entities.len() {
            let output = self.state.entities[f]
                .production
                .as_ref()
                .filter(|p| {
                    p.silo.is_none() && p.active_item.as_ref().is_some_and(|a| a.awaiting_output)
                })
                .map(|p| p.output_tile);
            if self.state.entities[f].lifecycle == Lifecycle::Complete
                && let Some(output) = output
            {
                self.push_factory_output(f, output);
            }
            let factory = &self.state.entities[f];
            if factory.lifecycle != Lifecycle::Complete {
                continue;
            }
            let Some(prod) = factory.production.as_ref() else {
                continue;
            };
            let Some(active) = prod.active_item.as_ref() else {
                continue;
            };
            if !active.awaiting_output {
                continue;
            }
            let out = prod.output_tile;
            if !self.traversable(out) || self.occ[self.idx(out)] != NONE {
                continue;
            }
            let ty = self.type_index(&active.type_key)?;
            let def = self.content.types[ty].clone();
            if def.movement.is_none() {
                continue;
            }
            let id = identity::production(&active.item_id, active.occurrence);
            if self.find(&id).is_some() || newborns.iter().any(|n| n.id == id) {
                return Err("duplicate birth identity".into());
            }
            let (dx, dy) = cardinal_offset(prod.output_direction);
            let owner = factory.owner;
            let (mut action, mut locks) = (prod.stored_order.clone(), vec![]);
            let group = prod.spawn_group.clone();
            if let Some(g) = group
                .as_ref()
                .and_then(|g| self.state.control_groups.iter().find(|s| s.id == *g))
            {
                if let Some(saved) = &g.latest_order {
                    action = saved.order.clone();
                }
                locks = g.order_locks.clone();
                joins.push((g.id.clone(), id.clone()));
            }
            let compatible = match &action {
                Order::Idle {} | Order::AttackMove { .. } => true,
                Order::Support { target } => self.find(target).is_some(),
                Order::Mine { .. } => def.mining.is_some(),
                Order::Construct { .. } => def.construction.is_some(),
            };
            if !compatible {
                action = Order::Idle {};
            }
            let support_target = match &action {
                Order::Support { target } => Some(target.clone()),
                _ => None,
            };
            newborns.push(EntityState {
                id,
                owner,
                type_key: active.type_key.clone(),
                tile: out,
                last_move_direction: direction_of(dx, dy),
                hp: def.max_hp,
                paid_matter: def.matter_cost,
                lifecycle: Lifecycle::Complete,
                blueprint_id: None,
                action,
                priority: Priority::Medium,
                next_action_tick: t + 1,
                next_move_tick: t + 1,
                production: None,
                support_target,
                engaged_target: None,
                resolved_destination: None,
                failed_move_attempts: 0,
                blocked_step: None,
                born_at_tick: Some(t),
                order_locks: locks,
                goal_settled: false,
                local_detour: vec![],
            });
            self.finish_production(f);
            self.set_occ(out, RESERVED);
            self.progress = true;
            self.note(owner, Activity::Construction);
        }
        if newborns.is_empty() {
            return Ok(());
        }
        for (group, id) in joins {
            let g = self
                .state
                .control_groups
                .iter_mut()
                .find(|g| g.id == group)
                .unwrap();
            g.members.push(id);
            g.members.sort();
            g.members.dedup();
        }
        self.state.entities.extend(newborns);
        self.occupancy_changed_tick = t;
        self.reindex()?;
        for i in 0..self.state.entities.len() {
            self.intern(i);
        }
        Ok(())
    }

    /// First funding this tick creates site entities on their reserved tiles.
    fn create_sites(&mut self) -> Result<()> {
        if self.pending_sites.is_empty() {
            return Ok(());
        }
        let t = self.state.tick;
        for (id, give) in std::mem::take(&mut self.pending_sites) {
            let b = self
                .state
                .blueprints
                .iter()
                .position(|b| b.id == id)
                .unwrap();
            let bp = self.state.blueprints[b].clone();
            let ty = self.type_index(&bp.type_key)?;
            let def = self.content.types[ty].clone();
            let production = def.production.as_ref().map(|_| {
                let direction = bp.output_direction.unwrap_or(CardinalDirection::E);
                let (dx, dy) = cardinal_offset(direction);
                Production {
                    silo: def.silo.as_ref().map(|_| SiloState {
                        plan: bp
                            .settings
                            .as_ref()
                            .and_then(|s| s.silo_plan.clone())
                            .unwrap_or_default(),
                        ..SiloState::default()
                    }),
                    pending_items: bp
                        .settings
                        .as_ref()
                        .map(|s| {
                            s.queue
                                .iter()
                                .enumerate()
                                .map(|(index, key)| QueueItem {
                                    item_id: QueueItemId {
                                        birth_command: bp
                                            .settings_command
                                            .clone()
                                            .expect("configured blueprint has a settings command"),
                                        item_index: index as u16,
                                    },
                                    type_key: key.clone(),
                                    loop_enabled: s
                                        .queue_loop_flags
                                        .get(index)
                                        .copied()
                                        .unwrap_or(false),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    active_item: None,
                    loop_enabled: bp.settings.as_ref().is_some_and(|s| s.loop_enabled),
                    stored_order: bp
                        .settings
                        .as_ref()
                        .map(|s| s.order.clone())
                        .unwrap_or(Order::Idle {}),
                    output_tile: self.offset(bp.tile, dx, dy).unwrap_or(bp.tile),
                    occurrence_counters: vec![],
                    spawn_group: None,
                    output_direction: direction,
                }
            });
            self.state.entities.push(EntityState {
                id: bp.id.clone(),
                owner: bp.owner,
                type_key: bp.type_key.clone(),
                tile: bp.tile,
                last_move_direction: Direction::N,
                hp: (give / def.matter_cost * def.max_hp).max(EPS),
                paid_matter: give,
                lifecycle: Lifecycle::Site,
                blueprint_id: Some(bp.id.clone()),
                action: Order::Idle {},
                priority: bp.priority,
                next_action_tick: t + 1,
                next_move_tick: t + 1,
                production,
                support_target: None,
                engaged_target: None,
                resolved_destination: None,
                failed_move_attempts: 0,
                blocked_step: None,
                born_at_tick: Some(t),
                order_locks: vec![],
                goal_settled: false,
                local_detour: vec![],
            });
            self.state.blueprints[b].site_id = Some(bp.id);
            self.occupancy_changed_tick = t;
        }
        self.reindex()?;
        for i in 0..self.state.entities.len() {
            self.intern(i);
        }
        Ok(())
    }

    fn survival(&mut self) {
        let t = self.state.tick;
        let players = self.state.players.len();
        let mut active = vec![false; players];
        let mut build = vec![false; players];
        for (i, e) in self.state.entities.iter().enumerate() {
            if e.lifecycle != Lifecycle::Complete {
                continue;
            }
            let def = &self.content.types[self.ty[i]];
            let p = usize::from(e.owner);
            active[p] |= def.counts_for_survival;
            build[p] |= def.provides_build_ability;
        }
        for p in 0..players {
            let mut reasons = vec![];
            if !active[p] {
                reasons.push(Reason::NoActiveBuilding);
            }
            if !build[p] {
                reasons.push(Reason::NoBuildAbility);
            }
            let eliminated = !reasons.is_empty();
            let player = &mut self.state.players[p];
            if player.currently_eliminated != eliminated {
                player.currently_eliminated = eliminated;
                player.status_since_tick = t + 1;
                let transition = SurvivalTransition {
                    player_id: p as u8,
                    resolved_tick: t,
                    status: if eliminated {
                        SurvivalStatus::Eliminated
                    } else {
                        SurvivalStatus::Alive
                    },
                    reasons: reasons.clone(),
                };
                self.state.survival_transitions.push(transition.clone());
                self.push_event(PresentationEvent::Survival { transition });
            }
            self.state.players[p].elimination_reasons = reasons;
        }
    }

    /// Decided cutoff: at most one side can still act. A side is finished when every member has
    /// no entities, or is eliminated with only idle entities and no future commands.
    pub fn decided(&self) -> bool {
        if !self.state.missiles.is_empty() {
            return false;
        }
        let now = self.state.tick;
        let sides =
            scoring::sides(self.config.player_count, &self.config.multiplayer).unwrap_or_default();
        let can_act = |p: PlayerId| {
            let mut own = self.state.entities.iter().filter(|e| e.owner == p);
            if own.clone().next().is_none() {
                return false;
            }
            !self.state.players[usize::from(p)].currently_eliminated
                || own.any(|e| !matches!(e.action, Order::Idle {}))
                || self.state.entities.iter().enumerate().any(|(i, e)| {
                    e.owner == p
                        && e.lifecycle == Lifecycle::Complete
                        && self.silo_has_launch_work(i, true)
                })
                || self
                    .events
                    .iter()
                    .any(|t| t.player == p && t.tick >= now && !t.commands.is_empty())
        };
        let live = sides
            .values()
            .filter(|ps| ps.iter().any(|p| can_act(*p)))
            .count();
        live <= 1
    }

    /// Inactivity cutoff: quiet through the deadline, no future command and no cooldown-only action.
    pub fn inactive(&self) -> bool {
        if !self.state.missiles.is_empty() {
            return false;
        }
        let now = self.state.tick;
        if now < self.state.inactivity_deadline {
            return false;
        }
        if self
            .events
            .iter()
            .any(|t| t.tick >= now && !t.commands.is_empty())
        {
            return false;
        }
        !self.cooldown_guard()
    }

    fn cooldown_guard(&self) -> bool {
        for (i, e) in self.state.entities.iter().enumerate() {
            if e.lifecycle != Lifecycle::Complete {
                continue;
            }
            if self.silo_has_launch_work(i, false) {
                return true;
            }
            let def = &self.content.types[self.ty[i]];
            if let Some(w) = &def.weapon
                && w.damage > 0.0
                && let Some(j) = e.engaged_target.as_ref().and_then(|id| self.find(id))
                && self.target_valid(i, j, w.range, !w.indirect)
            {
                return true;
            }
            if self.mines_here(i) {
                return true;
            }
            if let (Order::Support { target }, Some(h)) = (&e.action, &def.healing)
                && let Some(ally) = self.find(target)
                && self.state.entities[ally].lifecycle == Lifecycle::Complete
                && self.state.entities[ally].hp < self.def(ally).max_hp - EPS
                && Self::dist2(e.tile, self.state.entities[ally].tile) <= h.range * h.range + EPS
                && self.state.players[usize::from(e.owner)].bank > 0.0
            {
                return true;
            }
        }
        false
    }
}

pub(crate) fn in_rect(t: Tile, r: &Rect) -> bool {
    t.x >= r.min.x && t.x <= r.max.x && t.y >= r.min.y && t.y <= r.max.y
}
pub(crate) fn chebyshev(a: Tile, b: Tile) -> i32 {
    (i32::from(a.x) - i32::from(b.x))
        .abs()
        .max((i32::from(a.y) - i32::from(b.y)).abs())
}
