//! `human` and `auto` actions (FR-7, FR-8, FR-9, AD-33, AD-35, AD-86, D-27,
//! D-170): the request key and id, the files of `relay/approvals/`, what
//! `officina run` does next for a request, the cards the window shows and
//! what the window may decide. An `auto` request takes a `human` one's path
//! (AD-86); who approved it is in its approval file.
//! Every command shown or approved is rebuilt from the approved catalogue,
//! never read from a request file, which any process running as the author
//! can write (D-27). Pure: the host reads and writes the files.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::argv;
use super::auto::EscalationCause;
use super::catalogue::{
    ActionClass, ApprovedCatalogue, action_from_canonical, credentials_from_canonical,
};
use super::http::{self, HttpMethod, HttpPlan};
use super::journal::TreeFingerprint;
use crate::quota::normalise_path;
use crate::registry::types::AuthorReason;
use crate::untrusted::Untrusted;

/// A request no decision answered is listed and reused this long, then
/// expires (AD-33, FR-9).
pub const PENDING_MS: i64 = 60 * 60 * 1000;
/// An approval runs, and a rejection blocks the same request, this long
/// after the decision (AD-33).
pub const DECISION_VALID_MS: i64 = 15 * 60 * 1000;
/// The window deletes approval files older than this; `officina run` never
/// deletes any (AD-33).
pub const RETENTION_MS: i64 = 24 * 60 * 60 * 1000;
/// `officina run --wait` when not given: fits the Bash tool's default two
/// minutes (AD-33).
pub const DEFAULT_WAIT_S: u32 = 110;
/// How often a waiting `officina run` looks for the decision and the stop
/// flag (AD-33, AD-34).
pub const WAIT_TICK_MS: u64 = 250;
/// Hex digits of the request key that start a request id (AD-33).
pub const ID_KEY_HEX: usize = 16;

/// Where `officina run` keeps the output of a check run (D-170): the relay
/// directory's `outputs/`, one file per run id.
pub const OUTPUTS_DIR: &str = "outputs";
/// The most a kept check output holds (D-170): stdout, then stderr, then a
/// cut marker past it.
pub const KEPT_OUTPUT_BYTES: usize = 256 * 1024;
/// The window deletes kept outputs older than this (D-170).
pub const KEPT_OUTPUT_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

closed_enum! {
    /// The files of one request in `relay/approvals/` (AD-25).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum FileKind {
        /// Written by `officina run`, with `create_new`.
        Request,
        /// Written by the window: approved, not used yet.
        Approved,
        /// Written by the window.
        Rejected,
        /// An approval `officina run` used: claimed with `create_new`,
        /// the `Approved` name removed after.
        Consumed,
        /// Empty, created with `create_new` by whoever decides first, the
        /// author or a review, before its decision file (D-157): a second
        /// decider is refused `already_decided` before it writes anything.
        Decided,
        /// Empty, written by the window while a live review decides the
        /// request (D-157): the author cannot decide it then. The window
        /// removes it when the review answers, and every one at its start.
        Reviewing,
    }
}

impl FileKind {
    /// What follows the id in the file's name.
    pub fn suffix(self) -> &'static str {
        match self {
            FileKind::Request => ".request.json",
            FileKind::Approved => ".approved.json",
            FileKind::Rejected => ".rejected.json",
            FileKind::Consumed => ".consumed.json",
            FileKind::Decided => ".decided",
            FileKind::Reviewing => ".reviewing",
        }
    }
}

/// The file of request `id` of `kind`.
pub fn file_name(id: &str, kind: FileKind) -> String {
    format!("{id}{}", kind.suffix())
}

/// A request id: [`ID_KEY_HEX`] lowercase hex digits, `-`, digits.
pub fn is_valid_id(id: &str) -> bool {
    id.split_once('-').is_some_and(|(key, ms)| {
        key.len() == ID_KEY_HEX
            && key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && !ms.is_empty()
            && ms.bytes().all(|b| b.is_ascii_digit())
    })
}

/// The id and kind a file name stands for; `None` for any other file.
///
/// ```
/// use officina_core::relay::approvals::{FileKind, parse_file_name};
/// assert_eq!(
///     parse_file_name("0123456789abcdef-17.approved.json"),
///     Some(("0123456789abcdef-17", FileKind::Approved)),
/// );
/// assert_eq!(parse_file_name("notes.txt"), None);
/// ```
pub fn parse_file_name(name: &str) -> Option<(&str, FileKind)> {
    FileKind::ALL.iter().find_map(|kind| {
        let id = name.strip_suffix(kind.suffix())?;
        is_valid_id(id).then_some((id, *kind))
    })
}

/// What a request is about: the request key covers exactly this (AD-33,
/// D-170, D-173). The dry run's run id is present only for an action naming
/// a check; the tree for every request in a git work tree, so a request
/// outside one is keyed by AD-33's four fields.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct KeyInputs<'a> {
    pub project: &'a str,
    pub action: &'a str,
    pub params: &'a BTreeMap<String, String>,
    pub catalogue_hash: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_run_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<&'a TreeFingerprint>,
}

/// SHA-256 of the canonical form of `inputs`, lowercase hex (AD-33).
/// Parameters are a sorted map, so their order on the command line does not
/// matter.
pub fn request_key(inputs: &KeyInputs<'_>) -> String {
    // A struct of strings and string-keyed maps always serializes.
    super::sha256_hex(&serde_json::to_vec(inputs).unwrap_or_default())
}

/// A request's id: the key's first [`ID_KEY_HEX`] hex digits, `-`, the
/// time it was created (AD-33).
pub fn request_id(key: &str, now_ms: i64) -> String {
    format!("{}-{now_ms}", key.get(..ID_KEY_HEX).unwrap_or(key))
}

/// Content of `<id>.request.json`, written by `officina run` (AD-33). Every
/// field can be forged; the window trusts none of them for display (D-27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    pub request_key: String,
    pub created_at_ms: i64,
    pub project: String,
    pub action: String,
    pub params: BTreeMap<String, String>,
    /// What `officina run` would start, for the journal and for diagnosis;
    /// never shown (D-27). Absent for an HTTP action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argv: Vec<String>,
    /// What an HTTP action would send; never shown either (D-27).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpRequestView>,
    pub catalogue_hash: String,
    #[serde(default)]
    pub session_id: Option<String>,
    /// The check run the request was made after, for an action naming a
    /// check (D-170).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_run_id: Option<String>,
    /// The project's files when the request was made, the ones its check ran
    /// on when it names one; absent outside a git work tree (D-170, D-173).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeFingerprint>,
}

