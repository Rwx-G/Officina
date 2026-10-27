//! Hook latency and concurrent appends (story 1.3, AD-10, AD-17). Ignored by
//! default: run with `cargo test --release -p officina -- --ignored`.

mod common;

use std::time::{Duration, Instant};

use common::{TestEnv, read_events};

fn big_payload() -> Vec<u8> {
    serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "00000000-0000-0000-0000-000000000001",
        "cwd": "C:/work/repo-a",
        "last_assistant_message": "x".repeat(200_000),
    })
    .to_string()
    .into_bytes()
}

fn p95(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() * 95 / 100 - 1]
}

fn timed(runs: usize, mut f: impl FnMut()) -> Vec<Duration> {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            f();
            started.elapsed()
        })
        .collect()
}

/// PRD section 7: p95 < 30 ms as Claude Code pays it, process start included.
/// That absolute figure depends on how fast the machine creates processes, so
/// the test also times a baseline process that does nothing (the fake claude
/// answering `--version`) and gates Officina's own cost, hook minus baseline,
/// at 30 ms everywhere. The absolute budget defaults to 30 ms (the
/// workstation figure) and CI raises it with `OFFICINA_HOOK_P95_BUDGET_MS`,
/// because hosted runners start any process several times slower.
#[test]
#[ignore = "timing test, run in release by CI"]
fn hook_latency_p95() {
    let env = TestEnv::new();
    let payload = big_payload();
    for _ in 0..5 {
        env.run(&["hook"], &payload);
    }
    let hook = p95(timed(200, || {
        assert!(env.run(&["hook"], &payload).status.success());
    }));
    let baseline = p95(timed(200, || {
        let out = std::process::Command::new(common::FAKE_CLAUDE)
            .arg("--version")
            .output()
            .unwrap();
        assert!(out.status.success());
    }));
    let own_cost = hook.saturating_sub(baseline);
    let budget = Duration::from_millis(
        std::env::var("OFFICINA_HOOK_P95_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
    );
    println!(
        "hook p95 {hook:?}, empty process p95 {baseline:?}, own cost {own_cost:?}, budget {budget:?}"
    );
    assert!(
        own_cost < Duration::from_millis(30),
        "own cost {own_cost:?}"
    );
    assert!(hook < budget, "p95 {hook:?} over {budget:?}");
}

#[test]
#[ignore = "stress test, run in release by CI"]
fn concurrent_appends_never_interleave() {
    let env = TestEnv::new();
    let payload = r#"{"hook_event_name":"UserPromptSubmit","session_id":"s","prompt":"a prompt long enough to make the write non-trivial, with some accents: é è à ç"}"#.as_bytes();
    std::thread::scope(|scope| {
        for _ in 0..16 {
            scope.spawn(|| {
                for _ in 0..500 {
                    assert!(env.run(&["hook"], payload).status.success());
                }
            });
        }
    });
    let lines = read_events(&env);
    assert_eq!(lines.len(), 8000);
}
