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
    /// The run holds a package still undecided (`review`, `quarantined`).
    pub awaits_verdict: bool,
}

/// One thing the hourly pass does for the workshop (AD-124, D-213).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetentionAction {
    /// Clear the run's `texts_json`.
    ClearTexts(String),
    /// Delete the run's folder under `<home>/workshop/runs/`.
    DeleteFolder(String),
    /// Discard the run's undecided package with reason `expired`, its
    /// folder going in the same pass (D-213).
    Expire(String),
    /// Delete the run's row, its `work_order` and its approvals, in one
    /// transaction.
    DeleteRun(String),
}

/// What the hourly pass does at `now_ms` for `rows` (the runs the registry
/// holds) and `folders` (the run folders on disk, by name): texts cleared
/// at the excerpt TTL after launch, folders deleted at
/// [`RUN_FOLDER_TTL_MS`], rows at [`RUN_ROW_TTL_MS`]. A folder the
/// registry holds no row for is deleted: a launch inserts its row before
/// it writes its folder (AD-107), and the row outlives the folder. A
/// package still undecided at [`RUN_FOLDER_TTL_MS`] expires in the same
/// pass, whether its folder is still on disk or not, so no Import is
/// offered on a folder already gone (D-213, review F39).
pub fn retention(rows: &[RunAge], folders: &[String], now_ms: i64) -> Vec<RetentionAction> {
    let mut actions = Vec::new();
    let texts_cutoff = excerpt_cutoff(now_ms);
    for row in rows {
        if row.awaits_verdict && row.started_at_ms <= now_ms - RUN_FOLDER_TTL_MS {
            actions.push(RetentionAction::Expire(row.id.clone()));
        }
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

/// What looking at a stop flag gave (AD-34, AD-116).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagLookup {
    Missing,
    Present,
    /// Anything but "not found": the flag may be there.
    Unreadable,
}

/// True when the flag stops its consumer (AD-34's predicate, per file,
/// P-50): present, or not readable, so a flag the process cannot look at
/// never lets an action start.
pub const fn flag_set(lookup: FlagLookup) -> bool {
    !matches!(lookup, FlagLookup::Missing)
}

/// The flag a writer writes (AD-116): one when the flag is missing; none
/// when it is present or may be, since a writer leaves a present flag and
/// its first time alone.
pub fn flag_to_write(
    lookup: FlagLookup,
    since_ms: i64,
    by: crate::relay::StopBy,
) -> Option<crate::relay::StopFlag> {
    match lookup {
        FlagLookup::Missing => Some(crate::relay::StopFlag {
            since_ms,
            by: Some(by),
        }),
        FlagLookup::Present | FlagLookup::Unreadable => None,
    }
}

/// How long Stop all may take with the engine answering (AD-116).
pub const STOP_BUDGET_MS: u64 = 10_000;

/// How long a stopped run keeps its containers (AD-116): they hold the
/// credential in their engine configuration until removed.
pub const KEPT_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Which containers a stop reached (AD-116): the window's floor, or
    /// every floor of the engine from a terminal (D-30).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum StopScope {
        Floor,
        All,
    }
}

/// The running managed containers a stop reaches (AD-116): of the floor
/// whose key is given, or of every floor when none is.
pub fn stop_targets(
    rows: &[super::engine::PsRow],
    floor_key: Option<&str>,
) -> Vec<super::names::ObjectName> {
    rows.iter()
        .filter(|row| row.state == "running")
        .filter(|row| managed(&row.labels) && on_floor(&row.labels, floor_key))
        .filter_map(|row| super::names::ObjectName::parse(&row.name))
        .collect()
}

/// The engine objects of one run on one floor (AD-121, D-215): those
/// labelled managed, with the run's id and the floor's key, and nothing
/// else.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunObjects {
    pub containers: Vec<super::names::ObjectName>,
    pub networks: Vec<super::names::ObjectName>,
    pub volumes: Vec<super::names::ObjectName>,
}

