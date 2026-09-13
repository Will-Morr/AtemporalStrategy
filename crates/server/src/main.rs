//! `atemporal-server --port 8080 --config config/game.yaml`: one match per process, same-origin
//! assets/guide/WebSocket on exactly the requested port. `--resume <match_id>` reopens an archive
//! under its pinned config/content; `--verify <match_id>` replays it and compares hashes.
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
                "atemporal-server [--port N] [--config YAML] [--content YAML] [--client DIR] [--prose HTML] [--replays DIR] [--guide-dir DIR] [--memory-budget-mb N] [--results-budget-mb N] [--resume MATCH_ID | --verify MATCH_ID]"
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
    let replay_option = options.get("--replays").cloned();
    if let Some(match_id) = options.get("--verify") {
        let replays = replay_option.as_deref().unwrap_or("replays");
        let (_, loaded) = archive::Archive::open(Path::new(replays), match_id)?;
        let content = atemporal_content::load_archived_content(
            &loaded.content_yaml,
            &loaded.manifest.fingerprint.content_hash,
        )?;
        println!(
            "verifying {match_id}: {} rounds, {} turns",
            loaded.rounds.len(),
            loaded.turns.len()
        );
        return if controller::verify_archive(&loaded, &content)? {
            println!("all rounds reproduce their recorded hashes");
            Ok(())
        } else {
            Err("replay verification failed".into())
        };
    }
    // A resumed match uses its pinned config/content copies, never the CLI files.
    let resume = match options.get("--resume") {
        Some(match_id) => {
            let replays = replay_option.as_deref().unwrap_or("replays");
            Some((
                match_id.clone(),
                archive::Archive::open(Path::new(replays), match_id)?,
            ))
        }
        None => None,
    };
    let (config_yaml, content_yaml) = match &resume {
        Some((_, (_, loaded))) => (loaded.config_yaml.clone(), loaded.content_yaml.clone()),
        None => (
            read(Path::new(&option("--config", "config/game.yaml")))?,
            read(Path::new(&option("--content", "config/content.yaml")))?,
        ),
    };
    let setup = atemporal_content::load_setup(&config_yaml)?;
    let content = match &resume {
        Some((_, (_, loaded))) => atemporal_content::load_archived_content(
            &content_yaml,
            &loaded.manifest.fingerprint.content_hash,
        )?,
        None => atemporal_content::load_content(&content_yaml)?,
    };
    // CLI beats the PORT environment variable, which beats the YAML default.
    let port: u16 = match options
        .get("--port")
        .cloned()
        .or_else(|| std::env::var("PORT").ok())
    {
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
    let (mut controller, mode) = match resume {
        Some((match_id, (archive, loaded))) => (
            controller::Controller::resume(
                setup,
                content,
                "/guide/".into(),
                replay_root.clone(),
                sim,
                match_id,
                archive,
                loaded,
            )?,
            "resumed match",
        ),
        None => (
            controller::Controller::new(
                setup,
                content,
                config_yaml,
                content_yaml,
                "/guide/".into(),
                replay_root.clone(),
                sim,
            )?,
            "new match",
        ),
    };
    let budget = |key: &str, default: u64| -> Result<u64> {
        match options.get(key) {
            Some(v) => v
                .parse::<u64>()
                .map(|mb| mb << 20)
                .map_err(|_| format!("{key} must be a whole number of MiB")),
            None => Ok(default),
        }
    };
    controller.memory_budget = budget("--memory-budget-mb", controller::DEFAULT_MEMORY_BUDGET)?;
    controller.disk_budget = budget("--results-budget-mb", controller::DEFAULT_DISK_BUDGET)?;
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
        println!("atemporal-server listening on http://127.0.0.1:{port}/ ({mode} {match_id}; guide {}; replays {})", guide_dir.display(), replay_root.display());
        // A resumed match may need its opening rerun or a fully committed round simulated.
        let pending = {
            let mut c = app.controller.lock().unwrap();
            if c.phase == Phase::Simulating && c.revisions.len() == 1 {
                c.start_job(0, vec![], vec![]).map(Some)
            } else {
                c.restart_pending_round()
            }
        };
        if let Some(rx) = pending? {
            ws::drive_job(app.clone(), rx);
        }
        axum::serve(listener, ws::router(app)).await.map_err(|e| e.to_string())
    })
}
