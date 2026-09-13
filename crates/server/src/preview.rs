//! Read-only completed prefixes. Generation identity never crosses into published query caches.
use crate::controller::RevisionData;
use atemporal_sim::*;

pub fn frontier(data: &RevisionData, generation: &str, end_tick: Tick) -> ReplayFrontier {
    ReplayFrontier {
        revision: data.revision,
        generation: generation.into(),
        through_tick: data.samples.keys().next_back().copied().unwrap_or(0),
        end_tick,
    }
}

pub fn response(
    data: &RevisionData,
    generation: &str,
    request_id: String,
    tick: Tick,
    from: Tick,
    to: Tick,
) -> Result<ServerMessage> {
    let through = frontier(data, generation, 0).through_tick;
    if tick > through || from > to || to > through || to.saturating_sub(from) > 2000 {
        return Err("preview range is outside the completed prefix".into());
    }
    let snapshot = data
        .checkpoints
        .range(..=tick)
        .next_back()
        .ok_or("preview has no checkpoint")?
        .1
        .clone();
    Ok(ServerMessage::ReplayPreview {
        request_id,
        generation: generation.into(),
        revision: data.revision,
        through_tick: through,
        snapshot: Box::new(snapshot),
        entity_dictionary: data.dictionary.clone(),
        samples: data
            .samples
            .range(from..=to)
            .map(|(_, s)| s.clone())
            .collect(),
        events: data
            .events
            .iter()
            .filter(|e| {
                e.tick >= from && e.tick <= to && !matches!(e.event, PresentationEvent::Move { .. })
            })
            .cloned()
            .collect(),
    })
}
