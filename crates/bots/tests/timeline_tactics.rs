#[path = "../../sim/tests/common/mod.rs"]
mod common;

use atemporal_bots::{
    History, MatchRules, PolicyTimeline, PublishedRound, PublishedWindow, Style, TurnMarker,
    published_plan,
};
use atemporal_contracts::*;
use atemporal_sim::player_timeline::{self, PlayerTimeline};
use common::{World, tile};
use std::sync::atomic::AtomicBool;

struct NoEarlierRounds;

impl History for NoEarlierRounds {
    fn revisions(&self) -> Box<dyn Iterator<Item = Revision> + '_> {
        Box::new(std::iter::empty())
    }

    fn round(&self, _: Revision) -> Option<&PublishedRound> {
        None
    }

    fn turns(&self, _: Revision) -> Option<&[TurnMarker]> {
        None
    }

    fn analyze(&self, _: Revision) -> Result<PlayerTimeline> {
        Err("no earlier rounds".into())
    }
}

fn input(world: World, end: Tick) -> PolicyTimeline {
    input_with_window(&world, end, 0..=0)
}

fn input_with_window(
    world: &World,
    end: Tick,
    editable: std::ops::RangeInclusive<Tick>,
) -> PolicyTimeline {
    let mut request = world.request();
    request.minimum_end_tick = end;
    request.end_tick_exclusive = end;
    let timeline = player_timeline::build(&request, &[0], &AtomicBool::new(false))
        .unwrap()
        .remove(&0)
        .unwrap();
    PolicyTimeline::new(
        timeline,
        Box::new(NoEarlierRounds),
        MatchRules {
            config: world.config.clone(),
            content: world.content.clone(),
        },
        PublishedWindow {
            revision: request.revision,
            player: 0,
            editable,
        },
    )
    .unwrap()
}

fn contact_world(lanes: &[u16], factory: Tile) -> World {
    let mut world = World::new(&["........................................"; 40]);
    for unit in &mut world.content.types {
        unit.vision = if unit.key == "miner" { 6. } else { 1. };
        if unit.key == "grunt" {
            unit.weapon = None;
            let movement = unit.movement.as_mut().unwrap();
            movement.cooldown = 1;
            movement.neighbors = Neighbors::Four;
        }
    }
    world.spawn(0, "factory", 3, 5);
    world.spawn(0, "silo", 3, 8);
    world.spawn(0, "tank", 3, 6);
    world.spawn(0, "miner", 16, 5);
    world.spawn(1, "factory", factory.x, factory.y);
    for (i, &x) in lanes.iter().enumerate() {
        world.spawn_with(
            1,
            "grunt",
            x,
            13 + i as u16 / 3,
            Order::AttackMove {
                destination: tile(x, 0),
            },
        );
    }
    world
}

fn targets(draft: &TurnDraft) -> (Option<Tile>, Option<Tile>) {
    let army = draft.commands.iter().find_map(|row| {
        if let Command::AssignOrder {
            order: Order::AttackMove { destination },
            ..
        } = &row.command
        {
            Some(*destination)
        } else {
            None
        }
    });
    let missile = draft.commands.iter().find_map(|row| {
        if let Command::SetSiloPlan { plan, .. } = &row.command {
            plan.launches.first().map(|l| l.target)
        } else {
            None
        }
    });
    (army, missile)
}

fn reveal(world: &mut World, target: Tile, radius: f64, launch_tick: Tick, reveal_ticks: Tick) {
    let satellite = world
        .content
        .types
        .iter_mut()
        .find(|d| d.key == "satellite")
        .unwrap()
        .missile
        .as_mut()
        .unwrap();
    satellite.radius = radius;
    satellite.reveal_ticks = reveal_ticks;
    let silo = world
        .state
        .entities
        .iter_mut()
        .find(|e| e.type_key == "silo")
        .unwrap();
    silo.production
        .as_mut()
        .unwrap()
        .silo
        .as_mut()
        .unwrap()
        .inventory
        .push(MissileStock {
            type_key: "satellite".into(),
            count: 1,
        });
    let silo = silo.id.clone();
    world.turn(
        1,
        0,
        launch_tick,
        vec![Command::SetSiloPlan {
            silos: vec![silo],
            plan: SiloPlan {
                automatic: false,
                launches: vec![MissileLaunch {
                    type_key: "satellite".into(),
                    target,
                }],
            },
        }],
    );
}

