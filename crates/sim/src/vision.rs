//! Ground vision follows open tiles within the observer's circular sight radius.
use crate::{Terrain, TerrainCell, Tile};
use std::collections::VecDeque;

/// Eight-way breadth-first sight with at most two steps of detour. Walls can be
/// seen themselves. A diagonal step requires both adjacent side tiles to be open.
pub fn flood_vision(
    terrain: &Terrain,
    origin: Tile,
    radius: f64,
    wall: impl Fn(Tile) -> bool,
    mut reveal: impl FnMut(Tile),
) {
    let r = (radius.ceil() as u16).min(terrain.width.max(terrain.height));
    let x0 = origin.x.saturating_sub(r);
    let y0 = origin.y.saturating_sub(r);
    let x1 = origin.x.saturating_add(r).min(terrain.width - 1);
    let y1 = origin.y.saturating_add(r).min(terrain.height - 1);
    let width = usize::from(x1 - x0 + 1);
    let index = |t: Tile| usize::from(t.y - y0) * width + usize::from(t.x - x0);
    let blocked = |t: Tile| {
        terrain.cells[usize::from(t.y) * usize::from(terrain.width) + usize::from(t.x)]
            == TerrainCell::Wall
            || wall(t)
    };
    let mut visited = vec![false; width * usize::from(y1 - y0 + 1)];
    let mut frontier = VecDeque::from([(origin, 0u16)]);
    visited[index(origin)] = true;
    while let Some((tile, steps)) = frontier.pop_front() {
        let dx = origin.x.abs_diff(tile.x);
        let dy = origin.y.abs_diff(tile.y);
        if steps <= dx.max(dy).saturating_add(2) {
            reveal(tile);
        }
        if steps >= r || (tile != origin && blocked(tile)) {
            continue;
        }
        for (dx, dy, _) in crate::world::DIRS {
            let x = i32::from(tile.x) + dx;
            let y = i32::from(tile.y) + dy;
            if x < i32::from(x0) || x > i32::from(x1) || y < i32::from(y0) || y > i32::from(y1) {
                continue;
            }
            let next = Tile {
                x: x as u16,
                y: y as u16,
            };
            if visited[index(next)] || crate::Sim::dist2(origin, next) > radius * radius {
                continue;
            }
            if dx != 0
                && dy != 0
                && (blocked(Tile {
                    x: next.x,
                    y: tile.y,
                }) || blocked(Tile {
                    x: tile.x,
                    y: next.y,
                }))
            {
                continue;
            }
            visited[index(next)] = true;
            frontier.push_back((next, steps + 1));
        }
    }
}
