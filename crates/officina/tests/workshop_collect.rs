//! Collection, host verification, the run's outcome and `no_change` end to
//! end (story 6.13; AD-110, AD-111, D-218, D-219, D-229): a card launched
//! through the window's functions on the fake engine, whose packer copy
//! streams a package built by real git in a clone of the project (D-23),
//! then the worker's exit, which the window collects on its own.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::package::{MAX_PACKAGE_STREAM_BYTES, QuarantineCause};
use officina_core::workshop::run::{RemovalCause, RunState, WorkshopOutcome};
use officina_core::workshop::tar::{
    PACKAGE_BUNDLE, PACKAGE_DIR, PACKAGE_MANIFEST, PACKAGE_TIP, stream_of,
};
use officina_host::registry::workshop::RunRecord;
use officina_host::workshop::follow::{FollowContext, reconcile_now};
use officina_host::workshop::launch::WorkshopRuntime;

use common::git_support::run_git;
use common::workshop_home::{Home, Options};

const WAIT: Duration = Duration::from_secs(60);

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn base_env() -> Vec<(String, String)> {
    vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )]
}

fn said(text: &str) -> String {
    serde_json::json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}).to_string()
}

fn result() -> String {
    serde_json::json!({"type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.02, "num_turns": 2, "duration_ms": 900, "session_id": "s"}).to_string()
}

/// One package entry: its name in the stream and its bytes.
type Entry = (String, Vec<u8>);

/// A launched run of a home.
struct Run<'h> {
    home: &'h Home,
    runtime: WorkshopRuntime,
    id: String,
}

/// A home whose fake engine replays `worker` as the worker's output.
fn home(options: &Options, worker: &[String]) -> Option<Home> {
    let home = Home::start(options)?;
    home.set_engine(
        "follow",
        serde_json::json!({"worker": {"stdout": worker.join("\n")}, "gateway": {"stdout": ""}}),
    );
    Some(home)
}

/// A card issued and launched with `runtime`.
fn launch<'h>(home: &'h Home, runtime: WorkshopRuntime) -> Run<'h> {
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    Run {
        home,
        runtime,
        id: card.id,
    }
}

