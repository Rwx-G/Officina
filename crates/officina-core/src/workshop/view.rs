//! The workshop's payloads (AD-121): the board's workshop cards, FR-11's
//! group notice, the per-template table (FR-12), the section 7 metrics the
//! view shows, and `WorkshopView` itself.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::package::PackageState;
use super::review::DiscardReason;
use super::run::{RunState, WorkshopOutcome};
use super::sensitive::SensitiveGroup;
use crate::board::{
    Badge, Card, Column, DONE_WINDOW_MS, ProjectRule, ProjectView, SessionKindView, derived_color,
};
use crate::relay::journal::JournalEntry;
use crate::untrusted::Untrusted;

/// The prefix of a workshop card's key: `ws:<run id>` (AD-121).
pub const CARD_KEY_PREFIX: &str = "ws:";

/// What the board reads of one run (AD-121): its registry row, without
/// facts or texts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardRun {
    pub id: String,
    pub template: String,
    pub project: String,
    /// The work order's name, `<template>-<seq>`: Officina's, carried as
    /// untrusted on the card only because `Card.name` is a session's too.
    pub name: String,
    pub state: RunState,
    /// When the run entered `state`.
    pub state_at_ms: i64,
    pub started_at_ms: i64,
}

/// Where a run in `state` sits on the board, and its badge (AD-121): a
/// live run works, a package awaiting a verdict needs the author, a kept
/// run waits to be resumed, the rest are done. A `no_change` run is never
/// Needs you.
pub const fn placement(state: RunState) -> (Column, Option<Badge>) {
    match state {
        RunState::Launching | RunState::Running | RunState::Stopping | RunState::Collecting => {
            (Column::Working, None)
        }
        RunState::Review | RunState::Quarantined => (Column::NeedsYou, None),
        RunState::Stopped => (Column::ToResume, None),
        RunState::NoChange => (Column::Done, Some(Badge::NoChange)),
        RunState::Failed => (Column::Done, Some(Badge::Interrupted)),
        RunState::Unknown => (Column::Done, Some(Badge::Unknown)),
        RunState::Imported | RunState::Discarded => (Column::Done, None),
    }
}

/// What a Needs you workshop card waits for (AD-121's "review" chip).
pub const WAITING_FOR_REVIEW: &str = "review";

/// The board's project of a run: the floor's own when it declares one of
/// that name, as a session card inside it would show it, else a derived
/// colour.
fn project_view(name: &str, projects: &[ProjectRule]) -> ProjectView {
    projects
        .iter()
        .find(|p| p.name == name)
        .or_else(|| projects.iter().find(|p| p.name.eq_ignore_ascii_case(name)))
        .map_or_else(
            || ProjectView {
                name: name.to_owned(),
                color: Some(derived_color(name)),
                priority: crate::floor::Priority::default(),
            },
            ProjectRule::view,
        )
}

/// The board cards of `runs` at `now_ms` (AD-121): kind `workshop`, key
/// `ws:<run id>`, placed by [`placement`]; a done run is listed for
/// [`DONE_WINDOW_MS`] after it entered its state, a kept run until it is
/// resumed or removed. No card has a scene before story 6.23 (D-619).
pub fn workshop_cards(runs: &[BoardRun], projects: &[ProjectRule], now_ms: i64) -> Vec<Card> {
    runs.iter()
        .filter_map(|run| {
            let (column, badge) = placement(run.state);
            if column == Column::Done && now_ms - run.state_at_ms > DONE_WINDOW_MS {
                return None;
            }
            Some(Card {
                key: format!("{CARD_KEY_PREFIX}{}", run.id),
                column,
                badge,
                kind: Some(SessionKindView::Workshop),
                name: Untrusted::new(run.name.clone()),
                project: project_view(&run.project, projects),
                session_id: None,
                background_id: None,
                pid: None,
                cwd: None,
                waiting_for: (column == Column::NeedsYou).then(|| WAITING_FOR_REVIEW.to_owned()),
                last_prompt: None,
                last_message: None,
                last_message_tail: None,
                last_event_at_ms: Some(run.state_at_ms),
                started_at_ms: Some(run.started_at_ms),
                attach_available: false,
                subject: None,
                scene: None,
                settled: None,
                subagents: Vec::new(),
                run_state: Some(run.state),
            })
        })
        .collect()
}

/// Verdicts a template needs before its notice can show (FR-11).
pub const NOTICE_MIN_VERDICTS: usize = 10;

/// One package of a template, as the journal tells it: the families of
/// its groups (`review_package.sensitive`) and whether the author decided
/// it (Import, or Discard with one of his reasons, D-222).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePackage {
    pub template: String,
    pub sensitive: Vec<SensitiveGroup>,
    pub decided: bool,
}

/// FR-11's notice (AD-113): `group` appeared in more than half of the
/// `verdicts` packages the author decided on `template`, so he may keep
/// the group or narrow the template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroupNotice {
    pub template: String,
    pub group: SensitiveGroup,
    pub packages: u32,
    pub verdicts: u32,
}

