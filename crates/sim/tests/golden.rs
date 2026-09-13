//! Run every authored tiny world through the real engine and compare with the golden comparator.
use atemporal_sim::*;
use std::{fs, path::PathBuf, sync::atomic::AtomicBool};

fn fixtures() -> Vec<GoldenWorldFixture> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/worlds");
    let mut out: Vec<GoldenWorldFixture> = vec![];
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|s| s == "json") {
            out.push(serde_json::from_slice(&fs::read(path).unwrap()).unwrap());
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn run_all(request: &SimRequest) -> (RunResult, Vec<WorldState>) {
    let mut checkpoints = vec![];
    let mut sink = |o: Output| {
        if let Output::Checkpoint(s) = o {
            checkpoints.push(s);
        }
    };
    let result = run(request, &AtomicBool::new(false), &mut sink).unwrap();
    (result, checkpoints)
}

#[test]
fn golden_worlds_pass_through_the_real_engine() {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), 8);
    for fixture in &fixtures {
        let (result, _) = run_all(&fixture.request);
        // Exact states for every asserted tick come from in-process reconstruction.
        let mut states = vec![];
        let mut ticks: Vec<Tick> = fixture
            .expected
            .states
            .iter()
            .map(|s| s.state_tick)
            .collect();
        ticks.push(result.outcome.terminal_state_tick);
        ticks.sort();
        ticks.dedup();
        for tick in ticks {
            states.push(reconstruct(&fixture.request, tick).unwrap());
        }
        golden::verify(fixture, &states, &result.outcome, &result.command_outcomes)
            .unwrap_or_else(|e| panic!("{e}\noutcome: {:?}", result.outcome));
    }
}

#[test]
fn checkpoint_replay_matches_full_replay() {
    for fixture in fixtures() {
        let (full, checkpoints) = run_all(&fixture.request);
        for checkpoint in checkpoints {
            if checkpoint.tick == fixture.request.checkpoint.tick
                || checkpoint.tick >= full.outcome.terminal_state_tick
            {
                continue;
            }
            let mut request = fixture.request.clone();
            request.checkpoint = checkpoint;
            let (partial, _) = run_all(&request);
            assert_eq!(
                partial.final_hash, full.final_hash,
                "{}: checkpoint {} diverges",
                fixture.name, request.checkpoint.tick
            );
            assert_eq!(
                partial.outcome, full.outcome,
                "checkpoint replay must retain full survival history"
            );
        }
    }
}
