//! Match controller: lobby slots, simultaneous planning rounds, revision store, scoring and
//! publication. Held behind a mutex; never awaited while locked.
use crate::adapter::{Job, OUT_CAPACITY, SimThread};
use crate::archive::{Archive, Manifest, ResultSizes, RoundRecord};
use atemporal_sim::*;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};

pub const TIMELINE_BUCKET: Tick = 20;
const EXACT_CACHE: usize = 64;

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
}

#[derive(Serialize, Clone, Debug)]
pub struct RoundMeasurement {
    pub round: u32,
    pub revision: Revision,
    pub base_tick: Tick,
    pub terminal_tick: Tick,
    pub sim_ms: u64,
    pub commit_to_job_ms: u64,
    pub job_to_complete_ms: u64,
    pub persist_ms: u64,
    pub commit_to_publish_ms: u64,
    pub sizes: ResultSizes,
    pub timeline_index_bytes: usize,
}

pub struct Controller {
    pub instance_id: String,
    pub match_id: String,
    pub content: Content,
    pub content_yaml: String,
    pub config_yaml: String,
    pub config: MatchConfig,
    pub fingerprint: Fingerprint,
    pub guide_url: String,
    pub lobby: LobbyState,
    tokens: Vec<Option<String>>,
    connections: Vec<u32>,
    pub phase: Phase,
    pub revisions: BTreeMap<Revision, RevisionData>,
    pub current: Revision,
    pub round: u32,
    committed: BTreeMap<PlayerId, (String, AcceptedTurn)>,
    ready_at: BTreeMap<PlayerId, Instant>,
    planning_opened_at: Instant,
    last_commit_at: Option<Instant>,
    pub time_totals: Vec<u64>,
    pub scores: Vec<RoundScore>,
    pub timed: Option<TimedAdjudication>,
    pub editable_from: Tick,
    archive: Option<Archive>,
    replay_root: PathBuf,
    exact_cache: VecDeque<((Revision, Tick), WorldState)>,
    pub broadcast: broadcast::Sender<ServerMessage>,
    pub sim: SimThread,
    running: Option<(String, Arc<AtomicBool>)>,
    pending: Option<RevisionData>,
    pub match_winners: Vec<SideId>,
    pub measurements: Vec<RoundMeasurement>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn token(seed: &str, n: usize) -> String {
    identity::sha256(format!("{seed}:{n}:{}", now_ms()).as_bytes())[..32].to_string()
}

impl Controller {
    pub fn new(
        setup: Setup,
        content: Content,
        config_yaml: String,
        content_yaml: String,
        guide_url: String,
        replay_root: PathBuf,
        sim: SimThread,
    ) -> Result<Self> {
        let config = setup.match_defaults.clone();
        let instance_id = token("instance", std::process::id() as usize);
        let fingerprint = Fingerprint {
            schema_version: Version::default(),
            sim_build: format!("atemporal-sim {}", env!("CARGO_PKG_VERSION")),
            target: std::env::consts::ARCH.to_string() + "-" + std::env::consts::OS,
            config_hash: identity::canonical_hash(&config)?,
            content_hash: atemporal_content::content_hash(&content)?,
        };
        let players = usize::from(config.player_count);
        let lobby = LobbyState {
            revision: 0,
            slots: (0..config.player_count)
                .map(|slot| LobbySlot {
                    slot,
                    claimed: false,
                    connected: false,
                    profile: None,
                })
                .collect(),
            available_teams: setup.available_teams.clone(),
            can_start: false,
            rule_summary: rule_summary(&config, &content),
        };
        let (broadcast, _) = broadcast::channel(256);
        Ok(Self {
            instance_id,
            match_id: format!("match-{}", now_ms()),
            content,
            content_yaml,
            config_yaml,
            config,
            fingerprint,
            guide_url,
            lobby,
            tokens: vec![None; players],
            connections: vec![0; players],
            phase: Phase::Lobby,
            revisions: BTreeMap::new(),
            current: 0,
            round: 0,
            committed: BTreeMap::new(),
            ready_at: BTreeMap::new(),
            planning_opened_at: Instant::now(),
            last_commit_at: None,
            time_totals: vec![0; players],
            scores: vec![],
            timed: None,
            editable_from: 0,
            archive: None,
            replay_root,
            exact_cache: VecDeque::new(),
            broadcast,
            sim,
            running: None,
            pending: None,
            match_winners: vec![],
            measurements: vec![],
        })
    }

    pub fn send(&self, message: ServerMessage) {
        let _ = self.broadcast.send(message);
    }

    // ---- lobby -------------------------------------------------------------------------------

    pub fn player_for(&self, token: &str) -> Option<PlayerId> {
        self.tokens
            .iter()
            .position(|t| t.as_deref() == Some(token))
            .map(|p| p as PlayerId)
    }

