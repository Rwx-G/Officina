//! Collection (AD-110, D-218, D-229, D-248): once a run's worker exited
//! and its gateway was stopped, a separate `Packer` container packages the
//! clone, `docker cp` streams its `out/` through the capped reader into
//! core's package reader, the bundle alone reaches the disk under the name
//! Officina chose, host git verifies it (AD-111), core decides what the
//! run produced, and every engine object of the run is removed. Runs on
//! thread `workshop`; the collector records the result.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use officina_core::relay::StreamHash;
use officina_core::workshop::engine::EngineCall;
use officina_core::workshop::launch::RunPlan;
use officina_core::workshop::names::{RunId, RunNames, run_labels};
use officina_core::workshop::output::clean_host;
use officina_core::workshop::package::{
    Expected, MAX_PACKAGE_STREAM_BYTES, PackageAssessment, QuarantineCause, outcome,
};
use officina_core::workshop::review::parse_shortstat;
use officina_core::workshop::run::{CollectFailure, FinishCause, Recorded, WorkshopOutcome};
use officina_core::workshop::tar::{PackageError, PackageReader};
use officina_core::workshop::vector::{ContainerRole, WORK_DIR, packer};

use crate::process::StreamError;
use crate::registry::workshop::RunRecord;
use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, ask};
use crate::workshop::engine::{ResolvedDocker, StreamOutError, execute, stream_out};
use crate::workshop::follow::{FollowContext, FollowEvent, RunSummary};
use crate::workshop::inventory::WorkshopGit;
use crate::workshop::launch::run_dir;
use crate::workshop::verify::{Received, SCRATCH_DIR, scratch, verify_with_paths};

/// The one file collection writes from a package, under a name Officina
/// chooses (D-229).
pub const BUNDLE_FILE: &str = "work.bundle";

/// The packer's `out/` inside the run's volume (AD-110).
pub const PACKAGE_SOURCE: &str = "/work/.officina/out";

/// The run's `out/` on the host.
pub fn out_dir(home: &Path, run_id: &str) -> PathBuf {
    run_dir(home, run_id).join("out")
}

/// `out_dir` joined with `name`, refused unless every component past
/// `out_dir` is a plain name and the result, resolved, stays inside
/// `out_dir` resolved the same way (D-229): checked before any write.
pub fn checked_target(out_dir: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    if relative.components().count() != 1
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("{name:?} is not a plain name"));
    }
    let joined = out_dir.join(relative);
    let root = std::fs::canonicalize(out_dir).map_err(|e| e.to_string())?;
    let parent = joined
        .parent()
        .ok_or_else(|| "no parent".to_owned())
        .and_then(|p| std::fs::canonicalize(p).map_err(|e| e.to_string()))?;
    let resolved = parent.join(joined.file_name().ok_or_else(|| "no name".to_owned())?);
    if resolved.starts_with(&root) && resolved != root {
        Ok(joined)
    } else {
        Err(format!("{name:?} leaves the run's out folder"))
    }
}

/// What a package stream gave once it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpacked {
    pub received: Received,
    pub bundle_sha256: Option<String>,
}

/// Writes a package stream into `out_dir` as it comes: core's reader
/// decides what each byte is, and only the bundle's bytes reach a file,
/// [`BUNDLE_FILE`], opened on its first byte after [`checked_target`].
pub struct PackageWriter {
    out_dir: PathBuf,
    reader: PackageReader,
    file: Option<std::fs::File>,
    hash: StreamHash,
    bundle: Option<PathBuf>,
}

impl PackageWriter {
    /// A writer into `out_dir`, which it creates.
    pub fn new(out_dir: &Path) -> Result<PackageWriter, String> {
        std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
        Ok(PackageWriter {
            out_dir: out_dir.to_path_buf(),
            reader: PackageReader::new(),
            file: None,
            hash: StreamHash::new(),
            bundle: None,
        })
    }

