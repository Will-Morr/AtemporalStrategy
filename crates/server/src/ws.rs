//! Same-origin HTTP routes (client assets, generated guide) and the WebSocket protocol.
use crate::controller::Controller;
use atemporal_sim::*;
use axum::{
    Router,
    body::Body,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct App {
    pub controller: Arc<Mutex<Controller>>,
    pub client_dir: PathBuf,
    pub guide_dir: PathBuf,
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/guide", get(guide_root))
        .route("/guide/", get(guide_root))
        .route("/guide/{*path}", get(guide_file))
        .fallback(get(client_file))
        .with_state(app)
}

async fn guide_root(State(app): State<App>) -> Response {
    serve(&app.guide_dir, "index.html").await
}
async fn guide_file(
    State(app): State<App>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    serve(&app.guide_dir, &path).await
}
async fn client_file(State(app): State<App>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    serve(&app.client_dir, path).await
}

async fn serve(root: &Path, relative: &str) -> Response {
    if relative
        .split('/')
        .any(|part| part == ".." || part.is_empty())
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = root.join(relative);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("json") | Some("map") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from(bytes),
    )
        .into_response()
}

async fn ws_upgrade(State(app): State<App>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(64 << 20)
        .on_upgrade(move |socket| connection(app, socket))
}

fn envelope(app: &App, message: ServerMessage) -> String {
    let instance = app.controller.lock().unwrap().instance_id.clone();
    serde_json::to_string(&ServerEnvelope {
        schema_version: Version::default(),
        server_instance_id: instance,
        message,
    })
    .unwrap_or_default()
}

