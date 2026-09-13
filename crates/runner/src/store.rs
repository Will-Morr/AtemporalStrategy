//! Local revision store: reproduces every published revision from the controller's inputs with
//! the same simulation library, compares the reference hash, and serves world-state queries to
//! the local browser. The controller never streams world state; only inputs and hashes arrive.
use atemporal_server::adapter::{Job, OUT_CAPACITY, SimThread};
use atemporal_server::controller::{
    RevisionData, fingerprint, minimum_end_tick, now_ms, timeline_index,
};
use atemporal_sim::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, watch};

const EXACT_CACHE: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Inputs or reference hash still missing, or waiting for the sim thread.
    Pending,
    Simulating,
    Verified,
    Mismatch(String),
}

pub struct Local {
    pub data: RevisionData,
    /// This round's turns, its precedence and the lock boundary the controller's job ran under.
    pub inputs: Option<(Vec<AcceptedTurn>, RoundPrecedence, Tick)>,
    pub reference: Option<(Tick, String)>,
    pub status: Status,
}

pub struct Store {
    pub match_id: String,
    pub fingerprint: Fingerprint,
    pub config: MatchConfig,
    pub content: Content,
    pub revisions: BTreeMap<Revision, Local>,
    pub memory_budget: u64,
    use_counter: u64,
    exact_cache: VecDeque<((Revision, Tick), WorldState)>,
    pub evictions: u64,
    pub regenerations: u64,
}

/// Shared between the controller link, the local browser sockets and the sim thread.
pub struct Peripheral {
    pub store: Mutex<Option<Store>>,
    pub changed: watch::Sender<u64>,
    pub sim: SimThread,
    /// Locally generated broadcasts (catch-up progress) for every local browser socket.
    pub local: broadcast::Sender<ServerMessage>,
    pub connected: AtomicBool,
    running: AtomicBool,
    pub memory_budget: u64,
}

impl Store {
    /// Adopt bootstrap data after checking this build reproduces the controller's fingerprint.
    pub fn new(
        match_id: String,
        remote: Fingerprint,
        config: MatchConfig,
        content: Content,
        initial: WorldState,
        memory_budget: u64,
    ) -> Result<Self> {
        let local = fingerprint(&config, &content)?;
        if local != remote {
            return Err(format!(
                "incompatible fingerprint: controller {remote:?} but this peripheral is {local:?}"
            ));
        }
        let mut genesis = RevisionData::new(0, 0, None, 0);
        genesis.checkpoints.insert(0, initial);
        let mut revisions = BTreeMap::new();
        revisions.insert(
            0,
            Local {
                data: genesis,
                inputs: None,
                reference: None,
                status: Status::Pending,
            },
        );
        Ok(Self {
            match_id,
            fingerprint: remote,
            config,
            content,
            revisions,
            memory_budget,
            use_counter: 0,
            exact_cache: VecDeque::new(),
            evictions: 0,
            regenerations: 0,
        })
    }

    fn next_use(&mut self) -> u64 {
        self.use_counter += 1;
        self.use_counter
    }

    pub fn status(&self, revision: Revision) -> Status {
        self.revisions
            .get(&revision)
            .map_or(Status::Pending, |l| l.status.clone())
    }

    /// Latest verified revision: the one whose body is never evicted.
    pub fn current(&self) -> Option<Revision> {
        self.revisions
            .iter()
            .rev()
            .find(|(_, l)| l.status == Status::Verified)
            .map(|(r, _)| *r)
    }

    pub fn accept_inputs(
        &mut self,
        revision: Revision,
        turns: Vec<AcceptedTurn>,
        precedence: RoundPrecedence,
        editable_from: Tick,
    ) {
        let parent = self
            .revisions
            .range(..revision)
            .next_back()
            .map(|(r, _)| *r);
        let entry = self.revisions.entry(revision).or_insert_with(|| Local {
            data: RevisionData::new(revision, precedence.round, parent, 0),
            inputs: None,
            reference: None,
            status: Status::Pending,
        });
        if entry.inputs.is_none() {
            entry.data.round = precedence.round;
            entry.inputs = Some((turns, precedence, editable_from));
        }
    }

