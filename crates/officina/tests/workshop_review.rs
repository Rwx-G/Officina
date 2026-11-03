//! Sensitive groups and their approvals (story 6.15; AD-113, D-219,
//! D-239), Import and Discard (story 6.16; AD-114) end to end: packages
//! built by real git in a clone of the project (D-23), collected by the
//! window on the fake engine, then reviewed, approved and decided through
//! the functions the window's commands call.

mod common;

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::review::{
    ApprovalAnswer, ApprovalRefusal, DiscardAnswer, DiscardReason, DiscardRefusal, ImportAnswer,
    ImportRefusal, NotReadyCause, PackageVerdict, ReviewGroup, ReviewView,
};
use officina_core::workshop::run::RunState;
use officina_core::workshop::sensitive::{ChangedFile, SensitiveGroup, hunks_sha256};
use officina_host::workshop::launch::WorkshopRuntime;

use common::package_run::{Run, base_env, git_text, home, launch, references, work_tree};
use common::workshop::GitLogger;
use common::workshop_home::{Home, Options};

/// A run of `runtime` whose worker committed `files`, collected.
fn a_package<'h>(home: &'h Home, runtime: WorkshopRuntime, files: &[(&str, &str)]) -> Run<'h> {
    let run = launch(home, runtime);
    let clone = run.clone();
    for (path, text) in files {
        run.commit(&clone, path, text);
    }
    run.serve(&run.packed_branch(&clone), 0);
    run.exit(0);
    run.collected();
    run
}

fn open(home: &Home, runtime: &WorkshopRuntime, id: &str) -> ReviewView {
    officina_host::workshop::review::open_review(
        home.sender(),
        runtime,
        &home.env.officina_home(),
        &base_env(),
        id,
    )
    .unwrap()
}

fn approve(
    home: &Home,
    runtime: &WorkshopRuntime,
    id: &str,
    key: &str,
    hash: &str,
) -> ApprovalAnswer {
    officina_host::workshop::review::approve_sensitive(
        home.sender(),
        runtime,
        &home.env.officina_home(),
        &base_env(),
        id,
        key,
        hash,
    )
    .unwrap()
}

fn group(view: &ReviewView, family: SensitiveGroup) -> &ReviewGroup {
    view.groups
        .iter()
        .find(|g| g.group == family)
        .unwrap_or_else(|| panic!("no {family:?} group in {:?}", view.groups))
}

fn approvals(run: &Run<'_>) -> Vec<(String, String)> {
    run.lines()
        .into_iter()
        .filter_map(|line| match line {
            JournalEntry::SensitiveApproved(l) => Some((l.group, l.hunks_sha256)),
            _ => None,
        })
        .collect()
}

const BUILD_RS: &str = "fn main() {}\n";

// AC 2: hunks.

/// D-219: a group's hunks come from the old and new blob ids `diff --raw`
/// reported, an added file's old side being the empty blob, and their
/// hash is over the raw bytes: what `diff --raw` said of the file, then
/// the blob diff.
#[test]
fn hunks_come_from_blob_ids() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let logger = GitLogger::new(&home.git);
    let mut runtime = home.seeded();
    runtime.git = Some(logger.exe.clone());
    let run = a_package(&home, runtime.clone(), &[("build.rs", BUILD_RS)]);
    let view = open(&home, &runtime, &run.id);
    let build = group(&view, SensitiveGroup::BuildScripts);
    assert!(build.approvable);
    let tip = run.row().tip_commit.unwrap();
    let scratch = run.run_dir().join("verify.git");
    let blob = git_text(&home, &scratch, &["rev-parse", &format!("{tip}:build.rs")]);
    let empty = git_text(&home, &scratch, &["hash-object", "-t", "blob", "--stdin"]);
    let diffed = logger.log().into_iter().any(|call| {
        let argv: Vec<&str> = call["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        argv.contains(&"diff") && argv.ends_with(&[empty.as_str(), blob.as_str()])
    });
    assert!(diffed, "no blob diff in {:?}", logger.log());
    let header = ChangedFile {
        status: "A".into(),
        old_mode: "000000".into(),
        new_mode: "100644".into(),
        old_id: "0".repeat(40),
        new_id: blob.clone(),
        paths: vec!["build.rs".into()],
        binary: false,
    }
    .hunk_header();
    let out = common::git_support::run_git(
        &home.git,
        &scratch,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            &empty,
            &blob,
        ],
    );
    let mut raw = header;
    raw.extend_from_slice(&out.stdout);
    assert_eq!(
        build.hunks_sha256.as_deref(),
        Some(hunks_sha256(&raw).as_str())
    );
    let shown = serde_json::to_string(build.hunks.as_ref().unwrap()).unwrap();
    assert!(shown.contains("+fn main() {}"), "{shown}");
}