    /// Feeds the next bytes of the stream.
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), PackageError> {
        let out_dir = &self.out_dir;
        let file = &mut self.file;
        let hash = &mut self.hash;
        let bundle = &mut self.bundle;
        let mut sink = |part: &[u8]| -> Result<(), String> {
            if file.is_none() {
                let target = checked_target(out_dir, BUNDLE_FILE)?;
                *file = Some(
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)
                        .map_err(|e| e.to_string())?,
                );
                *bundle = Some(target);
            }
            hash.update(part);
            file.as_mut()
                .map_or(Ok(()), |f| f.write_all(part))
                .map_err(|e| e.to_string())
        };
        self.reader.push(chunk, &mut sink)
    }

    /// What the stream held, once it ended; the bundle's file flushed.
    pub fn finish(self) -> Result<Unpacked, PackageError> {
        if let Some(mut file) = self.file {
            file.flush()
                .map_err(|e| PackageError::Sink(e.to_string()))?;
        }
        let package = self.reader.finish()?;
        let bundle = package.bundle_bytes.map(|_| {
            self.bundle
                .clone()
                .unwrap_or_else(|| self.out_dir.join(BUNDLE_FILE))
        });
        // An empty bundle entry opened no file: write it, so the bytes
        // host git reads are the stream's.
        if let Some(path) = bundle.as_ref().filter(|p| !p.exists()) {
            let target = checked_target(&self.out_dir, BUNDLE_FILE).map_err(PackageError::Sink)?;
            debug_assert_eq!(&target, path);
            std::fs::write(&target, b"").map_err(|e| PackageError::Sink(e.to_string()))?;
        }
        Ok(Unpacked {
            bundle_sha256: package.bundle_bytes.map(|_| self.hash.hex()),
            received: Received {
                tip: package.tip,
                bundle,
                bundle_bytes: package.bundle_bytes,
            },
        })
    }
}

/// Deletes `dir` and everything under it, when it exists.
fn clear(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", dir.display())),
    }
}

/// What collection found, for the collector to record (AD-110, AD-120).
#[derive(Debug, Clone, PartialEq)]
pub struct Collected {
    pub run_id: String,
    /// The worker's cause, as the row holds it.
    pub cause: FinishCause,
    pub outcome: WorkshopOutcome,
    /// What host git found; none when collection failed.
    pub assessment: Option<PackageAssessment>,
    pub failure: Option<CollectFailure>,
    pub bundle_sha256: Option<String>,
    pub files_changed: Option<u32>,
    pub insertions: Option<u32>,
    pub deletions: Option<u32>,
    pub summary: RunSummary,
    /// The gateway's allowed hosts, replaced (D-230); empty while the
    /// window holds no needles (story 6.14 journals them after its scan).
    pub allowed: std::collections::BTreeMap<String, u32>,
    pub gateway_exit: Option<i64>,
    pub upstream: Option<officina_core::workshop::gateway::UpstreamState>,
    pub texts: Option<officina_core::workshop::run::RunTexts>,
    /// The run's engine objects were removed (`workshop_removed`).
    pub removed: bool,
}

/// Where a collection stopped short.
enum Short {
    /// Nothing is collected while the workshop is stopped (AD-116).
    Stopped,
    Failed(CollectFailure),
}

/// Queues the collection of `run_id` on thread `workshop`.
pub fn collect_on_thread(context: &FollowContext, run_id: &str) {
    let job = context.clone();
    let run_id = run_id.to_owned();
    context
        .runtime
        .thread
        .submit(move || collect(&job, &run_id));
}