    fn bump_lobby(&mut self) {
        self.lobby.revision += 1;
        self.lobby.can_start =
            self.phase == Phase::Lobby && self.lobby.slots.iter().all(|s| s.claimed);
        self.send(ServerMessage::LobbyUpdated {
            lobby: self.lobby.clone(),
        });
    }

    fn validate_profile(username: &str, color: &str) -> Result<()> {
        let name = username.trim();
        if name.is_empty() || name.len() > 24 {
            return Err("username must be 1–24 characters".into());
        }
        if color.len() != 7
            || !color.starts_with('#')
            || !color[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("color must be #RRGGBB".into());
        }
        Ok(())
    }

    pub fn claim_slot(
        &mut self,
        slot: PlayerId,
        username: String,
        color: String,
        team_id: Option<TeamId>,
    ) -> Result<String> {
        if self.phase != Phase::Lobby {
            return Err("match already started; reconnect with your slot token".into());
        }
        let index = usize::from(slot);
        let Some(entry) = self.lobby.slots.get_mut(index) else {
            return Err("no such slot".into());
        };
        if entry.claimed {
            return Err("slot already claimed".into());
        }
        Self::validate_profile(&username, &color)?;
        if let Some(team) = &team_id
            && !self
                .lobby
                .available_teams
                .iter()
                .any(|t| t.team_id == *team)
        {
            return Err("unknown team".into());
        }
        entry.claimed = true;
        entry.connected = true;
        entry.profile = Some(PlayerProfile {
            player_id: slot,
            username: username.trim().to_string(),
            color,
            team_id,
        });
        let token = token(&self.instance_id, index);
        self.tokens[index] = Some(token.clone());
        self.connections[index] = 1;
        self.bump_lobby();
        Ok(token)
    }

    pub fn release_slot(&mut self, token: &str) -> Result<()> {
        if self.phase != Phase::Lobby {
            return Err("slots are pinned after start".into());
        }
        let player = self.player_for(token).ok_or("unknown slot token")?;
        let index = usize::from(player);
        self.tokens[index] = None;
        self.connections[index] = 0;
        let entry = &mut self.lobby.slots[index];
        entry.claimed = false;
        entry.connected = false;
        entry.profile = None;
        self.bump_lobby();
        Ok(())
    }

    pub fn update_profile(
        &mut self,
        token: &str,
        username: Option<String>,
        color: Option<String>,
        team_id: Option<TeamId>,
    ) -> Result<()> {
        if self.phase != Phase::Lobby {
            return Err("profiles are pinned after start".into());
        }
        let player = self.player_for(token).ok_or("unknown slot token")?;
        let profile = self.lobby.slots[usize::from(player)]
            .profile
            .as_mut()
            .ok_or("slot not claimed")?;
        let name = username.unwrap_or_else(|| profile.username.clone());
        let color = color.unwrap_or_else(|| profile.color.clone());
        Self::validate_profile(&name, &color)?;
        profile.username = name.trim().to_string();
        profile.color = color;
        if team_id.is_some() {
            profile.team_id = team_id;
        }
        self.bump_lobby();
        Ok(())
    }

    pub fn connect(&mut self, token: Option<&str>) -> Option<PlayerId> {
        let player = token.and_then(|t| self.player_for(t))?;
        let index = usize::from(player);
        self.connections[index] += 1;
        if !self.lobby.slots[index].connected {
            self.lobby.slots[index].connected = true;
            self.bump_lobby();
        }
        Some(player)
    }

    pub fn disconnect(&mut self, player: PlayerId) {
        let index = usize::from(player);
        self.connections[index] = self.connections[index].saturating_sub(1);
        if self.connections[index] == 0 && self.lobby.slots[index].connected {
            self.lobby.slots[index].connected = false;
            self.bump_lobby();
        }
    }

    pub fn welcome(&self) -> ServerMessage {
        ServerMessage::Welcome {
            match_id: self.match_id.clone(),
            config: self.config.clone(),
            phase: self.phase,
            lobby: self.lobby.clone(),
            fingerprint: self.fingerprint.clone(),
            guide_url: self.guide_url.clone(),
            timed: self.timed.clone(),
        }
    }

    pub fn time_totals(&self) -> Vec<PlayerTime> {
        self.time_totals
            .iter()
            .enumerate()
            .map(|(p, ms)| PlayerTime {
                player_id: p as u8,
                total_ms: (*ms).try_into().unwrap_or_default(),
            })
            .collect()
    }

    pub fn available_through(&self) -> Tick {
        let terminal = self
            .revisions
            .get(&self.current)
            .and_then(|r| r.outcome.as_ref())
            .map_or(0, |o| o.terminal_state_tick);
        terminal.min(self.config.max_tick - 1)
    }

