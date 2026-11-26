//! Replay data and queries shared by the authoritative controller and native runner.
use atemporal_sim::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

pub const TIMELINE_BUCKET: Tick = 20;
/// Published index entries per player; cap-length runs widen buckets instead of growing.
pub const TIMELINE_INDEX_PER_PLAYER: Tick = 500;
/// Most stats buckets one `get_stats` answer carries; wider windows coarsen automatically.
pub const MAX_STAT_BUCKETS: Tick = 2000;

#[derive(Clone)]
pub struct RevisionData {
    pub revision: Revision,
    pub round: u32,
    pub parent: Option<Revision>,
    pub base_tick: Tick,
    pub dictionary: Vec<EntityRef>,
    pub samples: BTreeMap<Tick, Sample>,
    pub checkpoints: BTreeMap<Tick, WorldState>,
    pub stats: BTreeMap<Tick, StatsSample>,
    pub events: Vec<WorldEvent>,
    pub timeline: Vec<TimelineBucket>,
    pub outcome: Option<Outcome>,
    pub final_hash: String,
    pub command_outcomes: Vec<CommandOutcome>,
    pub sim_duration_ms: u64,
    pub turns: Vec<AcceptedTurn>,
    pub precedence: Vec<RoundPrecedence>,
    pub timeline_index: Vec<TimelineBucket>,
    pub score: Option<RoundScore>,
    pub timed: Option<TimedAdjudication>,
    pub started: Option<Instant>,
    /// False once the regenerable body (samples, checkpoints, stats, events, timeline) was evicted.
    pub loaded: bool,
    /// Body size proxy: JSON bytes of the result cache (or an estimate before it is written).
    pub bytes: u64,
    pub last_used: u64,
}

impl RevisionData {
    pub fn new(revision: Revision, round: u32, parent: Option<Revision>, base_tick: Tick) -> Self {
        Self {
            revision,
            round,
            parent,
            base_tick,
            dictionary: vec![],
            samples: BTreeMap::new(),
            checkpoints: BTreeMap::new(),
            stats: BTreeMap::new(),
            events: vec![],
            timeline: vec![],
            outcome: None,
            final_hash: String::new(),
            command_outcomes: vec![],
            sim_duration_ms: 0,
            turns: vec![],
            precedence: vec![],
            timeline_index: vec![],
            score: None,
            timed: None,
            started: None,
            loaded: true,
            bytes: 0,
            last_used: 0,
        }
    }

    /// Start a child revision from the last unchanged checkpoint. Samples, events,
    /// stats and command outcomes at the checkpoint tick belong to the replayed suffix.
    pub fn fork_at_inputs(
        &self,
        revision: Revision,
        round: u32,
        turns: &[AcceptedTurn],
    ) -> Result<(WorldState, Self)> {
        // Empty turns change the score ledger without changing the simulated world.
        // Only published passes can reuse the final prefix. Jobs without new
        // turns (genesis or an explicit regeneration) start at the opening.
        let latest_safe_pass_tick = self
            .outcome
            .as_ref()
            .map_or(0, |outcome| outcome.terminal_state_tick.saturating_sub(1));
        let earliest = turns
            .iter()
            .filter(|t| t.round == round)
            .map(|t| {
                if t.commands.is_empty() {
                    latest_safe_pass_tick
                } else {
                    t.tick
                }
            })
            .min()
            .unwrap_or(0);
        let (base_tick, checkpoint) = self
            .checkpoints
            .range(..=earliest)
            .next_back()
            .map(|(t, s)| (*t, s.clone()))
            .ok_or("no checkpoint at or before the earliest new input")?;
        let mut data = Self::new(
            revision,
            round,
            (round > 0).then_some(self.revision),
            base_tick,
        );
        // Reuse the unchanged prefix from the parent; the job regenerates everything from base_tick.
        data.dictionary = self.dictionary.clone();
        data.samples = self
            .samples
            .range(..base_tick)
            .map(|(t, s)| (*t, s.clone()))
            .collect();
        data.checkpoints = self
            .checkpoints
            .range(..=base_tick)
            .map(|(t, s)| (*t, s.clone()))
            .collect();
        data.stats = self
            .stats
            .range(..base_tick)
            .map(|(t, s)| (*t, s.clone()))
            .collect();
        data.events = self
            .events
            .iter()
            .filter(|e| e.tick < base_tick)
            .cloned()
            .collect();
        data.timeline = self
            .timeline
            .iter()
            .filter(|b| b.to_tick_exclusive <= base_tick)
            .cloned()
            .collect();
        // Preserve authoritative diagnostics for the unchanged checkpoint prefix.
        let prefix_commands: BTreeSet<_> = self
            .turns
            .iter()
            .filter(|turn| turn.tick < base_tick)
            .flat_map(|turn| turn.commands.iter().map(|command| &command.id))
            .collect();
        data.command_outcomes = self
            .command_outcomes
            .iter()
            .filter(|outcome| prefix_commands.contains(&outcome.command_id))
            .cloned()
            .collect();
        Ok((checkpoint, data))
    }

