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
    created: u64,
    #[serde(default)]
    members: BTreeMap<String, Vec<PlayerId>>,
}
#[derive(Default, Serialize, Deserialize)]
struct Directory {
    games: BTreeMap<String, GameRecord>,
    users: BTreeMap<String, String>,
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
    directory: Arc<Mutex<Directory>>,
    workers: Arc<Mutex<BTreeMap<String, Worker>>>,
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
            directory: Arc::new(Mutex::new(directory)),
            workers: Arc::new(Mutex::new(BTreeMap::new())),
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
            if !matches!(
                coordinator.status(&game.id)["phase"].as_str(),
                Some("finished" | "archived")
            ) {
                coordinator.worker(&game, None)?;
            }
        }
        Ok(coordinator)
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
            .cloned()
            .ok_or_else(|| "Game not found".into())
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
        let key = format!("{}:{}", game.id, replay.unwrap_or("live"));
        let mut workers = self.workers.lock().unwrap();
        if let Some(w) = workers.get_mut(&key)
            && w.child.try_wait().map_err(|e| e.to_string())?.is_none()
        {
            return Ok(w.port);
        }
        workers.remove(&key);
        let dir = self.game_root(&game.id);
        let replays = dir.join("replays");
        std::fs::create_dir_all(&replays).map_err(|e| e.to_string())?;
        atomic_json(&dir.join("setup.json"), &game.setup)?;
        let pinned_content = dir.join("content.yaml");
        if !pinned_content.exists() {
            std::fs::copy(&self.content, &pinned_content).map_err(|e| e.to_string())?;
        }
        atomic_json(&dir.join("bots.json"), &game.bots)?;
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
        .route("/games/{id}/guide/", get(game_guide))
        .route("/games/{id}/guide/{*path}", get(game_guide_file))
        .route("/games/{id}/ws", get(game_ws))
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
    let games:Vec<_>=d.games.values().map(|g|{
        let status=c.status(&g.id);
        let mut replays=vec![];
        if let Ok(entries)=std::fs::read_dir(c.game_root(&g.id).join("replays")) {
            for entry in entries.flatten(){let id=entry.file_name().to_string_lossy().to_string();
                if valid_id(&id)&&entry.path().join("manifest.json").exists()&&(status["match_id"]!=id||matches!(status["phase"].as_str(),Some("finished"|"archived"))){replays.push(id);}
            }
        }
        replays.sort();replays.reverse();replays.truncate(12);
        json!({"id":g.id,"name":g.name,"created":g.created,"locked":!g.password.is_empty(),"status":status,"players":g.setup.match_defaults.player_count,"bots":g.bots.slots,"mine":user.as_ref().is_some_and(|u|g.members.get(u).is_some_and(|slots|slots.iter().any(|slot|status["slots"].as_array().is_some_and(|ss|ss.iter().any(|s|s["slot"]==*slot&&s["claimed"]==true))))),"replays":replays})
    }).collect();
    Json(json!({"games":games,"user":user.as_ref().and_then(|u|d.users.get(u)),"defaults":c.defaults})).into_response()
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
}
async fn create(State(c): State<Coordinator>, Json(mut input): Json<CreateInput>) -> Response {
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
            } else {
                input.name
            },
            password: input.password,
            setup: input.setup,
            bots: input.bots,
            created: now_ms(),
            members: BTreeMap::new(),
        };
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
    let game = match c.game(&id) {
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
    page(c, id, h).await
}
async fn replay_page(
    State(c): State<Coordinator>,
    Path((id, _)): Path<(String, String)>,
    h: HeaderMap,
) -> Response {
    page(c, id, h).await
}
async fn page(c: Coordinator, id: String, h: HeaderMap) -> Response {
    let game = match c.game(&id) {
        Ok(g) => g,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !allowed(&game, &h) {
        // IDs are generated internally; user-provided names never enter HTML.
        return Html(format!(r#"<!doctype html><meta charset="utf-8"><title>Lobby password</title><body style="background:#0e141c;color:#edf4fa;font:18px system-ui;padding:4rem"><h1>Lobby password</h1><form><label>Password <input type="password" autofocus></label> <button>Enter lobby</button><p role="alert"></p></form><a href="/">All games</a><script>document.querySelector('form').onsubmit=async e=>{{e.preventDefault();const r=await fetch('/api/games/{id}/join',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{password:document.querySelector('input').value}})}});const d=await r.json();if(d.error)document.querySelector('p').textContent=d.error;else location.reload();}};</script>"#)).into_response();
    }
    ws::serve(&c.client, "index.html").await
}
async fn game_ws(
    State(c): State<Coordinator>,
    Path(id): Path<String>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    relay(c, id, None, h, upgrade).await
}
async fn replay_ws(
    State(c): State<Coordinator>,
    Path((id, replay)): Path<(String, String)>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    relay(c, id, Some(replay), h, upgrade).await
}
async fn relay(
    c: Coordinator,
    id: String,
    replay: Option<String>,
    h: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let game = match c.game(&id) {
        Ok(g) => g,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !allowed(&game, &h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let readonly = replay.is_some();
    let worker_key = format!("{}:{}", id, replay.as_deref().unwrap_or("live"));
    if let Some(r) = &replay {
        let status = c.status(&id);
        if !valid_id(r)
            || (status["match_id"] == *r
                && !matches!(status["phase"].as_str(), Some("finished" | "archived")))
        {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
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
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await
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
                            if readonly {
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
                            if !readonly && t.len() < 1024 && let Some(user)=&user && let Ok(v)=serde_json::from_str::<Value>(t.as_str()) && v["message"]["kind"]=="slot_claimed" && let Some(slot)=v["message"]["slot"].as_u64() {
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
        "/" => ws::serve(&c.client, "directory.html").await,
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
    let game = match c.game(&id) {
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
            created: 0,
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
    fn replay_identifiers_cannot_escape_the_game_directory() {
        assert!(valid_id("match-123-abcdef"));
        for id in ["..", "../manifest", "/etc/passwd", "a/b", "a%2fb", ""] {
            assert!(!valid_id(id));
        }
    }
}