    pub fn planning_opened(&self) -> ServerMessage {
        ServerMessage::PlanningOpened {
            round: self.round,
            revision: self.current,
            editable_from: self.editable_from,
            available_through: self.available_through(),
            committed_players: self.committed.keys().copied().collect(),
            time_totals: self.time_totals(),
        }
    }

    pub fn revision_published(&self, revision: Revision) -> Option<ServerMessage> {
        let data = self.revisions.get(&revision)?;
        Some(ServerMessage::RevisionPublished {
            revision,
            outcome: data.outcome.clone()?,
            timeline_index: data.timeline_index.clone(),
            score: data.score.clone(),
            timed: data.timed.clone(),
            time_totals: self.time_totals(),
            time_ratios: scoring::time_ratios(self.config.player_count, &self.time_totals())
                .unwrap_or_default(),
            sim_duration_ms: data.sim_duration_ms.try_into().unwrap_or_default(),
        })
    }

    // ---- match start -------------------------------------------------------------------------

    pub fn start_match(&mut self, token: &str, based_on_lobby_revision: Revision) -> Result<()> {
        if self.phase != Phase::Lobby {
            return Err("match already started".into());
        }
        let player = self
            .player_for(token)
            .ok_or("only a claimed slot can start")?;
        let first = self
            .lobby
            .slots
            .iter()
            .position(|s| s.claimed)
            .map(|p| p as u8);
        if first != Some(player) {
            return Err("only the first occupied slot may start".into());
        }
        if based_on_lobby_revision != self.lobby.revision {
            self.send(ServerMessage::LobbyUpdated {
                lobby: self.lobby.clone(),
            });
            return Err("lobby changed; review the roster and start again".into());
        }
        if !self.lobby.slots.iter().all(|s| s.claimed) {
            return Err("all slots must be claimed".into());
        }
        let initial = map::generate(&self.config, &self.content)?;
        let archive = Archive::create(&self.replay_root, &self.match_id)?;
        let manifest = Manifest {
            schema_version: SCHEMA_VERSION,
            match_id: self.match_id.clone(),
            server_instance_id: self.instance_id.clone(),
            fingerprint: self.fingerprint.clone(),
            config: self.config.clone(),
            profiles: self
                .lobby
                .slots
                .iter()
                .filter_map(|s| s.profile.clone())
                .collect(),
            initial_state: "initial-state.json".into(),
        };
        archive.write_manifest(&manifest, &self.config_yaml, &self.content_yaml, &initial)?;
        self.archive = Some(archive);
        self.phase = Phase::Simulating;
        self.lobby.can_start = false;
        self.send(ServerMessage::LobbyUpdated {
            lobby: self.lobby.clone(),
        });
        let mut genesis = RevisionData::new(0, 0, None, 0);
        genesis.checkpoints.insert(0, initial);
        self.revisions.insert(0, genesis);
        self.editable_from = 0;
        Ok(())
    }

    // ---- simulation jobs ---------------------------------------------------------------------

    pub fn build_request(
        &self,
        revision: Revision,
        checkpoint: WorldState,
        turns: Vec<AcceptedTurn>,
        precedence: Vec<RoundPrecedence>,
        minimum_end_tick: Tick,
    ) -> SimRequest {
        SimRequest {
            schema_version: Version::default(),
            job_id: format!("r{revision}-{}", now_ms()),
            revision,
            fingerprint: self.fingerprint.clone(),
            config: self.config.clone(),
            content: self.content.clone(),
            checkpoint,
            events: turns,
            precedence,
            end_tick_exclusive: self.config.max_tick,
            minimum_end_tick,
            entity_dictionary: self
                .revisions
                .get(&revision)
                .map(|r| r.dictionary.clone())
                .unwrap_or_default(),
        }
    }

