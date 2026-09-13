use atemporal_content::*;
use atemporal_contracts::{identity::*, scoring::*, *};
use std::{collections::BTreeSet, fs, path::PathBuf};
#[test]
fn golden_requests_have_valid_content_state_fingerprints_and_endpoint_accounting() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut count = 0;
    for entry in fs::read_dir(root.join("fixtures/worlds")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|s| s != "json") {
            continue;
        }
        let fixture: GoldenWorldFixture = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let request = &fixture.request;
        validate_config(&request.config).unwrap();
        normalize_content(request.content.clone()).unwrap();
        assert_eq!(
            content_hash(&request.content).unwrap(),
            request.fingerprint.content_hash
        );
        assert_eq!(
            canonical_hash(&request.config).unwrap(),
            request.fingerprint.config_hash
        );
        assert_eq!(
            world_hash(&request.checkpoint).unwrap(),
            fixture.initial_hash
        );
        validate_outcome(
            request.config.player_count,
            &request.config.multiplayer,
            &fixture.expected.outcome,
        )
        .unwrap();
        assert!(fixture.expected.outcome.terminal_state_tick <= request.end_tick_exclusive);
        assert!(request.minimum_end_tick <= fixture.expected.outcome.terminal_state_tick);
        for state in &fixture.expected.states {
            assert!(state.state_tick <= fixture.expected.outcome.terminal_state_tick);
        }
        let ids: BTreeSet<_> = request
            .events
            .iter()
            .flat_map(|t| t.commands.iter().map(|c| c.id.clone()))
            .collect();
        for outcome in &fixture.expected.command_outcomes {
            assert!(ids.contains(&outcome.command_id));
        }
        for mask in &request.suppressions {
            assert!(ids.contains(&mask.source_command_id));
            assert!(ids.contains(&mask.historical_command_id));
        }
        for p in 0..request.config.player_count {
            assert_eq!(
                request
                    .checkpoint
                    .control_groups
                    .iter()
                    .filter(|g| g.id.owner == p)
                    .count(),
                10
            );
        }
        if fixture.name == "quiet" {
            let mut final_state = request.checkpoint.clone();
            final_state.tick = 3;
            assert_eq!(
                Some(world_hash(&final_state).unwrap()),
                fixture.expected.final_hash
            );
        }
        count += 1;
    }
    assert_eq!(count, 8, "update acceptance coverage deliberately");
}
