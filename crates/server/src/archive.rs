//! Durable match files: pinned config/content, atomic per-turn and per-round records, and
//! regenerable per-revision result caches. Sizes are recorded for Gate 2 measurement.
use atemporal_sim::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Manifest {
    pub schema_version: u32,
    pub match_id: String,
    pub server_instance_id: String,
    pub fingerprint: Fingerprint,
    pub config: MatchConfig,
    pub profiles: Vec<PlayerProfile>,
    pub initial_state: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoundRecord {
    pub round: u32,
    pub turns: Vec<String>,
    pub precedence: RoundPrecedence,
    pub parent_revision: Revision,
    pub revision: Revision,
    pub base_tick: Tick,
    pub editable_from: Tick,
    pub outcome: Outcome,
    pub final_hash: String,
    pub sim_duration_ms: u64,
    pub score: Option<RoundScore>,
    pub timed: Option<TimedAdjudication>,
    pub command_outcomes: Vec<CommandOutcome>,
    pub time_totals: Vec<PlayerTime>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ResultSizes {
    pub revision: Revision,
    pub samples_bytes: u64,
    pub checkpoints_bytes: u64,
    pub stats_bytes: u64,
    pub events_bytes: u64,
    pub timeline_bytes: u64,
    pub dictionary_bytes: u64,
    pub sample_count: usize,
    pub event_count: usize,
    pub write_ms: u64,
}

pub struct Archive {
    pub root: PathBuf,
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

impl Archive {
    pub fn create(replay_root: &Path, match_id: &str) -> Result<Self> {
        let root = replay_root.join(match_id);
        fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        Ok(Self { root })
    }
    pub fn write_manifest(
        &self,
        manifest: &Manifest,
        config_yaml: &str,
        content_yaml: &str,
        initial: &WorldState,
    ) -> Result<()> {
        atomic_write(&self.root.join("initial-state.json"), &json(initial)?)?;
        atomic_write(&self.root.join("config.yaml"), config_yaml.as_bytes())?;
        atomic_write(&self.root.join("content.yaml"), content_yaml.as_bytes())?;
        atomic_write(
            &self.root.join("manifest.json"),
            &serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?,
        )
    }
    /// Flushed before acknowledgement; a partial round survives restart.
    pub fn write_turn(&self, turn: &AcceptedTurn) -> Result<String> {
        let name = format!("turns/{}-{}.json", turn.round, turn.player);
        atomic_write(
            &self.root.join(&name),
            &serde_json::to_vec_pretty(turn).map_err(|e| e.to_string())?,
        )?;
        Ok(name)
    }
    pub fn write_round(&self, record: &RoundRecord) -> Result<()> {
        atomic_write(
            &self.root.join(format!("rounds/{}.json", record.round)),
            &serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?,
        )
    }
    pub fn write_results(&self, data: &crate::controller::RevisionData) -> Result<ResultSizes> {
        let start = std::time::Instant::now();
        let dir = self.root.join(format!("results/{}", data.revision));
        let samples: Vec<&Sample> = data.samples.values().collect();
        let checkpoints: Vec<&WorldState> = data.checkpoints.values().collect();
        let stats: Vec<&StatsSample> = data.stats.values().collect();
        let mut sizes = ResultSizes {
            revision: data.revision,
            sample_count: samples.len(),
            event_count: data.events.len(),
            ..Default::default()
        };
        for (name, bytes, slot) in [
            ("samples.json", json(&samples)?, &mut sizes.samples_bytes),
            (
                "checkpoints.json",
                json(&checkpoints)?,
                &mut sizes.checkpoints_bytes,
            ),
            ("stats.json", json(&stats)?, &mut sizes.stats_bytes),
            ("events.json", json(&data.events)?, &mut sizes.events_bytes),
            (
                "timeline.json",
                json(&data.timeline)?,
                &mut sizes.timeline_bytes,
            ),
            (
                "dictionary.json",
                json(&data.dictionary)?,
                &mut sizes.dictionary_bytes,
            ),
        ] {
            *slot = bytes.len() as u64;
            atomic_write(&dir.join(name), &bytes)?;
        }
        atomic_write(&dir.join("complete.json"), &json(&sizes)?)?;
        sizes.write_ms = start.elapsed().as_millis() as u64;
        Ok(sizes)
    }
    pub fn append_measurement<T: Serialize>(&self, record: &T) -> Result<()> {
        let path = self.root.join("measurements.jsonl");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let mut line = json(record)?;
        line.push(b'\n');
        file.write_all(&line).map_err(|e| e.to_string())
    }
}