    /// Start the job for a new revision; returns the worker message receiver.
    pub fn start_job(
        &mut self,
        round: u32,
        turns: Vec<AcceptedTurn>,
        precedence: Vec<RoundPrecedence>,
    ) -> Result<mpsc::Receiver<WorkerMessage>> {
        let parent_rev = self.current;
        let parent = self
            .revisions
            .get(&parent_rev)
            .ok_or("no parent revision")?;
        let earliest = turns
            .iter()
            .filter(|t| t.round == round)
            .map(|t| t.tick)
            .min()
            .unwrap_or(0);
        let (base_tick, checkpoint) = parent
            .checkpoints
            .range(..=earliest)
            .next_back()
            .map(|(t, s)| (*t, s.clone()))
            .ok_or("no checkpoint at or before the earliest new input")?;
        let revision = if round == 0 {
            0
        } else {
            self.revisions.keys().max().map_or(0, |r| r + 1)
        };
        let mut data = RevisionData::new(
            revision,
            round,
            (round > 0).then_some(parent_rev),
            base_tick,
        );
        // Reuse the unchanged prefix from the parent; the job regenerates everything from base_tick.
        data.dictionary = parent.dictionary.clone();
        data.samples = parent
            .samples
            .range(..base_tick)
            .map(|(t, s)| (*t, s.clone()))
            .collect();
        data.checkpoints = parent
            .checkpoints
            .range(..=base_tick)
            .map(|(t, s)| (*t, s.clone()))
            .collect();
        data.stats = parent
            .stats
            .range(..base_tick)
            .map(|(t, s)| (*t, s.clone()))
            .collect();
        data.events = parent
            .events
            .iter()
            .filter(|e| e.tick < base_tick)
            .cloned()
            .collect();
        data.timeline = parent
            .timeline
            .iter()
            .filter(|b| b.to_tick_exclusive <= base_tick)
            .cloned()
            .collect();
        // Preserve authoritative diagnostics for the unchanged checkpoint prefix.
        data.command_outcomes = parent
            .command_outcomes
            .iter()
            .filter(|outcome| {
                parent.turns.iter().any(|turn| {
                    turn.tick < base_tick
                        && turn
                            .commands
                            .iter()
                            .any(|command| command.id == outcome.command_id)
                })
            })
            .cloned()
            .collect();
        data.turns = turns.clone();
        data.precedence = precedence.clone();
        data.started = Some(Instant::now());
        let minimum_end_tick = match self.config.objective {
            Objective::Timed {
                lock_ticks_per_round,
            } => (self.editable_from + lock_ticks_per_round).min(self.config.max_tick),
            Objective::Scoreboard { .. } => 0,
        };
        let mut request =
            self.build_request(revision, checkpoint, turns, precedence, minimum_end_tick);
        request.entity_dictionary = data.dictionary.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel(OUT_CAPACITY);
        self.running = Some((request.job_id.clone(), cancel.clone()));
        self.pending = Some(data);
        self.phase = Phase::Simulating;
        self.sim.submit(Job::Run {
            request: Box::new(request),
            cancel,
            out: tx,
        });
        Ok(rx)
    }

    pub fn on_batch(&mut self, message: WorkerMessage) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        match message {
            WorkerMessage::Batch {
                job_id,
                dictionary,
                samples,
                checkpoints,
                stats,
                events,
                timeline,
                ..
            } => {
                if self.running.as_ref().is_none_or(|(id, _)| *id != job_id) {
                    return false;
                }
                pending.dictionary.extend(dictionary);
                for s in samples {
                    pending.samples.insert(s.tick, s);
                }
                for c in checkpoints {
                    pending.checkpoints.insert(c.tick, c);
                }
                for s in stats {
                    pending.stats.insert(s.tick, s);
                }
                pending.events.extend(events);
                pending.timeline.extend(timeline);
                true
            }
            WorkerMessage::Progress {
                job_id,
                revision,
                tick,
                end_tick,
            } => {
                if self.running.as_ref().is_none_or(|(id, _)| *id != job_id) {
                    return false;
                }
                self.send(ServerMessage::SimulationProgress {
                    revision,
                    tick,
                    end_tick,
                });
                true
            }
            _ => false,
        }
    }

    /// Take the completed revision out of the pending slot; the async publish path follows.
    pub fn on_complete(
        &mut self,
        job_id: &str,
        outcome: Outcome,
        final_hash: String,
        sim_duration_ms: u64,
        command_outcomes: Vec<CommandOutcome>,
    ) -> Option<RevisionData> {
        if self.running.as_ref().is_none_or(|(id, _)| *id != job_id) {
            return None;
        }
        self.running = None;
        let mut data = self.pending.take()?;
        data.outcome = Some(outcome);
        data.final_hash = final_hash;
        data.sim_duration_ms = sim_duration_ms;
        data.command_outcomes.extend(command_outcomes);
        data.timeline_index = timeline_index(&data.timeline, self.config.player_count);
        Some(data)
    }

    pub fn on_failed(&mut self, job_id: &str, message: &str) {
        if self.running.as_ref().is_none_or(|(id, _)| *id != job_id) {
            return;
        }
        eprintln!("simulation job {job_id} failed: {message}; last published revision retained");
        self.running = None;
        self.pending = None;
        self.committed.clear();
        if self.revisions.contains_key(&self.current) {
            self.phase = Phase::Planning;
            self.planning_opened_at = Instant::now();
            self.send(self.planning_opened());
        } else {
            self.phase = Phase::Lobby;
        }
    }

