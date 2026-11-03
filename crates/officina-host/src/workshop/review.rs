//! The review view's host half (AD-121, D-239): the collector hands the
//! stored facts and journals the first Receive; thread `workshop` runs
//! host git in the run's scratch repository for the commits, the file
//! list, the diff and each sensitive group's hunks, every text with the
//! credential's forms replaced (D-230), and keeps what it showed per run.
//! An approval (AD-113) is checked on thread `workshop` against those
//! hunks, recomputed when the window restarted, and recorded by the
//! collector.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use officina_core::ipc::IpcError;
use officina_core::meeting::{DIFF_CAP_BYTES, shape_diff};
use officina_core::relay::journal::{JournalEntry, ReviewOpened, SensitiveApproved};
use officina_core::untrusted::Untrusted;
use officina_core::workshop::package::QuarantineCause;
use officina_core::workshop::review::{
    ApprovalAnswer, ApprovalRefusal, PackageDisplay, REVIEW_LOG_FORMAT, ReviewFacts, ReviewInputs,
    ReviewView, ShownFile, ShownHunks, approval_refusal, parse_log,
};
use officina_core::workshop::run::{Recorded, RunState, WorkshopOutcome};
use officina_core::workshop::scan::{Needles, replace};
use officina_core::workshop::sensitive::{
    ChangedFile, MAX_GROUP_HUNK_BYTES, PackageGroup, hunks_sha256,
};

use crate::collector::Collector;
use crate::registry::window::WindowRegistry;
use crate::registry::workshop::RunRecord;
use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, CollectorMsg, ask};
use crate::workshop::inventory::WorkshopGit;
use crate::workshop::launch::{RunNeedles, WorkshopRuntime, credential_store_name, run_dir};
use crate::workshop::verify::{SCRATCH_DIR, package_changes};

/// What host git showed of each run's package, by run id: computed once
/// per window, so a second Receive or an approval runs no git (D-239).
pub type ReviewCache = Arc<Mutex<BTreeMap<String, PackageDisplay>>>;

/// What the collector holds of a run's review: its row and the groups
/// approved so far, each with the hash it was approved for.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredReview {
    pub row: RunRecord,
    pub approvals: Vec<(String, String)>,
}

/// An approval thread `workshop` checked against the hunks, for the
/// collector to record (AD-113).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveApproval {
    pub run_id: String,
    pub group_key: String,
    pub hunks_sha256: String,
}

fn stored(registry: &mut WindowRegistry, id: &str) -> Result<AuthorAnswer, IpcError> {
    let Some(row) = registry.workshop_run(id)? else {
        return Ok(AuthorAnswer::WorkshopReview(None));
    };
    let approvals = registry.workshop_approvals(&row.id)?;
    Ok(AuthorAnswer::WorkshopReview(Some(Box::new(StoredReview {
        row,
        approvals,
    }))))
}

fn journal(collector: &Collector, line: JournalEntry, now_ms: i64) -> Result<(), IpcError> {
    crate::relay::journal::record(collector.relay(), &[line], now_ms).map_err(|e| {
        IpcError::new(
            officina_core::ipc::IpcErrorCode::JournalUnavailable,
            e.to_string(),
        )
    })
}

/// The collector's half of Receive (AD-120, D-249): run `id`'s row and
/// approvals, and `review_opened` journaled at a package's first Receive.
pub(crate) fn open_on_collector(
    collector: &Collector,
    registry: &mut WindowRegistry,
    id: &str,
    now_ms: i64,
) -> Result<AuthorAnswer, IpcError> {
    let row = registry.workshop_run(id)?;
    if let Some(row) = row
        .as_ref()
        .filter(|r| r.outcome == Some(WorkshopOutcome::Package))
        && registry.open_workshop_review(&row.id, now_ms)?
    {
        let line = JournalEntry::ReviewOpened(ReviewOpened {
            run_id: row.id.clone(),
        });
        journal(collector, line, now_ms)?;
    }
    stored(registry, id)
}

