//! Browser setup diagnostics that can arrive before a match socket or slot exists.
//! The wire format deliberately has no free-form text, URL, or identity fields.
use crate::{archive, controller::now_ms};
use atemporal_sim::identity;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

pub(crate) const MAX_BODY: usize = 2048;
const MAX_REPORTS: usize = 64;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Environment,
    Directory,
    Socket,
    Welcome,
    Lobby,
    Game,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum View {
    Standalone,
    Game,
    Replay,
    Practice,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Category {
    Type,
    Reference,
    Range,
    Syntax,
    Error,
    Other,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    MissingUi,
    NullAccess,
    UndefinedAccess,
    InvalidJson,
    Unclassified,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Username,
    TopControls,
    SetAttribute,
    Hidden,
    Value,
    TextContent,
    Closest,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    line: u32,
    column: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    stage: Stage,
    view: View,
    category: Category,
    reason: Reason,
    operation: Option<Operation>,
    frames: Vec<Frame>,
    round: Option<u32>,
    revision: Option<u32>,
}

pub(crate) fn save(
    root: &Path,
    game: Option<&str>,
    match_id: Option<&str>,
    report: Report,
) -> Response {
    if report.frames.len() > 5
        || report
            .frames
            .iter()
            .any(|frame| frame.line == 0 || frame.column == 0 || frame.column > 10_000_000)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let commit = archive::server_commit();
    let hash = identity::canonical_hash(&(&commit, match_id, &report))
        .expect("setup report has a fixed serializable structure");
    let id = format!("client-{}", &hash[..24]);
    let record = json!({
        "schema_version": 1,
        "report_id": id,
        "server_commit": commit,
        "game_id": game,
        "match_id": match_id,
        "received_at_ms": now_ms(),
        "setup": report,
    });
    match archive::write_incident(root, "client-incidents", &id, &record, Some(MAX_REPORTS)) {
        Ok(new_report) => {
            if new_report {
                eprintln!("client_setup_incident status=saved report={id}");
            }
            Json(json!({"saved": true, "report_id": id})).into_response()
        }
        Err(error) => {
            eprintln!("client_setup_incident status=save_failed report={id} error={error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