impl ApprovalRequest {
    /// The inputs its key covers, as the file states them.
    pub fn key_inputs(&self) -> KeyInputs<'_> {
        KeyInputs {
            project: &self.project,
            action: &self.action,
            params: &self.params,
            catalogue_hash: &self.catalogue_hash,
            check_run_id: self.check_run_id.as_deref(),
            tree: self.tree.as_ref(),
        }
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Who gave an approval (AD-86, AD-25 as amended): the author in the
    /// window, an `auto` action's review, or the rule baseline (D-141).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ApprovalBy {
        Author,
        Review,
        Rule,
    }
}

/// Content of `<id>.approved.json`, `.rejected.json` and `.consumed.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub decided_at_ms: i64,
    pub request_key: String,
    /// Who decided; absent in files written before v0.5, which only the
    /// author wrote ([`Decision::by`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<ApprovalBy>,
    /// The review row that approved, for an approval `by` a review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
    /// Why the author rejected an `auto` request (FR-9, AD-86).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<AuthorReason>,
}

impl Decision {
    /// The author's decision, as the window writes it.
    pub fn by_author(decided_at_ms: i64, request_key: &str, reason: Option<AuthorReason>) -> Self {
        Decision {
            decided_at_ms,
            request_key: request_key.to_owned(),
            by: Some(ApprovalBy::Author),
            review_id: None,
            reason,
        }
    }

    /// An approval by a review or the rule (AD-86, D-141), naming the
    /// review row that gave it.
    pub fn by_review(
        decided_at_ms: i64,
        request_key: &str,
        by: ApprovalBy,
        review_id: i64,
    ) -> Self {
        Decision {
            decided_at_ms,
            request_key: request_key.to_owned(),
            by: Some(by),
            review_id: Some(review_id.to_string()),
            reason: None,
        }
    }

    /// Who decided: the author when the file does not say (AD-86).
    pub fn by(&self) -> ApprovalBy {
        self.by.unwrap_or(ApprovalBy::Author)
    }
}

/// The readable files of `relay/approvals/`, by request id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApprovalFiles {
    pub requests: BTreeMap<String, ApprovalRequest>,
    pub approved: BTreeMap<String, Decision>,
    pub rejected: BTreeMap<String, Decision>,
    pub consumed: BTreeMap<String, Decision>,
    /// The requests whose decision marker exists (D-157).
    pub markers: std::collections::BTreeSet<String>,
    /// The requests a live review is deciding (D-157).
    pub reviewing: std::collections::BTreeSet<String>,
}

impl ApprovalFiles {
    /// True when the window decided request `id`, whether the approval was
    /// used or not.
    pub fn decided(&self, id: &str) -> bool {
        self.approved.contains_key(id)
            || self.rejected.contains_key(id)
            || self.consumed.contains_key(id)
    }
}

/// True while a request created at `created_at_ms` is pending: not dated in
/// the future and not older than [`PENDING_MS`].
pub fn is_live(created_at_ms: i64, now_ms: i64) -> bool {
    (0..=PENDING_MS).contains(&(now_ms - created_at_ms))
}

/// What `officina run` does next for a request key (AD-33 steps 1 to 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextStep {
    /// A rejection for this key under [`DECISION_VALID_MS`] old: refused.
    Rejected { id: String },
    /// An approval for this key under [`DECISION_VALID_MS`] old, unused:
    /// consume it and run.
    Consume { id: String },
    /// A pending request for this key: wait for its decision.
    Wait { id: String },
    /// Create a request and wait for its decision.
    Create,
}

/// AD-33 steps 1 to 3 for `key` at `now_ms`; the latest file wins each step.
pub fn next_step(files: &ApprovalFiles, key: &str, now_ms: i64) -> NextStep {
    let recent = |decisions: &BTreeMap<String, Decision>| {
        decisions
            .iter()
            .filter(|(_, d)| {
                d.request_key == key && (0..DECISION_VALID_MS).contains(&(now_ms - d.decided_at_ms))
            })
            .max_by_key(|(_, d)| d.decided_at_ms)
            .map(|(id, _)| id.clone())
    };
    if let Some(id) = recent(&files.rejected) {
        return NextStep::Rejected { id };
    }
    // An approval with a consumed file too was used: a crash can leave both.
    let unused: BTreeMap<String, Decision> = files
        .approved
        .iter()
        .filter(|(id, _)| !files.consumed.contains_key(*id))
        .map(|(id, d)| (id.clone(), d.clone()))
        .collect();
    if let Some(id) = recent(&unused) {
        return NextStep::Consume { id };
    }
    files
        .requests
        .iter()
        .filter(|(id, r)| {
            r.request_key == key && is_live(r.created_at_ms, now_ms) && !files.decided(id)
        })
        .max_by_key(|(_, r)| r.created_at_ms)
        .map_or(NextStep::Create, |(id, _)| NextStep::Wait {
            id: id.clone(),
        })
}

/// What a request is checked against: the approved snapshot, the projects'
/// folders as the relay floor declares them (D-6) and the system directory
/// `wsl.exe` comes from (AD-30).
#[derive(Debug, Clone, Copy)]
pub struct ApprovalContext<'a> {
    pub approved: Option<&'a ApprovedCatalogue>,
    pub project_dirs: &'a BTreeMap<String, String>,
    pub system_dir: &'a str,
}

/// An HTTP request as a card and a request file state it (AD-36, D-27):
/// the method, the URL with its values, and the credential by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct HttpRequestView {
    pub method: HttpMethod,
    pub url: String,
    pub credential: Option<String>,
}

impl From<&HttpPlan> for HttpRequestView {
    fn from(plan: &HttpPlan) -> HttpRequestView {
        HttpRequestView {
            method: plan.method,
            url: plan.url.clone(),
            credential: plan.credential.as_ref().map(|c| c.name.clone()),
        }
    }
}

/// A request rebuilt from the approved catalogue (D-27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rebuilt {
    /// `human` or `auto`: the classes that wait for an approval.
    pub class: ActionClass,
    /// The program, then its arguments: exactly what `officina run` starts;
    /// empty for an HTTP action.
    pub argv: Vec<String>,
    /// What an HTTP action sends.
    pub http: Option<HttpRequestView>,
    pub description: String,
    /// The check the action names, its dry run (D-170).
    pub check: Option<String>,
}

