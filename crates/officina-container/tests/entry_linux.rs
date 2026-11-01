//! `officina-entry` on Linux with real git in a temporary root and a fake
//! `claude` script (story 6.7 AC 1, 2, 5; AD-108, AD-110, D-606, D-608,
//! D-609). `/work` lives under the temporary root (`--root`).
#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use officina_core::workshop::launch::{MountLink, RUN_PLAN_SCHEMA, RunPlan};
use officina_core::workshop::scan::needles;

const ENTRY: &str = env!("CARGO_BIN_EXE_officina-entry");
const BRANCH: &str = "officina/bump-1";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "User One")
        .env("GIT_AUTHOR_EMAIL", "user01@example.com")
        .env("GIT_COMMITTER_NAME", "User One")
        .env("GIT_COMMITTER_EMAIL", "user01@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A temporary root holding `/work/.officina/{in,mounts,login,config}`, a
/// base bundle of a one-commit repository, and a fake `claude`.
struct Root {
    _dir: tempfile::TempDir,
    root: PathBuf,
    base: String,
    claude: PathBuf,
}

impl Root {
    fn new(claude_script: &str) -> Root {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "--quiet", "-b", "main"]);
        fs::write(source.join("README.md"), "base\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "--quiet", "-m", "base"]);
        let base = git(&source, &["rev-parse", "HEAD"]);
        let work = root.join("work/.officina");
        for sub in ["in", "mounts/0", "login", "config"] {
            fs::create_dir_all(work.join(sub)).unwrap();
        }
        git(
            &source,
            &[
                "bundle",
                "create",
                "--quiet",
                work.join("in/base.bundle").to_str().unwrap(),
                "main",
            ],
        );
        fs::write(work.join("in/prompt.txt"), "Bump serde.\n").unwrap();
        let claude = root.join("claude");
        fs::write(&claude, claude_script).unwrap();
        fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
        Root {
            _dir: dir,
            root,
            base,
            claude,
        }
    }

    fn work(&self) -> PathBuf {
        self.root.join("work")
    }

    fn plan(&self, login_files: &[&str], deadline_s: u64) -> RunPlan {
        RunPlan {
            schema: RUN_PLAN_SCHEMA,
            project: "api".into(),
            clone_path: "/work/api".into(),
            branch: BRANCH.into(),
            base_branch: "main".into(),
            base_commit: self.base.clone(),
            mount_links: vec![MountLink {
                index: 0,
                path: "_bmad/notes".into(),
            }],
            exclude: vec!["_bmad/notes".into()],
            claude_argv: vec!["-p".into(), "--output-format".into(), "stream-json".into()],
            deadline_s,
            prompt_file: "/work/.officina/in/prompt.txt".into(),
            login_files: login_files.iter().map(|f| (*f).to_owned()).collect(),
            memory: None,
        }
    }

    fn write_plan(&self, plan: &RunPlan) {
        fs::write(
            self.work().join(".officina/in/plan.json"),
            serde_json::to_string(plan).unwrap(),
        )
        .unwrap();
    }

    fn entry(&self, args: &[&str]) -> Output {
        Command::new(ENTRY)
            .args(args)
            .arg("--root")
            .arg(&self.root)
            .arg("--claude")
            .arg(&self.claude)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn clone(&self) -> PathBuf {
        self.work().join("api")
    }
}

const LOGGING_CLAUDE: &str = "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$PWD/../claude-argv\"\ncat > \"$PWD/../claude-stdin\"\nexit 0\n";

#[test]
fn clone_branch_and_head_equal_the_base() {
    let root = Root::new(LOGGING_CLAUDE);
    root.write_plan(&root.plan(&[], 60));
    let out = root.entry(&["run"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(git(&root.clone(), &["rev-parse", "HEAD"]), root.base);
    assert_eq!(git(&root.clone(), &["branch", "--show-current"]), BRANCH);
}

#[test]
fn mounts_are_excluded_and_linked() {
    let root = Root::new(LOGGING_CLAUDE);
    root.write_plan(&root.plan(&[], 60));
    assert_eq!(root.entry(&["run"]).status.code(), Some(0));
    let link = root.clone().join("_bmad/notes");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        root.work().join(".officina/mounts/0")
    );
    let exclude = fs::read_to_string(root.clone().join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|l| l == "/_bmad/notes"), "{exclude}");
}

#[test]
fn login_files_are_copied_with_mode_0600_and_never_printed() {
    let root = Root::new(LOGGING_CLAUDE);
    let secret = "stored-login-content-never-printed";
    fs::write(
        root.work().join(".officina/login/.credentials.json"),
        secret,
    )
    .unwrap();
    root.write_plan(&root.plan(&[".credentials.json"], 60));
    let out = root.entry(&["run"]);
    assert_eq!(out.status.code(), Some(0));
    let copied = root.work().join(".officina/config/.credentials.json");
    assert_eq!(fs::read_to_string(&copied).unwrap(), secret);
    assert_eq!(
        fs::metadata(&copied).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains(secret));
    assert!(!String::from_utf8_lossy(&out.stdout).contains(secret));
}

#[test]
fn a_missing_login_file_exits_non_zero() {
    let root = Root::new(LOGGING_CLAUDE);
    root.write_plan(&root.plan(&[".credentials.json"], 60));
    let out = root.entry(&["run"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("login_missing"));
}

#[test]
fn the_fake_claude_gets_the_plan_vector_and_the_prompt_on_stdin() {
    let root = Root::new(LOGGING_CLAUDE);
    let plan = root.plan(&[], 60);
    root.write_plan(&plan);
    assert_eq!(root.entry(&["run"]).status.code(), Some(0));
    let argv = fs::read_to_string(root.work().join("claude-argv")).unwrap();
    assert_eq!(argv.lines().collect::<Vec<_>>(), plan.claude_argv);
    assert_eq!(
        fs::read_to_string(root.work().join("claude-stdin")).unwrap(),
        "Bump serde.\n"
    );
}

#[test]
fn a_timeout_exits_124() {
    let root = Root::new("#!/bin/sh\nsleep 30\n");
    root.write_plan(&root.plan(&[], 1));
    assert_eq!(root.entry(&["run"]).status.code(), Some(124));
}

#[test]
fn the_exit_code_is_passed_through() {
    let root = Root::new("#!/bin/sh\nexit 7\n");
    root.write_plan(&root.plan(&[], 60));
    let out = root.entry(&["run"]);
    assert_eq!(out.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&out.stderr).contains("\"exit_code\":7"));
}

#[test]
fn an_unknown_plan_schema_exits_non_zero() {
    let root = Root::new(LOGGING_CLAUDE);
    let mut plan = root.plan(&[], 60);
    plan.schema = RUN_PLAN_SCHEMA + 1;
    root.write_plan(&plan);
    assert_ne!(root.entry(&["run"]).status.code(), Some(0));
}

#[test]
fn nothing_is_added_to_the_clone_but_links_and_excludes() {
    let root = Root::new(LOGGING_CLAUDE);
    root.write_plan(&root.plan(&[], 60));
    assert_eq!(root.entry(&["run"]).status.code(), Some(0));
    let status = git(&root.clone(), &["status", "--porcelain", "--ignored"]);
    // Only the ignored link and its parent folder: git names the folder
    // when everything in it is ignored.
    assert!(
        status
            .lines()
            .all(|l| l == "!! _bmad/" || l == "!! _bmad/notes"),
        "{status}"
    );
    assert!(!status.is_empty());
    for added in ["CLAUDE.md", ".claude", ".mcp.json"] {
        assert!(!root.clone().join(added).exists(), "{added}");
    }
}

// ---- pack ----

fn ran(root: &Root) {
    root.write_plan(&root.plan(&[], 60));
    assert_eq!(root.entry(&["run"]).status.code(), Some(0));
}

fn commit(root: &Root, name: &str) -> String {
    fs::write(root.clone().join(name), name).unwrap();
    git(&root.clone(), &["add", name]);
    git(&root.clone(), &["commit", "--quiet", "-m", name]);
    git(&root.clone(), &["rev-parse", "HEAD"])
}

fn pack(root: &Root) -> Output {
    root.entry(&[
        "pack",
        "--branch",
        BRANCH,
        "--base",
        &root.base,
        "--clone",
        "/work/api",
    ])
}

fn out(root: &Root) -> PathBuf {
    root.work().join(".officina/out")
}

#[test]
fn out_is_emptied_first() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    fs::create_dir_all(out(&root)).unwrap();
    fs::write(out(&root).join("planted"), "x").unwrap();
    assert_eq!(pack(&root).status.code(), Some(0));
    assert!(!out(&root).join("planted").exists());
}

