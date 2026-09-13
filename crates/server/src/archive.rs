//! Durable match files: pinned config/content, atomic per-turn and per-round records, and
//! regenerable per-revision result caches. Sizes are recorded for Gate 2 measurement.
use atemporal_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Coarse per-player activity index; kept here so a resumed server can publish without
    /// reloading the result cache.
    #[serde(default)]
    pub timeline_index: Vec<TimelineBucket>,
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

/// Private slot tokens so browsers can reconnect to a resumed match; never broadcast.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LobbyRecord {
    pub tokens: Vec<Option<String>>,
}

/// Everything a resume or replay verification needs, read back from disk.
pub struct Loaded {
    pub manifest: Manifest,
    pub config_yaml: String,
    pub content_yaml: String,
    pub initial: WorldState,
    pub turns: BTreeMap<(u32, PlayerId), AcceptedTurn>,
    pub request_ids: BTreeMap<(u32, PlayerId), String>,
    pub rounds: Vec<RoundRecord>,
    pub tokens: Option<Vec<Option<String>>>,
    pub archived: Option<ArchiveRecord>,
}

pub struct ResultCache {
    pub dictionary: Vec<EntityRef>,
    pub samples: Vec<Sample>,
    pub checkpoints: Vec<WorldState>,
    pub stats: Vec<StatsSample>,
    pub events: Vec<WorldEvent>,
    pub timeline: Vec<TimelineBucket>,
}

impl ResultSizes {
    pub fn total(&self) -> u64 {
        self.samples_bytes
            + self.checkpoints_bytes
            + self.stats_bytes
            + self.events_bytes
            + self.timeline_bytes
            + self.dictionary_bytes
    }
}

pub struct Archive {
    pub root: PathBuf,
}