/// Rebuilds the command request `id` stands for from the approved catalogue
/// and the request's project, action and parameters, or says why the
/// request is invalid (D-27): no catalogue approved, another catalogue, a key
/// or id that does not match the request, an action the project no longer
/// lists or that is neither `human` nor `auto`, parameters that no longer
/// fit, a dry run
/// missing or unexpected, or a stored command that differs.
pub fn rebuild(
    ctx: &ApprovalContext<'_>,
    id: &str,
    request: &ApprovalRequest,
) -> Result<Rebuilt, String> {
    let approved = ctx
        .approved
        .ok_or_else(|| "no catalogue is approved".to_owned())?;
    if request.catalogue_hash != approved.hash {
        return Err(
            "it was made under another catalogue than the approved one; the session runs the action again"
                .to_owned(),
        );
    }
    if request_key(&request.key_inputs()) != request.request_key {
        return Err("its key does not match its project, action and parameters".to_owned());
    }
    if request_id(&request.request_key, request.created_at_ms) != id {
        return Err("its id does not match its key and time".to_owned());
    }
    let project = approved
        .canonical
        .projects
        .get(&request.project)
        .filter(|p| p.actions.contains(&request.action))
        .ok_or_else(|| {
            "the approved catalogue no longer gives this action to this project".to_owned()
        })?;
    let action = approved
        .canonical
        .actions
        .get(&request.action)
        .filter(|a| waits_for_approval(a.class))
        .ok_or_else(|| {
            "the approved catalogue has no human or auto action of this name".to_owned()
        })?;
    match (&action.check, &request.check_run_id) {
        (Some(_), None) => return Err("it names no dry run, which the action requires".to_owned()),
        (None, Some(_)) => return Err("it names a dry run the action has no check for".to_owned()),
        _ => {}
    }
    let validated = action_from_canonical(&request.action, action)
        .map_err(|_| "the approved action no longer validates".to_owned())?;
    let project_dir = ctx
        .project_dirs
        .get(&request.project)
        .filter(|dir| normalise_path(dir) == project.path)
        .cloned()
        .unwrap_or_else(|| project.path.clone());
    let unfit = |refusal: argv::ArgvRefusal| {
        format!("its parameters do not fit the approved action: {refusal}")
    };
    let credentials = credentials_from_canonical(&approved.canonical);
    let (rebuilt, http) = match http::build(&validated, &request.params, &credentials) {
        Some(plan) => (
            Vec::new(),
            Some(HttpRequestView::from(&plan.map_err(unfit)?)),
        ),
        None => {
            let launch = argv::build(&validated, &request.params, &project_dir, ctx.system_dir)
                .map_err(unfit)?;
            let argv = std::iter::once(launch.program).chain(launch.args).collect();
            (argv, None)
        }
    };
    if rebuilt != request.argv || http != request.http {
        return Err("its stored command differs from the approved catalogue's".to_owned());
    }
    Ok(Rebuilt {
        class: action.class,
        argv: rebuilt,
        http,
        description: action.description.clone(),
        check: action.check.clone(),
    })
}

/// True for the classes whose runs wait for an approval: `human`, and
/// `auto`, which takes its path (AD-86).
pub fn waits_for_approval(class: ActionClass) -> bool {
    matches!(class, ActionClass::Human | ActionClass::Auto)
}

/// Why the window refuses to decide a request (AD-35, D-27, D-157).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecideRefusal {
    #[error("no request {0} in the relay's approvals")]
    Unknown(String),
    /// A decision file exists, or another decider created the marker first
    /// (`already_decided`, D-157).
    #[error("request {0} is already decided")]
    Decided(String),
    /// A live review is deciding it (D-157): the author decides once it
    /// escalates.
    #[error(
        "request {0} is being reviewed; it comes to Needs you if the review does not approve it"
    )]
    Reviewing(String),
    /// Rejecting an `auto` request needs `should_not_run` or `not_now`, and
    /// a `human` rejection or an approval takes none (FR-9, AD-86).
    #[error("request {id}: {why}")]
    Reason { id: String, why: String },
    #[error("request {0} expired")]
    Expired(String),
    #[error("request {id} is invalid: {why}")]
    Invalid { id: String, why: String },
    /// A review or the rule may only approve an `auto` request (AD-86).
    #[error("request {0} is not an auto action's")]
    NotAuto(String),
    /// The action's review approvals of the last hour reached its
    /// `auto_max_per_hour` (D-142, review F15).
    #[error("request {0}: its action reached its hourly cap of review approvals")]
    HourlyCap(String),
}

/// The request the window may decide now, with its class, or why not: it
/// exists, nothing decided it (no decision file, no marker, D-157), it is
/// still pending, and it rebuilds from the approved catalogue, which also
/// rules out a request of another catalogue (AD-35).
pub fn decidable<'f>(
    files: &'f ApprovalFiles,
    ctx: &ApprovalContext<'_>,
    id: &str,
    now_ms: i64,
) -> Result<(&'f ApprovalRequest, ActionClass), DecideRefusal> {
    let request = files
        .requests
        .get(id)
        .ok_or_else(|| DecideRefusal::Unknown(id.to_owned()))?;
    if files.decided(id) || files.markers.contains(id) {
        return Err(DecideRefusal::Decided(id.to_owned()));
    }
    if !is_live(request.created_at_ms, now_ms) {
        return Err(DecideRefusal::Expired(id.to_owned()));
    }
    if files.reviewing.contains(id) {
        return Err(DecideRefusal::Reviewing(id.to_owned()));
    }
    let rebuilt = rebuild(ctx, id, request).map_err(|why| DecideRefusal::Invalid {
        id: id.to_owned(),
        why,
    })?;
    Ok((request, rebuilt.class))
}

