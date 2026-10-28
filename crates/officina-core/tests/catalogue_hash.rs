//! The canonical catalogue and its hash (AD-24): a golden fixture pins the
//! hash, cosmetic edits keep it, and every canonical leaf moves it.

use std::collections::BTreeSet;

use officina_core::relay::catalogue::{
    CANONICAL_FORMAT, CanonicalCatalogue, RelayFloor, hash, load,
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
    2,
    "3113cd73c06f616216aed61dc1910f43f4176920f4c0988011f41cb5de3cdb14",
);

fn golden() -> RelayFloor {
    load(GOLDEN, FLOOR_PATH).expect("the golden floor parses")
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
    let variant = load(VARIANT, FLOOR_PATH).expect("the variant parses");
    assert!(
        !variant.catalogue.issues.is_empty(),
        "the variant carries an invalid action"
    );
    assert_eq!(variant.canonical, golden.canonical);
    assert_eq!(variant.hash, golden.hash);
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
/// variant; the golden floor uses at least two of each.
fn mutations(leaf: &Value, pool: &BTreeSet<String>) -> Vec<Value> {
    let others: Vec<Value> = pool
        .iter()
        .filter(|s| leaf.as_str() != Some(s.as_str()))
        .map(|s| Value::String(s.clone()))
        .collect();
    match leaf {
        Value::String(s) => std::iter::once(Value::String(format!("{s}x")))
            .chain(others)
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
