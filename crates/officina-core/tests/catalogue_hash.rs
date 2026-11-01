//! The canonical catalogue and its hashes (AD-24, AD-76, AD-101, D-244): a
//! golden fixture, which declares templates, an `auto` action with its
//! check, `[auto_review]` and every workshop table with its files, pins the
//! global and the relay-section hash, cosmetic edits keep them, and every
//! canonical leaf moves the global one.

use std::collections::{BTreeMap, BTreeSet};

use officina_core::relay::catalogue::{
    ApprovedCatalogue, CANONICAL_FORMAT, CanonicalCatalogue, RelayFloor, diff, hash,
    load_with_files, relay_hash,
};
use officina_core::workshop::floor::{
    CatalogueFiles, ContextFile, CredentialKind, DockerfileFiles, FileContent, ReviewScope,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const GOLDEN: &str = include_str!("data/relay/golden-floor.toml");
const VARIANT: &str = include_str!("data/relay/golden-floor-variant.toml");

/// A fixed floor path, so the hash does not depend on the checkout.
const FLOOR_PATH: &str = "C:/path/to/work/officina.toml";

/// The canonical format and hash of the golden floor. A change to what the
/// canonical form holds fails this test until `CANONICAL_FORMAT` is bumped
/// and the new pair pinned here: every approval is then invalidated on
/// purpose (AD-24).
const PINNED: (u32, &str) = (
    5,
    "36159419817910af0ea6d089fe296250b51b6868e43c0e3b76025617546093e1",
);

/// The golden floor's relay-section hash (D-244), pinned like the global
/// one: what `officina run` compares.
const PINNED_RELAY: &str = "ca8338aa93d4bd09d52f6a9aef2a87fe391ca52a80e291b21f9e7f563ee9e9d3";

/// The previous format's approval of the golden floor, as `main` wrote it
/// before this format (D-131).
const PREVIOUS: &str = include_str!("data/relay/approved-previous-format.json");

/// The files the host would read for the golden floor: its CA file, its
/// project Dockerfile and that file's folder.
fn golden_files() -> CatalogueFiles {
    let dockerfile =
        b"FROM rust:1-bookworm AS build\nFROM officina-workshop:local\nCOPY --from=build /x /y\n"
            .to_vec();
    CatalogueFiles {
        read: true,
        ca_file: Some(FileContent::Bytes(
            b"-----BEGIN CERTIFICATE-----\nTUlJQmV4YW1wbGU=\n-----END CERTIFICATE-----\n".to_vec(),
        )),
        dockerfiles: BTreeMap::from([(
            "supervision".to_owned(),
            DockerfileFiles {
                dockerfile: FileContent::Bytes(dockerfile.clone()),
                context: Ok(vec![
                    ContextFile {
                        name: "workshop.Dockerfile".to_owned(),
                        bytes: dockerfile,
                    },
                    ContextFile {
                        name: "requirements.txt".to_owned(),
                        bytes: b"ruff\n".to_vec(),
                    },
                ]),
            },
        )]),
    }
}

fn golden() -> RelayFloor {
    load_with_files(GOLDEN, FLOOR_PATH, &golden_files()).expect("the golden floor parses")
}

#[test]
fn golden_fixture_hash_is_pinned() {
    let golden = golden();
    assert!(
        golden.catalogue.issues.is_empty(),
        "the golden floor is fully valid: {:#?}",
        golden.catalogue.issues
    );
    assert_eq!(
        (CANONICAL_FORMAT, golden.hash.as_str()),
        PINNED,
        "the canonical form changed: bump CANONICAL_FORMAT and pin the new hash"
    );
    assert!(
        golden.canonical.workshop.is_some(),
        "the golden floor fills the workshop"
    );
}

#[test]
fn relay_section_hash_is_pinned() {
    let golden = golden();
    assert_eq!(golden.relay_hash, PINNED_RELAY);
    assert_eq!(relay_hash(&golden.canonical), golden.relay_hash);
    let without_files = load_with_files(GOLDEN, FLOOR_PATH, &CatalogueFiles::not_read()).unwrap();
    assert_eq!(
        without_files.relay_hash, golden.relay_hash,
        "officina run reads no workshop file and computes the same relay-section hash"
    );
    assert_ne!(golden.relay_hash, golden.hash);
}

/// AD-76's four facts for an approval in the previous format: it parses,
/// keeps its own stored hash, differs from today's hashes, and its diff
/// against today's floor shows the format change and what the new format
/// adds.
#[test]
fn a_previous_format_approval_is_void() {
    let previous: ApprovedCatalogue =
        serde_json::from_str(PREVIOUS).expect("the previous format parses");
    assert_eq!(previous.canonical.format, CANONICAL_FORMAT - 1);
    assert!(
        previous.relay_hash.is_none(),
        "written before the relay-section hash"
    );
    let stored: Value = serde_json::from_str(PREVIOUS).unwrap();
    assert_eq!(previous.hash, stored["hash"].as_str().unwrap());
    let golden = golden();
    assert_ne!(previous.hash, golden.hash);
    assert_ne!(previous.hash, golden.relay_hash);
    let d = diff(&previous.canonical, &golden.canonical);
    let format = d.format.as_ref().expect("the format changed");
    assert_eq!(format.after, serde_json::json!(CANONICAL_FORMAT));
    assert!(!d.workshop.is_empty(), "the workshop table is new");
    assert!(d.workshop.iter().all(|change| change.before.is_null()));
    assert_eq!(d.templates.added, ["isolated-bump"]);
    assert!(
        d.projects
            .changed
            .iter()
            .any(|entry| entry.name == "supervision"),
        "the project's workshop table is new"
    );
}

#[test]
fn hash_is_sha256_of_the_serialized_form_in_lowercase_hex() {
    let golden = golden();
    let digest = Sha256::digest(serde_json::to_vec(&golden.canonical).unwrap());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(golden.hash, hex);
    assert_eq!(hash(&golden.canonical), hex);
}

#[test]
fn formatting_and_excluded_fields_keep_the_hash() {
    let golden = golden();
    let variant =
        load_with_files(VARIANT, FLOOR_PATH, &golden_files()).expect("the variant parses");
    assert!(
        !variant.catalogue.issues.is_empty(),
        "the variant carries an invalid action"
    );
    assert_eq!(variant.canonical, golden.canonical);
    assert_eq!(variant.hash, golden.hash);
    assert_eq!(variant.relay_hash, golden.relay_hash);
}

/// One step of a path into a JSON value.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Index(usize),
}

