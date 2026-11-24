use atemporal_bots::{FrameRules, Style, plan_frame};
use atemporal_contracts::{Command, ControlLimit, Order};
use atemporal_sim::{map, placement, player_timeline};
use std::collections::BTreeSet;

#[test]
fn enclosure_opening_pairs_builders_with_walled_routes_and_forward_production() {
    let setup = atemporal_content::load_setup(include_str!("../../../config/game.yaml")).unwrap();
    let content =
        atemporal_content::load_content(include_str!("../../../config/content.yaml")).unwrap();
    for (seed, player) in [(42u64, 0), (211, 1), (907, 0)] {
        let mut config = setup.match_defaults.clone();
        config.seed = seed.try_into().unwrap();
        let world = map::generate(&config, &content).unwrap();
        let view = player_timeline::project(&world, player, &config, &content).unwrap();
        let rules = FrameRules {
            config: &config,
            content: &content,
            terrain: &world.terrain,
        };
        let enclosure = plan_frame(&view, player, Style::Enclosure, rules, 0);
        let tactical = plan_frame(&view, player, Style::Tactical, rules, 0);
        assert_ne!(
            enclosure, tactical,
            "seed {seed} did not get a distinct opening"
        );
        let mut placements = BTreeSet::new();
        let mut outputs = BTreeSet::new();
        let (mut walls, mut turrets, mut attackers) = (0, 0, 0);
        let mut builds = false;
        for row in &enclosure.commands {
            match &row.command {
                Command::AssignOrder {
                    order: Order::Construct { .. },
                    ..
                } => builds = true,
                Command::PlaceBlueprints {
                    type_key,
                    tiles,
                    output_directions,
                    ..
                } => {
                    for (index, &tile) in tiles.iter().enumerate() {
                        assert!(placement::is_floor(&world.terrain, tile));
                        assert!(placements.insert(tile));
                        if let Some(heading) =
                            output_directions.as_ref().and_then(|dirs| dirs.get(index))
                        {
                            let out =
                                placement::output_tile(&world.terrain, tile, *heading).unwrap();
                            assert!(placement::is_floor(&world.terrain, out));
                            assert!(outputs.insert(out));
                        }
                    }
                    walls += usize::from(type_key == "wall") * tiles.len();
                    turrets += usize::from(type_key == "turret") * tiles.len();
                }
                Command::ConfigureBlueprints { settings, .. }
                    if matches!(settings.order, Order::AttackMove { .. })
                        && settings.queue.iter().any(|kind| kind == "artillery") =>
                {
                    attackers += 1;
                    assert!(settings.queue_loop_flags.iter().any(|&loops| loops));
                }
                _ => {}
            }
        }
        assert!(builds);
        assert!(outputs.is_disjoint(&placements));
        assert!(
            walls >= 2 && turrets >= 4 && attackers >= 2,
            "seed {seed}: {walls} walls, {turrets} turrets, {attackers} attack factories"
        );
    }
}

#[test]
fn enclosure_reads_only_the_player_view_and_respects_single_order() {
    let mut config = atemporal_content::load_setup(include_str!("../../../config/game.yaml"))
        .unwrap()
        .match_defaults;
    config.seed = 211.try_into().unwrap();
    let content =
        atemporal_content::load_content(include_str!("../../../config/content.yaml")).unwrap();
    let original = map::generate(&config, &content).unwrap();
    let mut changed = original.clone();
    // Give the hidden opposing seat a different bank without changing the player's view.
    changed.players[1].bank += 999.;
    let view = player_timeline::project(&original, 0, &config, &content).unwrap();
    let other = player_timeline::project(&changed, 0, &config, &content).unwrap();
    let rules = FrameRules {
        config: &config,
        content: &content,
        terrain: &original.terrain,
    };
    assert_eq!(
        plan_frame(&view, 0, Style::Enclosure, rules, 0),
        plan_frame(&other, 0, Style::Enclosure, rules, 0)
    );
    config.control_limit = ControlLimit::SingleOrder;
    let rules = FrameRules {
        config: &config,
        content: &content,
        terrain: &original.terrain,
    };
    assert_eq!(
        plan_frame(&view, 0, Style::Enclosure, rules, 0)
            .commands
            .len(),
        1
    );
}
