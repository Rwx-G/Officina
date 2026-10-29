//! Replays the golden spawn vectors (AD-79, AD-95 spawn row) through each
//! kind's builder, and refuses a set that leaves a kind without a case.

#[path = "../../../tests/support/spawn_cases.rs"]
mod spawn_cases;

use officina_core::spawn::VectorKind;

#[test]
fn every_case_matches_its_builder() {
    let set = spawn_cases::load();
    assert!(!set.cases.is_empty());
    for case in &set.cases {
        let built = serde_json::to_value(case.build(None)).unwrap();
        assert_eq!(built, case.expect, "case {}", case.name);
    }
}

#[test]
fn every_kind_has_a_case() {
    let set = spawn_cases::load();
    for kind in VectorKind::ALL {
        assert!(
            set.cases.iter().any(|case| case.kind() == *kind),
            "{kind:?} has no case in tests/vectors/spawn.json"
        );
    }
}