impl RunObjects {
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty() && self.networks.is_empty() && self.volumes.is_empty()
    }
}

/// The objects of `run_id` on `floor_key` among the engine's listings.
pub fn run_objects(
    containers: &[super::engine::PsRow],
    networks: &[super::engine::ObjectRow],
    volumes: &[super::engine::ObjectRow],
    run_id: &str,
    floor_key: &str,
) -> RunObjects {
    let ours = |labels: &std::collections::BTreeMap<String, String>| {
        managed(labels)
            && on_floor(labels, Some(floor_key))
            && labels.get(super::names::RUN_LABEL).map(String::as_str) == Some(run_id)
    };
    let names =
        |rows: &mut dyn Iterator<Item = (&String, &std::collections::BTreeMap<String, String>)>| {
            rows.filter(|(_, labels)| ours(labels))
                .filter_map(|(name, _)| super::names::ObjectName::parse(name))
                .collect::<Vec<_>>()
        };
    RunObjects {
        containers: names(&mut containers.iter().map(|r| (&r.name, &r.labels))),
        networks: names(&mut networks.iter().map(|r| (&r.name, &r.labels))),
        volumes: names(&mut volumes.iter().map(|r| (&r.name, &r.labels))),
    }
}

fn managed(labels: &std::collections::BTreeMap<String, String>) -> bool {
    let (key, value) = super::names::MANAGED_LABEL;
    labels.get(key).map(String::as_str) == Some(value)
}

fn on_floor(labels: &std::collections::BTreeMap<String, String>, floor_key: Option<&str>) -> bool {
    floor_key
        .is_none_or(|key| labels.get(super::names::FLOOR_LABEL).map(String::as_str) == Some(key))
}

/// The stopped runs whose kept containers are due for removal at `now_ms`
/// (AD-116, D-213): [`KEPT_TTL_MS`] after `stopped_at_ms`. A run without
/// its stop time is kept: Stop all writes it with the state.
pub fn kept_expired(stopped: &[(String, Option<i64>)], now_ms: i64) -> Vec<String> {
    stopped
        .iter()
        .filter(|(_, at)| at.is_some_and(|at| at <= now_ms - KEPT_TTL_MS))
        .map(|(id, _)| id.clone())
        .collect()
}

/// What Stop all did (AD-116), for the window and its tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StopAnswer {
    /// The containers both passes stopped.
    pub containers: u32,
    /// The runs that became `stopped`.
    pub runs: u32,
    /// False when the engine could not be reached: containers may still
    /// run.
    pub engine_reached: bool,
    /// How long it took, against [`STOP_BUDGET_MS`].
    pub elapsed_ms: u64,
}

impl StopAnswer {
    /// True when Stop all ended within [`STOP_BUDGET_MS`].
    pub const fn within_budget(&self) -> bool {
        self.elapsed_ms <= STOP_BUDGET_MS
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What Resume does with one kept run (AD-116): collect it (the
    /// default) or remove it.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum KeptChoice {
        #[default]
        Collect,
        Remove,
    }
}

/// One kept run and the author's choice for it, as the Resume dialog
/// sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResumeChoice {
    pub run_id: String,
    pub choice: KeptChoice,
}

/// What Resume did (AD-116).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResumeAnswer {
    /// The runs queued for collection.
    pub collected: u32,
    /// The runs queued for removal.
    pub removed: u32,
    /// True when the relay was resumed too.
    pub relay_resumed: bool,
}

