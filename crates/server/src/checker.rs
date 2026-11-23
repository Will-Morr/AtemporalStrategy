//! Durable evidence for a discrepancy between the browser and authoritative order checkers.
use crate::{archive, controller::Controller};
use atemporal_sim::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    CompilerRejected,
    StateRejected,
    ClientRejected,
    DraftRecovery,
}

pub(crate) struct Finding {
    pub failure: Failure,
    pub source: Option<DraftCheckSource>,
    pub reason: String,
    /// The browser's own message is echoed to that browser and never written to the archive.
    pub browser_issue: Option<String>,
}

pub(crate) struct Context {
    archive_root: Option<PathBuf>,
    match_id: String,
    round: u32,
    config: MatchConfig,
    content: Content,
    fingerprint: Fingerprint,
}

impl Context {
    pub(crate) fn capture(controller: &Controller) -> Self {
        Self {
            archive_root: controller.checker_archive_root(),
            match_id: controller.match_id.clone(),
            round: controller.round,
            config: controller.config.clone(),
            content: controller.content.clone(),
            fingerprint: controller.fingerprint.clone(),
        }
    }

    pub(crate) async fn report(
        self,
        player: PlayerId,
        request: &CommitRequest,
        state: WorldState,
        finding: Finding,
    ) -> String {
        let Finding {
            failure,
            source,
            reason,
            browser_issue,
        } = finding;
        let reason = safe_reason(&reason, &request.slot_token);
        let shown_reason = browser_issue.map_or_else(
            || reason.clone(),
            |issue| {
                let browser_issue = safe_reason(&issue, &request.slot_token);
                format!("{browser_issue} {reason}")
            },
        );
        let draft = sanitized_draft(&request.draft, &self.content);
        let revision = request.draft.based_on_revision;
        let result = tokio::task::spawn_blocking(move || {
            let draft = draft?;
            let details = json!({
                "match_id": self.match_id,
                "round": self.round,
                "player": player,
                "revision": revision,
                "tick": state.tick,
                "fingerprint": self.fingerprint,
                "failure": failure,
                "client_check_source": source,
                "reason": reason,
                "draft": draft,
            });
            let digest = identity::canonical_hash(&details)?;
            let id = format!("checker-{}", &digest[..24]);
            let saved = self.archive_root.as_ref().ok_or("match archive unavailable".to_string())
                .and_then(|root| {
                    let report = json!({
                        "schema_version": 1,
                        "report_id": id,
                        "received_at_ms": crate::controller::now_ms(),
                        "server_commit": archive::server_commit(),
                        "checker": details,
                        "config": self.config,
                        "content": self.content,
                        "state": state,
                    });
                    archive::write_checker_incident(root, &id, &report)
                });
            match saved {
                Ok(()) => {
                    eprintln!("checker_incident status=saved report={id} round={} player={player} tick={} kind={failure:?}", self.round, details["tick"]);
                    Ok(id)
                }
                Err(error) => {
                    eprintln!("checker_incident status=save_failed report={id} round={} player={player} tick={} kind={failure:?} error={error}", self.round, details["tick"]);
                    Err(error)
                }
            }
        }).await;
        match result {
            Ok(Ok(id)) => {
                format!("{shown_reason} (this is a failure on the checker, reported · {id})")
            }
            _ => format!(
                "{shown_reason} (this is a failure on the checker, but the report could not be saved)"
            ),
        }
    }
}

fn safe_reason(reason: &str, token: &str) -> String {
    let redacted = if token.is_empty() {
        reason.to_string()
    } else {
        reason.replace(token, "[removed]")
    };
    redacted
        .chars()
        .filter(|c| !c.is_control())
        .take(1024)
        .collect()
}

fn sanitized_draft(draft: &TurnDraft, content: &Content) -> Result<Value> {
    let mut value = serde_json::to_value(draft).map_err(|e| e.to_string())?;
    redact(&mut value, "", &mut BTreeMap::new(), content);
    Ok(value)
}

fn redact(value: &mut Value, name: &str, ids: &mut BTreeMap<String, usize>, content: &Content) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                redact(value, key, ids, content);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact(item, name, ids, content);
            }
        }
        Value::String(id) if name == "local_id" && !id.is_empty() => {
            // Keep alias relationships and overlong-ID failures without saving client strings.
            let length = id.len();
            let next = ids.len();
            let index = ids.entry(id.clone()).or_insert(next);
            *id = format!("draft-{index}");
            if length > 64 {
                *id = format!("{id:0>65}");
            }
        }
        Value::String(key)
            if matches!(name, "type_key" | "items" | "queue")
                && !content.types.iter().any(|unit| unit.key == *key) =>
        {
            *key = "[unsupported type]".into();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_draft_reproduces_alias_rejection() {
        let content = atemporal_content::load_content(include_str!("../../../config/content.yaml"))
            .expect("stock content");
        let entry = json!({
            "local_id": "private-client-alias",
            "future_orders": "keep",
            "command": {
                "kind": "place_blueprints",
                "type_key": "turret",
                "tiles": [{"x": 3, "y": 4}],
                "priority": "medium",
                "output_directions": null,
            },
        });
        let input: TurnDraft = serde_json::from_value(json!({
            "based_on_revision": 4,
            "tick": 80,
            "commands": [entry, entry],
        }))
        .unwrap();
        let sanitized = sanitized_draft(&input, &content).unwrap();
        assert!(!sanitized.to_string().contains("private-client-alias"));
        let cleaned: TurnDraft = serde_json::from_value(sanitized).unwrap();
        let original = draft::resolve_local_references(&input, 5, 0).unwrap_err();
        let reproduced = draft::resolve_local_references(&cleaned, 5, 0).unwrap_err();
        assert_eq!(original, reproduced);
        assert_eq!(input.commands[0].command, cleaned.commands[0].command);
    }
}
