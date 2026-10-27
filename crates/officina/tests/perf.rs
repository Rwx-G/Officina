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

#[test]
#[ignore = "timing test, run in release by CI"]
fn hook_latency_p95() {
    let env = TestEnv::new();
    let payload = big_payload();
    for _ in 0..5 {
        env.run(&["hook"], &payload);
    }
    let mut samples: Vec<Duration> = (0..200)
        .map(|_| {
            let started = Instant::now();
            let out = env.run(&["hook"], &payload);
            assert!(out.status.success());
            started.elapsed()
        })
        .collect();
    samples.sort();
    let p95 = samples[189];
    let p50 = samples[99];
    println!("hook latency p50 {p50:?} p95 {p95:?}");
    assert!(p95 < Duration::from_millis(30), "p95 {p95:?}");
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
