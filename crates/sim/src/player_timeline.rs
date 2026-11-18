//! A player-scoped projection of an authoritative execution. Policy and analysis code can
//! use this database without access to the simulation checkpoint or an opponent's ledger.
use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VisibleEntity {
    pub id: EntityId,
    pub owner: PlayerId,
    pub type_key: TypeKey,
    pub tile: Tile,
    pub hp: f64,
    pub lifecycle: Lifecycle,
    pub last_move_direction: Direction,
    pub activity: Activity,
}

impl VisibleEntity {
    fn matches(&self, entity: &EntityState) -> bool {
        self.id == entity.id
            && self.owner == entity.owner
            && self.type_key == entity.type_key
            && self.tile == entity.tile
            && self.hp == entity.hp
            && self.lifecycle == entity.lifecycle
            && self.last_move_direction == entity.last_move_direction
            && self.activity == crate::output::activity_of(entity)
    }
}

impl From<&EntityState> for VisibleEntity {
    fn from(entity: &EntityState) -> Self {
        Self {
            id: entity.id.clone(),
            owner: entity.owner,
            type_key: entity.type_key.clone(),
            tile: entity.tile,
            hp: entity.hp,
            lifecycle: entity.lifecycle,
            last_move_direction: entity.last_move_direction,
            activity: crate::output::activity_of(entity),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OwnProduction {
    pub pending_items: Vec<QueueItem>,
    pub active_item: Option<ActiveItem>,
    pub loop_enabled: bool,
    pub stored_order: Order,
    pub output_tile: Tile,
    pub spawn_group: Option<ControlGroupId>,
    pub output_direction: CardinalDirection,
    pub silo: Option<OwnSilo>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OwnSilo {
    pub inventory: Vec<MissileStock>,
    pub plan: SiloPlan,
}

impl OwnProduction {
    fn matches(&self, p: &Production) -> bool {
        self.pending_items == p.pending_items
            && self.active_item == p.active_item
            && self.loop_enabled == p.loop_enabled
            && self.stored_order == p.stored_order
            && self.output_tile == p.output_tile
            && self.spawn_group == p.spawn_group
            && self.output_direction == p.output_direction
            && match (&self.silo, &p.silo) {
                (Some(own), Some(state)) => {
                    own.inventory == state.inventory && own.plan == state.plan
                }
                (None, None) => true,
                _ => false,
            }
    }
}

impl From<&Production> for OwnProduction {
    fn from(p: &Production) -> Self {
        Self {
            pending_items: p.pending_items.clone(),
            active_item: p.active_item.clone(),
            loop_enabled: p.loop_enabled,
            stored_order: p.stored_order.clone(),
            output_tile: p.output_tile,
            spawn_group: p.spawn_group.clone(),
            output_direction: p.output_direction,
            silo: p.silo.as_ref().map(|silo| OwnSilo {
                inventory: silo.inventory.clone(),
                plan: silo.plan.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OwnEntity {
    pub id: EntityId,
    pub owner: PlayerId,
    pub type_key: TypeKey,
    pub tile: Tile,
    pub last_move_direction: Direction,
    pub hp: f64,
    pub paid_matter: f64,
    pub lifecycle: Lifecycle,
    pub blueprint_id: Option<BlueprintId>,
    pub action: Order,
    pub activity: Activity,
    pub priority: Priority,
    pub production: Option<Arc<OwnProduction>>,
    pub engaged_target: Option<EntityId>,
    pub order_locks: Vec<OrderLock>,
}

impl OwnEntity {
    fn new(
        entity: &EntityState,
        engaged_target: Option<&EntityId>,
        activity: Activity,
        previous: Option<&OwnEntity>,
    ) -> Self {
        Self {
            id: entity.id.clone(),
            owner: entity.owner,
            type_key: entity.type_key.clone(),
            tile: entity.tile,
            last_move_direction: entity.last_move_direction,
            hp: entity.hp,
            paid_matter: entity.paid_matter,
            lifecycle: entity.lifecycle,
            blueprint_id: entity.blueprint_id.clone(),
            action: entity.action.clone(),
            activity,
            priority: entity.priority,
            production: entity.production.as_ref().map(|p| {
                match previous.and_then(|e| e.production.as_ref()) {
                    Some(old) if old.matches(p) => Arc::clone(old),
                    _ => Arc::new(OwnProduction::from(p)),
                }
            }),
            engaged_target: engaged_target.cloned(),
            order_locks: entity.order_locks.clone(),
        }
    }

    fn matches(
        &self,
        e: &EntityState,
        engaged_target: Option<&EntityId>,
        activity: Activity,
    ) -> bool {
        self.id == e.id
            && self.owner == e.owner
            && self.type_key == e.type_key
            && self.tile == e.tile
            && self.last_move_direction == e.last_move_direction
            && self.hp == e.hp
            && self.paid_matter == e.paid_matter
            && self.lifecycle == e.lifecycle
            && self.blueprint_id == e.blueprint_id
            && self.action == e.action
            && self.activity == activity
            && self.priority == e.priority
            && match (&self.production, &e.production) {
                (Some(own), Some(state)) => own.matches(state),
                (None, None) => true,
                _ => false,
            }
            && self.engaged_target.as_ref() == engaged_target
            && self.order_locks == e.order_locks
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OwnPlayer {
    pub player_id: PlayerId,
    pub bank: f64,
    pub currently_eliminated: bool,
    pub status_since_tick: Tick,
    pub elimination_reasons: Vec<Reason>,
}

impl From<&PlayerState> for OwnPlayer {
    fn from(p: &PlayerState) -> Self {
        Self {
            player_id: p.player_id,
            bank: p.bank,
            currently_eliminated: p.currently_eliminated,
            status_since_tick: p.status_since_tick,
            elimination_reasons: p.elimination_reasons.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlayerFrame {
    pub tick: Tick,
    pub player: OwnPlayer,
    pub own_entities: Arc<[Arc<OwnEntity>]>,
    pub own_blueprints: Arc<[Arc<Blueprint>]>,
    pub own_control_groups: Arc<[ControlGroupState]>,
    /// Other seats' allied entities and observed hostiles. Own entities are listed separately.
    pub visible_entities: Arc<[Arc<VisibleEntity>]>,
    /// Ore is public throughout the map, even in fog.
    pub visible_ore: Arc<[OreCell]>,
    pub missiles: Arc<[MissileFlight]>,
    pub recon: Arc<[ReconZone]>,
    pub hostile_missiles: Arc<[VisibleMissile]>,
    pub survival: Arc<[PublicSurvival]>,
    /// Events of simulation tick t are attached to the resulting frame S[t+1].
    pub events: Vec<WorldEvent>,
    /// Origins of hostile launches actually seen. Their flight plans stay private.
    pub hostile_missile_launches: Vec<VisibleMissileLaunch>,
    pub command_outcomes: Vec<CommandOutcome>,
    #[serde(skip)]
    sight: Sight,
}

impl PlayerFrame {
    pub fn sees(&self, tile: Tile) -> bool {
        self.sight.sees(tile)
    }

    /// Row-major tile indices, from the least significant bit of each word.
    pub fn visible_tile_words(&self) -> &[u64] {
        &self.sight.tiles
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PublicSurvival {
    pub player_id: PlayerId,
    pub currently_eliminated: bool,
    pub status_since_tick: Tick,
    pub has_survival_building: bool,
    pub has_build_ability: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VisibleMissile {
    pub owner: PlayerId,
    pub type_key: TypeKey,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VisibleMissileLaunch {
    pub tick: Tick,
    pub owner: PlayerId,
    pub type_key: TypeKey,
    pub origin: Tile,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PublicStatsSample {
    pub tick: Tick,
    pub players: Vec<PublicPlayerStats>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PublicPlayerStats {
    pub player_id: PlayerId,
    pub bank: f64,
    pub mined: f64,
    pub total_spend: f64,
    pub living_army_value: f64,
    pub living_infrastructure_value: f64,
    /// Population totals are shown in the round comparison, rather than as a live graph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_count: Option<u32>,
}

impl From<StatsSample> for PublicStatsSample {
    fn from(stats: StatsSample) -> Self {
        Self {
            tick: stats.tick,
            players: stats
                .players
                .into_iter()
                .map(|p| PublicPlayerStats {
                    player_id: p.player_id,
                    bank: p.bank,
                    mined: p.counters.mined,
                    total_spend: p.counters.total_spend,
                    living_army_value: p.living_army_value,
                    living_infrastructure_value: p.living_infrastructure_value,
                    entity_count: Some(p.entity_count),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LastSeen<'a> {
    pub tick: Tick,
    pub entity: &'a VisibleEntity,
}

/// Consecutive sightings of an unchanged entity form one span. A gap starts a distinct span
/// even when the entity reappears in the same state.
#[derive(Debug)]
struct Sighting {
    from: Tick,
    through: Tick,
    entity: Arc<VisibleEntity>,
}

#[derive(Debug)]
pub struct PlayerTimeline {
    first_tick: Tick,
    frames: Vec<PlayerFrame>,
    sightings: BTreeMap<EntityId, Vec<Sighting>>,
    buildings: BTreeSet<EntityId>,
    remembered_tiles: BTreeMap<Tile, EntityId>,
    cleared: BTreeMap<EntityId, Vec<Tick>>,
    pub terrain: Arc<Terrain>,
    pub turns: Vec<AcceptedTurn>,
    pub public_stats: Arc<BTreeMap<Tick, PublicStatsSample>>,
    /// Aggregate count displayed in the round comparison; individual events still require sight.
    pub destructions: u64,
    pub outcome: Option<Arc<Outcome>>,
}

impl PlayerTimeline {
    pub fn frame(&self, tick: Tick) -> Option<&PlayerFrame> {
        self.frames.get(tick.checked_sub(self.first_tick)? as usize)
    }

    pub fn frames(&self) -> impl DoubleEndedIterator<Item = &PlayerFrame> + ExactSizeIterator {
        self.frames.iter()
    }

    pub fn last_seen(&self, tick: Tick, id: &EntityId) -> Option<LastSeen<'_>> {
        self.frame(tick)?;
        let history = self.sightings.get(id)?;
        let index = history.partition_point(|sighting| sighting.from <= tick);
        let sighting = history.get(index.checked_sub(1)?)?;
        Some(LastSeen {
            tick: tick.min(sighting.through),
            entity: &sighting.entity,
        })
    }

    pub fn last_seen_entities(&self, tick: Tick) -> impl Iterator<Item = LastSeen<'_>> {
        self.sightings
            .keys()
            .filter_map(move |id| self.last_seen(tick, id))
    }

    /// Other players’ buildings last observed in fog. A later clear view of the tile invalidates
    /// the remembered silhouette without erasing earlier observations.
    pub fn remembered_buildings(&self, tick: Tick) -> impl Iterator<Item = LastSeen<'_>> {
        let frame = self.frame(tick);
        self.buildings.iter().filter_map(move |id| {
            let last = self.last_seen(tick, id)?;
            if frame?.sees(last.entity.tile) {
                return None;
            }
            let invalidated = self.cleared.get(id).is_some_and(|ticks| {
                let index = ticks.partition_point(|cleared| *cleared <= tick);
                index > 0 && ticks[index - 1] > last.tick
            });
            (!invalidated).then_some(last)
        })
    }

    fn push(&mut self, frame: PlayerFrame, types: &BTreeMap<&str, &TypeDefinition>) {
        let mut observed_buildings = BTreeMap::new();
        for e in &*frame.visible_entities {
            let history = self.sightings.entry(e.id.clone()).or_default();
            if let Some(previous) = history.last_mut()
                && previous.through.checked_add(1) == Some(frame.tick)
                && Arc::ptr_eq(&previous.entity, e)
            {
                previous.through = frame.tick;
            } else {
                history.push(Sighting {
                    from: frame.tick,
                    through: frame.tick,
                    entity: Arc::clone(e),
                });
            }
            if types
                .get(e.type_key.as_str())
                .is_some_and(|def| def.kind == TypeKind::Structure)
            {
                self.buildings.insert(e.id.clone());
                observed_buildings.insert(e.tile, e.id.clone());
            }
        }
        self.remembered_tiles.retain(|tile, id| {
            if frame.sees(*tile) && !observed_buildings.get(tile).is_some_and(|seen| seen == id) {
                self.cleared.entry(id.clone()).or_default().push(frame.tick);
                false
            } else {
                true
            }
        });
        self.remembered_tiles.extend(observed_buildings);
        for event in &frame.events {
            if let PresentationEvent::Destroyed { entity_id, .. } = &event.event
                && self.buildings.contains(entity_id)
            {
                self.cleared
                    .entry(entity_id.clone())
                    .or_default()
                    .push(frame.tick);
            }
        }
        self.frames.push(frame);
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Sight {
    width: u16,
    height: u16,
    tiles: Arc<[u64]>,
}

impl Sight {
    fn sees(&self, tile: Tile) -> bool {
        if tile.x >= self.width || tile.y >= self.height {
            return false;
        }
        let index = usize::from(tile.y) * usize::from(self.width) + usize::from(tile.x);
        self.tiles[index / 64] & (1 << (index % 64)) != 0
    }
}

#[derive(Default)]
struct VisionCache {
    walls: BTreeSet<Tile>,
    legacy: bool,
    /// Static obstacles plus current built walls determine a footprint. A wall change clears it.
    footprints: BTreeMap<(Tile, u64), Vec<usize>>,
}

impl VisionCache {
    fn update_walls(&mut self, state: &WorldState) {
        let walls = state
            .entities
            .iter()
            .filter(|e| e.type_key == "wall")
            .map(|e| e.tile)
            .collect();
        if self.walls != walls {
            self.walls = walls;
            self.footprints.clear();
        }
    }

    fn sight(
        &mut self,
        state: &WorldState,
        player: PlayerId,
        types: &BTreeMap<&str, &TypeDefinition>,
        teams: &[SideId],
        previous: Option<&Sight>,
    ) -> Sight {
        let terrain = &state.terrain;
        let width = usize::from(terrain.width);
        let height = usize::from(terrain.height);
        let mut tiles = vec![0; (width * height).div_ceil(64)];
        let mark = |tiles: &mut [u64], index: usize| tiles[index / 64] |= 1 << (index % 64);
        for observer in &state.entities {
            if observer.lifecycle != Lifecycle::Complete || !allied(teams, player, observer.owner) {
                continue;
            }
            let Some(def) = types.get(observer.type_key.as_str()) else {
                continue;
            };
            // Bound the cache on moving-unit workloads; each footprint is local to its radius.
            if self.footprints.len() >= 8192 {
                self.footprints.clear();
            }
            let footprint = self
                .footprints
                .entry((observer.tile, def.vision.to_bits()))
                .or_insert_with(|| {
                    let mut footprint = Vec::new();
                    let mut reveal = |tile: Tile| {
                        footprint.push(usize::from(tile.y) * width + usize::from(tile.x))
                    };
                    if self.legacy {
                        let radius = def.vision.ceil() as u16;
                        let x0 = observer.tile.x.saturating_sub(radius);
                        let y0 = observer.tile.y.saturating_sub(radius);
                        let x1 = observer
                            .tile
                            .x
                            .saturating_add(radius)
                            .min(terrain.width - 1);
                        let y1 = observer
                            .tile
                            .y
                            .saturating_add(radius)
                            .min(terrain.height - 1);
                        for y in y0..=y1 {
                            for x in x0..=x1 {
                                let tile = Tile { x, y };
                                if Sim::dist2(observer.tile, tile) <= def.vision.powi(2) {
                                    reveal(tile);
                                }
                            }
                        }
                    } else {
                        vision::flood_vision(
                            terrain,
                            observer.tile,
                            def.vision,
                            |tile| self.walls.contains(&tile),
                            reveal,
                        );
                    }
                    footprint
                });
            for &index in footprint.iter() {
                mark(&mut tiles, index);
            }
        }
        let mut reveal_circle = |x: f64, y: f64, radius: f64| {
            let xmin = ((x - radius).floor() as usize).min(width - 1);
            let xmax = ((x + radius).ceil() as usize).min(width - 1);
            let ymin = ((y - radius).floor() as usize).min(height - 1);
            let ymax = ((y + radius).ceil() as usize).min(height - 1);
            for iy in ymin..=ymax {
                for ix in xmin..=xmax {
                    if (x - ix as f64).powi(2) + (y - iy as f64).powi(2) <= radius * radius {
                        mark(&mut tiles, iy * width + ix);
                    }
                }
            }
        };
        for flight in &state.missiles {
            if !allied(teams, player, flight.owner)
                || flight.launch_tick >= state.tick
                || flight.impact_tick < state.tick
            {
                continue;
            }
            if let Some(missile) = types
                .get(flight.type_key.as_str())
                .and_then(|def| def.missile.as_ref())
                && missile.effect == MissileEffect::Satellite
            {
                let (x, y) = missiles::flight_position(flight, f64::from(state.tick));
                reveal_circle(x, y, missile.radius);
            }
        }
        for zone in &state.recon {
            if zone.starts_at <= state.tick
                && zone.expires_at > state.tick
                && allied(teams, player, zone.owner)
            {
                reveal_circle(
                    f64::from(zone.center.x),
                    f64::from(zone.center.y),
                    zone.radius,
                );
            }
        }
        Sight {
            width: terrain.width,
            height: terrain.height,
            tiles: share(tiles, previous.map(|sight| &sight.tiles)),
        }
    }
}

fn share<T: PartialEq>(rows: Vec<T>, previous: Option<&Arc<[T]>>) -> Arc<[T]> {
    match previous {
        Some(rows_before) if rows == **rows_before => Arc::clone(rows_before),
        _ => rows.into(),
    }
}

fn reuse_rows<'a, T: PartialEq + Clone + 'a>(
    rows: impl Iterator<Item = &'a T>,
    previous: Option<&Arc<[Arc<T>]>>,
    identity: impl Fn(&T) -> &EntityId,
) -> Arc<[Arc<T>]> {
    let mut before = previous.into_iter().flat_map(|rows| rows.iter()).peekable();
    let current = rows
        .map(|row| {
            while before
                .peek()
                .is_some_and(|old| identity(old) < identity(row))
            {
                before.next();
            }
            match before.peek() {
                Some(old) if ***old == *row => Arc::clone(old),
                _ => Arc::new(row.clone()),
            }
        })
        .collect();
    share(current, previous)
}

fn allied(teams: &[SideId], player: PlayerId, other: PlayerId) -> bool {
    teams[usize::from(player)] == teams[usize::from(other)]
}

fn teams(config: &MatchConfig) -> Result<Vec<SideId>> {
    let mut teams = vec![SideId::Player { player_id: 0 }; usize::from(config.player_count)];
    for (side, players) in scoring::sides(config.player_count, &config.multiplayer)? {
        for player in players {
            teams[usize::from(player)] = side.clone();
        }
    }
    Ok(teams)
}

impl VisionCache {
    fn frame(
        &mut self,
        state: &WorldState,
        player: PlayerId,
        types: &BTreeMap<&str, &TypeDefinition>,
        teams: &[SideId],
        previous: Option<&PlayerFrame>,
        ore: Arc<[OreCell]>,
    ) -> PlayerFrame {
        let sight = self.sight(state, player, types, teams, previous.map(|f| &f.sight));
        let mut before = previous
            .into_iter()
            .flat_map(|f| f.visible_entities.iter())
            .peekable();
        let visible_entities: Vec<_> = state
            .entities
            .iter()
            .filter(|e| e.owner != player && sight.sees(e.tile))
            .map(|e| {
                while before.peek().is_some_and(|old| old.id < e.id) {
                    before.next();
                }
                match before.peek() {
                    Some(old) if old.matches(e) => Arc::clone(old),
                    _ => Arc::new(VisibleEntity::from(e)),
                }
            })
            .collect();
        let sees_id = |id: &EntityId| {
            visible_entities.binary_search_by(|e| e.id.cmp(id)).is_ok()
                || state
                    .entities
                    .binary_search_by(|e| e.id.cmp(id))
                    .is_ok_and(|i| state.entities[i].owner == player)
        };
        let mut before = previous
            .into_iter()
            .flat_map(|f| f.own_entities.iter())
            .peekable();
        let own_entities = state
            .entities
            .iter()
            .filter(|e| e.owner == player)
            .map(|e| {
                while before.peek().is_some_and(|old| old.id < e.id) {
                    before.next();
                }
                let engaged = e.engaged_target.as_ref().filter(|id| sees_id(id));
                let activity = crate::output::activity_of(e);
                let previous = before.peek().filter(|old| old.id == e.id).copied();
                match previous {
                    Some(old) if old.matches(e, engaged, activity) => Arc::clone(old),
                    _ => Arc::new(OwnEntity::new(
                        e,
                        engaged,
                        activity,
                        previous.map(Arc::as_ref),
                    )),
                }
            })
            .collect();
        let mut survival = state
            .players
            .iter()
            .map(|p| PublicSurvival {
                player_id: p.player_id,
                currently_eliminated: p.currently_eliminated,
                status_since_tick: p.status_since_tick,
                has_survival_building: false,
                has_build_ability: false,
            })
            .collect::<Vec<_>>();
        for e in &state.entities {
            if e.lifecycle == Lifecycle::Complete
                && let Some(def) = types.get(e.type_key.as_str())
            {
                let status = &mut survival[usize::from(e.owner)];
                status.has_survival_building |= def.counts_for_survival;
                status.has_build_ability |= def.provides_build_ability;
            }
        }
        let hostile_missiles = state
            .missiles
            .iter()
            .filter_map(|f| {
                if allied(teams, player, f.owner)
                    || f.launch_tick >= state.tick
                    || f.impact_tick < state.tick
                {
                    return None;
                }
                let (x, y) = missiles::flight_position(f, f64::from(state.tick));
                sight
                    .sees(Tile {
                        x: x.round() as u16,
                        y: y.round() as u16,
                    })
                    .then(|| VisibleMissile {
                        owner: f.owner,
                        x,
                        y,
                        type_key: f.type_key.clone(),
                        heading: (f64::from(f.target.y) - f64::from(f.origin.y))
                            .atan2(f64::from(f.target.x) - f64::from(f.origin.x)),
                    })
            })
            .collect();
        PlayerFrame {
            tick: state.tick,
            player: OwnPlayer::from(&state.players[usize::from(player)]),
            own_entities: share(own_entities, previous.map(|f| &f.own_entities)),
            own_blueprints: reuse_rows(
                state.blueprints.iter().filter(|b| b.owner == player),
                previous.map(|f| &f.own_blueprints),
                |b| &b.id,
            ),
            own_control_groups: share(
                state
                    .control_groups
                    .iter()
                    .filter(|g| g.id.owner == player)
                    .cloned()
                    .collect(),
                previous.map(|f| &f.own_control_groups),
            ),
            visible_entities: share(visible_entities, previous.map(|f| &f.visible_entities)),
            visible_ore: ore,
            missiles: share(
                state
                    .missiles
                    .iter()
                    .filter(|flight| allied(teams, player, flight.owner))
                    .cloned()
                    .collect(),
                previous.map(|f| &f.missiles),
            ),
            recon: share(
                state
                    .recon
                    .iter()
                    .filter(|zone| allied(teams, player, zone.owner))
                    .cloned()
                    .collect(),
                previous.map(|f| &f.recon),
            ),
            survival: share(survival, previous.map(|f| &f.survival)),
            hostile_missiles: share(hostile_missiles, previous.map(|f| &f.hostile_missiles)),
            events: Vec::new(),
            hostile_missile_launches: Vec::new(),
            command_outcomes: Vec::new(),
            sight,
        }
    }
}

fn ore_cells(state: &WorldState, previous: Option<&Arc<[OreCell]>>) -> Arc<[OreCell]> {
    share(
        state
            .ore
            .iter()
            .enumerate()
            .filter(|(_, amount)| **amount > 0.0)
            .map(|(index, amount)| OreCell {
                index: index as u32,
                remaining: *amount,
            })
            .collect(),
        previous,
    )
}

fn event_visible(
    event: &PresentationEvent,
    player: PlayerId,
    teams: &[SideId],
    before: &PlayerFrame,
    after: &PlayerFrame,
) -> bool {
    let sees = |tile| before.sees(tile);
    let owns = |id: &EntityId| {
        [before, after].iter().any(|frame| {
            frame
                .own_entities
                .binary_search_by(|e| e.id.cmp(id))
                .is_ok()
        })
    };
    match event {
        PresentationEvent::Move {
            entity_id,
            from,
            to,
        } => owns(entity_id) || (sees(*from) && sees(*to)),
        PresentationEvent::Displacement {
            mover_from,
            mover_to,
            blocker_from,
            blocker_to,
            ..
        } => [*mover_from, *mover_to, *blocker_from, *blocker_to]
            .into_iter()
            .all(sees),
        PresentationEvent::Attack {
            source_tile,
            target_tile,
            ..
        } => sees(*source_tile) && sees(*target_tile),
        PresentationEvent::Impact { target_tile, .. } => sees(*target_tile),
        PresentationEvent::Destroyed {
            entity_id, tile, ..
        } => owns(entity_id) || sees(*tile),
        PresentationEvent::MissileLaunch { flight } => allied(teams, player, flight.owner),
        PresentationEvent::MissileImpact { owner, target, .. } => {
            allied(teams, player, *owner) || sees(*target)
        }
        PresentationEvent::Survival { transition } => transition.player_id == player,
    }
}

/// Project one state for callers handling a trusted exact snapshot. Use `build` when history,
/// publication metrics, or observed outcomes are needed.
pub fn project(
    state: &WorldState,
    player: PlayerId,
    config: &MatchConfig,
    content: &Content,
) -> Result<PlayerFrame> {
    if player >= config.player_count || usize::from(player) >= state.players.len() {
        return Err("player is outside the match".into());
    }
    let types = content.types.iter().map(|d| (d.key.as_str(), d)).collect();
    let mut cache = VisionCache::default();
    cache.update_walls(state);
    Ok(cache.frame(
        state,
        player,
        &types,
        &teams(config)?,
        None,
        ore_cells(state, None),
    ))
}

/// Execute the same job as the simulation worker and project every exact state for the selected
/// seats. A complete history must start from the initial checkpoint. Callers using a later
/// checkpoint receive the observed suffix beginning at that checkpoint.
pub fn build(
    request: &SimRequest,
    players: &[PlayerId],
    cancel: &AtomicBool,
) -> Result<BTreeMap<PlayerId, PlayerTimeline>> {
    build_inner(request, players, cancel, None)
}

/// Check the replay against its authoritative publication before handing the player data out.
/// The digest stays in the trusted execution boundary.
pub fn build_verified(
    request: &SimRequest,
    players: &[PlayerId],
    cancel: &AtomicBool,
    terminal_tick: Tick,
    final_hash: &str,
) -> Result<BTreeMap<PlayerId, PlayerTimeline>> {
    build_inner(request, players, cancel, Some((terminal_tick, final_hash)))
}

fn build_inner(
    request: &SimRequest,
    players: &[PlayerId],
    cancel: &AtomicBool,
    expected: Option<(Tick, &str)>,
) -> Result<BTreeMap<PlayerId, PlayerTimeline>> {
    let teams = teams(&request.config)?;
    let types = request
        .content
        .types
        .iter()
        .map(|d| (d.key.as_str(), d))
        .collect();
    let terrain = Arc::new(request.checkpoint.terrain.clone());
    let mut ledgers = BTreeMap::new();
    for &player in players {
        if player >= request.config.player_count {
            return Err("player is outside the match".into());
        }
        ledgers.insert(
            player,
            PlayerTimeline {
                first_tick: request.checkpoint.tick,
                frames: Vec::new(),
                sightings: BTreeMap::new(),
                buildings: BTreeSet::new(),
                remembered_tiles: BTreeMap::new(),
                cleared: BTreeMap::new(),
                terrain: Arc::clone(&terrain),
                turns: request
                    .events
                    .iter()
                    .filter(|turn| turn.player == player)
                    .cloned()
                    .collect(),
                public_stats: Arc::default(),
                destructions: 0,
                outcome: None,
            },
        );
    }
    let mut cache = VisionCache {
        legacy: request.fingerprint.sim_build.ends_with(" rules 7"),
        ..VisionCache::default()
    };
    let mut public_stats = BTreeMap::new();
    let mut destructions = 0;
    let mut cursor = 0;
    let mut ore = None;
    let mut sequences = BTreeMap::<PlayerId, u32>::new();
    let result = run_observed(
        request,
        cancel,
        &mut |output| match output {
            Output::Stats(stats) => {
                public_stats.insert(stats.tick, PublicStatsSample::from(stats));
            }
            Output::Events(events) => {
                destructions += events
                    .iter()
                    .filter(|e| matches!(e.event, PresentationEvent::Destroyed { .. }))
                    .count() as u64;
            }
            _ => {}
        },
        &mut || {},
        &mut |sim, events| {
            cache.update_walls(&sim.state);
            let current_ore = ore_cells(&sim.state, ore.as_ref());
            for (&player, timeline) in &mut ledgers {
                let previous = timeline.frames.last();
                let mut frame = cache.frame(
                    &sim.state,
                    player,
                    &types,
                    &teams,
                    previous,
                    Arc::clone(&current_ore),
                );
                if let Some(previous) = previous {
                    frame.hostile_missile_launches = events
                        .iter()
                        .filter_map(|event| {
                            let PresentationEvent::MissileLaunch { flight } = &event.event else {
                                return None;
                            };
                            (!allied(&teams, player, flight.owner) && previous.sees(flight.origin))
                                .then(|| VisibleMissileLaunch {
                                    tick: event.tick,
                                    owner: flight.owner,
                                    type_key: flight.type_key.clone(),
                                    origin: flight.origin,
                                })
                        })
                        .collect();
                    frame.events = events
                        .iter()
                        .filter(|e| event_visible(&e.event, player, &teams, previous, &frame))
                        .map(|e| {
                            let mut event = e.clone();
                            let sequence = sequences.entry(player).or_default();
                            event.sequence = *sequence;
                            *sequence += 1;
                            event
                        })
                        .collect();
                }
                frame.command_outcomes = sim.command_outcomes[cursor..]
                    .iter()
                    .filter(|o| o.command_id.player == player)
                    .cloned()
                    .map(|mut outcome| {
                        // An attempt to act on a foreign ID must not distinguish a concealed live
                        // target from an absent one. The supplied ID belongs to the submitted command.
                        for skipped in &mut outcome.skipped {
                            if skipped.reason == SkipReason::WrongOwner
                                && skipped.entity_id.as_ref().is_some_and(|id| {
                                    !frame.visible_entities.iter().any(|e| e.id == *id)
                                })
                            {
                                skipped.reason = SkipReason::Absent;
                            }
                        }
                        outcome
                    })
                    .collect();
                timeline.push(frame, &types);
            }
            cursor = sim.command_outcomes.len();
            ore = Some(current_ore);
        },
    )?;
    if let Some((tick, hash)) = expected
        && (result.outcome.terminal_state_tick != tick || result.final_hash != hash)
    {
        return Err("player timeline did not match the authoritative replay".into());
    }
    for (&tick, sample) in &mut public_stats {
        if tick != request.checkpoint.tick && tick != result.outcome.terminal_state_tick {
            for player in &mut sample.players {
                player.entity_count = None;
            }
        }
    }
    let public_stats = Arc::new(public_stats);
    let outcome = Arc::new(result.outcome);
    for timeline in ledgers.values_mut() {
        timeline.public_stats = Arc::clone(&public_stats);
        timeline.destructions = destructions;
        timeline.outcome = Some(Arc::clone(&outcome));
    }
    Ok(ledgers)
}
