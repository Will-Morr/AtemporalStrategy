//! Validate compiled orders against the authoritative pre-tick state.

use atemporal_contracts::{
    BlueprintId, BlueprintSettings, CardinalDirection, Command, Content, EntityId, EntityState,
    MatchConfig, Order, PlayerId, ProductionEdit, SiloPlan, Tile, TypeDefinition, TypeKind,
    WorldState, identity, validate_silo_plan,
};
use atemporal_sim::{placement, player_timeline};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) enum StateCheckIssue {
    /// A recipient may no longer be available when a draft is moved to another tick.
    Unavailable(String),
    Invalid(String),
}

impl StateCheckIssue {
    pub(crate) fn reason(self) -> String {
        match self {
            Self::Unavailable(reason) | Self::Invalid(reason) => reason,
        }
    }
}

impl From<String> for StateCheckIssue {
    fn from(reason: String) -> Self {
        Self::Invalid(reason)
    }
}

impl From<&str> for StateCheckIssue {
    fn from(reason: &str) -> Self {
        Self::Invalid(reason.into())
    }
}

type CheckResult<T = ()> = std::result::Result<T, StateCheckIssue>;
type KnownStructures<'a> = BTreeMap<EntityId, (&'a str, Tile, Option<Tile>)>;

struct BlueprintSequence<'a> {
    structures: Option<KnownStructures<'a>>,
    cancelled: BTreeSet<BlueprintId>,
}

pub(crate) struct OrderChecker<'a> {
    pub(crate) player: PlayerId,
    pub(crate) round: u32,
    pub(crate) state: &'a WorldState,
    pub(crate) content: &'a Content,
    pub(crate) commands: &'a [Command],
    pub(crate) reserve_factory_outputs: bool,
}

impl<'a> OrderChecker<'a> {
    pub(crate) fn check(
        &self,
        config: &MatchConfig,
        compare_client_placement: bool,
    ) -> CheckResult {
        // Hidden blockers and competing turns are resolved by the simulation.
        let structures = if (self.reserve_factory_outputs || compare_client_placement)
            && self
                .commands
                .iter()
                .any(|command| matches!(command, Command::PlaceBlueprints { .. }))
        {
            Some(self.known_structures(config)?)
        } else {
            None
        };
        let mut blueprints = BlueprintSequence {
            structures,
            cancelled: BTreeSet::new(),
        };
        for (index, command) in self.commands.iter().enumerate() {
            self.check_command(index, command, &mut blueprints)?;
        }
        Ok(())
    }

    fn check_command(
        &self,
        command_index: usize,
        command: &'a Command,
        blueprints: &mut BlueprintSequence<'a>,
    ) -> CheckResult {
        match command {
            Command::ClearFutureOrders {} => {}
            Command::AssignOrder { entities, order } => self.check_assignment(entities, order)?,
            Command::AssignGroupOrder { group, .. } | Command::EditGroupMembers { group, .. } => {
                if group.owner != self.player {
                    return Err(StateCheckIssue::Unavailable(
                        "not your control group".into(),
                    ));
                }
            }
            Command::BindFactoryGroup { factories, group } => {
                if group.as_ref().is_some_and(|g| g.owner != self.player) {
                    return Err(StateCheckIssue::Unavailable(
                        "not your control group".into(),
                    ));
                }
                for id in factories {
                    self.owned(id)?;
                }
            }
            Command::SetPriority { entities, .. } => {
                for id in entities {
                    if self.owned(id).is_err()
                        && !self
                            .state
                            .blueprints
                            .iter()
                            .any(|b| b.id == *id && b.owner == self.player)
                    {
                        return Err(StateCheckIssue::Unavailable(
                            "priority target is not yours".into(),
                        ));
                    }
                }
            }
            Command::PlaceBlueprints {
                type_key,
                tiles,
                output_directions,
                ..
            } => self.place_blueprints(
                command_index,
                type_key,
                tiles,
                output_directions.as_deref(),
                &mut blueprints.structures,
            )?,
            Command::ConfigureBlueprints {
                blueprint_ids,
                settings,
            } => {
                self.configure_blueprints(
                    command_index,
                    blueprint_ids,
                    settings,
                    &blueprints.cancelled,
                )?;
            }
            Command::CancelBlueprints { blueprint_ids } => {
                self.cancel_blueprints(command_index, blueprint_ids, blueprints)?;
            }
            Command::SetSiloPlan { silos, plan } => {
                if silos.is_empty() {
                    return Err("launch plan needs a silo".into());
                }
                for id in silos {
                    let e = self.owned(id)?;
                    let d = self.definition(&e.type_key).ok_or("unknown silo type")?;
                    self.check_silo_plan(d, plan)?;
                }
            }
            Command::EditProduction { factories, edit } => {
                self.check_production_edit(factories, edit)?
            }
            Command::SetQueueLoop { factories, .. } | Command::SetStoredOrder { factories, .. } => {
                for id in factories {
                    let e = self.owned(id)?;
                    if self
                        .definition(&e.type_key)
                        .is_none_or(|d| d.production.is_none())
                    {
                        return Err("target is not a factory".into());
                    }
                }
            }
        }
        Ok(())
    }