/// The notices of `packages` (AD-113, FR-11): a template with at least
/// [`NOTICE_MIN_VERDICTS`] decided packages gets one per family present in
/// more than half of them. `file` is left out: `review = "all"` puts it in
/// every package by the author's own choice, so it says nothing about the
/// template.
pub fn group_notices(packages: &[TemplatePackage]) -> Vec<GroupNotice> {
    let mut by_template: std::collections::BTreeMap<&str, Vec<&TemplatePackage>> =
        std::collections::BTreeMap::new();
    for package in packages.iter().filter(|p| p.decided) {
        by_template
            .entry(package.template.as_str())
            .or_default()
            .push(package);
    }
    let mut notices = Vec::new();
    for (template, decided) in by_template {
        if decided.len() < NOTICE_MIN_VERDICTS {
            continue;
        }
        let verdicts = u32::try_from(decided.len()).unwrap_or(u32::MAX);
        for group in SensitiveGroup::ALL
            .iter()
            .filter(|g| **g != SensitiveGroup::File)
        {
            let count = decided
                .iter()
                .filter(|p| p.sensitive.contains(group))
                .count();
            if count * 2 > decided.len() {
                notices.push(GroupNotice {
                    template: template.to_owned(),
                    group: *group,
                    packages: u32::try_from(count).unwrap_or(u32::MAX),
                    verdicts,
                });
            }
        }
    }
    notices
}

/// How many of a template's packages were discarded with one reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiscardCount {
    pub reason: DiscardReason,
    pub count: u32,
}

/// One template's row of the per-template table (AD-114, FR-12 as
/// amended, D-222): its runs and what became of them. Acceptance is
/// `imported` over `decided`, the author's verdicts only: a `no_change` or
/// failed run, and a package `expired` or `not_collected`, is no verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TemplateRow {
    pub template: String,
    /// Runs whose outcome is known (`workshop_finished`).
    pub runs: u32,
    pub no_change: u32,
    pub failed: u32,
    /// Packages collection quarantined (`review_package`).
    pub quarantined: u32,
    pub imported: u32,
    /// Discarded packages per reason, every reason listed.
    pub discarded: Vec<DiscardCount>,
    /// Import and the author's three reasons: Acceptance's denominator.
    pub decided: u32,
    /// FR-11's notices of this template.
    pub notices: Vec<GroupNotice>,
}

/// What the journal says of one run, joined by its id.
#[derive(Default)]
struct RunSeen {
    template: Option<String>,
    outcome: Option<WorkshopOutcome>,
    package: Option<PackageState>,
    sensitive: Vec<SensitiveGroup>,
    imported: bool,
    discarded: Option<DiscardReason>,
}

/// The per-template table of `lines` (AD-114, D-222): a run belongs to the
/// template its `workshop_finished` line names, and a verdict line of a
/// run with no such line is not counted. Templates in name order.
pub fn template_table<'a>(lines: impl IntoIterator<Item = &'a JournalEntry>) -> Vec<TemplateRow> {
    let mut runs: BTreeMap<&str, RunSeen> = BTreeMap::new();
    for line in lines {
        match line {
            JournalEntry::WorkshopFinished(l) => {
                let seen = runs.entry(&l.run_id).or_default();
                seen.template = Some(l.template.clone());
                seen.outcome = Some(l.outcome);
            }
            JournalEntry::ReviewPackage(l) => {
                let seen = runs.entry(&l.run_id).or_default();
                seen.package = Some(l.outcome);
                seen.sensitive.clone_from(&l.sensitive);
            }
            JournalEntry::ReviewImported(l) => runs.entry(&l.run_id).or_default().imported = true,
            JournalEntry::ReviewDiscarded(l) => {
                runs.entry(&l.run_id).or_default().discarded = Some(l.reason);
            }
            _ => {}
        }
    }
    let mut rows: BTreeMap<String, TemplateRow> = BTreeMap::new();
    let mut packages = Vec::new();
    let count = |n: &mut u32| *n = n.saturating_add(1);
    for seen in runs.into_values() {
        let Some(template) = seen.template else {
            continue;
        };
        let row = rows.entry(template.clone()).or_insert_with(|| TemplateRow {
            template: template.clone(),
            runs: 0,
            no_change: 0,
            failed: 0,
            quarantined: 0,
            imported: 0,
            discarded: DiscardReason::ALL
                .iter()
                .map(|reason| DiscardCount {
                    reason: *reason,
                    count: 0,
                })
                .collect(),
            decided: 0,
            notices: Vec::new(),
        });
        count(&mut row.runs);
        match seen.outcome {
            Some(WorkshopOutcome::NoChange) => count(&mut row.no_change),
            Some(WorkshopOutcome::Failed) => count(&mut row.failed),
            _ => {}
        }
        if seen.package == Some(PackageState::Quarantined) {
            count(&mut row.quarantined);
        }
        let decided = if seen.imported {
            count(&mut row.imported);
            true
        } else if let Some(reason) = seen.discarded {
            if let Some(slot) = row.discarded.iter_mut().find(|d| d.reason == reason) {
                count(&mut slot.count);
            }
            reason.is_authors()
        } else {
            false
        };
        if decided {
            count(&mut row.decided);
        }
        if seen.package.is_some() {
            packages.push(TemplatePackage {
                template,
                sensitive: seen.sensitive,
                decided,
            });
        }
    }
    for notice in group_notices(&packages) {
        if let Some(row) = rows.get_mut(&notice.template) {
            row.notices.push(notice);
        }
    }
    rows.into_values().collect()
}

