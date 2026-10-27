//! The contract canary (FR-12, AD-12): the fields Officina reads from
//! `claude agents --json`, checked against a live sample.

use std::collections::BTreeSet;

/// Fields every entry carries (research notes E1).
pub const AGENT_FIELDS_ALWAYS: [&str; 4] = ["sessionId", "cwd", "kind", "startedAt"];

/// Fields present only in some states: `pid` and `status` while the process
/// lives, `id` and `state` on background sessions, `waitingFor` while waiting.
pub const AGENT_FIELDS_CONDITIONAL: [&str; 5] = ["pid", "status", "id", "state", "waitingFor"];

/// Result of a contract check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractCheck {
    /// Always-present fields absent from at least one entry: the contract broke.
    pub missing: Vec<String>,
    /// Conditional fields never seen in this sample: not a break, just not
    /// exercised yet (no session was waiting, for instance).
    pub not_observed: Vec<String>,
}

impl ContractCheck {
    /// True when no always-present field is missing.
    pub fn held(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Checks a raw `claude agents --json` array. An empty sample proves nothing
/// is missing and observes nothing.
///
/// ```
/// let sample: Vec<serde_json::Value> = serde_json::from_str(
///     r#"[{"sessionId":"s","cwd":"c","kind":"interactive","startedAt":1,"pid":2,"status":"busy"}]"#,
/// ).unwrap();
/// let check = officina_core::contract::check_agents(&sample);
/// assert!(check.held());
/// assert!(check.not_observed.contains(&"waitingFor".to_string()));
/// ```
pub fn check_agents(entries: &[serde_json::Value]) -> ContractCheck {
    let mut missing = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        for field in AGENT_FIELDS_ALWAYS {
            if entry.get(field).is_none() {
                missing.insert(field.to_owned());
            }
        }
        for field in AGENT_FIELDS_CONDITIONAL {
            if entry.get(field).is_some() {
                seen.insert(field);
            }
        }
    }
    ContractCheck {
        missing: missing.into_iter().collect(),
        not_observed: AGENT_FIELDS_CONDITIONAL
            .iter()
            .filter(|f| !seen.contains(*f))
            .map(|f| (*f).to_owned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_vanished_field_breaks_the_contract() {
        let check = check_agents(&[
            json!({"sessionId": "a", "cwd": "c", "kind": "background", "startedAt": 1, "id": "x", "state": "done"}),
            json!({"cwd": "c", "kind": "interactive", "startedAt": 1}),
        ]);
        assert!(!check.held());
        assert_eq!(check.missing, vec!["sessionId"]);
        assert_eq!(check.not_observed, vec!["pid", "status", "waitingFor"]);
    }

    #[test]
    fn empty_sample_holds_and_observes_nothing() {
        let check = check_agents(&[]);
        assert!(check.held());
        assert_eq!(check.not_observed.len(), AGENT_FIELDS_CONDITIONAL.len());
    }
}
