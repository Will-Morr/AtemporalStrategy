//! Generated maps: connected caves, rotational symmetry for 2 and 4 players, asymmetric mode,
//! start access and configurable ore budgets across seeds and sizes.
use atemporal_sim::*;
use std::collections::VecDeque;

fn setup() -> (MatchConfig, Content) {
    let content = load_content(include_str!("../../../config/content.yaml")).unwrap();
    let config = atemporal_content::load_setup(include_str!("../../../config/game.yaml"))
        .unwrap()
        .match_defaults;
    (config, content)
}

fn rotate(t: Tile, size: u16, quarters: u8) -> Tile {
    let mut t = t;
    for _ in 0..quarters {
        t = Tile {
            x: size - 1 - t.y,
            y: t.x,
        };
    }
    t
}

fn idx(w: &WorldState, t: Tile) -> usize {
    usize::from(t.y) * usize::from(w.terrain.width) + usize::from(t.x)
}

/// Floor cells reachable from the first player's miner by four-neighbor steps.
fn reachable(w: &WorldState) -> Vec<bool> {
    let n = usize::from(w.terrain.width);
    let from = w.entities[0].tile;
    let mut seen = vec![false; n * n];
    let mut queue = VecDeque::from([from]);
    seen[idx(w, from)] = true;
    while let Some(t) = queue.pop_front() {
        for (dx, dy) in [(0i32, -1i32), (1, 0), (0, 1), (-1, 0)] {
            let (x, y) = (i32::from(t.x) + dx, i32::from(t.y) + dy);
            if x < 0 || y < 0 || x >= n as i32 || y >= n as i32 {
                continue;
            }
            let nt = Tile {
                x: x as u16,
                y: y as u16,
            };
            if w.terrain.cells[idx(w, nt)] == TerrainCell::Floor && !seen[idx(w, nt)] {
                seen[idx(w, nt)] = true;
                queue.push_back(nt);
            }
        }
    }
    seen
}

fn assert_connected_and_accessible(w: &WorldState, config: &MatchConfig) {
    let seen = reachable(w);
    for (i, cell) in w.terrain.cells.iter().enumerate() {
        assert_eq!(
            *cell == TerrainCell::Floor,
            seen[i],
            "floor cell {i} is a disconnected pocket"
        );
    }
    assert_eq!(w.entities.len(), 3 * usize::from(config.player_count));
    for e in &w.entities {
        assert!(
            seen[idx(w, e.tile)],
            "{:?} of player {} is cut off",
            e.type_key,
            e.owner
        );
    }
    for (i, ore) in w.ore.iter().enumerate() {
        if *ore > 0.0 {
            assert!(seen[i], "ore cell {i} is cut off");
        }
    }
    let total: f64 = w.ore.iter().sum();
    let budget = config.ore_matter_per_start * f64::from(config.player_count);
    assert!(
        (total - budget).abs() < 1e-6,
        "ore budget {total} vs {budget}"
    );
    let sizes = clusters(w);
    assert!(!sizes.is_empty(), "no ore clusters");
    assert!(
        sizes.iter().all(|s| (1..=9).contains(s)),
        "cluster sizes {sizes:?}"
    );
    for e in &w.entities {
        assert!(w.ore[idx(w, e.tile)] == 0.0, "ore under a start entity");
    }
    // The slice walkthroughs place a factory two east and one north of each constructor.
    for player in 0..config.player_count {
        let constructor = w
            .entities
            .iter()
            .find(|e| e.owner == player && e.type_key == "constructor")
            .unwrap();
        if player == 0 {
            let site = Tile {
                x: constructor.tile.x + 2,
                y: constructor.tile.y - 1,
            };
            let output = Tile {
                x: site.x + 1,
                y: site.y,
            };
            assert!(
                seen[idx(w, site)] && seen[idx(w, output)],
                "factory site and output are open"
            );
        }
    }
}