/// Collects `run_id` on the calling thread (AD-110): the row asked of the
/// collector, which must be `collecting`; nothing while the workshop is
/// stopped.
pub fn collect(context: &FollowContext, run_id: &str) {
    if crate::workshop::workshop_stopped(&context.home) {
        return;
    }
    let Ok(AuthorAnswer::WorkshopRun(Some(row))) = ask(
        &context.collector,
        AuthorCommand::WorkshopRun(run_id.to_owned()),
        AUTHOR_WAIT,
    ) else {
        return;
    };
    if row.state != officina_core::workshop::run::RunState::Collecting {
        return;
    }
    let Some(names) = RunId::parse(&row.id).map(|id| RunNames::of(&id)) else {
        return;
    };
    let floor_key = match ask(
        &context.collector,
        AuthorCommand::WorkshopRowsToReconcile,
        AUTHOR_WAIT,
    ) {
        Ok(AuthorAnswer::WorkshopRows(rows)) => rows.1.unwrap_or_default(),
        _ => String::new(),
    };
    let cause = match row.cause {
        Some(Recorded::Known(cause)) => cause,
        _ => FinishCause::Failed,
    };
    let found = package(context, &row, &names, &floor_key);
    if matches!(found, Err(Short::Stopped)) {
        return;
    }
    let removed = remove(context, &names);
    let run = run_dir(&context.home, &row.id);
    let _ = clear(&run.join("in"));
    let mut collected = summarise(context, &row.id, cause);
    collected.removed = removed;
    match found {
        Ok(found) => {
            collected.outcome = outcome(cause, Some(&found.assessment));
            collected.assessment = Some(found.assessment);
            collected.bundle_sha256 = found.bundle_sha256;
            collected.files_changed = found.files_changed;
            collected.insertions = found.insertions;
            collected.deletions = found.deletions;
        }
        Err(short) => {
            collected.outcome = WorkshopOutcome::Failed;
            collected.failure = Some(match short {
                Short::Failed(failure) => failure,
                Short::Stopped => CollectFailure::Host,
            });
        }
    }
    if collected.outcome != WorkshopOutcome::Package {
        let _ = clear(&run.join("out"));
        let _ = clear(&run.join(SCRATCH_DIR));
    }
    if let Ok(mut followed) = context.runtime.followed.lock() {
        followed.remove(&row.id);
    }
    let _ = ask(
        &context.collector,
        AuthorCommand::WorkshopFollow(Box::new(FollowEvent::Collected(Box::new(collected)))),
        AUTHOR_WAIT,
    );
}

/// The facts the run's output gave, from the window's memory.
fn summarise(context: &FollowContext, run_id: &str, cause: FinishCause) -> Collected {
    let followed = context.runtime.followed.lock().ok();
    let entry = followed.as_ref().and_then(|f| f.get(run_id));
    let allowed = entry
        .and_then(|e| {
            let needles = e.needles()?;
            Some(
                e.output
                    .gateway
                    .allowed
                    .iter()
                    .map(|(host, count)| (clean_host(host, needles.needles()), *count))
                    .collect(),
            )
        })
        .unwrap_or_default();
    Collected {
        run_id: run_id.to_owned(),
        cause,
        outcome: WorkshopOutcome::Failed,
        assessment: None,
        failure: None,
        bundle_sha256: None,
        files_changed: None,
        insertions: None,
        deletions: None,
        summary: entry.map(|e| e.summary()).unwrap_or_default(),
        allowed,
        gateway_exit: entry.and_then(|e| e.gateway_exit()),
        upstream: entry.and_then(|e| e.output.gateway.upstream),
        texts: entry.and_then(|e| e.run_texts()),
        removed: false,
    }
}

/// What the package was, once on the host and verified.
struct Found {
    assessment: PackageAssessment,
    bundle_sha256: Option<String>,
    files_changed: Option<u32>,
    insertions: Option<u32>,
    deletions: Option<u32>,
}

impl Found {
    fn quarantined(cause: QuarantineCause) -> Found {
        Found {
            assessment: PackageAssessment::Package {
                tip: None,
                commits_past_base: 0,
                cause: Some(cause),
            },
            bundle_sha256: None,
            files_changed: None,
            insertions: None,
            deletions: None,
        }
    }
}

fn docker(context: &FollowContext) -> Result<&ResolvedDocker, Short> {
    context
        .runtime
        .docker
        .as_ref()
        .ok_or(Short::Failed(CollectFailure::Packer))
}

fn succeeded(context: &FollowContext, call: &EngineCall) -> bool {
    docker(context).is_ok_and(|docker| {
        execute(docker, call, &context.base_env, &context.home, None)
            .is_ok_and(|outcome| outcome.succeeded())
    })
}

/// The mounts the run's plan linked (AD-108), Officina's own record.
fn mounts_of(row: &RunRecord) -> Vec<String> {
    serde_json::from_str::<RunPlan>(&row.plan_json)
        .map(|plan| plan.mount_links.into_iter().map(|m| m.path).collect())
        .unwrap_or_default()
}

