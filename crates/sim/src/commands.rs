//! Canonical command application at the start of a tick: round, precedence rank, then index.
use crate::world::{Sim, cardinal_offset};
use crate::*;

impl Sim {
    pub(crate) fn apply_commands(&mut self) -> Result<()> {
        let tick = self.state.tick;
        for e in &mut self.state.entities {
            locks::canonicalize(&mut e.order_locks, tick)?;
        }
        for g in &mut self.state.control_groups {
            locks::canonicalize(&mut g.order_locks, tick)?;
        }
        let mut due: Vec<(u32, u8, AcceptedTurn)> = self
            .events
            .iter()
            .filter(|t| t.tick == tick)
            .map(|t| {
                let rank = self
                    .precedence
                    .iter()
                    .find(|p| p.round == t.round)
                    .and_then(|p| p.players.iter().position(|p| *p == t.player))
                    .unwrap_or(usize::from(t.player)) as u8;
                (t.round, rank, t.clone())
            })
            .collect();
        due.sort_by_key(|(round, rank, _)| (*round, *rank));
        for (round, _, turn) in due {
            for committed in &turn.commands {
                let outcome = self.apply(round, turn.player, committed)?;
                self.command_outcomes.push(outcome);
            }
        }
        Ok(())
    }

    fn skip(&self, entity: Option<EntityId>, reason: SkipReason) -> SkippedTarget {
        SkippedTarget {
            entity_id: entity,
            reason,
        }
    }

    /// Ownership/lifecycle/lock checks shared by assignments and entity-directed settings.
    fn target_check(
        &self,
        id: &EntityId,
        player: PlayerId,
        round: u32,
        settings_only: bool,
    ) -> std::result::Result<usize, SkipReason> {
        let Some(i) = self.find(id) else {
            return Err(SkipReason::Absent);
        };
        let e = &self.state.entities[i];
        if e.owner != player {
            return Err(SkipReason::WrongOwner);
        }
        if e.lifecycle == Lifecycle::Site && !settings_only && e.production.is_none() {
            return Err(SkipReason::Incompatible);
        }
        if locks::blocks(&e.order_locks, self.state.tick, round) {
            return Err(SkipReason::LockedByLaterRound);
        }
        Ok(i)
    }

    fn order_compatible(&self, i: usize, order: &Order) -> bool {
        let def = self.def(i);
        if def.production.is_some() {
            return true;
        }
        match order {
            Order::Idle {} => true,
            Order::AttackMove { .. } | Order::Support { .. } => def.movement.is_some(),
            Order::Mine { .. } => def.mining.is_some() && def.movement.is_some(),
            Order::Construct { .. } => def.construction.is_some() && def.movement.is_some(),
        }
    }

    /// Assign one order to an entity that passed checks; installs the policy lock on success.
    pub(crate) fn assign(
        &mut self,
        i: usize,
        order: &Order,
        round: u32,
        policy: FutureOrderPolicy,
        inherited: &[OrderLock],
    ) -> Result<()> {
        let tick = self.state.tick;
        let (window, max_tick) = (self.config.future_orders.window_ticks, self.config.max_tick);
        let e = &mut self.state.entities[i];
        if let Some(p) = &mut e.production {
            p.stored_order = order.clone();
        } else {
            e.action = order.clone();
        }
        e.engaged_target = None;
        e.support_target = match order {
            Order::Support { target } => Some(target.clone()),
            _ => None,
        };
        e.resolved_destination = None;
        e.goal_settled = false;
        e.failed_move_attempts = 0;
        e.blocked_step = None;
        e.local_detour.clear();
        if round > 0 {
            locks::install(&mut e.order_locks, tick, round, policy, window, max_tick)?;
        }
        if !inherited.is_empty() {
            e.order_locks.extend(inherited.iter().cloned());
            locks::canonicalize(&mut e.order_locks, tick)?;
        }
        Ok(())
    }

    fn deliver(
        &mut self,
        ids: &[EntityId],
        order: &Order,
        player: PlayerId,
        round: u32,
        policy: FutureOrderPolicy,
        outcome: &mut CommandOutcome,
    ) -> Result<()> {
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        for id in ids {
            match self.target_check(&id, player, round, false) {
                Err(reason) => outcome.skipped.push(self.skip(Some(id), reason)),
                Ok(i) => {
                    if !self.order_compatible(i, order) {
                        outcome
                            .skipped
                            .push(self.skip(Some(id), SkipReason::Incompatible));
                        continue;
                    }
                    if let Order::Support { target } = order
                        && (self.find(target).is_none() || target == &id)
                    {
                        outcome
                            .skipped
                            .push(self.skip(Some(id), SkipReason::MissingSupportTarget));
                        continue;
                    }
                    self.assign(i, order, round, policy, &[])?;
                    outcome.applied_entities.push(id);
                }
            }
        }
        Ok(())
    }