fn git_text(home: &Home, dir: &Path, args: &[&str]) -> String {
    let out = run_git(&home.git, dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

impl Run<'_> {
    fn row(&self) -> RunRecord {
        self.home.run(&self.id)
    }

    fn run_dir(&self) -> PathBuf {
        self.home
            .env
            .officina_home()
            .join("workshop")
            .join("runs")
            .join(&self.id)
    }

    /// The worker's clone: the project cloned, on the run's branch.
    fn clone(&self) -> PathBuf {
        let dir = self
            .home
            .env
            .root
            .path()
            .join(format!("clone-{}", &self.id[..8]));
        let project = self.home.project();
        git_text(
            self.home,
            self.home.env.root.path(),
            &[
                "clone",
                "-q",
                &project.to_string_lossy(),
                &dir.to_string_lossy(),
            ],
        );
        git_text(
            self.home,
            &dir,
            &["checkout", "-q", "-b", &self.row().branch],
        );
        dir
    }

    fn commit(&self, clone: &Path, path: &str, text: &str) {
        let file = clone.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, text).unwrap();
        git_text(self.home, clone, &["add", "-f", "--", path]);
        git_text(
            self.home,
            clone,
            &["commit", "-q", "-m", &format!("add {path}")],
        );
    }

    /// What the packer writes from `clone` (AD-110): the tip, a bundle of
    /// `refs` past `exclude` when asked, a manifest.
    fn packed(&self, clone: &Path, bundle: Option<(&[&str], &str)>) -> Vec<Entry> {
        let tip = git_text(self.home, clone, &["rev-parse", "HEAD"]);
        let mut entries = vec![
            (PACKAGE_DIR.to_owned(), Vec::new()),
            (PACKAGE_TIP.to_owned(), format!("{tip}\n").into_bytes()),
            (PACKAGE_MANIFEST.to_owned(), b"{\"steps\":[]}".to_vec()),
        ];
        if let Some((refs, exclude)) = bundle {
            let path = clone.join("work.bundle.tmp");
            let mut args = vec!["bundle", "create", "-q"];
            let target = path.to_string_lossy().into_owned();
            args.push(&target);
            args.extend_from_slice(refs);
            let excluded = format!("^{exclude}");
            args.push(&excluded);
            git_text(self.home, clone, &args);
            entries.push((PACKAGE_BUNDLE.to_owned(), std::fs::read(&path).unwrap()));
            std::fs::remove_file(&path).unwrap();
        }
        entries
    }

    /// A bundle of the run's branch past the base.
    fn packed_branch(&self, clone: &Path) -> Vec<Entry> {
        let branch = format!("refs/heads/{}", self.row().branch);
        let base = self.row().base_commit;
        self.packed(clone, Some((&[branch.as_str()], &base)))
    }

    /// The fake engine's packer copy streams `entries`, then `extra` zero
    /// bytes.
    fn serve(&self, entries: &[Entry], extra: u64) {
        let list: Vec<(&str, &[u8], u8)> = entries
            .iter()
            .map(|(name, bytes)| {
                let kind = if name.ends_with('/') { b'5' } else { b'0' };
                (name.as_str(), bytes.as_slice(), kind)
            })
            .collect();
        let path = self
            .home
            .env
            .root
            .path()
            .join(format!("package-{}.tar", &self.id[..8]));
        std::fs::write(&path, stream_of(&list)).unwrap();
        self.home
            .set_engine("package", path.to_string_lossy().into_owned().into());
        self.home.set_engine("package_extra_bytes", extra.into());
    }

    /// The worker's container exits with `code`.
    fn exit(&self, code: i64) {
        let path = self.home.engine.dir.path().join("fake-docker-state.json");
        let mut all: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let worker = format!("officina-worker-{}", self.id);
        for c in all["containers"].as_array_mut().unwrap() {
            if c["name"] == worker.as_str() {
                c["state"] = "exited".into();
                c["exit_code"] = code.into();
            }
        }
        std::fs::write(&path, all.to_string()).unwrap();
    }

    /// The run once collection decided it.
    fn collected(&self) -> RunRecord {
        until("the run's outcome", || self.row().outcome.is_some());
        self.home.settle();
        self.row()
    }

    fn lines(&self) -> Vec<JournalEntry> {
        self.home
            .env
            .journal()
            .into_iter()
            .map(|line| line.entry)
            .filter(|entry| run_of(entry) == Some(self.id.as_str()))
            .collect()
    }

    fn context(&self) -> FollowContext {
        FollowContext {
            runtime: self.runtime.clone(),
            collector: self.home.sender().clone(),
            home: self.home.env.officina_home(),
            base_env: base_env(),
        }
    }
}

fn run_of(entry: &JournalEntry) -> Option<&str> {
    match entry {
        JournalEntry::WorkshopFinished(l) => Some(&l.run_id),
        JournalEntry::WorkshopRemoved(l) => Some(&l.run_id),
        JournalEntry::GatewaySummary(l) => Some(&l.run_id),
        JournalEntry::ReviewPackage(l) => Some(&l.run_id),
        JournalEntry::ReviewOpened(l) => Some(&l.run_id),
        _ => None,
    }
}

fn finished(lines: &[JournalEntry]) -> Vec<WorkshopOutcome> {
    lines
        .iter()
        .filter_map(|line| match line {
            JournalEntry::WorkshopFinished(l) => Some(l.outcome),
            _ => None,
        })
        .collect()
}

fn review_packages(lines: &[JournalEntry]) -> usize {
    lines
        .iter()
        .filter(|line| matches!(line, JournalEntry::ReviewPackage(_)))
        .count()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
}