    pub fn accept_reference(&mut self, revision: Revision, tick: Tick, hash: String) {
        if let Some(entry) = self.revisions.get_mut(&revision) {
            entry.reference.get_or_insert((tick, hash));
        }
    }

    /// Scores and timed adjudication are controller-owned; keep them for the round viewer.
    pub fn accept_published(
        &mut self,
        revision: Revision,
        score: Option<RoundScore>,
        timed: Option<TimedAdjudication>,
    ) {
        if let Some(entry) = self.revisions.get_mut(&revision) {
            entry.data.score = score;
            entry.data.timed = timed;
        }
    }

    /// Lowest pending revision with inputs and a reference whose parent is settled.
    pub fn next_runnable(&self) -> Option<Revision> {
        self.revisions
            .iter()
            .find(|(r, l)| {
                l.status == Status::Pending
                    && l.inputs.is_some()
                    && l.reference.is_some()
                    && self
                        .revisions
                        .range(..**r)
                        .next_back()
                        .is_none_or(|(_, p)| {
                            matches!(p.status, Status::Verified | Status::Mismatch(_))
                        })
            })
            .map(|(r, _)| *r)
    }

    fn cumulative(
        &self,
        revision: Revision,
    ) -> Result<(Vec<AcceptedTurn>, Vec<RoundPrecedence>, Tick)> {
        let local = self.revisions.get(&revision).ok_or("unknown revision")?;
        let (turns, precedence, boundary) = local.inputs.as_ref().ok_or("inputs missing")?;
        let mut all_turns = vec![];
        let mut all_precedence = vec![];
        if let Some((_, parent)) = self.revisions.range(..revision).next_back() {
            all_turns = parent.data.turns.clone();
            all_precedence = parent.data.precedence.clone();
        }
        all_turns.extend(turns.iter().cloned());
        if revision > 0 {
            all_precedence.push(precedence.clone());
        }
        Ok((all_turns, all_precedence, *boundary))
    }

    fn request(
        &self,
        revision: Revision,
        checkpoint: WorldState,
        turns: Vec<AcceptedTurn>,
        precedence: Vec<RoundPrecedence>,
        boundary: Tick,
        dictionary: Vec<EntityRef>,
    ) -> SimRequest {
        SimRequest {
            schema_version: Version::default(),
            job_id: format!("peripheral-r{revision}-{}", now_ms()),
            revision,
            fingerprint: self.fingerprint.clone(),
            config: self.config.clone(),
            content: self.content.clone(),
            checkpoint,
            events: turns,
            precedence,
            end_tick_exclusive: self.config.max_tick,
            minimum_end_tick: minimum_end_tick(&self.config, boundary),
            entity_dictionary: dictionary,
        }
    }

    /// Full replay from the initial state over the revision's whole ledger.
    pub fn replay_request(&self, revision: Revision) -> Result<SimRequest> {
        let (turns, precedence, boundary) = self.cumulative(revision)?;
        let initial = self.initial()?;
        Ok(self.request(revision, initial, turns, precedence, boundary, vec![]))
    }

    fn initial(&self) -> Result<WorldState> {
        self.revisions
            .get(&0)
            .and_then(|g| g.data.checkpoints.get(&0))
            .cloned()
            .ok_or_else(|| "initial state missing".into())
    }