#[test]
fn an_out_symlink_is_replaced() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    let elsewhere = root.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("keep"), "x").unwrap();
    std::os::unix::fs::symlink(&elsewhere, out(&root)).unwrap();
    assert_eq!(pack(&root).status.code(), Some(0));
    assert!(fs::symlink_metadata(out(&root)).unwrap().is_dir());
    assert!(elsewhere.join("keep").exists(), "the link was not followed");
}

#[test]
fn a_forged_plan_does_not_change_what_is_packed() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    let tip = commit(&root, "change.txt");
    let mut forged = root.plan(&[], 60);
    forged.branch = "main".into();
    root.write_plan(&forged);
    assert_eq!(pack(&root).status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(out(&root).join("tip")).unwrap().trim(),
        tip
    );
}

#[test]
fn a_commit_past_the_base_gives_tip_and_bundle() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    let tip = commit(&root, "change.txt");
    assert_eq!(pack(&root).status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(out(&root).join("tip")).unwrap().trim(),
        tip
    );
    assert!(out(&root).join("work.bundle").is_file());
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out(&root).join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["too_large"], false);
}

#[test]
fn no_commit_gives_tip_and_no_bundle() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    assert_eq!(pack(&root).status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(out(&root).join("tip")).unwrap().trim(),
        root.base
    );
    assert!(!out(&root).join("work.bundle").exists());
}