fn quarantined(row: &RunRecord) -> Option<QuarantineCause> {
    row.quarantine_cause
        .as_deref()
        .and_then(QuarantineCause::from_code)
}

/// A run whose worker committed one file, collected.
fn a_package<'h>(home: &'h Home) -> Run<'h> {
    let run = launch(home, home.seeded());
    let clone = run.clone();
    run.commit(&clone, "src/lib.rs", "pub fn bumped() {}\n");
    run.serve(&run.packed_branch(&clone), 0);
    run.exit(0);
    run
}

/// A run that changed nothing and exited 0, collected.
fn a_no_change(home: &Home) -> Run<'_> {
    let run = launch(home, home.seeded());
    let clone = run.clone();
    run.serve(&run.packed(&clone, None), 0);
    run.exit(0);
    run.collected();
    run
}

// AC 1: the packer and the copy.

/// D-218: past the copy's cap the stream is stopped, the package is
/// `too_large` and nothing is written.
#[test]
fn an_oversized_stream_writes_nothing() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.commit(&clone, "a.txt", "a\n");
    run.serve(&run.packed_branch(&clone), MAX_PACKAGE_STREAM_BYTES);
    run.exit(0);
    let row = run.collected();
    assert_eq!(row.state, RunState::Quarantined);
    assert_eq!(quarantined(&row), Some(QuarantineCause::TooLarge));
    assert!(files_under(&run.run_dir().join("out")).is_empty());
}

/// AD-110: the packer gets no network, no credential and no mount but
/// the run's volume; its branch and base come from its command.
#[test]
fn the_packer_has_no_network_no_credential_no_mount() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home);
    run.collected();
    let packer = format!("officina-pack-{}", run.id);
    let log = home.engine.log();
    let create = log
        .iter()
        .find(|line| {
            let argv = line["argv"].as_array().unwrap();
            argv.first().is_some_and(|a| a == "create") && argv.iter().any(|a| a == packer.as_str())
        })
        .expect("the packer's create");
    let argv: Vec<&str> = create["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    let after = |flag: &str| -> Vec<&str> {
        argv.windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1])
            .collect()
    };
    assert_eq!(after("--network"), ["none"]);
    let mounts = after("--mount");
    assert_eq!(mounts.len(), 1, "{mounts:?}");
    assert!(mounts[0].starts_with("type=volume,"), "{mounts:?}");
    let env_names: Vec<&str> = create["env_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    for name in ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        assert!(!env_names.contains(&name), "{env_names:?}");
        assert!(!argv.iter().any(|a| a.contains(name)), "{argv:?}");
    }
    let row = run.row();
    assert!(
        argv.windows(2)
            .any(|p| p == ["--branch", row.branch.as_str()])
    );
    assert!(
        argv.windows(2)
            .any(|p| p == ["--base", row.base_commit.as_str()])
    );
}

/// AD-116: nothing is collected while the workshop is stopped.
#[test]
fn nothing_is_collected_while_stopped() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let flag = officina_host::workshop::stopped_file(&home.env.officina_home());
    std::fs::write(&flag, b"").unwrap();
    let from = home.engine.log().len();
    run.exit(0);
    until("collecting", || run.row().state == RunState::Collecting);
    std::thread::sleep(Duration::from_millis(1500));
    home.settle();
    assert_eq!(run.row().state, RunState::Collecting);
    let packer = format!("officina-pack-{}", run.id);
    assert!(
        !home
            .calls(from)
            .iter()
            .any(|argv| argv.iter().any(|a| a == packer.as_str())),
        "the packer ran while stopped"
    );
}

/// Review F53: a partial `out/` an interrupted collection left is
/// deleted before the copy.
#[test]
fn a_partial_out_is_deleted_before_the_copy() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let out = run.run_dir().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("stale.txt"), b"left by an interrupted copy").unwrap();
    let clone = run.clone();
    run.commit(&clone, "a.txt", "a\n");
    run.serve(&run.packed_branch(&clone), 0);
    run.exit(0);
    let row = run.collected();
    assert_eq!(row.state, RunState::Review);
    assert_eq!(files_under(&out), [out.join("work.bundle")]);
}