/// The `auto` request a review or the rule may approve now, rebuilt, or
/// why not (AD-86, D-157): as [`decidable`], except that its own
/// `reviewing` mark does not stop it, and the request must rebuild as an
/// `auto` action of the approved catalogue, which refuses one whose
/// catalogue changed while it was reviewed.
pub fn review_decidable<'f>(
    files: &'f ApprovalFiles,
    ctx: &ApprovalContext<'_>,
    id: &str,
    now_ms: i64,
) -> Result<(&'f ApprovalRequest, Rebuilt), DecideRefusal> {
    let request = files
        .requests
        .get(id)
        .ok_or_else(|| DecideRefusal::Unknown(id.to_owned()))?;
    if files.decided(id) || files.markers.contains(id) {
        return Err(DecideRefusal::Decided(id.to_owned()));
    }
    if !is_live(request.created_at_ms, now_ms) {
        return Err(DecideRefusal::Expired(id.to_owned()));
    }
    let rebuilt = rebuild(ctx, id, request).map_err(|why| DecideRefusal::Invalid {
        id: id.to_owned(),
        why,
    })?;
    if rebuilt.class != ActionClass::Auto {
        return Err(DecideRefusal::NotAuto(id.to_owned()));
    }
    Ok((request, rebuilt))
}

/// Whether the author's decision carries the reason its class asks for
/// (FR-9, AD-86): rejecting an `auto` request needs one, a `human`
/// rejection and any approval take none.
///
/// ```
/// use officina_core::registry::types::AuthorReason;
/// use officina_core::relay::approvals::reason_fits;
/// use officina_core::relay::catalogue::ActionClass;
/// assert!(reason_fits(ActionClass::Auto, false, Some(AuthorReason::NotNow)).is_ok());
/// assert!(reason_fits(ActionClass::Auto, false, None).is_err());
/// assert!(reason_fits(ActionClass::Human, false, None).is_ok());
/// assert!(reason_fits(ActionClass::Human, true, Some(AuthorReason::NotNow)).is_err());
/// ```
pub fn reason_fits(
    class: ActionClass,
    approve: bool,
    reason: Option<AuthorReason>,
) -> Result<(), &'static str> {
    match (approve, class, reason) {
        (true, _, Some(_)) => Err("an approval takes no reason"),
        (false, ActionClass::Auto, None) => {
            Err("rejecting an auto request needs a reason: it should not run, or not now")
        }
        (false, ActionClass::Auto, Some(_)) | (_, _, None) => Ok(()),
        (false, _, Some(_)) => Err("only an auto request's rejection takes a reason"),
    }
}

/// The dry run a card shows (D-170): the check, its run, and its kept
/// output or why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DryRunView {
    pub check: String,
    pub run_id: String,
    pub output: Option<String>,
    /// Why there is no output, when there is none.
    pub missing: Option<String>,
}

/// What a request's files are bound to (D-170, D-173): what the run checks
/// again before it uses the approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TreeBinding {
    /// The files the dry run ran on.
    DryRun,
    /// The files when the request was made, for an action naming no check.
    Request,
    /// None: the project is not a git work tree.
    NotGitTree,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a pending request stands (AD-92, D-157, D-571).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ApprovalState {
        /// It waits on the author.
        Pending,
        /// A live review is deciding it: it waits on the review, not on the
        /// author, who cannot decide it until it escalates.
        Reviewing,
    }
}

/// One pending request as Needs you shows it (AD-35, D-27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ApprovalView {
    pub id: String,
    pub project: String,
    pub action: String,
    /// `human` or `auto`; for an invalid request, the class its action has
    /// in the approved catalogue when it waits for an approval, else
    /// `human`.
    pub class: ActionClass,
    /// [`ApprovalState::Pending`] unless a live review is deciding it.
    pub state: ApprovalState,
    /// Why a pending `auto` request is the author's, as the board shows it
    /// (AD-87): before he decides, only a cause found before any call or
    /// `auto_approval_off`, never what a review answered (D-143); `None`
    /// for a `human` request, or before the window looked at it.
    pub escalation: Option<EscalationCause>,
    /// From the approved catalogue; empty for an invalid request.
    pub description: String,
    /// Rebuilt from the approved catalogue (D-27); empty for an invalid
    /// request, whose own command is never shown, and for an HTTP action.
    pub argv: Vec<String>,
    /// An HTTP action's request, rebuilt from the approved catalogue (D-27).
    pub http: Option<HttpRequestView>,
    pub valid: bool,
    pub invalid_reason: Option<String>,
    pub session_id: Option<String>,
    /// The name of the card with this session id, when there is one.
    pub session_name: Option<Untrusted<String>>,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
    pub dry_run: Option<DryRunView>,
    /// What the request's files are bound to; absent for an invalid request.
    pub binding: Option<TreeBinding>,
    /// How many tracked and unignored files the request binds.
    pub bound_files: Option<u32>,
    /// True while the project's fingerprint equals the request's, false once
    /// it differs (approving would then be refused at the run), absent when
    /// unknown or not bound.
    pub files_unchanged: Option<bool>,
}

impl ApprovalView {
    /// True for a valid request in [`ApprovalState::Pending`]: one the author
    /// can decide now. Only these count as waiting, on the board and in the
    /// scene (AD-92, D-571).
    pub fn waits_on_the_author(&self) -> bool {
        self.valid && self.state == ApprovalState::Pending
    }
}

/// What the host found for the dry runs of the pending requests (D-170).
#[derive(Debug, Clone, Default)]
pub struct DryRunInputs {
    /// Kept output by run id, or why it cannot be read; a run id absent here
    /// has no kept output.
    pub outputs: BTreeMap<String, Result<String, String>>,
    /// The current fingerprint of each project computed, by name: `None`
    /// outside a git work tree. A project absent here was not computed.
    pub trees: BTreeMap<String, Option<TreeFingerprint>>,
}

/// Whether the project's files are still the ones `request` is bound to,
/// when both are known.
fn files_unchanged(request: &ApprovalRequest, inputs: &DryRunInputs) -> Option<bool> {
    match (&request.tree, inputs.trees.get(&request.project)) {
        (Some(bound), Some(current)) => Some(current.as_ref() == Some(bound)),
        _ => None,
    }
}

fn dry_run_view(request: &ApprovalRequest, check: &str, inputs: &DryRunInputs) -> DryRunView {
    let run_id = request.check_run_id.clone().unwrap_or_default();
    let (output, missing) = match inputs.outputs.get(&run_id) {
        Some(Ok(text)) => (Some(text.clone()), None),
        Some(Err(why)) => (None, Some(why.clone())),
        None => (
            None,
            Some(
                "the dry run's output is not kept (pruned after 7 days, or older than this build)"
                    .to_owned(),
            ),
        ),
    };
    DryRunView {
        check: check.to_owned(),
        run_id,
        output,
        missing,
    }
}

