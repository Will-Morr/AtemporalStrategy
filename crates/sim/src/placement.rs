//! Shared terrain and direction rules for placing structures.
use crate::{CardinalDirection, Terrain, TerrainCell, Tile, world::cardinal_offset};

pub fn in_bounds(terrain: &Terrain, tile: Tile) -> bool {
    tile.x < terrain.width && tile.y < terrain.height
}

pub fn is_floor(terrain: &Terrain, tile: Tile) -> bool {
    in_bounds(terrain, tile)
        && terrain.cells[usize::from(tile.y) * usize::from(terrain.width) + usize::from(tile.x)]
            == TerrainCell::Floor
}

pub fn output_tile(terrain: &Terrain, origin: Tile, direction: CardinalDirection) -> Option<Tile> {
    let (dx, dy) = cardinal_offset(direction);
    let tile = Tile {
        x: u16::try_from(i32::from(origin.x) + dx).ok()?,
        y: u16::try_from(i32::from(origin.y) + dy).ok()?,
    };
    in_bounds(terrain, tile).then_some(tile)
}

pub fn output_directions(
    wanted: Option<CardinalDirection>,
) -> impl Iterator<Item = CardinalDirection> {
    [
        CardinalDirection::N,
        CardinalDirection::E,
        CardinalDirection::S,
        CardinalDirection::W,
    ]
    .into_iter()
    .filter(move |direction| wanted.is_none_or(|wanted| wanted == *direction))
}
