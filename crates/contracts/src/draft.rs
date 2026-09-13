//! Structural draft checks and local-reference resolution. Exact-state projection remains server work.
use crate::{identity, *};
use std::collections::BTreeMap;

pub fn validate_policy(command: &DraftCommand, window: Option<Tick>) -> Result<()> {
    if command.future_orders != FutureOrderPolicy::Keep {
        if !matches!(
            command.command,
            Command::AssignOrder { .. } | Command::AssignGroupOrder { .. }
        ) {
            return Err("only action assignments may remove future orders".into());
        }
        if command.future_orders == FutureOrderPolicy::DropWindow && window.is_none_or(|w| w == 0) {
            return Err("drop_window requires a positive configured window".into());
        }
    }
    Ok(())
}
/// Interval excludes t and includes the returned end. None means Keep or no future ticks.
pub fn removal_interval(
    tick: Tick,
    policy: FutureOrderPolicy,
    window: Option<Tick>,
    max_tick: Tick,
) -> Result<Option<PreviewInterval>> {
    if tick >= max_tick {
        return Err("draft tick is outside horizon".into());
    }
    let through_tick = match policy {
        FutureOrderPolicy::Keep => return Ok(None),
        FutureOrderPolicy::DropAll => max_tick - 1,
        FutureOrderPolicy::DropWindow => tick
            .saturating_add(
                window
                    .filter(|w| *w > 0)
                    .ok_or("drop_window requires a positive configured window")?,
            )
            .min(max_tick - 1),
    };
    Ok((through_tick > tick).then_some(PreviewInterval {
        after_tick: tick,
        through_tick,
    }))
}
/// Resolves earlier blueprint/queue causes only; it never creates predicted entities or ticks.
/// Registry writes are atomic on success. Server still validates projected ownership/capabilities,
/// selects output directions, and resolves suppression against the common base revision.
pub fn resolve_local_references(
    draft: &TurnDraft,
    round: u32,
    player: PlayerId,
    registry: &mut IdentityRegistry,
) -> Result<Vec<Command>> {
    let mut registry_candidate = registry.clone();
    let mut earlier = BTreeMap::<String, (u32, &Command<DraftItemRef>)>::new();
    let mut resolved = vec![];
    for (index, entry) in draft.commands.iter().enumerate() {
        if entry.local_id.is_empty()
            || entry.local_id.len() > 64
            || earlier.contains_key(&entry.local_id)
        {
            return Err("draft local IDs must be nonempty, unique and at most 64 bytes".into());
        }
        let index = u32::try_from(index).map_err(|_| "too many draft commands")?;
        let blueprint_ref =
            |reference: &DraftItemRef, registry: &mut IdentityRegistry| -> Result<String> {
                match reference {
                    DraftItemRef::Persistent { id } => Ok(id.clone()),
                    DraftItemRef::Draft {
                        local_id,
                        item_index,
                    } => {
                        let (source_index, source) = earlier
                            .get(local_id)
                            .ok_or("unknown, forward or cyclic draft reference")?;
                        let Command::PlaceBlueprints { tiles, .. } = source else {
                            return Err("blueprint reference must point to placement".into());
                        };
                        if *item_index as usize >= tiles.len() {
                            return Err("blueprint item_index out of range".into());
                        }
                        identity::blueprint(
                            registry,
                            &identity::command_id(round, player, *source_index),
                            *item_index,
                        )
                    }
                }
            };
        let queue_refs = |references: &[DraftItemRef],
                          factories: &[EntityId],
                          registry: &mut IdentityRegistry|
         -> Result<Vec<String>> {
            let mut ids = vec![];
            for reference in references {
                match reference {
                    DraftItemRef::Persistent { id } => ids.push(id.clone()),
                    DraftItemRef::Draft {
                        local_id,
                        item_index,
                    } => {
                        let (source_index, source) = earlier
                            .get(local_id)
                            .ok_or("unknown, forward or cyclic draft reference")?;
                        let Command::EditProduction {
                            factories: source_factories,
                            edit,
                        } = source
                        else {
                            return Err("queue reference must point to a queue edit".into());
                        };
                        let (ProductionEdit::Append { items }
                        | ProductionEdit::ReplacePending { items }) = edit
                        else {
                            return Err("queue reference must point to new items".into());
                        };
                        if *item_index as usize >= items.len() {
                            return Err("queue item_index out of range".into());
                        }
                        for factory in factories {
                            if !source_factories.contains(factory) {
                                return Err(
                                    "referenced queue edit did not target this factory".into()
                                );
                            }
                            ids.push(identity::queue_item(
                                registry,
                                factory,
                                &identity::command_id(round, player, *source_index),
                                *item_index,
                            )?);
                        }
                    }
                }
            }
            ids.sort();
            ids.dedup();
            Ok(ids)
        };
        let command = match &entry.command {
            Command::AssignOrder { entities, order } => Command::AssignOrder {
                entities: entities.clone(),
                order: order.clone(),
            },
            Command::AssignGroupOrder { group, order } => Command::AssignGroupOrder {
                group: group.clone(),
                order: order.clone(),
            },
            Command::EditGroupMembers { group, edit } => Command::EditGroupMembers {
                group: group.clone(),
                edit: edit.clone(),
            },
            Command::BindFactoryGroup { factories, group } => Command::BindFactoryGroup {
                factories: factories.clone(),
                group: group.clone(),
            },
            Command::SetPriority { entities, priority } => Command::SetPriority {
                entities: entities.clone(),
                priority: *priority,
            },
            Command::PlaceBlueprints {
                type_key,
                tiles,
                priority,
                output_directions,
            } => {
                if output_directions
                    .as_ref()
                    .is_some_and(|dirs| dirs.len() != tiles.len())
                {
                    return Err("output directions must correspond to every placement tile".into());
                }
                Command::PlaceBlueprints {
                    type_key: type_key.clone(),
                    tiles: tiles.clone(),
                    priority: *priority,
                    output_directions: output_directions.clone(),
                }
            }
            Command::CancelBlueprints { blueprint_ids } => Command::CancelBlueprints {
                blueprint_ids: blueprint_ids
                    .iter()
                    .map(|r| blueprint_ref(r, &mut registry_candidate))
                    .collect::<Result<_>>()?,
            },
            Command::EditProduction { factories, edit } => Command::EditProduction {
                factories: factories.clone(),
                edit: match edit {
                    ProductionEdit::Append { items } => ProductionEdit::Append {
                        items: items.clone(),
                    },
                    ProductionEdit::ReplacePending { items } => ProductionEdit::ReplacePending {
                        items: items.clone(),
                    },
                    ProductionEdit::RemovePending { item_ids } => ProductionEdit::RemovePending {
                        item_ids: queue_refs(item_ids, factories, &mut registry_candidate)?,
                    },
                    ProductionEdit::CancelActive {} => ProductionEdit::CancelActive {},
                },
            },
            Command::SetQueueLoop { factories, enabled } => Command::SetQueueLoop {
                factories: factories.clone(),
                enabled: *enabled,
            },
            Command::SetStoredOrder { factories, order } => Command::SetStoredOrder {
                factories: factories.clone(),
                order: order.clone(),
            },
        };
        earlier.insert(entry.local_id.clone(), (index, &entry.command));
        resolved.push(command);
    }
    *registry = registry_candidate;
    Ok(resolved)
}