    /// Release the materialized replay while retaining its ledger and verification result.
    /// Revision zero owns the opening state used to regenerate every other revision.
    pub fn discard_body(&mut self) {
        self.checkpoints
            .retain(|&tick, _| self.revision == 0 && tick == 0);
        self.dictionary = Vec::new();
        self.samples.clear();
        self.stats.clear();
        self.events = Vec::new();
        self.timeline = Vec::new();
        self.loaded = false;
    }

    pub fn snapshot_range(
        &self,
        snapshot_interval: Tick,
        from: Tick,
        to: Tick,
        stride: Tick,
    ) -> ServerMessage {
        let stride = stride.max(snapshot_interval);
        let samples = self
            .samples
            .range(from..=to)
            .filter(|(t, _)| (*t - from.min(**t)).is_multiple_of(stride) || **t == to)
            .map(|(_, s)| s.clone())
            .collect();
        ServerMessage::SnapshotRange {
            revision: self.revision,
            index_width: if self.dictionary.len() <= usize::from(u16::MAX) {
                16
            } else {
                32
            },
            entity_dictionary: self.dictionary.clone(),
            samples,
        }
    }

    pub fn stats_range(
        &self,
        snapshot_interval: Tick,
        from: Tick,
        to: Tick,
        bucket_width: Tick,
    ) -> ServerMessage {
        let interval = snapshot_interval.max(1);
        let span = to.saturating_sub(from).saturating_add(1);
        let width = bucket_width
            .max(interval)
            .max(span.div_ceil(MAX_STAT_BUCKETS))
            .next_multiple_of(interval);
        // Banks and counters are gauges/cumulative, so a bucket reports its latest sample.
        let mut buckets: Vec<StatsSample> = vec![];
        for (tick, sample) in self.stats.range(from..=to) {
            let bucket = (tick - from) / width;
            if buckets
                .last()
                .is_some_and(|last| (last.tick - from) / width == bucket)
            {
                buckets.pop();
            }
            buckets.push(sample.clone());
        }
        ServerMessage::StatsRange {
            revision: self.revision,
            buckets,
        }
    }

    pub fn round_result(&self, player_count: u8) -> Result<ServerMessage> {
        let mut totals: BTreeMap<PlayerId, u64> =
            (0..player_count).map(|player| (player, 0)).collect();
        for turn in &self.turns {
            *totals.entry(turn.player).or_default() += turn.duration_ms.get();
        }
        Ok(ServerMessage::RoundResult {
            revision: self.revision,
            round: self.round,
            parent_revision: self.parent,
            outcome: self.outcome.clone().ok_or("revision is not published")?,
            timeline_index: self.timeline_index.clone(),
            score: self.score.clone(),
            timed: self.timed.clone(),
            time_totals: totals
                .into_iter()
                .map(|(player_id, duration_ms)| PlayerTime {
                    player_id,
                    total_ms: duration_ms.try_into().unwrap_or_default(),
                })
                .collect(),
            sim_duration_ms: self.sim_duration_ms.try_into().unwrap_or_default(),
            command_outcomes: self.command_outcomes.clone(),
        })
    }

    pub fn commands_range(&self, from: Tick, to: Tick) -> ServerMessage {
        ServerMessage::Commands {
            revision: self.revision,
            turns: draft::effective_turns(&self.turns)
                .into_iter()
                .filter(|t| t.tick >= from && t.tick <= to)
                .collect(),
        }
    }

