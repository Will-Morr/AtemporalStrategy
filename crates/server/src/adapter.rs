//! Dedicated simulation thread: owned jobs in, bounded typed batches out, per-tick cancellation.
//! Short exact-state jobs are serviced at tick boundaries of a running job. Batches are flushed
//! by estimated bytes as well as ticks, so the bounded channel caps in-flight memory at about
//! `OUT_CAPACITY × BATCH_BYTES` regardless of population.
use atemporal_sim::{Output, SimRequest, Tick, WorkerMessage, WorldState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use tokio::sync::{mpsc as tmpsc, oneshot};

pub enum Job {
    Run {
        request: Box<SimRequest>,
        cancel: Arc<AtomicBool>,
        out: tmpsc::Sender<WorkerMessage>,
    },
    Exact {
        request: Box<SimRequest>,
        tick: Tick,
        reply: oneshot::Sender<Result<WorldState, String>>,
    },
    /// Regenerate an evicted revision's body from its full ledger; queued behind running jobs.
    Replay {
        request: Box<SimRequest>,
        players: u8,
        reply: oneshot::Sender<Result<crate::controller::RevisionData, String>>,
    },
}

#[derive(Clone)]
pub struct SimThread {
    tx: mpsc::Sender<Job>,
}

const BATCH_TICKS: Tick = 250;
const BATCH_BYTES: usize = 4 << 20;
pub const OUT_CAPACITY: usize = 8;

/// Cheap upper-bound estimate of an output's in-memory footprint, used for channel accounting.
pub fn estimate_bytes(output: &Output) -> usize {
    match output {
        Output::Dictionary(d) => d.len() * 64,
        Output::Sample(s) => 32 + s.entities.len() * 40 + s.ore.len() * 16,
        Output::Checkpoint(c) => {
            256 + c.terrain.cells.len() + c.ore.len() * 8 + c.entities.len() * 400
        }
        Output::Stats(s) => 16 + s.players.len() * 120,
        Output::Events(e) => e.len() * 96,
        Output::Timeline(t) => t.len() * 48,
        Output::Progress { .. } => 0,
    }
}

impl SimThread {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("atemporal-sim".into())
            .spawn(move || worker(rx))
            .expect("spawn simulation thread");
        Self { tx }
    }
    pub fn submit(&self, job: Job) {
        let _ = self.tx.send(job);
    }
    pub async fn exact(&self, request: SimRequest, tick: Tick) -> Result<WorldState, String> {
        let (reply, rx) = oneshot::channel();
        self.submit(Job::Exact {
            request: Box::new(request),
            tick,
            reply,
        });
        rx.await
            .map_err(|_| "simulation thread stopped".to_string())?
    }
    pub async fn replay(
        &self,
        request: SimRequest,
        players: u8,
    ) -> Result<crate::controller::RevisionData, String> {
        let (reply, rx) = oneshot::channel();
        self.submit(Job::Replay {
            request: Box::new(request),
            players,
            reply,
        });
        rx.await
            .map_err(|_| "simulation thread stopped".to_string())?
    }
}

fn service_replay(
    request: &SimRequest,
    players: u8,
    reply: oneshot::Sender<Result<crate::controller::RevisionData, String>>,
) {
    let mut data = crate::controller::RevisionData::new(request.revision, 0, None, 0);
    let _ = reply.send(crate::controller::collect_run(request, &mut data, players).map(|_| data));
}

fn service_exact(
    request: &SimRequest,
    tick: Tick,
    reply: oneshot::Sender<Result<WorldState, String>>,
) {
    let _ = reply.send(atemporal_sim::reconstruct(request, tick));
}