/// The collector's read of run `id`'s review for an approval: nothing
/// journaled.
pub(crate) fn stored_on_collector(
    registry: &mut WindowRegistry,
    id: &str,
) -> Result<AuthorAnswer, IpcError> {
    stored(registry, id)
}

/// The collector's half of an approval (AD-113): the row read again, still
/// in `review` and holding the group, then the approval stored and
/// `sensitive_approved` journaled.
pub(crate) fn record_on_collector(
    collector: &Collector,
    registry: &mut WindowRegistry,
    approval: &SensitiveApproval,
    now_ms: i64,
) -> Result<AuthorAnswer, IpcError> {
    let refused = |why| Ok(AuthorAnswer::Approval(ApprovalAnswer::Refused(why)));
    let Some(row) = registry.workshop_run(&approval.run_id)? else {
        return refused(ApprovalRefusal::UnknownRun);
    };
    if let Some(why) = stored_refusal(&row, &approval.group_key) {
        return refused(why);
    }
    registry.approve_workshop_group(
        &row.id,
        &approval.group_key,
        &approval.hunks_sha256,
        now_ms,
    )?;
    let line = JournalEntry::SensitiveApproved(SensitiveApproved {
        run_id: row.id.clone(),
        group: approval.group_key.clone(),
        hunks_sha256: approval.hunks_sha256.clone(),
    });
    journal(collector, line, now_ms)?;
    Ok(AuthorAnswer::Approval(ApprovalAnswer::Approved))
}

/// The groups collection recorded for `row` (AD-113); none for a run that
/// is not a package that passed every check.
pub fn groups_of(row: &RunRecord) -> Vec<PackageGroup> {
    row.groups_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

/// What the stored row alone refuses of an approval of `group_key`.
fn stored_refusal(row: &RunRecord, group_key: &str) -> Option<ApprovalRefusal> {
    if row.state != RunState::Review {
        return Some(ApprovalRefusal::NotInReview);
    }
    (!groups_of(row).iter().any(|g| g.key == group_key)).then_some(ApprovalRefusal::UnknownGroup)
}

/// The approved floor's key, for the credential's needles (D-230).
pub(crate) fn floor_key(
    sender: &std::sync::mpsc::Sender<CollectorMsg>,
) -> Result<Option<String>, IpcError> {
    Ok(
        match ask(sender, AuthorCommand::WorkshopRowsToReconcile, AUTHOR_WAIT)? {
            AuthorAnswer::WorkshopRows(rows) => rows.1,
            _ => None,
        },
    )
}

/// Runs `job` on thread `workshop` and waits for what it sends.
pub(crate) fn on_workshop<T: Send + 'static>(
    runtime: &WorkshopRuntime,
    job: impl FnOnce() -> T + Send + 'static,
) -> Result<T, IpcError> {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let submitted = runtime.thread.submit(move || {
        let _ = done_tx.send(job());
    });
    if !submitted {
        return Err(IpcError::internal("thread workshop has stopped"));
    }
    done_rx
        .recv()
        .map_err(|_| IpcError::internal("thread workshop has stopped"))
}

/// Receive on run `run_id`'s review (AD-121): the stored facts from the
/// collector, host git on thread `workshop`.
pub fn open_review(
    sender: &std::sync::mpsc::Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    home: &Path,
    base_env: &[(String, String)],
    run_id: &str,
) -> Result<ReviewView, IpcError> {
    let stored = match ask(
        sender,
        AuthorCommand::OpenReview(run_id.to_owned()),
        AUTHOR_WAIT,
    )? {
        AuthorAnswer::WorkshopReview(Some(stored)) => *stored,
        AuthorAnswer::WorkshopReview(None) => {
            return Err(IpcError::new(
                officina_core::ipc::IpcErrorCode::Refused,
                format!("no run {run_id}"),
            ));
        }
        other => return Err(IpcError::internal(format!("unexpected answer {other:?}"))),
    };
    let floor_key = floor_key(sender)?;
    let job = runtime.clone();
    let home = home.to_path_buf();
    let base_env = base_env.to_vec();
    on_workshop(runtime, move || {
        let display = display_of(&job, &home, &base_env, &stored.row, floor_key.as_deref());
        ReviewView::build(inputs_of(&stored), display)
    })
}