// AC 2: names.

/// D-229: a name outside the allow-list is `bundle_invalid` and never
/// reaches the disk.
#[test]
fn an_extra_file_in_out_is_bundle_invalid_and_never_written() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.commit(&clone, "a.txt", "a\n");
    let mut entries = run.packed_branch(&clone);
    entries.insert(1, ("out/evil.txt".to_owned(), b"planted".to_vec()));
    run.serve(&entries, 0);
    run.exit(0);
    let row = run.collected();
    assert_eq!(quarantined(&row), Some(QuarantineCause::BundleInvalid));
    assert!(
        !files_under(&run.run_dir())
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "evil.txt"))
    );
    assert!(files_under(&run.run_dir().join("out")).is_empty());
}

// AC 3: host verification.

/// AD-111: each quarantine cause from a package built by real git.
#[test]
fn each_quarantine_cause_from_a_crafted_package() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    for cause in QuarantineCause::ALL.iter().copied() {
        let run = launch(&home, home.seeded());
        let clone = run.clone();
        let base = run.row().base_commit;
        let branch = format!("refs/heads/{}", run.row().branch);
        let mut extra = 0;
        let entries = match cause {
            QuarantineCause::BranchMissing => {
                let mut entries = run.packed(&clone, None);
                entries.retain(|(name, _)| name != PACKAGE_TIP);
                entries
            }
            QuarantineCause::BundleInvalid => {
                run.commit(&clone, "a.txt", "a\n");
                let mut entries = run.packed_branch(&clone);
                entries.last_mut().unwrap().1 = b"not a bundle\n".to_vec();
                entries
            }
            QuarantineCause::TooLarge => {
                run.commit(&clone, "a.txt", "a\n");
                extra = MAX_PACKAGE_STREAM_BYTES;
                run.packed_branch(&clone)
            }
            QuarantineCause::WrongBase => {
                run.commit(&clone, "a.txt", "a\n");
                let other = git_text(&home, &clone, &["rev-parse", "HEAD"]);
                run.commit(&clone, "b.txt", "b\n");
                run.packed(&clone, Some((&[branch.as_str()], &other)))
            }
            QuarantineCause::ExtraRef => {
                run.commit(&clone, "a.txt", "a\n");
                git_text(&home, &clone, &["branch", "extra"]);
                run.packed(
                    &clone,
                    Some((&[branch.as_str(), "refs/heads/extra"], &base)),
                )
            }
            QuarantineCause::MountCommitted => {
                run.commit(&clone, ".claude/settings.json", "{}\n");
                run.packed_branch(&clone)
            }
        };
        run.serve(&entries, extra);
        run.exit(0);
        let row = run.collected();
        assert_eq!(row.state, RunState::Quarantined, "{cause:?}");
        assert_eq!(quarantined(&row), Some(cause), "{cause:?}");
        assert_eq!(review_packages(&run.lines()), 1, "{cause:?}");
    }
}

/// D-219, review F38: a forged tip and a path shaped like an option
/// never reach git as an argument.
#[test]
fn a_forged_tip_never_reaches_git_as_an_option() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let logger = common::workshop::GitLogger::new(&home.git);
    let mut runtime = home.seeded();
    runtime.git = Some(logger.exe.clone());
    // A tip that is not an object id.
    let forged = launch(&home, runtime.clone());
    let clone = forged.clone();
    let mut entries = forged.packed(&clone, None);
    for (name, bytes) in &mut entries {
        if name == PACKAGE_TIP {
            *bytes = b"--upload-pack=forged-tip\n".to_vec();
        }
    }
    forged.serve(&entries, 0);
    forged.exit(0);
    assert_eq!(
        quarantined(&forged.collected()),
        Some(QuarantineCause::BranchMissing)
    );
    // A committed path shaped like an option.
    let shaped = launch(&home, runtime);
    let clone = shaped.clone();
    shaped.commit(&clone, "--output=forged-path", "x\n");
    shaped.serve(&shaped.packed_branch(&clone), 0);
    shaped.exit(0);
    assert_eq!(shaped.collected().state, RunState::Review);
    let log = logger.log();
    assert!(log.len() > 3, "{log:?}");
    for argv in log {
        let text = argv.to_string();
        assert!(!text.contains("forged-tip"), "{text}");
        assert!(!text.contains("forged-path"), "{text}");
    }
}

