//! Deterministic tick engine: S[t] is the state before tick t; running t yields S[t+1].
#![deny(clippy::disallowed_types)]
pub use atemporal_content::{load_content, normalize_content};
pub use atemporal_contracts::*;

mod artillery;
mod commands;
mod fields;
pub mod map;
mod output;
pub mod player_timeline;
mod silo;
mod tick;
pub mod vision;
mod world;

pub use artillery::{ArtilleryRetreat, ArtilleryRetreatScore};
pub use output::{Output, RunResult};
pub use world::Sim;

/// Increment when deterministic rules change so archives and peripherals fail closed across rulesets.
pub const RULES_VERSION: u32 = 11;

use std::sync::atomic::{AtomicBool, Ordering};

/// Run a job from its checkpoint to the horizon or inactivity, emitting samples/checkpoints/events.
pub fn run(
    request: &SimRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Output),
) -> Result<RunResult> {
    run_with_hook(request, cancel, emit, &mut || {})
}

/// As `run`, calling `between_ticks` at every tick boundary so short jobs can be serviced.
pub fn run_with_hook(
    request: &SimRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Output),
    between_ticks: &mut dyn FnMut(),
) -> Result<RunResult> {
    run_observed(request, cancel, emit, between_ticks, &mut |_, _| {})
}

fn run_observed(
    request: &SimRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Output),
    between_ticks: &mut dyn FnMut(),
    observe: &mut dyn FnMut(&Sim, &[WorldEvent]),
) -> Result<RunResult> {
    let mut sim = Sim::new(request)?;
    observe(&sim, &[]);
    sim.emit_state_outputs(emit, true);
    loop {
        between_ticks();
        if cancel.load(Ordering::Relaxed) {
            return Err("canceled".into());
        }
        let tick = sim.state.tick;
        if tick >= request.end_tick_exclusive {
            return Ok(sim.finish(StopReason::AbsoluteHorizon, emit));
        }
        sim.step_observed(emit, observe)?;
        let now = sim.state.tick;
        // Simple win ends on the first resolved loss, regardless of remaining orders,
        // missiles, the optional decided cutoff or a requested minimum horizon.
        if matches!(request.config.objective, Objective::SimpleWin { .. })
            && sim.state.players.iter().any(|p| p.currently_eliminated)
        {
            return Ok(sim.finish(StopReason::Elimination, emit));
        }
        if now >= request.end_tick_exclusive {
            return Ok(sim.finish(StopReason::AbsoluteHorizon, emit));
        }
        if request.config.stop_when_decided && now >= request.minimum_end_tick && sim.decided() {
            return Ok(sim.finish(StopReason::Elimination, emit));
        }
        if now >= request.minimum_end_tick && sim.inactive() {
            return Ok(sim.finish(StopReason::Inactivity, emit));
        }
    }
}

/// Exact in-process reconstruction of S[target] from the request checkpoint; no sampling or snapping.
pub fn reconstruct(request: &SimRequest, target: Tick) -> Result<WorldState> {
    if target < request.checkpoint.tick {
        return Err("target precedes checkpoint".into());
    }
    let mut sim = Sim::new(request)?;
    let mut sink = |_: Output| {};
    while sim.state.tick < target {
        sim.step(&mut sink)?;
    }
    Ok(sim.state)
}