    /// Same job the controller ran: from the parent's checkpoint at or before the earliest new
    /// input, reusing the unchanged prefix. Without a loaded verified parent, replay from tick 0.
    pub fn start(&mut self, revision: Revision) -> Result<(SimRequest, RevisionData)> {
        let (turns, precedence, boundary) = self.cumulative(revision)?;
        let local = self.revisions.get(&revision).ok_or("unknown revision")?;
        let round = local.data.round;
        let parent_key = self
            .revisions
            .range(..revision)
            .next_back()
            .map(|(r, _)| *r);
        let parent = parent_key
            .and_then(|p| self.revisions.get(&p))
            .filter(|p| p.status == Status::Verified && p.data.loaded);
        let earliest = turns
            .iter()
            .filter(|t| t.round == round)
            .map(|t| t.tick)
            .min()
            .unwrap_or(0);
        let mut data = RevisionData::new(revision, round, parent_key.filter(|_| revision > 0), 0);
        let checkpoint = match parent {
            Some(parent) if revision > 0 => {
                let (base_tick, checkpoint) = parent
                    .data
                    .checkpoints
                    .range(..=earliest)
                    .next_back()
                    .map(|(t, s)| (*t, s.clone()))
                    .ok_or("no checkpoint at or before the earliest new input")?;
                data.base_tick = base_tick;
                data.dictionary = parent.data.dictionary.clone();
                data.samples = parent
                    .data
                    .samples
                    .range(..base_tick)
                    .map(|(t, s)| (*t, s.clone()))
                    .collect();
                data.checkpoints = parent
                    .data
                    .checkpoints
                    .range(..=base_tick)
                    .map(|(t, s)| (*t, s.clone()))
                    .collect();
                data.stats = parent
                    .data
                    .stats
                    .range(..base_tick)
                    .map(|(t, s)| (*t, s.clone()))
                    .collect();
                data.events = parent
                    .data
                    .events
                    .iter()
                    .filter(|e| e.tick < base_tick)
                    .cloned()
                    .collect();
                data.timeline = parent
                    .data
                    .timeline
                    .iter()
                    .filter(|b| b.to_tick_exclusive <= base_tick)
                    .cloned()
                    .collect();
                data.command_outcomes = parent
                    .data
                    .command_outcomes
                    .iter()
                    .filter(|outcome| {
                        parent.data.turns.iter().any(|turn| {
                            turn.tick < base_tick
                                && turn.commands.iter().any(|c| c.id == outcome.command_id)
                        })
                    })
                    .cloned()
                    .collect();
                checkpoint
            }
            _ => self.initial()?,
        };
        data.turns = turns.clone();
        data.precedence = precedence.clone();
        data.score = local.data.score.clone();
        data.timed = local.data.timed.clone();
        data.checkpoints
            .entry(checkpoint.tick)
            .or_insert_with(|| checkpoint.clone());
        let dictionary = data.dictionary.clone();
        let request = self.request(
            revision, checkpoint, turns, precedence, boundary, dictionary,
        );
        let entry = self
            .revisions
            .get_mut(&revision)
            .ok_or("unknown revision")?;
        entry.status = Status::Simulating;
        Ok((request, data))
    }

    /// Adopt a finished reproduction; the reference hash and terminal tick decide its status.
    pub fn finish(&mut self, revision: Revision, mut data: RevisionData) -> Status {
        // Targeted fault injection for the mismatch check: corrupt one revision's local hash.
        if std::env::var("ATEMPORAL_PERIPHERAL_CORRUPT").is_ok_and(|v| v == revision.to_string()) {
            data.final_hash = format!("corrupt-{}", data.final_hash);
        }
        let stamp = self.next_use();
        let Some(entry) = self.revisions.get_mut(&revision) else {
            return Status::Pending;
        };
        let terminal = data.outcome.as_ref().map_or(0, |o| o.terminal_state_tick);
        let status = match &entry.reference {
            Some((tick, hash)) if *hash == data.final_hash && *tick == terminal => Status::Verified,
            Some((tick, hash)) => Status::Mismatch(format!(
                "peripheral mismatch on revision {revision}: local hash {}… at tick {terminal} but the controller recorded {}… at tick {tick}; order entry stopped",
                &data.final_hash[..12.min(data.final_hash.len())],
                &hash[..12.min(hash.len())]
            )),
            None => Status::Pending,
        };
        data.timeline_index = timeline_index(&data.timeline, self.config.player_count);
        data.bytes = data.estimate_bytes();
        data.last_used = stamp;
        data.loaded = true;
        entry.data = data;
        entry.status = status.clone();
        self.exact_cache.retain(|((r, _), _)| *r != revision);
        self.enforce_budget(Some(revision));
        status
    }