/// The requests Needs you lists (AD-35, FR-9): pending (no decision, not
/// expired, not dated in the future), each rebuilt from the approved
/// catalogue or marked invalid with the reason, oldest first. Session names
/// are filled by the board from its cards.
pub fn pending(
    files: &ApprovalFiles,
    ctx: &ApprovalContext<'_>,
    dry_runs: &DryRunInputs,
    now_ms: i64,
) -> Vec<ApprovalView> {
    let mut views: Vec<ApprovalView> = files
        .requests
        .iter()
        .filter(|(id, r)| !files.decided(id) && is_live(r.created_at_ms, now_ms))
        .map(|(id, request)| {
            let rebuilt = rebuild(ctx, id, request);
            let (description, argv, http, dry_run) = match &rebuilt {
                Ok(rebuilt) => (
                    rebuilt.description.clone(),
                    rebuilt.argv.clone(),
                    rebuilt.http.clone(),
                    rebuilt
                        .check
                        .as_deref()
                        .map(|check| dry_run_view(request, check, dry_runs)),
                ),
                Err(_) => (String::new(), Vec::new(), None, None),
            };
            let binding =
                rebuilt
                    .as_ref()
                    .ok()
                    .map(|rebuilt| match (&request.tree, &rebuilt.check) {
                        (None, _) => TreeBinding::NotGitTree,
                        (Some(_), Some(_)) => TreeBinding::DryRun,
                        (Some(_), None) => TreeBinding::Request,
                    });
            let class = match &rebuilt {
                Ok(rebuilt) => rebuilt.class,
                Err(_) => ctx
                    .approved
                    .and_then(|a| a.canonical.actions.get(&request.action))
                    .map(|a| a.class)
                    .filter(|class| waits_for_approval(*class))
                    .unwrap_or(ActionClass::Human),
            };
            ApprovalView {
                id: id.clone(),
                project: request.project.clone(),
                action: request.action.clone(),
                class,
                state: if files.reviewing.contains(id) {
                    ApprovalState::Reviewing
                } else {
                    ApprovalState::Pending
                },
                escalation: None,
                description,
                argv,
                http,
                valid: rebuilt.is_ok(),
                invalid_reason: rebuilt.err(),
                session_id: request.session_id.clone(),
                session_name: None,
                requested_at_ms: request.created_at_ms,
                expires_at_ms: request.created_at_ms + PENDING_MS,
                dry_run,
                binding,
                bound_files: binding
                    .filter(|b| *b != TreeBinding::NotGitTree)
                    .and_then(|_| request.tree.as_ref().map(|t| t.files)),
                files_unchanged: binding.and_then(|_| files_unchanged(request, dry_runs)),
            }
        })
        .collect();
    views.sort_by(|a, b| {
        a.requested_at_ms
            .cmp(&b.requested_at_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    views
}

/// The file a check run's output is kept in: `<run_id>.txt`.
pub fn kept_output_name(run_id: &str) -> String {
    format!("{run_id}.txt")
}

/// The id `officina run` gives a run: its start time and its process id,
/// in lowercase hex, joined by `-`.
pub fn run_id(started_ms: i64, process_id: u32) -> String {
    format!("{started_ms:x}-{process_id:x}")
}

/// True for an id [`run_id`] can give: what a run id read from a request
/// file, which any process can write, must be before it names a kept
/// output, so it can never lead out of the outputs folder (D-175).
///
/// ```
/// use officina_core::relay::approvals::{is_run_id, run_id};
/// assert!(is_run_id(&run_id(1_793_102_400_000, 4242)));
/// assert!(!is_run_id("C:/path/to/notes"));
/// assert!(!is_run_id(r"\\192.0.2.10\x\y"));
/// ```
pub fn is_run_id(text: &str) -> bool {
    let hex = |part: &str| {
        (1..=16).contains(&part.len())
            && part
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    text.split_once('-')
        .is_some_and(|(time, process)| hex(time) && hex(process))
}

/// What a check run keeps (D-170): its stdout then its stderr, at most
/// [`KEPT_OUTPUT_BYTES`]; past it, a last line says how many bytes were
/// dropped. `stdout` and `stderr` are what was captured, the totals what
/// the streams carried.
///
/// ```
/// use officina_core::relay::approvals::{KEPT_OUTPUT_BYTES, kept_output};
/// assert_eq!(kept_output(b"out\n", b"err\n", 4, 4), b"out\nerr\n");
/// let big = vec![b'x'; KEPT_OUTPUT_BYTES];
/// let kept = kept_output(&big, b"tail", KEPT_OUTPUT_BYTES as u64, 4);
/// assert!(kept.ends_with(b"\n[officina: output cut at 256 KiB, 4 bytes dropped]\n"));
/// ```
pub fn kept_output(stdout: &[u8], stderr: &[u8], stdout_total: u64, stderr_total: u64) -> Vec<u8> {
    let mut kept: Vec<u8> = stdout
        .iter()
        .chain(stderr)
        .take(KEPT_OUTPUT_BYTES)
        .copied()
        .collect();
    let dropped = (stdout_total + stderr_total).saturating_sub(kept.len() as u64);
    if dropped > 0 {
        if !kept.ends_with(b"\n") {
            kept.push(b'\n');
        }
        kept.extend_from_slice(
            format!(
                "[officina: output cut at {} KiB, {dropped} bytes dropped]\n",
                KEPT_OUTPUT_BYTES / 1024
            )
            .as_bytes(),
        );
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::catalogue::load;

    const NOW: i64 = 1_793_102_400_000;
    const MIN: i64 = 60_000;
    use crate::relay::fixtures::{FLOOR_PATH, params};
    const SYSTEM: &str = r"C:\Windows\System32";
    const FLOOR: &str = "schema = 2
[[project]]
name = \"lab\"
path = \"Lab\"
actions = [\"restart\", \"remote\", \"verify\", \"apply\"]
[[action]]
name = \"restart\"
description = \"Restart one service\"
class = \"human\"
program = \"C:/path/to/tool.exe\"
args = [\"restart\", \"{service}\"]
[action.params.service]
values = [\"web01\", \"db01\"]
[[action]]
name = \"remote\"
class = \"human\"
runner = \"wsl\"
program = \"/usr/bin/tool\"
args = [\"{service}\"]
[action.params.service]
values = [\"web01\"]
[[action]]
name = \"verify\"
class = \"free\"
program = \"C:/path/to/tool.exe\"
args = [\"--check\", \"{group}\"]
[action.params.group]
pattern = \"lab-[a-z]+\"
[[action]]
name = \"apply\"
class = \"human\"
check = \"verify\"
program = \"C:/path/to/tool.exe\"
args = [\"{group}\"]
[action.params.group]
pattern = \"lab-[a-z]+\"
";

    fn approved() -> ApprovedCatalogue {
        let relay = load(FLOOR, FLOOR_PATH).unwrap();
        assert!(
            relay.catalogue.issues.is_empty(),
            "{:?}",
            relay.catalogue.issues
        );
        ApprovedCatalogue::approving(&relay, FLOOR_PATH, NOW - 60 * MIN)
    }

    fn dirs() -> BTreeMap<String, String> {
        BTreeMap::from([("lab".to_owned(), "C:/path/to/Lab".to_owned())])
    }

    fn tree(text: &str) -> TreeFingerprint {
        TreeFingerprint {
            sha256: crate::relay::sha256_hex(text.as_bytes()),
            files: 0,
            git: None,
        }
    }

    /// A request as `officina run` writes it, with the command it starts.
    fn request(
        approved: &ApprovedCatalogue,
        action: &str,
        pairs: &[(&str, &str)],
        argv: &[&str],
        created_at_ms: i64,
    ) -> (String, ApprovalRequest) {
        let params = params(pairs);
        let check_run_id = (action == "apply").then(|| "run-1".to_owned());
        let bound = (action == "apply").then(|| tree("a"));
        let key = request_key(&KeyInputs {
            project: "lab",
            action,
            params: &params,
            catalogue_hash: &approved.hash,
            check_run_id: check_run_id.as_deref(),
            tree: bound.as_ref(),
        });
        let id = request_id(&key, created_at_ms);
        let request = ApprovalRequest {
            request_key: key,
            created_at_ms,
            project: "lab".to_owned(),
            action: action.to_owned(),
            params,
            argv: argv.iter().map(|a| (*a).to_owned()).collect(),
            http: None,
            catalogue_hash: approved.hash.clone(),
            session_id: Some("s1".to_owned()),
            check_run_id,
            tree: bound,
        };
        (id, request)
    }

    fn files_with(requests: &[(String, ApprovalRequest)]) -> ApprovalFiles {
        ApprovalFiles {
            requests: requests.iter().cloned().collect(),
            ..Default::default()
        }
    }

    fn key(project: &str, action: &str, p: &BTreeMap<String, String>, hash: &str) -> String {
        request_key(&KeyInputs {
            project,
            action,
            params: p,
            catalogue_hash: hash,
            check_run_id: None,
            tree: None,
        })
    }

    #[test]
    fn key_is_stable_across_param_order() {
        let mut forward = BTreeMap::new();
        forward.insert("a".to_owned(), "1".to_owned());
        forward.insert("b".to_owned(), "2".to_owned());
        let mut backward = BTreeMap::new();
        backward.insert("b".to_owned(), "2".to_owned());
        backward.insert("a".to_owned(), "1".to_owned());
        assert_eq!(
            key("lab", "restart", &forward, "h"),
            key("lab", "restart", &backward, "h")
        );
        let k = key("lab", "restart", &forward, "h");
        assert_eq!(k.len(), 64);
        assert!(
            k.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
    }

    #[test]
    fn key_changes_with_each_component() {
        let p = params(&[("service", "web01")]);
        let base = key("lab", "restart", &p, "h");
        let others = [
            key("lab2", "restart", &p, "h"),
            key("lab", "remote", &p, "h"),
            key("lab", "restart", &params(&[("service", "db01")]), "h"),
            key("lab", "restart", &params(&[("other", "web01")]), "h"),
            key("lab", "restart", &p, "h2"),
            request_key(&KeyInputs {
                project: "lab",
                action: "restart",
                params: &p,
                catalogue_hash: "h",
                check_run_id: Some("run-1"),
                tree: None,
            }),
            request_key(&KeyInputs {
                project: "lab",
                action: "restart",
                params: &p,
                catalogue_hash: "h",
                check_run_id: None,
                tree: Some(&tree("a")),
            }),
        ];
        for other in &others {
            assert_ne!(other, &base);
        }
        let unique: std::collections::BTreeSet<&String> = others.iter().collect();
        assert_eq!(unique.len(), others.len());
    }

    #[test]
    fn id_format() {
        let k = key("lab", "restart", &params(&[]), "h");
        let id = request_id(&k, 1_793_102_400_123);
        assert_eq!(id, format!("{}-1793102400123", &k[..ID_KEY_HEX]));
        assert!(is_valid_id(&id));
        for name in FileKind::ALL {
            assert_eq!(
                parse_file_name(&file_name(&id, *name)),
                Some((id.as_str(), *name))
            );
        }
        for bad in [
            "0123-1",
            "0123456789ABCDEF-1",
            "0123456789abcdef-",
            "0123456789abcdef-1x",
        ] {
            assert!(!is_valid_id(bad), "{bad}");
        }
        assert_eq!(parse_file_name("../0123456789abcdef-1.request.json"), None);
    }

    #[test]
    fn view_argv_is_rebuilt_from_the_catalogue() {
        let approved = approved();
        let dirs = dirs();
        let ctx = ApprovalContext {
            approved: Some(&approved),
            project_dirs: &dirs,
            system_dir: SYSTEM,
        };
        let native = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &["C:/path/to/tool.exe", "restart", "web01"],
            NOW - MIN,
        );
        let wsl = request(
            &approved,
            "remote",
            &[("service", "web01")],
            &[
                r"C:\Windows\System32\wsl.exe",
                "--cd",
                "C:/path/to/Lab",
                "--exec",
                "/usr/bin/env",
                "/usr/bin/timeout",
                "-k",
                "10",
                "545",
                "/usr/bin/tool",
                "web01",
            ],
            NOW - 2 * MIN,
        );
        let views = pending(
            &files_with(&[native, wsl.clone()]),
            &ctx,
            &DryRunInputs::default(),
            NOW,
        );
        assert_eq!(views.len(), 2);
        assert!(views.iter().all(|v| v.valid), "{views:#?}");
        assert_eq!(views[0].action, "remote", "oldest first");
        assert_eq!(
            views[0].argv, wsl.1.argv,
            "the folder as the floor declares it (D-6)"
        );
        assert_eq!(views[1].argv, ["C:/path/to/tool.exe", "restart", "web01"]);
        assert_eq!(views[1].description, "Restart one service");
        assert_eq!(views[1].expires_at_ms, NOW - MIN + PENDING_MS);

        let tampered = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &["C:/Windows/System32/cmd.exe", "/c", "harmless"],
            NOW - MIN,
        );
        let views = pending(
            &files_with(&[tampered]),
            &ctx,
            &DryRunInputs::default(),
            NOW,
        );
        assert!(!views[0].valid);
        assert!(
            views[0].argv.is_empty(),
            "the stored command is never shown"
        );
        assert!(
            views[0]
                .invalid_reason
                .as_deref()
                .unwrap()
                .contains("stored command"),
            "{views:?}"
        );
    }

    #[test]
    fn mismatched_key_makes_the_request_invalid() {
        let approved = approved();
        let dirs = dirs();
        let ctx = ApprovalContext {
            approved: Some(&approved),
            project_dirs: &dirs,
            system_dir: SYSTEM,
        };
        let (id, mut forged) = request(
            &approved,
            "restart",
            &[("service", "db01")],
            &["C:/path/to/tool.exe", "restart", "web01"],
            NOW - MIN,
        );
        // A key of another parameter set, with a harmless stored command.
        forged.params = params(&[("service", "web01")]);
        let files = files_with(&[(id.clone(), forged)]);
        let views = pending(&files, &ctx, &DryRunInputs::default(), NOW);
        assert!(!views[0].valid);
        assert!(views[0].argv.is_empty());
        assert!(views[0].invalid_reason.as_deref().unwrap().contains("key"));
        assert!(matches!(
            decidable(&files, &ctx, &id, NOW),
            Err(DecideRefusal::Invalid { .. })
        ));

        let mut stale = approved.clone();
        stale.hash = "another".to_owned();
        let (id, request) = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &["C:/path/to/tool.exe", "restart", "web01"],
            NOW - MIN,
        );
        let stale_ctx = ApprovalContext {
            approved: Some(&stale),
            ..ctx
        };
        let files = files_with(&[(id.clone(), request)]);
        assert!(matches!(
            decidable(&files, &stale_ctx, &id, NOW),
            Err(DecideRefusal::Invalid { why, .. }) if why.contains("another catalogue")
        ));
    }

    #[test]
    fn a_request_must_carry_the_dry_run_its_action_names() {
        let approved = approved();
        let dirs = dirs();
        let ctx = ApprovalContext {
            approved: Some(&approved),
            project_dirs: &dirs,
            system_dir: SYSTEM,
        };
        let (id, request) = request(
            &approved,
            "apply",
            &[("group", "lab-web")],
            &["C:/path/to/tool.exe", "lab-web"],
            NOW - MIN,
        );
        let mut inputs = DryRunInputs::default();
        inputs
            .outputs
            .insert("run-1".to_owned(), Ok("changed: 1\n".to_owned()));
        inputs.trees.insert("lab".to_owned(), Some(tree("a")));
        let view = &pending(
            &files_with(&[(id.clone(), request.clone())]),
            &ctx,
            &inputs,
            NOW,
        )[0];
        assert!(view.valid, "{view:?}");
        let dry_run = view.dry_run.as_ref().unwrap();
        assert_eq!(
            (
                dry_run.check.as_str(),
                dry_run.output.as_deref(),
                view.files_unchanged
            ),
            ("verify", Some("changed: 1\n"), Some(true))
        );
        assert_eq!(view.binding, Some(TreeBinding::DryRun));
        inputs.trees.insert("lab".to_owned(), Some(tree("b")));
        inputs.outputs.clear();
        let view = &pending(&files_with(&[(id, request.clone())]), &ctx, &inputs, NOW)[0];
        let dry_run = view.dry_run.as_ref().unwrap();
        assert_eq!(view.files_unchanged, Some(false));
        assert!(dry_run.output.is_none() && dry_run.missing.is_some());

        let mut without = request;
        without.check_run_id = None;
        without.tree = None;
        without.request_key = request_key(&without.key_inputs());
        let id = request_id(&without.request_key, without.created_at_ms);
        let view = &pending(&files_with(&[(id, without)]), &ctx, &inputs, NOW)[0];
        assert!(!view.valid);
        assert!(view.invalid_reason.as_deref().unwrap().contains("dry run"));
    }

    /// D-173: a request names what its files are bound to: its dry run's,
    /// its own when the action names no check, or none outside git; an
    /// invalid request names nothing.
    #[test]
    fn a_request_states_its_binding() {
        let approved = approved();
        let dirs = dirs();
        let ctx = ApprovalContext {
            approved: Some(&approved),
            project_dirs: &dirs,
            system_dir: SYSTEM,
        };
        let argv = ["C:/path/to/tool.exe", "restart", "web01"];
        let (outside_id, outside) = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &argv,
            NOW - MIN,
        );
        let mut bound = outside.clone();
        bound.tree = Some(TreeFingerprint {
            files: 7,
            ..tree("a")
        });
        bound.created_at_ms = NOW - 2 * MIN;
        bound.request_key = request_key(&bound.key_inputs());
        let bound_id = request_id(&bound.request_key, bound.created_at_ms);
        let mut inputs = DryRunInputs::default();
        let files = files_with(&[(outside_id, outside), (bound_id, bound)]);
        let views = pending(&files, &ctx, &inputs, NOW);
        assert_eq!(
            views
                .iter()
                .map(|v| (v.binding, v.bound_files, v.files_unchanged))
                .collect::<Vec<_>>(),
            [
                (Some(TreeBinding::Request), Some(7), None),
                (Some(TreeBinding::NotGitTree), None, None),
            ],
            "oldest first; the current files not computed yet"
        );
        inputs.trees.insert(
            "lab".to_owned(),
            Some(TreeFingerprint {
                files: 7,
                ..tree("a")
            }),
        );
        assert_eq!(
            pending(&files, &ctx, &inputs, NOW)[0].files_unchanged,
            Some(true)
        );
        inputs.trees.insert("lab".to_owned(), None);
        assert_eq!(
            pending(&files, &ctx, &inputs, NOW)[0].files_unchanged,
            Some(false),
            "no longer a work tree"
        );

        let (id, mut forged) = request(
            &approved,
            "restart",
            &[("service", "db01")],
            &argv,
            NOW - MIN,
        );
        forged.params = params(&[("service", "web01")]);
        let view = &pending(&files_with(&[(id, forged)]), &ctx, &inputs, NOW)[0];
        assert!(!view.valid);
        assert_eq!((view.binding, view.files_unchanged), (None, None));
    }

    #[test]
    fn decided_expired_and_future_requests_are_not_pending() {
        let approved = approved();
        let dirs = dirs();
        let ctx = ApprovalContext {
            approved: Some(&approved),
            project_dirs: &dirs,
            system_dir: SYSTEM,
        };
        let argv = ["C:/path/to/tool.exe", "restart", "web01"];
        let live = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &argv,
            NOW - MIN,
        );
        let old = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &argv,
            NOW - PENDING_MS - 1,
        );
        let future = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &argv,
            NOW + MIN,
        );
        let mut files = files_with(&[live.clone(), old.clone(), future.clone()]);
        let pending_ids: Vec<String> = pending(&files, &ctx, &DryRunInputs::default(), NOW)
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(pending_ids, [live.0.as_str()]);
        assert_eq!(
            decidable(&files, &ctx, &old.0, NOW),
            Err(DecideRefusal::Expired(old.0.clone()))
        );
        assert_eq!(
            decidable(&files, &ctx, "0123456789abcdef-1", NOW),
            Err(DecideRefusal::Unknown("0123456789abcdef-1".to_owned()))
        );
        assert_eq!(
            decidable(&files, &ctx, &live.0, NOW).map(|(r, class)| (r.created_at_ms, class)),
            Ok((NOW - MIN, ActionClass::Human))
        );
        let mut marked = files.clone();
        marked.markers.insert(live.0.clone());
        assert_eq!(
            decidable(&marked, &ctx, &live.0, NOW),
            Err(DecideRefusal::Decided(live.0.clone())),
            "a marker another decider created refuses the second (D-157)"
        );
        let mut reviewing = files.clone();
        reviewing.reviewing.insert(live.0.clone());
        assert_eq!(
            decidable(&reviewing, &ctx, &live.0, NOW),
            Err(DecideRefusal::Reviewing(live.0.clone())),
            "a live review is deciding it (D-157)"
        );
        assert_eq!(
            pending(&reviewing, &ctx, &DryRunInputs::default(), NOW)
                .iter()
                .find(|view| view.id == live.0)
                .map(|view| view.state),
            Some(ApprovalState::Reviewing)
        );
        files.consumed.insert(
            live.0.clone(),
            Decision::by_author(NOW, &live.1.request_key, None),
        );
        assert!(pending(&files, &ctx, &DryRunInputs::default(), NOW).is_empty());
        assert_eq!(
            decidable(&files, &ctx, &live.0, NOW),
            Err(DecideRefusal::Decided(live.0.clone()))
        );
    }

    /// AD-86: an approval file without `by`, as v0.4 wrote it, reads as the
    /// author's.
    #[test]
    fn an_approval_without_by_reads_as_author() {
        let old: Decision =
            serde_json::from_str(r#"{"decided_at_ms": 1, "request_key": "ab"}"#).unwrap();
        assert_eq!(old.by, None);
        assert_eq!(old.by(), ApprovalBy::Author);
        assert_eq!(old.review_id, None);
        assert_eq!(old.reason, None);
    }

    /// AD-86: `by`, `review_id` and the reason survive a write and a read,
    /// and an unknown field is still refused.
    #[test]
    fn by_and_review_id_round_trip() {
        let by_review = Decision {
            decided_at_ms: NOW,
            request_key: "ab".to_owned(),
            by: Some(ApprovalBy::Review),
            review_id: Some("7".to_owned()),
            reason: None,
        };
        let text = serde_json::to_string(&by_review).unwrap();
        assert!(text.contains(r#""by":"review""#), "{text}");
        assert_eq!(serde_json::from_str::<Decision>(&text).unwrap(), by_review);
        let rejected = Decision::by_author(NOW, "ab", Some(AuthorReason::ShouldNotRun));
        let text = serde_json::to_string(&rejected).unwrap();
        assert!(
            text.contains(r#""by":"author""#) && text.contains(r#""reason":"should_not_run""#),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Decision>(&text).unwrap(), rejected);
        assert!(
            serde_json::from_str::<Decision>(
                r#"{"decided_at_ms": 1, "request_key": "ab", "extra": 1}"#
            )
            .is_err()
        );
    }

    /// The marker's file name parses like the others (D-157).
    #[test]
    fn the_decision_marker_is_a_file_of_the_request() {
        assert_eq!(
            parse_file_name("0123456789abcdef-17.decided"),
            Some(("0123456789abcdef-17", FileKind::Decided))
        );
    }

    #[test]
    fn next_step_follows_ad_33() {
        let approved = approved();
        let argv = ["C:/path/to/tool.exe", "restart", "web01"];
        let (id, request) = request(
            &approved,
            "restart",
            &[("service", "web01")],
            &argv,
            NOW - 5 * MIN,
        );
        let key = request.request_key.clone();
        let decision = |at: i64| Decision::by_author(at, &key, None);
        let mut files = ApprovalFiles::default();
        assert_eq!(next_step(&files, &key, NOW), NextStep::Create);
        files.requests.insert(id.clone(), request.clone());
        assert_eq!(
            next_step(&files, &key, NOW),
            NextStep::Wait { id: id.clone() }
        );
        assert_eq!(
            next_step(&files, &key, NOW - 5 * MIN + PENDING_MS + 1),
            NextStep::Create,
            "an expired request is not reused"
        );
        files.approved.insert(id.clone(), decision(NOW - MIN));
        assert_eq!(
            next_step(&files, &key, NOW),
            NextStep::Consume { id: id.clone() }
        );
        assert_eq!(
            next_step(&files, &key, NOW - MIN + DECISION_VALID_MS),
            NextStep::Create,
            "an approval 15 minutes old is not used, and its request is decided"
        );
        files.consumed.insert(id.clone(), decision(NOW - MIN));
        assert_eq!(
            next_step(&files, &key, NOW),
            NextStep::Create,
            "both names left by a crash: used"
        );
        files.approved.clear();
        assert_eq!(next_step(&files, &key, NOW), NextStep::Create, "single use");
        files.rejected.insert(id.clone(), decision(NOW - MIN));
        assert_eq!(next_step(&files, &key, NOW), NextStep::Rejected { id });
        assert_eq!(next_step(&files, "another key", NOW), NextStep::Create);
    }
}
