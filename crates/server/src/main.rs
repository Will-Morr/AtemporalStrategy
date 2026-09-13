//! `atemporal-server --port 8080 --config config/game.yaml`: one match per process, same-origin
//! assets/guide/WebSocket on exactly the requested port.
mod adapter;
mod archive;
mod controller;
mod ws;

use atemporal_sim::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn main() {
    if let Err(e) = run() {
        eprintln!("atemporal-server: {e}");
        std::process::exit(1);
    }
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<()> {
    let mut options = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "atemporal-server [--port N] [--config YAML] [--content YAML] [--client DIR] [--prose HTML] [--replays DIR] [--guide-dir DIR]"
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
    let config_yaml = read(Path::new(&option("--config", "config/game.yaml")))?;
    let content_yaml = read(Path::new(&option("--content", "config/content.yaml")))?;
    let setup = atemporal_content::load_setup(&config_yaml)?;
    let content = atemporal_content::load_content(&content_yaml)?;
    let port: u16 = match options.get("--port") {
        Some(p) => p.parse().map_err(|_| "port must be 1..65535")?,
        None => setup.default_port,
    };
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
    let prose = read(Path::new(&option(
        "--prose",
        "client/guide/introduction.html",
    )))?;
    let content_hash = atemporal_content::content_hash(&content)?;
    let guide_dir = PathBuf::from(option(
        "--guide-dir",
        &format!(".guide-cache/{}", &content_hash[..12]),
    ));
    atemporal_content::write_guide(&content, &prose, &guide_dir)?;
    let replay_root = PathBuf::from(option("--replays", &setup.match_defaults.replay_directory));
    let sim = adapter::SimThread::spawn();
    let controller = controller::Controller::new(
        setup,
        content,
        config_yaml,
        content_yaml,
        "/guide/".into(),
        replay_root.clone(),
        sim,
    )?;
    let match_id = controller.match_id.clone();
    let app = ws::App {
        controller: Arc::new(Mutex::new(controller)),
        client_dir,
        guide_dir: guide_dir.clone(),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
            .await
            .map_err(|e| format!("cannot bind port {port}: {e}"))?;
        println!("atemporal-server listening on http://127.0.0.1:{port}/ (new match {match_id}; guide {}; replays {})", guide_dir.display(), replay_root.display());
        axum::serve(listener, ws::router(app)).await.map_err(|e| e.to_string())
    })
}