/// Gate 5 injection: `ATEMPORAL_FAIL_AT=<point>:<round>` aborts the process at that point of
/// that round, and `ATEMPORAL_DISK_FULL_AFTER=<n>` makes every durable write after the n-th
/// fail like ENOSPC.
pub fn fail_point(point: &str, round: u32) {
    if std::env::var("ATEMPORAL_FAIL_AT").is_ok_and(|p| p == format!("{point}:{round}")) {
        eprintln!("ATEMPORAL_FAIL_AT={point}:{round}: aborting");
        std::process::abort();
    }
}
static WRITES: AtomicU64 = AtomicU64::new(0);
fn disk_full() -> bool {
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    std::env::var("ATEMPORAL_DISK_FULL_AFTER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|limit| n >= limit)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if disk_full() {
        return Err(format!(
            "{}: No space left on device (injected)",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

// Stream large regenerable caches so export does not duplicate an entire revision in RAM.
fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<u64> {
    if disk_full() {
        return Err(format!(
            "{}: No space left on device (injected)",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let file = fs::File::create(&temp).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    writer.get_ref().sync_all().map_err(|e| e.to_string())?;
    let bytes = writer
        .get_ref()
        .metadata()
        .map_err(|e| e.to_string())?
        .len();
    fs::rename(temp, path).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_reader(std::io::BufReader::new(file))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn read_optional<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    if path.exists() {
        read_json(path).map(Some)
    } else {
        Ok(None)
    }
}

fn turn_key(name: &str) -> Option<(u32, PlayerId)> {
    let stem = name.strip_suffix(".json")?;
    let (round, player) = stem.split_once('-')?;
    Some((round.parse().ok()?, player.parse().ok()?))
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
    /// Reopen an archived match: pinned files, every turn/round record and private lobby data.
    pub fn open(replay_root: &Path, match_id: &str) -> Result<(Self, Loaded)> {
        let root = replay_root.join(match_id);
        let manifest: Manifest = read_json(&root.join("manifest.json"))?;
        let initial = read_json(&root.join(&manifest.initial_state))?;
        let config_yaml =
            fs::read_to_string(root.join("config.yaml")).map_err(|e| e.to_string())?;
        let content_yaml =
            fs::read_to_string(root.join("content.yaml")).map_err(|e| e.to_string())?;
        let mut turns = BTreeMap::new();
        let mut request_ids = BTreeMap::new();
        if let Ok(entries) = fs::read_dir(root.join("turns")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(id) = name.strip_suffix(".request") {
                    if let Some(key) = turn_key(&format!("{id}.json")) {
                        request_ids
                            .insert(key, fs::read_to_string(entry.path()).unwrap_or_default());
                    }
                } else if let Some(key) = turn_key(&name) {
                    // Interrupted temporary files never match `<round>-<player>.json`.
                    turns.insert(key, read_json::<AcceptedTurn>(&entry.path())?);
                }
            }
        }
        let mut rounds: Vec<RoundRecord> = vec![];
        if let Ok(entries) = fs::read_dir(root.join("rounds")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name
                    .strip_suffix(".json")
                    .is_some_and(|n| n.parse::<u32>().is_ok())
                {
                    rounds.push(read_json(&entry.path())?);
                }
            }
        }
        rounds.sort_by_key(|r| r.round);
        // Recovery commits at complete round records: a gap ends the usable history there.
        rounds.truncate(
            rounds
                .iter()
                .enumerate()
                .take_while(|(i, r)| r.round == *i as u32)
                .count(),
        );
        for record in &rounds {
            for name in &record.turns {
                let key = turn_key(name.trim_start_matches("turns/"));
                if key.is_none_or(|k| !turns.contains_key(&k)) {
                    return Err(format!("round {} references missing {name}", record.round));
                }
            }
        }
        let tokens = read_optional::<LobbyRecord>(&root.join("lobby.json"))?.map(|l| l.tokens);
        let archived = read_optional(&root.join("archive.json"))?;
        Ok((
            Self { root },
            Loaded {
                manifest,
                config_yaml,
                content_yaml,
                initial,
                turns,
                request_ids,
                rounds,
                tokens,
                archived,
            },
        ))
    }
    pub fn write_lobby(&self, record: &LobbyRecord) -> Result<()> {
        atomic_write(&self.root.join("lobby.json"), &json(record)?)
    }
    pub fn write_archive_record(&self, record: &ArchiveRecord) -> Result<()> {
        atomic_write(
            &self.root.join("archive.json"),
            &serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?,
        )
    }
    /// Flushed before acknowledgement; a partial round survives restart. The request id is
    /// written first so a recovered turn still answers the original commit idempotently.
    pub fn write_turn(&self, turn: &AcceptedTurn, request_id: &str) -> Result<String> {
        let name = format!("turns/{}-{}.json", turn.round, turn.player);
        atomic_write(
            &self
                .root
                .join(format!("turns/{}-{}.request", turn.round, turn.player)),
            request_id.as_bytes(),
        )?;
        atomic_write(
            &self.root.join(&name),
            &serde_json::to_vec_pretty(turn).map_err(|e| e.to_string())?,
        )?;
        Ok(name)
    }
    pub fn write_draft(&self, round: u32, player: PlayerId, draft: &TurnDraft) -> Result<()> {
        atomic_write(
            &self.root.join(format!("turns/{round}-{player}.draft")),
            &serde_json::to_vec(draft).map_err(|e| e.to_string())?,
        )
    }
    pub fn read_draft(&self, round: u32, player: PlayerId) -> Option<TurnDraft> {
        read_json(&self.root.join(format!("turns/{round}-{player}.draft"))).ok()
    }
    /// Removing the accepted input is the withdrawal's durable commit point.
    pub fn withdraw_turn(&self, round: u32, player: PlayerId) -> Result<()> {
        fs::remove_file(self.root.join(format!("turns/{round}-{player}.json")))
            .map_err(|e| e.to_string())?;
        fs::File::open(self.root.join("turns"))
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())
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
        // A stale marker from an interrupted earlier attempt must not vouch for mixed files.
        let _ = fs::remove_file(dir.join("complete.json"));
        let samples: Vec<&Sample> = data.samples.values().collect();
        let checkpoints: Vec<&WorldState> = data.checkpoints.values().collect();
        let stats: Vec<&StatsSample> = data.stats.values().collect();
        let mut sizes = ResultSizes {
            revision: data.revision,
            sample_count: samples.len(),
            event_count: data.events.len(),
            ..Default::default()
        };
        sizes.samples_bytes = atomic_json(&dir.join("samples.json"), &samples)?;
        sizes.checkpoints_bytes = atomic_json(&dir.join("checkpoints.json"), &checkpoints)?;
        sizes.stats_bytes = atomic_json(&dir.join("stats.json"), &stats)?;
        sizes.events_bytes = atomic_json(&dir.join("events.json"), &data.events)?;
        sizes.timeline_bytes = atomic_json(&dir.join("timeline.json"), &data.timeline)?;
        sizes.dictionary_bytes = atomic_json(&dir.join("dictionary.json"), &data.dictionary)?;
        atomic_write(&dir.join("complete.json"), &json(&sizes)?)?;
        sizes.write_ms = start.elapsed().as_millis() as u64;
        Ok(sizes)
    }
    /// A complete, parseable result cache for a revision; anything else counts as missing.
    pub fn read_results(&self, revision: Revision) -> Option<ResultCache> {
        let dir = self.root.join(format!("results/{revision}"));
        read_json::<ResultSizes>(&dir.join("complete.json")).ok()?;
        fn load<T: for<'de> Deserialize<'de>>(dir: &Path, name: &str) -> Option<T> {
            read_json(&dir.join(name)).ok()
        }
        Some(ResultCache {
            dictionary: load(&dir, "dictionary.json")?,
            samples: load(&dir, "samples.json")?,
            checkpoints: load(&dir, "checkpoints.json")?,
            stats: load(&dir, "stats.json")?,
            events: load(&dir, "events.json")?,
            timeline: load(&dir, "timeline.json")?,
        })
    }
    pub fn remove_results(&self, revision: Revision) {
        let _ = fs::remove_dir_all(self.root.join(format!("results/{revision}")));
    }
    /// Bytes per result directory on disk, oldest revision first.
    pub fn results_usage(&self) -> Vec<(Revision, u64)> {
        let mut usage = vec![];
        if let Ok(entries) = fs::read_dir(self.root.join("results")) {
            for entry in entries.flatten() {
                let Ok(revision) = entry.file_name().to_string_lossy().parse::<Revision>() else {
                    continue;
                };
                let bytes = fs::read_dir(entry.path())
                    .map(|files| {
                        files
                            .flatten()
                            .filter_map(|f| f.metadata().ok())
                            .map(|m| m.len())
                            .sum()
                    })
                    .unwrap_or(0);
                usage.push((revision, bytes));
            }
        }
        usage.sort();
        usage
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
