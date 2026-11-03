//! Import and Discard (AD-114, FR-12): the author's verdict on a package.
//! Import runs on thread `workshop` under the workshop rule: the review's
//! state and approvals checked against the hunks host git computes, then
//! the project's references read, the package's objects fetched from the
//! run's scratch repository and one new reference created with a zero old
//! value, so an existing reference is never moved. The collector records
//! each verdict and its journal line; Discard runs no git at all.

use std::path::Path;
use std::sync::mpsc::Sender;

use officina_core::ipc::IpcError;
use officina_core::relay::journal::{JournalEntry, ReviewDiscarded, ReviewImported};
use officina_core::workshop::review::{
    DiscardAnswer, DiscardReason, DiscardRefusal, ImportAnswer, ImportRefusal, PackageVerdict,
    ReviewView, discard_refusal, import_refusal,
};
use officina_core::workshop::run::RunState;

use crate::collector::Collector;
use crate::registry::window::WindowRegistry;
use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, CollectorMsg, ask};
use crate::workshop::launch::{WorkshopRuntime, run_dir};
use crate::workshop::review::{StoredReview, display_of, floor_key, inputs_of, on_workshop};
use crate::workshop::verify::SCRATCH_DIR;

/// An Import thread `workshop` carried out, for the collector to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    pub run_id: String,
    pub branch: String,
    pub tip: String,
}

/// The zero object id of `tip`'s object format: `update-ref`'s old value
/// for a reference that must not exist yet.
fn zero_id(tip: &str) -> String {
    "0".repeat(tip.len())
}

/// Import on run `run_id`'s package (AD-114), pointer only: `tip` is the
/// tip the review showed.
pub fn import_package(
    sender: &Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    home: &Path,
    base_env: &[(String, String)],
    run_id: &str,
    tip: &str,
) -> Result<ImportAnswer, IpcError> {
    import_package_staged(sender, runtime, home, base_env, run_id, tip, || {})
}

/// [`import_package`] with `before_update` called on thread `workshop`
/// right before `update-ref`, once every check passed and the objects
/// were fetched: where a branch created behind Officina's back lands, for
/// the test that creates one there.
pub fn import_package_staged(
    sender: &Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    home: &Path,
    base_env: &[(String, String)],
    run_id: &str,
    tip: &str,
    before_update: impl FnOnce() + Send + 'static,
) -> Result<ImportAnswer, IpcError> {
    let refused = |why| Ok(ImportAnswer::Refused(why));
    let stored = match ask(
        sender,
        AuthorCommand::ReviewRow(run_id.to_owned()),
        AUTHOR_WAIT,
    )? {
        AuthorAnswer::WorkshopReview(Some(stored)) => *stored,
        AuthorAnswer::WorkshopReview(None) => return refused(ImportRefusal::UnknownRun),
        other => return Err(IpcError::internal(format!("unexpected answer {other:?}"))),
    };
    if stored.row.state != RunState::Review {
        return refused(ImportRefusal::NotInReview);
    }
    let floor_key = floor_key(sender)?;
    let job = runtime.clone();
    let home = home.to_path_buf();
    let base_env = base_env.to_vec();
    let collector = sender.clone();
    let tip = tip.to_owned();
    on_workshop(runtime, move || {
        let record = match import_on_workshop(
            &job,
            &home,
            &base_env,
            &stored,
            floor_key.as_deref(),
            &tip,
            before_update,
        ) {
            Ok(record) => record,
            Err(why) => return Ok(ImportAnswer::Refused(why)),
        };
        match ask(
            &collector,
            AuthorCommand::RecordImport(Box::new(record)),
            AUTHOR_WAIT,
        )? {
            AuthorAnswer::Import(answer) => Ok(answer),
            other => Err(IpcError::internal(format!("unexpected answer {other:?}"))),
        }
    })?
}

