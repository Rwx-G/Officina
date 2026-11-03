//! The review view's host half (AD-121, D-239): the collector hands the
//! stored facts and journals the first Receive; thread `workshop` runs
//! host git in the run's scratch repository for the commits, the file
//! list and the diff, every text with the credential's forms replaced
//! (D-230), and keeps what it showed per run for story 6.15's approvals.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use officina_core::ipc::IpcError;
use officina_core::meeting::{DIFF_CAP_BYTES, shape_diff};
use officina_core::relay::journal::{JournalEntry, ReviewOpened};
use officina_core::untrusted::Untrusted;
use officina_core::workshop::package::QuarantineCause;
use officina_core::workshop::review::{
    PackageDisplay, REVIEW_LOG_FORMAT, ReviewFacts, ReviewInputs, ReviewView, parse_log,
};
use officina_core::workshop::run::{Recorded, WorkshopOutcome};
use officina_core::workshop::scan::{Needles, replace};

use crate::collector::Collector;
use crate::registry::window::WindowRegistry;
use crate::registry::workshop::RunRecord;
use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, CollectorMsg, ask};
use crate::workshop::inventory::WorkshopGit;
use crate::workshop::launch::{RunNeedles, WorkshopRuntime, credential_store_name, run_dir};
use crate::workshop::verify::{SCRATCH_DIR, raw_paths};

/// What host git showed of each run's package, by run id: computed once
/// per window, so a second Receive runs no git (D-239).
pub type ReviewCache = Arc<Mutex<BTreeMap<String, PackageDisplay>>>;

/// The collector's half of Receive (AD-120, D-249): run `id`'s row, and
/// `review_opened` journaled at a package's first Receive.
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
        crate::relay::journal::record(collector.relay(), &[line], now_ms).map_err(|e| {
            IpcError::new(
                officina_core::ipc::IpcErrorCode::JournalUnavailable,
                e.to_string(),
            )
        })?;
    }
    Ok(AuthorAnswer::WorkshopRun(row.map(Box::new)))
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
    let row = match ask(
        sender,
        AuthorCommand::OpenReview(run_id.to_owned()),
        AUTHOR_WAIT,
    )? {
        AuthorAnswer::WorkshopRun(Some(row)) => *row,
        AuthorAnswer::WorkshopRun(None) => {
            return Err(IpcError::new(
                officina_core::ipc::IpcErrorCode::Refused,
                format!("no run {run_id}"),
            ));
        }
        other => return Err(IpcError::internal(format!("unexpected answer {other:?}"))),
    };
    let floor_key = match ask(sender, AuthorCommand::WorkshopRowsToReconcile, AUTHOR_WAIT)? {
        AuthorAnswer::WorkshopRows(rows) => rows.1,
        _ => None,
    };
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let job = runtime.clone();
    let home = home.to_path_buf();
    let base_env = base_env.to_vec();
    let submitted = runtime.thread.submit(move || {
        let display = display_of(&job, &home, &base_env, &row, floor_key.as_deref());
        let _ = done_tx.send(ReviewView::build(inputs_of(&row), display));
    });
    if !submitted {
        return Err(IpcError::internal("thread workshop has stopped"));
    }
    done_rx
        .recv()
        .map_err(|_| IpcError::internal("thread workshop has stopped"))
}

/// What the registry holds of `row`, as the view reads it.
pub fn inputs_of(row: &RunRecord) -> ReviewInputs {
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
        },
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
fn display_of(
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
    )?;
    runtime
        .reviews
        .lock()
        .ok()?
        .insert(row.id.clone(), shown.clone());
    Some(shown)
}

/// Commits, the file list and the diff of `base..tip` in `scratch`,
/// worker texts replaced when `needles` are known (D-230). Ids only reach
/// git (D-219).
pub fn show(
    git: &WorkshopGit,
    scratch: &Path,
    base: &str,
    tip: &str,
    needles: Option<&Needles>,
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
    let raw = git
        .output(
            scratch,
            &[
                "diff",
                "--raw",
                "-z",
                "-M",
                "--no-ext-diff",
                "--no-textconv",
                base,
                tip,
            ],
        )
        .ok()?;
    let files = raw_paths(&raw)
        .into_iter()
        .map(|path| Untrusted::new(clean(path)))
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
    })
}