    fn group_index(&self, group: &ControlGroupId) -> Option<usize> {
        self.state
            .control_groups
            .iter()
            .position(|g| g.id == *group)
    }

    fn apply(
        &mut self,
        round: u32,
        player: PlayerId,
        committed: &CommittedCommand,
    ) -> Result<CommandOutcome> {
        let tick = self.state.tick;
        let mut outcome = CommandOutcome {
            command_id: committed.id.clone(),
            applied_entities: vec![],
            skipped: vec![],
        };
        let policy = committed.future_orders;
        match &committed.command {
            Command::AssignOrder { entities, order } => {
                self.deliver(entities, order, player, round, policy, &mut outcome)?;
            }
            Command::AssignGroupOrder { group, order } => {
                let Some(g) = self.group_index(group).filter(|_| group.owner == player) else {
                    outcome
                        .skipped
                        .push(self.skip(None, SkipReason::WrongOwner));
                    return Ok(outcome);
                };
                if locks::blocks(&self.state.control_groups[g].order_locks, tick, round) {
                    outcome
                        .skipped
                        .push(self.skip(None, SkipReason::LockedByLaterRound));
                    return Ok(outcome);
                }
                let (window, max_tick) =
                    (self.config.future_orders.window_ticks, self.config.max_tick);
                let slot = &mut self.state.control_groups[g];
                slot.latest_order = Some(SavedOrder {
                    source_command_id: committed.id.clone(),
                    tick,
                    order: order.clone(),
                });
                if round > 0 {
                    locks::install(&mut slot.order_locks, tick, round, policy, window, max_tick)?;
                }
                let members = slot.members.clone();
                self.deliver(&members, order, player, round, policy, &mut outcome)?;
                // Dead members are ignored rather than reported for a group delivery.
                outcome.skipped.retain(|s| s.reason != SkipReason::Absent);
            }
            Command::EditGroupMembers { group, edit } => {
                let Some(g) = self.group_index(group).filter(|_| group.owner == player) else {
                    outcome
                        .skipped
                        .push(self.skip(None, SkipReason::WrongOwner));
                    return Ok(outcome);
                };
                let (list, replace) = match edit {
                    MemberEdit::Replace { entities } => (entities, true),
                    MemberEdit::Add { entities } => (entities, false),
                    MemberEdit::Remove { entities } => {
                        let slot = &mut self.state.control_groups[g];
                        slot.members.retain(|m| !entities.contains(m));
                        outcome.applied_entities = entities.clone();
                        return Ok(outcome);
                    }
                };
                let mut accepted = vec![];
                for id in list {
                    match self.find(id) {
                        Some(i) if self.state.entities[i].owner == player => {
                            accepted.push(id.clone())
                        }
                        Some(_) => outcome
                            .skipped
                            .push(self.skip(Some(id.clone()), SkipReason::WrongOwner)),
                        None => outcome
                            .skipped
                            .push(self.skip(Some(id.clone()), SkipReason::Absent)),
                    }
                }
                let slot = &mut self.state.control_groups[g];
                if replace {
                    slot.members.clear();
                }
                slot.members.extend(accepted.iter().cloned());
                slot.members.sort();
                slot.members.dedup();
                outcome.applied_entities = accepted;
            }
            Command::BindFactoryGroup { factories, group } => {
                if group.as_ref().is_some_and(|g| g.owner != player) {
                    outcome
                        .skipped
                        .push(self.skip(None, SkipReason::WrongOwner));
                    return Ok(outcome);
                }
                for id in factories {
                    match self.target_check(id, player, round, true) {
                        Err(reason) => outcome.skipped.push(self.skip(Some(id.clone()), reason)),
                        Ok(i) => match &mut self.state.entities[i].production {
                            Some(p) => {
                                p.spawn_group = group.clone();
                                outcome.applied_entities.push(id.clone());
                            }
                            None => outcome
                                .skipped
                                .push(self.skip(Some(id.clone()), SkipReason::Incompatible)),
                        },
                    }
                }
            }
            Command::SetPriority { entities, priority } => {
                for id in entities {
                    match self.target_check(id, player, round, true) {
                        Err(reason) => outcome.skipped.push(self.skip(Some(id.clone()), reason)),
                        Ok(i) => {
                            self.state.entities[i].priority = *priority;
                            outcome.applied_entities.push(id.clone());
                        }
                    }
                }
                for b in &mut self.state.blueprints {
                    if entities.contains(&b.id) && b.owner == player && b.site_id.is_none() {
                        b.priority = *priority;
                    }
                }
            }
            Command::PlaceBlueprints {
                type_key,
                tiles,
                priority,
                output_directions,
            } => {
                let Ok(ty) = self.type_index(type_key) else {
                    outcome
                        .skipped
                        .push(self.skip(None, SkipReason::Incompatible));
                    return Ok(outcome);
                };
                let def = &self.content.types[ty];
                if def.kind != TypeKind::Structure {
                    outcome
                        .skipped
                        .push(self.skip(None, SkipReason::Incompatible));
                    return Ok(outcome);
                }
                let produces = def.production.is_some();
                let rank = self
                    .precedence
                    .iter()
                    .find(|p| p.round == round)
                    .and_then(|p| p.players.iter().position(|p| *p == player))
                    .unwrap_or(usize::from(player)) as u8;
                for (index, tile) in tiles.iter().enumerate() {
                    let id = identity::blueprint(&committed.id, index as u32)?;
                    let legal = self.in_bounds(i32::from(tile.x), i32::from(tile.y))
                        && self.floor(*tile)
                        && !self
                            .state
                            .blueprints
                            .iter()
                            .any(|b| b.tile == *tile && b.owner == player)
                        && self.traversable(*tile);
                    if !legal {
                        outcome
                            .skipped
                            .push(self.skip(Some(id), SkipReason::Blocked));
                        continue;
                    }
                    let mut output = None;
                    if produces {
                        let wanted = output_directions.as_ref().map(|d| d[index]);
                        let candidates: Vec<CardinalDirection> = match wanted {
                            Some(d) => vec![d],
                            None => vec![
                                CardinalDirection::N,
                                CardinalDirection::E,
                                CardinalDirection::S,
                                CardinalDirection::W,
                            ],
                        };
                        output = candidates.into_iter().find(|d| {
                            let (dx, dy) = cardinal_offset(*d);
                            self.offset(*tile, dx, dy)
                                .is_some_and(|t| self.traversable(t))
                        });
                        if output.is_none() {
                            outcome
                                .skipped
                                .push(self.skip(Some(id), SkipReason::Blocked));
                            continue;
                        }
                    }
                    self.state.blueprints.push(Blueprint {
                        id: id.clone(),
                        owner: player,
                        type_key: type_key.clone(),
                        tile: *tile,
                        priority: *priority,
                        source_command_id: committed.id.clone(),
                        precedence: EventKey {
                            tick,
                            round,
                            player_rank: rank,
                            command_index: committed.id.index,
                        },
                        site_id: None,
                        settings: None,
                        settings_command: None,
                        output_direction: output,
                    });
                    outcome.applied_entities.push(id);
                }
                self.state.blueprints.sort_by(|a, b| a.id.cmp(&b.id));
            }
            Command::ConfigureBlueprints {
                blueprint_ids,
                settings,
            } => {
                let mut ids = blueprint_ids.clone();
                ids.sort();
                ids.dedup();
                for (target_index, id) in ids.iter().enumerate() {
                    let Some(b) = self
                        .state
                        .blueprints
                        .iter()
                        .position(|b| b.id == *id && b.owner == player && b.site_id.is_none())
                    else {
                        outcome
                            .skipped
                            .push(self.skip(Some(id.clone()), SkipReason::Absent));
                        continue;
                    };
                    let ty = self.type_index(&self.state.blueprints[b].type_key)?;
                    let def = &self.content.types[ty];
                    if !settings.queue.is_empty()
                        && def
                            .production
                            .as_ref()
                            .is_none_or(|p| settings.queue.iter().any(|k| !p.recipes.contains(k)))
                    {
                        outcome
                            .skipped
                            .push(self.skip(Some(id.clone()), SkipReason::Incompatible));
                        continue;
                    }
                    self.state.blueprints[b].priority = settings.priority;
                    self.state.blueprints[b].settings = Some(settings.clone());
                    self.state.blueprints[b].settings_command = Some(BirthCommandId {
                        command: committed.id.clone(),
                        target_index: u16::try_from(target_index)
                            .map_err(|_| "too many blueprint targets")?,
                    });
                    outcome.applied_entities.push(id.clone());
                }
            }
            Command::CancelBlueprints { blueprint_ids } => {
                for id in blueprint_ids {
                    let Some(pos) = self
                        .state
                        .blueprints
                        .iter()
                        .position(|b| b.id == *id && b.owner == player)
                    else {
                        outcome
                            .skipped
                            .push(self.skip(Some(id.clone()), SkipReason::Absent));
                        continue;
                    };
                    let site = self.state.blueprints[pos].site_id.clone();
                    self.state.blueprints.remove(pos);
                    if let Some(site) = site
                        && let Some(i) = self.find(&site)
                    {
                        let paid = self.state.entities[i].paid_matter;
                        self.state.players[usize::from(player)]
                            .counters
                            .lost_invested_matter += paid;
                        let tile = self.state.entities[i].tile;
                        self.state.entities.remove(i);
                        self.structure_version += 1;
                        self.occupancy_changed_tick = self.state.tick;
                        self.reindex()?;
                        self.push_event(PresentationEvent::Destroyed {
                            entity_id: site,
                            tile,
                            visual_style: VisualStyle::Direct,
                        });
                    }
                    outcome.applied_entities.push(id.clone());
                }
            }
            Command::EditProduction { factories, edit } => {
                let mut targets = factories.clone();
                targets.sort();
                targets.dedup();
                for (target_index, id) in targets.iter().enumerate() {
                    let i = match self.target_check(id, player, round, true) {
                        Err(reason) => {
                            outcome.skipped.push(self.skip(Some(id.clone()), reason));
                            continue;
                        }
                        Ok(i) => i,
                    };
                    let recipes = self.def(i).production.as_ref().map(|p| p.recipes.clone());
                    let Some(p) = self.state.entities[i].production.as_mut() else {
                        outcome
                            .skipped
                            .push(self.skip(Some(id.clone()), SkipReason::Incompatible));
                        continue;
                    };
                    let recipes = recipes.unwrap_or_default();
                    let make = |items: &[TypeKey]| -> Result<Vec<QueueItem>> {
                        let mut out = vec![];
                        for (item_index, key) in items.iter().enumerate() {
                            if !recipes.contains(key) {
                                return Err(format!("recipe {key} is not producible"));
                            }
                            out.push(QueueItem {
                                item_id: identity::queue_item(
                                    &committed.id,
                                    target_index as u32,
                                    item_index as u32,
                                )?,
                                type_key: key.clone(),
                            });
                        }
                        Ok(out)
                    };
                    match edit {
                        ProductionEdit::Append { items } => match make(items) {
                            Ok(items) => p.pending_items.extend(items),
                            Err(_) => {
                                outcome
                                    .skipped
                                    .push(self.skip(Some(id.clone()), SkipReason::Incompatible));
                                continue;
                            }
                        },
                        ProductionEdit::ReplacePending { items } => match make(items) {
                            Ok(items) => p.pending_items = items,
                            Err(_) => {
                                outcome
                                    .skipped
                                    .push(self.skip(Some(id.clone()), SkipReason::Incompatible));
                                continue;
                            }
                        },
                        ProductionEdit::RemovePending { item_ids } => {
                            p.pending_items.retain(|q| !item_ids.contains(&q.item_id));
                        }
                        ProductionEdit::CancelActive {} => {
                            if let Some(active) = p.active_item.take() {
                                self.state.players[usize::from(player)]
                                    .counters
                                    .lost_invested_matter += active.paid_matter;
                            }
                        }
                    }
                    outcome.applied_entities.push(id.clone());
                }
            }
            Command::SetQueueLoop { factories, enabled } => {
                for id in factories {
                    match self.target_check(id, player, round, true) {
                        Err(reason) => outcome.skipped.push(self.skip(Some(id.clone()), reason)),
                        Ok(i) => match &mut self.state.entities[i].production {
                            Some(p) => {
                                p.loop_enabled = *enabled;
                                outcome.applied_entities.push(id.clone());
                            }
                            None => outcome
                                .skipped
                                .push(self.skip(Some(id.clone()), SkipReason::Incompatible)),
                        },
                    }
                }
            }
            Command::SetStoredOrder { factories, order } => {
                for id in factories {
                    match self.target_check(id, player, round, true) {
                        Err(reason) => outcome.skipped.push(self.skip(Some(id.clone()), reason)),
                        Ok(i) => match &mut self.state.entities[i].production {
                            Some(p) => {
                                p.stored_order = order.clone();
                                outcome.applied_entities.push(id.clone());
                            }
                            None => outcome
                                .skipped
                                .push(self.skip(Some(id.clone()), SkipReason::Incompatible)),
                        },
                    }
                }
            }
        }
        Ok(outcome)
    }
}