/// Approve on group `group_key` of run `run_id`'s package (AD-113, D-239):
/// the collector's stored state, then on thread `workshop` the group's
/// hunks from the cache, recomputed when the window restarted, compared
/// with the `hunks_sha256` the panel showed; the approval itself is a
/// request the collector never drops.
pub fn approve_sensitive(
    sender: &std::sync::mpsc::Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    home: &Path,
    base_env: &[(String, String)],
    run_id: &str,
    group_key: &str,
    hunks_sha256: &str,
) -> Result<ApprovalAnswer, IpcError> {
    let stored = match ask(
        sender,
        AuthorCommand::ReviewRow(run_id.to_owned()),
        AUTHOR_WAIT,
    )? {
        AuthorAnswer::WorkshopReview(Some(stored)) => *stored,
        AuthorAnswer::WorkshopReview(None) => {
            return Ok(ApprovalAnswer::Refused(ApprovalRefusal::UnknownRun));
        }
        other => return Err(IpcError::internal(format!("unexpected answer {other:?}"))),
    };
    if let Some(why) = stored_refusal(&stored.row, group_key) {
        return Ok(ApprovalAnswer::Refused(why));
    }
    let floor_key = floor_key(sender)?;
    let job = runtime.clone();
    let home = home.to_path_buf();
    let base_env = base_env.to_vec();
    let collector = sender.clone();
    let approval = SensitiveApproval {
        run_id: stored.row.id.clone(),
        group_key: group_key.to_owned(),
        hunks_sha256: hunks_sha256.to_owned(),
    };
    on_workshop(runtime, move || {
        let display = display_of(&job, &home, &base_env, &stored.row, floor_key.as_deref());
        let shown = display
            .as_ref()
            .and_then(|d| d.hunks.get(&approval.group_key));
        if let Some(why) = approval_refusal(shown, &approval.hunks_sha256) {
            return Ok(ApprovalAnswer::Refused(why));
        }
        match ask(
            &collector,
            AuthorCommand::RecordApproval(Box::new(approval)),
            AUTHOR_WAIT,
        )? {
            AuthorAnswer::Approval(answer) => Ok(answer),
            other => Err(IpcError::internal(format!("unexpected answer {other:?}"))),
        }
    })?
}

/// What the registry holds of `stored`'s run, as the view reads it.
pub fn inputs_of(stored: &StoredReview) -> ReviewInputs {
    let row = &stored.row;
    let texts = row.texts.clone().unwrap_or_default();
    ReviewInputs {
        run_id: row.id.clone(),
        template: row.template.clone(),
        project: row.project.clone(),
        base_branch: row.base_branch.clone(),
        base_commit: row.base_commit.clone(),
        tip: row.tip_commit.clone(),
        state: row.state,
        outcome: row.outcome,
        cause: match row.cause {
            Some(Recorded::Known(cause)) => Some(cause),
            _ => None,
        },
        quarantine_cause: row
            .quarantine_cause
            .as_deref()
            .and_then(QuarantineCause::from_code),
        report_tail: texts.report_tail,
        facts: ReviewFacts {
            duration_ms: row
                .finished_at_ms
                .map(|end| end.saturating_sub(row.started_at_ms).max(0)),
            cost_usd_micros: row.facts.cost_usd_micros,
            turns: row.facts.turns,
            exit_code: row.exit_code,
            hooks: row.facts.hooks.clone(),
            allowed: texts.allowed_hosts,
            refused: row.facts.refused,
            refusals: texts.refusals,
            clock_skew_ms: row.facts.clock_skew_ms,
            commits_past_base: row.commits_past_base,
            files_changed: row.files_changed,
            insertions: row.facts.insertions,
            deletions: row.facts.deletions,
            collect_failed: row.facts.collect_failed,
            credential_locations: row.facts.credential_locations,
            credential_places: texts.credential_locations,
        },
        groups: groups_of(row),
        approvals: stored.approvals.clone(),
    }
}

