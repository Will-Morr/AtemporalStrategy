//! Small persistent directory and same-origin WebSocket relay to isolated match workers.
use crate::{bots::BotConfig, controller::now_ms, ws};
use atemporal_sim::*;
use axum::{
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Child, Command as Process, Stdio},
    sync::{Arc, Mutex},
};
use tokio_tungstenite::tungstenite::Message as RemoteMessage;

#[derive(Clone, Serialize, Deserialize)]
pub struct GameRecord {
    id: String,
    name: String,
    password: String,
    setup: Setup,
    bots: BotConfig,
    #[serde(default)]
    debug_test: bool,
    created: u64,
    #[serde(default)]
    expired: bool,
    #[serde(default)]
    members: BTreeMap<String, Vec<PlayerId>>,
}
#[derive(Default, Serialize, Deserialize)]
struct Directory {
    games: BTreeMap<String, GameRecord>,
    users: BTreeMap<String, String>,
}
#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct ReplayListing {
    time: u64,
    game: String,
    id: String,
    name: String,
    finished: bool,
    locked: bool,
    players: Vec<String>,
}
struct Worker {
    child: Child,
    port: u16,
    readers: usize,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[derive(Clone)]
pub struct Coordinator {
    root: PathBuf,
    client: PathBuf,
    guide: PathBuf,
    content: PathBuf,
    prose: PathBuf,
    defaults: Setup,
    test_server: bool,
    directory: Arc<Mutex<Directory>>,
    workers: Arc<Mutex<BTreeMap<String, Worker>>>,
    admissions: Arc<Mutex<Admissions>>,
}
#[derive(Default)]
struct Admissions {
    count: usize,
}
struct Admission(Arc<Mutex<Admissions>>);
impl Admission {
    fn new(c: &Coordinator) -> Self {
        c.admissions.lock().unwrap().count += 1;
        Self(c.admissions.clone())
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.lock().unwrap().count -= 1;
    }
}
fn error(message: impl ToString) -> Response {
    Json(json!({"error":message.to_string()})).into_response()
}
fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            let (k, v) = c.trim().split_once('=')?;
            (k == name).then(|| v.to_owned())
        })
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn access(game: &GameRecord) -> String {
    identity::canonical_hash(&(game.id.clone(), game.password.clone())).unwrap()
}
fn allowed(game: &GameRecord, h: &HeaderMap) -> bool {
    game.password.is_empty()
        || cookie(h, &format!("game-{}", game.id)).as_deref() == Some(access(game).as_str())
}
impl Coordinator {
    pub fn new(
        root: PathBuf,
        client: PathBuf,
        guide: PathBuf,
        content: PathBuf,
        prose: PathBuf,
        defaults: Setup,
        test_server: bool,
    ) -> Result<Self> {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let path = root.join("directory.json");
        let directory = if path.exists() {
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("directory: {e}"))?
        } else {
            Directory::default()
        };
        let coordinator = Self {
            root,
            client,
            guide,
            content,
            prose,
            defaults,
            test_server,
            directory: Arc::new(Mutex::new(directory)),
            workers: Arc::new(Mutex::new(BTreeMap::new())),
            admissions: Arc::new(Mutex::new(Admissions { count: 0 })),
        };
        // Resume open/active games even if no browser has returned yet (bot playtests included).
        let games: Vec<_> = coordinator
            .directory
            .lock()
            .unwrap()
            .games
            .values()
            .cloned()
            .collect();
        for game in games {
            let replays = coordinator.game_root(&game.id).join("replays");
            if replays.is_dir() {
                for entry in std::fs::read_dir(replays).map_err(|e| e.to_string())? {
                    crate::archive::migrate_completion(&entry.map_err(|e| e.to_string())?.path())?;
                }
            }
            if !coordinator.expired(&game, now_ms())
                && !matches!(
                    coordinator.status(&game.id)["phase"].as_str(),
                    Some("finished" | "archived")
                )
            {
                coordinator.worker(&game, None)?;
            }
        }
        Ok(coordinator)
    }
    fn expired(&self, game: &GameRecord, now: u64) -> bool {
        game.expired
            || now.saturating_sub(
                self.status(&game.id)["last_activity_ms"]
                    .as_u64()
                    .unwrap_or(game.created),
            ) >= 24 * 60 * 60 * 1000
    }
    fn active_players(&self, game: &GameRecord, status: &Value) -> usize {
        // Persisted connection bits alone are stale after a coordinator crash.
        if !self
            .workers
            .lock()
            .unwrap()
            .get(&format!("{}:live", game.id))
            .is_some_and(|w| w.readers > 0)
        {
            return 0;
        }
        status["slots"].as_array().map_or(0, |slots| {
            slots
                .iter()
                .filter(|s| {
                    let slot = s["slot"].as_u64().unwrap_or(u64::MAX) as usize;
                    s["connected"] == true
                        && s["claimed"] == true
                        && game.bots.slots.get(slot).is_none_or(Option::is_none)
                })
                .count()
        })
    }
    /// Local supervisor handshake. No unauthenticated Internet restart endpoint.
    pub async fn maintain(self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut changed = false;
        loop {
            interval.tick().await;
            let mut d = self.directory.lock().unwrap();
            for game in d.games.values_mut() {
                if !game.expired && self.expired(game, now_ms()) {
                    game.expired = true;
                    changed = true;
                    self.workers
                        .lock()
                        .unwrap()
                        .remove(&format!("{}:live", game.id));
                }
            }
            if changed {
                match self.save(&d) {
                    Ok(()) => changed = false,
                    Err(e) => eprintln!("directory expiry: {e}"),
                }
            }
            // Hold admission lock until exit: no creation can race the idle decision.
            let admissions = self.admissions.lock().unwrap();
            // Connected players don't block updates: turns are on disk and browsers
            // reload with their uncommitted drafts. Only creations/feedback and running rounds wait.
            if self.root.join("update-request").exists()
                && admissions.count == 0
                && !d
                    .games
                    .values()
                    .any(|g| !g.expired && self.status(&g.id)["phase"] == "simulating")
            {
                eprintln!("Update ready; server idle, exiting for supervisor");
                self.workers.lock().unwrap().clear();
                std::process::exit(75);
            }
        }
    }
    fn save(&self, d: &Directory) -> Result<()> {
        atomic_json(&self.root.join("directory.json"), d)
    }
    fn game(&self, id: &str) -> Result<GameRecord> {
        self.directory
            .lock()
            .unwrap()
            .games
            .get(id)
            .filter(|g| !self.expired(g, now_ms()))
            .cloned()
            .ok_or_else(|| "Game not found".into())
    }
    fn reviewable_game(&self, id: &str, replay: Option<&str>) -> Result<GameRecord> {
        let directory = self.directory.lock().unwrap();
        let game = directory.games.get(id).ok_or("Game not found")?;
        if replay.is_none() && !self.expired(game, now_ms()) {
            return Ok(game.clone());
        }
        if self
            .replay_catalog(&directory)
            .iter()
            .any(|r| r.game == id && replay.is_none_or(|id| r.id == id))
        {
            Ok(game.clone())
        } else {
            Err("Replay not found".into())
        }
    }

    fn replay_catalog(&self, directory: &Directory) -> Vec<ReplayListing> {
        let mut completed = Vec::new();
        let mut archived = Vec::new();
        for game in directory.games.values() {
            let Ok(entries) = std::fs::read_dir(self.game_root(&game.id).join("replays")) else {
                continue;
            };
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                let root = entry.path();
                if !valid_id(&id) || !root.join("manifest.json").is_file() {
                    continue;
                }
                let completion = std::fs::read(root.join("completed.json"))
                    .ok()
                    .and_then(|data| serde_json::from_slice::<u64>(&data).ok());
                let (time, finished) = match completion {
                    Some(time) => (time, true),
                    None => {
                        let Some(time) = std::fs::read(root.join("archive.json"))
                            .ok()
                            .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
                            .and_then(|a| a["stopped_at_unix_ms"].as_u64())
                        else {
                            continue;
                        };
                        if now_ms().saturating_sub(time) >= 24 * 60 * 60 * 1000 {
                            continue;
                        }
                        (time, false)
                    }
                };
                let listing = ReplayListing {
                    time,
                    game: game.id.clone(),
                    id,
                    name: game.name.clone(),
                    finished,
                    players: Vec::new(),
                    locked: !game.password.is_empty(),
                };
                if finished {
                    completed.push(listing);
                } else {
                    archived.push(listing);
                }
            }
        }
        completed.sort_unstable_by(|a, b| b.cmp(a));
        completed.truncate(5);
        archived.sort_unstable_by(|a, b| b.cmp(a));
        completed.extend(archived);
        for row in &mut completed {
            let manifest = self
                .game_root(&row.game)
                .join("replays")
                .join(&row.id)
                .join("manifest.json");
            if let Ok(bytes) = std::fs::read(manifest)
                && let Ok(data) = serde_json::from_slice::<Value>(&bytes)
                && let Some(profiles) = data["profiles"].as_array()
            {
                row.players = profiles
                    .iter()
                    .filter_map(|p| p["username"].as_str().map(str::to_owned))
                    .collect();
            }
        }

        completed
    }
    /// Appends who did what to visits.jsonl for counting unique users; best effort.
    fn log_visit(&self, h: &HeaderMap, event: &str, detail: &str) {
        use std::io::Write;
        let get = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("");
        let line = json!({"ms":now_ms(),"event":event,"detail":detail,"ip":get("x-forwarded-for"),"ua":get("user-agent"),"lang":get("accept-language"),"user":cookie(h,"atemporal-user"),"fp":cookie(h,"atemporal-fp")});
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("visits.jsonl"));
        if let Ok(mut file) = file {
            let _ = writeln!(file, "{line}");
        }
    }
    fn game_root(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }
    fn status(&self, id: &str) -> Value {
        std::fs::read(self.game_root(id).join("status.json"))
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or(json!({"phase":"lobby","round":0}))
    }
    fn worker(&self, game: &GameRecord, replay: Option<&str>) -> Result<u16> {
        if game.setup.match_defaults.simulation_threads
            > self.defaults.match_defaults.simulation_threads
        {
            return Err("Game exceeds this host's simulation thread budget".into());
        }
        let key = format!("{}:{}", game.id, replay.unwrap_or("live"));
        let mut workers = self.workers.lock().unwrap();
        if replay.is_none() && self.expired(game, now_ms()) {
            return Err("Game expired".into());
        }
        if let Some(w) = workers.get_mut(&key)
            && w.child.try_wait().map_err(|e| e.to_string())?.is_none()
        {
            return Ok(w.port);
        }
        workers.remove(&key);
        let dir = self.game_root(&game.id);
        let replays = dir.join("replays");
        std::fs::create_dir_all(&replays).map_err(|e| e.to_string())?;
        if replay.is_none() {
            atomic_json(&dir.join("setup.json"), &game.setup)?;
            atomic_json(&dir.join("bots.json"), &game.bots)?;
        }
        let pinned_content = dir.join("content.yaml");
        if !pinned_content.exists() {
            std::fs::copy(&self.content, &pinned_content).map_err(|e| e.to_string())?;
        }
        let saved = self.status(&game.id);
        let latest = if let Some(r) = replay {
            Some(r.to_owned())
        } else if saved["match_id"].as_str().is_none() {
            let mut ids: Vec<_> = std::fs::read_dir(&replays)
                .map_err(|e| e.to_string())?
                .flatten()
                .filter(|e| e.path().join("manifest.json").exists())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|id| valid_id(id))
                .collect();
            ids.sort();
            ids.pop()
        } else {
            saved["match_id"]
                .as_str()
                .filter(|id| valid_id(id) && replays.join(id).join("manifest.json").exists())
                .map(str::to_owned)
        };
        if let Some(r) = &latest
            && (!valid_id(r) || !replays.join(r).join("manifest.json").exists())
        {
            return Err("Replay not found".into());
        }
        // The child binds only loopback. Its public address never depends on this ephemeral port.
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let port = probe.local_addr().map_err(|e| e.to_string())?.port();
        drop(probe);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("worker.log"))
            .map_err(|e| e.to_string())?;
        let mut command = Process::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .args([
                "--single-match",
                "--managed",
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ])
            .arg("--config")
            .arg(dir.join("setup.json"))
            .arg("--content")
            .arg(&pinned_content)
            .arg("--client")
            .arg(&self.client)
            .arg("--prose")
            .arg(&self.prose)
            .arg("--guide-dir")
            .arg(dir.join("guide"))
            .arg("--replays")
            .arg(&replays)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
            .stderr(Stdio::from(log));
        if let Some(r) = latest {
            command.args(["--resume", &r]);
        }
        if replay.is_none() && self.test_server && game.debug_test {
            command.arg("--debug-test-opening");
        }
        if replay.is_none() {
            command
                .arg("--bots")
                .arg(dir.join("bots.json"))
                .arg("--status-file")
                .arg(dir.join("status.json"))
                .arg("--lobby-file")
                .arg(dir.join("lobby.json"));
        }
        let child = command.spawn().map_err(|e| e.to_string())?;
        workers.insert(
            key,
            Worker {
                child,
                port,
                readers: 0,
            },
        );
        Ok(port)
    }
}
pub fn atomic_json(path: &std::path::Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub fn router(c: Coordinator) -> Router {
    Router::new()
        .route("/api/directory", get(directory))
        .route("/api/identity", post(identity_update))
        .route("/api/games", post(create))
        .route("/api/games/{id}/join", post(join))
        .route("/api/games/{id}/feedback", post(feedback))
        .route("/games/{id}/guide/", get(game_guide))
        .route("/games/{id}/guide/{*path}", get(game_guide_file))
        .route("/games/{id}/ws", get(game_ws))
        .route(
            "/games/{id}/ws/practice/{source}/{rev}/{player}",
            get(game_practice_ws),
        )
        .route(
            "/games/{id}/replays/{replay}/ws/practice/{source}/{rev}/{player}",
            get(replay_practice_ws),
        )
        .route("/games/{id}/", get(game_page))
        .route("/games/{id}/replays/{replay}/ws", get(replay_ws))
        .route("/games/{id}/replays/{replay}/", get(replay_page))
        .route("/guide/", get(guide))
        .route("/guide/{*path}", get(guide_file))
        .fallback(get(asset))
        .with_state(c)
}
async fn directory(State(c): State<Coordinator>, h: HeaderMap) -> Response {
    let d = c.directory.lock().unwrap();
    let user = cookie(&h, "atemporal-user");
    let replays = c.replay_catalog(&d);
    let games: Vec<_> = d
        .games
        .values()
        .filter(|g| !c.expired(g, now_ms()))
        .map(|g| {
            let status = c.status(&g.id);
            let mine = user
                .as_ref()
                .and_then(|u| g.members.get(u))
                .is_some_and(|owned| {
                    status["slots"].as_array().is_some_and(|slots| {
                        slots.iter().any(|s| {
                            s["claimed"] == true && owned.iter().any(|id| s["slot"] == *id)
                        })
                    })
                });
            let game_replays: Vec<_> = replays
                .iter()
                .filter(|r| r.game == g.id)
                .map(|r| &r.id)
                .collect();
            json!({
                "active_players": c.active_players(g, &status), "id": g.id, "name": g.name,
                "created": g.created, "locked": !g.password.is_empty(), "status": status,
                "players": g.setup.match_defaults.player_count, "bots": g.bots.slots,
                "mine": mine, "replays": game_replays
            })
        })
        .collect();
    Json(json!({"games":games,"replays":replays,"user":user.as_ref().and_then(|u|d.users.get(u)),"defaults":c.defaults,"default_bot":crate::bots::DEFAULT_STYLE,"debug_test":c.test_server})).into_response()
}
#[derive(Deserialize)]
struct IdentityInput {
    username: String,
}
async fn identity_update(
    State(c): State<Coordinator>,
    h: HeaderMap,
    Json(input): Json<IdentityInput>,
) -> Response {
    let mut d = c.directory.lock().unwrap();
    let id = cookie(&h, "atemporal-user")
        .filter(|s| d.users.contains_key(s))
        .unwrap_or_else(|| format!("user-{}-{}", now_ms(), d.users.len()));
    d.users.insert(id.clone(), input.username.clone());
    c.log_visit(&h, "identity", &id);
    if let Err(e) = c.save(&d) {
        return error(e);
    }
    (
        [(
            header::SET_COOKIE,
            format!("atemporal-user={id}; Path=/; SameSite=Lax; Max-Age=31536000; HttpOnly"),
        )],
        Json(json!({"username":input.username})),
    )
        .into_response()
}
#[derive(Deserialize)]
struct CreateInput {
    name: String,
    #[serde(default)]
    password: String,
    setup: Setup,
    #[serde(default)]
    bots: BotConfig,
    /// Append " v{n}", counting earlier games with this name (expired ones included).
    #[serde(default)]
    numbered: bool,
    #[serde(default)]
    debug_test: bool,
}
async fn create(
    State(c): State<Coordinator>,
    h: HeaderMap,
    Json(mut input): Json<CreateInput>,
) -> Response {
    let _admission = Admission::new(&c);
    if input.debug_test {
        if !c.test_server {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error":"DEBUG_TEST requires --test-server."})),
            )
                .into_response();
        }
        input.setup = c.defaults.clone();
        input.setup.match_defaults.player_count = 2;
        input.setup.match_defaults.multiplayer = Multiplayer::Ffa {};
        input.setup.match_defaults.transport = Transport::Authoritative;
        input.setup.match_defaults.objective = Objective::Scoreboard {
            rules: ScoreboardRules::default(),
        };
        input.setup.available_teams.clear();
        input.bots = BotConfig {
            slots: vec![None, Some(crate::bots::DEFAULT_STYLE)],
            playtest_rounds: 20,
        };
    }
    // A browser may tune game rules, but cannot allocate more simulation threads than
    // the host's trusted startup configuration. Large values otherwise spawn one OS
    // thread per entity once parallel intent processing begins.
    if input.setup.match_defaults.simulation_threads > c.defaults.match_defaults.simulation_threads
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":format!(
                "This host allows at most {} simulation threads per game.",
                c.defaults.match_defaults.simulation_threads
            )})),
        )
            .into_response();
    }
    // Match settings are validated by the same loader as standalone YAML, never by UI alone.
    input.setup.match_defaults.replay_directory = "replays".into();
    let serialized = match serde_json::to_string(&input.setup) {
        Ok(s) => s,
        Err(e) => return error(e),
    };
    if let Err(e) = atemporal_content::load_setup(&serialized) {
        return error(e);
    }
    if input.setup.match_defaults.transport != Transport::Authoritative {
        return error(
            "Coordinated browser games use authoritative transport; use standalone mode for peripherals.",
        );
    }
    if let Err(e) = input.bots.validate(input.setup.match_defaults.player_count) {
        return error(e);
    }
    if input.name.len() > 200 || input.password.len() > 200 {
        return error("Game names and passwords may be up to 200 characters.");
    }
    let game = {
        let mut d = c.directory.lock().unwrap();
        let id = format!("game-{}-{}", now_ms(), d.games.len());
        let game = GameRecord {
            id: id.clone(),
            name: if input.name.is_empty() {
                "New game".into()
            } else if input.numbered {
                let prefix = format!("{} v", input.name);
                let n = d
                    .games
                    .values()
                    .filter(|g| g.name.starts_with(&prefix))
                    .count()
                    + 1;
                format!("{prefix}{n}")
            } else {
                input.name
            },
            password: input.password,
            setup: input.setup,
            bots: input.bots,
            debug_test: input.debug_test,
            created: now_ms(),
            expired: false,
            members: BTreeMap::new(),
        };
        c.log_visit(&h, "create", &id);
        d.games.insert(id, game.clone());
        if let Err(e) = c.save(&d) {
            return error(e);
        }
        game
    };
    let worker_c = c.clone();
    let worker_game = game.clone();
    match tokio::task::spawn_blocking(move || worker_c.worker(&worker_game, None)).await {
        Ok(Ok(_)) => (
            [(
                header::SET_COOKIE,
                format!(
                    "game-{}={}; Path=/; SameSite=Lax; HttpOnly",
                    game.id,
                    access(&game)
                ),
            )],
            Json(json!({"url":format!("/games/{}/",game.id)})),
        )
            .into_response(),
        e => error(format!("Could not start game: {e:?}")),
    }
}
#[derive(Deserialize)]
struct FeedbackInput {
    match_id: String,
    submission_id: String,
    message: String,
    revision: u64,
}
async fn feedback(
    State(c): State<Coordinator>,
    Path(id): Path<String>,
    h: HeaderMap,
    Json(input): Json<FeedbackInput>,
) -> Response {
    let _admission = Admission::new(&c);
    let game = match c.reviewable_game(&id, None) {
        Ok(g) => g,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !allowed(&game, &h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !valid_id(&input.match_id)
        || !valid_id(&input.submission_id)
        || input.message.trim().is_empty()
        || input.message.chars().count() > 5000
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"Please enter a message of 1–5000 characters."})),
        )
            .into_response();
    }
    let root = c.game_root(&id).join("replays").join(&input.match_id);
    if !root.join("manifest.json").is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Serialize submissions with directory writes, including same-ID retries. Feedback is
    // never returned by directory APIs and its storage is outside the served assets.
    let d = c.directory.lock().unwrap();
    let Some(user) = cookie(&h, "atemporal-user").filter(|u| d.users.contains_key(u)) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let record = json!({"submission_id":input.submission_id,"game_id":id,"match_id":input.match_id,
        "revision":input.revision,"message":input.message,"user_id":user,"username":d.users[&user],
        "received_at_ms":now_ms(),"log_path":format!("{id}/replays/{}",input.match_id)});
    let result = (|| -> Result<()> {
        let folder = root.join("feedback");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join(format!("{}.json", input.submission_id));
        if path.exists() {
            let previous: Value =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if previous["message"] != input.message || previous["user_id"] != user {
                return Err("Submission ID already used. Please reopen the form.".into());
            }
        } else {
            atomic_json(&path, &record)?;
        }
        std::fs::File::open(&path)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::File::open(&folder)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    match result {
        Ok(()) => Json(json!({"saved":true})).into_response(),
        Err(e) => {
            eprintln!("feedback: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"Could not save feedback. Your message is still here; please retry."}))).into_response()
        }
    }
}
#[derive(Deserialize)]
struct JoinInput {
    #[serde(default)]
    password: String,
}
async fn join(
    State(c): State<Coordinator>,
    Path(id): Path<String>,
    h: HeaderMap,
    Json(input): Json<JoinInput>,
) -> Response {
    let game = match c.reviewable_game(&id, None) {
        Ok(g) => g,
        Err(e) => return error(e),
    };
    if !allowed(&game, &h) && input.password != game.password {
        return error("Incorrect lobby password.");
    }
    (
        [(
            header::SET_COOKIE,
            format!(
                "game-{id}={}; Path=/; SameSite=Lax; HttpOnly",
                access(&game)
            ),
        )],
        Json(json!({"url":format!("/games/{id}/")})),
    )
        .into_response()
}
async fn game_page(State(c): State<Coordinator>, Path(id): Path<String>, h: HeaderMap) -> Response {
    page(c, id, None, h).await
}
async fn replay_page(
    State(c): State<Coordinator>,
    Path((id, replay)): Path<(String, String)>,
    h: HeaderMap,
) -> Response {
    page(c, id, Some(replay), h).await
}
async fn page(c: Coordinator, id: String, replay: Option<String>, h: HeaderMap) -> Response {
    let game = match replay
        .as_ref()
        .map_or_else(|| c.game(&id), |r| c.reviewable_game(&id, Some(r)))
    {
        Ok(g) => g,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !allowed(&game, &h) {
        // IDs are generated internally; user-provided names never enter HTML.
        return Html(format!(r#"<!doctype html><meta charset="utf-8"><title>Lobby password</title><body style="background:#0e141c;color:#edf4fa;font:18px system-ui;padding:4rem"><h1>Lobby password</h1><form><label>Password <input type="password" autofocus></label> <button>Enter lobby</button><p role="alert"></p></form><a href="/lobbies/">All games</a><script>document.querySelector('form').onsubmit=async e=>{{e.preventDefault();const r=await fetch('/api/games/{id}/join',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{password:document.querySelector('input').value}})}});const d=await r.json();if(d.error)document.querySelector('p').textContent=d.error;else location.reload();}};</script>"#)).into_response();
    }
    ws::serve(&c.client, "index.html").await
}
async fn game_ws(
    State(c): State<Coordinator>,
    Path(id): Path<String>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    relay(c, id, None, None, h, upgrade).await
}
async fn replay_ws(
    State(c): State<Coordinator>,
    Path((id, replay)): Path<(String, String)>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    relay(c, id, Some(replay), None, h, upgrade).await
}
async fn game_practice_ws(
    State(c): State<Coordinator>,
    Path((id, source, rev, player)): Path<(String, String, Revision, PlayerId)>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    relay(c, id, None, Some((source, rev, player)), h, upgrade).await
}
async fn replay_practice_ws(
    State(c): State<Coordinator>,
    Path((id, replay, source, rev, player)): Path<(String, String, String, Revision, PlayerId)>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    relay(c, id, Some(replay), Some((source, rev, player)), h, upgrade).await
}
async fn relay(
    c: Coordinator,
    id: String,
    replay: Option<String>,
    practice: Option<(String, Revision, PlayerId)>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let game = match replay
        .as_ref()
        .map_or_else(|| c.game(&id), |r| c.reviewable_game(&id, Some(r)))
    {
        Ok(g) => g,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !allowed(&game, &h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let readonly = replay.is_some();
    let practice_route = match &practice {
        Some((source, rev, player)) if valid_id(source) => {
            format!("/ws/practice/{source}/{rev}/{player}")
        }
        Some(_) => return StatusCode::BAD_REQUEST.into_response(),
        None => "/ws".to_owned(),
    };
    let worker_key = format!("{}:{}", id, replay.as_deref().unwrap_or("live"));
    c.log_visit(&h, if readonly { "replay" } else { "game" }, &worker_key);
    let wc = c.clone();
    let wg = game.clone();
    let port = match tokio::task::spawn_blocking(move || wc.worker(&wg, replay.as_deref())).await {
        Ok(Ok(p)) => p,
        _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    // Connect before upgrading, so a failed worker never leaves a half-open browser socket.
    let mut remote = None;
    for _ in 0..100 {
        if let Ok((socket, _)) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}{practice_route}")).await
        {
            remote = Some(socket);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let Some(remote) = remote else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let user = cookie(&h, "atemporal-user");
    upgrade.max_message_size(64<<20).on_upgrade(move|mut browser|async move {
        if let Some(worker) = c.workers.lock().unwrap().get_mut(&worker_key) && worker.port == port { worker.readers += 1; }
        let (mut tx,mut rx)=remote.split();
        loop {
            tokio::select! {
                incoming=browser.recv()=>{
                    match incoming {
                        Some(Ok(Message::Text(t)))=>{
                            let mut value:Value=match serde_json::from_str(t.as_str()){Ok(v)=>v,Err(_)=>continue};
                            if readonly && practice.is_none() {
                                let kind=value["message"]["kind"].as_str().unwrap_or("");
                                if kind=="hello" {value["message"]["slot_token"]=Value::Null;}
                                else if !kind.starts_with("get_"){continue;}
                            }
                            if tx.send(RemoteMessage::Text(value.to_string().into())).await.is_err(){break;}
                        },
                        Some(Ok(Message::Ping(t)))=>{if browser.send(Message::Pong(t)).await.is_err(){break;}},
                        Some(Ok(Message::Pong(_)))=>{},
                        _=>break,
                    }
                },
                outgoing=rx.next()=>{
                    match outgoing {
                        Some(Ok(RemoteMessage::Text(t)))=>{
                            if practice.is_none() && !readonly && t.len() < 1024 && let Some(user)=&user && let Ok(v)=serde_json::from_str::<Value>(t.as_str()) && v["message"]["kind"]=="slot_claimed" && let Some(slot)=v["message"]["slot"].as_u64() {
                                let mut d=c.directory.lock().unwrap();
                                if d.users.contains_key(user)&&let Some(g)=d.games.get_mut(&id){
                                    // A slot is owned by one browser user; reconnect updates this mapping.
                                    for slots in g.members.values_mut(){slots.retain(|s|u64::from(*s)!=slot);}
                                    g.members.entry(user.clone()).or_default().push(slot as u8);
                                    if let Err(e)=c.save(&d){eprintln!("directory membership: {e}");}
                                }
                            }
                            if browser.send(Message::Text(t.to_string().into())).await.is_err(){break;}
                        },
                        Some(Ok(RemoteMessage::Binary(b)))=>{if browser.send(Message::Binary(b)).await.is_err(){break;}},
                        Some(Ok(RemoteMessage::Ping(b)))=>{if tx.send(RemoteMessage::Pong(b)).await.is_err(){break;}},
                        Some(Ok(RemoteMessage::Pong(_)))=>{},
                        _=>break,
                    }
                }
            }
        }
        let _=tx.close().await;
        let _=browser.close().await;
        let mut workers = c.workers.lock().unwrap();
        if let Some(worker) = workers.get_mut(&worker_key) && worker.port == port {
            worker.readers = worker.readers.saturating_sub(1);
            if readonly && worker.readers == 0 { workers.remove(&worker_key); }
        }
    })
}
async fn guide(State(c): State<Coordinator>) -> Response {
    ws::serve(&c.guide, "index.html").await
}
async fn guide_file(State(c): State<Coordinator>, Path(path): Path<String>) -> Response {
    ws::serve(&c.guide, &path).await
}
async fn asset(State(c): State<Coordinator>, uri: axum::http::Uri) -> Response {
    match uri.path() {
        "/" => ws::serve(&c.client, "landing.html").await,
        "/lobbies/" | "/replays/" => ws::serve(&c.client, "directory.html").await,
        "/replays" => Redirect::to("/replays/").into_response(),
        "/lobbies" => Redirect::to("/lobbies/").into_response(),
        "/guide" => Redirect::to("/guide/").into_response(),
        p => ws::serve(&c.client, p.trim_start_matches('/')).await,
    }
}

async fn game_guide(
    State(c): State<Coordinator>,
    Path(id): Path<String>,
    h: HeaderMap,
) -> Response {
    game_guide_response(c, id, "index.html".into(), h).await
}
async fn game_guide_file(
    State(c): State<Coordinator>,
    Path((id, path)): Path<(String, String)>,
    h: HeaderMap,
) -> Response {
    game_guide_response(c, id, path, h).await
}
async fn game_guide_response(c: Coordinator, id: String, path: String, h: HeaderMap) -> Response {
    let game = match c.reviewable_game(&id, None) {
        Ok(g) => g,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !allowed(&game, &h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws::serve(&c.game_root(&id).join("guide"), &path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_gate_is_per_game_and_rejects_unrelated_cookies() {
        let setup =
            atemporal_content::load_setup(include_str!("../../../config/game.yaml")).unwrap();
        let game = GameRecord {
            id: "game-one".into(),
            name: "Test".into(),
            password: "casual".into(),
            setup,
            bots: BotConfig::default(),
            debug_test: false,
            created: 0,
            expired: false,
            members: BTreeMap::new(),
        };
        let mut h = HeaderMap::new();
        assert!(!allowed(&game, &h));
        h.insert(
            header::COOKIE,
            format!("game-other={}", access(&game)).parse().unwrap(),
        );
        assert!(!allowed(&game, &h));
        h.insert(
            header::COOKIE,
            format!("game-game-one={}; other=cookie", access(&game))
                .parse()
                .unwrap(),
        );
        assert!(allowed(&game, &h));
        let mut changed = game.clone();
        changed.password = "changed".into();
        assert!(!allowed(&changed, &h));
        changed.password.clear();
        assert!(allowed(&changed, &HeaderMap::new()));
    }
    #[test]
    fn expiry_is_durable_and_uses_activity_not_server_restart_time() {
        let root = std::env::temp_dir().join(format!(
            "atemporal-expiry-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let setup =
            atemporal_content::load_setup(include_str!("../../../config/game.yaml")).unwrap();
        let c = Coordinator::new(
            root.clone(),
            root.clone(),
            root.clone(),
            root.clone(),
            root.clone(),
            setup.clone(),
            false,
        )
        .unwrap();
        let mut game = GameRecord {
            id: "game-expiry".into(),
            name: "Expiry".into(),
            password: String::new(),
            setup,
            bots: BotConfig::default(),
            debug_test: false,
            created: 1000,
            expired: false,
            members: BTreeMap::new(),
        };
        let one_day = 24 * 60 * 60 * 1000;
        assert!(!c.expired(&game, 1000 + one_day - 1));
        assert!(c.expired(&game, 1000 + one_day));
        std::fs::create_dir_all(c.game_root(&game.id)).unwrap();
        atomic_json(
            &c.game_root(&game.id).join("status.json"),
            &json!({"last_activity_ms":2000,"phase":"finished"}),
        )
        .unwrap();
        assert!(!c.expired(&game, 1000 + one_day));
        assert!(c.expired(&game, 2000 + one_day));
        game.expired = true;
        assert!(c.expired(&game, 2000)); // A clock change cannot resurrect a tombstone.
        assert_eq!(
            c.active_players(
                &game,
                &json!({"slots":[{"slot":0,"claimed":true,"connected":true}]})
            ),
            0
        );
        let admission = Admission::new(&c);
        assert_eq!(c.admissions.lock().unwrap().count, 1);
        drop(admission);
        assert_eq!(c.admissions.lock().unwrap().count, 0);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn public_creation_rejects_host_thread_amplification_before_spawning() {
        let root = std::env::temp_dir().join(format!(
            "atemporal-security-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let setup =
            atemporal_content::load_setup(include_str!("../../../config/game.yaml")).unwrap();
        let budget = setup.match_defaults.simulation_threads;
        let c = Coordinator::new(
            root.clone(),
            root.clone(),
            root.clone(),
            root.clone(),
            root.clone(),
            setup.clone(),
            false,
        )
        .unwrap();
        let mut attack = setup;
        attack.match_defaults.simulation_threads = u16::MAX;
        attack.match_defaults.replay_directory = "/tmp/attacker-controlled".into();
        let response = create(
            State(c.clone()),
            HeaderMap::new(),
            Json(CreateInput {
                name: "oversized public thread request".into(),
                password: String::new(),
                setup: attack,
                bots: BotConfig::default(),
                debug_test: false,
                numbered: false,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains(&format!("at most {budget}")));
        assert!(c.directory.lock().unwrap().games.is_empty());
        assert!(c.workers.lock().unwrap().is_empty());
        let mut legacy = GameRecord {
            id: "game-legacy".into(),
            name: "old unbounded setup".into(),
            password: String::new(),
            setup: c.defaults.clone(),
            bots: BotConfig::default(),
            debug_test: false,
            created: now_ms(),
            expired: false,
            members: BTreeMap::new(),
        };
        legacy.setup.match_defaults.simulation_threads = u16::MAX;
        assert!(
            c.worker(&legacy, None)
                .unwrap_err()
                .contains("thread budget")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn replay_identifiers_cannot_escape_the_game_directory() {
        assert!(valid_id("match-123-abcdef"));
        for id in ["..", "../manifest", "/etc/passwd", "a/b", "a%2fb", ""] {
            assert!(!valid_id(id));
        }
    }
}
