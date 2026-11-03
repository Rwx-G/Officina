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
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    // ts-rs writes one file per name: the relay's `RunState` and
    // `Runner` hold theirs (D-1107's lesson, D-1150).
    #[cfg_attr(feature = "ts", ts(rename = "OrderRunner"))]
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
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    // ts-rs writes one file per name: the relay's `RunState` and
    // `Runner` hold theirs (D-1107's lesson, D-1150).
    #[cfg_attr(feature = "ts", ts(rename = "WorkshopRunState"))]
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
    /// One step of a launch (AD-107 steps 1 to 4), as `workshop_started`
    /// records it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum LaunchStep {
        /// The `work_order` and `workshop_run` rows.
        Record,
        /// `git bundle create` and its header read back.
        Bundle,
        /// `prompt.txt` and `plan.json`.
        Files,
        /// Both networks.
        Networks,
        Volume,
        /// The gateway's create and its connection to the internal network.
        Gateway,
        /// The worker's create, with the credential.
        Worker,
        /// `in/` into the worker's volume.
        Copy,
        /// The gateway's start, then the worker's.
        Start,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How one launch step went.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum StepOutcome {
        Ok,
        Failed,
        /// The workshop's stop flag was set before it.
        Stopped,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a run's engine objects were removed (AD-120).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum RemovalCause {
        Collected,
        KeptExpired,
        Resume,
        LaunchFailed,
        Orphan,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a run produced, decided after collection (AD-109, AD-110).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A step of `officina-entry run` (AD-108).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum EntryStep {
        Plan,
        Clone,
        Branch,
        Head,
        Mounts,
        Memory,
        Login,
        LoginMissing,
        Claude,
        Exit,
    }
}

/// The longest status line the entry point prints and the window keeps
/// (D-216).
pub const MAX_ENTRY_LINE_BYTES: usize = 4 * 1024;

/// One status line of the entry point on stderr (AD-108): a step, whether
/// it went well, a detail, and at the end `claude`'s exit code. Worker
/// text: any process of the container could print one (AD-109).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryLine {
    pub step: EntryStep,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl EntryLine {
    pub fn ok(step: EntryStep) -> EntryLine {
        EntryLine {
            step,
            ok: true,
            detail: None,
            exit_code: None,
        }
    }

    pub fn failed(step: EntryStep, detail: impl Into<String>) -> EntryLine {
        EntryLine {
            step,
            ok: false,
            detail: Some(detail.into()),
            exit_code: None,
        }
    }

    /// The last line: `claude`'s exit code.
    pub fn exited(code: i32) -> EntryLine {
        EntryLine {
            step: EntryStep::Exit,
            ok: code == 0,
            detail: None,
            exit_code: Some(code),
        }
    }

    /// The line as printed, within [`MAX_ENTRY_LINE_BYTES`]: a detail that
    /// would pass it is cut on a character boundary.
    pub fn to_line(&self) -> String {
        let mut line = self.clone();
        loop {
            let text = serde_json::to_string(&line).unwrap_or_default();
            if text.len() <= MAX_ENTRY_LINE_BYTES {
                return text;
            }
            let Some(detail) = line.detail.as_mut() else {
                return text;
            };
            let keep = detail
                .len()
                .saturating_sub(text.len() - MAX_ENTRY_LINE_BYTES + 3);
            let mut cut = keep.min(detail.len());
            while cut > 0 && !detail.is_char_boundary(cut) {
                cut -= 1;
            }
            detail.truncate(cut);
            if cut == 0 {
                line.detail = None;
            }
        }
    }

    /// A line the window read, `None` past [`MAX_ENTRY_LINE_BYTES`] or when
    /// it is not one: counted and skipped (AD-109).
    pub fn parse(line: &str) -> Option<EntryLine> {
        if line.len() > MAX_ENTRY_LINE_BYTES {
            return None;
        }
        serde_json::from_str(line).ok()
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a collection stopped before host git had anything to check
    /// (AD-110's `collect_failed` row).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum CollectFailure {
        /// The packer's create or attached start failed, or passed 120 s.
        Packer,
        /// `docker cp` failed or passed its deadline.
        Copy,
        /// The host could not write the bundle or run its own git.
        Host,
    }
}

/// One gateway refusal as the run's texts keep it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunRefusal {
    pub host: Untrusted<String>,
    pub port: u16,
    pub cause: super::gateway::GatewayRefusal,
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
    /// Why the window stopped the worker (AD-109), when it did.
    pub stop_reason: Option<super::lifecycle::StopReason>,
    /// How many of the gateway's refusals are journaled (AD-120): a
    /// follower that reads again from the start journals only the ones
    /// past it, so each refusal is journaled once (AD-109).
    pub refusals_journaled: u32,
    /// What the run's output told when it was collected (AD-120's
    /// `workshop_finished` and `gateway_summary`), kept so the review view
    /// reads it after a window restart.
    pub cost_usd_micros: Option<u64>,
    pub turns: Option<u64>,
    pub hooks: std::collections::BTreeMap<String, super::stream::HookCounts>,
    pub refused: u32,
    pub stream_bytes: u64,
    pub gateway_exit: Option<i64>,
    /// The gateway's upstreams at start: unreachable when any was.
    pub upstream: Option<super::gateway::UpstreamState>,
    /// Host git's `--shortstat` of the package (AD-120 `review_package`).
    pub insertions: Option<u32>,
    pub deletions: Option<u32>,
    /// Why collection gave nothing to check (AD-110 `collect_failed`).
    pub collect_failed: Option<CollectFailure>,
}

impl Default for RunFacts {
    fn default() -> RunFacts {
        RunFacts {
            version: RUN_FACTS_VERSION,
            clock_skew_ms: None,
            credential_locations: None,
            stop_reason: None,
            refusals_journaled: 0,
            cost_usd_micros: None,
            turns: None,
            hooks: std::collections::BTreeMap::new(),
            refused: 0,
            stream_bytes: 0,
            gateway_exit: None,
            upstream: None,
            insertions: None,
            deletions: None,
            collect_failed: None,
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
    /// The gateway's refusals, their hosts replaced (D-230), bounded.
    pub refusals: Vec<RunRefusal>,
    /// The hosts the gateway allowed, with their counts, replaced.
    pub allowed_hosts: Vec<(Untrusted<String>, u32)>,
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
    fn entry_lines_round_trip() {
        for line in [
            EntryLine::ok(EntryStep::Clone),
            EntryLine::failed(EntryStep::LoginMissing, ".credentials.json is missing"),
            EntryLine::exited(7),
        ] {
            assert_eq!(EntryLine::parse(&line.to_line()), Some(line));
        }
    }

    #[test]
    fn an_oversized_entry_line_is_counted_not_kept() {
        let long = EntryLine::failed(EntryStep::Claude, "é".repeat(MAX_ENTRY_LINE_BYTES));
        let text = long.to_line();
        assert!(text.len() <= MAX_ENTRY_LINE_BYTES);
        assert!(EntryLine::parse(&text).is_some());
        let forged = format!(
            "{{\"step\":\"exit\",\"ok\":true,\"detail\":\"{}\"}}",
            "x".repeat(MAX_ENTRY_LINE_BYTES)
        );
        assert_eq!(EntryLine::parse(&forged), None);
        assert_eq!(EntryLine::parse("not json"), None);
    }

    #[test]
    fn facts_read_back_with_defaults() {
        let facts: RunFacts = serde_json::from_str("{}").unwrap();
        assert_eq!(facts, RunFacts::default());
        assert_eq!(facts.version, RUN_FACTS_VERSION);
    }
}
