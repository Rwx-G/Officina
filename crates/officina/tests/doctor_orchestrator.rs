//! `officina doctor`'s Orchestrator section end to end (story 5.19 AC 1,
//! AD-94, FR-15 as amended): the checks of Epic 5's stories print once
//! each, together, in [`ORCHESTRATOR_SECTION`]'s order, as doctor's last
//! section; its warnings alone exit 0; a `claude` that is not an `.exe`
//! and a non-empty orchestrator workdir exit 1. The relay's lines in the
//! section (templates, `auto`) need a floor whose programs resolve, a
//! Windows path (AD-22, as `doctor_relay.rs`): off Windows they are left out.

mod common;

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use common::{TestEnv, fake_claude_in, printed_hooks_block};
use officina_core::meeting::RESUME_PROMPT_MIN_VERSION;
use officina_core::orders::{ORCHESTRATOR_DIR, ORCHESTRATOR_WORKDIR, PROPOSALS_DIR};
use officina_core::spawn::AGENT_VIEW_VAR;
use officina_host::doctor::ORCHESTRATOR_SECTION;

const STATUSLINE: &str = r#"{"session_id":"s1","model":{"display_name":"Opus"},
  "rate_limits":{"five_hour":{"used_percentage":62.0,"resets_at":1800000000},
                 "seven_day":{"used_percentage":41.0,"resets_at":1800500000}}}"#;

/// The section's titles that come from the relay floor.
const RELAY_TITLES: [&str; 4] = [
    "template issue",
    "templates",
    "relay auto actions",
    "relay auto review",
];

/// The titles that print one line per finding, each in the order found.
const REPEATED: [&str; 2] = ["template issue", "orchestrator folder"];

/// The titles doctor prints only under `--permissions`, and the one it
/// prints only without it.
const PERMISSIONS_ONLY: [&str; 2] = ["permission mode switch", "autoMode block"];

/// A free project, its check and an `auto` action naming it, a failing
/// `[auto_review]` and one template; `{lab}` and `{tool}` filled in.
const FLOOR: &str = "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{lab}\"\nautonomy = \"free\"\nactions = [\"check\", \"apply\"]\n[[action]]\nname = \"check\"\nclass = \"free\"\nprogram = \"{tool}\"\n[[action]]\nname = \"apply\"\nclass = \"auto\"\ncheck = \"check\"\nprogram = \"{tool}\"\n[auto_review]\nmodel = \"gpt\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"lab\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";

/// Added after the approval: a template awaiting it, and one with an issue.
const LATER_TEMPLATES: &str = "[[template]]\nname = \"notes\"\ndescription = \"Write the notes\"\nprojects = [\"lab\"]\nprompt = \"Write the notes.\"\n[[template]]\nname = \"broken\"\ndescription = \"Starts with a command\"\nprojects = [\"lab\"]\nprompt = \"/clear\"\n";

struct Home {
    env: TestEnv,
    claude: PathBuf,
}

impl Home {
    /// A home whose checks outside the Orchestrator section pass: the
    /// hooks merged, a hook line seen (carrying a permission mode) and the
    /// quota written. `claude` is a copy of the fake answering `version`.
    fn wired(version: &str, agents: &serde_json::Value) -> Home {
        let env = TestEnv::new();
        env.write_agents(&agents.to_string());
        let claude = fake_claude_in(
            &env.root.path().join("bin"),
            &serde_json::json!({"fixture": env.fixture(), "version": version}),
        );
        let home = Home { env, claude };
        let block = printed_hooks_block(&home.doctor(&[], &[]).stdout);
        std::fs::write(
            home.env.claude_config().join("settings.json"),
            block.to_string(),
        )
        .unwrap();
        home.env.run(
            &["hook"],
            br#"{"hook_event_name":"SessionStart","session_id":"s","permission_mode":"default"}"#,
        );
        home.env.run(&["quota", "write"], STATUSLINE.as_bytes());
        home
    }

    fn doctor(&self, args: &[&str], vars: &[(&str, &str)]) -> std::process::Output {
        let mut full = vec!["doctor"];
        full.extend_from_slice(args);
        let mut cmd = self.env.officina(&full);
        cmd.env("OFFICINA_CLAUDE_BIN", &self.claude);
        for (name, value) in vars {
            cmd.env(name, value);
        }
        common::run_with(cmd, b"")
    }

    fn orchestrator_dir(&self) -> PathBuf {
        self.env.officina_home().join(ORCHESTRATOR_DIR)
    }
}

/// Every orchestrator line that can warn, warning at once: agent view
/// turned off (the doctor's environment, see [`warned_vars`]), a `claude`
/// below the answers' version, a stranger under the orchestrator's folder,
/// a stale temporary proposal file and, on Windows, a template issue,
/// templates awaiting approval and a failing `[auto_review]`.
fn warned_home() -> Home {
    let [major, minor, patch] = RESUME_PROMPT_MIN_VERSION;
    assert!(patch > 0, "a version below {major}.{minor}.{patch}");
    let home = Home::wired(&format!("{major}.{minor}.0"), &serde_json::json!([]));
    let stranger = home.orchestrator_dir().join("elsewhere");
    home.env.write_agents(
        &serde_json::json!([{
            "pid": 7, "cwd": stranger, "kind": "interactive", "startedAt": 1,
            "sessionId": "x", "status": "idle", "name": "elsewhere"
        }])
        .to_string(),
    );
    let proposals = home.orchestrator_dir().join(PROPOSALS_DIR);
    std::fs::create_dir_all(&proposals).unwrap();
    let stale = proposals.join("p1.tmp");
    std::fs::write(&stale, b"{").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(2 * 24 * 3600))
        .unwrap();
    if cfg!(windows) {
        let lab = home.env.root.path().join("work").join("Lab");
        std::fs::create_dir_all(&lab).unwrap();
        let floor = common::relay::fill(FLOOR, &[("lab", lab)]);
        home.env.write_floor(&floor);
        home.env.approve();
        home.env.write_floor(&format!("{floor}{LATER_TEMPLATES}"));
    }
    home
}

