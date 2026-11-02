//! The `headers` section of `tests/vectors/bundle.json` (story 6.11 AC 5):
//! byte fixtures made by real git (`git bundle create`, v2 and v3, two
//! references, a prerequisite) and refusals cut from them, each read by
//! `workshop::bundle::parse_header`. The file is exempt from every
//! formatter: its bytes are the fixtures'.

use officina_core::workshop::bundle::{BundleError, BundleHeader, parse_header};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/bundle.json");

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex"))
        .collect()
}

#[test]
fn header_cases_match() {
    let set: Value = serde_json::from_str(VECTORS).unwrap();
    assert_eq!(set["schema"], 1);
    let cases = set["headers"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let got = parse_header(&hex(case["bytes_hex"].as_str().unwrap()));
        match (&case["expect"]["ok"], &case["expect"]["error"]) {
            (Value::Null, Value::String(code)) => {
                assert_eq!(got, Err(BundleError::from_code(code).unwrap()), "{name}")
            }
            (ok, Value::Null) => {
                let wanted: BundleHeader = serde_json::from_value(ok.clone()).unwrap();
                assert_eq!(got, Ok(wanted), "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    for error in BundleError::ALL {
        assert!(
            cases.iter().any(|c| c["expect"]["error"] == error.code()),
            "{error:?} has no case"
        );
    }
}
