//! Job outputs: compact samples, stats, timeline buckets, checkpoints and the final result.
use crate::world::Sim;
use crate::*;

pub enum Output {
    Progress { tick: Tick },
    Dictionary(Vec<EntityRef>),
    Sample(Sample),
    Checkpoint(WorldState),
    Stats(StatsSample),
    Events(Vec<WorldEvent>),
    Timeline(Vec<TimelineBucket>),
}

pub struct RunResult {
    pub outcome: Outcome,
    pub final_hash: String,
    pub command_outcomes: Vec<CommandOutcome>,
    pub sim_duration_ms: u64,
    pub final_state: WorldState,
}

impl Sim {
    /// Emit outputs for the current state S[tick]; `initial` is the job's checkpoint state.
    pub(crate) fn emit_state_outputs(&mut self, emit: &mut dyn FnMut(Output), initial: bool) {
        let tick = self.state.tick;
        if !self.new_dictionary.is_empty() {
            emit(Output::Dictionary(std::mem::take(&mut self.new_dictionary)));
        }
        if !self.pending_events.is_empty() {
            emit(Output::Events(std::mem::take(&mut self.pending_events)));
        }
        if !initial {
            self.accumulate_timeline();
        }
        let interval = self.config.snapshot_interval;
        if tick.is_multiple_of(interval) {
            emit(Output::Sample(self.sample()));
            emit(Output::Stats(self.stats()));
            if !initial {
                self.flush_timeline(emit, tick);
            }
        }
        if !initial && tick.is_multiple_of(self.config.checkpoint_interval) {
            emit(Output::Checkpoint(self.state.clone()));
        }
        if !initial && tick.is_multiple_of(100) {
            emit(Output::Progress { tick });
        }
    }

    pub(crate) fn finish(
        mut self,
        stop_reason: StopReason,
        emit: &mut dyn FnMut(Output),
    ) -> RunResult {
        let tick = self.state.tick;
        let interval = self.config.snapshot_interval;
        if !tick.is_multiple_of(interval) {
            emit(Output::Sample(self.sample()));
            emit(Output::Stats(self.stats()));
            self.flush_timeline(emit, tick);
        }
        if !tick.is_multiple_of(self.config.checkpoint_interval) {
            emit(Output::Checkpoint(self.state.clone()));
        }
        let survivors: Vec<PlayerId> = self
            .state
            .players
            .iter()
            .filter(|p| !p.currently_eliminated)
            .map(|p| p.player_id)
            .collect();
        let eliminated: Vec<PlayerId> = self
            .state
            .players
            .iter()
            .filter(|p| p.currently_eliminated)
            .map(|p| p.player_id)
            .collect();
        let sides =
            scoring::sides(self.config.player_count, &self.config.multiplayer).unwrap_or_default();
        let surviving_sides = sides
            .into_iter()
            .filter(|(_, ps)| ps.iter().any(|p| survivors.contains(p)))
            .map(|(side, _)| side)
            .collect();
        let outcome = Outcome {
            kind: scoring::classify(self.config.player_count, &survivors)
                .unwrap_or(OutcomeKind::Stalemate),
            stop_reason,
            terminal_state_tick: tick,
            last_progress_tick: self.state.last_progress_tick,
            survivors,
            eliminated,
            surviving_sides,
            survival_transitions: self.state.survival_transitions.clone(),
        };
        RunResult {
            outcome,
            final_hash: self.hash_state().unwrap_or_default(),
            command_outcomes: std::mem::take(&mut self.command_outcomes),
            sim_duration_ms: self.sim_start.elapsed().as_millis() as u64,
            final_state: self.state,
        }
    }

    fn activity_of(&self, e: &EntityState) -> Activity {
        match &e.action {
            Order::Mine { .. } => Activity::Mining,
            Order::Construct { .. } => Activity::Construction,
            _ if e.engaged_target.is_some() => Activity::Combat,
            Order::AttackMove { .. } | Order::Support { .. } if !e.goal_settled => {
                Activity::Movement
            }
            _ => Activity::Idle,
        }
    }

