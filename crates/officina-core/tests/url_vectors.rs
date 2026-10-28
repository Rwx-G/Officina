//! Replays the hand-written URL template vector set (AD-36, D-40) through
//! `relay::url`, and the credential binding of a case's hosts (D-37)
//! through `relay::http::host_is_bound`; refuses a set that no longer covers
//! every issue kind, every byte class of the encoder and both outcomes of
//! the binding; and checks on every filled case that the values changed
//! neither the scheme, the authority nor the URL's shape (story 2.16 AC 5).

use std::collections::{BTreeMap, BTreeSet};

use officina_core::relay::http::host_is_bound;
use officina_core::relay::url::{
    ByteClass, UrlIssue, UrlRefusal, UrlTemplate, byte_class, host_entry,
};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/url.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    template: String,
    #[serde(default)]
    params: BTreeMap<String, String>,
    /// A credential's `hosts`, as a floor writes them.
    #[serde(default)]
    bound_to: Option<Vec<String>>,
    expect: Expect,
    /// Whether that credential goes with the request.
    #[serde(default)]
    bound: Option<bool>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Expect {
    Url(String),
    Issue(UrlIssue),
    Refusal(UrlRefusal),
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

fn replay(case: &Case) -> Expect {
    match UrlTemplate::parse(&case.template) {
        Err(issue) => Expect::Issue(issue),
        Ok(template) => match template.fill(&case.params) {
            Ok(url) => Expect::Url(url),
            Err(refusal) => Expect::Refusal(refusal),
        },
    }
}

/// Whether a credential of `case.bound_to` goes with the request: its
/// entries read as the catalogue reads them (an entry `host_entry` refuses
/// binds nothing), then compared with the template's host.
fn binds(case: &Case) -> Option<bool> {
    let entries = case.bound_to.as_ref()?;
    let template = UrlTemplate::parse(&case.template).ok()?;
    let hosts: Vec<String> = entries.iter().filter_map(|e| host_entry(e)).collect();
    Some(host_is_bound(&hosts, template.host()))
}

#[test]
fn every_case_matches() {
    for case in &load().cases {
        assert_eq!(replay(case), case.expect, "case: {}", case.name);
        assert_eq!(
            case.bound_to.is_some(),
            case.bound.is_some(),
            "case {}: `bound_to` and `bound` go together",
            case.name
        );
        assert_eq!(binds(case), case.bound, "case: {}", case.name);
    }
}

/// A URL's scheme, authority, path segment count and query item count.
fn shape(url: &str) -> (String, String, usize, usize) {
    let (scheme, rest) = url.split_once("://").expect("a URL has a scheme");
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (path, query) = match rest[authority_end..].split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (&rest[authority_end..], None),
    };
    (
        scheme.to_owned(),
        rest[..authority_end].to_owned(),
        path.matches('/').count(),
        query.map_or(0, |q| q.split('&').count()),
    )
}

/// Story 2.16 AC 5: whatever a value holds, the filled URL has the
/// template's scheme, authority, segments and query items; only the
/// placeholders' places change.
#[test]
fn a_value_never_changes_the_scheme_authority_or_shape() {
    let mut filled = 0;
    for case in &load().cases {
        let Expect::Url(url) = replay(case) else {
            continue;
        };
        let template = UrlTemplate::parse(&case.template).unwrap();
        let blank: BTreeMap<String, String> = template
            .placeholders()
            .into_iter()
            .map(|name| (name.to_owned(), "x".to_owned()))
            .collect();
        let reference = template.fill(&blank).unwrap();
        assert_eq!(shape(&url), shape(&reference), "case: {}", case.name);
        filled += 1;
    }
    assert!(filled > 0);
}

#[test]
fn every_rule_has_a_case() {
    let set = load();
    let issues: BTreeSet<String> = set
        .cases
        .iter()
        .filter_map(|case| match replay(case) {
            Expect::Issue(issue) => Some(format!("{issue:?}")),
            _ => None,
        })
        .collect();
    for issue in UrlIssue::ALL {
        assert!(
            issues.contains(&format!("{issue:?}")),
            "no case for the URL issue {issue:?}"
        );
    }

    // Every byte class of the encoder appears in a value that filled a URL.
    let classes: BTreeSet<String> = set
        .cases
        .iter()
        .filter(|case| matches!(replay(case), Expect::Url(_)))
        .flat_map(|case| case.params.values())
        .flat_map(|value| value.bytes())
        .map(|byte| format!("{:?}", byte_class(byte)))
        .collect();
    for class in ByteClass::ALL {
        assert!(
            classes.contains(&format!("{class:?}")),
            "no filled value holds a byte of the class {class:?}"
        );
    }
    assert!(
        set.cases
            .iter()
            .any(|case| matches!(replay(case), Expect::Refusal(_))),
        "no case refuses a value"
    );
    let outcomes: BTreeSet<bool> = set.cases.iter().filter_map(binds).collect();
    assert_eq!(
        outcomes,
        BTreeSet::from([false, true]),
        "the binding needs a case that sends the credential and one that does not"
    );
}