async fn connection(app: App, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (direct_tx, mut direct_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let mut public = app.controller.lock().unwrap().broadcast.subscribe();
    let writer_app = app.clone();
    let writer = tokio::spawn(async move {
        loop {
            let message = tokio::select! {
                Some(m) = direct_rx.recv() => m,
                result = public.recv() => match result {
                    Ok(m) => m,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                },
            };
            if sink
                .send(Message::Text(envelope(&writer_app, message).into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let mut player: Option<PlayerId> = None;
    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let parsed: std::result::Result<ClientEnvelope, _> = serde_json::from_str(&text);
        let client = match parsed {
            Ok(envelope) => envelope.message,
            Err(e) => {
                let _ = direct_tx.send(ServerMessage::CommitRejected {
                    request_id: String::new(),
                    code: "malformed".into(),
                    message: format!("malformed message: {e}"),
                });
                continue;
            }
        };
        handle(&app, &direct_tx, &mut player, client).await;
    }
    if let Some(p) = player {
        app.controller.lock().unwrap().disconnect(p);
    }
    writer.abort();
}

fn reject(
    tx: &mpsc::UnboundedSender<ServerMessage>,
    request_id: &str,
    code: &str,
    message: String,
) {
    let _ = tx.send(ServerMessage::CommitRejected {
        request_id: request_id.into(),
        code: code.into(),
        message,
    });
}

async fn handle(
    app: &App,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    player: &mut Option<PlayerId>,
    client: ClientMessage,
) {
    match client {
        ClientMessage::Hello { slot_token, .. } => {
            let mut c = app.controller.lock().unwrap();
            let _ = tx.send(c.welcome());
            // A repeated hello on the same socket must not count as another connection.
            let already = *player;
            if let Some(p) = already.or_else(|| c.connect(slot_token.as_deref())) {
                *player = Some(p);
                let _ = tx.send(ServerMessage::SlotClaimed {
                    slot: p,
                    private_token: slot_token.unwrap_or_default(),
                });
            }
            if c.phase != Phase::Lobby {
                if let Some(m) = c.revision_published(c.current) {
                    let _ = tx.send(m);
                }
                if c.phase == Phase::Planning {
                    let _ = tx.send(c.planning_opened());
                }
            }
        }
        ClientMessage::ClaimSlot {
            slot,
            username,
            color,
            team_id,
        } => {
            let mut c = app.controller.lock().unwrap();
            match c.claim_slot(slot, username, color, team_id) {
                Ok(token) => {
                    *player = Some(slot);
                    let _ = tx.send(ServerMessage::SlotClaimed {
                        slot,
                        private_token: token,
                    });
                }
                Err(message) => {
                    let lobby = c.lobby.clone();
                    let _ = tx.send(ServerMessage::LobbyUpdateRejected {
                        request_id: "claim".into(),
                        code: "claim_rejected".into(),
                        message,
                        lobby,
                    });
                }
            }
        }
        ClientMessage::ReleaseSlot { slot_token } => {
            let mut c = app.controller.lock().unwrap();
            if c.release_slot(&slot_token).is_ok() {
                *player = None;
            }
        }
        ClientMessage::UpdateLobbyProfile {
            request_id,
            slot_token,
            username,
            color,
            team_id,
        } => {
            let mut c = app.controller.lock().unwrap();
            if let Err(message) = c.update_profile(&slot_token, username, color, team_id) {
                let lobby = c.lobby.clone();
                let _ = tx.send(ServerMessage::LobbyUpdateRejected {
                    request_id,
                    code: "profile_rejected".into(),
                    message,
                    lobby,
                });
            }
        }
        ClientMessage::StartMatch {
            slot_token,
            based_on_lobby_revision,
        } => {
            let result = {
                let mut c = app.controller.lock().unwrap();
                c.start_match(&slot_token, based_on_lobby_revision)
                    .and_then(|_| c.start_job(0, vec![], vec![]))
            };
            match result {
                Ok(rx) => drive_job(app.clone(), rx),
                Err(message) => {
                    let lobby = app.controller.lock().unwrap().lobby.clone();
                    let _ = tx.send(ServerMessage::LobbyUpdateRejected {
                        request_id: "start".into(),
                        code: "start_rejected".into(),
                        message,
                        lobby,
                    });
                }
            }
        }
        ClientMessage::PlanningReady { round, .. } => {
            if let Some(p) = *player {
                app.controller.lock().unwrap().planning_ready(p, round);
            }
        }
        ClientMessage::Commit { request } => {
            let request_id = request.request_id.clone();
            let Some(p) = app
                .controller
                .lock()
                .unwrap()
                .player_for(&request.slot_token)
            else {
                return reject(tx, &request_id, "unauthorized", "unknown slot token".into());
            };
            let (round, exact) = {
                let c = app.controller.lock().unwrap();
                match c.precheck_commit(p, &request) {
                    Ok(Some(original)) => {
                        let _ = tx.send(original);
                        return;
                    }
                    Ok(None) => {}
                    Err(message) => return reject(tx, &request_id, "invalid", message),
                }
                (c.round, c.exact_request(c.current, request.draft.tick))
            };
            let commands = match draft::resolve_local_references(&request.draft, round, p) {
                Ok(commands) => commands,
                Err(message) => return reject(tx, &request_id, "invalid", message),
            };
            let state = match exact {
                Ok(exact) => {
                    exact_state(
                        app,
                        exact,
                        request.draft.based_on_revision,
                        request.draft.tick,
                    )
                    .await
                }
                Err(message) => Err(message),
            };
            let state = match state {
                Ok(state) => state,
                Err(message) => return reject(tx, &request_id, "state_unavailable", message),
            };
            let result = {
                let mut c = app.controller.lock().unwrap();
                if c.round != round {
                    return reject(
                        tx,
                        &request_id,
                        "stale",
                        "planning round changed while validating".into(),
                    );
                }
                c.validate_against_state(p, &commands, &state)
                    .and_then(|_| c.accept_commit(p, &request, commands))
            };
            match result {
                Ok((accepted, rx)) => {
                    let _ = tx.send(accepted);
                    if let Some(rx) = rx {
                        drive_job(app.clone(), rx);
                    }
                }
                Err(message) => reject(tx, &request_id, "rejected", message),
            }
        }
        ClientMessage::GetSnapshotRange {
            revision,
            from_tick,
            to_tick,
            stride,
        } => {
            let result = app
                .controller
                .lock()
                .unwrap()
                .snapshot_range(revision, from_tick, to_tick, stride);
            respond(tx, result);
        }
        ClientMessage::GetExactState { revision, tick } => {
            let started = Instant::now();
            let exact = app.controller.lock().unwrap().exact_request(revision, tick);
            let result = match exact {
                Ok(exact) => exact_state(app, exact, revision, tick)
                    .await
                    .map(|snapshot| {
                        println!("exact state r{revision} t{tick}: {:?}", started.elapsed());
                        ServerMessage::ExactState {
                            revision,
                            tick,
                            snapshot,
                        }
                    }),
                Err(message) => Err(message),
            };
            respond(tx, result);
        }
        ClientMessage::GetStats {
            revision,
            from_tick,
            to_tick,
            bucket_width,
        } => {
            let result = app.controller.lock().unwrap().stats_range(
                revision,
                from_tick,
                to_tick,
                bucket_width,
            );
            respond(tx, result);
        }
        ClientMessage::GetRound { revision } => {
            let result = app.controller.lock().unwrap().round_result(revision);
            respond(tx, result);
        }
        ClientMessage::GetCommands {
            revision,
            from_tick,
            to_tick,
        } => {
            let result = app
                .controller
                .lock()
                .unwrap()
                .commands_range(revision, from_tick, to_tick);
            respond(tx, result);
        }
        ClientMessage::GetEvents {
            revision,
            from_tick,
            to_tick,
        } => {
            let result = app
                .controller
                .lock()
                .unwrap()
                .events_range(revision, from_tick, to_tick);
            respond(tx, result);
        }
        ClientMessage::GetControlGroups {
            revision,
            tick,
            player: owner,
        } => {
            let exact = app.controller.lock().unwrap().exact_request(revision, tick);
            let result = match exact {
                Ok(exact) => exact_state(app, exact, revision, tick).await.map(|state| {
                    ServerMessage::ControlGroups {
                        revision,
                        tick,
                        groups: state
                            .control_groups
                            .into_iter()
                            .filter(|g| g.id.owner == owner)
                            .collect(),
                    }
                }),
                Err(message) => Err(message),
            };
            respond(tx, result);
        }
        ClientMessage::StopAndArchive {
            request_id,
            based_on_revision,
            slot_token,
        } => {
            let result = app.controller.lock().unwrap().stop_and_archive(
                &slot_token,
                request_id.clone(),
                based_on_revision,
            );
            if let Err(message) = result {
                reject(tx, &request_id, "rejected", message);
            }
        }
    }
}

fn respond(tx: &mpsc::UnboundedSender<ServerMessage>, result: Result<ServerMessage>) {
    match result {
        Ok(message) => {
            let _ = tx.send(message);
        }
        Err(message) => reject(tx, "", "query_failed", message),
    }
}

/// Exact S[tick]: cached checkpoint/LRU hit, otherwise in-process reconstruction on the sim thread.
async fn exact_state(
    app: &App,
    request: SimRequest,
    revision: Revision,
    tick: Tick,
) -> Result<WorldState> {
    if let Some(state) = app.controller.lock().unwrap().cached_exact(revision, tick) {
        return Ok(state);
    }
    let sim = app.controller.lock().unwrap().sim.clone();
    let state = sim.exact(request, tick).await?;
    app.controller
        .lock()
        .unwrap()
        .remember_exact(revision, tick, state.clone());
    Ok(state)
}

/// Consume a job's typed messages; complete revisions are persisted then published.
pub fn drive_job(app: App, mut rx: mpsc::Receiver<WorkerMessage>) {
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                WorkerMessage::Complete {
                    job_id,
                    outcome,
                    final_hash,
                    sim_duration_ms,
                    command_outcomes,
                    ..
                } => {
                    let data = app.controller.lock().unwrap().on_complete(
                        &job_id,
                        outcome,
                        final_hash,
                        sim_duration_ms.get(),
                        command_outcomes,
                    );
                    let Some(data) = data else { continue };
                    let timed_state = {
                        let c = app.controller.lock().unwrap();
                        match c.config.objective {
                            Objective::Timed {
                                lock_ticks_per_round,
                            } if data.round > 0 => {
                                let boundary =
                                    (c.editable_from + lock_ticks_per_round).min(c.config.max_tick);
                                let checkpoint = data
                                    .checkpoints
                                    .range(..=boundary)
                                    .next_back()
                                    .map(|(_, s)| s.clone());
                                checkpoint.map(|cp| {
                                    (
                                        boundary,
                                        c.build_request(
                                            data.revision,
                                            cp,
                                            data.turns.clone(),
                                            data.precedence.clone(),
                                            0,
                                        ),
                                    )
                                })
                            }
                            _ => None,
                        }
                    };
                    let timed_state = match timed_state {
                        Some((boundary, request)) => {
                            let sim = app.controller.lock().unwrap().sim.clone();
                            match sim.exact(request, boundary).await {
                                Ok(state) => Some(state),
                                Err(e) => {
                                    eprintln!("timed adjudication state failed: {e}");
                                    None
                                }
                            }
                        }
                        None => None,
                    };
                    let result = app.controller.lock().unwrap().publish(data, timed_state);
                    if let Err(e) = result {
                        eprintln!("publish failed: {e}");
                    }
                }
                WorkerMessage::Failed {
                    job_id, message, ..
                } => {
                    app.controller.lock().unwrap().on_failed(&job_id, &message);
                }
                other => {
                    app.controller.lock().unwrap().on_batch(other);
                }
            }
        }
    });
}