    fn check_assignment(&self, entities: &[EntityId], order: &Order) -> CheckResult {
        if entities.is_empty() {
            return Err("assignment without recipients".into());
        }
        for id in entities {
            let e = self.owned(id)?;
            let d = self.definition(&e.type_key).ok_or("unknown type")?;
            let ok = match order {
                Order::Idle {} => true,
                Order::AttackMove { .. } | Order::Support { .. } => d.movement.is_some(),
                Order::Mine { .. } => d.mining.is_some(),
                Order::Construct { .. } => d.construction.is_some(),
            };
            if d.silo.is_some() || (!ok && d.production.is_none()) {
                return Err(format!("{} cannot perform that order", e.type_key).into());
            }
        }
        Ok(())
    }

    fn check_production_edit(&self, factories: &[EntityId], edit: &ProductionEdit) -> CheckResult {
        for id in factories {
            let e = self.owned(id)?;
            let producer = self
                .definition(&e.type_key)
                .ok_or("target cannot produce")?;
            let recipes = producer
                .production
                .as_ref()
                .ok_or("target cannot produce")?;
            if let ProductionEdit::SupplyLaunches { plan, .. } = edit {
                self.check_silo_plan(producer, plan)?;
            }
            if let ProductionEdit::Append { items }
            | ProductionEdit::SupplyLaunches { items, .. }
            | ProductionEdit::ReplacePending { items } = edit
                && items.iter().any(|key| !recipes.recipes.contains(key))
            {
                return Err("recipe is not supported by the selected producer".into());
            }
        }
        Ok(())
    }

    fn check_silo_plan(&self, definition: &TypeDefinition, plan: &SiloPlan) -> CheckResult {
        let terrain = &self.state.terrain;
        validate_silo_plan(definition, plan, terrain.width, terrain.height)?;
        Ok(())
    }