/// AD-114 steps 1 to 5 on thread `workshop`.
fn import_on_workshop(
    runtime: &WorkshopRuntime,
    home: &Path,
    base_env: &[(String, String)],
    stored: &StoredReview,
    floor_key: Option<&str>,
    tip: &str,
    before_update: impl FnOnce(),
) -> Result<ImportRecord, ImportRefusal> {
    let row = &stored.row;
    let display = display_of(runtime, home, base_env, row, floor_key);
    let view = ReviewView::build(inputs_of(stored), display);
    if let Some(why) = import_refusal(&view, tip) {
        return Err(why);
    }
    let git = runtime.git.as_deref().ok_or(ImportRefusal::Unavailable)?;
    let hooks_dir =
        crate::workshop::git::ensure_empty_hooks(home).map_err(|_| ImportRefusal::Unavailable)?;
    let project = Path::new(&row.project_dir);
    let run = |args: &[&str]| {
        crate::workshop::git::run(git, project, args, base_env, &hooks_dir, None)
            .map_err(|_| ImportRefusal::Unavailable)
    };
    let reference = format!("refs/heads/{}", row.branch);
    let exists = |name: &str| -> Result<bool, ImportRefusal> {
        let ran = run(&["rev-parse", "--verify", "--quiet", name])?;
        match ran.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(ImportRefusal::Unavailable),
        }
    };
    if exists(&reference)? {
        return Err(ImportRefusal::BranchExists);
    }
    if exists(BRANCH_PARENT)? {
        return Err(ImportRefusal::BranchConflict);
    }
    let scratch = run_dir(home, &row.id).join(SCRATCH_DIR);
    if !scratch.is_dir() {
        return Err(ImportRefusal::PackageChanged);
    }
    let source = scratch.to_string_lossy().into_owned();
    let fetched = run(&[
        "fetch",
        "--quiet",
        "--no-tags",
        "--no-write-fetch-head",
        &source,
        &reference,
    ])?;
    if !fetched.status.success() {
        return Err(ImportRefusal::PackageChanged);
    }
    let held = run(&["cat-file", "-e", &format!("{tip}^{{commit}}")])?;
    if !held.status.success() {
        return Err(ImportRefusal::PackageChanged);
    }
    before_update();
    let updated = run(&[
        "-c",
        "core.logAllRefUpdates=false",
        "update-ref",
        &reference,
        tip,
        &zero_id(tip),
    ])?;
    if !updated.status.success() {
        return Err(if exists(&reference)? {
            ImportRefusal::BranchExists
        } else {
            ImportRefusal::BranchConflict
        });
    }
    Ok(ImportRecord {
        run_id: row.id.clone(),
        branch: row.branch.clone(),
        tip: tip.to_owned(),
    })
}

/// The reference whose existence makes every `officina/<name>` branch
/// impossible (D-248).
const BRANCH_PARENT: &str = "refs/heads/officina";

fn journal(collector: &Collector, line: JournalEntry, now_ms: i64) -> Result<(), IpcError> {
    crate::relay::journal::record(collector.relay(), &[line], now_ms).map_err(|e| {
        IpcError::new(
            officina_core::ipc::IpcErrorCode::JournalUnavailable,
            e.to_string(),
        )
    })
}

/// The collector's half of Import (AD-114 step 6): the row read again,
/// still in `review` at the tip imported, then state `imported` and
/// `review_imported`.
pub(crate) fn record_import_on_collector(
    collector: &Collector,
    registry: &mut WindowRegistry,
    record: &ImportRecord,
    now_ms: i64,
) -> Result<AuthorAnswer, IpcError> {
    let refused = |why| Ok(AuthorAnswer::Import(ImportAnswer::Refused(why)));
    let Some(row) = registry.workshop_run(&record.run_id)? else {
        return refused(ImportRefusal::UnknownRun);
    };
    if row.state != RunState::Review {
        return refused(ImportRefusal::NotInReview);
    }
    if row.tip_commit.as_deref() != Some(record.tip.as_str()) {
        return refused(ImportRefusal::StateChanged);
    }
    registry.set_workshop_verdict(&row.id, PackageVerdict::Imported, None, now_ms)?;
    let line = JournalEntry::ReviewImported(ReviewImported {
        run_id: row.id.clone(),
        branch: record.branch.clone(),
        tip: record.tip.clone(),
    });
    journal(collector, line, now_ms)?;
    Ok(AuthorAnswer::Import(ImportAnswer::Imported(
        record.branch.clone(),
    )))
}

