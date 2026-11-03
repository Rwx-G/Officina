//! Collection, host verification, the run's outcome and `no_change` end to
//! end (story 6.13; AD-110, AD-111, D-218, D-219, D-229): a card launched
//! through the window's functions on the fake engine, whose packer copy
//! streams a package built by real git in a clone of the project (D-23),
//! then the worker's exit, which the window collects on its own.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::package::{MAX_PACKAGE_STREAM_BYTES, QuarantineCause};
use officina_core::workshop::run::{RemovalCause, RunState, WorkshopOutcome};
use officina_core::workshop::tar::PACKAGE_TIP;
use officina_host::registry::workshop::RunRecord;
use officina_host::workshop::follow::reconcile_now;

use common::package_run::{Run, base_env, git_text, home, launch, result, said, until};
use common::workshop_home::{Home, Options, SEEDED};

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
            QuarantineCause::CredentialFound => {
                run.commit(&clone, "a.txt", SEEDED);
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

/// AD-111 step 3, D-225: a bundle holding the work branch and another
/// reference at the same commit is `extra_ref`, never a package: the case
/// CI's `mutants` job expects `mutant-accept-extra-ref` to turn red.
#[test]
fn a_bundle_with_an_extra_reference_is_quarantined() {
    const GUARD: &str = "(guarded by mutant-accept-extra-ref)";
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    let base = run.row().base_commit;
    let branch = format!("refs/heads/{}", run.row().branch);
    run.commit(&clone, "a.txt", "a\n");
    git_text(&home, &clone, &["branch", "extra"]);
    run.serve(
        &run.packed(
            &clone,
            Some((&[branch.as_str(), "refs/heads/extra"], &base)),
        ),
        0,
    );
    run.exit(0);
    let row = run.collected();
    assert_eq!(row.state, RunState::Quarantined, "{GUARD}");
    assert_eq!(
        quarantined(&row),
        Some(QuarantineCause::ExtraRef),
        "{GUARD}"
    );
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

// Story 6.14: the credential scan (AD-112, D-212, D-230, D-237, D-609).

/// Base64 of `bytes`, standard alphabet with padding: the test derives
/// every encoded form of the seeded value, never types one (AD-112).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The seeded value raw, in base64 and in hex, by name.
fn forms() -> Vec<(&'static str, String)> {
    let value = SEEDED.as_bytes();
    vec![
        ("raw", SEEDED.to_owned()),
        ("base64", base64(value)),
        ("hex", value.iter().map(|b| format!("{b:02x}")).collect()),
    ]
}

/// A home whose fake engine replays `worker` and `gateway` lines.
fn home_with(worker: &[String], gateway: &[String]) -> Option<Home> {
    let home = Home::start(&Options::default())?;
    home.set_engine(
        "follow",
        serde_json::json!({"worker": {"stdout": worker.join("\n")}, "gateway": {"stdout": gateway.join("\n")}}),
    );
    Some(home)
}

/// Every text the registry keeps of `row`, its facts included.
fn stored(row: &RunRecord) -> String {
    format!(
        "{}{}",
        serde_json::to_string(&row.texts).unwrap(),
        serde_json::to_string(&row.facts).unwrap()
    )
}

/// Where the scan found the value, as the row keeps it.
fn places(row: &RunRecord) -> Vec<String> {
    row.texts
        .as_ref()
        .map(|t| {
            t.credential_locations
                .iter()
                .map(|l| {
                    serde_json::to_value(l)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A run whose worker's clone gets `change` before the package, collected
/// and checked quarantined `credential_found` at `place`, nothing of the
/// value stored and its package deleted.
fn quarantines_at(form: &str, place: &str, change: impl FnOnce(&Run, &Path)) {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    change(&run, &clone);
    run.serve(&run.packed_branch(&clone), 0);
    run.exit(0);
    let row = run.collected();
    assert_eq!(row.state, RunState::Quarantined, "{form} at {place}");
    assert_eq!(quarantined(&row), Some(QuarantineCause::CredentialFound));
    assert!(
        places(&row).iter().any(|p| p.starts_with(place)),
        "{form}: {:?}",
        places(&row)
    );
    assert!(row.facts.credential_locations.is_some_and(|n| n >= 1));
    assert!(!stored(&row).contains(form), "{form} stored");
    assert!(!run.run_dir().join("out").exists());
    assert!(!run.run_dir().join("verify.git").exists());
    assert!(run.lines().iter().any(|l| matches!(l,
        JournalEntry::ReviewPackage(p) if p.credential_locations == row.facts.credential_locations)));
}

#[test]
fn a_token_in_a_blob_quarantines() {
    for (name, form) in forms() {
        quarantines_at(name, "file notes.txt", |run, clone| {
            run.commit(clone, "notes.txt", &format!("key = {form}\n"));
        });
    }
}

#[test]
fn a_token_in_a_commit_message_quarantines() {
    for (name, form) in forms() {
        quarantines_at(name, "commit message", |run, clone| {
            std::fs::write(clone.join("a.txt"), "a\n").unwrap();
            git_text(run.home, clone, &["add", "a.txt"]);
            git_text(
                run.home,
                clone,
                &["commit", "-q", "-m", &format!("use {form}")],
            );
        });
    }
}

#[test]
fn a_token_in_a_path_quarantines() {
    for (name, form) in forms() {
        let path = format!("keys/{}.txt", form.replace('/', "_"));
        quarantines_at(name, "path", |run, clone| {
            run.commit(clone, &path, "harmless\n");
        });
    }
}

#[test]
fn a_token_in_an_author_email_quarantines() {
    for (name, form) in forms() {
        quarantines_at(name, "author", |run, clone| {
            std::fs::write(clone.join("a.txt"), "a\n").unwrap();
            git_text(run.home, clone, &["add", "a.txt"]);
            let email = format!("user.email={}@example.com", form.replace('/', "_"));
            git_text(
                run.home,
                clone,
                &["-c", &email, "commit", "-q", "-m", "plain"],
            );
        });
    }
}

/// D-237: coreutils `base64` wraps at 76 columns; the form is found
/// across the line break.
#[test]
fn a_wrapped_base64_token_in_a_blob_quarantines() {
    let mut text = "p".repeat(50).into_bytes();
    text.extend_from_slice(SEEDED.as_bytes());
    text.extend_from_slice(&[b's'; 40]);
    let encoded = base64(&text);
    let wrapped: Vec<String> = encoded
        .as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8(c.to_vec()).unwrap())
        .collect();
    assert!(wrapped.len() > 1, "the form spans a line break");
    quarantines_at("wrapped", "file encoded.txt", |run, clone| {
        run.commit(clone, "encoded.txt", &format!("{}\n", wrapped.join("\n")));
    });
}

/// A run whose worker said `report` and whose gateway wrote `gateway`,
/// with one ordinary commit, collected.
fn found_in_texts(report: &str, gateway: &[String]) -> Option<(Home, String)> {
    let home = home_with(&[said(report), result()], gateway)?;
    let id = {
        let run = launch(&home, home.seeded());
        let clone = run.clone();
        run.commit(&clone, "src/lib.rs", "pub fn bumped() {}\n");
        run.serve(&run.packed_branch(&clone), 0);
        run.exit(0);
        run.collected();
        run.id
    };
    Some((home, id))
}

#[test]
fn a_token_in_the_report_tail_is_found() {
    for (name, form) in forms() {
        let Some((home, id)) = found_in_texts(&format!("done, used {form}"), &[]) else {
            return;
        };
        let row = home.run(&id);
        assert!(places(&row).contains(&"report".to_owned()), "{name}");
        let tail = serde_json::to_string(&row.texts.as_ref().unwrap().report_tail).unwrap();
        assert!(
            tail.contains("[credential]") && !tail.contains(&form),
            "{name}: {tail}"
        );
        assert_eq!(quarantined(&row), Some(QuarantineCause::CredentialFound));
    }
}

#[test]
fn a_token_in_a_refused_host_is_found() {
    use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};
    for (name, form) in forms() {
        if form.contains(['+', '/', '=']) {
            // Not a host name's characters: the raw and hex forms are.
            continue;
        }
        let host = format!("{form}.example.com");
        let line = GatewayLine::refused(&host, 443, GatewayRefusal::NotAllowed, None).to_line();
        let Some((home, id)) = found_in_texts("done", &[line]) else {
            return;
        };
        let row = home.run(&id);
        assert!(places(&row).contains(&"gateway line".to_owned()), "{name}");
        assert!(!stored(&row).contains(&form), "{name}");
        let journal = serde_json::to_string(
            &home
                .env
                .journal()
                .into_iter()
                .map(|l| l.entry)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(!journal.contains(&form), "{name}: journaled");
    }
}

/// AD-112: a `no_change` run's texts are scanned and replaced too; it
/// carries the count for the revoke advice, with no package to quarantine.
#[test]
fn a_token_in_a_no_change_run_is_replaced() {
    let Some(home) = home_with(&[said(&format!("nothing to do, {SEEDED}")), result()], &[]) else {
        return;
    };
    let run = a_no_change(&home);
    let row = run.row();
    assert_eq!(row.outcome, Some(WorkshopOutcome::NoChange));
    assert_eq!(quarantined(&row), None);
    assert_eq!(row.facts.credential_locations, Some(1));
    assert!(!stored(&row).contains(SEEDED));
    assert!(stored(&row).contains("[credential]"));
}

/// D-230: the run's needles leave the window with its entry once
/// collection's scan ran.
#[test]
fn needles_are_dropped_after_collection() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home);
    run.collected();
    assert!(run.runtime.followed.lock().unwrap().get(&run.id).is_none());
}

/// D-609: the pass applies ranges alike whatever their source: the run's
/// needles, or a fixed range list standing for the `Scanner`.
#[test]
fn ranges_from_any_source_are_applied_alike() {
    use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};
    use officina_core::workshop::output::LineRead;
    use officina_core::workshop::output::Pipe;
    use officina_core::workshop::scan::{Place, ScanRecord, ScanSource, needles};
    use officina_core::workshop::vector::ContainerRole;
    use officina_host::workshop::collect::{TextRecords, scan_pass};
    use officina_host::workshop::follow::Followed;
    type Ranges = Vec<Vec<std::ops::Range<usize>>>;
    struct Fixed(Ranges);
    impl ScanSource for Fixed {
        fn ranges(&mut self, records: &[&[u8]]) -> Option<Ranges> {
            (records.len() == self.0.len()).then(|| self.0.clone())
        }
    }
    let host = format!("{SEEDED}.example.com");
    let mut followed = Followed::new(None);
    let lines = [
        (ContainerRole::Worker, said(&format!("used {SEEDED}"))),
        (
            ContainerRole::Gateway,
            GatewayLine::refused(&host, 443, GatewayRefusal::NotAllowed, None).to_line(),
        ),
    ];
    for (role, line) in lines {
        followed.absorb("r", role, Pipe::Stdout, &LineRead::Line(line));
    }
    let texts = TextRecords::of(&followed);
    let records = vec![ScanRecord {
        place: Place::File(None),
        bytes: format!("x{SEEDED}").into_bytes(),
    }];
    let found = needles(SEEDED.as_bytes());
    let by_needles = scan_pass(&texts, Some(b"{}"), &records, &mut &found).unwrap();
    let report = format!("used {SEEDED}").into_bytes();
    let all: Vec<&[u8]> = vec![&report, host.as_bytes(), b"{}", &records[0].bytes];
    let ranges = (&found).ranges(&all).unwrap();
    let by_list = scan_pass(&texts, Some(b"{}"), &records, &mut Fixed(ranges)).unwrap();
    assert_eq!(by_needles, by_list);
    assert_eq!(by_list.locations, ["report", "gateway line", "file"]);
    assert_eq!(by_list.released.len(), 1);
    assert!(!format!("{by_list:?}").contains(SEEDED));
    // A source that cannot scan releases nothing.
    assert!(scan_pass(&texts, Some(b"{}"), &records, &mut Fixed(Vec::new())).is_none());
}

#[test]
fn found_deletes_out_and_verify_at_once() {
    quarantines_at("raw", "file leak.txt", |run, clone| {
        run.commit(clone, "leak.txt", SEEDED);
    });
}

#[test]
fn stored_texts_hold_the_placeholder_not_the_value() {
    let Some((home, id)) = found_in_texts(&format!("token {SEEDED}"), &[]) else {
        return;
    };
    let row = home.run(&id);
    assert!(stored(&row).contains("[credential]"));
    for (name, form) in forms() {
        assert!(!stored(&row).contains(&form), "{name}");
    }
}

/// D-212: the count lives in the facts, so the review view carries the
/// revoke advice once retention cleared the texts.
#[test]
fn the_revoke_advice_outlives_the_texts() {
    let Some((home, id)) = found_in_texts(&format!("token {SEEDED}"), &[]) else {
        return;
    };
    let mut row = home.run(&id);
    let count = row.facts.credential_locations;
    assert!(count.is_some_and(|n| n >= 1));
    row.texts = None;
    let inputs = officina_host::workshop::review::inputs_of(
        &officina_host::workshop::review::StoredReview {
            row,
            approvals: Vec::new(),
        },
    );
    assert_eq!(inputs.facts.credential_locations, count);
    assert!(inputs.facts.credential_places.is_empty());
}