/// AD-111: verification borrows the project's objects read-only.
#[test]
fn verification_never_writes_the_project_objects() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let objects = home.project().join(".git").join("objects");
    let listing = || -> Vec<(PathBuf, u64)> {
        files_under(&objects)
            .into_iter()
            .map(|p| (p.clone(), std::fs::metadata(&p).unwrap().len()))
            .collect()
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.commit(&clone, "a.txt", "a\n");
    let entries = run.packed_branch(&clone);
    let before = listing();
    run.serve(&entries, 0);
    run.exit(0);
    assert_eq!(run.collected().state, RunState::Review);
    assert_eq!(listing(), before);
}

/// Review F17: a SHA-256 project is verified in a SHA-256 scratch
/// repository.
#[test]
fn a_sha256_project_verifies_in_a_sha256_scratch_repository() {
    let options = Options {
        sha256: true,
        ..Options::default()
    };
    let Some(home) = home(&options, &[]) else {
        return;
    };
    let run = a_package(&home);
    let row = run.collected();
    assert_eq!(row.state, RunState::Review, "{row:?}");
    assert_eq!(row.tip_commit.as_deref().map(str::len), Some(64));
    let scratch = run.run_dir().join("verify.git");
    assert_eq!(
        git_text(&home, &scratch, &["rev-parse", "--show-object-format"]),
        "sha256"
    );
}

// AC 4: the outcome.

#[test]
fn exit_0_and_no_commit_is_no_change() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_no_change(&home);
    let row = run.row();
    assert_eq!(row.state, RunState::NoChange);
    assert_eq!(row.outcome, Some(WorkshopOutcome::NoChange));
    assert_eq!(row.commits_past_base, Some(0));
}

#[test]
fn a_non_zero_exit_and_no_commit_is_failed() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.serve(&run.packed(&clone, None), 0);
    run.exit(1);
    let row = run.collected();
    assert_eq!(row.state, RunState::Failed);
    assert_eq!(row.outcome, Some(WorkshopOutcome::Failed));
    assert_eq!(finished(&run.lines()), [WorkshopOutcome::Failed]);
}

/// AD-110: no stream line or report enters the decision.
#[test]
fn a_report_claiming_no_changes_with_a_commit_is_a_package() {
    let worker = [
        said("No changes were needed; nothing was committed."),
        result(),
    ];
    let Some(home) = home(&Options::default(), &worker) else {
        return;
    };
    let run = a_package(&home);
    let row = run.collected();
    assert_eq!(row.state, RunState::Review);
    assert_eq!(row.outcome, Some(WorkshopOutcome::Package));
    assert_eq!(row.commits_past_base, Some(1));
}

/// R65: a worker that resets its branch to the base produced nothing.
#[test]
fn a_branch_reset_to_the_base_is_no_change() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.commit(&clone, "a.txt", "a\n");
    git_text(
        &home,
        &clone,
        &["reset", "-q", "--hard", &run.row().base_commit],
    );
    run.serve(&run.packed(&clone, None), 0);
    run.exit(0);
    assert_eq!(run.collected().state, RunState::NoChange);
}

// AC 5: `no_change`.

#[test]
fn no_change_writes_no_review_package() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_no_change(&home);
    let lines = run.lines();
    assert_eq!(review_packages(&lines), 0);
    assert_eq!(finished(&lines), [WorkshopOutcome::NoChange]);
}