/// The needles of `row`'s credential, read again (D-230); none for a
/// `login` run or a credential the window cannot read.
fn needles_of(runtime: &WorkshopRuntime, row: &RunRecord, floor_key: &str) -> Option<RunNeedles> {
    let secret = runtime
        .store
        .read(&credential_store_name(floor_key))
        .ok()
        .flatten()?;
    RunNeedles::of(row.credential_kind, &secret)
}

/// Host git's display of `row`'s package (AD-111 step 8), from the cache
/// or the scratch repository; none for a run that is not a package, or
/// whose package was never fetched.
pub(crate) fn display_of(
    runtime: &WorkshopRuntime,
    home: &Path,
    base_env: &[(String, String)],
    row: &RunRecord,
    floor_key: Option<&str>,
) -> Option<PackageDisplay> {
    if row.outcome != Some(WorkshopOutcome::Package) || row.quarantine_cause.is_some() {
        return None;
    }
    if let Some(shown) = runtime.reviews.lock().ok()?.get(&row.id) {
        return Some(shown.clone());
    }
    let tip = row.tip_commit.as_deref()?;
    let scratch = run_dir(home, &row.id).join(SCRATCH_DIR);
    if !scratch.is_dir() {
        return None;
    }
    let git = WorkshopGit {
        git: runtime.git.clone()?,
        hooks_dir: crate::workshop::git::ensure_empty_hooks(home).ok()?,
        base_env: base_env.to_vec(),
    };
    let needles = floor_key.and_then(|key| needles_of(runtime, row, key));
    let shown = show(
        &git,
        &scratch,
        &row.base_commit,
        tip,
        needles.as_ref().map(RunNeedles::needles),
        &groups_of(row),
    )?;
    runtime
        .reviews
        .lock()
        .ok()?
        .insert(row.id.clone(), shown.clone());
    Some(shown)
}

/// Commits, the file list, the diff and `groups`' hunks of `base..tip` in
/// `scratch`, worker texts replaced when `needles` are known (D-230). Ids
/// only reach git (D-219).
pub fn show(
    git: &WorkshopGit,
    scratch: &Path,
    base: &str,
    tip: &str,
    needles: Option<&Needles>,
    groups: &[PackageGroup],
) -> Option<PackageDisplay> {
    let clean = |text: String| match needles {
        Some(needles) => replace(&text, needles),
        None => text,
    };
    let range = format!("{base}..{tip}");
    let log = git
        .output(scratch, &["log", "-z", REVIEW_LOG_FORMAT, &range])
        .ok()?;
    let commits = parse_log(&log)
        .into_iter()
        .map(|commit| officina_core::workshop::review::ReviewCommit {
            subject: commit.subject.mapped(|t| clean(t.to_owned())),
            author: commit.author.mapped(|t| clean(t.to_owned())),
            ..commit
        })
        .collect();
    let changes = package_changes(git, scratch, base, tip).ok()?;
    let hunks = group_hunks(git, scratch, &changes, groups, &clean)?;
    let files = changes
        .into_iter()
        .map(|file| ShownFile {
            status: file.status,
            paths: file
                .paths
                .into_iter()
                .map(|path| Untrusted::new(clean(path)))
                .collect(),
        })
        .collect();
    let ran = crate::workshop::git::run(
        &git.git,
        scratch,
        &[
            "diff",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            base,
            tip,
        ],
        &git.base_env,
        &git.hooks_dir,
        None,
    )
    .ok()?;
    let over = ran.stdout_over || ran.stdout.len() > DIFF_CAP_BYTES;
    let text = clean(String::from_utf8_lossy(&ran.stdout).into_owned());
    Some(PackageDisplay {
        commits,
        files,
        diff: shape_diff(text.as_bytes(), over),
        hunks,
    })
}

