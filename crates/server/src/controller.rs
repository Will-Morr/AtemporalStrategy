//! Match controller: lobby slots, simultaneous planning rounds, revision store, scoring and
//! publication. Held behind a mutex; never awaited while locked.
use crate::adapter::{Job, OUT_CAPACITY, SimThread};
use crate::archive::{
    Archive, Loaded, LobbyRecord, Manifest, ResultSizes, RoundRecord, fail_point,
};
use atemporal_sim::*;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};

pub const TIMELINE_BUCKET: Tick = 20;
/// Published index entries per player; cap-length runs widen buckets instead of growing.
pub const TIMELINE_INDEX_PER_PLAYER: Tick = 500;
/// Most stats buckets one `get_stats` answer carries; wider windows coarsen automatically.
pub const MAX_STAT_BUCKETS: Tick = 2000;
const EXACT_CACHE: usize = 64;
pub const DEFAULT_MEMORY_BUDGET: u64 = 512 << 20;
pub const DEFAULT_DISK_BUDGET: u64 = 2048 << 20;

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
    pub resident_bytes: u64,
    pub evictions: u64,
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
    worker_retries: u8,
    pub match_winners: Vec<SideId>,
    pub measurements: Vec<RoundMeasurement>,
    pub memory_budget: u64,
    pub disk_budget: u64,
    use_counter: u64,
    pub evictions: u64,
}

/// Build/target/schema/config/content identity; a peripheral must reproduce it exactly.
pub fn fingerprint(config: &MatchConfig, content: &Content) -> Result<Fingerprint> {
    Ok(Fingerprint {
        schema_version: Version::default(),
        sim_build: format!("atemporal-sim {}", env!("CARGO_PKG_VERSION")),
        target: std::env::consts::ARCH.to_string() + "-" + std::env::consts::OS,
        config_hash: identity::canonical_hash(config)?,
        content_hash: atemporal_content::content_hash(content)?,
    })
}