/// Each kept run's choice: the author's, else collect (AD-116). A choice
/// naming no kept run is ignored, so a stale dialog never reaches another
/// run.
pub fn resume_plan(kept: &[String], choices: &[ResumeChoice]) -> Vec<(String, KeptChoice)> {
    kept.iter()
        .map(|id| {
            let choice = choices
                .iter()
                .find(|c| &c.run_id == id)
                .map_or(KeptChoice::Collect, |c| c.choice);
            (id.clone(), choice)
        })
        .collect()
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why `remove_run` removed nothing (AD-121, D-215).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RemoveRunRefusal {
        /// Not 32 lowercase hex characters.
        MalformedId,
        /// Neither kept nor an orphan of the window's floor.
        NotRemovable,
        /// No approved floor, so no floor key to act on.
        NoFloor,
        EngineUnavailable,
    }
}

/// What `remove_run` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum RemoveRunAnswer {
    /// The run's objects removed: how many.
    Removed(u32),
    Refused(RemoveRunRefusal),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::retention::{COPY_LIFETIME_MS, EXCERPT_TTL_MS};

    /// Story 6.17 AC 1 (AD-34 as amended, P-50): a flag that cannot be
    /// looked at stops, as a present one does.
    #[test]
    fn an_unreadable_flag_counts_as_set() {
        assert!(flag_set(FlagLookup::Present));
        assert!(flag_set(FlagLookup::Unreadable));
        assert!(!flag_set(FlagLookup::Missing));
    }

    /// Story 6.17 AC 1 (AD-116): a writer creates a missing flag with its
    /// writer and time, and leaves a present one, or one it cannot read.
    #[test]
    fn a_present_flag_is_left_alone() {
        use crate::relay::StopBy;
        for by in StopBy::ALL {
            assert_eq!(
                flag_to_write(FlagLookup::Missing, 7, *by),
                Some(crate::relay::StopFlag {
                    since_ms: 7,
                    by: Some(*by)
                })
            );
            assert_eq!(flag_to_write(FlagLookup::Present, 7, *by), None);
            assert_eq!(flag_to_write(FlagLookup::Unreadable, 7, *by), None);
        }
    }

    /// Story 6.17 AC 5 (AD-116, D-213): kept containers go 7 days after
    /// the stop, not before; a run without a stop time is kept.
    #[test]
    fn kept_containers_expire_after_7_days() {
        let stopped = [
            ("young".to_owned(), Some(NOW - KEPT_TTL_MS + 1)),
            ("due".to_owned(), Some(NOW - KEPT_TTL_MS)),
            ("untimed".to_owned(), None),
        ];
        assert_eq!(kept_expired(&stopped, NOW), ["due".to_owned()]);
    }

    fn ps_row(name: &str, state: &str, labels: &[(&str, &str)]) -> super::super::engine::PsRow {
        super::super::engine::PsRow {
            id: format!("id-{name}"),
            name: name.to_owned(),
            state: state.to_owned(),
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    /// AD-116: the window stops its floor's running containers; the CLI
    /// every floor's; an unmanaged or exited container is never named.
    #[test]
    fn stop_targets_are_running_managed_containers_of_the_floor() {
        let rows = [
            ps_row(
                "officina-worker-a",
                "running",
                &[("officina.managed", "1"), ("officina.floor", "f1")],
            ),
            ps_row(
                "officina-worker-b",
                "running",
                &[("officina.managed", "1"), ("officina.floor", "f2")],
            ),
            ps_row(
                "officina-worker-c",
                "exited",
                &[("officina.managed", "1"), ("officina.floor", "f1")],
            ),
            ps_row("someone-else", "running", &[("officina.floor", "f1")]),
        ];
        let names = |found: Vec<super::super::names::ObjectName>| {
            found
                .iter()
                .map(|n| n.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(stop_targets(&rows, Some("f1"))),
            ["officina-worker-a"]
        );
        assert_eq!(
            names(stop_targets(&rows, None)),
            ["officina-worker-a", "officina-worker-b"]
        );
    }

    /// AD-121, D-215: a run's objects are those with its id and the
    /// floor's key, never another floor's or another run's.
    #[test]
    fn run_objects_are_labelled_with_the_run_and_the_floor() {
        let run = "0123456789abcdef0123456789abcdef";
        let mine = [
            ("officina.managed", "1"),
            ("officina.floor", "f1"),
            ("officina.run", run),
        ];
        let other_floor = [
            ("officina.managed", "1"),
            ("officina.floor", "f2"),
            ("officina.run", run),
        ];
        let rows = [
            ps_row("officina-worker-mine", "exited", &mine),
            ps_row("officina-worker-theirs", "exited", &other_floor),
        ];
        let object = |name: &str, labels: &[(&str, &str)]| super::super::engine::ObjectRow {
            name: name.to_owned(),
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        let networks = [
            object("officina-net-mine", &mine),
            object("officina-net-theirs", &other_floor),
        ];
        let volumes = [object("officina-work-mine", &mine)];
        let found = run_objects(&rows, &networks, &volumes, run, "f1");
        assert_eq!(found.containers.len(), 1);
        assert_eq!(found.containers[0].as_str(), "officina-worker-mine");
        assert_eq!(found.networks.len(), 1);
        assert_eq!(found.volumes.len(), 1);
        assert!(run_objects(&rows, &networks, &volumes, run, "f3").is_empty());
    }

    /// AD-116: a kept run the dialog does not name is collected; a choice
    /// for a run that is not kept is ignored.
    #[test]
    fn resume_collects_unless_told_otherwise() {
        let kept = ["a".to_owned(), "b".to_owned()];
        let choices = [
            ResumeChoice {
                run_id: "b".into(),
                choice: KeptChoice::Remove,
            },
            ResumeChoice {
                run_id: "z".into(),
                choice: KeptChoice::Remove,
            },
        ];
        assert_eq!(
            resume_plan(&kept, &choices),
            [
                ("a".to_owned(), KeptChoice::Collect),
                ("b".to_owned(), KeptChoice::Remove)
            ]
        );
    }

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
            awaits_verdict: false,
        }
    }

    fn undecided(id: &str, age_ms: i64) -> RunAge {
        RunAge {
            awaits_verdict: true,
            ..row(id, age_ms, false)
        }
    }

    /// D-213, review F39: an undecided package expires in the pass that
    /// deletes its folder, never before; a decided one is left alone.
    #[test]
    fn an_undecided_package_expires_with_its_folder() {
        let rows = [
            undecided("young", RUN_FOLDER_TTL_MS - 1),
            undecided("due", RUN_FOLDER_TTL_MS),
            row("decided", RUN_FOLDER_TTL_MS, false),
        ];
        let folders = ["young".to_owned(), "due".to_owned(), "decided".to_owned()];
        assert_eq!(
            retention(&rows, &folders, NOW),
            [
                RetentionAction::Expire("due".into()),
                RetentionAction::DeleteFolder("due".into()),
                RetentionAction::DeleteFolder("decided".into()),
            ]
        );
        // A folder already gone (a credential found, a failed deletion
        // retried) does not keep the package offered.
        assert_eq!(
            retention(&rows[1..2], &[], NOW),
            [RetentionAction::Expire("due".into())]
        );
    }

    /// D-213: across a range of ages, every pass that deletes an undecided
    /// package's folder expires it, and every expiry is at or past the
    /// folder's age.
    #[test]
    fn no_undecided_package_outlives_its_folder() {
        let step = PASS_INTERVAL_MS / 2;
        let mut age = RUN_FOLDER_TTL_MS - 4 * PASS_INTERVAL_MS;
        while age <= RUN_ROW_TTL_MS + PASS_INTERVAL_MS {
            let rows = [undecided("p", age)];
            let actions = retention(&rows, &["p".to_owned()], NOW);
            let deleted = actions.contains(&RetentionAction::DeleteFolder("p".into()));
            let expired = actions.contains(&RetentionAction::Expire("p".into()));
            assert_eq!(deleted, expired, "age {age}");
            assert_eq!(expired, age >= RUN_FOLDER_TTL_MS, "age {age}");
            age += step;
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