    pub fn fail(&mut self, revision: Revision, message: String) {
        if let Some(entry) = self.revisions.get_mut(&revision) {
            entry.status = Status::Mismatch(format!(
                "peripheral could not reproduce revision {revision}: {message}"
            ));
        }
    }

    /// Evict least-recently-used bodies other than the current revision and `keep` until resident
    /// bodies fit; the initial state and all inputs/hashes stay so anything can be regenerated.
    fn enforce_budget(&mut self, keep: Option<Revision>) {
        let current = self.current();
        loop {
            let resident: u64 = self
                .revisions
                .values()
                .filter(|l| l.data.loaded)
                .map(|l| l.data.bytes)
                .sum();
            if resident <= self.memory_budget {
                return;
            }
            let victim = self
                .revisions
                .values()
                .filter(|l| l.data.loaded && Some(l.data.revision) != current)
                .filter(|l| Some(l.data.revision) != keep && l.status != Status::Simulating)
                .min_by_key(|l| l.data.last_used)
                .map(|l| l.data.revision);
            let Some(revision) = victim else { return };
            let initial = (revision == 0).then(|| self.initial().ok()).flatten();
            let Some(entry) = self.revisions.get_mut(&revision) else {
                return;
            };
            entry.data.dictionary = vec![];
            entry.data.samples = BTreeMap::new();
            entry.data.checkpoints = initial
                .map(|s| BTreeMap::from([(0, s)]))
                .unwrap_or_default();
            entry.data.stats = BTreeMap::new();
            entry.data.events = vec![];
            entry.data.timeline = vec![];
            entry.data.loaded = false;
            self.exact_cache.retain(|((r, _), _)| *r != revision);
            self.evictions += 1;
        }
    }

    /// `Ok(None)`: the body is resident. `Ok(Some(request))`: regenerate it by full replay.
    pub fn ensure_loaded(&mut self, revision: Revision) -> Result<Option<SimRequest>> {
        let stamp = self.next_use();
        let local = self
            .revisions
            .get_mut(&revision)
            .ok_or_else(|| format!("unknown revision {revision}"))?;
        match &local.status {
            Status::Verified => {}
            Status::Mismatch(message) => return Err(message.clone()),
            _ => {
                return Err(format!(
                    "revision {revision} is still being reproduced locally"
                ));
            }
        }
        local.data.last_used = stamp;
        if local.data.loaded {
            return Ok(None);
        }
        self.replay_request(revision).map(Some)
    }

    /// Adopt a regenerated body only when it reproduces the verified hash again.
    pub fn install_body(&mut self, revision: Revision, fresh: RevisionData) -> Result<()> {
        let stamp = self.next_use();
        let local = self
            .revisions
            .get_mut(&revision)
            .ok_or("unknown revision")?;
        if local
            .reference
            .as_ref()
            .is_none_or(|(_, h)| *h != fresh.final_hash)
        {
            return Err(format!(
                "revision {revision} regenerated with hash {} but the controller recorded {:?}",
                fresh.final_hash, local.reference
            ));
        }
        local.data.dictionary = fresh.dictionary;
        local.data.samples = fresh.samples;
        local.data.checkpoints = fresh.checkpoints;
        local.data.stats = fresh.stats;
        local.data.events = fresh.events;
        local.data.timeline = fresh.timeline;
        local.data.loaded = true;
        local.data.bytes = local.data.estimate_bytes();
        local.data.last_used = stamp;
        self.regenerations += 1;
        self.enforce_budget(Some(revision));
        Ok(())
    }

