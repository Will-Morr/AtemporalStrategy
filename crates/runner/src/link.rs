//! Controller connections. The sync link (`/ws/peripheral`) receives bootstrap, per-revision
//! inputs and reference hashes and drives local reproduction. Each local browser socket gets its
//! own relay link (`/ws`): lobby, readiness and commits pass through with the player's own slot
//! token; world-state queries are answered locally; publications wait for local verification.
use crate::store::{Peripheral, Status, is_local_query};
use atemporal_sim::*;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as Upstream;

pub struct Guide {
    pub prose: String,
    pub directory: Option<PathBuf>,
    /// Directory and content hash of the guide currently served.
    pub generated: std::sync::Mutex<Option<(PathBuf, String)>>,
}

fn client_json(message: ClientMessage) -> String {
    serde_json::to_string(&ClientEnvelope {
        schema_version: Version::default(),
        message,
    })
    .unwrap_or_default()
}

fn server_json(instance: &str, message: ServerMessage) -> String {
    serde_json::to_string(&ServerEnvelope {
        schema_version: Version::default(),
        server_instance_id: instance.into(),
        message,
    })
    .unwrap_or_default()
}

/// Keep one inputs-only connection to the controller alive; reconnect and catch up after drops.
pub async fn sync_loop(p: Arc<Peripheral>, controller: String, guide: Arc<Guide>) {
    let url = format!("{controller}/ws/peripheral");
    let mut announced = false;
    loop {
        let Ok((socket, _)) = tokio_tungstenite::connect_async(&url).await else {
            if !announced {
                eprintln!("controller {controller} unreachable; retrying every second");
                announced = true;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        announced = false;
        p.connected.store(true, Ordering::Relaxed);
        println!("controller link open: {url}");
        let (mut sink, mut stream) = socket.split();
        let hello = client_json(ClientMessage::Hello {
            protocol_version: Version::default(),
            last_revision: None,
            slot_token: None,
        });
        if sink.send(Upstream::Text(hello.into())).await.is_err() {
            continue;
        }
        let mut match_id = String::new();
        while let Some(Ok(message)) = stream.next().await {
            let Upstream::Text(text) = message else {
                continue;
            };
            let Ok(envelope) = serde_json::from_str::<ServerEnvelope>(&text) else {
                continue;
            };
            match envelope.message {
                ServerMessage::Welcome { match_id: id, .. } => {
                    let stale = p
                        .store
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|s| s.match_id != id);
                    if stale {
                        println!(
                            "controller now runs match {id}; dropping the previous local store"
                        );
                        *p.store.lock().unwrap() = None;
                        p.bump();
                    }
                    match_id = id;
                }
                ServerMessage::ReplayBootstrap {
                    fingerprint,
                    config,
                    content,
                    initial_state,
                    ..
                } => {
                    let known = p
                        .store
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|s| s.match_id == match_id);
                    if known {
                        continue;
                    }
                    let store = crate::store::Store::new(
                        match_id.clone(),
                        fingerprint,
                        config,
                        content,
                        *initial_state,
                        p.memory_budget,
                    );
                    let store = match store {
                        Ok(store) => store,
                        Err(message) => {
                            eprintln!("peripheral handshake rejected: {message}");
                            std::process::exit(3);
                        }
                    };
                    // The lobby-time guide came from the local content file; the pinned content wins.
                    match write_guide(&guide, &store.content) {
                        Ok(true) => println!(
                            "local content differs from the pinned match content; guide regenerated"
                        ),
                        Ok(false) => {}
                        Err(e) => {
                            eprintln!("guide generation failed: {e}");
                            std::process::exit(3);
                        }
                    }
                    println!(
                        "bootstrapped match {match_id}: fingerprint verified ({} {})",
                        store.fingerprint.sim_build, store.fingerprint.target
                    );
                    *p.store.lock().unwrap() = Some(store);
                    p.bump();
                }
                ServerMessage::RoundInputs {
                    revision,
                    turns,
                    precedence,
                    editable_from,
                } => {
                    if let Some(store) = p.store.lock().unwrap().as_mut() {
                        store.accept_inputs(revision, turns, precedence, editable_from);
                    }
                    p.bump();
                    p.schedule();
                }
                ServerMessage::ReferenceHash {
                    revision,
                    tick,
                    hash,
                } => {
                    if let Some(store) = p.store.lock().unwrap().as_mut() {
                        store.accept_reference(revision, tick, hash);
                        p.bump();
                    }
                    p.schedule();
                }
                ServerMessage::PlanningOpened { revision, .. } => {
                    let discarded = p
                        .store
                        .lock()
                        .unwrap()
                        .as_mut()
                        .is_some_and(|s| s.discard_unpublished_after(revision));
                    if discarded {
                        let _ = p
                            .local
                            .send(ServerMessage::ReplayProgress { preview: None });
                        p.bump();
                    }
                }
                ServerMessage::RevisionPublished {
                    revision,
                    score,
                    timed,
                    ..
                } => {
                    if let Some(store) = p.store.lock().unwrap().as_mut() {
                        store.accept_published(revision, score, timed);
                    }
                }
                _ => {}
            }
        }
        p.connected.store(false, Ordering::Relaxed);
        eprintln!("controller link closed; reconnecting");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// One generation path for the startup content file and the pinned bootstrap content; returns
/// whether the served guide changed.
pub fn write_guide(guide: &Guide, content: &Content) -> Result<bool> {
    let hash = atemporal_content::content_hash(content)?;
    let mut generated = guide.generated.lock().unwrap();
    if generated.as_ref().is_some_and(|(_, h)| *h == hash) {
        return Ok(false);
    }
    let directory = guide
        .directory
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!(".guide-cache/peripheral-{}", &hash[..12])));
    atemporal_content::write_guide(content, &guide.prose, &directory)?;
    let changed = generated.is_some();
    *generated = Some((directory, hash));
    Ok(changed)
}