/// Sizes of four-connected ore clusters.
fn clusters(w: &WorldState) -> Vec<usize> {
    let n = usize::from(w.terrain.width);
    let mut seen = vec![false; n * n];
    let mut sizes = vec![];
    for start in 0..n * n {
        if w.ore[start] <= 0.0 || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut queue = VecDeque::from([start]);
        let mut size = 0;
        while let Some(i) = queue.pop_front() {
            size += 1;
            let (x, y) = ((i % n) as i32, (i / n) as i32);
            for (dx, dy) in [(0i32, -1i32), (1, 0), (0, 1), (-1, 0)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= n as i32 || ny >= n as i32 {
                    continue;
                }
                let j = ny as usize * n + nx as usize;
                if w.ore[j] > 0.0 && !seen[j] {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
        }
        sizes.push(size);
    }
    sizes
}

fn assert_symmetric(w: &WorldState, quarters: u8) {
    let size = w.terrain.width;
    for y in 0..size {
        for x in 0..size {
            let t = Tile { x, y };
            let r = rotate(t, size, quarters);
            assert_eq!(
                w.terrain.cells[idx(w, t)],
                w.terrain.cells[idx(w, r)],
                "terrain at {t:?}"
            );
            assert_eq!(w.ore[idx(w, t)], w.ore[idx(w, r)], "ore at {t:?}");
        }
    }
    let players = w.players.len() as u8;
    for e in &w.entities {
        let owner = (e.owner + 1) % players;
        let r = rotate(e.tile, size, quarters);
        assert!(
            w.entities
                .iter()
                .any(|o| o.owner == owner && o.type_key == e.type_key && o.tile == r),
            "player {} {} at {:?} rotates onto player {owner}",
            e.owner,
            e.type_key,
            e.tile
        );
    }
}

fn mismatches(w: &WorldState, quarters: u8) -> usize {
    let size = w.terrain.width;
    (0..size)
        .flat_map(|y| (0..size).map(move |x| Tile { x, y }))
        .filter(|t| {
            w.terrain.cells[idx(w, *t)] != w.terrain.cells[idx(w, rotate(*t, size, quarters))]
        })
        .count()
}

fn ascii(w: &WorldState) -> String {
    let size = usize::from(w.terrain.width);
    let mut rows = vec![];
    for y in 0..size {
        let mut row = String::new();
        for x in 0..size {
            let i = y * size + x;
            let t = Tile {
                x: x as u16,
                y: y as u16,
            };
            row.push(match w.entities.iter().find(|e| e.tile == t) {
                Some(e) => char::from(b'0' + e.owner),
                None if w.ore[i] > 0.0 => 'o',
                None if w.terrain.cells[i] == TerrainCell::Wall => '#',
                None => '.',
            });
        }
        rows.push(row);
    }
    rows.join("\n")
}

#[test]
fn default_two_player_map_is_rotationally_symmetric_and_connected() {
    let (config, content) = setup();
    let w = map::generate(&config, &content).unwrap();
    eprintln!("{}", ascii(&w));
    assert_connected_and_accessible(&w, &config);
    assert_symmetric(&w, 2);
    assert_eq!(
        w.entities[0].tile,
        Tile { x: 4, y: 7 },
        "slice start layout retained"
    );
    let sizes = clusters(&w);
    assert!(sizes.len() >= 16, "scattered clusters: {sizes:?}");
    assert!(
        sizes.iter().any(|s| *s >= 5),
        "some larger deposits: {sizes:?}"
    );
    // Weights 9..=1 give about 53% clusters of three tiles or fewer; check across seeds.
    let mut pooled = vec![];
    for seed in 0..6u64 {
        let mut config = config.clone();
        config.seed = seed.try_into().unwrap();
        pooled.extend(clusters(&map::generate(&config, &content).unwrap()));
    }
    let small = pooled.iter().filter(|s| **s <= 3).count();
    assert!(
        small * 2 > pooled.len(),
        "weighted toward small: {pooled:?}"
    );
    for (i, ore) in w.ore.iter().enumerate() {
        let (x, y) = ((i % 48) as i32, (i / 48) as i32);
        let near_start = [(4, 4), (43, 43)]
            .iter()
            .any(|(ax, ay)| (x - ax).abs() <= 5 && (y - ay).abs() <= 5);
        assert!(
            *ore == 0.0 || !near_start,
            "ore at ({x},{y}) is next to a start"
        );
    }
    // Ore stays off high-traffic paths: nothing hotter than 30% of the smoothed peak.
    let starts = map::starts(config.map_size, config.player_count, &content).unwrap();
    let traffic = map::traffic(&config, &w.terrain, &starts);
    let peak = traffic.smooth.iter().cloned().fold(0.0, f64::max);
    assert!(peak > 0.0, "traffic layer is populated");
    for (i, ore) in w.ore.iter().enumerate() {
        assert!(
            *ore == 0.0 || traffic.smooth[i] <= 0.3 * peak + 1e-9,
            "ore at cell {i} sits on a busy path"
        );
    }
    let walls = w
        .terrain
        .cells
        .iter()
        .filter(|c| **c == TerrainCell::Wall)
        .count();
    let cells = w.terrain.cells.len();
    assert!(
        walls * 100 / cells > 15 && walls * 100 / cells < 60,
        "cave density: {walls}/{cells}"
    );
}

#[test]
fn four_player_map_has_fourfold_symmetry() {
    let (mut config, content) = setup();
    config.player_count = 4;
    let w = map::generate(&config, &content).unwrap();
    eprintln!("{}", ascii(&w));
    assert_connected_and_accessible(&w, &config);
    assert_symmetric(&w, 1);
    assert_eq!(w.players.len(), 4);
    assert_eq!(w.control_groups.len(), 40);
}

#[test]
fn asymmetric_maps_support_two_to_four_players() {
    let (mut config, content) = setup();
    config.symmetric = false;
    for players in 2..=4u8 {
        config.player_count = players;
        let w = map::generate(&config, &content).unwrap();
        assert_connected_and_accessible(&w, &config);
        let quarters = if players == 4 { 1 } else { 2 };
        assert!(
            mismatches(&w, quarters) > 50,
            "{players} players: rock is not mirrored"
        );
    }
    config.symmetric = true;
    config.player_count = 3;
    assert!(
        map::generate(&config, &content).is_err(),
        "three players cannot be symmetric"
    );
}

#[test]
fn generation_succeeds_across_seeds_sizes_and_budgets() {
    let (base, content) = setup();
    for seed in 0..12u64 {
        for size in [16u16, 24, 48, 96] {
            for players in 2..=4u8 {
                for symmetric in [true, false] {
                    if symmetric && players == 3 {
                        continue;
                    }
                    let mut config = base.clone();
                    config.seed = seed.try_into().unwrap();
                    config.map_size = size;
                    config.player_count = players;
                    config.symmetric = symmetric;
                    config.ore_matter_per_start = 1000.0 + f64::from(size);
                    let w = map::generate(&config, &content).unwrap_or_else(|e| {
                        panic!("seed {seed} size {size} players {players}: {e}")
                    });
                    assert_connected_and_accessible(&w, &config);
                    if symmetric {
                        assert_symmetric(&w, if players == 4 { 1 } else { 2 });
                    }
                }
            }
        }
    }
}

#[test]
fn generation_is_deterministic_per_seed() {
    let (mut config, content) = setup();
    let a = map::generate(&config, &content).unwrap();
    let b = map::generate(&config, &content).unwrap();
    assert_eq!(
        identity::world_hash(&a).unwrap(),
        identity::world_hash(&b).unwrap()
    );
    config.seed = 7u64.try_into().unwrap();
    let c = map::generate(&config, &content).unwrap();
    assert_ne!(
        identity::world_hash(&a).unwrap(),
        identity::world_hash(&c).unwrap()
    );
}

/// `MAPS=20 cargo test -p atemporal-sim --test maps dump_maps -- --ignored --nocapture` prints a
/// batch of default-size maps for visual tuning.
#[test]
#[ignore]
fn dump_maps() {
    let (mut config, content) = setup();
    // MAPS=<n> chooses how many seeds to print.
    let count = std::env::var("MAPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8u64);
    for seed in 0..count {
        config.seed = seed.try_into().unwrap();
        println!(
            "seed {seed}\n{}",
            ascii(&map::generate(&config, &content).unwrap())
        );
    }
}

#[test]
fn generated_ore_survives_json_without_changing_float_bits() {
    let (mut config, content) = setup();
    for seed in [42u64, 1, 17, 99] {
        config.seed = seed.try_into().unwrap();
        let world = map::generate(&config, &content).unwrap();
        let encoded = serde_json::to_vec(&world).unwrap();
        let decoded: WorldState = serde_json::from_slice(&encoded).unwrap();
        for (tile, (before, after)) in world.ore.iter().zip(&decoded.ore).enumerate() {
            assert_eq!(
                before.to_bits(),
                after.to_bits(),
                "seed {seed}, ore tile {tile}: {before} became {after}"
            );
        }
        assert_eq!(
            identity::world_hash(&world).unwrap(),
            identity::world_hash(&decoded).unwrap()
        );
        assert_eq!(world, decoded);
    }
}