    /// Persist and publish a finished revision, then open the next planning round.
    pub fn publish(
        &mut self,
        mut data: RevisionData,
        timed_state: Option<WorldState>,
    ) -> Result<()> {
        let persist_start = Instant::now();
        let outcome = data.outcome.clone().ok_or("missing outcome")?;
        let round = data.round;
        if round > 0 {
            if let Objective::Scoreboard { rules } = &self.config.objective {
                let previous = self.scores.last();
                let score = scoring::resolve_round(
                    round,
                    self.config.player_count,
                    &self.config.multiplayer,
                    rules,
                    &outcome,
                    &self.time_totals(),
                    previous,
                )?;
                self.match_winners = score.match_winners.clone();
                self.scores.push(score.clone());
                data.score = Some(score);
            }
            if let Objective::Timed { .. } = &self.config.objective {
                let locked = timed_state.ok_or("timed adjudication requires the locked state")?;
                let previously_lost = self
                    .timed
                    .as_ref()
                    .map(|t| t.timed_lost_players.clone())
                    .unwrap_or_default();
                let timed = timed::adjudicate(
                    &self.config,
                    &self.content,
                    self.editable_from,
                    &locked,
                    &previously_lost,
                )?;
                self.editable_from = timed.boundary;
                self.match_winners = timed.match_winners.clone();
                data.timed = Some(timed.clone());
                self.timed = Some(timed);
            }
        }
        let mut sizes = ResultSizes::default();
        if let Some(archive) = &self.archive {
            sizes = archive.write_results(&data)?;
            if round > 0 {
                let precedence = data
                    .precedence
                    .iter()
                    .find(|p| p.round == round)
                    .cloned()
                    .unwrap_or(RoundPrecedence {
                        round,
                        players: vec![],
                    });
                archive.write_round(&RoundRecord {
                    round,
                    turns: data
                        .turns
                        .iter()
                        .filter(|t| t.round == round)
                        .map(|t| format!("turns/{}-{}.json", t.round, t.player))
                        .collect(),
                    precedence,
                    parent_revision: data.parent.unwrap_or(0),
                    revision: data.revision,
                    base_tick: data.base_tick,
                    editable_from: self.editable_from,
                    outcome: outcome.clone(),
                    final_hash: data.final_hash.clone(),
                    sim_duration_ms: data.sim_duration_ms,
                    score: data.score.clone(),
                    timed: data.timed.clone(),
                    command_outcomes: data.command_outcomes.clone(),
                    time_totals: self.time_totals(),
                })?;
            }
        }
        let persist_ms = persist_start.elapsed().as_millis() as u64;
        let revision = data.revision;
        let started = data.started.unwrap_or(persist_start);
        let commit_at = self.last_commit_at.unwrap_or(started);
        let measurement = RoundMeasurement {
            round,
            revision,
            base_tick: data.base_tick,
            terminal_tick: outcome.terminal_state_tick,
            sim_ms: data.sim_duration_ms,
            commit_to_job_ms: started.saturating_duration_since(commit_at).as_millis() as u64,
            job_to_complete_ms: persist_start.saturating_duration_since(started).as_millis() as u64,
            persist_ms,
            commit_to_publish_ms: commit_at.elapsed().as_millis() as u64,
            timeline_index_bytes: serde_json::to_vec(&data.timeline_index)
                .map(|v| v.len())
                .unwrap_or(0),
            sizes,
        };
        println!(
            "round {round} → revision {revision}: {}",
            serde_json::to_string(&measurement).unwrap_or_default()
        );
        if let Some(archive) = &self.archive {
            let _ = archive.append_measurement(&measurement);
        }
        self.measurements.push(measurement);
        self.revisions.insert(revision, data);
        self.current = revision;
        self.exact_cache.clear();
        if let Some(message) = self.revision_published(revision) {
            self.send(message);
        }
        if !self.match_winners.is_empty()
            || self
                .timed
                .as_ref()
                .is_some_and(|t| t.status != TimedStatus::Planning)
        {
            self.phase = Phase::Finished;
            self.send(ServerMessage::MatchFinished {
                match_winners: self.match_winners.clone(),
                final_outcome: outcome,
                reason: if self.match_winners.is_empty() {
                    "history_exhausted".into()
                } else {
                    "victory".into()
                },
            });
            return Ok(());
        }
        self.open_planning();
        Ok(())
    }

    fn open_planning(&mut self) {
        self.round += 1;
        self.committed.clear();
        self.ready_at.clear();
        self.planning_opened_at = Instant::now();
        self.last_commit_at = None;
        self.phase = Phase::Planning;
        self.send(self.planning_opened());
    }

    pub fn planning_ready(&mut self, player: PlayerId, round: u32) {
        if self.phase == Phase::Planning && round == self.round {
            self.ready_at.entry(player).or_insert_with(Instant::now);
        }
    }

    // ---- commits -----------------------------------------------------------------------------

