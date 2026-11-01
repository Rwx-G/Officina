//! A workshop run's record (AD-109, AD-119, AD-124): who runs a work
//! order, the states a run moves through, what it produced, the facts and
//! texts the registry keeps of it, and when each is deleted. Story 6.12
//! adds the transitions and the finish causes; no variant is added after
//! the workshop migration, whose `CHECK` lists read these enums' `ALL`
//! (D-133, D-212).

use serde::{Deserialize, Serialize};

use crate::registry::retention::{
    PASS_INTERVAL_MS, PRIVACY_BOUND_MS, WORK_ORDER_TTL_MS, excerpt_cutoff,
};
use crate::untrusted::Untrusted;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a work order runs (AD-119): on the author's machine (v0.5) or
    /// in the workshop.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Runner {
        #[default]
        Host,
        Workshop,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A workshop run's state (AD-109): live while launching to collecting,
    /// then a package awaiting a verdict, a run with nothing to review, a
    /// run Stop all kept, or a terminal state.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum RunState {
        Launching,
        Running,
        Stopping,
        Collecting,
        Review,
        Quarantined,
        NoChange,
        Stopped,
        Imported,
        Discarded,
        Failed,
        Unknown,
    }
}

impl RunState {
    /// True for a state no transition leaves (AD-109): a `no_change` run
    /// has nothing to review, so it ends there.
    pub const fn is_terminal(self) -> bool {
        match self {
            RunState::NoChange
            | RunState::Imported
            | RunState::Discarded
            | RunState::Failed
            | RunState::Unknown => true,
            RunState::Launching
            | RunState::Running
            | RunState::Stopping
            | RunState::Collecting
            | RunState::Review
            | RunState::Quarantined
            | RunState::Stopped => false,
        }
    }

    /// True while a container of the run may live: it counts against
    /// `max_workers` (AD-107's `workshop_full`).
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            RunState::Launching | RunState::Running | RunState::Stopping | RunState::Collecting
        )
    }

    /// True for a package awaiting the author's verdict (AD-107's
    /// `reviews_pending`, the board's Needs you).
    pub const fn awaits_verdict(self) -> bool {
        matches!(self, RunState::Review | RunState::Quarantined)
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a run produced, decided after collection (AD-109, AD-110).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum WorkshopOutcome {
        Package,
        NoChange,
        Failed,
        NotCollected,
        Unknown,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a run ended (AD-109). Story 6.12 decides it; the column holding
    /// it carries no `CHECK`, since the list grows (D-133).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum FinishCause {
        Exited,
        Failed,
        Deadline,
        Budget,
        GatewayLimit,
        OutputCap,
        UnexpectedTool,
        HookFailed,
        AuthFailed,
        Memory,
        Stopped,
        EngineLost,
    }
}

/// A value of a column without `CHECK`, read back (D-133): a variant this
/// build knows, or one it does not, which is kept as unknown rather than
/// dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded<T> {
    Known(T),
    Unknown,
}

impl<T> Recorded<T> {
    /// `code` as `parse` reads it, or [`Recorded::Unknown`].
    pub fn parse(code: &str, parse: fn(&str) -> Option<T>) -> Recorded<T> {
        parse(code).map_or(Recorded::Unknown, Recorded::Known)
    }
}

/// The version of [`RunFacts`]' JSON in `workshop_run.facts_json`.
pub const RUN_FACTS_VERSION: u32 = 1;

/// The typed facts of a run the registry keeps (AD-119): counts and
/// measures, never a worker's text. Later stories add fields with a
/// default, so a row written before them still reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunFacts {
    pub version: u32,
    /// The engine's clock against the window's at launch (D-242), a
    /// diagnostic only.
    pub clock_skew_ms: Option<i64>,
    /// How many credential locations the scan found (D-212): the count
    /// outlives the texts, so the revoke advice does too.
    pub credential_locations: Option<u32>,
}

impl Default for RunFacts {
    fn default() -> RunFacts {
        RunFacts {
            version: RUN_FACTS_VERSION,
            clock_skew_ms: None,
            credential_locations: None,
        }
    }
}

/// Every untrusted text of a run (AD-119, D-130): what a worker or its
/// gateway wrote. Read back as [`Untrusted`] only, cleared at
/// [`EXCERPT_TTL_MS`](crate::registry::retention::EXCERPT_TTL_MS).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunTexts {
    pub report_tail: Option<Untrusted<String>>,
    pub commit_subjects: Vec<Untrusted<String>>,
    pub paths: Vec<Untrusted<String>>,
    pub refused_hosts: Vec<Untrusted<String>>,
    /// Where the scan found the credential (AD-112): paths, never a value.
    pub credential_locations: Vec<Untrusted<String>>,
    pub memory_proposals: Vec<Untrusted<String>>,
}

/// A run folder (`<home>/workshop/runs/<id>/`) is deleted this long after
/// its launch (AD-124): 30 days less one pass, since run folders are not in
/// the registry's copies and the pass runs once an hour.
pub const RUN_FOLDER_TTL_MS: i64 = PRIVACY_BOUND_MS - PASS_INTERVAL_MS;

/// A run's row, with its `work_order` and approvals, goes at the work
/// orders' 90 days (D-134, AD-124).
pub const RUN_ROW_TTL_MS: i64 = WORK_ORDER_TTL_MS;