/// One local browser socket: relay inputs upstream, answer world-state queries locally, and hold
/// each publication until the local reproduction settled so readiness never precedes local state.
pub async fn relay(p: Arc<Peripheral>, controller: String, mut socket: WebSocket) {
    let url = format!("{controller}/ws");
    let upstream = match tokio_tungstenite::connect_async(&url).await {
        Ok((upstream, _)) => upstream,
        Err(e) => {
            // No controller: the browser's own reconnect loop retries against this peripheral.
            eprintln!("relay: cannot reach controller at {url}: {e}");
            let _ = socket.close().await;
            return;
        }
    };
    let (mut up_sink, mut up_stream) = upstream.split();
    let (mut local_sink, mut local_stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let instance = Arc::new(std::sync::Mutex::new(String::new()));

    let writer = tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if local_sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let progress = {
        let mut local = p.local.subscribe();
        let p = p.clone();
        let out = out_tx.clone();
        let instance = instance.clone();
        tokio::spawn(async move {
            loop {
                let message = match local.recv().await {
                    Ok(ServerMessage::ReplayProgress { .. })
                    | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        p.replay_progress()
                    }
                    Ok(message) => message,
                    Err(_) => break,
                };
                let id = instance.lock().unwrap().clone();
                if !id.is_empty() && out.send(server_json(&id, message)).is_err() {
                    break;
                }
            }
        })
    };

    let downstream = {
        let p = p.clone();
        let out = out_tx.clone();
        let instance = instance.clone();
        tokio::spawn(async move {
            loop {
                let message = match up_stream.next().await {
                    Some(Ok(message)) => message,
                    Some(Err(e)) => {
                        eprintln!("relay: controller connection lost: {e}");
                        break;
                    }
                    None => break,
                };
                let Upstream::Text(text) = message else {
                    continue;
                };
                let Ok(envelope) = serde_json::from_str::<ServerEnvelope>(&text) else {
                    continue;
                };
                *instance.lock().unwrap() = envelope.server_instance_id.clone();
                match &envelope.message {
                    ServerMessage::ReplayProgress { .. }
                    | ServerMessage::SimulationProgress { .. } => continue,
                    ServerMessage::RevisionPublished {
                        revision,
                        score,
                        timed,
                        ..
                    } => {
                        // Loading: the browser sees this revision only once it exists locally.
                        if let Status::Mismatch(m) = p.wait_settled(*revision).await {
                            eprintln!("{m}");
                        }
                        if let Some(store) = p.store.lock().unwrap().as_mut() {
                            store.accept_published(*revision, score.clone(), timed.clone());
                        }
                    }
                    ServerMessage::PlanningOpened { revision, .. }
                        if p.status(*revision) != Status::Verified =>
                    {
                        // Mismatch or not yet reproduced: order entry stays closed.
                        continue;
                    }
                    _ => {}
                }
                if out.send(text.to_string()).is_err() {
                    break;
                }
            }
        })
    };

    let upstream_relay = {
        let p = p.clone();
        let out = out_tx.clone();
        let instance = instance.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = local_stream.next().await {
                let text = match message {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let Ok(envelope) = serde_json::from_str::<ClientEnvelope>(&text) else {
                    // The controller answers malformed input the same way it would directly.
                    let _ = up_sink.send(Upstream::Text(text.into())).await;
                    continue;
                };
                if is_local_query(&envelope.message) {
                    let p = p.clone();
                    let out = out.clone();
                    let instance = instance.clone();
                    tokio::spawn(async move {
                        let preview_request_id = match &envelope.message {
                            ClientMessage::GetReplayPreview { request_id, .. } => {
                                request_id.clone()
                            }
                            _ => String::new(),
                        };
                        let message = match p.answer(envelope.message).await {
                            Ok(message) => message,
                            Err(message) => ServerMessage::CommitRejected {
                                request_id: preview_request_id,
                                code: "query_failed".into(),
                                message,
                            },
                        };
                        let id = instance.lock().unwrap().clone();
                        let _ = out.send(server_json(&id, message));
                    });
                    continue;
                }
                if up_sink.send(Upstream::Text(text.into())).await.is_err() {
                    break;
                }
            }
        })
    };

    let tasks = [writer, progress, downstream, upstream_relay];
    let (_, _, rest) = futures_util::future::select_all(tasks).await;
    for task in rest {
        task.abort();
    }
}