/// Version of the `WorkshopView` contract (AD-121, P-53).
pub const WORKSHOP_VIEW_SCHEMA: u32 = 1;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The engine as the workshop last read it (AD-98).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum EngineStatus {
        /// No `docker.exe` resolved, or a shim.
        Unresolved,
        /// `docker.exe` gave no answer in time.
        Unreachable,
        /// The engine answered and cannot run the workshop.
        Unavailable,
        Ready,
    }
}

/// The engine's state and key (AD-98, D-232): `key` while it is ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EngineView {
    pub status: EngineStatus,
    /// Why it is not ready, Officina's own words or the engine's refusal
    /// code.
    pub detail: Option<String>,
    pub key: Option<String>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where an image stands (AD-121): what Build images would do to it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ImageState {
        /// Not built, or not on this engine.
        Absent,
        /// Build images is running.
        Building,
        /// The image the running Officina would build now (AD-99).
        Ready,
        /// Built, but a compared label or a measurement differs.
        Mismatch,
        /// Its last build failed.
        Failed,
    }
}

/// One project's inventory on an image (AD-118): when it was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InventoryView {
    pub project: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
}

/// An image's newest battery (AD-117): its outcome, when it finished, and
/// whether it keeps the image valid on the engine as read now; `valid` is
/// `None` while the engine's key is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BatteryView {
    pub outcome: Option<super::battery::BatteryOutcome>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub finished_at_ms: Option<i64>,
    pub valid: Option<bool>,
}

/// One image of the Workshop section (AD-121).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageView {
    pub tag: String,
    pub kind: super::image::ImageKind,
    pub state: ImageState,
    /// Why it is `mismatch`, and the label the comparison stopped at.
    pub mismatch: Option<super::image::ImageMismatch>,
    pub label: Option<String>,
    /// The id the engine holds under the tag, when it is ready.
    pub image_id: Option<String>,
    pub inventories: Vec<InventoryView>,
    /// `None` for the gateway image, which runs no battery.
    pub battery: Option<BatteryView>,
}

/// What the view reads of one image (AD-99, AD-117, AD-118).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFacts {
    pub check: super::doctor::ImageInput,
    pub kind: super::image::ImageKind,
    pub inventories: Vec<InventoryView>,
    pub battery: Option<super::doctor::BatteryFacts>,
}

/// An image's row from its facts at `now_ms`, `building` while Build
/// images runs: the launch's own check decides `ready` and `mismatch`
/// (AD-99), a recorded failed build is `failed`.
pub fn image_view(facts: &ImageFacts, building: bool, now_ms: i64) -> ImageView {
    use super::image::{ImageMismatch, check};
    let input = &facts.check;
    let failed = input.recorded.as_ref().is_some_and(|r| !r.built);
    let checked = check(
        &input.expected,
        input.inspected.as_ref(),
        input.recorded.as_ref(),
    );
    let (state, mismatch, label, image_id) = match checked {
        _ if building => (ImageState::Building, None, None, None),
        Ok(id) => (ImageState::Ready, None, None, Some(id)),
        Err(_) if failed => (ImageState::Failed, None, None, None),
        Err(m) if m.cause == ImageMismatch::Absent => (ImageState::Absent, None, None, None),
        Err(m) => (ImageState::Mismatch, Some(m.cause), m.label, None),
    };
    let battery = facts.battery.as_ref().map(|b| {
        let newest = b.runs.first();
        BatteryView {
            outcome: newest.and_then(|run| run.outcome),
            finished_at_ms: newest.and_then(|run| run.finished_at_ms),
            valid: b
                .current
                .as_ref()
                .map(|key| super::battery::is_valid(&b.runs, key, now_ms).is_some()),
        }
    });
    ImageView {
        tag: input.tag.clone(),
        kind: facts.kind,
        state,
        mismatch,
        label,
        image_id,
        inventories: facts.inventories.clone(),
        battery,
    }
}

/// One run of the Workshop section's lists (AD-121).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkshopRunView {
    pub id: String,
    pub template: String,
    pub project: String,
    pub name: String,
    pub state: RunState,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub state_at_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at_ms: i64,
}

impl WorkshopRunView {
    fn of(run: &BoardRun) -> WorkshopRunView {
        WorkshopRunView {
            id: run.id.clone(),
            template: run.template.clone(),
            project: run.project.clone(),
            name: run.name.clone(),
            state: run.state,
            state_at_ms: run.state_at_ms,
            started_at_ms: run.started_at_ms,
        }
    }
}

/// The live, kept and orphan runs (AD-116, AD-121): an orphan is a run
/// container of this floor no row owns, listed by name, never adopted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunLists {
    pub live: Vec<WorkshopRunView>,
    pub kept: Vec<WorkshopRunView>,
    pub orphans: Vec<String>,
}

/// PRD section 7's Workshop share for one UTC ISO week (D-214): workshop
/// launches that started over every work order launch that started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkshopShare {
    /// `YYYY-Www`.
    pub week: String,
    pub workshop: u32,
    pub host: u32,
}

