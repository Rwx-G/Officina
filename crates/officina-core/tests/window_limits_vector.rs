//! The workshop limits the window sizes itself for (story 6.20 AC 2,
//! D-618): the live-run count on the Orchestrator view switch sits in a
//! slot wide enough for `floor::MAX_WORKERS_RANGE`'s upper bound, and the
//! frontend cannot import a Rust constant, so it reads
//! `tests/vectors/window_limits.json`. This test rebuilds the file from the
//! constant and refuses a stale one. `OFFICINA_WRITE_VECTORS=1` writes it
//! for a reviewed change.

use serde_json::{Value, json};

use officina_core::workshop::floor::MAX_WORKERS_RANGE;

const VECTORS: &str = include_str!("../../../tests/vectors/window_limits.json");

#[expect(
    clippy::disallowed_methods,
    reason = "a test's write mode for a reviewed change of the golden file"
)]
fn writing() -> bool {
    std::env::var("OFFICINA_WRITE_VECTORS").is_ok_and(|v| v == "1")
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test's write mode for a reviewed change of the golden file"
)]
fn write(all: &Value) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/window_limits.json"
    );
    std::fs::write(path, serde_json::to_string_pretty(all).unwrap() + "\n").unwrap();
}

fn rebuilt() -> Value {
    json!({
        "_doc": "Rebuilt from officina_core::workshop::floor::MAX_WORKERS_RANGE by crates/officina-core/tests/window_limits_vector.rs; [first, last], inclusive.",
        "max_workers": [MAX_WORKERS_RANGE.start(), MAX_WORKERS_RANGE.end()],
    })
}

#[test]
fn the_file_is_the_constant() {
    let fresh = rebuilt();
    if writing() {
        write(&fresh);
        return;
    }
    let committed: Value = serde_json::from_str(VECTORS).expect("window_limits.json parses");
    assert_eq!(
        committed, fresh,
        "tests/vectors/window_limits.json is stale: rerun with OFFICINA_WRITE_VECTORS=1"
    );
}