    pub fn events_range(&self, from: Tick, to: Tick, effects_only: bool) -> ServerMessage {
        ServerMessage::Events {
            revision: self.revision,
            events: self
                .events
                .iter()
                .filter(|e| e.tick >= from && e.tick <= to)
                .filter(|e| {
                    !effects_only
                        || matches!(
                            e.event,
                            PresentationEvent::MissileLaunch { .. }
                                | PresentationEvent::MissileImpact { .. }
                                | PresentationEvent::Attack { .. }
                                | PresentationEvent::Impact { .. }
                                | PresentationEvent::Destroyed { .. }
                        )
                })
                .cloned()
                .collect(),
        }
    }

    pub fn estimate_bytes(&self) -> u64 {
        let checkpoints: usize = self
            .checkpoints
            .values()
            .map(|c| 256 + c.terrain.cells.len() + c.ore.len() * 8 + c.entities.len() * 400)
            .sum();
        let samples: usize = self
            .samples
            .values()
            .map(|s| 32 + s.entities.len() * 40 + s.ore.len() * 16)
            .sum();
        (checkpoints
            + samples
            + self.stats.len() * 256
            + self.events.len() * 96
            + self.timeline.len() * 48
            + self.dictionary.len() * 64) as u64
    }
}

/// Dominant activity per player per coarse bucket: combat > construction > mining > movement > idle.
/// Bucket width grows with the run so the index never exceeds `players × TIMELINE_INDEX_PER_PLAYER`.
pub fn timeline_index(buckets: &[TimelineBucket], players: u8) -> Vec<TimelineBucket> {
    let span = buckets
        .iter()
        .map(|b| b.to_tick_exclusive)
        .max()
        .unwrap_or(0);
    let width = span
        .div_ceil(TIMELINE_INDEX_PER_PLAYER)
        .next_multiple_of(TIMELINE_BUCKET)
        .max(TIMELINE_BUCKET);
    let mut merged: BTreeMap<(Tick, PlayerId), [u32; 5]> = BTreeMap::new();
    for b in buckets {
        let key = (b.from_tick / width * width, b.player_id);
        let slot = merged.entry(key).or_insert([0; 5]);
        let a = b.activity as usize;
        slot[a] = slot[a].max(b.affected_entities);
    }
    let _ = players;
    merged
        .into_iter()
        .filter_map(|((from, player_id), counts)| {
            let order = [
                Activity::Combat,
                Activity::Construction,
                Activity::Mining,
                Activity::Movement,
                Activity::Idle,
            ];
            let (a, c) = order
                .iter()
                .enumerate()
                .map(|(i, a)| (*a, counts[i]))
                .find(|(_, c)| *c > 0)?;
            Some(TimelineBucket {
                from_tick: from,
                to_tick_exclusive: from + width,
                player_id,
                activity: a,
                affected_entities: c,
                severity: f64::from(c),
            })
        })
        .collect()
}

/// Timed jobs cannot stop before the next lock boundary; scoreboard jobs may stop any time.
pub fn minimum_end_tick(config: &MatchConfig, editable_from: Tick) -> Tick {
    match config.objective {
        Objective::Timed {
            lock_ticks_per_round,
        }
        | Objective::Hybrid {
            lock_ticks_per_round,
            ..
        } => editable_from
            .saturating_add(lock_ticks_per_round)
            .min(config.max_tick),
        Objective::SimpleWin { .. } | Objective::Scoreboard { .. } => 0,
    }
}

/// Run a job to completion on the calling thread, collecting its outputs into `data`.
pub fn collect_run(request: &SimRequest, data: &mut RevisionData, players: u8) -> Result<()> {
    let cancel = AtomicBool::new(false);
    // The engine emits checkpoints at interval ticks; the base state is the first one.
    data.checkpoints
        .entry(request.checkpoint.tick)
        .or_insert_with(|| request.checkpoint.clone());
    let result = atemporal_sim::run(request, &cancel, &mut |o| match o {
        Output::Dictionary(d) => data.dictionary.extend(d),
        Output::Sample(s) => {
            data.samples.insert(s.tick, s);
        }
        Output::Checkpoint(c) => {
            data.checkpoints.insert(c.tick, c);
        }
        Output::Stats(s) => {
            data.stats.insert(s.tick, s);
        }
        Output::Events(e) => data.events.extend(e),
        Output::Timeline(t) => data.timeline.extend(t),
        Output::Progress { .. } => {}
    })?;
    data.outcome = Some(result.outcome);
    data.final_hash = result.final_hash;
    data.sim_duration_ms = result.sim_duration_ms;
    data.command_outcomes = result.command_outcomes;
    data.timeline_index = timeline_index(&data.timeline, players);
    data.loaded = true;
    Ok(())
}