/// D-219, review F38: a worker path starting with a dash is matched and
/// shown, and never reaches git as an argument, through collection, the
/// review and an approval.
#[test]
fn a_path_starting_with_a_dash_never_reaches_git() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let logger = GitLogger::new(&home.git);
    let mut runtime = home.seeded();
    runtime.git = Some(logger.exe.clone());
    let path = "-officina-dash-probe/build.rs";
    let run = a_package(&home, runtime.clone(), &[(path, BUILD_RS)]);
    let view = open(&home, &runtime, &run.id);
    let build = group(&view, SensitiveGroup::BuildScripts).clone();
    let shown = serde_json::to_string(&view.files).unwrap();
    assert!(shown.contains(path), "{shown}");
    let answer = approve(
        &home,
        &runtime,
        &run.id,
        &build.key,
        build.hunks_sha256.as_deref().unwrap(),
    );
    assert_eq!(answer, ApprovalAnswer::Approved);
    let calls = logger.log();
    assert!(!calls.is_empty());
    for call in calls {
        let argv = call["argv"].to_string();
        assert!(!argv.contains("dash-probe"), "{argv}");
    }
}

/// AD-113: a group whose hunks pass 1 MiB shows none and cannot be
/// approved; the package can only be discarded.
#[test]
fn an_oversized_group_cannot_be_approved() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let big: String = (0..80_000)
        .map(|i| format!("\"line-{i:08}\": 1,\n"))
        .collect();
    let run = a_package(&home, home.seeded(), &[("package.json", &big)]);
    let view = open(&home, &run.runtime, &run.id);
    let build = group(&view, SensitiveGroup::BuildScripts);
    assert!(!build.approvable && build.hunks.is_none());
    assert!(build.hunks_bytes.unwrap() > 1024 * 1024);
    let answer = approve(
        &home,
        &run.runtime,
        &run.id,
        &build.key,
        build.hunks_sha256.as_deref().unwrap(),
    );
    assert_eq!(answer, ApprovalAnswer::Refused(ApprovalRefusal::TooLarge));
    assert_eq!(view.readiness.reasons[0].cause, NotReadyCause::TooLarge);
    assert!(approvals(&run).is_empty());
}

// AC 3: approval.

/// AD-113, AD-24's pattern: the hash the panel showed is stored and
/// journaled, the next view shows the group approved, and collection's
/// `review_package` counted the group and named its family.
#[test]
fn an_approval_binds_to_the_hunks_displayed() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("build.rs", BUILD_RS)]);
    let package = run
        .lines()
        .into_iter()
        .find_map(|l| match l {
            JournalEntry::ReviewPackage(p) => Some(p),
            _ => None,
        })
        .unwrap();
    assert_eq!(package.groups, 1);
    assert_eq!(package.sensitive, [SensitiveGroup::BuildScripts]);
    let view = open(&home, &run.runtime, &run.id);
    let build = group(&view, SensitiveGroup::BuildScripts);
    assert!(!build.approved);
    let hash = build.hunks_sha256.clone().unwrap();
    let answer = approve(&home, &run.runtime, &run.id, &build.key, &hash);
    assert_eq!(answer, ApprovalAnswer::Approved);
    assert_eq!(approvals(&run), [(build.key.clone(), hash)]);
    // The next view reads the approval back from the registry.
    let again = open(&home, &run.runtime, &run.id);
    assert!(group(&again, SensitiveGroup::BuildScripts).approved);
    assert!(again.readiness.ready);
}