    /// Cheap structural checks; ownership is validated against exact S[tick] by the caller.
    pub fn precheck_commit(
        &self,
        player: PlayerId,
        request: &CommitRequest,
    ) -> Result<Option<ServerMessage>> {
        if self.phase != Phase::Planning {
            return Err("planning is not open".into());
        }
        if let Some((request_id, _)) = self.committed.get(&player) {
            if *request_id == request.request_id {
                return Ok(Some(ServerMessage::CommitAccepted {
                    request_id: request.request_id.clone(),
                    round: self.round,
                }));
            }
            return Err("already committed this round".into());
        }
        let draft = &request.draft;
        if draft.based_on_revision != self.current {
            return Err(format!(
                "draft is based on revision {}, current is {}",
                draft.based_on_revision, self.current
            ));
        }
        if draft.tick < self.editable_from
            || draft.tick > self.available_through()
            || draft.tick >= self.config.max_tick
        {
            return Err(format!(
                "tick must be within {}..={}",
                self.editable_from,
                self.available_through()
            ));
        }
        if self.config.control_limit == ControlLimit::SingleOrder && draft.commands.len() > 1 {
            return Err("single-order mode allows one command per turn".into());
        }
        for command in &draft.commands {
            draft::validate_policy(command, self.config.future_orders.window_ticks)?;
        }
        draft::resolve_local_references(draft, self.round, player)?;
        Ok(None)
    }

