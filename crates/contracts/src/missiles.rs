//! Shared missile planning validation and flight timing for controllers and replicas.
use crate::*;

pub fn validate_silo_plan(
    def: &TypeDefinition,
    plan: &SiloPlan,
    width: u16,
    height: u16,
) -> Result<()> {
    let recipes = def
        .production
        .as_ref()
        .filter(|_| def.silo.is_some())
        .ok_or("target is not a missile silo")?;
    if plan.launches.len() > 65536 {
        return Err("too many queued launches".into());
    }
    for launch in &plan.launches {
        if !recipes.recipes.contains(&launch.type_key) {
            return Err("missile is not supported by this silo".into());
        }
        if launch.target.x >= width || launch.target.y >= height {
            return Err("missile target is outside the map".into());
        }
    }
    Ok(())
}
pub fn flight_ticks(from: Tile, to: Tile, missile: &MissileCapability) -> Tick {
    let dx = f64::from(from.x) - f64::from(to.x);
    let dy = f64::from(from.y) - f64::from(to.y);
    ((dx * dx + dy * dy).sqrt() / missile.speed)
        .ceil()
        .clamp(1.0, f64::from(missile.max_flight_ticks)) as Tick
}
pub fn flight_position(flight: &MissileFlight, tick: f64) -> (f64, f64) {
    let fraction = ((tick - f64::from(flight.launch_tick))
        / f64::from(flight.impact_tick - flight.launch_tick))
    .clamp(0.0, 1.0);
    (
        f64::from(flight.origin.x)
            + (f64::from(flight.target.x) - f64::from(flight.origin.x)) * fraction,
        f64::from(flight.origin.y)
            + (f64::from(flight.target.y) - f64::from(flight.origin.y)) * fraction,
    )
}