#[test]
fn no_change_deletes_out_at_once() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_no_change(&home);
    assert!(!run.run_dir().join("out").exists());
    assert!(!run.run_dir().join("verify.git").exists());
}

#[test]
fn no_change_is_not_awaiting_a_verdict() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_no_change(&home);
    assert!(!run.row().state.awaits_verdict());
    let waiting = officina_host::registry::store::open_reader(&home.env.officina_home(), &[])
        .unwrap()
        .workshop_awaiting_verdict()
        .unwrap();
    assert!(waiting.iter().all(|row| row.id != run.id));
}

#[test]
fn no_change_offers_neither_import_nor_discard() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_no_change(&home);
    let view = officina_host::workshop::review::open_review(
        home.sender(),
        &run.runtime,
        &home.env.officina_home(),
        &base_env(),
        &run.id,
    )
    .unwrap();
    assert!(view.nothing_to_review);
    assert!(view.commits.is_empty() && view.files.is_empty() && view.diff.is_none());
    assert_eq!(view.tip.as_deref(), Some(run.row().base_commit.as_str()));
    // Not a package: its Receive is no review (PRD section 7).
    assert!(
        !run.lines()
            .iter()
            .any(|l| matches!(l, JournalEntry::ReviewOpened(_)))
    );
}

// AC 6: removal and records.

/// Amendment 10: once the package is on the host, every engine object of
/// the run is removed.
#[test]
fn the_credential_leaves_the_engine_after_collection() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home);
    run.collected();
    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.engine.dir.path().join("fake-docker-state.json")).unwrap(),
    )
    .unwrap();
    for key in ["containers", "networks", "volumes"] {
        let left: Vec<&serde_json::Value> = state[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["name"].as_str().is_some_and(|n| n.ends_with(&run.id)))
            .collect();
        assert!(left.is_empty(), "{key}: {left:?}");
    }
    assert!(run.lines().iter().any(|l| matches!(
        l,
        JournalEntry::WorkshopRemoved(r) if r.cause == RemovalCause::Collected
    )));
    assert!(!run.run_dir().join("in").exists());
    let summary = run.lines().into_iter().find_map(|l| match l {
        JournalEntry::GatewaySummary(s) => Some(s),
        _ => None,
    });
    assert!(summary.is_some());
}

/// AD-120: one `workshop_finished` per run, whatever reconciles later.
#[test]
fn one_workshop_finished_per_run() {
    let Some(home) = home(&Options::default(), &[result()]) else {
        return;
    };
    let run = a_package(&home);
    run.collected();
    reconcile_now(&run.context());
    home.settle();
    let lines = run.lines();
    assert_eq!(finished(&lines), [WorkshopOutcome::Package]);
    assert_eq!(review_packages(&lines), 1);
    let line = lines
        .iter()
        .find_map(|l| match l {
            JournalEntry::WorkshopFinished(f) => Some(f.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(line.commits_past_base, Some(1));
    assert_eq!(line.tip, run.row().tip_commit);
    assert_eq!(line.cost_usd_micros, Some(20_000));
}

// AC 7: the review view.

/// D-249: the first Receive of a package journals `review_opened`, once.
#[test]
fn review_opened_is_journaled_once() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home);
    run.collected();
    let open = || {
        officina_host::workshop::review::open_review(
            home.sender(),
            &run.runtime,
            &home.env.officina_home(),
            &base_env(),
            &run.id,
        )
        .unwrap()
    };
    let view = open();
    open();
    let opened = run
        .lines()
        .iter()
        .filter(|l| matches!(l, JournalEntry::ReviewOpened(_)))
        .count();
    assert_eq!(opened, 1);
    assert!(!view.nothing_to_review);
    assert_eq!(view.commits.len(), 1);
    assert_eq!(view.files.len(), 1);
    assert!(view.diff.is_some_and(|d| d.files.len() == 1));
    assert!(run.row().opened_at_ms.is_some());
}
