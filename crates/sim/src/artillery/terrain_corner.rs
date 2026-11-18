//! Static retreat potential over the cardinal floor graph.
use super::{DEAD_END_COST, LOOKAHEAD};
use crate::{Terrain, TerrainCell};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

const CACHE_CAPACITY: usize = 8;
const SCALES: [(usize, i32); 4] = [(4, 44), (8, 132), (12, 190), (20, 144)];

#[derive(Clone, Copy)]
struct Span {
    min_sum: i32,
    max_sum: i32,
    min_difference: i32,
    max_difference: i32,
}

impl Span {
    fn at(x: i32, y: i32) -> Self {
        Self {
            min_sum: x + y,
            max_sum: x + y,
            min_difference: x - y,
            max_difference: x - y,
        }
    }

    fn merge(&mut self, other: Self) {
        self.min_sum = self.min_sum.min(other.min_sum);
        self.max_sum = self.max_sum.max(other.max_sum);
        self.min_difference = self.min_difference.min(other.min_difference);
        self.max_difference = self.max_difference.max(other.max_difference);
    }

    fn diameter(self) -> i32 {
        (self.max_sum - self.min_sum).max(self.max_difference - self.min_difference)
    }
}

const NO_NEIGHBOR: usize = usize::MAX;

fn adjacent(neighbors: &[[usize; 4]], index: usize) -> impl Iterator<Item = usize> + '_ {
    neighbors[index]
        .iter()
        .copied()
        .take_while(|&next| next != NO_NEIGHBOR)
}

fn short_dead_end(neighbors: &[[usize; 4]], tile: usize) -> i32 {
    let mut arms = 0;
    for next in adjacent(neighbors, tile) {
        let (mut previous, mut current) = (tile, next);
        for depth in 0..=LOOKAHEAD {
            let mut onward = adjacent(neighbors, current).filter(|&n| n != previous);
            let Some(next) = onward.next() else {
                break;
            };
            if onward.next().is_some() || depth == LOOKAHEAD {
                arms += 1;
                break;
            }
            (previous, current) = (current, next);
        }
    }
    (2_i32 - arms).max(0) * DEAD_END_COST
}

fn build(terrain: &Terrain) -> Arc<[i32]> {
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    let len = terrain.cells.len();
    let mut floor = Vec::new();
    let mut neighbors = vec![[NO_NEIGHBOR; 4]; len];
    let mut spans = vec![Span::at(0, 0); len];
    let mut costs = vec![0; len];

    for (i, cell) in terrain.cells.iter().enumerate() {
        if *cell != TerrainCell::Floor {
            continue;
        }
        let (x, y) = (i % width, i / width);
        floor.push(i);
        spans[i] = Span::at(x as i32, y as i32);
        let candidates = [
            (x > 0).then(|| i - 1),
            (x + 1 < width).then_some(i + 1),
            (y > 0).then(|| i - width),
            (y + 1 < height).then_some(i + width),
        ];
        let mut degree = 0;
        for next in candidates.into_iter().flatten() {
            if terrain.cells[next] == TerrainCell::Floor {
                neighbors[i][degree] = next;
                degree += 1;
            }
        }
        costs[i] = (4 - degree as i32) * 4;
    }

    // Each round adds one legal path step to the reachable neighborhood. The widest
    // projection along x+y or x−y gives its Manhattan diameter. A through passage expands
    // in both directions; space lost at a pocket's end reduces the diameter at several scales.
    let mut next_spans = spans.clone();
    let mut round = 0;
    for (distance, weight) in SCALES {
        while round < distance {
            for &i in &floor {
                let mut span = spans[i];
                for next in adjacent(&neighbors, i) {
                    span.merge(spans[next]);
                }
                next_spans[i] = span;
            }
            std::mem::swap(&mut spans, &mut next_spans);
            round += 1;
        }
        let full_diameter = (2 * distance) as i32;
        for &i in &floor {
            costs[i] += weight * (full_diameter - spans[i].diameter()) / full_diameter;
        }
    }

    // Round grid quantization off along connected floor without diffusing across walls.
    let mut smoothed = costs.clone();
    for _ in 0..3 {
        for &i in &floor {
            let mut total = 4 * costs[i];
            let mut samples = 4;
            for next in adjacent(&neighbors, i) {
                total += costs[next];
                samples += 1;
            }
            smoothed[i] = (total + samples / 2) / samples;
        }
        std::mem::swap(&mut costs, &mut smoothed);
    }
    for i in floor {
        costs[i] += short_dead_end(&neighbors, i);
    }
    costs.into()
}

type Cache = VecDeque<(Terrain, Arc<[i32]>)>;

pub(crate) fn cached(terrain: &Terrain) -> Arc<[i32]> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let lookup = |entries: &mut Cache| {
        let index = entries.iter().position(|(key, _)| key == terrain)?;
        let entry = entries.remove(index).unwrap();
        let values = Arc::clone(&entry.1);
        entries.push_back(entry);
        Some(values)
    };
    if let Some(values) = lookup(&mut cache.lock().unwrap()) {
        return values;
    }
    let values = build(terrain);
    let mut entries = cache.lock().unwrap();
    if let Some(existing) = lookup(&mut entries) {
        return existing;
    }
    if entries.len() == CACHE_CAPACITY {
        entries.pop_front();
    }
    entries.push_back((terrain.clone(), Arc::clone(&values)));
    values
}
