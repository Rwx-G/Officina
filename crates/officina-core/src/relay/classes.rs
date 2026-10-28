//! Action classes (FR-8, AD-33): which classes this build can run, and the
//! check rule an `after-check` action, and a `human` action naming its dry
//! run (D-170), must meet. `free`, `after-check` and `human` run, the last
//! once per approval (`relay::approvals`); `auto` is reserved for the
//! orchestrator and marked unavailable wherever actions are offered (D-17).

use std::collections::{BTreeMap, HashSet};
use std::ops::RangeInclusive;

use super::catalogue::ActionClass;
use super::journal::{JournalEntry, JournalLine, RunOutcome, RunStarted, TreeFingerprint};

/// How long a successful check lets its `after-check` action run (AD-33).
pub const CHECK_VALID_MS: i64 = 30 * 60 * 1000;

/// Accepted `officina run --wait <s>` for a `human` action (AD-33): how long
/// a call waits for the author's decision before it gives up.
pub const WAIT_RANGE_S: RangeInclusive<u32> = 1..=540;

/// True when this build runs actions of `class`.
///
/// ```
/// use officina_core::relay::catalogue::ActionClass;
/// use officina_core::relay::classes::is_available;
/// assert!(is_available(ActionClass::Human));
/// assert!(!is_available(ActionClass::Auto));
/// ```
pub fn is_available(class: ActionClass) -> bool {
    match class {
        ActionClass::Free | ActionClass::AfterCheck | ActionClass::Human => true,
        ActionClass::Auto => false,
    }
}