pub fn now_ms() -> u64 {
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
        let fingerprint = fingerprint(&config, &content)?;
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
            worker_retries: 0,
            match_winners: vec![],
            measurements: vec![],
            memory_budget: DEFAULT_MEMORY_BUDGET,
            disk_budget: DEFAULT_DISK_BUDGET,
            use_counter: 0,
            evictions: 0,
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
        self.lobby.can_start = self.phase == Phase::Lobby && self.roster_multiplayer().is_ok();
        self.send(ServerMessage::LobbyUpdated {
            lobby: self.lobby.clone(),
        });
    }

    /// Pinned multiplayer mode from the roster: FFA without configured teams, otherwise every
    /// slot's chosen team must form at least two nonempty sides.
    fn roster_multiplayer(&self) -> Result<Multiplayer> {
        if !self.lobby.slots.iter().all(|s| s.claimed) {
            return Err("all slots must be claimed".into());
        }
        if self.lobby.available_teams.is_empty() {
            return Ok(Multiplayer::Ffa {});
        }
        let mut assignments = vec![];
        for slot in &self.lobby.slots {
            let profile = slot.profile.as_ref().ok_or("all slots must be claimed")?;
            assignments.push(TeamAssignment {
                player_id: slot.slot,
                team_id: profile
                    .team_id
                    .clone()
                    .ok_or_else(|| format!("slot {} has not chosen a team", slot.slot))?,
            });
        }
        let mode = Multiplayer::Teams { assignments };
        scoring::sides(self.config.player_count, &mode)?;
        Ok(mode)
    }

    /// Team choice validated against config/teams.yaml: known team with free capacity.
    fn validate_team(&self, slot: PlayerId, team_id: Option<&TeamId>) -> Result<()> {
        if self.lobby.available_teams.is_empty() {
            return match team_id {
                Some(_) => Err("this match has no teams".into()),
                None => Ok(()),
            };
        }
        let team_id = team_id.ok_or("choose a team")?;
        let team = self
            .lobby
            .available_teams
            .iter()
            .find(|t| t.team_id == *team_id)
            .ok_or("unknown team")?;
        let members = self
            .lobby
            .slots
            .iter()
            .filter(|s| s.slot != slot)
            .filter(|s| {
                s.profile
                    .as_ref()
                    .is_some_and(|p| p.team_id.as_ref() == Some(team_id))
            })
            .count();
        if team.capacity.is_some_and(|cap| members >= usize::from(cap)) {
            return Err(format!("team {} is full", team.label));
        }
        Ok(())
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
        let Some(entry) = self.lobby.slots.get(index) else {
            return Err("no such slot".into());
        };
        if entry.claimed {
            return Err("slot already claimed".into());
        }
        Self::validate_profile(&username, &color)?;
        self.validate_team(slot, team_id.as_ref())?;
        let entry = &mut self.lobby.slots[index];
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
        if team_id.is_some() {
            self.validate_team(player, team_id.as_ref())?;
        }
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
        // Team choices become the pinned assignments; the fingerprint follows the pinned config.
        self.config.multiplayer = self.roster_multiplayer()?;
        self.fingerprint.config_hash = identity::canonical_hash(&self.config)?;
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
        archive.write_lobby(&LobbyRecord {
            tokens: self.tokens.clone(),
        })?;
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
        let minimum_end_tick = minimum_end_tick(&self.config, self.editable_from);
        let mut request =
            self.build_request(revision, checkpoint, turns, precedence, minimum_end_tick);
        request.entity_dictionary = data.dictionary.clone();
        request
            .job_id
            .push_str(&format!("-retry{}", self.worker_retries));
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
                fail_point("during_job", pending.round);
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

    /// Retry a panicked worker once from the durable inputs and an untouched published base.
    pub fn retry_worker_panic(
        &mut self,
        job_id: &str,
    ) -> Result<Option<mpsc::Receiver<WorkerMessage>>> {
        if self.running.as_ref().is_none_or(|(id, _)| id != job_id) || self.worker_retries > 0 {
            return Ok(None);
        }
        self.worker_retries += 1;
        if let Some((_, cancel)) = self.running.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.pending = None;
        eprintln!("retrying panicked worker from durable round {}", self.round);
        if self.round == 0 {
            self.start_job(0, vec![], vec![]).map(Some)
        } else {
            self.close_round().map(Some)
        }
    }

    pub fn on_failed(&mut self, job_id: &str, message: &str) {
        if self.running.as_ref().is_none_or(|(id, _)| *id != job_id) {
            return;
        }
        eprintln!("simulation job {job_id} failed: {message}");
        self.running = None;
        self.pending = None;
        self.reopen_round();
    }

    /// Persisting or publishing failed (for example a full disk): the last published revision
    /// stays current and the round reopens for this round's players to commit again. Their
    /// durable turns remain on disk, so a restart with --resume replays them instead.
    pub fn recover_after_failed_publish(&mut self, message: &str) {
        eprintln!("publish failed: {message}; last published revision retained");
        self.reopen_round();
    }

    fn reopen_round(&mut self) {
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
            data.bytes = sizes.total();
            fail_point("after_results", round);
            let precedence = data
                .precedence
                .iter()
                .find(|p| p.round == round)
                .cloned()
                .unwrap_or(RoundPrecedence {
                    round,
                    players: vec![],
                });
            // The complete round record is the recovery commit point; publication follows it.
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
                timeline_index: data.timeline_index.clone(),
            })?;
            fail_point("before_publish", round);
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
            resident_bytes: self.resident_bytes() + data.bytes,
            evictions: self.evictions,
        };
        println!(
            "round {round} → revision {revision}: {}",
            serde_json::to_string(&measurement).unwrap_or_default()
        );
        if let Some(archive) = &self.archive {
            let _ = archive.append_measurement(&measurement);
        }
        self.measurements.push(measurement);
        data.bytes = if data.bytes == 0 {
            data.estimate_bytes()
        } else {
            data.bytes
        };
        data.last_used = self.next_use();
        self.revisions.insert(revision, data);
        self.current = revision;
        self.exact_cache.clear();
        self.enforce_budgets(None);
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
            if self.match_winners.is_empty() {
                // Locked history reached max_tick undecided: unfinished, no invented result.
                let archive = ArchiveRecord {
                    request_id: format!("history-exhausted-{revision}"),
                    revision,
                    status: ArchiveStatus::Unfinished,
                    reason: ArchiveReason::HistoryExhausted,
                    actor: None,
                    stopped_at_unix_ms: now_ms().try_into().unwrap_or_default(),
                };
                if let Some(a) = &self.archive {
                    a.write_archive_record(&archive)?;
                }
                self.send(ServerMessage::MatchArchived { archive });
            }
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
        self.worker_retries = 0;
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
        // A retry answers from the recorded turn even after the round closed.
        if let Some((request_id, _)) = self.committed.get(&player) {
            if *request_id == request.request_id {
                return Ok(Some(ServerMessage::CommitAccepted {
                    request_id: request.request_id.clone(),
                    round: self.round,
                }));
            }
            return Err("already committed this round".into());
        }
        if self.phase != Phase::Planning {
            return Err("planning is not open".into());
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
                        if !ok && d.production.is_none() {
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
                Command::ConfigureBlueprints {
                    blueprint_ids,
                    settings,
                } => {
                    if settings.queue.len() > 65536 {
                        return Err("too many queued recipes".into());
                    }
                    for id in blueprint_ids {
                        let key = state
                            .blueprints
                            .iter()
                            .find(|b| b.id == *id && b.owner == player && b.site_id.is_none())
                            .map(|b| &b.type_key)
                            .or_else(|| {
                                commands
                                    .get(id.birth_command.command.index as usize)
                                    .and_then(|c| match c {
                                        Command::PlaceBlueprints {
                                            type_key, tiles, ..
                                        } if id.birth_command.command.player == player
                                            && id.birth_command.command.round == self.round
                                            && usize::from(id.item_index) < tiles.len() =>
                                        {
                                            Some(type_key)
                                        }
                                        _ => None,
                                    })
                            })
                            .ok_or("blueprint is not yours or does not exist")?;
                        let d = def(key).ok_or("unknown structure")?;
                        if !settings.queue.is_empty()
                            && d.production.as_ref().is_none_or(|p| {
                                settings.queue.iter().any(|k| !p.recipes.contains(k))
                            })
                        {
                            return Err("invalid blueprint recipe".into());
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
            archive.write_turn(&turn, &request.request_id)?;
        }
        fail_point("after_turn_written", self.round);
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
        Ok((accepted, Some(self.close_round()?)))
    }

    /// Every player committed: run the round on the ledger so far.
    fn close_round(&mut self) -> Result<mpsc::Receiver<WorkerMessage>> {
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
        self.start_job(self.round, turns, precedence)
    }

    /// After a resume: a fully committed round whose record is missing is simulated again.
    pub fn restart_pending_round(&mut self) -> Result<Option<mpsc::Receiver<WorkerMessage>>> {
        if self.phase == Phase::Planning
            && self.committed.len() == usize::from(self.config.player_count)
        {
            self.last_commit_at = Some(Instant::now());
            return self.close_round().map(Some);
        }
        Ok(None)
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
        Ok(self
            .revision(revision)?
            .snapshot_range(self.config.snapshot_interval, from, to, stride))
    }

    pub fn stats_range(
        &self,
        revision: Revision,
        from: Tick,
        to: Tick,
        bucket_width: Tick,
    ) -> Result<ServerMessage> {
        Ok(self.revision(revision)?.stats_range(
            self.config.snapshot_interval,
            from,
            to,
            bucket_width,
        ))
    }

    pub fn round_result(&self, revision: Revision) -> Result<ServerMessage> {
        self.revision(revision)?
            .round_result(self.config.player_count)
    }

    pub fn commands_range(
        &self,
        revision: Revision,
        from: Tick,
        to: Tick,
    ) -> Result<ServerMessage> {
        Ok(self.revision(revision)?.commands_range(from, to))
    }

    pub fn events_range(&self, revision: Revision, from: Tick, to: Tick) -> Result<ServerMessage> {
        Ok(self.revision(revision)?.events_range(from, to))
    }

    // ---- inputs-only peripheral --------------------------------------------------------------

    /// Initialization data for a trusted peripheral: pinned identity plus the current ledger.
    pub fn replay_bootstrap(&self) -> Result<ServerMessage> {
        let initial = self
            .revisions
            .get(&0)
            .and_then(|g| g.checkpoints.get(&0))
            .ok_or("match has not started")?;
        let current = self.revision(self.current)?;
        Ok(ServerMessage::ReplayBootstrap {
            fingerprint: self.fingerprint.clone(),
            config: self.config.clone(),
            content: self.content.clone(),
            initial_state: Box::new(initial.clone()),
            ledger: current.turns.clone(),
            precedence: current.precedence.clone(),
        })
    }

    /// The turns a revision's round added and the lock boundary its job ran under, which is the
    /// parent's timed boundary (0 without one) and fixes `minimum_end_tick` for reproduction.
    pub fn round_inputs(&self, revision: Revision) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        let round = data.round;
        let previous_boundary = data
            .parent
            .and_then(|p| self.revisions.get(&p))
            .and_then(|p| p.timed.as_ref())
            .map_or(0, |t| t.boundary);
        Ok(ServerMessage::RoundInputs {
            revision,
            turns: data
                .turns
                .iter()
                .filter(|t| t.round == round)
                .cloned()
                .collect(),
            precedence: data
                .precedence
                .iter()
                .find(|p| p.round == round)
                .cloned()
                .unwrap_or(RoundPrecedence {
                    round,
                    players: vec![],
                }),
            editable_from: previous_boundary,
        })
    }

    pub fn reference_hash(&self, revision: Revision) -> Result<ServerMessage> {
        let data = self.revision(revision)?;
        let outcome = data.outcome.as_ref().ok_or("revision is not published")?;
        Ok(ServerMessage::ReferenceHash {
            revision,
            tick: outcome.terminal_state_tick,
            hash: data.final_hash.clone(),
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

    fn next_use(&mut self) -> u64 {
        self.use_counter += 1;
        self.use_counter
    }

    /// Make a revision's body resident for a query. `Ok(None)` means ready; `Ok(Some(request))`
    /// means the caller must regenerate it on the sim thread and `install_body` the result.
    pub fn ensure_loaded(&mut self, revision: Revision) -> Result<Option<SimRequest>> {
        let stamp = self.next_use();
        let data = self
            .revisions
            .get_mut(&revision)
            .ok_or_else(|| format!("unknown revision {revision}"))?;
        data.last_used = stamp;
        if data.loaded {
            return Ok(None);
        }
        if let Some(cache) = self.archive.as_ref().and_then(|a| a.read_results(revision)) {
            let bytes = self
                .archive
                .as_ref()
                .map(|a| {
                    a.results_usage()
                        .into_iter()
                        .find(|(r, _)| *r == revision)
                        .map_or(0, |(_, b)| b)
                })
                .unwrap_or(0);
            let data = self
                .revisions
                .get_mut(&revision)
                .ok_or("unknown revision")?;
            data.dictionary = cache.dictionary;
            data.samples = cache.samples.into_iter().map(|s| (s.tick, s)).collect();
            data.checkpoints = cache.checkpoints.into_iter().map(|c| (c.tick, c)).collect();
            data.stats = cache.stats.into_iter().map(|s| (s.tick, s)).collect();
            data.events = cache.events;
            data.timeline = cache.timeline;
            data.loaded = true;
            data.bytes = if bytes == 0 {
                data.estimate_bytes()
            } else {
                bytes
            };
            self.enforce_budgets(Some(revision));
            return Ok(None);
        }
        self.replay_request(revision).map(Some)
    }

    /// Drop a revision's regenerable body; the initial state and all metadata stay resident.
    fn evict_body(&mut self, revision: Revision) {
        let Some(data) = self.revisions.get_mut(&revision) else {
            return;
        };
        let initial = (revision == 0)
            .then(|| data.checkpoints.get(&0).cloned())
            .flatten();
        data.dictionary = vec![];
        data.samples = BTreeMap::new();
        data.checkpoints = initial
            .map(|s| BTreeMap::from([(0, s)]))
            .unwrap_or_default();
        data.stats = BTreeMap::new();
        data.events = vec![];
        data.timeline = vec![];
        data.loaded = false;
        self.exact_cache.retain(|((r, _), _)| *r != revision);
        self.evictions += 1;
    }

    /// Memory: evict least-recently-used bodies other than the current revision until the sum of
    /// resident bodies fits. Disk: remove the oldest result caches other than the current and
    /// pending revisions until `results/` fits. Turns and round records are never touched.
    pub fn enforce_budgets(&mut self, keep: Option<Revision>) {
        loop {
            let resident: u64 = self
                .revisions
                .values()
                .filter(|d| d.loaded)
                .map(|d| d.bytes)
                .sum();
            if resident <= self.memory_budget {
                break;
            }
            let victim = self
                .revisions
                .values()
                .filter(|d| d.loaded && d.revision != self.current && Some(d.revision) != keep)
                .min_by_key(|d| d.last_used)
                .map(|d| d.revision);
            match victim {
                Some(revision) => self.evict_body(revision),
                None => break,
            }
        }
        let Some(archive) = &self.archive else { return };
        let usage = archive.results_usage();
        let mut total: u64 = usage.iter().map(|(_, b)| b).sum();
        let pending = self.pending.as_ref().map(|p| p.revision);
        for (revision, bytes) in usage {
            if total <= self.disk_budget {
                break;
            }
            if revision == self.current || Some(revision) == pending {
                continue;
            }
            archive.remove_results(revision);
            total -= bytes;
        }
    }

    pub fn resident_bytes(&self) -> u64 {
        self.revisions
            .values()
            .filter(|d| d.loaded)
            .map(|d| d.bytes)
            .sum()
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
        // Same host role as starting: the first occupied slot operates the match.
        let host = self
            .lobby
            .slots
            .iter()
            .position(|s| s.claimed)
            .map(|p| p as u8);
        if host != Some(player) {
            return Err("only the first occupied slot may stop the match".into());
        }
        if !matches!(self.phase, Phase::Planning | Phase::Simulating) {
            return Err(format!("match is {:?}; nothing to stop", self.phase));
        }
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
            a.write_archive_record(&archive)?;
        }
        let message = ServerMessage::MatchArchived { archive };
        self.send(message.clone());
        Ok(message)
    }
}

impl Controller {
    /// Reopen an archived match: pinned config/content, lobby profiles and tokens, every complete
    /// round, the current revision's result cache (regenerated from the ledger when missing) and
    /// any partial round, which resumes planning with those commits already accepted.
    #[allow(clippy::too_many_arguments)]
    pub fn resume(
        setup: Setup,
        content: Content,
        guide_url: String,
        replay_root: PathBuf,
        sim: SimThread,
        match_id: String,
        archive: Archive,
        loaded: Loaded,
    ) -> Result<Self> {
        let mut c = Self::new(
            setup,
            content,
            loaded.config_yaml.clone(),
            loaded.content_yaml.clone(),
            guide_url,
            replay_root,
            sim,
        )?;
        c.match_id = match_id;
        c.config = loaded.manifest.config.clone();
        c.fingerprint.config_hash = identity::canonical_hash(&c.config)?;
        let recorded = &loaded.manifest.fingerprint;
        if recorded.content_hash != c.fingerprint.content_hash
            || recorded.config_hash != c.fingerprint.config_hash
            || recorded.schema_version != c.fingerprint.schema_version
        {
            return Err("archive was recorded under different content/config; refusing to replay it silently".into());
        }
        if recorded.sim_build != c.fingerprint.sim_build {
            eprintln!(
                "warning: archive recorded with {} but this server is {}; hashes are verified on regeneration",
                recorded.sim_build, c.fingerprint.sim_build
            );
        }
        let players = usize::from(c.config.player_count);
        c.lobby.rule_summary = rule_summary(&c.config, &c.content);
        for profile in &loaded.manifest.profiles {
            let slot = &mut c.lobby.slots[usize::from(profile.player_id)];
            slot.claimed = true;
            slot.profile = Some(profile.clone());
        }
        c.tokens = match loaded.tokens.clone() {
            Some(tokens) if tokens.len() == players => tokens,
            _ => {
                let tokens: Vec<_> = (0..players)
                    .map(|i| Some(token(&c.instance_id, i)))
                    .collect();
                println!("lobby.json missing; fresh slot tokens: {tokens:?}");
                archive.write_lobby(&LobbyRecord {
                    tokens: tokens.clone(),
                })?;
                tokens
            }
        };
        c.lobby.revision = 1;
        let mut genesis = RevisionData::new(0, 0, None, 0);
        genesis.checkpoints.insert(0, loaded.initial.clone());
        c.revisions.insert(0, genesis);
        let Some(last) = loaded.rounds.last() else {
            // Started but never published: rerun the opening like a fresh start.
            c.archive = Some(archive);
            c.phase = Phase::Simulating;
            return Ok(c);
        };
        for record in &loaded.rounds {
            let (turns, precedence) = ledger(&loaded, record.round)?;
            let parent = (record.round > 0).then_some(record.parent_revision);
            let mut data =
                RevisionData::new(record.revision, record.round, parent, record.base_tick);
            data.turns = turns;
            data.precedence = precedence;
            data.outcome = Some(record.outcome.clone());
            data.final_hash = record.final_hash.clone();
            data.sim_duration_ms = record.sim_duration_ms;
            data.command_outcomes = record.command_outcomes.clone();
            data.score = record.score.clone();
            data.timed = record.timed.clone();
            data.timeline_index = record.timeline_index.clone();
            data.loaded = false;
            if record.revision == 0 {
                // The initial state stays resident: every replay/regeneration starts from it.
                data.checkpoints.insert(0, loaded.initial.clone());
            }
            if let Some(score) = &record.score {
                c.scores.push(score.clone());
            }
            c.revisions.insert(record.revision, data);
        }
        c.current = last.revision;
        c.round = last.round + 1;
        c.editable_from = last.editable_from;
        c.timed = last.timed.clone();
        c.match_winners = last
            .score
            .as_ref()
            .map(|s| s.match_winners.clone())
            .or_else(|| last.timed.as_ref().map(|t| t.match_winners.clone()))
            .unwrap_or_default();
        c.time_totals = last.time_totals.iter().map(|t| t.total_ms.get()).collect();
        c.time_totals.resize(players, 0);
        c.archive = Some(archive);
        // The current revision is always resident; older bodies load or regenerate on demand.
        let started = Instant::now();
        let regenerated = c.load_body(c.current)?;
        println!(
            "resumed {}: {} rounds, current revision {} {} in {:?}",
            c.match_id,
            loaded.rounds.len(),
            c.current,
            if regenerated {
                "regenerated from the ledger"
            } else {
                "loaded from results/"
            },
            started.elapsed()
        );
        if let Some(archived) = &loaded.archived {
            c.phase = Phase::Archived;
            println!("match was archived: {archived:?}");
            return Ok(c);
        }
        let finished = !c.match_winners.is_empty()
            || c.timed
                .as_ref()
                .is_some_and(|t| t.status != TimedStatus::Planning);
        if finished {
            c.phase = Phase::Finished;
            return Ok(c);
        }
        for player in 0..c.config.player_count {
            if let Some(turn) = loaded.turns.get(&(c.round, player)) {
                let request_id = loaded
                    .request_ids
                    .get(&(c.round, player))
                    .cloned()
                    .unwrap_or_else(|| format!("recovered-{}-{player}", c.round));
                c.time_totals[usize::from(player)] += turn.duration_ms.get();
                c.committed.insert(player, (request_id, turn.clone()));
            }
        }
        c.phase = Phase::Planning;
        c.planning_opened_at = Instant::now();
        Ok(c)
    }

    /// Full replay request for a revision: from the initial state over the ledger through its round.
    pub fn replay_request(&self, revision: Revision) -> Result<SimRequest> {
        let data = self.revision(revision)?;
        let initial = self
            .revisions
            .get(&0)
            .and_then(|g| g.checkpoints.get(&0))
            .ok_or("initial state missing")?;
        let previous = data.parent.and_then(|p| self.revisions.get(&p));
        let previous_boundary = previous
            .and_then(|p| p.timed.as_ref())
            .map_or(0, |t| t.boundary);
        let mut request = self.build_request(
            revision,
            initial.clone(),
            data.turns.clone(),
            data.precedence.clone(),
            minimum_end_tick(&self.config, previous_boundary),
        );
        request.entity_dictionary = vec![];
        Ok(request)
    }

    /// Make a revision's body resident: from `results/` when complete, otherwise by replaying the
    /// ledger on this thread and checking the recorded hash. Returns whether it was regenerated.
    pub fn load_body(&mut self, revision: Revision) -> Result<bool> {
        if self.revision(revision)?.loaded {
            return Ok(false);
        }
        if let Some(cache) = self.archive.as_ref().and_then(|a| a.read_results(revision)) {
            let data = self
                .revisions
                .get_mut(&revision)
                .ok_or("unknown revision")?;
            data.dictionary = cache.dictionary;
            data.samples = cache.samples.into_iter().map(|s| (s.tick, s)).collect();
            data.checkpoints = cache.checkpoints.into_iter().map(|c| (c.tick, c)).collect();
            data.stats = cache.stats.into_iter().map(|s| (s.tick, s)).collect();
            data.events = cache.events;
            data.timeline = cache.timeline;
            data.loaded = true;
            return Ok(false);
        }
        let request = self.replay_request(revision)?;
        let mut fresh = RevisionData::new(revision, 0, None, 0);
        collect_run(&request, &mut fresh, self.config.player_count)?;
        self.install_body(revision, fresh)?;
        Ok(true)
    }

    /// Adopt a regenerated body after checking it reproduces the recorded final hash.
    pub fn install_body(&mut self, revision: Revision, fresh: RevisionData) -> Result<()> {
        let data = self
            .revisions
            .get_mut(&revision)
            .ok_or("unknown revision")?;
        if fresh.final_hash != data.final_hash {
            return Err(format!(
                "revision {revision} regenerated with hash {} but the archive recorded {}",
                fresh.final_hash, data.final_hash
            ));
        }
        data.dictionary = fresh.dictionary;
        data.samples = fresh.samples;
        data.checkpoints = fresh.checkpoints;
        data.stats = fresh.stats;
        data.events = fresh.events;
        data.timeline = fresh.timeline;
        data.timeline_index = fresh.timeline_index;
        data.loaded = true;
        data.bytes = data.estimate_bytes();
        if let Some(archive) = &self.archive {
            data.bytes = archive.write_results(data)?.total();
        }
        self.enforce_budgets(Some(revision));
        Ok(())
    }
}

/// Timed jobs cannot stop before the next lock boundary; scoreboard jobs may stop any time.
pub fn minimum_end_tick(config: &MatchConfig, editable_from: Tick) -> Tick {
    match config.objective {
        Objective::Timed {
            lock_ticks_per_round,
        } => (editable_from + lock_ticks_per_round).min(config.max_tick),
        Objective::Scoreboard { .. } => 0,
    }
}

/// Accepted turns and precedence for every round through `round`, in record order.
pub fn ledger(loaded: &Loaded, round: u32) -> Result<(Vec<AcceptedTurn>, Vec<RoundPrecedence>)> {
    let mut turns = vec![];
    let mut precedence = vec![];
    for record in loaded
        .rounds
        .iter()
        .filter(|r| r.round > 0 && r.round <= round)
    {
        for name in &record.turns {
            let stem = name.trim_start_matches("turns/").trim_end_matches(".json");
            let (r, p) = stem.split_once('-').ok_or("bad turn reference")?;
            let key = (
                r.parse().map_err(|_| "bad turn round")?,
                p.parse().map_err(|_| "bad turn player")?,
            );
            turns.push(loaded.turns.get(&key).ok_or("missing turn")?.clone());
        }
        precedence.push(record.precedence.clone());
    }
    Ok((turns, precedence))
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

/// `--verify`: replay every recorded round from the initial state and compare final hashes.
pub fn verify_archive(loaded: &Loaded, content: &Content) -> Result<bool> {
    let config = &loaded.manifest.config;
    let mut all_match = true;
    let mut previous_boundary = 0;
    for record in &loaded.rounds {
        let (turns, precedence) = ledger(loaded, record.round)?;
        let request = SimRequest {
            schema_version: Version::default(),
            job_id: format!("verify-{}", record.revision),
            revision: record.revision,
            fingerprint: loaded.manifest.fingerprint.clone(),
            config: config.clone(),
            content: content.clone(),
            checkpoint: loaded.initial.clone(),
            events: turns,
            precedence,
            end_tick_exclusive: config.max_tick,
            minimum_end_tick: minimum_end_tick(config, previous_boundary),
            entity_dictionary: vec![],
        };
        let started = Instant::now();
        let mut data = RevisionData::new(record.revision, record.round, None, 0);
        collect_run(&request, &mut data, config.player_count)?;
        let terminal = data.outcome.as_ref().map_or(0, |o| o.terminal_state_tick);
        let ok =
            data.final_hash == record.final_hash && terminal == record.outcome.terminal_state_tick;
        all_match &= ok;
        println!(
            "round {} revision {}: {} (terminal {} vs recorded {}, {:?}, hash {}…)",
            record.round,
            record.revision,
            if ok { "match" } else { "MISMATCH" },
            terminal,
            record.outcome.terminal_state_tick,
            started.elapsed(),
            &data.final_hash[..12.min(data.final_hash.len())]
        );
        previous_boundary = record.timed.as_ref().map_or(0, |t| t.boundary);
    }
    Ok(all_match)
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
            turns: self
                .turns
                .iter()
                .filter(|t| t.tick >= from && t.tick <= to)
                .cloned()
                .collect(),
        }
    }

    pub fn events_range(&self, from: Tick, to: Tick) -> ServerMessage {
        ServerMessage::Events {
            revision: self.revision,
            events: self
                .events
                .iter()
                .filter(|e| e.tick >= from && e.tick <= to)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn test_root(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "atemporal-server-test-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    pub fn controller(teams: bool, edit: impl FnOnce(&mut Setup)) -> Controller {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        let config_yaml = fs::read_to_string(dir.join("game.yaml")).unwrap();
        let content_yaml = fs::read_to_string(dir.join("content.yaml")).unwrap();
        let mut setup = atemporal_content::load_setup(&config_yaml).unwrap();
        if teams {
            setup.available_teams = ["cyan", "orange"]
                .iter()
                .map(|t| AvailableTeam {
                    team_id: t.to_string(),
                    label: t.to_uppercase(),
                    capacity: Some(1),
                })
                .collect();
        }
        edit(&mut setup);
        let content = atemporal_content::load_content(&content_yaml).unwrap();
        Controller::new(
            setup,
            content,
            config_yaml,
            content_yaml,
            "/guide/".into(),
            test_root("lobby"),
            SimThread::spawn(),
        )
        .unwrap()
    }

    fn claim(c: &mut Controller, slot: u8, team: Option<&str>) -> Result<String> {
        c.claim_slot(
            slot,
            format!("player{slot}"),
            "#4fc3f7".into(),
            team.map(str::to_string),
        )
    }

    #[test]
    fn teams_require_choice_capacity_and_two_sides() {
        let mut c = controller(true, |_| {});
        assert!(
            claim(&mut c, 0, None)
                .unwrap_err()
                .contains("choose a team")
        );
        assert!(
            claim(&mut c, 0, Some("pink"))
                .unwrap_err()
                .contains("unknown team")
        );
        let a = claim(&mut c, 0, Some("cyan")).unwrap();
        assert!(
            claim(&mut c, 1, Some("cyan"))
                .unwrap_err()
                .contains("is full")
        );
        assert!(!c.lobby.can_start);
        let b = claim(&mut c, 1, Some("orange")).unwrap();
        assert!(c.lobby.can_start);
        // Moving slot 1 onto slot 0's full team is rejected; swapping via a free team works.
        assert!(
            c.update_profile(&b, None, None, Some("cyan".into()))
                .is_err()
        );
        c.update_profile(&a, Some("Ada".into()), None, None)
            .unwrap();
        assert_eq!(c.lobby.slots[0].profile.as_ref().unwrap().username, "Ada");
        let before = c.fingerprint.config_hash.clone();
        c.start_match(&a, c.lobby.revision).unwrap();
        match &c.config.multiplayer {
            Multiplayer::Teams { assignments } => {
                assert_eq!(assignments.len(), 2);
                assert_eq!(assignments[1].team_id, "orange");
            }
            other => panic!("expected pinned teams, got {other:?}"),
        }
        assert_ne!(c.fingerprint.config_hash, before);
        assert!(
            c.update_profile(&a, Some("late".into()), None, None)
                .is_err()
        );
    }

    #[test]
    fn ffa_lobby_rejects_teams_and_pins_ffa() {
        let mut c = controller(false, |_| {});
        assert!(claim(&mut c, 0, Some("cyan")).is_err());
        let a = claim(&mut c, 0, None).unwrap();
        assert!(!c.lobby.can_start);
        claim(&mut c, 1, None).unwrap();
        assert!(c.lobby.can_start);
        c.start_match(&a, c.lobby.revision).unwrap();
        assert_eq!(c.config.multiplayer, Multiplayer::Ffa {});
        assert_eq!(c.phase, Phase::Simulating);
    }

    #[test]
    fn stale_start_is_rejected_with_a_roster_refresh() {
        let mut c = controller(false, |_| {});
        let a = claim(&mut c, 0, None).unwrap();
        let seen = c.lobby.revision;
        let b = claim(&mut c, 1, None).unwrap();
        let mut rx = c.broadcast.subscribe();
        let err = c.start_match(&a, seen).unwrap_err();
        assert!(err.contains("lobby changed"), "{err}");
        assert!(
            matches!(rx.try_recv(), Ok(ServerMessage::LobbyUpdated { lobby }) if lobby.revision == c.lobby.revision)
        );
        assert_eq!(c.phase, Phase::Lobby);
        assert!(
            c.start_match(&b, c.lobby.revision)
                .unwrap_err()
                .contains("first occupied")
        );
        assert!(c.update_profile(&a, Some(" ".into()), None, None).is_err());
        assert!(
            c.update_profile(&a, None, Some("red".into()), None)
                .is_err()
        );
        c.update_profile(&a, None, Some("#ff0000".into()), None)
            .unwrap();
        c.start_match(&a, c.lobby.revision).unwrap();
    }

    /// Drive one job to publication on this thread, like `ws::drive_job` without the runtime.
    pub fn run_round(c: &mut Controller, mut rx: mpsc::Receiver<WorkerMessage>) {
        while let Some(message) = rx.blocking_recv() {
            match message {
                WorkerMessage::Complete {
                    job_id,
                    outcome,
                    final_hash,
                    sim_duration_ms,
                    command_outcomes,
                    ..
                } => {
                    let data = c
                        .on_complete(
                            &job_id,
                            outcome,
                            final_hash,
                            sim_duration_ms.get(),
                            command_outcomes,
                        )
                        .expect("job is current");
                    let timed_state = match c.config.objective {
                        Objective::Timed {
                            lock_ticks_per_round,
                        } if data.round > 0 => {
                            let boundary =
                                (c.editable_from + lock_ticks_per_round).min(c.config.max_tick);
                            let (_, cp) = data.checkpoints.range(..=boundary).next_back().unwrap();
                            let request = c.build_request(
                                data.revision,
                                cp.clone(),
                                data.turns.clone(),
                                data.precedence.clone(),
                                0,
                            );
                            Some(atemporal_sim::reconstruct(&request, boundary).unwrap())
                        }
                        _ => None,
                    };
                    c.publish(data, timed_state).unwrap();
                    return;
                }
                WorkerMessage::Failed { message, .. } => panic!("job failed: {message}"),
                other => {
                    c.on_batch(other);
                }
            }
        }
        panic!("job ended without completion");
    }

    pub fn commit(
        c: &mut Controller,
        player: PlayerId,
        request_id: &str,
        tick: Tick,
    ) -> Option<mpsc::Receiver<WorkerMessage>> {
        let miner = c.revisions[&0].checkpoints[&0]
            .entities
            .iter()
            .find(|e| e.owner == player && e.type_key == "miner")
            .unwrap()
            .id
            .clone();
        let command = Command::AssignOrder {
            entities: vec![miner],
            order: Order::Idle {},
        };
        let request = CommitRequest {
            request_id: request_id.into(),
            slot_token: c.tokens[usize::from(player)].clone().unwrap(),
            draft: TurnDraft {
                based_on_revision: c.current,
                tick,
                commands: vec![DraftCommand {
                    local_id: "c0".into(),
                    command: Command::AssignOrder {
                        entities: match &command {
                            Command::AssignOrder { entities, .. } => entities.clone(),
                            _ => unreachable!(),
                        },
                        order: Order::Idle {},
                    },
                    future_orders: FutureOrderPolicy::Keep,
                }],
            },
        };
        assert!(c.precheck_commit(player, &request).unwrap().is_none());
        c.accept_commit(player, &request, vec![command]).unwrap().1
    }

    pub fn started(teams: bool, edit: impl FnOnce(&mut Setup)) -> Controller {
        let mut c = controller(teams, edit);
        let a = claim(&mut c, 0, teams.then_some("cyan")).unwrap();
        claim(&mut c, 1, teams.then_some("orange")).unwrap();
        c.start_match(&a, c.lobby.revision).unwrap();
        let rx = c.start_job(0, vec![], vec![]).unwrap();
        run_round(&mut c, rx);
        assert_eq!((c.phase, c.round, c.current), (Phase::Planning, 1, 0));
        c
    }

    fn reopen(c: &Controller) -> Controller {
        let (archive, loaded) = Archive::open(&c.replay_root, &c.match_id).unwrap();
        let setup = atemporal_content::load_setup(&loaded.config_yaml).unwrap();
        Controller::resume(
            setup,
            c.content.clone(),
            "/guide/".into(),
            c.replay_root.clone(),
            SimThread::spawn(),
            c.match_id.clone(),
            archive,
            loaded,
        )
        .unwrap()
    }

    #[test]
    fn resume_restores_ledger_profiles_tokens_and_partial_round() {
        let mut c = started(true, |_| {});
        assert!(commit(&mut c, 0, "a1", 0).is_none());
        let rx = commit(&mut c, 1, "b1", 0).unwrap();
        run_round(&mut c, rx);
        assert_eq!((c.round, c.current), (2, 1));
        assert!(commit(&mut c, 0, "a2", 5).is_none());
        let token_a = c.tokens[0].clone().unwrap();
        let turns_before = c.revisions[&1].turns.clone();
        let hash_before = c.revisions[&1].final_hash.clone();
        let time_before = c.time_totals.clone();

        // Killed after player 0's durable input for round 2: planning resumes with it accepted.
        let mut r = reopen(&c);
        assert_eq!((r.phase, r.round, r.current), (Phase::Planning, 2, 1));
        assert_eq!(r.player_for(&token_a), Some(0));
        assert_eq!(
            r.lobby.slots[1]
                .profile
                .as_ref()
                .unwrap()
                .team_id
                .as_deref(),
            Some("orange")
        );
        assert!(matches!(r.config.multiplayer, Multiplayer::Teams { .. }));
        assert_eq!(r.revisions[&1].turns, turns_before);
        assert_eq!(r.revisions[&1].final_hash, hash_before);
        assert!(r.revisions[&1].loaded && !r.revisions[&0].loaded);
        assert_eq!(r.time_totals, time_before);
        let precheck = r
            .precheck_commit(
                0,
                &CommitRequest {
                    request_id: "a2".into(),
                    slot_token: token_a.clone(),
                    draft: TurnDraft {
                        based_on_revision: 1,
                        tick: 5,
                        commands: vec![],
                    },
                },
            )
            .unwrap();
        assert!(
            matches!(precheck, Some(ServerMessage::CommitAccepted { .. })),
            "retry is idempotent"
        );
        assert!(r.restart_pending_round().unwrap().is_none());

        // Killed after both inputs but before the round record: the round is simulated again.
        let _dropped_job = commit(&mut r, 1, "b2", 5).unwrap();
        let mut r2 = reopen(&r);
        assert_eq!(r2.committed.len(), 2);
        let rx = r2.restart_pending_round().unwrap().unwrap();
        run_round(&mut r2, rx);
        assert_eq!((r2.phase, r2.round, r2.current), (Phase::Planning, 3, 2));
        assert_eq!(r2.revisions[&2].turns.len(), 4);

        // A missing result cache regenerates from the ledger and must reproduce the hash.
        fs::remove_dir_all(r2.replay_root.join(&r2.match_id).join("results/2")).unwrap();
        let r3 = reopen(&r2);
        assert!(r3.revisions[&2].loaded);
        assert_eq!(r3.revisions[&2].final_hash, r2.revisions[&2].final_hash);
        assert!(
            r3.replay_root
                .join(&r3.match_id)
                .join("results/2/complete.json")
                .exists()
        );
        let (_, loaded) = Archive::open(&r3.replay_root, &r3.match_id).unwrap();
        assert!(verify_archive(&loaded, &r3.content).unwrap());
    }

    #[test]
    fn resume_refuses_changed_content_and_keeps_archived_state() {
        let mut c = started(false, |_| {});
        let token = c.tokens[0].clone().unwrap();
        c.stop_and_archive(&token, "stop".into(), 0).unwrap();
        let r = reopen(&c);
        assert_eq!(r.phase, Phase::Archived);
        let (archive, mut loaded) = Archive::open(&c.replay_root, &c.match_id).unwrap();
        loaded.manifest.fingerprint.content_hash = "0".repeat(64);
        let setup = atemporal_content::load_setup(&loaded.config_yaml).unwrap();
        let err = Controller::resume(
            setup,
            c.content.clone(),
            "/guide/".into(),
            c.replay_root.clone(),
            SimThread::spawn(),
            c.match_id.clone(),
            archive,
            loaded,
        )
        .err()
        .unwrap();
        assert!(err.contains("different content/config"), "{err}");
    }

    #[test]
    fn budgets_evict_regenerable_bodies_only_and_regenerate_on_demand() {
        let mut c = started(false, |_| {});
        c.memory_budget = 1;
        assert!(commit(&mut c, 0, "a1", 0).is_none());
        let rx = commit(&mut c, 1, "b1", 0).unwrap();
        run_round(&mut c, rx);
        let root = c.replay_root.join(&c.match_id);
        // The current revision is pinned; the previous body went, the initial state stayed.
        assert!(c.revisions[&1].loaded && !c.revisions[&0].loaded);
        assert!(c.revisions[&0].checkpoints.contains_key(&0) && c.revisions[&0].samples.is_empty());
        assert_eq!(c.evictions, 1);
        assert!(
            c.ensure_loaded(0).unwrap().is_none(),
            "disk cache reloads without a replay"
        );
        assert!(c.revisions[&0].loaded);
        assert!(c.snapshot_range(0, 0, 300, 5).is_ok());
        // Disk budget removes only result caches of other revisions; ledger files stay.
        c.disk_budget = 1;
        c.enforce_budgets(None);
        assert!(!root.join("results/0/complete.json").exists());
        assert!(root.join("results/1/complete.json").exists());
        assert!(root.join("rounds/0.json").exists() && root.join("turns/1-0.json").exists());
        c.evict_body(0);
        let request = c
            .ensure_loaded(0)
            .unwrap()
            .expect("no cache left: replay needed");
        let mut fresh = RevisionData::new(0, 0, None, 0);
        collect_run(&request, &mut fresh, 2).unwrap();
        let mut wrong = RevisionData::new(0, 0, None, 0);
        wrong.final_hash = "bad".into();
        assert!(c.install_body(0, wrong).unwrap_err().contains("recorded"));
        c.install_body(0, fresh).unwrap();
        assert!(
            c.revisions[&0].loaded,
            "kept resident right after regeneration"
        );
        assert_eq!(c.exact_request(0, 7).map(|_| ()), Ok(()));
    }

    #[test]
    fn stats_buckets_report_latest_sample_and_stay_bounded() {
        let c = started(false, |_| {});
        let ticks = |m: ServerMessage| match m {
            ServerMessage::StatsRange { buckets, .. } => {
                buckets.into_iter().map(|b| b.tick).collect::<Vec<_>>()
            }
            _ => unreachable!(),
        };
        assert_eq!(
            ticks(c.stats_range(0, 0, 300, 100).unwrap()),
            vec![95, 195, 295, 300]
        );
        assert_eq!(ticks(c.stats_range(0, 0, 300, 7).unwrap()).len(), 31);
        assert_eq!(ticks(c.stats_range(0, 0, 300, 5).unwrap()).len(), 61);
        assert_eq!(ticks(c.stats_range(0, 100, 130, 1000).unwrap()), vec![130]);
        assert!(
            ticks(c.stats_range(0, 0, 1_000_000, 1).unwrap()).len() <= MAX_STAT_BUCKETS as usize
        );
    }

    #[test]
    fn timeline_index_is_bounded_for_cap_length_runs() {
        let mut fine = vec![];
        for from in (0..20_000).step_by(5) {
            for player in 0..4u8 {
                fine.push(TimelineBucket {
                    from_tick: from,
                    to_tick_exclusive: from + 5,
                    player_id: player,
                    activity: if from % 40 == 0 {
                        Activity::Combat
                    } else {
                        Activity::Mining
                    },
                    affected_entities: 3,
                    severity: 3.0,
                });
            }
        }
        let index = timeline_index(&fine, 4);
        assert!(
            index.len() <= 4 * TIMELINE_INDEX_PER_PLAYER as usize,
            "{}",
            index.len()
        );
        assert!(
            index
                .iter()
                .all(|b| b.activity == Activity::Combat && b.to_tick_exclusive - b.from_tick == 40)
        );
        let short = timeline_index(&fine[..fine.len() / 100], 4);
        assert!(
            short
                .iter()
                .all(|b| b.to_tick_exclusive - b.from_tick == TIMELINE_BUCKET)
        );
    }

    #[test]
    fn spectators_cannot_start_stop_or_commit() {
        let mut c = controller(false, |_| {});
        assert_eq!(c.player_for("guest"), None);
        assert_eq!(c.connect(Some("guest")), None);
        claim(&mut c, 0, None).unwrap();
        claim(&mut c, 1, None).unwrap();
        assert!(
            c.start_match("guest", c.lobby.revision)
                .unwrap_err()
                .contains("claimed slot")
        );
        assert!(
            c.stop_and_archive("guest", "r".into(), 0)
                .unwrap_err()
                .contains("claimed slot")
        );
        assert!(c.release_slot("guest").is_err());
        assert!(c.lobby.can_start);
    }
    #[test]
    fn stale_job_messages_cannot_mutate_published_or_pending_revision() {
        let mut c = started(false, |_| {});
        commit(&mut c, 0, "a1", 0);
        let rx = commit(&mut c, 1, "b1", 0).unwrap();
        let hash = c.revisions[&0].final_hash.clone();
        let pending = c.pending.as_ref().unwrap().samples.len();
        assert!(!c.on_batch(WorkerMessage::Progress {
            job_id: "stale".into(),
            revision: 99,
            tick: 100,
            end_tick: 200
        }));
        assert!(
            c.on_complete(
                "stale",
                c.revisions[&0].outcome.clone().unwrap(),
                "wrong".into(),
                0,
                vec![]
            )
            .is_none()
        );
        c.on_failed("stale", "ignored");
        assert_eq!(c.current, 0);
        assert_eq!(c.revisions[&0].final_hash, hash);
        assert_eq!(c.pending.as_ref().unwrap().samples.len(), pending);
        assert_eq!(c.phase, Phase::Simulating);
        drop(rx);
    }
}
