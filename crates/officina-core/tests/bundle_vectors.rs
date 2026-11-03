//! The `headers` section of `tests/vectors/bundle.json` (story 6.11 AC 5):
//! byte fixtures made by real git (`git bundle create`, v2 and v3, two
//! references, a prerequisite) and refusals cut from them, each read by
//! `workshop::bundle::parse_header`. The file is exempt from every
//! formatter: its bytes are the fixtures'.

use officina_core::workshop::bundle::{BundleError, BundleHeader, parse_header};
use officina_core::workshop::package::{
    Assessed, Expected, Outputs, PackageAssessment, QuarantineCause, assess,
};
use officina_core::workshop::tar::{BLOCK, PackageError, PackageReader, stream_of};
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

fn tar_cases() -> Vec<Value> {
    let set: Value = serde_json::from_str(VECTORS).unwrap();
    set["tar"]["cases"].as_array().unwrap().clone()
}

/// The stream a `tar` case's entries make, and what the reader gives.
fn read_case(case: &Value) -> (Result<PackageCase, PackageError>, Vec<u8>) {
    let entries: Vec<(String, String, u8)> = case["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e[0].as_str().unwrap().to_owned(),
                e[1].as_str().unwrap().to_owned(),
                e[2].as_str().unwrap().as_bytes()[0],
            )
        })
        .collect();
    let borrowed: Vec<(&str, &[u8], u8)> = entries
        .iter()
        .map(|(n, b, k)| (n.as_str(), b.as_bytes(), *k))
        .collect();
    let stream = stream_of(&borrowed);
    let mut written = Vec::new();
    let mut reader = PackageReader::new();
    let mut result = Ok(());
    for chunk in stream.chunks(97) {
        result = reader.push(chunk, &mut |part| {
            written.extend_from_slice(part);
            Ok(())
        });
        if result.is_err() {
            break;
        }
    }
    let got = result
        .and_then(|()| reader.finish())
        .map(|package| PackageCase {
            tip: package.tip.map(|t| String::from_utf8(t).unwrap()),
            manifest: package.manifest.map(|m| String::from_utf8(m).unwrap()),
            bundle: package
                .bundle_bytes
                .map(|_| String::from_utf8(written.clone()).unwrap()),
        });
    (got, written)
}

#[derive(Debug, PartialEq)]
struct PackageCase {
    tip: Option<String>,
    manifest: Option<String>,
    bundle: Option<String>,
}

fn error_code(error: &PackageError) -> &'static str {
    match error {
        PackageError::Tar(_) => "tar",
        PackageError::NameRefused => "name_refused",
        PackageError::Duplicate => "duplicate",
        PackageError::TypeRefused => "type_refused",
        PackageError::TooLong => "too_long",
        PackageError::Sink(_) => "sink",
    }
}

/// Story 6.13 AC 2 (D-229): each case reads as the section says.
#[test]
fn tar_cases_match() {
    let cases = tar_cases();
    assert!(!cases.is_empty());
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let (got, _) = read_case(case);
        match (&case["expect"]["ok"], &case["expect"]["err"]) {
            (ok @ Value::Object(_), Value::Null) => {
                let want = PackageCase {
                    tip: ok["tip"].as_str().map(str::to_owned),
                    manifest: ok["manifest"].as_str().map(str::to_owned),
                    bundle: ok["bundle"].as_str().map(str::to_owned),
                };
                assert_eq!(got.as_ref().ok(), Some(&want), "{name}");
            }
            (Value::Null, Value::String(code)) => {
                let error = got.expect_err(name);
                assert_eq!(error_code(&error), code, "{name}");
            }
            other => panic!("{name}: malformed case {other:?}"),
        }
    }
}

/// Story 6.13 AC 2 (D-229): every refused shape writes not one byte of
/// the bundle before it is refused, and is refused.
#[test]
fn every_hostile_shape_is_bundle_invalid() {
    let hostile: Vec<Value> = tar_cases()
        .into_iter()
        .filter(|c| c["expect"]["err"].is_string())
        .collect();
    assert!(hostile.len() > 10);
    for case in &hostile {
        let (got, written) = read_case(case);
        assert!(got.is_err(), "{}", case["name"]);
        assert!(written.is_empty(), "{}", case["name"]);
    }
}

