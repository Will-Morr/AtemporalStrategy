//! Calibrate synthetic combat corridors against shipped two-player map generation.
//! Run with `cargo run -p atemporal-sim --example corridor_widths`.
use atemporal_sim::{Terrain, TerrainCell, Tile, map};
use std::collections::VecDeque;

const SEEDS: u64 = 32;
const ROOM_WIDTH: usize = 8;
const START_RADIUS: u16 = 7;

fn distances(terrain: &Terrain, anchor: Tile) -> Vec<usize> {
    let n = usize::from(terrain.width);
    let start = usize::from(anchor.y) * n + usize::from(anchor.x);
    let mut distances = vec![usize::MAX; terrain.cells.len()];
    let mut queue = VecDeque::from([start]);
    distances[start] = 0;
    while let Some(i) = queue.pop_front() {
        // Both axes are bounded by the generator's surrounding wall.
        for next in [i - n, i + 1, i + n, i - 1] {
            if terrain.cells[next] == TerrainCell::Floor && distances[next] == usize::MAX {
                distances[next] = distances[i] + 1;
                queue.push_back(next);
            }
        }
    }
    distances
}

/// Side of the largest entirely walkable, axis-aligned square containing a tile.
/// The square can extend to either side, so hugging one wall does not halve the width.
fn widths(terrain: &Terrain) -> Vec<usize> {
    let n = usize::from(terrain.width);
    let mut ending_here = vec![0; terrain.cells.len()];
    let mut widths = vec![0; terrain.cells.len()];
    for i in n + 1..terrain.cells.len() {
        if terrain.cells[i] != TerrainCell::Floor {
            continue;
        }
        let side = 1 + ending_here[i - 1]
            .min(ending_here[i - n])
            .min(ending_here[i - n - 1]);
        ending_here[i] = side;
        for row in 0..side {
            let right = i - row * n;
            for width in &mut widths[right + 1 - side..=right] {
                *width = side.max(*width);
            }
        }
    }
    widths
}

fn inside_radius(tile: Tile, center: Tile, radius: u16) -> bool {
    let dx = u32::from(tile.x.abs_diff(center.x));
    let dy = u32::from(tile.y.abs_diff(center.y));
    dx * dx + dy * dy <= u32::from(radius).pow(2)
}

fn report(label: &str, counts: &[usize]) {
    let total: usize = counts.iter().sum();
    if total == 0 {
        println!("{label}: no passage samples");
        return;
    }
    let mut cumulative = 0;
    let median = counts
        .iter()
        .position(|count| {
            cumulative += count;
            cumulative >= total.div_ceil(2)
        })
        .unwrap();
    println!("{label}: {total} samples, median {median} tiles");
    cumulative = 0;
    for (width, count) in counts.iter().enumerate().skip(1) {
        cumulative += count;
        println!(
            "  {width} tiles: {count:>6}    CDF {:>5.1}%",
            100. * cumulative as f64 / total as f64
        );
    }
}

fn main() -> atemporal_sim::Result<()> {
    let mut config =
        atemporal_content::load_setup(include_str!("../../../config/game.yaml"))?.match_defaults;
    config.player_count = 2;
    let content = atemporal_sim::load_content(include_str!("../../../config/content.yaml"))?;
    let n = usize::from(config.map_size);
    let starts = map::starts(config.map_size, config.player_count, &content)?;
    let center = Tile {
        x: config.map_size / 2,
        y: config.map_size / 2,
    };
    let center_radius = (config.map_size / 12).max(2);
    let mut shortest_counts = [0; ROOM_WIDTH];
    let mut traffic_counts = [0; ROOM_WIDTH];
    let mut room_tiles = 0;
    let mut room_visits = 0;
    for seed in 0..SEEDS {
        config.seed = seed.try_into()?;
        let terrain = map::terrain(&config, &starts)?;
        let traffic = map::traffic(&config, &terrain, &starts);
        let from_a = distances(&terrain, starts[0].anchor);
        let from_b = distances(&terrain, starts[1].anchor);
        let b = starts[1].anchor;
        let shortest = from_a[usize::from(b.y) * n + usize::from(b.x)];
        for (i, width) in widths(&terrain).into_iter().enumerate() {
            if width == 0 {
                continue;
            }
            let tile = Tile {
                x: (i % n) as u16,
                y: (i / n) as u16,
            };
            if starts
                .iter()
                .any(|start| inside_radius(tile, start.anchor, START_RADIUS))
                || inside_radius(tile, center, center_radius)
            {
                continue;
            }
            let on_shortest_route = from_a[i].saturating_add(from_b[i]) == shortest;
            let visits = traffic.raw[i] as usize;
            if width < ROOM_WIDTH {
                shortest_counts[width] += usize::from(on_shortest_route);
                traffic_counts[width] += visits;
            } else {
                room_tiles += usize::from(on_shortest_route);
                room_visits += visits;
            }
        }
    }
    println!(
        "{SEEDS} generated {n}×{n} maps, seeds 0–{}, start and center clearings excluded",
        SEEDS - 1
    );
    println!(
        "Width is the side of the largest axis-aligned all-floor square covering a route tile"
    );
    println!(
        "Excluded widths >= {ROOM_WIDTH} as rooms ({room_tiles} shortest-route tiles, {room_visits} traffic visits)"
    );
    report(
        "Tiles on any shortest orthogonal start-to-start route",
        &shortest_counts,
    );
    report(
        "Visits on the generator's mixed-movement traffic routes",
        &traffic_counts,
    );
    Ok(())
}