fn warned_vars() -> [(&'static str, &'static str); 1] {
    [(AGENT_VIEW_VAR, "1")]
}

/// The `[tag] title` of each line doctor printed, in order; the text to
/// paste under a line is not one.
fn lines(stdout: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?;
            let (tag, rest) = rest.split_once("] ")?;
            let (title, _) = rest.split_once(": ")?;
            Some((tag.trim().to_owned(), title.to_owned()))
        })
        .collect()
}

fn place(title: &str) -> Option<usize> {
    ORCHESTRATOR_SECTION.iter().position(|t| *t == title)
}

/// The section's lines are doctor's last, together, in the section's order.
fn assert_section(lines: &[(String, String)], text: &str) {
    let first = lines
        .iter()
        .position(|(_, title)| place(title).is_some())
        .unwrap_or_else(|| panic!("no Orchestrator line: {text}"));
    let places: Vec<usize> = lines[first..]
        .iter()
        .map(|(_, title)| {
            place(title).unwrap_or_else(|| panic!("{title} inside the section: {text}"))
        })
        .collect();
    assert!(places.is_sorted(), "out of order: {text}");
}

fn count(lines: &[(String, String)], title: &str) -> usize {
    lines.iter().filter(|(_, t)| t == title).count()
}

#[test]
fn the_orchestrator_section_lists_each_check_once() {
    let home = warned_home();
    for permissions in [false, true] {
        let args: &[&str] = if permissions { &["--permissions"] } else { &[] };
        let out = home.doctor(args, &warned_vars());
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let lines = lines(&out.stdout);
        assert_section(&lines, &text);
        for title in ORCHESTRATOR_SECTION {
            let expected = if RELAY_TITLES.contains(title) && !cfg!(windows) {
                0
            } else if PERMISSIONS_ONLY.contains(title) {
                usize::from(permissions)
            } else {
                1
            };
            let found = count(&lines, title);
            if REPEATED.contains(title) && expected == 1 {
                assert!(found >= 1, "{title}: {text}");
            } else {
                assert_eq!(
                    found, expected,
                    "{title}, --permissions {permissions}: {text}"
                );
            }
        }
    }
}

#[test]
fn warnings_alone_exit_0() {
    let home = warned_home();
    let out = home.doctor(&[], &warned_vars());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines = lines(&out.stdout);
    let mut warned = vec![
        "agent view",
        "answers",
        "orchestrator folder",
        "orchestrator proposals",
    ];
    if cfg!(windows) {
        warned.extend(["template issue", "templates", "relay auto review"]);
    }
    for title in warned {
        assert!(
            lines.iter().any(|(tag, t)| t == title && tag == "warn"),
            "{title} warns: {text}"
        );
    }
    for title in [
        "permission mode",
        "permissions block",
        "orchestrator workdir",
        "officina.exe",
    ] {
        assert!(
            lines.iter().any(|(tag, t)| t == title && tag == "ok"),
            "{title} informs: {text}"
        );
    }
    assert!(lines.iter().all(|(tag, _)| tag != "FAIL"), "{text}");
    assert_eq!(out.status.code(), Some(0), "{text}");
}

#[test]
fn a_shim_exits_1() {
    let home = Home::wired("9.9.9", &serde_json::json!([]));
    let shim = home.env.root.path().join("bin").join("claude.cmd");
    std::fs::write(&shim, "@echo off\r\n").unwrap();
    let mut cmd = home.env.officina(&["doctor"]);
    cmd.env("OFFICINA_CLAUDE_BIN", &shim);
    let out = common::run_with(cmd, b"");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let claude = text
        .lines()
        .find(|line| line.starts_with("[FAIL] claude:"))
        .unwrap_or_else(|| panic!("no failing claude line: {text}"));
    for refused in ["launches", "answers", "reviews", "the orchestrator"] {
        assert!(claude.contains(refused), "{claude}");
    }
    let failed: Vec<String> = lines(&out.stdout)
        .into_iter()
        .filter(|(tag, _)| tag == "FAIL")
        .map(|(_, title)| title)
        .collect();
    assert_eq!(failed, ["claude"], "{text}");
    assert_eq!(out.status.code(), Some(1), "{text}");
}

#[test]
fn a_non_empty_workdir_exits_1() {
    let home = Home::wired("9.9.9", &serde_json::json!([]));
    let clean = home.doctor(&[], &[]);
    assert_eq!(
        clean.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&clean.stdout)
    );
    let workdir = home.orchestrator_dir().join(ORCHESTRATOR_WORKDIR);
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::write(workdir.join("left.txt"), b"x").unwrap();
    let out = home.doctor(&[], &[]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let failed: Vec<String> = lines(&out.stdout)
        .into_iter()
        .filter(|(tag, _)| tag == "FAIL")
        .map(|(_, title)| title)
        .collect();
    assert_eq!(failed, ["orchestrator workdir"], "{text}");
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(workdir.join("left.txt").exists(), "doctor deletes nothing");
}
