//! Equal-purse combat trials using the production engine and the small-world test builder.
#[path = "../tests/common/mod.rs"]
mod common;
use atemporal_sim::*;
use common::*;

const PURSE: f64 = 600.;
const RECIPES: &[(&str, &[&str])] = &[
    ("grunt", &["grunt"]),
    ("tank", &["tank"]),
    ("grinder", &["grinder"]),
    ("artillery", &["artillery"]),
    ("screen", &["grunt", "grunt", "artillery"]),
    ("heavy", &["tank", "artillery"]),
    ("spotter", &["scout", "artillery"]),
];

fn arena(choke: bool, content: Content) -> World {
    let mut rows = vec![".".repeat(31); 15];
    if choke {
        for (y, row) in rows.iter_mut().enumerate() {
            if !(6..=8).contains(&y) {
                row.replace_range(14..17, "###");
            }
        }
    }
    let mut w = World::with_content(
        &rows.iter().map(String::as_str).collect::<Vec<_>>(),
        content,
    );
    w.config.max_tick = 900;
    w.config.snapshot_interval = 100;
    w.config.checkpoint_interval = 100;
    w
}

fn deploy(w: &mut World, p: PlayerId, recipe: &[&str], production: bool) {
    let mirror = |x| if p == 0 { x } else { 30 - x };
    let target = attack_move(mirror(28), 7);
    if production {
        w.bank(p, PURSE);
        let id = w.spawn(p, "factory", mirror(2), 7);
        let f = w.entity_mut(&id).production.as_mut().unwrap();
        f.output_tile = tile(mirror(3), 7);
        f.output_direction = if p == 0 {
            CardinalDirection::E
        } else {
            CardinalDirection::W
        };
        w.turn(
            1,
            p,
            0,
            vec![
                Command::SetStoredOrder {
                    factories: vec![id.clone()],
                    order: target,
                },
                Command::SetQueueLoop {
                    factories: vec![id.clone()],
                    enabled: true,
                },
                Command::EditProduction {
                    factories: vec![id],
                    edit: ProductionEdit::Append {
                        items: recipe.iter().map(|s| s.to_string()).collect(),
                    },
                },
            ],
        );
    } else {
        let mut funds = PURSE;
        for (i, key) in recipe.iter().cycle().enumerate() {
            let cost = w.def(key).matter_cost;
            if funds < cost {
                break;
            }
            funds -= cost;
            let y = [7, 6, 8, 5, 9, 4, 10, 3, 11, 2, 12, 1, 13, 0, 14][i % 15];
            w.spawn_with(p, key, mirror(5 - (i / 15) as u16), y, target.clone());
        }
        w.bank(p, funds);
    }
}

fn main() {
    let mut content = load_content(include_str!("../../../config/content.yaml")).unwrap();
    if let Some(value) = std::env::args().nth(1) {
        content
            .types
            .iter_mut()
            .find(|d| d.key == "artillery")
            .unwrap()
            .vision = value.parse().expect("artillery vision");
    }
    if std::env::args().nth(2).as_deref() == Some("siege") {
        println!(
            "mode,arena,recipe,all_turrets_destroyed,out_of,mean_turret_losses,mean_attack_losses"
        );
        for queue in [false, true] {
            for choke in [false, true] {
                for (name, recipe) in RECIPES {
                    let mut wins = 0;
                    let mut losses = [0.; 2];
                    for side in 0..2 {
                        let mut w = arena(choke, content.clone());
                        deploy(&mut w, side, recipe, queue);
                        for x in [24, 27] {
                            for y in [5, 7, 9] {
                                w.spawn(1 - side, "turret", if side == 0 { x } else { 30 - x }, y);
                            }
                        }
                        let state = w.run().result.final_state;
                        wins += usize::from(!state.entities.iter().any(|e| e.owner != side));
                        losses[0] += state.players[usize::from(1 - side)]
                            .counters
                            .lost_invested_matter
                            / 2.;
                        losses[1] += state.players[usize::from(side)]
                            .counters
                            .lost_invested_matter
                            / 2.;
                    }
                    println!(
                        "{},{},{name},{wins},2,{:.0},{:.0}",
                        if queue { "loop" } else { "field" },
                        if choke { "choke" } else { "open" },
                        losses[0],
                        losses[1]
                    );
                }
            }
        }
        return;
    }
    println!("mode,arena,a,b,wins_a,wins_b,ties,mean_net_destroyed_value_a");
    for production in [false, true] {
        for choke in [false, true] {
            for (i, (name_a, a)) in RECIPES.iter().enumerate() {
                for (name_b, b) in &RECIPES[i + 1..] {
                    let mut counts = [0; 3];
                    let mut value = 0.;
                    for side in 0..2 {
                        let mut w = arena(choke, content.clone());
                        deploy(&mut w, side, a, production);
                        deploy(&mut w, 1 - side, b, production);
                        let state = w.run().result.final_state;
                        let alive = |p| {
                            state
                                .entities
                                .iter()
                                .any(|e| e.owner == p && (!production || e.type_key == "factory"))
                        };
                        counts[match (alive(side), alive(1 - side)) {
                            (true, false) => 0,
                            (false, true) => 1,
                            _ => 2,
                        }] += 1;
                        let losses =
                            |p: u8| state.players[p as usize].counters.lost_invested_matter;
                        value += (losses(1 - side) - losses(side)) / 2.;
                    }
                    println!(
                        "{},{},{name_a},{name_b},{},{},{},{value:.1}",
                        if production { "loop" } else { "field" },
                        if choke { "choke" } else { "open" },
                        counts[0],
                        counts[1],
                        counts[2]
                    );
                }
            }
        }
    }
}