/// The gate 1 entry's real `docker cp` stream (PAX records before a long
/// and an accented name): the format reads, and its extra files are
/// refused by the allow-list at the first of them.
#[test]
fn a_real_docker_stream_with_pax_records_is_read() {
    let stream = include_bytes!("../../../tests/vectors/fixtures/docker-cp-out.tar");
    let mut reader = PackageReader::new();
    let refused = stream
        .chunks(300)
        .map(|chunk| reader.push(chunk, &mut |_| Ok(())))
        .find(Result::is_err);
    assert_eq!(refused, Some(Err(PackageError::NameRefused)));
    // The same stream with its extra files left out reads whole.
    let tip_at = (0..stream.len())
        .step_by(BLOCK)
        .find(|at| stream[*at..].starts_with(b"out/tip\0"))
        .expect("the tip's header");
    let mut kept = stream[..BLOCK].to_vec();
    kept.extend_from_slice(&stream[tip_at..tip_at + 2 * BLOCK]);
    kept.resize(kept.len() + 2 * BLOCK, 0);
    let mut reader = PackageReader::new();
    reader.push(&kept, &mut |_| Ok(())).unwrap();
    let package = reader.finish().unwrap();
    assert_eq!(package.tip.map(|t| t.len()), Some(41));
}

fn assess_section() -> (Expected, Vec<Value>) {
    let set: Value = serde_json::from_str(VECTORS).unwrap();
    let section = &set["assess"];
    (
        serde_json::from_value(section["expected"].clone()).unwrap(),
        section["cases"].as_array().unwrap().clone(),
    )
}

fn outputs_of(case: &Value) -> Outputs {
    let given = &case["outputs"];
    let mut outputs: Outputs = serde_json::from_value(serde_json::json!({
        "tip": given["tip"].as_str().map(|t| t.as_bytes().to_vec()),
        "bundle_bytes": given["bundle_bytes"],
        "tip_held": given["tip_held"],
        "verified": given["verified"],
        "fetched": given["fetched"],
        "base_is_ancestor": given["base_is_ancestor"],
        "commits": given["commits"],
        "paths": given["paths"],
        "inflated_bytes": given["inflated_bytes"],
    }))
    .unwrap();
    outputs.header = match &given["header"] {
        Value::Null => None,
        Value::String(code) => Some(Err(BundleError::from_code(code).unwrap())),
        header => Some(Ok(
            serde_json::from_value::<BundleHeader>(header.clone()).unwrap()
        )),
    };
    outputs
}

fn assessed_json(assessed: &Assessed) -> Value {
    match assessed {
        Assessed::Done(done) => serde_json::json!({ "done": done }),
        next => {
            let step = match next {
                Assessed::CheckTipHeld => "check_tip_held",
                Assessed::ParseHeader => "parse_header",
                Assessed::Verify => "verify",
                Assessed::Fetch => "fetch",
                Assessed::AncestryAndCount => "ancestry_and_count",
                Assessed::Diff => "diff",
                Assessed::Inflate => "inflate",
                Assessed::Done(_) => unreachable!(),
            };
            serde_json::json!({ "next": step })
        }
    }
}

/// Story 6.13 AC 3 (AD-111): each case's outputs give its step or answer.
#[test]
fn assess_cases_match() {
    let (expected, cases) = assess_section();
    assert!(!cases.is_empty());
    for case in &cases {
        let got = assessed_json(&assess(&outputs_of(case), &expected));
        assert_eq!(got, case["expect"], "{}", case["name"]);
    }
}

/// Story 6.13 AC 3: every quarantine cause the checks give has a case;
/// the scan's is collection's, after them (AD-112).
#[test]
fn every_quarantine_cause_has_a_case() {
    let (_, cases) = assess_section();
    for cause in QuarantineCause::ALL
        .iter()
        .filter(|c| **c != QuarantineCause::CredentialFound)
    {
        assert!(
            cases
                .iter()
                .any(|c| c["expect"]["done"]["cause"] == cause.code()),
            "{cause:?} has no assess case"
        );
    }
}

/// D-231: every way of naming the mount commits into it, derived from the
/// mount path itself.
#[test]
fn every_mount_shape_is_mount_committed() {
    let (expected, cases) = assess_section();
    let clean = cases
        .iter()
        .find(|c| {
            c["expect"]["done"]["cause"].is_null() && c["expect"]["done"]["kind"] == "package"
        })
        .unwrap();
    for mount in &expected.mounts {
        let shapes = [
            mount.clone(),
            mount.to_uppercase(),
            format!("{mount}."),
            format!("{mount} "),
            format!("{mount}/x"),
            format!("{}/x", mount.to_uppercase()),
            format!("{mount}./x"),
        ];
        for shape in shapes {
            let mut outputs = outputs_of(clean);
            outputs.paths = Some(vec!["src/a.rs".to_owned(), shape.clone()]);
            assert!(
                matches!(
                    assess(&outputs, &expected),
                    Assessed::Done(PackageAssessment::Package {
                        cause: Some(QuarantineCause::MountCommitted),
                        ..
                    })
                ),
                "{shape}"
            );
        }
    }
}