/// One file's hunks: what `diff --raw` said of it, then the diff of its
/// two blobs, or past git's output cap.
enum FileHunks {
    Bytes(Vec<u8>),
    Over,
}

/// Each group's hunks (AD-113, D-219): per file, from the old and new blob
/// ids `diff --raw` reported, an absent side read as the empty blob; the
/// SHA-256 over the raw bytes, the text shown with the credential's forms
/// replaced. None when host git failed.
fn group_hunks(
    git: &WorkshopGit,
    scratch: &Path,
    files: &[ChangedFile],
    groups: &[PackageGroup],
    clean: &dyn Fn(String) -> String,
) -> Option<BTreeMap<String, ShownHunks>> {
    let mut empty_blob: Option<String> = None;
    let mut per_file: BTreeMap<u32, FileHunks> = BTreeMap::new();
    let mut shown = BTreeMap::new();
    for group in groups {
        let mut bytes = Vec::new();
        let mut over = false;
        for index in &group.files {
            if !per_file.contains_key(index) {
                let file = files.get(usize::try_from(*index).ok()?)?;
                let hunks = file_hunks(git, scratch, file, &mut empty_blob)?;
                per_file.insert(*index, hunks);
            }
            match per_file.get(index)? {
                FileHunks::Over => over = true,
                FileHunks::Bytes(file) if !over => bytes.extend_from_slice(file),
                FileHunks::Bytes(_) => {}
            }
            over |= bytes.len() > MAX_GROUP_HUNK_BYTES;
        }
        let text =
            (!over).then(|| Untrusted::new(clean(String::from_utf8_lossy(&bytes).into_owned())));
        shown.insert(
            group.key.clone(),
            ShownHunks {
                text,
                bytes: if over {
                    (MAX_GROUP_HUNK_BYTES as u64)
                        .saturating_add(1)
                        .max(bytes.len() as u64)
                } else {
                    bytes.len() as u64
                },
                sha256: hunks_sha256(&bytes),
            },
        );
    }
    Some(shown)
}

/// The hunks of `file`, its blob ids alone reaching git.
fn file_hunks(
    git: &WorkshopGit,
    scratch: &Path,
    file: &ChangedFile,
    empty_blob: &mut Option<String>,
) -> Option<FileHunks> {
    let mut bytes = file.hunk_header();
    let Some((old, new)) = file.blob_ids() else {
        return Some(FileHunks::Bytes(bytes));
    };
    if empty_blob.is_none() && (old.is_none() || new.is_none()) {
        *empty_blob = Some(write_empty_blob(git, scratch)?);
    }
    let empty = empty_blob.as_deref().unwrap_or_default();
    let ran = crate::workshop::git::run(
        &git.git,
        scratch,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            old.unwrap_or(empty),
            new.unwrap_or(empty),
        ],
        &git.base_env,
        &git.hooks_dir,
        None,
    )
    .ok()?;
    if ran.stdout_over {
        return Some(FileHunks::Over);
    }
    if !ran.status.success() {
        return None;
    }
    bytes.extend_from_slice(&ran.stdout);
    Some(FileHunks::Bytes(bytes))
}

/// The empty blob's id in `scratch`'s object format, written there so a
/// side an added or deleted file lacks can be diffed from it.
fn write_empty_blob(git: &WorkshopGit, scratch: &Path) -> Option<String> {
    let ran = crate::workshop::git::run(
        &git.git,
        scratch,
        &["hash-object", "-w", "-t", "blob", "--stdin"],
        &git.base_env,
        &git.hooks_dir,
        Some(Vec::new()),
    )
    .ok()?;
    if !ran.status.success() {
        return None;
    }
    officina_core::workshop::package::object_id(String::from_utf8_lossy(&ran.stdout).trim())
}
