//! `tests/vectors/sensitive.json` (story 6.15 AC 1, AD-113, D-246): every
//! entry of `SENSITIVE_PATTERNS` with a matching and a non-matching path,
//! its case and depth variants derived here, and packages whose changed
//! files give their groups. The file is exempt from every formatter.

use officina_core::workshop::floor::ReviewScope;
use officina_core::workshop::sensitive::{
    ChangedFile, PackageGroup, SENSITIVE_PATTERNS, SensitiveGroup, SensitiveRules, families,
    groups, sort_files,
};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/sensitive.json");

fn set() -> Value {
    let set: Value = serde_json::from_str(VECTORS).unwrap();
    assert_eq!(set["schema"], 1);
    set
}

fn modified(path: &str) -> ChangedFile {
    ChangedFile {
        status: "M".to_owned(),
        old_mode: "100644".to_owned(),
        new_mode: "100644".to_owned(),
        paths: vec![path.to_owned()],
        ..ChangedFile::default()
    }
}

fn gives(path: &str, group: SensitiveGroup) -> bool {
    families(&modified(path), &SensitiveRules::default()).contains(&group)
}

fn rules_of(case: &Value) -> SensitiveRules {
    let rules = &case["rules"];
    SensitiveRules {
        review: ReviewScope::from_code(rules["review"].as_str().unwrap()).unwrap(),
        project_patterns: serde_json::from_value(rules["sensitive_paths"].clone()).unwrap(),
        floor_in_project: rules["floor_in_project"].as_str().map(str::to_owned),
    }
}

/// The groups package case `name` gives, and the ones it expects.
fn package(name: &str) -> (Vec<PackageGroup>, Vec<PackageGroup>) {
    let set = set();
    let case = set["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no package case {name}"));
    let mut files: Vec<ChangedFile> = serde_json::from_value(case["files"].clone()).unwrap();
    sort_files(&mut files);
    let expected: Vec<PackageGroup> = serde_json::from_value(case["groups"].clone()).unwrap();
    (groups(&files, &rules_of(case)), expected)
}

fn check(name: &str) {
    let (got, expected) = package(name);
    assert_eq!(got, expected, "{name}");
}

/// AD-113: each pattern of the constant has a case whose matching path
/// gives its group at the root, nested, and in other cases, and whose
/// non-matching path does not; no case names a pattern the constant
/// lacks.
#[test]
fn every_pattern_has_a_matching_and_a_non_matching_case() {
    let set = set();
    let cases = set["patterns"].as_array().unwrap();
    let mut seen = 0;
    for (group, patterns) in SENSITIVE_PATTERNS {
        for pattern in *patterns {
            let case = cases
                .iter()
                .find(|c| c["pattern"] == *pattern)
                .unwrap_or_else(|| panic!("{pattern} has no case"));
            seen += 1;
            let matching = case["matches"].as_str().unwrap();
            let missing = case["misses"].as_str().unwrap();
            for variant in [
                matching.to_owned(),
                matching.to_uppercase(),
                matching.to_lowercase(),
                format!("deep/er/{matching}"),
                format!("A/{}", matching.to_uppercase()),
            ] {
                assert!(gives(&variant, *group), "{pattern}: {variant}");
            }
            assert!(!gives(missing, *group), "{pattern}: {missing}");
        }
    }
    assert_eq!(
        seen,
        cases.len(),
        "a case names a pattern the constant lacks"
    );
}

/// Every group, the `file` group included, comes out of some package case.
#[test]
fn every_group_has_a_case() {
    let set = set();
    let mut given = Vec::new();
    for case in set["packages"].as_array().unwrap() {
        let (got, expected) = package(case["name"].as_str().unwrap());
        assert_eq!(got, expected, "{}", case["name"]);
        given.extend(got.into_iter().map(|g| g.group));
    }
    for group in SensitiveGroup::ALL {
        assert!(given.contains(group), "{group:?} has no case");
    }
}

#[test]
fn renames_match_both_sides() {
    check("rename");
}

#[test]
fn modes_give_their_groups() {
    check("modes");
}

#[test]
fn review_all_makes_one_group_per_file() {
    check("review all");
}

/// D-246, review F50: a segment shaped as an 8.3 short name gives
/// `file_modes`, on either side of a rename.
#[test]
fn an_8_3_alias_segment_gives_file_modes() {
    check("8.3 alias");
}

/// D-246, review F27: a blob git reports as binary gives `binary`.
#[test]
fn a_binary_blob_gives_binary() {
    check("binary");
}

#[test]
fn the_floor_file_and_project_patterns_give_their_groups() {
    check("officina and project");
}
