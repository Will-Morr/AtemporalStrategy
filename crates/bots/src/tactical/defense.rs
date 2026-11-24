//! Give an existing attack group a firing position outside observed turret coverage.
use super::{PlayerFrame, PolicyTimeline, allied, append, distance, route_lengths};
use atemporal_contracts::{Command, ControlLimit, Order, TurnDraft};
use std::collections::BTreeSet;

pub(super) fn redirect(input: &PolicyTimeline, view: &PlayerFrame, draft: &mut TurnDraft) {
    if input.config().control_limit == ControlLimit::SingleOrder {
        return;
    }
    let Some(artillery) = input
        .content()
        .types
        .iter()
        .find(|def| def.key == "artillery")
    else {
        return;
    };
    let Some(gun_range) = input
        .content()
        .types
        .iter()
        .find(|def| def.key == "turret")
        .and_then(|def| def.weapon.as_ref())
        .map(|weapon| weapon.range.powi(2))
    else {
        return;
    };
    let Some(vision) = artillery
        .weapon
        .as_ref()
        .map(|weapon| artillery.vision.min(weapon.range).powi(2))
    else {
        return;
    };
    if vision <= gun_range {
        return;
    }

    let through = view.tick.saturating_add(160);
    let first = view.tick.saturating_sub(120);
    let mut guns = BTreeSet::new();
    for frame in input
        .timeline
        .frames()
        .skip_while(|frame| frame.tick < first)
        .step_by(8)
        .take_while(|frame| frame.tick <= through && !input.cancelled())
    {
        guns.retain(|tile| !frame.sees(*tile));
        guns.extend(
            frame
                .visible_entities
                .iter()
                .filter(|e| {
                    e.type_key == "turret" && !allied(input.config(), input.player(), e.owner)
                })
                .map(|e| e.tile),
        );
    }
    if guns.len() < 2 {
        return;
    }
    let safe = |tile| {
        guns.iter()
            .all(|&gun| f64::from(distance(tile, gun)) > gun_range)
    };
    let mut redirects = vec![];
    for entry in &mut draft.commands {
        let Command::AssignOrder {
            entities,
            order: Order::AttackMove { destination },
        } = &mut entry.command
        else {
            continue;
        };
        if safe(*destination) {
            continue;
        }
        let Some(siege) = view
            .own_entities
            .iter()
            .filter(|entity| entity.type_key == "artillery" && entities.contains(&entity.id))
            .min_by_key(|entity| distance(entity.tile, *destination))
        else {
            continue;
        };
        let Some(firing_position) = route_lengths(&input.timeline.terrain, siege.tile)
            .into_iter()
            .filter(|&(tile, _)| {
                safe(tile)
                    && guns.iter().any(|&gun| {
                        f64::from(distance(tile, gun)) <= vision
                            && f64::from(distance(*destination, gun)) <= gun_range
                    })
            })
            .min_by_key(|&(tile, steps)| (steps, distance(tile, *destination), tile))
            .map(|(tile, _)| tile)
        else {
            continue;
        };
        let artillery: Vec<_> = view
            .own_entities
            .iter()
            .filter(|entity| entity.type_key == "artillery" && entities.contains(&entity.id))
            .map(|entity| entity.id.clone())
            .collect();
        if artillery.len() == entities.len() {
            *destination = firing_position;
            continue;
        }
        entities.retain(|id| !artillery.contains(id));
        redirects.push(Command::AssignOrder {
            entities: artillery,
            order: Order::AttackMove {
                destination: firing_position,
            },
        });
        if let Some(turret) = view
            .own_entities
            .iter()
            .filter(|entity| {
                entity.owner == input.player() && entity.type_key == "turret" && safe(entity.tile)
            })
            .min_by_key(|entity| distance(entity.tile, firing_position))
        {
            *destination = turret.tile;
        }
    }
    for command in redirects {
        append(&mut draft.commands, command);
    }
}