    /// Ownership/capability/placement validation against the exact pre-tick state.
    pub fn validate_against_state(
        &self,
        player: PlayerId,
        commands: &[Command],
        state: &WorldState,
    ) -> Result<()> {
        let owned = |id: &EntityId| -> Result<&EntityState> {
            let e =
                state.entities.iter().find(|e| e.id == *id).ok_or_else(|| {
                    format!("entity {} does not exist at that tick", short_id(id))
                })?;
            if e.owner != player {
                return Err(format!("entity {} is not yours", short_id(id)));
            }
            Ok(e)
        };
        let def = |key: &str| self.content.types.iter().find(|t| t.key == key);
        for command in commands {
            match command {
                Command::AssignOrder { entities, order } => {
                    if entities.is_empty() {
                        return Err("assignment without recipients".into());
                    }
                    for id in entities {
                        let e = owned(id)?;
                        let d = def(&e.type_key).ok_or("unknown type")?;
                        let ok = match order {
                            Order::Idle {} => true,
                            Order::AttackMove { .. } | Order::Support { .. } => {
                                d.movement.is_some()
                            }
                            Order::Mine { .. } => d.mining.is_some(),
                            Order::Construct { .. } => d.construction.is_some(),
                        };
                        if !ok {
                            return Err(format!("{} cannot perform that order", e.type_key));
                        }
                    }
                }
                Command::AssignGroupOrder { group, .. }
                | Command::EditGroupMembers { group, .. } => {
                    if group.owner != player {
                        return Err("not your control group".into());
                    }
                }
                Command::BindFactoryGroup { factories, group } => {
                    if group.as_ref().is_some_and(|g| g.owner != player) {
                        return Err("not your control group".into());
                    }
                    for id in factories {
                        owned(id)?;
                    }
                }
                Command::SetPriority { entities, .. } => {
                    for id in entities {
                        if owned(id).is_err()
                            && !state
                                .blueprints
                                .iter()
                                .any(|b| b.id == *id && b.owner == player)
                        {
                            return Err("priority target is not yours".into());
                        }
                    }
                }
                Command::PlaceBlueprints {
                    type_key, tiles, ..
                } => {
                    let d = def(type_key).ok_or("unknown structure type")?;
                    if d.kind != TypeKind::Structure {
                        return Err("only structures can be placed".into());
                    }
                    if tiles.is_empty() {
                        return Err("no placement tiles".into());
                    }
                    for t in tiles {
                        if t.x >= state.terrain.width || t.y >= state.terrain.height {
                            return Err("placement outside the map".into());
                        }
                        let i =
                            usize::from(t.y) * usize::from(state.terrain.width) + usize::from(t.x);
                        if state.terrain.cells[i] != TerrainCell::Floor {
                            return Err("cannot place on rock".into());
                        }
                        if state.entities.iter().any(|e| {
                            e.tile == *t
                                && def(&e.type_key).is_some_and(|d| d.kind == TypeKind::Structure)
                        }) {
                            return Err("tile already holds a structure".into());
                        }
                    }
                }
                Command::CancelBlueprints { blueprint_ids } => {
                    for id in blueprint_ids {
                        if !state
                            .blueprints
                            .iter()
                            .any(|b| b.id == *id && b.owner == player)
                        {
                            return Err("blueprint is not yours or does not exist".into());
                        }
                    }
                }
                Command::EditProduction { factories, .. }
                | Command::SetQueueLoop { factories, .. }
                | Command::SetStoredOrder { factories, .. } => {
                    for id in factories {
                        let e = owned(id)?;
                        if def(&e.type_key).is_none_or(|d| d.production.is_none()) {
                            return Err("target is not a factory".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Durably record an accepted turn. Returns the receiver when the round closes.
    pub fn accept_commit(
        &mut self,
        player: PlayerId,
        request: &CommitRequest,
        commands: Vec<Command>,
    ) -> Result<(ServerMessage, Option<mpsc::Receiver<WorkerMessage>>)> {
        let now = Instant::now();
        let started = self
            .ready_at
            .get(&player)
            .copied()
            .unwrap_or(self.planning_opened_at);
        let duration = now.saturating_duration_since(started).as_millis() as u64;
        let turn = AcceptedTurn {
            player,
            round: self.round,
            tick: request.draft.tick,
            commands: request
                .draft
                .commands
                .iter()
                .zip(commands)
                .enumerate()
                .map(|(index, (draft, command))| CommittedCommand {
                    id: identity::command_id(self.round, player, index as u32),
                    command,
                    future_orders: draft.future_orders,
                })
                .collect(),
            duration_ms: duration.try_into().unwrap_or_default(),
        };
        if let Some(archive) = &self.archive {
            archive.write_turn(&turn)?;
        }
        self.time_totals[usize::from(player)] += duration;
        self.committed
            .insert(player, (request.request_id.clone(), turn));
        let accepted = ServerMessage::CommitAccepted {
            request_id: request.request_id.clone(),
            round: self.round,
        };
        self.send(self.planning_opened());
        if self.committed.len() < usize::from(self.config.player_count) {
            return Ok((accepted, None));
        }
        self.last_commit_at = Some(now);
        let parent = self
            .revisions
            .get(&self.current)
            .ok_or("missing current revision")?;
        let mut turns = parent.turns.clone();
        turns.extend(self.committed.values().map(|(_, t)| t.clone()));
        let mut precedence = parent.precedence.clone();
        precedence.push(identity::round_precedence(
            self.round,
            self.config.player_count,
        )?);
        let rx = self.start_job(self.round, turns, precedence)?;
        Ok((accepted, Some(rx)))
    }

    // ---- inspection --------------------------------------------------------------------------

    pub fn revision(&self, revision: Revision) -> Result<&RevisionData> {
        self.revisions
            .get(&revision)
            .ok_or_else(|| format!("unknown revision {revision}"))
    }

    pub fn snapshot_range(
        &self,
        revision: Revision,
        from: Tick,
        to: Tick,
        stride: Tick,
    ) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        let stride = stride.max(self.config.snapshot_interval);
        let samples = data
            .samples
            .range(from..=to)
            .filter(|(t, _)| (*t - from.min(**t)).is_multiple_of(stride) || **t == to)
            .map(|(_, s)| s.clone())
            .collect();
        Ok(ServerMessage::SnapshotRange {
            revision,
            index_width: if data.dictionary.len() <= usize::from(u16::MAX) {
                16
            } else {
                32
            },
            entity_dictionary: data.dictionary.clone(),
            samples,
        })
    }

    pub fn stats_range(
        &self,
        revision: Revision,
        from: Tick,
        to: Tick,
        bucket_width: Tick,
    ) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        let width = bucket_width.max(self.config.snapshot_interval);
        Ok(ServerMessage::StatsRange {
            revision,
            buckets: data
                .stats
                .range(from..=to)
                .filter(|(t, _)| (**t).is_multiple_of(width) || **t == to)
                .map(|(_, s)| s.clone())
                .collect(),
        })
    }

    pub fn round_result(&self, revision: Revision) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        let mut totals = BTreeMap::<PlayerId, u64>::new();
        for turn in &data.turns {
            *totals.entry(turn.player).or_default() += turn.duration_ms.get();
        }
        Ok(ServerMessage::RoundResult {
            revision,
            round: data.round,
            parent_revision: data.parent,
            outcome: data.outcome.clone().ok_or("revision is not published")?,
            timeline_index: data.timeline_index.clone(),
            score: data.score.clone(),
            timed: data.timed.clone(),
            time_totals: totals
                .into_iter()
                .map(|(player_id, duration_ms)| PlayerTime {
                    player_id,
                    total_ms: duration_ms.try_into().unwrap_or_default(),
                })
                .collect(),
            sim_duration_ms: data.sim_duration_ms.try_into().unwrap_or_default(),
            command_outcomes: data.command_outcomes.clone(),
        })
    }

    pub fn commands_range(
        &self,
        revision: Revision,
        from: Tick,
        to: Tick,
    ) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        Ok(ServerMessage::Commands {
            revision,
            turns: data
                .turns
                .iter()
                .filter(|t| t.tick >= from && t.tick <= to)
                .cloned()
                .collect(),
        })
    }

    pub fn events_range(&self, revision: Revision, from: Tick, to: Tick) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        Ok(ServerMessage::Events {
            revision,
            events: data
                .events
                .iter()
                .filter(|e| e.tick >= from && e.tick <= to)
                .cloned()
                .collect(),
        })
    }

    pub fn cached_exact(&self, revision: Revision, tick: Tick) -> Option<WorldState> {
        if let Some(s) = self
            .revisions
            .get(&revision)
            .and_then(|r| r.checkpoints.get(&tick))
        {
            return Some(s.clone());
        }
        self.exact_cache
            .iter()
            .find(|(k, _)| *k == (revision, tick))
            .map(|(_, s)| s.clone())
    }

    pub fn exact_request(&self, revision: Revision, tick: Tick) -> Result<SimRequest> {
        let data = self.revision(revision)?;
        let terminal = data
            .outcome
            .as_ref()
            .map_or(tick, |o| o.terminal_state_tick);
        if tick > terminal || tick > self.config.max_tick {
            return Err(format!(
                "tick {tick} is beyond this revision's end {terminal}"
            ));
        }
        let (_, checkpoint) = data
            .checkpoints
            .range(..=tick)
            .next_back()
            .ok_or("no checkpoint before tick")?;
        Ok(self.build_request(
            revision,
            checkpoint.clone(),
            data.turns.clone(),
            data.precedence.clone(),
            0,
        ))
    }

    pub fn remember_exact(&mut self, revision: Revision, tick: Tick, state: WorldState) {
        if self.exact_cache.len() >= EXACT_CACHE {
            self.exact_cache.pop_front();
        }
        self.exact_cache.push_back(((revision, tick), state));
    }

    pub fn stop_and_archive(
        &mut self,
        token: &str,
        request_id: String,
        based_on_revision: Revision,
    ) -> Result<ServerMessage> {
        let player = self
            .player_for(token)
            .ok_or("only a claimed slot can stop the match")?;
        if based_on_revision != self.current {
            return Err("stale revision".into());
        }
        if let Some((_, cancel)) = &self.running {
            cancel.store(true, Ordering::Relaxed);
        }
        self.phase = Phase::Archived;
        let archive = ArchiveRecord {
            request_id,
            revision: self.current,
            status: ArchiveStatus::Unfinished,
            reason: ArchiveReason::ManualStop,
            actor: Some(player),
            stopped_at_unix_ms: now_ms().try_into().unwrap_or_default(),
        };
        if let Some(a) = &self.archive {
            a.append_measurement(&archive)?;
        }
        let message = ServerMessage::MatchArchived { archive };
        self.send(message.clone());
        Ok(message)
    }
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
        }
    }
}