/// PRD section 7's Environment failures for one UTC ISO week (D-217): runs
/// whose outcome is known, and those [`is_environment_failure`] counts; a
/// `no_change` run is a run, never a failure.
///
/// [`is_environment_failure`]: super::lifecycle::is_environment_failure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EnvironmentFailures {
    pub week: String,
    pub failures: u32,
    pub runs: u32,
}

/// The week's two metrics from `lines` at `now_ms`: lines from the start of
/// the UTC ISO week holding `now_ms`.
pub fn week_metrics(
    lines: &[crate::relay::journal::JournalLine],
    now_ms: i64,
) -> (WorkshopShare, EnvironmentFailures) {
    use crate::registry::types::LaunchOutcome;
    let since = crate::dates::iso_week_start_ms(now_ms);
    let week = crate::dates::iso_week_text(now_ms);
    let mut share = WorkshopShare {
        week: week.clone(),
        workshop: 0,
        host: 0,
    };
    let mut failures = EnvironmentFailures {
        week,
        failures: 0,
        runs: 0,
    };
    let count = |n: &mut u32| *n = n.saturating_add(1);
    for line in lines.iter().filter(|line| line.ts >= since) {
        match &line.entry {
            JournalEntry::WorkshopStarted(l) if l.outcome == LaunchOutcome::Started => {
                count(&mut share.workshop);
            }
            JournalEntry::WorkOrderLaunched(l) if l.outcome == LaunchOutcome::Started => {
                count(&mut share.host);
            }
            JournalEntry::WorkshopFinished(l) => {
                count(&mut failures.runs);
                if super::lifecycle::is_environment_failure(
                    Some(l.cause),
                    Some(l.outcome),
                    l.refused,
                ) {
                    count(&mut failures.failures);
                }
            }
            _ => {}
        }
    }
    (share, failures)
}

/// The workshop table's limits (FR-2, D-1103) and how many runs hold a
/// container now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LimitsView {
    pub max_workers: u32,
    pub cpus: u32,
    pub memory_mib: u32,
    pub live: u32,
}

/// What the author does about the floor's credential (D-607): nothing
/// while it is unchecked or works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialAction {
    /// A `login` floor: the owner's login command on one line (D-605).
    Login { command: String },
    /// A token floor: `claude setup-token`, then the floor's `cmdkey`
    /// line with the token pasted in (D-617).
    SetupToken { step: String, cmdkey_line: String },
}

/// The step that makes a new token for a token floor (D-607).
pub const SETUP_TOKEN_STEP: &str = "claude setup-token";

/// The floor's credential (AD-105, D-607): its kind and state, never a
/// value, and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkshopCredential {
    pub kind: super::floor::CredentialKind,
    pub state: super::launch::CredentialState,
    pub action: Option<CredentialAction>,
}

/// The action `state` calls for on a floor of `kind` (D-607): a missing
/// or expired credential's, the login command routed as the routes say
/// (D-605, D-611).
pub fn credential_action(
    kind: super::floor::CredentialKind,
    state: super::launch::CredentialState,
    floor_key: &str,
    routes: &BTreeMap<String, String>,
) -> Option<CredentialAction> {
    use super::floor::CredentialKind;
    use super::launch::CredentialState;
    match state {
        CredentialState::Unchecked | CredentialState::Ok => None,
        CredentialState::Missing
        | CredentialState::NeedsLogin
        | CredentialState::NeedsNewCredential => Some(match kind {
            CredentialKind::Login => CredentialAction::Login {
                command: super::launch::render_one_line(&super::launch::login_command(
                    floor_key,
                    super::image::WORKSHOP_BASE,
                    routes,
                )),
            },
            CredentialKind::OauthToken | CredentialKind::ApiKey => CredentialAction::SetupToken {
                step: SETUP_TOKEN_STEP.to_owned(),
                cmdkey_line: super::names::cmdkey_line(floor_key),
            },
        }),
    }
}

/// One allow-list entry's route on this network (D-611): `direct`, or the
/// proxy's `host:port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RouteView {
    pub entry: String,
    pub route: String,
}

/// The network the workers' gateways use (D-611): the floor's setting,
/// where the routes came from, and each entry's route, or why none could
/// be resolved. Never a value of the credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NetworkView {
    pub setting: String,
    pub source: Option<super::proxy::ProxySource>,
    pub routes: Vec<RouteView>,
    pub error: Option<String>,
}

impl NetworkView {
    /// The view of `routes` resolved for `setting`.
    pub fn of(setting: &str, routes: Result<&super::proxy::Routes, String>) -> NetworkView {
        use super::proxy::Route;
        match routes {
            Ok(routes) => NetworkView {
                setting: setting.to_owned(),
                source: Some(routes.source),
                routes: routes
                    .routes
                    .iter()
                    .map(|(entry, route)| RouteView {
                        entry: entry.clone(),
                        route: match route {
                            Route::Direct => "direct".to_owned(),
                            Route::Proxy(proxy) => proxy.clone(),
                        },
                    })
                    .collect(),
                error: None,
            },
            Err(error) => NetworkView {
                setting: setting.to_owned(),
                source: None,
                routes: Vec::new(),
                error: Some(error),
            },
        }
    }
}