/// Discard on run `run_id`'s package with the author's `reason` (AD-114):
/// it only reduces, so the keyboard reaches it.
pub fn discard_package(
    sender: &Sender<CollectorMsg>,
    run_id: &str,
    reason: DiscardReason,
) -> Result<DiscardAnswer, IpcError> {
    if !reason.is_authors() {
        return Ok(DiscardAnswer::Refused(DiscardRefusal::SystemReason));
    }
    match ask(
        sender,
        AuthorCommand::DiscardPackage {
            run_id: run_id.to_owned(),
            reason,
        },
        AUTHOR_WAIT,
    )? {
        AuthorAnswer::Discard(answer) => Ok(answer),
        other => Err(IpcError::internal(format!("unexpected answer {other:?}"))),
    }
}

/// The collector's half of Discard: state `discarded` with the reason and
/// `review_discarded`, for a package awaiting a verdict.
pub(crate) fn discard_on_collector(
    collector: &Collector,
    registry: &mut WindowRegistry,
    run_id: &str,
    reason: DiscardReason,
    now_ms: i64,
) -> Result<AuthorAnswer, IpcError> {
    let refused = |why| Ok(AuthorAnswer::Discard(DiscardAnswer::Refused(why)));
    let Some(row) = registry.workshop_run(run_id)? else {
        return refused(DiscardRefusal::UnknownRun);
    };
    if let Some(why) = discard_refusal(row.state, reason) {
        return refused(why);
    }
    registry.set_workshop_verdict(&row.id, PackageVerdict::Discarded, Some(reason), now_ms)?;
    let line = JournalEntry::ReviewDiscarded(ReviewDiscarded {
        run_id: row.id.clone(),
        reason,
    });
    journal(collector, line, now_ms)?;
    Ok(AuthorAnswer::Discard(DiscardAnswer::Discarded))
}

/// `review_discarded` with reason `expired` for each package the hourly
/// pass expired (D-213); a journal that cannot be written loses the lines,
/// as every journal line of the pass.
pub(crate) fn journal_expired(collector: &Collector, expired: &[String], now_ms: i64) {
    let lines: Vec<JournalEntry> = expired
        .iter()
        .map(|id| {
            JournalEntry::ReviewDiscarded(ReviewDiscarded {
                run_id: id.clone(),
                reason: DiscardReason::Expired,
            })
        })
        .collect();
    if !lines.is_empty() {
        let _ = crate::relay::journal::record(collector.relay(), &lines, now_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zero_id_follows_the_object_format() {
        assert_eq!(zero_id(&"a".repeat(40)), "0".repeat(40));
        assert_eq!(zero_id(&"a".repeat(64)), "0".repeat(64));
    }

    /// D-213: each package the hourly pass expired gets one
    /// `review_discarded` line with reason `expired`.
    #[test]
    fn each_expired_package_is_journaled_expired() {
        use crate::collector::LoadedFloor;
        use crate::paths::Paths;
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("claude"),
            claude_config_override: None,
            floor_file: None,
        };
        let collector = Collector::from_parts(paths, LoadedFloor::default(), None);
        let now = crate::files::now_ms();
        let ids = ["a".repeat(32), "b".repeat(32)];
        journal_expired(&collector, &ids, now);
        journal_expired(&collector, &[], now);
        let (lines, broken) = crate::relay::journal::read_recent(collector.relay(), now);
        assert_eq!(broken, 0);
        let expired: Vec<(String, DiscardReason)> = lines
            .into_iter()
            .filter_map(|line| match line.entry {
                JournalEntry::ReviewDiscarded(l) => Some((l.run_id, l.reason)),
                _ => None,
            })
            .collect();
        assert_eq!(
            expired,
            ids.iter()
                .map(|id| (id.clone(), DiscardReason::Expired))
                .collect::<Vec<_>>()
        );
    }
}