fn short_id(id: &EntityId) -> String {
    format!(
        "r{}p{}i{}/{}#{}.{}",
        id.birth_command.command.round,
        id.birth_command.command.player,
        id.birth_command.command.index,
        id.birth_command.target_index,
        id.item_index,
        id.occurrence
    )
}

fn rule_summary(config: &MatchConfig, content: &Content) -> String {
    let miner_rate = content
        .types
        .iter()
        .filter_map(|t| t.mining.as_ref())
        .map(|m| m.rate / f64::from(m.cooldown))
        .fold(0.0, f64::max);
    let depletion = if miner_rate > 0.0 {
        (config.ore_matter_per_start / miner_rate).round()
    } else {
        0.0
    };
    let objective = match &config.objective {
        Objective::Scoreboard { rules } => match rules.victory_rule {
            VictoryRule::FixedTarget { points } => {
                format!("scoreboard: first to {points} points, every resolved round scores")
            }
            VictoryRule::Lead { margin } => {
                format!("scoreboard: lead every rival by {margin} points")
            }
        },
        Objective::Timed {
            lock_ticks_per_round,
        } => format!("timed: {lock_ticks_per_round} ticks lock per round"),
    };
    format!(
        "{} players, {}×{} map, {objective}; inactivity stop after {} quiet ticks, cap {} ticks; ore {} per start ≈ {} ticks of one uninterrupted miner (estimate, not exclusive ownership)",
        config.player_count,
        config.map_size,
        config.map_size,
        config.stall_ticks,
        config.max_tick,
        config.ore_matter_per_start,
        depletion
    )
}

/// Dominant activity per player per coarse bucket: combat > construction > mining > movement > idle.
pub fn timeline_index(buckets: &[TimelineBucket], players: u8) -> Vec<TimelineBucket> {
    let mut merged: BTreeMap<(Tick, PlayerId), [u32; 5]> = BTreeMap::new();
    for b in buckets {
        let key = (b.from_tick / TIMELINE_BUCKET * TIMELINE_BUCKET, b.player_id);
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
                to_tick_exclusive: from + TIMELINE_BUCKET,
                player_id,
                activity: a,
                affected_entities: c,
                severity: f64::from(c),
            })
        })
        .collect()
}