#[test]
fn a_branch_reset_behind_the_base_gives_its_tip_and_no_bundle() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    commit(&root, "change.txt");
    git(&root.clone(), &["reset", "--quiet", "--hard", &root.base]);
    assert_eq!(pack(&root).status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(out(&root).join("tip")).unwrap().trim(),
        root.base
    );
    assert!(!out(&root).join("work.bundle").exists());
}

#[test]
fn a_deleted_branch_gives_no_tip() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    git(&root.clone(), &["checkout", "--quiet", "--detach"]);
    git(&root.clone(), &["branch", "--quiet", "-D", BRANCH]);
    assert_eq!(pack(&root).status.code(), Some(0));
    assert!(!out(&root).join("tip").exists());
}

#[test]
fn an_oversized_bundle_is_deleted_and_reported() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    // Incompressible content past the cap.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let big: Vec<u8> = (0..65 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xff) as u8
        })
        .collect();
    fs::write(root.clone().join("big.bin"), big).unwrap();
    git(&root.clone(), &["add", "big.bin"]);
    git(&root.clone(), &["commit", "--quiet", "-m", "big"]);
    assert_eq!(pack(&root).status.code(), Some(0));
    assert!(!out(&root).join("work.bundle").exists());
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out(&root).join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["too_large"], true);
}