/// AD-113: a hash other than the hunks' own is refused `hunks_changed`
/// and stores nothing; a key the package lacks and a run not in review
/// are refused too.
#[test]
fn a_stale_hash_is_refused() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("build.rs", BUILD_RS)]);
    let view = open(&home, &run.runtime, &run.id);
    let key = group(&view, SensitiveGroup::BuildScripts).key.clone();
    let stale = "0".repeat(64);
    assert_eq!(
        approve(&home, &run.runtime, &run.id, &key, &stale),
        ApprovalAnswer::Refused(ApprovalRefusal::HunksChanged)
    );
    assert_eq!(
        approve(&home, &run.runtime, &run.id, "file:0", &stale),
        ApprovalAnswer::Refused(ApprovalRefusal::UnknownGroup)
    );
    assert_eq!(
        approve(&home, &run.runtime, &"f".repeat(32), &key, &stale),
        ApprovalAnswer::Refused(ApprovalRefusal::UnknownRun)
    );
    assert!(approvals(&run).is_empty());
    let again = open(&home, &run.runtime, &run.id);
    assert!(!group(&again, SensitiveGroup::BuildScripts).approved);
    let no_change = launch(&home, home.seeded());
    let clone = no_change.clone();
    no_change.serve(&no_change.packed(&clone, None), 0);
    no_change.exit(0);
    assert_eq!(no_change.collected().state, RunState::NoChange);
    assert_eq!(
        approve(&home, &run.runtime, &no_change.id, &key, &stale),
        ApprovalAnswer::Refused(ApprovalRefusal::NotInReview)
    );
}

/// AD-113: Import readiness is state `review` and every group approved.
#[test]
fn readiness_needs_every_group() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(
        &home,
        home.seeded(),
        &[
            ("build.rs", BUILD_RS),
            (".github/workflows/ci.yml", "on: push\n"),
        ],
    );
    let view = open(&home, &run.runtime, &run.id);
    assert_eq!(view.state, RunState::Review);
    assert_eq!(view.readiness.reasons.len(), 2);
    let build = group(&view, SensitiveGroup::BuildScripts);
    let ci = group(&view, SensitiveGroup::Ci);
    approve(
        &home,
        &run.runtime,
        &run.id,
        &build.key,
        build.hunks_sha256.as_deref().unwrap(),
    );
    let half = open(&home, &run.runtime, &run.id);
    assert!(!half.readiness.ready);
    assert_eq!(half.readiness.reasons.len(), 1);
    assert_eq!(half.readiness.reasons[0].cause, NotReadyCause::Unapproved);
    assert_eq!(
        half.readiness.reasons[0].group.as_deref(),
        Some(ci.key.as_str())
    );
    approve(
        &home,
        &run.runtime,
        &run.id,
        &ci.key,
        ci.hunks_sha256.as_deref().unwrap(),
    );
    assert!(open(&home, &run.runtime, &run.id).readiness.ready);
}

/// D-239: after a window restart the review cache is empty, and the
/// approval recomputes the group's hunks on thread `workshop`.
#[test]
fn an_approval_after_a_restart_recomputes_the_hunks() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("build.rs", BUILD_RS)]);
    let view = open(&home, &run.runtime, &run.id);
    let build = group(&view, SensitiveGroup::BuildScripts);
    let restarted = home.seeded();
    assert!(restarted.reviews.lock().unwrap().is_empty());
    let answer = approve(
        &home,
        &restarted,
        &run.id,
        &build.key,
        build.hunks_sha256.as_deref().unwrap(),
    );
    assert_eq!(answer, ApprovalAnswer::Approved);
    assert!(restarted.reviews.lock().unwrap().contains_key(&run.id));
}

/// FR-11: with `review = "all"` every changed file is its own group,
/// keyed by its index in the package's sorted list.
#[test]
fn review_all_makes_one_group_per_file() {
    let options = Options {
        project_workshop: "review = \"all\"\n",
        ..Options::default()
    };
    let Some(home) = home(&options, &[]) else {
        return;
    };
    let run = a_package(
        &home,
        home.seeded(),
        &[("src/b.rs", "b\n"), ("src/a.rs", "a\n")],
    );
    let view = open(&home, &run.runtime, &run.id);
    let keys: Vec<&str> = view.groups.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(keys, ["file:0", "file:1"]);
    assert!(view.groups.iter().all(|g| g.group == SensitiveGroup::File));
    let first = serde_json::to_string(&view.files[0].paths).unwrap();
    assert!(first.contains("src/a.rs"), "{first}");
    assert_eq!(view.files[0].groups, ["file:0"]);
}