fn worker(rx: mpsc::Receiver<Job>) {
    let mut pending_runs: std::collections::VecDeque<Job> = Default::default();
    loop {
        let job = match pending_runs.pop_front() {
            Some(job) => job,
            None => match rx.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        match job {
            Job::Exact {
                request,
                tick,
                reply,
            } => service_exact(&request, tick, reply),
            Job::Replay {
                request,
                players,
                reply,
            } => service_replay(&request, players, reply),
            Job::Run {
                request,
                cancel,
                out,
            } => {
                let mut hook = || {
                    while let Ok(job) = rx.try_recv() {
                        match job {
                            Job::Exact {
                                request,
                                tick,
                                reply,
                            } => service_exact(&request, tick, reply),
                            run => pending_runs.push_back(run),
                        }
                    }
                };
                run_job(&request, cancel, out, &mut hook);
            }
        }
    }
}

struct Batch {
    bytes: usize,
    dictionary: Vec<atemporal_sim::EntityRef>,
    samples: Vec<atemporal_sim::Sample>,
    checkpoints: Vec<WorldState>,
    stats: Vec<atemporal_sim::StatsSample>,
    events: Vec<atemporal_sim::WorldEvent>,
    timeline: Vec<atemporal_sim::TimelineBucket>,
}
impl Batch {
    fn new() -> Self {
        Self {
            bytes: 0,
            dictionary: vec![],
            samples: vec![],
            checkpoints: vec![],
            stats: vec![],
            events: vec![],
            timeline: vec![],
        }
    }
    fn is_empty(&self) -> bool {
        self.dictionary.is_empty()
            && self.samples.is_empty()
            && self.checkpoints.is_empty()
            && self.stats.is_empty()
            && self.events.is_empty()
            && self.timeline.is_empty()
    }
    fn message(&mut self, job_id: &str, revision: u32) -> WorkerMessage {
        let taken = std::mem::replace(self, Batch::new());
        WorkerMessage::Batch {
            job_id: job_id.into(),
            revision,
            dictionary: taken.dictionary,
            samples: taken.samples,
            checkpoints: taken.checkpoints,
            stats: taken.stats,
            events: taken.events,
            timeline: taken.timeline,
        }
    }
}

fn run_job(
    request: &SimRequest,
    cancel: Arc<AtomicBool>,
    out: tmpsc::Sender<WorkerMessage>,
    hook: &mut dyn FnMut(),
) {
    let job_id = request.job_id.clone();
    let revision = request.revision;
    let mut batch = Batch::new();
    let mut last_flush = request.checkpoint.tick;
    let mut closed = false;
    let mut emit = |o: Output| {
        if closed {
            return;
        }
        batch.bytes += estimate_bytes(&o);
        match o {
            Output::Dictionary(d) => batch.dictionary.extend(d),
            Output::Sample(s) => batch.samples.push(s),
            Output::Checkpoint(c) => batch.checkpoints.push(c),
            Output::Stats(s) => batch.stats.push(s),
            Output::Events(e) => batch.events.extend(e),
            Output::Timeline(t) => batch.timeline.extend(t),
            Output::Progress { tick } => {
                if tick.saturating_sub(last_flush) >= BATCH_TICKS || batch.bytes >= BATCH_BYTES {
                    last_flush = tick;
                    // A slow consumer throttles the simulation, never the IO loop.
                    if out.blocking_send(batch.message(&job_id, revision)).is_err()
                        || out
                            .blocking_send(WorkerMessage::Progress {
                                job_id: job_id.clone(),
                                revision,
                                tick,
                                end_tick: request.end_tick_exclusive,
                            })
                            .is_err()
                    {
                        closed = true;
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    };
    let result = atemporal_sim::run_with_hook(request, &cancel, &mut emit, hook);
    if closed {
        return;
    }
    let message = match result {
        Ok(result) => {
            if !batch.is_empty() {
                let _ = out.blocking_send(batch.message(&job_id, revision));
            }
            WorkerMessage::Complete {
                job_id: job_id.clone(),
                revision,
                outcome: result.outcome,
                final_hash: result.final_hash,
                sim_duration_ms: result.sim_duration_ms.try_into().unwrap_or_default(),
                command_outcomes: result.command_outcomes,
            }
        }
        Err(message) => WorkerMessage::Failed {
            job_id: job_id.clone(),
            revision,
            error_code: if message == "canceled" {
                "canceled".into()
            } else {
                "sim_error".into()
            },
            message,
        },
    };
    let _ = out.blocking_send(message);
}
