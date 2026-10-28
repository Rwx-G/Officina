//! The files of `relay/approvals/` and `relay/outputs/` (AD-25, AD-33,
//! D-170): reading them for `officina run`, the window's board and its
//! authority; a request written once with `create_new`; an approval used by
//! claiming `<id>.consumed.json` with `create_new`, so two runs never use
//! one; kept check outputs written under a temporary name then renamed; and
//! the window's retention pass.
//! `officina run` writes requests and consumes approvals but deletes
//! nothing: only the window prunes.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, SystemTime};

use officina_core::relay::approvals::{
    ApprovalContext, ApprovalFiles, ApprovalRequest, ApprovalView, DryRunInputs, FileKind,
    KEPT_OUTPUT_RETENTION_MS, RETENTION_MS, file_name, is_live, kept_output_name, parse_file_name,
    pending,
};
use officina_core::relay::catalogue::project_dirs;
use officina_core::relay::journal::TreeFingerprint;

use super::home::{ApprovedState, RelayHome, load_relay_floor, system_dir};
use super::tree::TreeResult;
use crate::files::atomic_write;

/// Every readable file of `relay/approvals/`. A file that does not parse
/// (a request still being written, or not Officina's) is skipped.
pub fn read_files(home: &RelayHome) -> ApprovalFiles {
    let mut files = ApprovalFiles::default();
    let Ok(entries) = fs::read_dir(home.approvals_dir()) else {
        return files;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((id, kind)) = parse_file_name(&name) else {
            continue;
        };
        let Ok(bytes) = fs::read(entry.path()) else {
            continue;
        };
        let id = id.to_owned();
        match kind {
            FileKind::Request => {
                if let Ok(request) = serde_json::from_slice(&bytes) {
                    files.requests.insert(id, request);
                }
            }
            FileKind::Approved | FileKind::Rejected | FileKind::Consumed => {
                if let Ok(decision) = serde_json::from_slice(&bytes) {
                    let map = match kind {
                        FileKind::Approved => &mut files.approved,
                        FileKind::Rejected => &mut files.rejected,
                        _ => &mut files.consumed,
                    };
                    map.insert(id, decision);
                }
            }
        }
    }
    files
}

/// Writes request `id`, failing when it already exists (`create_new`).
pub fn create_request(home: &RelayHome, id: &str, request: &ApprovalRequest) -> io::Result<()> {
    let dir = home.approvals_dir();
    fs::create_dir_all(&dir)?;
    let bytes = serde_json::to_vec_pretty(request).map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(file_name(id, FileKind::Request)))?;
    file.write_all(&bytes)
}

/// Uses the approval of request `id`: `<id>.approved.json` becomes
/// `<id>.consumed.json`, and exactly one of several concurrent callers gets
/// `Ok`, the others `NotFound`, so one approval runs once without a lock
/// (AD-33).
///
/// AD-33 names a rename, but `std::fs::rename` on Windows opens the file and
/// renames the handle, so two callers that opened it at once both succeed
/// (`approvals_stress.rs` caught it). The claim is the creation of the
/// consumed name with `create_new`, which only one caller can make; the
/// approved name is removed after. A crash in between leaves both names,
/// which `next_step` reads as used.
pub fn consume(home: &RelayHome, id: &str) -> io::Result<()> {
    let dir = home.approvals_dir();
    let approved = dir.join(file_name(id, FileKind::Approved));
    let decision = fs::read(&approved)?;
    let mut claim = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(file_name(id, FileKind::Consumed)))
        .map_err(|e| match e.kind() {
            io::ErrorKind::AlreadyExists => io::Error::new(io::ErrorKind::NotFound, e),
            _ => e,
        })?;
    claim.write_all(&decision)?;
    drop(claim);
    // Used whatever happens next: the consumed name is the record.
    let _ = fs::remove_file(&approved);
    Ok(())
}

/// The window's decision on request `id`, as a stat finds it: approved (and
/// not used yet) or rejected; `None` while pending.
pub fn decision(home: &RelayHome, id: &str) -> Option<FileKind> {
    let dir = home.approvals_dir();
    [FileKind::Rejected, FileKind::Approved]
        .into_iter()
        .find(|kind| dir.join(file_name(id, *kind)).is_file())
}

/// Keeps a check run's output at `relay/outputs/<run_id>.txt`, written under
/// a temporary name then renamed (D-170).
pub fn keep_output(home: &RelayHome, run_id: &str, bytes: &[u8]) -> io::Result<()> {
    atomic_write(&home.outputs_dir().join(kept_output_name(run_id)), bytes)
}