// Story 6.16 AC 1: Import (AD-114).

const SOURCE: &str = "pub fn a() {}\n";

/// The branch Import creates for `run`, as a reference name.
fn branch_ref(run: &Run<'_>) -> String {
    format!("refs/heads/{}", run.row().branch)
}

/// The commit `reference` names in the project, if any.
fn named(home: &Home, reference: &str) -> Option<String> {
    references(home).into_iter().find_map(|line| {
        line.strip_prefix(&format!("{reference} "))
            .map(str::to_owned)
    })
}

/// AD-114: Import adds `refs/heads/officina/<name>` at the tip and
/// nothing else: every other reference, `HEAD`, the index and the work
/// tree stay; the run is `imported` and `review_imported` names the
/// branch and the tip.
#[test]
fn import_adds_exactly_one_reference() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("build.rs", BUILD_RS)]);
    run.approve_all();
    let refs = references(&home);
    let tree = work_tree(&home);
    let tip = run.row().tip_commit.unwrap();
    let branch = run.row().branch;
    assert_eq!(run.import(), ImportAnswer::Imported(branch.clone()));
    let mut expected = refs.clone();
    expected.push(format!("{} {tip}", branch_ref(&run)));
    let mut after = references(&home);
    expected.sort();
    after.sort();
    assert_eq!(after, expected);
    assert_eq!(work_tree(&home), tree);
    let row = run.row();
    assert_eq!(row.state, RunState::Imported);
    assert_eq!(row.verdict, Some(PackageVerdict::Imported));
    let imported: Vec<_> = run
        .lines()
        .into_iter()
        .filter_map(|l| match l {
            JournalEntry::ReviewImported(l) => Some((l.branch, l.tip)),
            _ => None,
        })
        .collect();
    assert_eq!(imported, [(branch, tip)]);
    // A second Import of a decided run is refused before any git.
    assert_eq!(
        run.import(),
        ImportAnswer::Refused(ImportRefusal::NotInReview)
    );
}

/// AD-114 step 2: an existing `officina/<name>` is refused and kept.
#[test]
fn import_refuses_an_existing_branch() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("src/a.rs", SOURCE)]);
    run.approve_all();
    home.git(&["branch", &run.row().branch]);
    let refs = references(&home);
    assert_eq!(
        run.import(),
        ImportAnswer::Refused(ImportRefusal::BranchExists)
    );
    assert_eq!(references(&home), refs);
    assert_eq!(run.row().state, RunState::Review);
}

/// D-248, review F47: a branch named `officina` makes `officina/<name>`
/// impossible, refused `branch_conflict` before any fetch.
#[test]
fn import_refuses_a_branch_named_officina_as_a_conflict() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("src/a.rs", SOURCE)]);
    run.approve_all();
    home.git(&["branch", "officina"]);
    let refs = references(&home);
    assert_eq!(
        run.import(),
        ImportAnswer::Refused(ImportRefusal::BranchConflict)
    );
    assert_eq!(references(&home), refs);
}

/// AD-114 step 1: a tip other than the recorded one is `state_changed`.
#[test]
fn import_refuses_a_stale_tip() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("src/a.rs", SOURCE)]);
    run.approve_all();
    let refs = references(&home);
    let stale = run.row().base_commit;
    assert_eq!(
        run.import_with(&run.runtime, &stale),
        ImportAnswer::Refused(ImportRefusal::StateChanged)
    );
    assert_eq!(references(&home), refs);
}

/// AD-113, AD-114 step 1: a group not approved keeps Import refused.
#[test]
fn import_refuses_an_unapproved_group() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("build.rs", BUILD_RS)]);
    let refs = references(&home);
    assert_eq!(
        run.import(),
        ImportAnswer::Refused(ImportRefusal::Unapproved)
    );
    assert_eq!(references(&home), refs);
    assert_eq!(run.row().state, RunState::Review);
}