/// The Workshop section's payload (AD-121, amendment 13): its own event,
/// apart from `OrchestratorView`, so followers' updates do not churn it.
/// It holds no worker text: run names are the work orders', orphans the
/// engine objects' names Officina gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkshopView {
    pub schema: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub generated_at_ms: i64,
    /// The approved catalogue holds a valid `[workshop]` table.
    pub enabled: bool,
    /// `None` until the engine was read.
    pub engine: Option<EngineView>,
    /// Both stop flags (AD-116): each stops only its own.
    pub relay_stopped: bool,
    pub workshop_stopped: bool,
    pub images: Vec<ImageView>,
    pub runs: RunLists,
    /// The per-template table with its notices (FR-11, FR-12, D-1188).
    pub templates: Vec<TemplateRow>,
    pub share: WorkshopShare,
    pub environment_failures: EnvironmentFailures,
    pub limits: Option<LimitsView>,
    pub credential: Option<WorkshopCredential>,
    pub network: Option<NetworkView>,
}

/// What reading the engine and the approved catalogue gave (AD-121's
/// "engine's read half"): the window reads it on thread `workshop`, the
/// command line in its own process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineHalf {
    pub enabled: bool,
    pub engine: Option<EngineView>,
    pub images: Vec<ImageFacts>,
    /// Build images ran when it was read.
    pub building: bool,
    /// `max_workers`, `cpus`, `memory_mib`.
    pub limits: Option<(u32, u32, u32)>,
    pub credential: Option<WorkshopCredential>,
    pub network: Option<NetworkView>,
}

/// What the registry, the journal and the flags hold (the view's other
/// half).
#[derive(Debug, Clone, Copy)]
pub struct RegistryHalf<'a> {
    pub runs: &'a [BoardRun],
    pub orphans: &'a [String],
    pub lines: &'a [crate::relay::journal::JournalLine],
    pub relay_stopped: bool,
    pub workshop_stopped: bool,
}