#[test]
fn distinct_ingress_routes_rally_troops_and_aim_a_missile_without_hidden_factory_coordinates() {
    let factory = tile(16, 17);
    let a = input(contact_world(&[13, 16, 19], factory), 25);
    let b = input(contact_world(&[13, 16, 19], tile(23, 21)), 25);
    assert!(!a.timeline().frames().any(|frame| frame.sees(factory)));
    assert!(
        a.timeline()
            .frames()
            .zip(b.timeline().frames())
            .all(|(a, b)| a.visible_entities == b.visible_entities)
    );
    let baseline = published_plan(&a, Style::Standard);
    let tactical = published_plan(&a, Style::Tactical);
    let (army, missile) = targets(&tactical);
    assert!(
        army.is_some_and(|t| t.x.abs_diff(factory.x) <= 4 && t.y.abs_diff(factory.y) <= 4),
        "{army:?}"
    );
    assert!(
        missile.is_some_and(|t| t.x.abs_diff(factory.x) <= 4 && t.y.abs_diff(factory.y) <= 4),
        "{missile:?}"
    );
    assert_ne!(army, targets(&baseline).0);
    assert_eq!(tactical, published_plan(&b, Style::Tactical));
}

#[test]
fn repeated_frames_and_a_single_lane_do_not_trigger_an_inferred_strike() {
    let factory = tile(16, 17);
    let input = input(contact_world(&[16], factory), 25);
    assert!(
        input
            .timeline()
            .frames()
            .filter(|f| f.visible_entities.iter().any(|e| e.type_key == "grunt"))
            .count()
            >= 5
    );
    assert_eq!(targets(&published_plan(&input, Style::Tactical)).1, None);
}

#[test]
fn later_coverage_of_a_hypothesis_prevents_a_strike_on_the_empty_area() {
    let mut world = contact_world(&[13, 16, 19], tile(35, 23));
    reveal(&mut world, tile(16, 17), 15., 14, 2);
    let input = input(world, 40);
    assert_eq!(targets(&published_plan(&input, Style::Tactical)).1, None);
}

#[test]
fn brief_factory_sight_overrides_ingress_guess_after_vision_expires() {
    let factory = tile(30, 17);
    let mut world = contact_world(&[13, 16, 19], factory);
    reveal(&mut world, factory, 1., 12, 2);
    let input = input(world, 40);
    assert!(!input.timeline().frame(40).unwrap().sees(factory));
    assert_eq!(
        targets(&published_plan(&input, Style::Tactical)),
        (Some(factory), Some(factory))
    );
}

#[test]
fn watching_a_previously_sighted_site_be_cleared_removes_both_orders() {
    let factory = tile(30, 17);
    let mut world = contact_world(&[], tile(35, 23));
    let site = world.site(1, "factory", factory.x, factory.y, 1.);
    reveal(&mut world, factory, 1., 12, 5);
    world.turn(
        1,
        1,
        24,
        vec![Command::CancelBlueprints {
            blueprint_ids: vec![site],
        }],
    );
    let input = input(world, 40);
    assert!(!input.timeline().frame(40).unwrap().sees(factory));
    let (army, missile) = targets(&published_plan(&input, Style::Tactical));
    assert_ne!(army, Some(factory));
    assert_eq!(missile, None);
}