    pub fn cached_exact(&self, revision: Revision, tick: Tick) -> Option<WorldState> {
        if let Some(s) = self
            .revisions
            .get(&revision)
            .and_then(|l| l.data.checkpoints.get(&tick))
        {
            return Some(s.clone());
        }
        self.exact_cache
            .iter()
            .find(|(k, _)| *k == (revision, tick))
            .map(|(_, s)| s.clone())
    }

    pub fn remember_exact(&mut self, revision: Revision, tick: Tick, state: WorldState) {
        if self.exact_cache.len() >= EXACT_CACHE {
            self.exact_cache.pop_front();
        }
        self.exact_cache.push_back(((revision, tick), state));
    }

    /// Reconstruction request for S[tick] from the nearest earlier local checkpoint.
    pub fn exact_request(&self, revision: Revision, tick: Tick) -> Result<SimRequest> {
        let local = self.revisions.get(&revision).ok_or("unknown revision")?;
        let terminal = local
            .data
            .outcome
            .as_ref()
            .map_or(tick, |o| o.terminal_state_tick);
        if tick > terminal || tick > self.config.max_tick {
            return Err(format!(
                "tick {tick} is beyond this revision's end {terminal}"
            ));
        }
        let (_, checkpoint) = local
            .data
            .checkpoints
            .range(..=tick)
            .next_back()
            .ok_or("no checkpoint before tick")?;
        let (_, _, boundary) = self.cumulative(revision).unwrap_or_default();
        Ok(self.request(
            revision,
            checkpoint.clone(),
            local.data.turns.clone(),
            local.data.precedence.clone(),
            boundary,
            vec![],
        ))
    }

    pub fn data(&self, revision: Revision) -> Result<&RevisionData> {
        self.revisions
            .get(&revision)
            .map(|l| &l.data)
            .ok_or_else(|| format!("unknown revision {revision}"))
    }

    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "match_id": self.match_id,
            "fingerprint": self.fingerprint,
            "current": self.current(),
            "evictions": self.evictions,
            "regenerations": self.regenerations,
            "revisions": self.revisions.values().map(|l| serde_json::json!({
                "revision": l.data.revision,
                "round": l.data.round,
                "base_tick": l.data.base_tick,
                "status": match &l.status {
                    Status::Pending => "pending".to_string(),
                    Status::Simulating => "simulating".to_string(),
                    Status::Verified => "verified".to_string(),
                    Status::Mismatch(m) => format!("mismatch: {m}"),
                },
                "local_hash": l.data.final_hash,
                "reference": l.reference,
                "terminal": l.data.outcome.as_ref().map(|o| o.terminal_state_tick),
                "loaded": l.data.loaded,
                "sim_ms": l.data.sim_duration_ms,
            })).collect::<Vec<_>>(),
        })
    }
}

impl Peripheral {
    pub fn new(memory_budget: u64) -> Arc<Self> {
        let (changed, _) = watch::channel(0);
        let (local, _) = broadcast::channel(64);
        Arc::new(Self {
            store: Mutex::new(None),
            changed,
            sim: SimThread::spawn(),
            local,
            connected: AtomicBool::new(false),
            running: AtomicBool::new(false),
            memory_budget,
        })
    }

    pub fn bump(&self) {
        self.changed.send_modify(|v| *v += 1);
    }

    pub fn status(&self, revision: Revision) -> Status {
        self.store
            .lock()
            .unwrap()
            .as_ref()
            .map_or(Status::Pending, |s| s.status(revision))
    }

    /// Wait until the local reproduction of `revision` settled one way or the other.
    pub async fn wait_settled(&self, revision: Revision) -> Status {
        let mut rx = self.changed.subscribe();
        loop {
            let status = self.status(revision);
            if matches!(status, Status::Verified | Status::Mismatch(_)) {
                return status;
            }
            if rx.changed().await.is_err() {
                return status;
            }
        }
    }

