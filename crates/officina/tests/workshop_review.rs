//! Sensitive groups and their approvals end to end (story 6.15; AD-113,
//! D-219, D-239): packages built by real git in a clone of the project
//! (D-23), collected by the window on the fake engine, then reviewed and
//! approved through the functions the window's commands call.

mod common;

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::review::{
    ApprovalAnswer, ApprovalRefusal, NotReadyCause, ReviewGroup, ReviewView,
};
use officina_core::workshop::run::RunState;
use officina_core::workshop::sensitive::{ChangedFile, SensitiveGroup, hunks_sha256};
use officina_host::workshop::launch::WorkshopRuntime;

use common::package_run::{Run, base_env, git_text, home, launch};
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