    pub fn sample(&self) -> Sample {
        let entities = self
            .state
            .entities
            .iter()
            .map(|e| SampleEntity {
                silo: e.production.as_ref().and_then(|p| p.silo.clone()),
                index: self.dictionary[&e.id],
                tile: e.tile,
                hp: e.hp,
                facing: e.last_move_direction,
                lifecycle: e.lifecycle,
                activity: self.activity_of(e),
                engaged: e
                    .engaged_target
                    .as_ref()
                    .and_then(|id| self.dictionary.get(id).copied()),
            })
            .collect();
        Sample {
            missiles: self.state.missiles.clone(),
            recon: self.state.recon.clone(),
            tick: self.state.tick,
            players: self
                .state
                .players
                .iter()
                .map(|p| SamplePlayer {
                    player_id: p.player_id,
                    bank: p.bank,
                    currently_eliminated: p.currently_eliminated,
                })
                .collect(),
            entities,
            ore: self
                .state
                .ore
                .iter()
                .enumerate()
                .filter(|(_, v)| **v > 0.0)
                .map(|(index, v)| OreCell {
                    index: index as u32,
                    remaining: *v,
                })
                .collect(),
        }
    }

    pub fn stats(&self) -> StatsSample {
        let players = self
            .state
            .players
            .iter()
            .map(|p| {
                let mut s = PlayerStats {
                    player_id: p.player_id,
                    bank: p.bank,
                    counters: p.counters.clone(),
                    living_army_value: 0.0,
                    living_infrastructure_value: 0.0,
                    entity_count: 0,
                    active_workers: 0,
                };
                for (i, e) in self.state.entities.iter().enumerate() {
                    if e.owner != p.player_id {
                        continue;
                    }
                    s.entity_count += 1;
                    let def = self.def(i);
                    if e.lifecycle == Lifecycle::Complete {
                        if def.kind == TypeKind::Unit {
                            s.living_army_value += def.matter_cost;
                        } else {
                            s.living_infrastructure_value += def.matter_cost;
                            if let Some(silo) = e.production.as_ref().and_then(|p| p.silo.as_ref())
                            {
                                s.living_infrastructure_value += silo
                                    .inventory
                                    .iter()
                                    .map(|stock| {
                                        self.content.types
                                            [self.type_index(&stock.type_key).unwrap()]
                                        .matter_cost
                                            * f64::from(stock.count)
                                    })
                                    .sum::<f64>();
                            }
                        }
                    }
                    if matches!(e.action, Order::Mine { .. } | Order::Construct { .. }) {
                        s.active_workers += 1;
                    }
                }
                s
            })
            .collect();
        StatsSample {
            tick: self.state.tick,
            players,
        }
    }

    fn accumulate_timeline(&mut self) {
        for (p, counts) in self.activity.iter().enumerate() {
            for (a, c) in counts.iter().enumerate() {
                self.bucket_acc[p][a] = self.bucket_acc[p][a].max(*c);
            }
        }
    }

    fn flush_timeline(&mut self, emit: &mut dyn FnMut(Output), to_tick: Tick) {
        let mut buckets = vec![];
        for (p, counts) in self.bucket_acc.iter_mut().enumerate() {
            for (a, c) in counts.iter_mut().enumerate() {
                if *c > 0 {
                    buckets.push(TimelineBucket {
                        from_tick: self.bucket_from,
                        to_tick_exclusive: to_tick,
                        player_id: p as u8,
                        activity: [
                            Activity::Combat,
                            Activity::Construction,
                            Activity::Mining,
                            Activity::Movement,
                            Activity::Idle,
                        ][a],
                        affected_entities: *c,
                        severity: f64::from(*c),
                    });
                }
                *c = 0;
            }
        }
        self.bucket_from = to_tick;
        if !buckets.is_empty() {
            emit(Output::Timeline(buckets));
        }
    }
}