/// The Workshop view at `now_ms` (AD-121): the same halves give the same
/// bytes.
pub fn build_view(engine: &EngineHalf, registry: &RegistryHalf, now_ms: i64) -> WorkshopView {
    let live: Vec<WorkshopRunView> = registry
        .runs
        .iter()
        .filter(|run| run.state.is_live())
        .map(WorkshopRunView::of)
        .collect();
    let kept = registry
        .runs
        .iter()
        .filter(|run| run.state == RunState::Stopped)
        .map(WorkshopRunView::of)
        .collect();
    let (share, environment_failures) = week_metrics(registry.lines, now_ms);
    let live_count = u32::try_from(live.len()).unwrap_or(u32::MAX);
    WorkshopView {
        schema: WORKSHOP_VIEW_SCHEMA,
        generated_at_ms: now_ms,
        enabled: engine.enabled,
        engine: engine.engine.clone(),
        relay_stopped: registry.relay_stopped,
        workshop_stopped: registry.workshop_stopped,
        images: engine
            .images
            .iter()
            .map(|facts| image_view(facts, engine.building, now_ms))
            .collect(),
        runs: RunLists {
            live,
            kept,
            orphans: registry.orphans.to_vec(),
        },
        templates: template_table(registry.lines.iter().map(|line| &line.entry)),
        share,
        environment_failures,
        limits: engine
            .limits
            .map(|(max_workers, cpus, memory_mib)| LimitsView {
                max_workers,
                cpus,
                memory_mib,
                live: live_count,
            }),
        credential: engine.credential.clone(),
        network: engine.network.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `decided` packages of template `bump`, the first `with_ci` of them
    /// carrying `ci`, and one undecided package carrying it too.
    fn packages(decided: usize, with_ci: usize) -> Vec<TemplatePackage> {
        let mut all: Vec<TemplatePackage> = (0..decided)
            .map(|i| TemplatePackage {
                template: "bump".into(),
                sensitive: if i < with_ci {
                    vec![SensitiveGroup::Ci, SensitiveGroup::File]
                } else {
                    vec![SensitiveGroup::File]
                },
                decided: true,
            })
            .collect();
        all.push(TemplatePackage {
            template: "bump".into(),
            sensitive: vec![SensitiveGroup::Ci],
            decided: false,
        });
        all
    }

    /// FR-11: no notice before 10 verdicts, an undecided package not
    /// counting as one.
    #[test]
    fn the_notice_needs_10_verdicts() {
        let short = NOTICE_MIN_VERDICTS - 1;
        assert!(group_notices(&packages(short, short)).is_empty());
        let notices = group_notices(&packages(NOTICE_MIN_VERDICTS, NOTICE_MIN_VERDICTS));
        assert_eq!(
            notices,
            [GroupNotice {
                template: "bump".into(),
                group: SensitiveGroup::Ci,
                packages: 10,
                verdicts: 10,
            }]
        );
    }

    /// FR-11: a group in exactly half of the decided packages gives no
    /// notice, one more does; `file` never gives one.
    #[test]
    fn the_notice_needs_more_than_half() {
        let half = NOTICE_MIN_VERDICTS / 2;
        assert!(group_notices(&packages(NOTICE_MIN_VERDICTS, half)).is_empty());
        let notices = group_notices(&packages(NOTICE_MIN_VERDICTS, half + 1));
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].group, SensitiveGroup::Ci);
        assert_eq!(notices[0].packages, u32::try_from(half + 1).unwrap());
    }

    use crate::relay::journal::{ReviewDiscarded, ReviewImported, ReviewPackage, WorkshopFinished};
    use crate::workshop::run::FinishCause;

    fn id(n: usize) -> String {
        format!("{n:032x}")
    }

    fn finished(n: usize, template: &str, outcome: WorkshopOutcome) -> JournalEntry {
        JournalEntry::WorkshopFinished(Box::new(WorkshopFinished {
            run_id: id(n),
            template: template.into(),
            project: "api".into(),
            base_branch: "main".into(),
            base_commit: "a".repeat(40),
            cause: FinishCause::Exited,
            outcome,
            commits_past_base: None,
            tip: None,
            exit_code: Some(0),
            duration_ms: 1,
            cost_usd_micros: None,
            turns: None,
            result_subtype: None,
            hooks: BTreeMap::new(),
            refused: 0,
            stream_bytes: 0,
        }))
    }

    fn package(n: usize, state: PackageState, sensitive: &[SensitiveGroup]) -> JournalEntry {
        JournalEntry::ReviewPackage(ReviewPackage {
            run_id: id(n),
            bundle_sha256: None,
            commits: 1,
            files: 1,
            insertions: 1,
            deletions: 0,
            groups: u32::try_from(sensitive.len()).unwrap(),
            outcome: state,
            cause: None,
            credential_locations: None,
            sensitive: sensitive.to_vec(),
        })
    }

    fn imported(n: usize) -> JournalEntry {
        JournalEntry::ReviewImported(ReviewImported {
            run_id: id(n),
            branch: format!("officina/bump-{n}"),
            tip: "b".repeat(40),
        })
    }

    fn discarded(n: usize, reason: DiscardReason) -> JournalEntry {
        JournalEntry::ReviewDiscarded(ReviewDiscarded {
            run_id: id(n),
            reason,
        })
    }

    fn discards(row: &TemplateRow, reason: DiscardReason) -> u32 {
        row.discarded
            .iter()
            .find(|d| d.reason == reason)
            .map_or(0, |d| d.count)
    }

    /// D-222, AD-110: a `no_change` and a failed run count as runs and in
    /// their own columns, never in `decided`.
    #[test]
    fn no_change_and_failed_are_not_decided() {
        let lines = [
            finished(1, "bump", WorkshopOutcome::NoChange),
            finished(2, "bump", WorkshopOutcome::Failed),
            finished(3, "bump", WorkshopOutcome::Package),
            package(3, PackageState::Review, &[]),
            imported(3),
        ];
        let table = template_table(&lines);
        assert_eq!(table.len(), 1);
        let row = &table[0];
        assert_eq!((row.runs, row.no_change, row.failed), (3, 1, 1));
        assert_eq!((row.imported, row.decided), (1, 1));
    }

    /// D-222: `expired` and `not_collected` sit in the discard columns and
    /// never in `decided`; a quarantined package discarded by the author
    /// is decided.
    #[test]
    fn expired_and_not_collected_are_not_decided() {
        let lines = [
            finished(1, "bump", WorkshopOutcome::Package),
            package(1, PackageState::Review, &[]),
            discarded(1, DiscardReason::Expired),
            finished(2, "bump", WorkshopOutcome::NotCollected),
            discarded(2, DiscardReason::NotCollected),
            finished(3, "bump", WorkshopOutcome::Package),
            package(3, PackageState::Quarantined, &[]),
            discarded(3, DiscardReason::ShouldNotHaveRun),
        ];
        let row = &template_table(&lines)[0];
        assert_eq!(discards(row, DiscardReason::Expired), 1);
        assert_eq!(discards(row, DiscardReason::NotCollected), 1);
        assert_eq!(discards(row, DiscardReason::ShouldNotHaveRun), 1);
        assert_eq!(row.quarantined, 1);
        assert_eq!(row.decided, 1);
        assert_eq!(row.discarded.len(), DiscardReason::ALL.len());
    }

    /// D-222, FR-11: Acceptance is imported over the author's verdicts,
    /// per template; a verdict on a run the journal never finished is not
    /// counted; the notice counts decided packages only.
    #[test]
    fn imported_over_decided_counts_author_verdicts_only() {
        let mut lines = Vec::new();
        for n in 0..NOTICE_MIN_VERDICTS {
            lines.push(finished(n, "bump", WorkshopOutcome::Package));
            lines.push(package(n, PackageState::Review, &[SensitiveGroup::Ci]));
            lines.push(if n % 2 == 0 {
                imported(n)
            } else {
                discarded(n, DiscardReason::PoorResult)
            });
        }
        let other = NOTICE_MIN_VERDICTS + 1;
        lines.push(finished(other, "lint", WorkshopOutcome::Package));
        lines.push(package(other, PackageState::Review, &[]));
        lines.push(discarded(other, DiscardReason::Expired));
        lines.push(imported(other + 1));
        let table = template_table(&lines);
        let names: Vec<&str> = table.iter().map(|r| r.template.as_str()).collect();
        assert_eq!(names, ["bump", "lint"]);
        let bump = &table[0];
        let half = u32::try_from(NOTICE_MIN_VERDICTS / 2).unwrap();
        assert_eq!((bump.imported, bump.decided), (half, half * 2));
        assert_eq!(discards(bump, DiscardReason::PoorResult), half);
        assert_eq!(bump.notices.len(), 1);
        assert_eq!(bump.notices[0].group, SensitiveGroup::Ci);
        let lint = &table[1];
        assert_eq!((lint.runs, lint.imported, lint.decided), (1, 0, 0));
        assert!(lint.notices.is_empty());
    }

    const NOW: i64 = 1_793_800_000_000;

    fn run(n: usize, state: RunState, state_at_ms: i64) -> BoardRun {
        BoardRun {
            id: id(n),
            template: "bump".into(),
            project: "api".into(),
            name: format!("bump-{n}"),
            state,
            state_at_ms,
            started_at_ms: state_at_ms - 1,
        }
    }

    fn board_of(runs: Vec<BoardRun>) -> crate::board::Board {
        crate::board::build_board(
            &crate::board::BoardInput {
                agents: Some(Vec::new()),
                workshop_runs: runs,
                ..Default::default()
            },
            NOW,
        )
    }

    /// AD-121, amendment 15: a `no_change` run is Done with its badge and
    /// never Needs you; a package awaiting a verdict is, and counts.
    #[test]
    fn no_change_is_done_never_needs_you() {
        let board = board_of(vec![
            run(1, RunState::NoChange, NOW),
            run(2, RunState::Review, NOW),
            run(3, RunState::Quarantined, NOW),
        ]);
        assert_eq!(board.columns.done.len(), 1);
        assert_eq!(board.columns.done[0].badge, Some(Badge::NoChange));
        assert_eq!(board.columns.done[0].key, format!("ws:{}", id(1)));
        assert_eq!(board.columns.needs_you.len(), 2);
        assert_eq!(board.waiting_count, 2);
        assert!(
            board
                .columns
                .needs_you
                .iter()
                .all(|c| c.waiting_for.as_deref() == Some(WAITING_FOR_REVIEW))
        );
    }

    /// AD-121, AD-116: a kept run is To resume, not counted, whatever its
    /// age; live runs feed `workshop_live`.
    #[test]
    fn stopped_is_to_resume_not_counted() {
        let board = board_of(vec![
            run(1, RunState::Stopped, NOW - 3 * DONE_WINDOW_MS),
            run(2, RunState::Running, NOW),
            run(3, RunState::Collecting, NOW),
        ]);
        assert_eq!(board.columns.to_resume.len(), 1);
        assert_eq!(board.waiting_count, 0);
        assert_eq!(board.columns.working.len(), 2);
        assert_eq!(board.workshop_live, 2);
    }

    /// AD-121: every terminal run is Done for 24 h after it entered its
    /// state, then leaves the board.
    #[test]
    fn done_lasts_24_hours() {
        for state in RunState::ALL.iter().copied().filter(|s| s.is_terminal()) {
            let fresh = workshop_cards(&[run(1, state, NOW - DONE_WINDOW_MS)], &[], NOW);
            assert_eq!(fresh.len(), 1, "{state:?}");
            assert_eq!(fresh[0].column, Column::Done, "{state:?}");
            let old = workshop_cards(&[run(1, state, NOW - DONE_WINDOW_MS - 1)], &[], NOW);
            assert!(old.is_empty(), "{state:?}");
        }
    }

    /// D-619: until story 6.23 no workshop card has a scene, so the scene
    /// draws no worker and counts no alert.
    #[test]
    fn workshop_cards_have_no_scene_yet() {
        let runs: Vec<BoardRun> = RunState::ALL
            .iter()
            .enumerate()
            .map(|(n, state)| run(n, *state, NOW))
            .collect();
        let board = board_of(runs);
        assert_eq!(board.columns.all().count(), RunState::ALL.len());
        assert!(board.columns.all().all(|card| card.scene.is_none()
            && card.kind == Some(SessionKindView::Workshop)
            && card.run_state.is_some()));
        assert_eq!(board.alert_count, 0);
    }

    fn line(ts: i64, entry: JournalEntry) -> crate::relay::journal::JournalLine {
        crate::relay::journal::JournalLine::new(ts, entry)
    }

    fn started(n: usize, outcome: crate::registry::types::LaunchOutcome) -> JournalEntry {
        use crate::relay::journal::WorkshopStarted;
        JournalEntry::WorkshopStarted(Box::new(WorkshopStarted {
            run_id: id(n),
            name: format!("bump-{n}"),
            template: "bump".into(),
            project: "api".into(),
            values: BTreeMap::new(),
            image: "officina-workshop:local".into(),
            image_id: "sha256:0".into(),
            base_branch: "main".into(),
            base_commit: "a".repeat(40),
            parent_run_id: None,
            hosts: Vec::new(),
            mounts: Vec::new(),
            deadline_ms: 1,
            max_usd_cents: None,
            credential_kind: crate::workshop::floor::CredentialKind::OauthToken,
            argv: Vec::new(),
            plan_sha256: String::new(),
            catalogue_hash: String::new(),
            prompt_sha256: String::new(),
            battery_id: String::new(),
            steps: Vec::new(),
            outcome,
        }))
    }

    fn launched(n: usize, outcome: crate::registry::types::LaunchOutcome) -> JournalEntry {
        use crate::relay::journal::WorkOrderLaunched;
        JournalEntry::WorkOrderLaunched(WorkOrderLaunched {
            order_id: id(n),
            name: format!("bump-{n}"),
            template: "bump".into(),
            project: "api".into(),
            values: BTreeMap::new(),
            argv: Vec::new(),
            prompt_sha256: String::new(),
            catalogue_hash: String::new(),
            outcome,
            exit_code: None,
        })
    }

    /// D-214: the week's share counts started launches of both runners,
    /// from the week's Monday; a failed launch and last week's do not.
    #[test]
    fn workshop_share_counts_started_outcomes_of_both_kinds() {
        use crate::registry::types::LaunchOutcome;
        let monday = crate::dates::iso_week_start_ms(NOW);
        let lines = [
            line(monday - 1, started(1, LaunchOutcome::Started)),
            line(monday, started(2, LaunchOutcome::Started)),
            line(monday + 1, started(3, LaunchOutcome::Failed)),
            line(monday + 2, launched(4, LaunchOutcome::Started)),
            line(monday + 3, launched(5, LaunchOutcome::Started)),
            line(monday + 4, launched(6, LaunchOutcome::UntrustedWorkspace)),
        ];
        let (share, _) = week_metrics(&lines, NOW);
        assert_eq!((share.workshop, share.host), (1, 2));
        assert_eq!(share.week, crate::dates::iso_week_text(NOW));
    }

    /// D-217, amendment 15: a `no_change` run counts in the denominator,
    /// never in the numerator; a cause of `ENVIRONMENT_FAILURE_CAUSES`
    /// does.
    #[test]
    fn environment_failures_exclude_no_change() {
        let finish = |n, cause, outcome| {
            let JournalEntry::WorkshopFinished(mut l) = finished(n, "bump", outcome) else {
                unreachable!()
            };
            l.cause = cause;
            l.refused = 1;
            line(NOW, JournalEntry::WorkshopFinished(l))
        };
        let mut lines = vec![finish(
            1,
            crate::workshop::run::FinishCause::Exited,
            WorkshopOutcome::NoChange,
        )];
        for (n, cause) in crate::workshop::lifecycle::ENVIRONMENT_FAILURE_CAUSES
            .iter()
            .enumerate()
        {
            lines.push(finish(n + 2, *cause, WorkshopOutcome::Failed));
        }
        let (_, failures) = week_metrics(&lines, NOW);
        assert_eq!(failures.runs as usize, lines.len());
        assert_eq!(failures.failures as usize, lines.len() - 1);
    }

    /// D-607: only a missing or expired credential calls for an action,
    /// the login command for a `login` floor and `setup-token` with the
    /// floor's `cmdkey` line for a token floor.
    #[test]
    fn every_credential_state_names_its_action() {
        use crate::workshop::floor::CredentialKind;
        use crate::workshop::launch::CredentialState;
        let key = "0123456789abcdef";
        for kind in CredentialKind::ALL {
            for state in CredentialState::ALL {
                let action = credential_action(*kind, *state, key, &BTreeMap::new());
                match (state, action) {
                    (CredentialState::Unchecked | CredentialState::Ok, None) => {}
                    (_, Some(CredentialAction::Login { command })) => {
                        assert_eq!(*kind, CredentialKind::Login);
                        assert!(command.contains("auth login"), "{command}");
                    }
                    (_, Some(CredentialAction::SetupToken { step, cmdkey_line })) => {
                        assert_ne!(*kind, CredentialKind::Login);
                        assert_eq!(step, SETUP_TOKEN_STEP);
                        assert_eq!(cmdkey_line, crate::workshop::names::cmdkey_line(key));
                    }
                    (state, action) => panic!("{kind:?} {state:?}: {action:?}"),
                }
            }
        }
    }

    /// AD-5: equal halves give equal bytes, so the window emits only on a
    /// change.
    #[test]
    fn the_view_is_stable_for_equal_inputs() {
        let runs = [
            run(1, RunState::Running, NOW),
            run(2, RunState::Stopped, NOW),
        ];
        let lines = [line(NOW, finished(1, "bump", WorkshopOutcome::Package))];
        let registry = RegistryHalf {
            runs: &runs,
            orphans: &["officina-w-x".into()],
            lines: &lines,
            relay_stopped: false,
            workshop_stopped: true,
        };
        let engine = EngineHalf {
            enabled: true,
            limits: Some((2, 2, 4096)),
            ..EngineHalf::default()
        };
        let one = serde_json::to_vec(&build_view(&engine, &registry, NOW)).unwrap();
        let two = serde_json::to_vec(&build_view(&engine, &registry, NOW)).unwrap();
        assert_eq!(one, two);
        let view = build_view(&engine, &registry, NOW);
        assert_eq!(view.schema, WORKSHOP_VIEW_SCHEMA);
        assert_eq!((view.runs.live.len(), view.runs.kept.len()), (1, 1));
        assert_eq!(view.limits.map(|l| l.live), Some(1));
        assert_eq!(view.templates.len(), 1);
    }
}