    /// Reproduce every runnable revision in order on the sim thread; one driver at a time.
    pub fn schedule(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let p = self.clone();
        tokio::spawn(async move {
            loop {
                let next = {
                    let store = p.store.lock().unwrap();
                    store.as_ref().and_then(|s| s.next_runnable())
                };
                let Some(revision) = next else { break };
                p.reproduce(revision).await;
                p.bump();
            }
            p.running.store(false, Ordering::Release);
            // A revision that became runnable while we were stopping is picked up by the next call.
            let more = {
                let store = p.store.lock().unwrap();
                store.as_ref().and_then(|s| s.next_runnable()).is_some()
            };
            if more {
                p.schedule();
            }
        });
    }

    async fn reproduce(&self, revision: Revision) {
        let started = std::time::Instant::now();
        let prepared = {
            let mut store = self.store.lock().unwrap();
            let Some(store) = store.as_mut() else { return };
            store.start(revision)
        };
        let (request, mut data) = match prepared {
            Ok(prepared) => prepared,
            Err(message) => {
                if let Some(store) = self.store.lock().unwrap().as_mut() {
                    store.fail(revision, message);
                }
                return;
            }
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = mpsc::channel(OUT_CAPACITY);
        let end_tick = request.end_tick_exclusive;
        self.sim.submit(Job::Run {
            request: Box::new(request),
            cancel,
            out: tx,
        });
        let mut done = false;
        while let Some(message) = rx.recv().await {
            match message {
                WorkerMessage::Batch {
                    dictionary,
                    samples,
                    checkpoints,
                    stats,
                    events,
                    timeline,
                    ..
                } => {
                    data.dictionary.extend(dictionary);
                    for s in samples {
                        data.samples.insert(s.tick, s);
                    }
                    for c in checkpoints {
                        data.checkpoints.insert(c.tick, c);
                    }
                    for s in stats {
                        data.stats.insert(s.tick, s);
                    }
                    data.events.extend(events);
                    data.timeline.extend(timeline);
                }
                WorkerMessage::Progress { tick, .. } => {
                    let _ = self.local.send(ServerMessage::SimulationProgress {
                        revision,
                        tick,
                        end_tick,
                    });
                }
                WorkerMessage::Complete {
                    outcome,
                    final_hash,
                    sim_duration_ms,
                    command_outcomes,
                    ..
                } => {
                    data.outcome = Some(outcome);
                    data.final_hash = final_hash;
                    data.sim_duration_ms = sim_duration_ms.get();
                    data.command_outcomes.extend(command_outcomes);
                    let status = self.store.lock().unwrap().as_mut().map(|s| {
                        s.finish(
                            revision,
                            std::mem::replace(&mut data, RevisionData::new(revision, 0, None, 0)),
                        )
                    });
                    match status {
                        Some(Status::Verified) => println!(
                            "revision {revision}: reproduced and verified in {:?}",
                            started.elapsed()
                        ),
                        Some(Status::Mismatch(m)) => eprintln!("{m}"),
                        _ => {}
                    }
                    done = true;
                }
                WorkerMessage::Failed { message, .. } => {
                    if let Some(store) = self.store.lock().unwrap().as_mut() {
                        store.fail(revision, message.clone());
                    }
                    eprintln!("revision {revision}: local simulation failed: {message}");
                    done = true;
                }
            }
        }
        if !done && let Some(store) = self.store.lock().unwrap().as_mut() {
            store.fail(revision, "simulation thread stopped".into());
        }
    }

    /// Body resident (regenerating an evicted one by full replay and re-checking its hash).
    pub async fn prepare(&self, revision: Revision) -> Result<()> {
        let request = {
            let mut store = self.store.lock().unwrap();
            let store = store.as_mut().ok_or("no match bootstrap yet")?;
            store.ensure_loaded(revision)?
        };
        if let Some(request) = request {
            let players = request.config.player_count;
            let started = std::time::Instant::now();
            let fresh = self.sim.replay(request, players).await?;
            self.store
                .lock()
                .unwrap()
                .as_mut()
                .ok_or("no match bootstrap yet")?
                .install_body(revision, fresh)?;
            println!(
                "regenerated revision {revision} from the ledger in {:?}",
                started.elapsed()
            );
        }
        Ok(())
    }

    pub async fn exact(&self, revision: Revision, tick: Tick) -> Result<WorldState> {
        self.prepare(revision).await?;
        let request = {
            let store = self.store.lock().unwrap();
            let store = store.as_ref().ok_or("no match bootstrap yet")?;
            if let Some(state) = store.cached_exact(revision, tick) {
                return Ok(state);
            }
            store.exact_request(revision, tick)?
        };
        let state = self.sim.exact(request, tick).await?;
        if let Some(store) = self.store.lock().unwrap().as_mut() {
            store.remember_exact(revision, tick, state.clone());
        }
        Ok(state)
    }

    /// Answer a world-state or inputs query from the local store.
    pub async fn answer(&self, client: ClientMessage) -> Result<ServerMessage> {
        match client {
            ClientMessage::GetSnapshotRange {
                revision,
                from_tick,
                to_tick,
                stride,
            } => {
                self.prepare(revision).await?;
                let store = self.store.lock().unwrap();
                let store = store.as_ref().ok_or("no match bootstrap yet")?;
                Ok(store.data(revision)?.snapshot_range(
                    store.config.snapshot_interval,
                    from_tick,
                    to_tick,
                    stride,
                ))
            }
            ClientMessage::GetStats {
                revision,
                from_tick,
                to_tick,
                bucket_width,
            } => {
                self.prepare(revision).await?;
                let store = self.store.lock().unwrap();
                let store = store.as_ref().ok_or("no match bootstrap yet")?;
                Ok(store.data(revision)?.stats_range(
                    store.config.snapshot_interval,
                    from_tick,
                    to_tick,
                    bucket_width,
                ))
            }
            ClientMessage::GetEvents {
                revision,
                from_tick,
                to_tick,
                effects_only,
            } => {
                self.prepare(revision).await?;
                let store = self.store.lock().unwrap();
                let store = store.as_ref().ok_or("no match bootstrap yet")?;
                Ok(store.data(revision)?.events_range(
                    from_tick,
                    to_tick,
                    effects_only.unwrap_or(false),
                ))
            }
            ClientMessage::GetCommands {
                revision,
                from_tick,
                to_tick,
            } => {
                let store = self.store.lock().unwrap();
                let store = store.as_ref().ok_or("no match bootstrap yet")?;
                Ok(store.data(revision)?.commands_range(from_tick, to_tick))
            }
            ClientMessage::GetRound { revision } => {
                let store = self.store.lock().unwrap();
                let store = store.as_ref().ok_or("no match bootstrap yet")?;
                store
                    .data(revision)?
                    .round_result(store.config.player_count)
            }
            ClientMessage::GetExactState { revision, tick } => {
                let snapshot = self.exact(revision, tick).await?;
                Ok(ServerMessage::ExactState {
                    revision,
                    tick,
                    snapshot,
                })
            }
            ClientMessage::GetControlGroups {
                revision,
                tick,
                player,
            } => {
                let state = self.exact(revision, tick).await?;
                Ok(ServerMessage::ControlGroups {
                    revision,
                    tick,
                    groups: state
                        .control_groups
                        .into_iter()
                        .filter(|g| g.id.owner == player)
                        .collect(),
                })
            }
            _ => Err("not a local query".into()),
        }
    }
}

/// Queries the peripheral answers itself; everything else relays to the controller.
pub fn is_local_query(client: &ClientMessage) -> bool {
    matches!(
        client,
        ClientMessage::GetSnapshotRange { .. }
            | ClientMessage::GetExactState { .. }
            | ClientMessage::GetStats { .. }
            | ClientMessage::GetEvents { .. }
            | ClientMessage::GetCommands { .. }
            | ClientMessage::GetRound { .. }
            | ClientMessage::GetControlGroups { .. }
    )
}
