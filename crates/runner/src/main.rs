//! `atemporal-runner --controller ws://host:port [--port 8090]`: the native inputs-only
//! peripheral. It downloads the pinned bootstrap and command ledger from the controller, replays
//! every revision with the same simulation library, checks reference hashes, and serves the
//! browser client, a guide for its pinned content and local world-state queries on its own port.
mod link;
mod store;

use atemporal_sim::*;
use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    http::{StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Clone)]
struct App {
    peripheral: Arc<store::Peripheral>,
    controller: String,
    client_dir: PathBuf,
    guide: Arc<link::Guide>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("atemporal-runner: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut options = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "atemporal-runner --controller ws://HOST:PORT [--port N] [--content YAML] [--client DIR] [--prose HTML] [--guide-dir DIR] [--memory-budget-mb N]"
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {key}"))?;
        options.insert(key, value);
    }
    let option =
        |key: &str, default: &str| options.get(key).cloned().unwrap_or_else(|| default.into());
    let controller = options
        .get("--controller")
        .cloned()
        .ok_or("--controller ws://HOST:PORT is required")?
        .trim_end_matches('/')
        .to_string();
    if !controller.starts_with("ws://") && !controller.starts_with("wss://") {
        return Err("--controller must be a ws:// or wss:// URL".into());
    }
    let port: u16 = option("--port", "8090")
        .parse()
        .map_err(|_| "port must be 1..65535")?;
    if port == 0 {
        return Err("port must be 1..65535".into());
    }
    let client_dir = PathBuf::from(option("--client", "client/dist"));
    if !client_dir.join("index.html").exists() {
        return Err(format!(
            "{} has no index.html; run `npm run build --prefix client` first",
            client_dir.display()
        ));
    }
    let prose_path = option("--prose", "client/guide/introduction.html");
    let prose = std::fs::read_to_string(&prose_path).map_err(|e| format!("{prose_path}: {e}"))?;
    // The lobby shows the guide before any match is pinned; the bootstrap content replaces it.
    let content_path = option("--content", "config/content.yaml");
    let content_yaml =
        std::fs::read_to_string(&content_path).map_err(|e| format!("{content_path}: {e}"))?;
    let content = atemporal_content::load_content(&content_yaml)?;
    let memory_budget = match options.get("--memory-budget-mb") {
        Some(v) => v
            .parse::<u64>()
            .map(|mb| mb << 20)
            .map_err(|_| "--memory-budget-mb must be a whole number of MiB")?,
        None => atemporal_server::controller::DEFAULT_MEMORY_BUDGET,
    };
    let app = App {
        peripheral: store::Peripheral::new(memory_budget),
        controller,
        client_dir,
        guide: Arc::new(link::Guide {
            prose,
            directory: options.get("--guide-dir").map(PathBuf::from),
            generated: std::sync::Mutex::new(None),
        }),
    };
    link::write_guide(&app.guide, &content)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
            .await
            .map_err(|e| format!("cannot bind port {port}: {e}"))?;
        println!(
            "atemporal-runner listening on http://127.0.0.1:{port}/ (peripheral of {}; guide {}; status at /peripheral/status)",
            app.controller,
            guide_dir(&app).unwrap_or_default().display()
        );
        tokio::spawn(link::sync_loop(
            app.peripheral.clone(),
            app.controller.clone(),
            app.guide.clone(),
        ));
        let router = Router::new()
            .route("/ws", get(ws_upgrade))
            .route("/peripheral/status", get(status))
            .route("/guide", get(guide_root))
            .route("/guide/", get(guide_root))
            .route("/guide/{*path}", get(guide_file))
            .fallback(get(client_file))
            .with_state(app);
        axum::serve(listener, router).await.map_err(|e| e.to_string())
    })
}

async fn ws_upgrade(State(app): State<App>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(64 << 20)
        .on_upgrade(move |socket: WebSocket| link::relay(app.peripheral, app.controller, socket))
}

async fn status(State(app): State<App>) -> Response {
    let store = app.peripheral.store.lock().unwrap();
    Json(serde_json::json!({
        "controller": app.controller,
        "controller_connected": app.peripheral.connected.load(Ordering::Relaxed),
        "bootstrapped": store.is_some(),
        "guide_dir": app.guide.generated.lock().unwrap().as_ref().map(|(d, _)| d.display().to_string()),
        "store": store.as_ref().map(|s| s.status_json()),
    }))
    .into_response()
}

fn guide_dir(app: &App) -> Option<PathBuf> {
    app.guide
        .generated
        .lock()
        .unwrap()
        .as_ref()
        .map(|(d, _)| d.clone())
}
async fn guide_root(State(app): State<App>) -> Response {
    match guide_dir(&app) {
        Some(dir) => atemporal_server::ws::serve(&dir, "index.html").await,
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "guide is generated once the match bootstrap arrives",
        )
            .into_response(),
    }
}
async fn guide_file(
    State(app): State<App>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    match guide_dir(&app) {
        Some(dir) => atemporal_server::ws::serve(&dir, &path).await,
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
async fn client_file(State(app): State<App>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    atemporal_server::ws::serve(Path::new(&app.client_dir), path).await
}