/// Steps 1 to 3 of collection: the partial `out/` deleted, the packer
/// run, the stream read, host git's verdict.
fn package(
    context: &FollowContext,
    row: &RunRecord,
    names: &RunNames,
    floor_key: &str,
) -> Result<Found, Short> {
    let docker = docker(context)?;
    let out = out_dir(&context.home, &row.id);
    clear(&out).map_err(|_| Short::Failed(CollectFailure::Host))?;
    let labels = run_labels(
        floor_key,
        crate::workshop::build::VERSION,
        &RunId::parse(&row.id).ok_or(Short::Failed(CollectFailure::Host))?,
        ContainerRole::Packer,
        &row.name,
    );
    let project = row.project.as_str();
    let vector = packer(
        &names.packer,
        &labels,
        &row.image_id,
        &names.volume,
        &row.branch,
        &row.base_commit,
        &format!("{WORK_DIR}/{project}"),
    );
    let _ = execute(
        docker,
        &EngineCall::rm(std::slice::from_ref(&names.packer)),
        &context.base_env,
        &context.home,
        None,
    );
    if !succeeded(context, &EngineCall::create(&vector))
        || !succeeded(
            context,
            &EngineCall::start_attached(&names.packer, ContainerRole::Packer, 0, false),
        )
    {
        return Err(Short::Failed(CollectFailure::Packer));
    }
    if crate::workshop::workshop_stopped(&context.home) {
        return Err(Short::Stopped);
    }
    let mut writer = PackageWriter::new(&out).map_err(|_| Short::Failed(CollectFailure::Host))?;
    let mut refused = None;
    let streamed = stream_out(
        docker,
        &EngineCall::cp_out(&names.packer, PACKAGE_SOURCE),
        &context.base_env,
        &context.home,
        MAX_PACKAGE_STREAM_BYTES,
        &mut |chunk| {
            writer.push(chunk).map_err(|e| {
                let text = e.to_string();
                refused = Some(e);
                text
            })
        },
    );
    match streamed {
        Ok(done) if done.status.success() => {}
        Ok(_) => return Err(Short::Failed(CollectFailure::Copy)),
        Err(StreamOutError::Stream(StreamError::TooLarge(_))) => {
            drop(writer);
            clear(&out).map_err(|_| Short::Failed(CollectFailure::Host))?;
            return Ok(Found::quarantined(QuarantineCause::TooLarge));
        }
        Err(StreamOutError::Stream(StreamError::Refused(_))) if refused.is_some() => {
            drop(writer);
            clear(&out).map_err(|_| Short::Failed(CollectFailure::Host))?;
            return Ok(Found::quarantined(QuarantineCause::BundleInvalid));
        }
        Err(_) => return Err(Short::Failed(CollectFailure::Copy)),
    }
    let unpacked = match writer.finish() {
        Ok(unpacked) => unpacked,
        Err(_) => {
            clear(&out).map_err(|_| Short::Failed(CollectFailure::Host))?;
            return Ok(Found::quarantined(QuarantineCause::BundleInvalid));
        }
    };
    verify(context, row, &unpacked)
}

/// Steps 4 to 6: the scratch repository, AD-111's checks, the counts.
fn verify(context: &FollowContext, row: &RunRecord, unpacked: &Unpacked) -> Result<Found, Short> {
    let host = || Short::Failed(CollectFailure::Host);
    let git = workshop_git(context).ok_or_else(host)?;
    let project = PathBuf::from(&row.project_dir);
    let run = run_dir(&context.home, &row.id);
    let scratch = scratch(&git, &project, &run).map_err(|_| host())?;
    let expected = Expected {
        base_commit: row.base_commit.clone(),
        branch_ref: format!("refs/heads/{}", row.branch),
        mounts: mounts_of(row),
    };
    let (assessment, paths) =
        verify_with_paths(&git, &scratch, &unpacked.received, &expected).map_err(|_| host())?;
    let (insertions, deletions) = match &assessment {
        PackageAssessment::Package {
            tip: Some(tip),
            cause: None,
            ..
        } => git
            .output(
                &scratch,
                &[
                    "diff",
                    "--shortstat",
                    "--no-ext-diff",
                    "--no-textconv",
                    &row.base_commit,
                    tip,
                ],
            )
            .map(|out| {
                let (i, d) = parse_shortstat(&out);
                (Some(i), Some(d))
            })
            .unwrap_or((None, None)),
        _ => (None, None),
    };
    Ok(Found {
        files_changed: paths
            .as_ref()
            .map(|p| u32::try_from(p.len()).unwrap_or(u32::MAX)),
        assessment,
        bundle_sha256: unpacked.bundle_sha256.clone(),
        insertions,
        deletions,
    })
}