/// The class as the floor writes it, for messages.
pub fn class_name(class: ActionClass) -> String {
    serde_json::to_value(class)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The `after-check` rule (AD-33, P-3): true only when the journal holds a
/// `run_started` of `check_action` in `project`, with exactly `params`,
/// under `catalogue_hash` and on the same `tree` (both absent outside git),
/// started at most [`CHECK_VALID_MS`] before `now_ms`, whose `run_finished`
/// says it exited with code 0. `lines` are the two days the host reads, so a
/// check just before midnight UTC counts just after it. A check still
/// running, timed out, failed or started in the future does not count.
pub fn check_satisfied(
    lines: &[JournalLine],
    project: &str,
    check_action: &str,
    params: &BTreeMap<String, String>,
    catalogue_hash: &str,
    tree: Option<&TreeFingerprint>,
    now_ms: i64,
) -> bool {
    satisfying_check(
        lines,
        project,
        check_action,
        params,
        catalogue_hash,
        tree,
        now_ms,
    )
    .is_some()
}

/// The latest check run that satisfies [`check_satisfied`], whose run id a
/// `human` action's request records (D-170).
pub fn satisfying_check<'l>(
    lines: &'l [JournalLine],
    project: &str,
    check_action: &str,
    params: &BTreeMap<String, String>,
    catalogue_hash: &str,
    tree: Option<&TreeFingerprint>,
    now_ms: i64,
) -> Option<&'l RunStarted> {
    let succeeded: HashSet<&str> = lines
        .iter()
        .filter_map(|line| match &line.entry {
            JournalEntry::RunFinished(finished)
                if finished.outcome == RunOutcome::Exited && finished.exit_code == Some(0) =>
            {
                Some(finished.run_id.as_str())
            }
            _ => None,
        })
        .collect();
    lines
        .iter()
        .filter_map(|line| match &line.entry {
            JournalEntry::RunStarted(started)
                if started.project == project
                    && started.action == check_action
                    && started.params == *params
                    && started.catalogue_hash == catalogue_hash
                    && started.tree.as_ref() == tree
                    && (0..=CHECK_VALID_MS).contains(&(now_ms - line.ts))
                    && succeeded.contains(started.run_id.as_str()) =>
            {
                Some((line.ts, started))
            }
            _ => None,
        })
        .max_by_key(|(ts, _)| *ts)
        .map(|(_, started)| started)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::journal::{RunFinished, RunStarted, StreamSummary};

    const NOW: i64 = 1_793_102_400_000;
    const MIN: i64 = 60_000;
    const HASH: &str = "0123";

    fn params(group: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("group".to_owned(), group.to_owned())])
    }

    fn tree(text: &str) -> TreeFingerprint {
        TreeFingerprint {
            sha256: crate::relay::sha256_hex(text.as_bytes()),
            files: 0,
            git: None,
        }
    }

    fn started(ts: i64, run_id: &str, project: &str, group: &str, hash: &str) -> JournalLine {
        JournalLine::new(
            ts,
            JournalEntry::RunStarted(RunStarted {
                run_id: run_id.to_owned(),
                project: project.to_owned(),
                session_id: None,
                action: "verify".to_owned(),
                class: ActionClass::Free,
                params: params(group),
                argv: vec!["C:/path/to/tool.exe".to_owned(), group.to_owned()],
                method: None,
                url: None,
                credential: None,
                catalogue_hash: hash.to_owned(),
                approval_id: None,
                tree: Some(tree("a")),
                timeout_s: Some(540),
            }),
        )
    }

    fn finished(ts: i64, run_id: &str, outcome: RunOutcome, code: Option<i64>) -> JournalLine {
        let stream = StreamSummary {
            bytes: 0,
            sha256: crate::relay::sha256_hex(b""),
        };
        JournalLine::new(
            ts,
            JournalEntry::RunFinished(RunFinished {
                run_id: run_id.to_owned(),
                outcome,
                exit_code: code,
                duration_ms: 10,
                stdout: Some(stream.clone()),
                stderr: Some(stream),
                dropped_bytes: 0,
                status: None,
                body_bytes: None,
                body_sha256: None,
                detail: None,
            }),
        )
    }

    /// A check of `lab` with `group=lab-web` on tree `a`, started `ago`
    /// before [`NOW`] and finished one second later with `outcome`.
    fn check(ago: i64, outcome: RunOutcome, code: Option<i64>) -> Vec<JournalLine> {
        vec![
            started(NOW - ago, "r1", "lab", "lab-web", HASH),
            finished(NOW - ago + 1000, "r1", outcome, code),
        ]
    }

    fn satisfied(lines: &[JournalLine]) -> bool {
        check_satisfied(
            lines,
            "lab",
            "verify",
            &params("lab-web"),
            HASH,
            Some(&tree("a")),
            NOW,
        )
    }

    #[test]
    fn matching_successful_check_satisfies() {
        assert!(satisfied(&check(5 * MIN, RunOutcome::Exited, Some(0))));
    }

    #[test]
    fn failed_check_does_not() {
        assert!(!satisfied(&check(5 * MIN, RunOutcome::Exited, Some(1))));
        assert!(!satisfied(&check(5 * MIN, RunOutcome::Timeout, None)));
        assert!(!satisfied(&check(5 * MIN, RunOutcome::SpawnFailed, None)));
        assert!(!satisfied(&check(5 * MIN, RunOutcome::Exited, None)));
    }

    #[test]
    fn other_params_do_not() {
        let lines = check(5 * MIN, RunOutcome::Exited, Some(0));
        for other in [
            params("lab-db"),
            BTreeMap::new(),
            BTreeMap::from([
                ("group".to_owned(), "lab-web".to_owned()),
                ("extra".to_owned(), "x".to_owned()),
            ]),
        ] {
            assert!(!check_satisfied(
                &lines,
                "lab",
                "verify",
                &other,
                HASH,
                Some(&tree("a")),
                NOW
            ));
        }
    }

    #[test]
    fn other_project_does_not() {
        let lines = check(5 * MIN, RunOutcome::Exited, Some(0));
        assert!(!check_satisfied(
            &lines,
            "notes",
            "verify",
            &params("lab-web"),
            HASH,
            Some(&tree("a")),
            NOW
        ));
        assert!(
            !check_satisfied(
                &lines,
                "lab",
                "probe",
                &params("lab-web"),
                HASH,
                Some(&tree("a")),
                NOW
            ),
            "another action is not the check"
        );
    }

    #[test]
    fn other_catalogue_hash_does_not() {
        let lines = check(5 * MIN, RunOutcome::Exited, Some(0));
        assert!(!check_satisfied(
            &lines,
            "lab",
            "verify",
            &params("lab-web"),
            "4567",
            Some(&tree("a")),
            NOW
        ));
    }

    #[test]
    fn other_tree_does_not() {
        let lines = check(5 * MIN, RunOutcome::Exited, Some(0));
        for other in [Some(&tree("b")), None] {
            assert!(!check_satisfied(
                &lines,
                "lab",
                "verify",
                &params("lab-web"),
                HASH,
                other,
                NOW
            ));
        }
        let other_git = TreeFingerprint {
            git: Some("C:/path/to/other/git.exe".to_owned()),
            ..tree("a")
        };
        assert!(!check_satisfied(
            &lines,
            "lab",
            "verify",
            &params("lab-web"),
            HASH,
            Some(&other_git),
            NOW
        ));
        let mut outside_git = lines.clone();
        if let JournalEntry::RunStarted(started) = &mut outside_git[0].entry {
            started.tree = None;
        }
        assert!(
            check_satisfied(
                &outside_git,
                "lab",
                "verify",
                &params("lab-web"),
                HASH,
                None,
                NOW
            ),
            "outside git only the parameters bind"
        );
    }

    #[test]
    fn check_older_than_30_min_does_not() {
        assert!(satisfied(&check(
            CHECK_VALID_MS,
            RunOutcome::Exited,
            Some(0)
        )));
        assert!(!satisfied(&check(
            CHECK_VALID_MS + 1,
            RunOutcome::Exited,
            Some(0)
        )));
        assert!(
            !satisfied(&check(-MIN, RunOutcome::Exited, Some(0))),
            "a check dated in the future does not count"
        );
    }

    #[test]
    fn started_without_finished_does_not() {
        let mut lines = check(5 * MIN, RunOutcome::Exited, Some(0));
        lines.pop();
        assert!(!satisfied(&lines));
        lines.push(finished(NOW - MIN, "r2", RunOutcome::Exited, Some(0)));
        assert!(!satisfied(&lines), "another run's end does not pair");
    }

    #[test]
    fn check_before_midnight_counts_after_midnight() {
        // A UTC midnight: the check at 23:50 the day before, the apply at
        // 00:10.
        const MIDNIGHT: i64 = 1_793_059_200_000;
        assert_eq!(MIDNIGHT % 86_400_000, 0);
        let lines = vec![
            started(MIDNIGHT - 10 * MIN, "r1", "lab", "lab-web", HASH),
            finished(MIDNIGHT - 9 * MIN, "r1", RunOutcome::Exited, Some(0)),
        ];
        assert!(check_satisfied(
            &lines,
            "lab",
            "verify",
            &params("lab-web"),
            HASH,
            Some(&tree("a")),
            MIDNIGHT + 10 * MIN
        ));
        assert_ne!(
            crate::dates::event_file_name(MIDNIGHT - 10 * MIN),
            crate::dates::event_file_name(MIDNIGHT + 10 * MIN),
            "the check and the apply fall in two journal files"
        );
    }

    #[test]
    fn every_class_but_auto_runs() {
        for class in [
            ActionClass::Free,
            ActionClass::AfterCheck,
            ActionClass::Human,
        ] {
            assert!(is_available(class), "{class:?}");
        }
        assert!(!is_available(ActionClass::Auto));
        assert_eq!(class_name(ActionClass::AfterCheck), "after-check");
    }

    #[test]
    fn the_latest_satisfying_check_is_named() {
        let lines = [
            started(NOW - 20 * MIN, "early", "lab", "lab-web", HASH),
            finished(NOW - 19 * MIN, "early", RunOutcome::Exited, Some(0)),
            started(NOW - 5 * MIN, "late", "lab", "lab-web", HASH),
            finished(NOW - 4 * MIN, "late", RunOutcome::Exited, Some(0)),
            started(NOW - MIN, "failed", "lab", "lab-web", HASH),
            finished(NOW - MIN, "failed", RunOutcome::Exited, Some(2)),
        ];
        let found = satisfying_check(
            &lines,
            "lab",
            "verify",
            &params("lab-web"),
            HASH,
            Some(&tree("a")),
            NOW,
        );
        assert_eq!(found.map(|s| s.run_id.as_str()), Some("late"));
    }
}