#[test]
fn earlier_recon_does_not_prevent_later_ingress_from_counting() {
    let factory = tile(16, 17);
    let mut world = contact_world(&[13, 16, 19], factory);
    world.state.recon.push(ReconZone {
        starts_at: 0,
        expires_at: 1,
        owner: 0,
        center: tile(16, 13),
        radius: 3.,
    });
    // Allow every unit to disappear before it starts down its approach.
    for entity in &mut world.state.entities {
        if entity.owner == 1 && entity.type_key == "grunt" {
            entity.next_move_tick = 3;
        }
    }
    let input = input(world, 25);
    assert_eq!(input.timeline().frame(0).unwrap().visible_entities.len(), 3);
    assert!(
        input
            .timeline()
            .frame(1)
            .unwrap()
            .visible_entities
            .is_empty()
    );
    assert!(
        targets(&published_plan(&input, Style::Tactical))
            .1
            .is_some()
    );
}

#[test]
fn existing_missile_coverage_prevents_an_overlapping_launch() {
    let factory = tile(31, 17);
    let mut world = contact_world(&[], factory);
    reveal(&mut world, factory, 1., 6, 2);
    let silo = world
        .state
        .entities
        .iter()
        .find(|e| e.type_key == "silo")
        .unwrap();
    world.state.missiles.push(MissileFlight {
        silo_id: silo.id.clone(),
        sequence: 5,
        owner: 0,
        type_key: "tac_nuke".into(),
        origin: tile(3, 8),
        target: tile(29, 17),
        launch_tick: 0,
        impact_tick: 30,
    });
    let input = input(world, 25);
    assert_eq!(
        targets(&published_plan(&input, Style::Tactical)),
        (Some(factory), None)
    );
}

#[test]
fn several_units_following_the_same_lane_suggest_scouting_instead_of_a_strike() {
    let factory = tile(16, 17);
    let mut world = contact_world(&[16], factory);
    for (y, delay) in [(14, 4), (15, 8)] {
        let id = world.spawn_with(
            1,
            "grunt",
            16,
            y,
            Order::AttackMove {
                destination: tile(16, 0),
            },
        );
        world.entity_mut(&id).next_move_tick = delay;
    }
    let input = input(world, 25);
    let (army, missile) = targets(&published_plan(&input, Style::Tactical));
    assert!(
        army.is_some_and(|t| t.x.abs_diff(factory.x) <= 4 && t.y.abs_diff(factory.y) <= 4),
        "{army:?}"
    );
    assert_eq!(missile, None);
}

#[test]
fn one_speculative_site_programs_only_one_missile_across_current_and_planned_silos() {
    let mut world = contact_world(&[13, 16, 19], tile(16, 17));
    world.spawn(0, "constructor", 2, 7);
    let input = input(world, 25);
    let draft = published_plan(&input, Style::Tactical);
    let (army, missile) = targets(&draft);
    let lead = missile.unwrap();
    assert_eq!(army, Some(lead));
    let mut planned_silos = 0;
    let mut launches_near_lead = 0;
    for row in &draft.commands {
        let plan = match &row.command {
            Command::SetSiloPlan { plan, .. } => plan,
            Command::ConfigureBlueprints { settings, .. } => {
                let Some(plan) = &settings.silo_plan else {
                    continue;
                };
                planned_silos += 1;
                plan
            }
            _ => continue,
        };
        launches_near_lead += plan
            .launches
            .iter()
            .filter(|launch| {
                let dx = i32::from(lead.x.abs_diff(launch.target.x));
                let dy = i32::from(lead.y.abs_diff(launch.target.y));
                dx * dx + dy * dy <= 16
            })
            .count();
    }
    assert!(planned_silos > 0);
    assert_eq!(launches_near_lead, 1);
}