/// The workshop's `git` under its rule (AD-111), or none when it cannot
/// be resolved or its empty hooks folder is not empty.
pub fn workshop_git(context: &FollowContext) -> Option<WorkshopGit> {
    let git = context.runtime.git.clone()?;
    let hooks_dir = crate::workshop::git::ensure_empty_hooks(&context.home).ok()?;
    Some(WorkshopGit {
        git,
        hooks_dir,
        base_env: context.base_env.clone(),
    })
}

/// Removes the run's worker, packer, gateway, volume and both networks
/// (AD-110, amendment 10): the credential leaves the engine. True when
/// every call succeeded.
fn remove(context: &FollowContext, names: &RunNames) -> bool {
    [
        EngineCall::rm(&[
            names.worker.clone(),
            names.packer.clone(),
            names.gateway.clone(),
        ]),
        EngineCall::network_rm(&[names.egress.clone(), names.internal.clone()]),
        EngineCall::volume_rm(std::slice::from_ref(&names.volume)),
    ]
    .iter()
    .map(|call| succeeded(context, call))
    .collect::<Vec<bool>>()
    .into_iter()
    .all(|ok| ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::workshop::tar::{
        PACKAGE_BUNDLE, PACKAGE_DIR, PACKAGE_MANIFEST, PACKAGE_TIP, stream_of,
    };

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// D-229: the tip and the manifest stay in memory; the bundle alone
    /// reaches the disk, under Officina's name, whatever the chunking.
    #[test]
    fn only_work_bundle_reaches_the_disk() {
        let stream = stream_of(&[
            (PACKAGE_DIR, b"", b'5'),
            (
                PACKAGE_TIP,
                format!("{}\n", "a".repeat(40)).as_bytes(),
                b'0',
            ),
            (PACKAGE_MANIFEST, b"{\"steps\":[]}", b'0'),
            (PACKAGE_BUNDLE, b"# v2 git bundle\nbytes", b'0'),
        ]);
        for chunk in [1, 7, 512, stream.len()] {
            let home = tempfile::tempdir().unwrap();
            let out = home.path().join("out");
            let mut writer = PackageWriter::new(&out).unwrap();
            for part in stream.chunks(chunk) {
                writer.push(part).unwrap();
            }
            let unpacked = writer.finish().unwrap();
            assert_eq!(files_in(&out), [BUNDLE_FILE]);
            assert_eq!(
                std::fs::read(out.join(BUNDLE_FILE)).unwrap(),
                b"# v2 git bundle\nbytes"
            );
            assert_eq!(
                unpacked.bundle_sha256.as_deref(),
                Some(officina_core::sha256_hex(b"# v2 git bundle\nbytes").as_str())
            );
            assert!(unpacked.received.tip.is_some());
        }
    }

    /// D-229: a name with a separator, a parent, a drive or an absolute
    /// root is refused before anything is written.
    #[test]
    fn a_path_leaving_the_out_folder_is_refused_before_any_write() {
        let home = tempfile::tempdir().unwrap();
        let out = home.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let mut names = vec!["../x", "..", "a/b", "/etc/x", ".", ""];
        if cfg!(windows) {
            names.extend(["C:x", "C:\\x", "\\\\server\\share\\x", "a\\b"]);
        }
        for name in names {
            assert!(checked_target(&out, name).is_err(), "{name:?}");
        }
        assert_eq!(files_in(&out), Vec::<String>::new());
        assert_eq!(
            checked_target(&out, BUNDLE_FILE).unwrap(),
            out.join(BUNDLE_FILE)
        );
    }
}