/// AD-114: a quarantined package offers Discard only.
#[test]
fn import_refuses_a_quarantined_package() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[(".claude/settings.json", "{}\n")]);
    assert_eq!(run.row().state, RunState::Quarantined);
    let refs = references(&home);
    assert_eq!(
        run.import(),
        ImportAnswer::Refused(ImportRefusal::NotInReview)
    );
    assert_eq!(references(&home), refs);
    assert_eq!(
        run.discard(DiscardReason::PoorResult),
        DiscardAnswer::Discarded
    );
}

/// AD-114 step 5 (amendment 10): a branch created after the checks, right
/// before `update-ref`, is never moved: the zero old value makes the
/// update fail, and the run stays in review.
#[test]
fn a_branch_created_between_check_and_update_is_not_moved() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("src/a.rs", SOURCE)]);
    run.approve_all();
    let tip = run.row().tip_commit.unwrap();
    let base = run.row().base_commit;
    let planted = run.row().branch;
    let (git_dir, project) = (home.git_dir.clone(), home.project());
    let answer = officina_host::workshop::import::import_package_staged(
        home.sender(),
        &run.runtime,
        &home.env.officina_home(),
        &base_env(),
        &run.id,
        &tip,
        move || common::git(&git_dir, &project, &["branch", &planted]),
    )
    .unwrap();
    assert_eq!(answer, ImportAnswer::Refused(ImportRefusal::BranchExists));
    assert_eq!(named(&home, &branch_ref(&run)), Some(base));
    assert_eq!(run.row().state, RunState::Review);
    assert!(
        !run.lines()
            .iter()
            .any(|l| matches!(l, JournalEntry::ReviewImported(_)))
    );
}

// Story 6.16 AC 3: Discard (AD-114).

/// AD-114: Discard records the author's reason in the row and in
/// `review_discarded`; a decided run and an unknown one are refused.
#[test]
fn discard_records_the_reason() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("src/a.rs", SOURCE)]);
    assert_eq!(
        run.discard(DiscardReason::NotNeeded),
        DiscardAnswer::Discarded
    );
    let row = run.row();
    assert_eq!(row.state, RunState::Discarded);
    assert_eq!(row.verdict, Some(PackageVerdict::Discarded));
    assert_eq!(row.discard_reason, Some(DiscardReason::NotNeeded));
    let reasons: Vec<_> = run
        .lines()
        .into_iter()
        .filter_map(|l| match l {
            JournalEntry::ReviewDiscarded(l) => Some(l.reason),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, [DiscardReason::NotNeeded]);
    assert_eq!(
        run.discard(DiscardReason::PoorResult),
        DiscardAnswer::Refused(DiscardRefusal::NoPackage)
    );
    assert_eq!(
        officina_host::workshop::import::discard_package(
            home.sender(),
            &"f".repeat(32),
            DiscardReason::PoorResult
        )
        .unwrap(),
        DiscardAnswer::Refused(DiscardRefusal::UnknownRun)
    );
}

/// AD-114: `expired` and `not_collected` are the system's; the author's
/// command refuses them and changes nothing.
#[test]
fn the_author_cannot_send_a_system_reason() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = a_package(&home, home.seeded(), &[("src/a.rs", SOURCE)]);
    for reason in DiscardReason::ALL.iter().filter(|r| !r.is_authors()) {
        assert_eq!(
            run.discard(*reason),
            DiscardAnswer::Refused(DiscardRefusal::SystemReason),
            "{reason:?}"
        );
    }
    assert_eq!(run.row().state, RunState::Review);
    assert!(
        !run.lines()
            .iter()
            .any(|l| matches!(l, JournalEntry::ReviewDiscarded(_)))
    );
}

/// AD-110, AD-114: a `no_change` run has no verdict, neither Discard nor
/// Import.
#[test]
fn a_no_change_run_refuses_discard() {
    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.serve(&run.packed(&clone, None), 0);
    run.exit(0);
    assert_eq!(run.collected().state, RunState::NoChange);
    assert_eq!(
        run.discard(DiscardReason::PoorResult),
        DiscardAnswer::Refused(DiscardRefusal::NoPackage)
    );
    assert_eq!(
        run.import(),
        ImportAnswer::Refused(ImportRefusal::NotInReview)
    );
    assert_eq!(run.row().state, RunState::NoChange);
}