#[test]
fn old_ingress_stays_on_the_scouting_map_but_no_longer_prompts_a_strike() {
    let factory = tile(16, 17);
    let input = input(contact_world(&[13, 16, 19], factory), 650);
    let (army, missile) = targets(&published_plan(&input, Style::Tactical));
    assert!(
        army.is_some_and(|t| t.x.abs_diff(factory.x) <= 4 && t.y.abs_diff(factory.y) <= 4),
        "{army:?}"
    );
    assert_eq!(missile, None);
}

#[test]
fn observed_factory_can_be_targeted_before_the_silo_finishes() {
    let enemy_factory = tile(16, 17);
    for funded in [false, true] {
        let mut world = contact_world(&[], enemy_factory);
        let initial_silo = world
            .state
            .entities
            .iter()
            .position(|e| e.type_key == "silo")
            .unwrap();
        let production = world.state.entities.remove(initial_silo).production;
        let id = world.site(0, "silo", 3, 8, 20.);
        if funded {
            world.state.entities.last_mut().unwrap().production = production;
        } else {
            world.state.entities.pop();
            world.state.blueprints.last_mut().unwrap().site_id = None;
        }
        world
            .state
            .entities
            .iter_mut()
            .find(|e| e.type_key == "miner")
            .unwrap()
            .tile = tile(16, 12);
        let draft = published_plan(&input(world, 25), Style::Tactical);
        assert!(draft.commands.iter().any(|row| {
            let plan = match &row.command {
                Command::SetSiloPlan { silos, plan } if funded && silos.contains(&id) => Some(plan),
                Command::ConfigureBlueprints {
                    blueprint_ids,
                    settings,
                } if !funded
                    && blueprint_ids.contains(&DraftItemRef::Persistent { id: id.clone() }) =>
                {
                    settings.silo_plan.as_ref()
                }
                _ => None,
            };
            plan.is_some_and(|plan| {
                plan.launches
                    .iter()
                    .any(|shot| shot.type_key == "tac_nuke" && shot.target == enemy_factory)
            })
        }));
    }
}

#[test]
fn tactical_keeps_artillery_outside_observed_turret_range() {
    let mut world = World::new(&["........................"; 24]);
    world.spawn(0, "factory", 5, 5);
    world.spawn(0, "turret", 6, 5);
    world.spawn(0, "constructor", 4, 5);
    let siege = world.spawn(0, "artillery", 4, 7);
    world.spawn_with(
        0,
        "scout",
        4,
        8,
        Order::AttackMove {
            destination: tile(15, 8),
        },
    );
    world.spawn(1, "turret", 15, 5);
    world.spawn(1, "turret", 15, 7);
    let pressure = |tile: Tile, gun: Tile| {
        (i32::from(tile.x) - i32::from(gun.x)).pow(2)
            + (i32::from(tile.y) - i32::from(gun.y)).pow(2)
    };
    let input = input_with_window(&world, 80, 10..=10);
    let plan = published_plan(&input, Style::Tactical);
    let destination = plan
        .commands
        .iter()
        .find_map(|entry| match &entry.command {
            Command::AssignOrder {
                entities,
                order: Order::AttackMove { destination },
                ..
            } if entities.contains(&siege) => Some(*destination),
            _ => None,
        })
        .expect("a siege rally");
    assert!(
        [tile(15, 5), tile(15, 7)]
            .iter()
            .all(|&gun| pressure(destination, gun) > 49),
        "destination: {destination:?}"
    );
    assert!(
        [tile(15, 5), tile(15, 7)]
            .iter()
            .any(|&gun| pressure(destination, gun) <= 64),
        "destination: {destination:?}"
    );
    let turrets_left = |state: &WorldState| {
        state
            .entities
            .iter()
            .filter(|entity| entity.owner == 1 && entity.type_key == "turret")
            .count()
    };
    assert_eq!(turrets_left(&world.at(220)), 2);
    let commands = draft::resolve_local_references(&plan, 1, 0).unwrap();
    world.turn(1, 0, plan.tick, commands);
    assert!(turrets_left(&world.at(220)) < 2);
}
