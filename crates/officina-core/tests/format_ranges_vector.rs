//! The format ranges as the window reads them (story 6.19 AC 4, D-245):
//! the review panel renders every character of
//! `params::FORBIDDEN_FORMAT_RANGES` as a visible escape, and the
//! frontend cannot import a Rust constant, so it reads
//! `tests/vectors/format_ranges.json`. This test rebuilds the file from the
//! constant and refuses a stale one. `OFFICINA_WRITE_VECTORS=1` writes it
//! for a reviewed change.

use serde_json::{Value, json};

use officina_core::relay::params::FORBIDDEN_FORMAT_RANGES;

const VECTORS: &str = include_str!("../../../tests/vectors/format_ranges.json");

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
        "/../../tests/vectors/format_ranges.json"
    );
    std::fs::write(path, serde_json::to_string_pretty(all).unwrap() + "\n").unwrap();
}

fn rebuilt() -> Value {
    let ranges: Vec<[u32; 2]> = FORBIDDEN_FORMAT_RANGES
        .iter()
        .map(|range| [u32::from(*range.start()), u32::from(*range.end())])
        .collect();
    json!({
        "_doc": "Rebuilt from officina_core::relay::params::FORBIDDEN_FORMAT_RANGES by crates/officina-core/tests/format_ranges_vector.rs; each range is [first, last] code point, inclusive.",
        "ranges": ranges,
    })
}

#[test]
fn the_file_is_the_constant() {
    let fresh = rebuilt();
    if writing() {
        write(&fresh);
        return;
    }
    let committed: Value = serde_json::from_str(VECTORS).expect("format_ranges.json parses");
    assert_eq!(
        committed, fresh,
        "tests/vectors/format_ranges.json is stale: rerun with OFFICINA_WRITE_VECTORS=1"
    );
}