/// The kept output of check run `run_id`, or why it cannot be read; `None`
/// when none is kept.
pub fn read_output(home: &RelayHome, run_id: &str) -> Option<Result<String, String>> {
    match fs::read(home.outputs_dir().join(kept_output_name(run_id))) {
        Ok(bytes) => Some(Ok(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => Some(Err(format!("the dry run's output cannot be read: {e}"))),
    }
}

fn older_than(path: &Path, now: SystemTime, age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed > age)
}

fn prune_dir(dir: &Path, now: SystemTime, age: Duration, ours: impl Fn(&str) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| ours(&entry.file_name().to_string_lossy()))
        .filter(|entry| older_than(&entry.path(), now, age))
        .filter(|entry| fs::remove_file(entry.path()).is_ok())
        .count()
}

fn millis(ms: i64) -> Duration {
    Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

/// The window's retention pass (AD-33, D-170): approval files older than
/// [`RETENTION_MS`] and kept outputs older than
/// [`KEPT_OUTPUT_RETENTION_MS`], by modification time. Returns how many
/// files went. Only Officina's own file names are touched.
pub fn prune(home: &RelayHome, now: SystemTime) -> usize {
    prune_dir(&home.approvals_dir(), now, millis(RETENTION_MS), |name| {
        parse_file_name(name).is_some()
    }) + prune_dir(
        &home.outputs_dir(),
        now,
        millis(KEPT_OUTPUT_RETENTION_MS),
        |name| name.ends_with(".txt"),
    )
}

/// What a request is rebuilt against (D-27), as read now: the approved
/// snapshot, the relay floor's project folders and the system directory.
pub struct ContextSources {
    pub approved: ApprovedState,
    pub project_dirs: BTreeMap<String, String>,
    pub system_dir: String,
}

impl ContextSources {
    pub fn read(home: &RelayHome) -> ContextSources {
        let approved = home.read_approved();
        let floor_path = home.relay_floor(&approved);
        let project_dirs = load_relay_floor(&floor_path)
            .map(|relay| project_dirs(&relay.floor, &floor_path.to_string_lossy()))
            .unwrap_or_default();
        ContextSources {
            approved,
            project_dirs,
            system_dir: system_dir()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    pub fn context(&self) -> ApprovalContext<'_> {
        ApprovalContext {
            approved: self.approved.approved(),
            project_dirs: &self.project_dirs,
            system_dir: &self.system_dir,
        }
    }
}

/// The pending requests Needs you lists (AD-35, D-27, D-170, D-173), each
/// rebuilt from the approved catalogue with the relay floor's project
/// folders and the system directory, with its dry run's kept output and
/// whether the project's files are still the ones it is bound to. `fingerprint` computes a
/// folder's tree, so the window can cache it; a tree it cannot compute is
/// left out, which the card shows as unknown. Nothing is read beyond the
/// approvals folder when no request is there.
pub fn board_approvals(
    home: &RelayHome,
    now_ms: i64,
    fingerprint: &mut dyn FnMut(&Path) -> TreeResult,
) -> Vec<ApprovalView> {
    let files = read_files(home);
    if files.requests.is_empty() {
        return Vec::new();
    }
    let sources = ContextSources::read(home);
    let ctx = sources.context();
    let dirs = &sources.project_dirs;
    let mut dry_runs = DryRunInputs::default();
    let mut trees: BTreeMap<String, Option<TreeFingerprint>> = BTreeMap::new();
    for (id, request) in &files.requests {
        if files.decided(id) || !is_live(request.created_at_ms, now_ms) {
            continue;
        }
        if let Some(run_id) = &request.check_run_id
            && let Some(output) = read_output(home, run_id)
        {
            dry_runs.outputs.insert(run_id.clone(), output);
        }
        if request.tree.is_none() {
            continue;
        }
        let project_dir = dirs.get(&request.project).cloned().or_else(|| {
            ctx.approved
                .and_then(|a| a.canonical.projects.get(&request.project))
                .map(|p| p.path.clone())
        });
        if let Some(dir) = project_dir
            && !trees.contains_key(&request.project)
            && let Ok(found) = fingerprint(Path::new(&dir))
        {
            trees.insert(request.project.clone(), found);
        }
    }
    dry_runs.trees = trees;
    pending(&files, &ctx, &dry_runs, now_ms)
}