    fn definition(&self, key: &str) -> Option<&'a TypeDefinition> {
        self.content
            .types
            .iter()
            .find(|definition| definition.key == key)
    }

    fn owned(&self, id: &EntityId) -> CheckResult<&EntityState> {
        let e = self
            .state
            .entities
            .iter()
            .find(|e| e.id == *id)
            .ok_or_else(|| {
                StateCheckIssue::Unavailable(format!(
                    "entity {} does not exist at that tick",
                    short_id(id)
                ))
            })?;
        if e.owner != self.player {
            return Err(StateCheckIssue::Unavailable(format!(
                "entity {} is not yours",
                short_id(id)
            )));
        }
        Ok(e)
    }

    fn known_structures(&self, config: &MatchConfig) -> CheckResult<KnownStructures<'a>> {
        let frame = player_timeline::project(self.state, self.player, config, self.content)?;
        let mut known = BTreeMap::new();
        for blueprint in self
            .state
            .blueprints
            .iter()
            .filter(|b| b.owner == self.player)
        {
            let output = blueprint.output_direction.and_then(|direction| {
                placement::output_tile(&self.state.terrain, blueprint.tile, direction)
            });
            known.insert(
                blueprint.id.clone(),
                (blueprint.type_key.as_str(), blueprint.tile, output),
            );
        }
        for entity in &self.state.entities {
            if (entity.owner == self.player || frame.sees(entity.tile))
                && self
                    .definition(&entity.type_key)
                    .is_some_and(|d| d.kind == TypeKind::Structure)
            {
                let output = entity
                    .production
                    .as_ref()
                    .filter(|p| p.silo.is_none())
                    .map(|p| p.output_tile);
                known.insert(
                    entity.id.clone(),
                    (entity.type_key.as_str(), entity.tile, output),
                );
            }
        }
        Ok(known)
    }

    fn place_blueprints(
        &self,
        command_index: usize,
        type_key: &'a str,
        tiles: &[Tile],
        output_directions: Option<&[CardinalDirection]>,
        structures: &mut Option<KnownStructures<'a>>,
    ) -> CheckResult {
        let d = self.definition(type_key).ok_or("unknown structure type")?;
        if d.kind != TypeKind::Structure {
            return Err("only structures can be placed".into());
        }
        if tiles.is_empty() {
            return Err("no placement tiles".into());
        }
        for (index, &tile) in tiles.iter().enumerate() {
            if !placement::in_bounds(&self.state.terrain, tile) {
                return Err("placement outside the map".into());
            }
            if !placement::is_floor(&self.state.terrain, tile) {
                return Err("cannot place on rock".into());
            }
            if let Some(known) = &mut *structures {
                let at = |target| known.values().find(|(_, tile, _)| *tile == target);
                if let Some((key, _, _)) = at(tile) {
                    return Err(format!(
                        "Cannot place {type_key} at ({}, {}). A {key} already occupies that tile.",
                        tile.x, tile.y
                    )
                    .into());
                }
                if self.reserve_factory_outputs
                    && let Some((key, origin, _)) =
                        known.values().find(|(_, _, output)| *output == Some(tile))
                {
                    let Tile { x, y } = tile;
                    let Tile { x: out_x, y: out_y } = *origin;
                    return Err(format!(
                        "Cannot place {type_key} at ({x}, {y}). It is the output of the {key} at ({out_x}, {out_y})."
                    ).into());
                }
                let output = if d.production.is_some() && d.silo.is_none() {
                    let wanted = output_directions.and_then(|dirs| dirs.get(index).copied());
                    Some(self.factory_output(known, type_key, tile, wanted)?)
                } else {
                    None
                };
                let command = identity::command_id(self.round, self.player, command_index as u32);
                known.insert(
                    identity::blueprint(&command, index as u32)?,
                    (type_key, tile, output),
                );
            } else if self.state.entities.iter().any(|e| {
                e.tile == tile
                    && self
                        .definition(&e.type_key)
                        .is_some_and(|d| d.kind == TypeKind::Structure)
            }) {
                return Err("tile already holds a structure".into());
            }
        }
        Ok(())
    }

    fn factory_output(
        &self,
        known: &KnownStructures<'_>,
        type_key: &str,
        tile: Tile,
        wanted: Option<CardinalDirection>,
    ) -> CheckResult<Tile> {
        let terrain = &self.state.terrain;
        for direction in placement::output_directions(wanted) {
            let Some(candidate) = placement::output_tile(terrain, tile, direction)
                .filter(|&out| placement::is_floor(terrain, out))
            else {
                continue;
            };
            match known.values().find(|(_, origin, _)| *origin == candidate) {
                None => return Ok(candidate),
                Some((key, _, _)) if wanted.is_some() => {
                    let Tile { x, y } = tile;
                    let Tile { x: out_x, y: out_y } = candidate;
                    return Err(format!(
                        "The output for {type_key} at ({x}, {y}) is blocked by a {key} at ({out_x}, {out_y})."
                    ).into());
                }
                Some(_) => {}
            }
        }
        Err(format!(
            "No clear output for {type_key} at ({}, {}).",
            tile.x, tile.y
        )
        .into())
    }

    fn configure_blueprints(
        &self,
        command_index: usize,
        blueprint_ids: &[BlueprintId],
        settings: &BlueprintSettings,
        cancelled: &BTreeSet<BlueprintId>,
    ) -> CheckResult {
        if settings.queue.len() > 65536 {
            return Err("too many queued recipes".into());
        }
        if !settings.queue_loop_flags.is_empty()
            && settings.queue_loop_flags.len() != settings.queue.len()
        {
            return Err("queue loop flags must match the queued recipe count".into());
        }
        for id in blueprint_ids {
            if cancelled.contains(id) {
                return Err(blueprint_unavailable());
            }
            let key = self
                .state
                .blueprints
                .iter()
                .find(|b| b.id == *id && b.owner == self.player && b.site_id.is_none())
                .map(|b| b.type_key.as_str())
                .or_else(|| self.draft_blueprint_type(id, command_index))
                .ok_or_else(blueprint_unavailable)?;
            let d = self.definition(key).ok_or("unknown structure")?;
            if let Some(plan) = &settings.silo_plan {
                self.check_silo_plan(d, plan)?;
            }
            if !settings.queue.is_empty()
                && d.production
                    .as_ref()
                    .is_none_or(|p| settings.queue.iter().any(|k| !p.recipes.contains(k)))
            {
                return Err("invalid blueprint recipe".into());
            }
        }
        Ok(())
    }

    fn cancel_blueprints(
        &self,
        command_index: usize,
        blueprint_ids: &[BlueprintId],
        blueprints: &mut BlueprintSequence<'a>,
    ) -> CheckResult {
        for id in blueprint_ids {
            let existing = self
                .state
                .blueprints
                .iter()
                .find(|b| b.id == *id && b.owner == self.player);
            if blueprints.cancelled.contains(id)
                || (existing.is_none() && self.draft_blueprint_type(id, command_index).is_none())
            {
                return Err(blueprint_unavailable());
            }
            blueprints.cancelled.insert(id.clone());
            if let Some(known) = &mut blueprints.structures {
                known.remove(id);
                if let Some(site) = existing.and_then(|b| b.site_id.as_ref()) {
                    known.remove(site);
                }
            }
        }
        Ok(())
    }

    fn draft_blueprint_type(&self, id: &BlueprintId, before_index: usize) -> Option<&'a str> {
        let birth = &id.birth_command;
        let index = birth.command.index as usize;
        if birth.command.player != self.player
            || birth.command.round != self.round
            || index >= before_index
            || birth.target_index != 0
            || id.occurrence != 0
        {
            return None;
        }
        match self.commands.get(index)? {
            Command::PlaceBlueprints {
                type_key, tiles, ..
            } if usize::from(id.item_index) < tiles.len() => Some(type_key),
            _ => None,
        }
    }
}

fn blueprint_unavailable() -> StateCheckIssue {
    StateCheckIssue::Unavailable("blueprint is not yours or does not exist".into())
}

fn short_id(id: &EntityId) -> String {
    format!(
        "r{}p{}i{}/{}#{}.{}",
        id.birth_command.command.round,
        id.birth_command.command.player,
        id.birth_command.command.index,
        id.birth_command.target_index,
        id.item_index,
        id.occurrence
    )
}