fn at<'a>(value: &'a mut Value, path: &[Step]) -> &'a mut Value {
    path.iter().fold(value, |v, step| match step {
        Step::Key(key) => &mut v[key.as_str()],
        Step::Index(index) => &mut v[*index],
    })
}

/// Every leaf of `value`: scalars, nulls and empty containers.
fn leaves(value: &Value, path: &mut Vec<Step>, out: &mut Vec<Vec<Step>>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, item) in map {
                path.push(Step::Key(key.clone()));
                leaves(item, path, out);
                path.pop();
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                path.push(Step::Index(index));
                leaves(item, path, out);
                path.pop();
            }
        }
        _ => out.push(path.clone()),
    }
}

/// Every map key of `value` with its parent path.
fn keys(value: &Value, path: &mut Vec<Step>, out: &mut Vec<(Vec<Step>, String)>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                out.push((path.clone(), key.clone()));
                path.push(Step::Key(key.clone()));
                keys(item, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                path.push(Step::Index(index));
                keys(item, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn strings(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => {
            out.insert(s.clone());
        }
        Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
        Value::Object(map) => map.values().for_each(|i| strings(i, out)),
        _ => {}
    }
}

/// Replacements for a leaf. Strings also try every other string of the
/// catalogue, so a closed enum (class, runner, autonomy) gets another valid
/// variant; the golden floor uses at least two of each, and the workshop's
/// enums, set once per floor, add their codes from their constants. They
/// try null last, for an optional field whose enum has one variant
/// (`forwarded_values`).
fn mutations(leaf: &Value, pool: &BTreeSet<String>) -> Vec<Value> {
    let enum_codes = CredentialKind::ALL
        .iter()
        .map(|k| k.code())
        .chain(ReviewScope::ALL.iter().map(|m| m.code()))
        .map(str::to_owned);
    let pool: BTreeSet<String> = pool.iter().cloned().chain(enum_codes).collect();
    let others: Vec<Value> = pool
        .iter()
        .filter(|s| leaf.as_str() != Some(s.as_str()))
        .map(|s| Value::String(s.clone()))
        .collect();
    match leaf {
        Value::String(s) => std::iter::once(Value::String(format!("{s}x")))
            .chain(others)
            .chain(std::iter::once(Value::Null))
            .collect(),
        Value::Number(n) => vec![Value::from(n.as_u64().unwrap_or(0) + 1)],
        Value::Bool(b) => vec![Value::Bool(!b)],
        Value::Null => others,
        Value::Array(_) => others.into_iter().map(|s| Value::Array(vec![s])).collect(),
        Value::Object(_) => others
            .into_iter()
            .map(|s| Value::Object(serde_json::Map::from_iter([("x".to_owned(), s)])))
            .collect(),
    }
}

#[test]
fn every_canonical_field_changes_the_hash() {
    let golden = golden();
    let original = serde_json::to_value(&golden.canonical).unwrap();
    let mut pool = BTreeSet::new();
    strings(&original, &mut pool);

    let mut paths = Vec::new();
    leaves(&original, &mut Vec::new(), &mut paths);
    assert!(
        paths.len() > 40,
        "the golden floor fills the canonical form"
    );
    for path in paths {
        let leaf = at(&mut original.clone(), &path).clone();
        let changed = mutations(&leaf, &pool).into_iter().find_map(|replacement| {
            let mut mutated = original.clone();
            *at(&mut mutated, &path) = replacement;
            serde_json::from_value::<CanonicalCatalogue>(mutated).ok()
        });
        let changed = changed.unwrap_or_else(|| panic!("no valid change for the leaf at {path:?}"));
        assert_ne!(
            hash(&changed),
            golden.hash,
            "changing the leaf at {path:?} keeps the hash"
        );
    }

    let mut map_keys = Vec::new();
    keys(&original, &mut Vec::new(), &mut map_keys);
    for (parent, key) in map_keys {
        let mut mutated = original.clone();
        let Value::Object(map) = at(&mut mutated, &parent) else {
            continue;
        };
        let item = map.remove(&key).unwrap();
        map.insert(format!("{key}x"), item);
        // Renaming a struct field does not deserialize; renaming an action,
        // project, parameter or env key does, and must move the hash.
        if let Ok(changed) = serde_json::from_value::<CanonicalCatalogue>(mutated) {
            assert_ne!(hash(&changed), golden.hash, "renaming {key} at {parent:?}");
        }
    }
}