#[test]
fn a_planted_hook_in_the_clone_never_runs() {
    let root = Root::new(LOGGING_CLAUDE);
    ran(&root);
    let marker = root.root.join("hook-ran");
    let hooks = root.clone().join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    for hook in [
        "pre-push",
        "reference-transaction",
        "post-checkout",
        "pre-auto-gc",
    ] {
        let path = hooks.join(hook);
        fs::write(&path, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    commit(&root, "change.txt");
    let _ = fs::remove_file(&marker);
    assert_eq!(pack(&root).status.code(), Some(0));
    assert!(!marker.exists());
}

// ---- auth-status and scan ----

const STATUS_CLAUDE: &str = "#!/bin/sh\nif [ -f \"$CLAUDE_CONFIG_DIR/.credentials.json\" ]; then\n  echo '{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"email\":\"user01@example.com\",\"orgName\":\"Example Org\"}'\n  exit 0\nfi\necho '{\"loggedIn\":false}'\nexit 1\n";

#[test]
fn auth_status_prints_one_line_without_email_or_organisation() {
    let root = Root::new(STATUS_CLAUDE);
    fs::write(root.work().join(".officina/login/.credentials.json"), "{}").unwrap();
    let out = root.entry(&["auth-status"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.lines().count(), 1, "{text}");
    let line: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(
        line,
        serde_json::json!({"logged_in": true, "auth_method": "claude.ai"})
    );
    assert!(!text.contains("example.com") && !text.contains("Example Org"));
}

#[test]
fn auth_status_reports_a_missing_login() {
    let root = Root::new(STATUS_CLAUDE);
    let out = root.entry(&["auth-status"]);
    let line: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(
        line,
        serde_json::json!({"logged_in": false, "auth_method": null})
    );
}

/// A seeded token, derived here, never typed in an encoded form.
fn seeded() -> String {
    (0u8..44).map(|i| char::from(b'a' + (i * 5) % 26)).collect()
}

fn scan(root: &Root, records: &[Vec<u8>]) -> Vec<serde_json::Value> {
    let mut child = Command::new(ENTRY)
        .args(["scan", "--root"])
        .arg(&root.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for (i, record) in records.iter().enumerate() {
            writeln!(stdin, "{i} {}", record.len()).unwrap();
            stdin.write_all(record).unwrap();
        }
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn store_login(root: &Root, token: &str) {
    let json = serde_json::json!({"claudeAiOauth": {"accessToken": token, "email": format!("user01@example.com{}", "x".repeat(40)), "scopes": ["user:inference"]}});
    fs::write(
        root.work().join(".officina/login/.credentials.json"),
        json.to_string(),
    )
    .unwrap();
}

#[test]
fn scan_reports_ranges_never_values() {
    let root = Root::new(LOGGING_CLAUDE);
    let token = seeded();
    store_login(&root, &token);
    let lines = scan(
        &root,
        &[
            format!("before {token} after").into_bytes(),
            b"clean".to_vec(),
        ],
    );
    assert_eq!(
        lines,
        [serde_json::json!({"record": 0, "ranges": [[7, 7 + token.len()]]})]
    );
    assert!(!serde_json::to_string(&lines).unwrap().contains(&token));
}

#[test]
fn scan_finds_every_form_of_a_seeded_value() {
    let root = Root::new(LOGGING_CLAUDE);
    let token = seeded();
    store_login(&root, &token);
    let hex: String = token.bytes().map(|b| format!("{b:02x}")).collect();
    let records = vec![
        token.clone().into_bytes(),
        hex.clone().into_bytes(),
        hex.to_uppercase().into_bytes(),
    ];
    let lines = scan(&root, &records);
    assert_eq!(lines.len(), records.len(), "{lines:?}");
    let _ = needles(token.as_bytes());
}

#[test]
fn scan_never_takes_an_email_or_a_short_value_as_a_needle() {
    let root = Root::new(LOGGING_CLAUDE);
    store_login(&root, "short");
    let lines = scan(&root, &[b"user01@example.com".to_vec()]);
    assert_eq!(lines, [serde_json::json!({"needles": 0})]);
}

#[test]
fn scan_with_no_login_reports_zero_needles() {
    let root = Root::new(LOGGING_CLAUDE);
    assert_eq!(scan(&root, &[]), [serde_json::json!({"needles": 0})]);
}

#[test]
fn an_oversized_record_is_refused() {
    let root = Root::new(LOGGING_CLAUDE);
    store_login(&root, &seeded());
    let mut child = Command::new(ENTRY)
        .args(["scan", "--root"])
        .arg(&root.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.as_mut().unwrap(),
        "0 {}",
        2 * 1024 * 1024 * 1024u64
    )
    .unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("too_large"));
}