/// A battery's row goes at 90 days (AD-124).
pub const BATTERY_ROW_TTL_MS: i64 = WORK_ORDER_TTL_MS;

/// What retention reads of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAge {
    pub id: String,
    pub started_at_ms: i64,
    /// `texts_json` is not cleared yet.
    pub has_texts: bool,
}

/// One thing the hourly pass does for the workshop (AD-124, D-213).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetentionAction {
    /// Clear the run's `texts_json`.
    ClearTexts(String),
    /// Delete the run's folder under `<home>/workshop/runs/`.
    DeleteFolder(String),
    /// Delete the run's row, its `work_order` and its approvals, in one
    /// transaction.
    DeleteRun(String),
}

/// What the hourly pass does at `now_ms` for `rows` (the runs the registry
/// holds) and `folders` (the run folders on disk, by name): texts cleared
/// at the excerpt TTL after launch, folders deleted at
/// [`RUN_FOLDER_TTL_MS`], rows at [`RUN_ROW_TTL_MS`]. A folder the
/// registry holds no row for is deleted: a launch inserts its row before
/// it writes its folder (AD-107), and the row outlives the folder.
pub fn retention(rows: &[RunAge], folders: &[String], now_ms: i64) -> Vec<RetentionAction> {
    let mut actions = Vec::new();
    let texts_cutoff = excerpt_cutoff(now_ms);
    for row in rows {
        if row.started_at_ms <= now_ms - RUN_ROW_TTL_MS {
            actions.push(RetentionAction::DeleteRun(row.id.clone()));
        } else if row.has_texts && row.started_at_ms <= texts_cutoff {
            actions.push(RetentionAction::ClearTexts(row.id.clone()));
        }
    }
    for folder in folders {
        let expired = rows
            .iter()
            .find(|row| &row.id == folder)
            .is_none_or(|row| row.started_at_ms <= now_ms - RUN_FOLDER_TTL_MS);
        if expired {
            actions.push(RetentionAction::DeleteFolder(folder.clone()));
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::retention::{COPY_LIFETIME_MS, EXCERPT_TTL_MS};

    const NOW: i64 = 1_793_102_400_000;

    #[test]
    fn no_change_is_terminal() {
        assert!(RunState::NoChange.is_terminal());
        for state in RunState::ALL {
            assert!(
                !(state.is_terminal() && (state.is_live() || state.awaits_verdict())),
                "{state:?}"
            );
        }
        assert!(
            RunState::ALL
                .iter()
                .any(|s| !s.is_terminal() && !s.is_live() && !s.awaits_verdict()),
            "Stopped is neither live, awaiting nor terminal"
        );
    }

    fn row(id: &str, age_ms: i64, has_texts: bool) -> RunAge {
        RunAge {
            id: id.to_owned(),
            started_at_ms: NOW - age_ms,
            has_texts,
        }
    }

    #[test]
    fn retention_clears_texts_at_the_excerpt_ttl() {
        let rows = [
            row("young", EXCERPT_TTL_MS - 1, true),
            row("due", EXCERPT_TTL_MS, true),
            row("cleared", EXCERPT_TTL_MS, false),
        ];
        assert_eq!(
            retention(&rows, &[], NOW),
            [RetentionAction::ClearTexts("due".into())]
        );
        // D-85's bound: a text cleared one pass late, held by a copy for a
        // copy's lifetime, still goes within 30 days.
        const { assert!(EXCERPT_TTL_MS + COPY_LIFETIME_MS + PASS_INTERVAL_MS <= PRIVACY_BOUND_MS) };
    }

    #[test]
    fn run_folders_go_before_30_days() {
        const { assert!(RUN_FOLDER_TTL_MS + PASS_INTERVAL_MS <= PRIVACY_BOUND_MS) };
        let rows = [
            row("young", RUN_FOLDER_TTL_MS - 1, false),
            row("due", RUN_FOLDER_TTL_MS, false),
        ];
        let folders = ["young".to_owned(), "due".to_owned(), "orphan".to_owned()];
        assert_eq!(
            retention(&rows, &folders, NOW),
            [
                RetentionAction::DeleteFolder("due".into()),
                RetentionAction::DeleteFolder("orphan".into())
            ]
        );
    }

    #[test]
    fn rows_go_at_90_days_with_their_folder() {
        let rows = [row("old", RUN_ROW_TTL_MS, true)];
        assert_eq!(
            retention(&rows, &["old".to_owned()], NOW),
            [
                RetentionAction::DeleteRun("old".into()),
                RetentionAction::DeleteFolder("old".into())
            ]
        );
    }

    #[test]
    fn an_unknown_cause_is_kept_as_unknown() {
        for cause in FinishCause::ALL {
            assert_eq!(
                Recorded::parse(cause.code(), FinishCause::from_code),
                Recorded::Known(*cause)
            );
        }
        assert_eq!(
            Recorded::parse("a_cause_of_a_later_build", FinishCause::from_code),
            Recorded::Unknown
        );
    }

    #[test]
    fn facts_read_back_with_defaults() {
        let facts: RunFacts = serde_json::from_str("{}").unwrap();
        assert_eq!(facts, RunFacts::default());
        assert_eq!(facts.version, RUN_FACTS_VERSION);
    }
}
