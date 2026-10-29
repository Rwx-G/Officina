//! `officina doctor`'s relay checks end to end (story 2.10, FR-14, D-30 to
//! D-33): the catalogue, its approval, the projects, program resolution, the
//! warnings, the levels and the `CLAUDE.md` paragraph. Windows only, like
//! `run.rs`: a `native` program is a Windows absolute path (AD-22), and the
//! resolution check needs the fake tool to be one; Linux is v1.

#![cfg(windows)]

mod common;

use std::path::PathBuf;
use std::process::Output;

use common::relay::fill;
use common::{TestEnv, fingerprint, git, git_dir_or_skip, now_ms, printed_hooks_block};
use officina_core::floor::Autonomy;
use officina_core::relay::catalogue::CanonicalCatalogue;
use officina_core::relay::classes::WAIT_RANGE_S;
use officina_core::relay::gate::{SHELL_TOOLS, ToolRule};
use officina_core::relay::journal::{GateError, GateFault, JournalEntry};
use officina_core::relay::run::BASH_TOOL_MAX_TIMEOUT_MS;
use officina_host::relay::journal::append;

const AGENTS: &str = r#"[{"pid":1,"cwd":"C:/x","kind":"interactive","startedAt":1,"sessionId":"s","status":"busy"}]"#;

/// A guarded project and two free ones, one serving an action; `{lab}`,
/// `{notes}`, `{quiet}` and `{tool}` are filled in by [`Doctor::write`].
const FLOOR: &str = r#"schema = 2

[[project]]
name = "lab"
path = "{lab}"
actions = ["probe"]

[[project]]
name = "notes"
path = "{notes}"
autonomy = "free"
actions = ["probe"]

[[project]]
name = "quiet"
path = "{quiet}"
autonomy = "free"

[[action]]
name = "probe"
description = "Reads one host"
class = "free"
program = "{tool}"
args = ["--host", "{host}"]

[action.params.host]
values = ["web01", "db01"]
"#;

/// A check and its `after-check` action, appended to [`FLOOR`]; the lab
/// project lists them through [`with_checked_lab`].
const CHECKED: &str = r#"
[[action]]
name = "verify"
class = "free"
program = "{tool}"
args = ["--check", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
class = "after-check"
check = "verify"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"
"#;

fn with_checked_lab() -> String {
    format!("{FLOOR}{CHECKED}").replacen(
        "actions = [\"probe\"]",
        "actions = [\"probe\", \"verify\", \"apply\"]",
        1,
    )
}

struct Doctor {
    env: TestEnv,
}

impl Doctor {
    fn new() -> Doctor {
        let doctor = Doctor {
            env: TestEnv::new(),
        };
        doctor.env.write_agents(AGENTS);
        for dir in [doctor.lab(), doctor.notes(), doctor.quiet()] {
            std::fs::create_dir_all(dir).unwrap();
        }
        doctor
    }

    fn work(&self) -> PathBuf {
        self.env.root.path().join("work")
    }

    fn lab(&self) -> PathBuf {
        self.work().join("Lab")
    }

    fn notes(&self) -> PathBuf {
        self.work().join("Notes")
    }

    fn quiet(&self) -> PathBuf {
        self.work().join("Quiet")
    }

    /// Writes the relay floor (the home's `floor.toml`), folders filled in.
    fn write(&self, floor: &str) {
        self.env.write_floor(&fill(
            floor,
            &[
                ("lab", self.lab()),
                ("notes", self.notes()),
                ("quiet", self.quiet()),
            ],
        ));
    }

    fn output(&self, args: &[&str]) -> Output {
        let mut full = vec!["doctor"];
        full.extend_from_slice(args);
        self.env.run(&full, b"")
    }

    fn report(&self, args: &[&str]) -> Report {
        Report::parse(&self.output(args))
    }

    /// Merges the hooks block doctor prints into the user settings, as the
    /// author pastes it.
    fn register_hooks(&self) {
        let block = printed_hooks_block(&self.output(&[]).stdout);
        std::fs::write(
            self.env.claude_config().join("settings.json"),
            block.to_string(),
        )
        .unwrap();
    }

    fn snapshot(&self) -> CanonicalCatalogue {
        self.env
            .relay_home()
            .read_approved()
            .approved()
            .unwrap()
            .canonical
            .clone()
    }
}

/// One `[tag] title: detail` line of doctor's output (tag `ok`, `warn` or
/// `FAIL`), with the text printed under it to paste.
#[derive(Debug)]
struct Line {
    tag: String,
    title: String,
    detail: String,
    paste: String,
}

#[derive(Debug)]
struct Report {
    code: Option<i32>,
    text: String,
    lines: Vec<Line>,
}

impl Report {
    fn parse(out: &Output) -> Report {
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut lines: Vec<Line> = Vec::new();
        for raw in text.lines() {
            let tagged = ["[ok  ] ", "[warn] ", "[FAIL] "]
                .iter()
                .find(|tag| raw.starts_with(*tag));
            match (tagged, lines.last_mut()) {
                (Some(tag), _) => {
                    let rest = &raw[tag.len()..];
                    let (title, detail) = rest.split_once(": ").unwrap_or((rest, ""));
                    lines.push(Line {
                        tag: tag.trim().trim_matches(['[', ']']).trim().to_owned(),
                        title: title.to_owned(),
                        detail: detail.to_owned(),
                        paste: String::new(),
                    });
                }
                (None, Some(last)) => {
                    last.paste.push_str(raw);
                    last.paste.push('\n');
                }
                (None, None) => {}
            }
        }
        Report {
            code: out.status.code(),
            text,
            lines,
        }
    }

    fn all(&self, title: &str) -> Vec<&Line> {
        self.lines.iter().filter(|l| l.title == title).collect()
    }

    fn line(&self, title: &str) -> &Line {
        let found = self.all(title);
        assert_eq!(found.len(), 1, "{title}: {}", self.text);
        found[0]
    }

    fn has(&self, title: &str) -> bool {
        !self.all(title).is_empty()
    }

    fn failures(&self) -> Vec<&Line> {
        self.lines.iter().filter(|l| l.tag == "FAIL").collect()
    }
}

// ---- AC 1: catalogue, approval, projects ----

#[test]
fn issues_are_reported_with_lines() {
    let doctor = Doctor::new();
    let broken = "\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative.exe\"\n";
    doctor.write(&format!("{FLOOR}{broken}"));
    let header_line = FLOOR.lines().count() + 2;
    let report = doctor.report(&[]);
    let issues = report.all("relay issue");
    assert_eq!(issues.len(), 1, "{}", report.text);
    assert_eq!(issues[0].tag, "warn");
    assert!(
        issues[0]
            .detail
            .starts_with(&format!("line {header_line}, action broken: ")),
        "{issues:?}"
    );
    assert!(issues[0].detail.ends_with("(program_not_absolute)"));
    let catalogue = report.line("relay catalogue");
    assert_eq!(catalogue.tag, "ok");
    assert!(
        catalogue.detail.starts_with("1 valid actions"),
        "{catalogue:?}"
    );
}

#[test]
fn pending_change_is_reported() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    let unapproved = doctor.report(&[]);
    let line = unapproved.line("relay approval");
    assert_eq!(line.tag, "warn");
    assert!(line.detail.starts_with("nothing approved yet"), "{line:?}");

    doctor.env.approve();
    let approved = doctor.report(&[]);
    let line = approved.line("relay approval");
    assert_eq!(line.tag, "ok", "{line:?}");
    assert!(line.detail.contains("approved"), "{line:?}");

    doctor.write(&FLOOR.replace("\"web01\", \"db01\"", "\"web01\""));
    let pending = doctor.report(&[]);
    let line = pending.line("relay approval");
    assert_eq!(line.tag, "warn");
    assert!(
        line.detail.contains("awaits approval in the window"),
        "{line:?}"
    );
    assert!(
        line.detail.contains("actions changed: probe (params)"),
        "{line:?}"
    );
}

#[test]
fn guarded_projects_are_listed() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    let report = doctor.report(&[]);
    let lab = report.line("relay project lab");
    assert_eq!(lab.tag, "ok");
    assert!(
        lab.detail.starts_with("guarded, actions probe ("),
        "{lab:?}"
    );
    assert!(
        lab.detail
            .contains(&doctor.lab().to_string_lossy().replace('\\', "/")),
        "{lab:?}"
    );
    let notes = report.line("relay project notes");
    assert!(
        notes.detail.starts_with("free, actions probe ("),
        "{notes:?}"
    );
    assert!(
        !report.has("relay project quiet"),
        "a free project without actions is v0.1's business"
    );

    let empty = Doctor::new();
    empty.write(&FLOOR.replacen("actions = [\"probe\"]\n", "", 1));
    let line = empty.report(&[]).line("relay project lab").detail.clone();
    assert!(line.starts_with("guarded, no action"), "{line}");
}

#[test]
fn a_workstation_without_the_relay_gets_no_relay_line() {
    let doctor = Doctor::new();
    let report = doctor.report(&[]);
    assert!(
        !report.lines.iter().any(|l| l.title.starts_with("relay")
            || l.title.starts_with("CLAUDE.md")
            || l.title == "floor mismatch"),
        "{}",
        report.text
    );
    doctor
        .env
        .write_floor("schema = 1\n[[project]]\nname = \"api\"\npath = \"Api\"\n");
    let v0_1 = doctor.report(&[]);
    assert!(
        !v0_1.lines.iter().any(|l| l.title.starts_with("relay")),
        "{}",
        v0_1.text
    );
}

// ---- AC 2: program resolution ----

#[test]
fn missing_native_program_fails() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    doctor.env.approve();
    doctor.register_hooks();
    let fine = doctor.report(&[]);
    assert_eq!(fine.code, Some(0), "{}", fine.text);
    assert_eq!(fine.line("relay program probe").tag, "ok");

    let missing = doctor
        .env
        .root
        .path()
        .join("tools")
        .join("missing-tool.exe")
        .to_string_lossy()
        .replace('\\', "/");
    doctor.write(&FLOOR.replace("{tool}", &missing));
    let report = doctor.report(&[]);
    assert_eq!(report.code, Some(1), "{}", report.text);
    let line = report.line("relay program probe");
    assert_eq!(line.tag, "FAIL");
    assert!(line.detail.contains("is not a file"), "{line:?}");
    assert_eq!(report.failures().len(), 1, "{}", report.text);
}

// ---- AC 3: warnings ----

#[test]
fn long_timeout_warns() {
    let doctor = Doctor::new();
    let cap_s = BASH_TOOL_MAX_TIMEOUT_MS / 1000;
    let floor = format!(
        "{}\n[[action]]\nname = \"slow\"\nclass = \"free\"\nprogram = \"{{tool}}\"\ntimeout_s = {cap_s}\n\n[[action]]\nname = \"approve-me\"\nclass = \"human\"\nprogram = \"{{tool}}\"\n",
        FLOOR.replacen(
            "actions = [\"probe\"]",
            "actions = [\"probe\", \"slow\", \"approve-me\"]",
            1
        )
    );
    doctor.write(&floor);
    let report = doctor.report(&[]);
    assert!(!report.has("relay timeout probe"), "{}", report.text);
    let slow = report.line("relay timeout slow");
    assert_eq!(slow.tag, "warn");
    assert!(
        slow.detail.contains(&format!(
            "timeout_s {cap_s} reaches the Bash tool's {cap_s} s cap"
        )),
        "{slow:?}"
    );
    let human = report.line("relay timeout approve-me");
    assert_eq!(human.tag, "warn");
    assert!(
        human
            .detail
            .contains(&format!("--wait of up to {} s", WAIT_RANGE_S.end())),
        "{human:?}"
    );
}

#[test]
fn after_check_without_tree_binding_warns() {
    let doctor = Doctor::new();
    doctor.write(&with_checked_lab());
    let report = doctor.report(&[]);
    let tree = report.line("relay tree lab");
    assert_eq!(tree.tag, "warn");
    assert!(
        tree.detail.contains("bind their parameters only"),
        "{tree:?}"
    );
    assert!(
        !report.has("relay tree notes"),
        "notes has no after-check action"
    );
}

/// Story 2.6's known limit: an `after-check` action runs only after its
/// check ran in the same project, so a project listing it without its check
/// can never run it.
#[test]
fn after_check_without_its_check_in_the_project_warns() {
    let doctor = Doctor::new();
    let floor = with_checked_lab().replacen(
        "autonomy = \"free\"\nactions = [\"probe\"]",
        "autonomy = \"free\"\nactions = [\"probe\", \"apply\"]",
        1,
    );
    doctor.write(&floor);
    let report = doctor.report(&[]);
    let notes = report.line("relay check notes");
    assert_eq!(notes.tag, "warn");
    assert!(
        notes
            .detail
            .contains("lists apply but not its check verify"),
        "{notes:?}"
    );
    assert!(!report.has("relay check lab"), "lab lists verify");
}

/// Story 2.8's adversarial pass: a `human` action's dry run (D-170) obeys
/// the same rule as an `after-check` action's check, and binds the tree the
/// same way.
#[test]
fn a_human_dry_run_its_project_does_not_list_warns() {
    let doctor = Doctor::new();
    let human = format!(
        "{FLOOR}{CHECKED}\n[[action]]\nname = \"restart\"\nclass = \"human\"\ncheck = \"verify\"\nprogram = \"{{tool}}\"\nargs = [\"{{group}}\"]\n\n[action.params.group]\npattern = \"lab-[a-z0-9-]{{1,30}}\"\n"
    )
    .replacen(
        "autonomy = \"free\"\nactions = [\"probe\"]",
        "autonomy = \"free\"\nactions = [\"probe\", \"restart\"]",
        1,
    );
    doctor.write(&human);
    let report = doctor.report(&[]);
    let notes = report.line("relay check notes");
    assert!(
        notes
            .detail
            .contains("lists restart but not its check verify"),
        "{notes:?}"
    );
    assert_eq!(report.line("relay tree notes").tag, "warn");
}

#[test]
fn relay_floor_mismatch_warns() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    doctor.env.approve();
    let other = doctor.env.root.path().join("other.toml");
    std::fs::copy(doctor.env.floor_file(), &other).unwrap();
    let other_text = other.to_string_lossy().into_owned();

    let same = doctor.report(&[]);
    assert!(!same.has("floor mismatch"), "{}", same.text);
    let report = doctor.report(&["--floor", &other_text]);
    let line = report.line("floor mismatch");
    assert_eq!(line.tag, "warn");
    assert!(line.detail.contains("other.toml"), "{line:?}");
    assert!(line.detail.contains("floor.toml"), "{line:?}");

    // `OFFICINA_FLOOR` moves both doctor's floor and the relay's.
    let mut cmd = doctor.env.officina(&["doctor"]);
    cmd.env("OFFICINA_FLOOR", &other);
    let both = Report::parse(&common::run_with(cmd, b""));
    assert!(!both.has("floor mismatch"), "{}", both.text);
}

/// Adversarial pass (story 2.2's open item): a relative `OFFICINA_FLOOR`
/// resolves against each process's own folder.
#[test]
fn a_relative_floor_override_warns() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    let mut cmd = doctor.env.officina(&["doctor"]);
    cmd.env("OFFICINA_FLOOR", "floor.toml")
        .current_dir(doctor.env.officina_home());
    let report = Report::parse(&common::run_with(cmd, b""));
    let line = report.line("relay floor override");
    assert_eq!(line.tag, "warn");
    assert!(line.detail.contains("set an absolute path"), "{line:?}");

    let absolute = doctor.report(&[]);
    assert!(!absolute.has("relay floor override"), "{}", absolute.text);
}

#[test]
fn gate_errors_are_reported() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    let clean = doctor.report(&[]);
    assert_eq!(clean.line("relay journal").tag, "ok");

    let home = doctor.env.relay_home();
    let error = |detail: &str| {
        JournalEntry::GateError(GateError {
            project: Some("lab".to_owned()),
            session_id: None,
            cause: GateFault::FloorInvalid,
            detail: detail.to_owned(),
            command_excerpt: None,
        })
    };
    let now = now_ms();
    append(&home, &[error("first")], now - 60_000).unwrap();
    append(&home, &[error("second")], now).unwrap();
    let today = home
        .journal_dir()
        .join(officina_core::dates::event_file_name(now));
    let mut text = std::fs::read_to_string(&today).unwrap();
    text.push_str("not a journal line\n");
    std::fs::write(&today, text).unwrap();

    let report = doctor.report(&[]);
    let line = report.line("relay journal");
    assert_eq!(line.tag, "warn");
    assert!(
        line.detail
            .contains("2 gate errors in the last 24 h (floor_invalid x2), the last: second"),
        "{line:?}"
    );
    assert!(line.detail.contains("1 unreadable lines"), "{line:?}");
}

// ---- AC 4: levels ----

#[test]
fn missing_gate_with_a_guarded_project_exits_1() {
    let doctor = Doctor::new();
    // The v0.1 block, registered before the relay guarded anything.
    doctor.register_hooks();
    doctor.write(FLOOR);
    doctor.env.approve();
    let report = doctor.report(&[]);
    assert_eq!(report.code, Some(1), "{}", report.text);
    let hooks = report.line("hooks");
    assert_eq!(hooks.tag, "FAIL");
    assert!(
        hooks.detail.starts_with("not registered for PreToolUse"),
        "{hooks:?}"
    );
    assert_eq!(report.failures().len(), 1, "{}", report.text);

    doctor.register_hooks();
    let fixed = doctor.report(&[]);
    assert_eq!(fixed.code, Some(0), "{}", fixed.text);
}

/// Story 2.17 AC 2 and 3, D-452: a gate entry merged before 2.17, without
/// `onFailure: "block"`, fails doctor until the printed block is pasted; a
/// Claude Code older than the version that honours the field fails too,
/// and a newer one is reported as blocking on failure. No line without a
/// guarded project.
#[test]
fn a_gate_that_fails_open_fails_doctor() {
    let doctor = Doctor::new();
    let report = doctor.report(&[]);
    assert!(!report.has("gate failure mode"), "{}", report.text);

    doctor.write(FLOOR);
    doctor.env.approve();
    doctor.register_hooks();
    let settings = doctor.env.claude_config().join("settings.json");
    let mut pasted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    let gate_entry = &mut pasted["hooks"]["PreToolUse"][0]["hooks"][0];
    assert_eq!(gate_entry["onFailure"], "block", "{gate_entry}");
    gate_entry.as_object_mut().unwrap().remove("onFailure");
    std::fs::write(&settings, pasted.to_string()).unwrap();

    let report = doctor.report(&[]);
    assert_eq!(report.code, Some(1), "{}", report.text);
    let hooks = report.line("hooks");
    assert_eq!(hooks.tag, "FAIL");
    assert!(
        hooks
            .detail
            .starts_with("not registered for PreToolUse fail-closed (onFailure: \"block\")"),
        "{hooks:?}"
    );
    assert!(
        hooks.paste.contains("\"onFailure\": \"block\""),
        "{hooks:?}"
    );
    let mode = report.line("gate failure mode");
    assert_eq!(mode.tag, "ok", "{mode:?}");

    doctor.register_hooks();
    let fixed = doctor.report(&[]);
    assert_eq!(fixed.code, Some(0), "{}", fixed.text);

    let [major, minor, patch] = officina_core::contract::ON_FAILURE_SINCE;
    let older = format!("{major}.{minor}.{}", patch - 1);
    let mut cmd = doctor.env.officina(&["doctor"]);
    cmd.env("FAKE_CLAUDE_VERSION", &older);
    let report = Report::parse(&common::run_with(cmd, b""));
    assert_eq!(report.code, Some(1), "{}", report.text);
    let mode = report.line("gate failure mode");
    assert_eq!(mode.tag, "FAIL", "{mode:?}");
    assert!(mode.detail.starts_with(&older), "{mode:?}");
    assert_eq!(report.failures().len(), 1, "{}", report.text);
}

#[test]
fn warnings_alone_exit_0() {
    let doctor = Doctor::new();
    let broken = "\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative.exe\"\n";
    doctor.write(&with_checked_lab());
    doctor.env.approve();
    let slow_check = format!(
        "timeout_s = {}\nargs = [\"--check\"",
        BASH_TOOL_MAX_TIMEOUT_MS / 1000
    );
    let long = format!(
        "{}{broken}",
        with_checked_lab()
            .replace("\"web01\", \"db01\"", "\"web01\"")
            .replace("args = [\"--check\"", &slow_check)
    );
    doctor.write(&long);
    doctor.register_hooks();
    let report = doctor.report(&[]);
    for title in [
        "relay issue",
        "relay approval",
        "relay timeout verify",
        "relay tree lab",
    ] {
        assert_eq!(report.line(title).tag, "warn", "{title}: {}", report.text);
    }
    assert!(report.failures().is_empty(), "{}", report.text);
    assert_eq!(report.code, Some(0), "{}", report.text);
}

// ---- AC 5: the CLAUDE.md paragraph ----

/// The `- \`officina run ...\`` lines of a paragraph, their calls only.
fn listed_calls(paragraph: &str) -> Vec<String> {
    paragraph
        .lines()
        .filter_map(|l| l.strip_prefix("- `"))
        .filter_map(|l| l.split('`').next())
        .map(str::to_owned)
        .collect()
}

/// What the paragraph of every guarded project must list: each action the
/// approved snapshot gives the project, with all its parameters, and nothing
/// else. Taken from the snapshot, so a catalogue change needs no test edit.
fn assert_paragraphs_follow_the_snapshot(doctor: &Doctor) -> Vec<String> {
    let snapshot = doctor.snapshot();
    let report = doctor.report(&[]);
    let mut all = Vec::new();
    for (name, project) in &snapshot.projects {
        let title = format!("CLAUDE.md {name}");
        if project.autonomy != Autonomy::Guarded {
            assert!(!report.has(&title), "{title}");
            continue;
        }
        let paragraph = &report.line(&title).paste;
        let expected: Vec<String> = project
            .actions
            .iter()
            .map(|action| {
                let mut call = format!("officina run {action}");
                for param in snapshot.actions[action].params.keys() {
                    call.push_str(&format!(" {param}=<...>"));
                }
                call
            })
            .collect();
        assert_eq!(listed_calls(paragraph), expected, "{paragraph}");
        all.extend(expected);
    }
    all
}

#[test]
fn claude_md_paragraph_lists_the_projects_actions() {
    let doctor = Doctor::new();
    doctor.write(&with_checked_lab());
    doctor.env.approve();
    let report = doctor.report(&[]);
    let lab = report.line("CLAUDE.md lab");
    assert_eq!(lab.tag, "ok");
    assert!(lab.detail.contains("Officina never writes it"), "{lab:?}");
    let paragraph = &lab.paste;
    assert!(
        paragraph.contains("- `officina run probe host=<...>`: Reads one host.\n"),
        "{paragraph}"
    );
    assert!(
        paragraph.contains(
            "- `officina run apply group=<...>`: Runs only after `officina run verify` succeeded"
        ),
        "{paragraph}"
    );
    assert!(paragraph.contains("`officina run <action> name=value ...`"));
    assert!(paragraph.contains("`officina run --list`"), "{paragraph}");
    assert!(
        paragraph.contains(&format!("Bash timeout of {BASH_TOOL_MAX_TIMEOUT_MS} ms")),
        "{paragraph}"
    );
    assert!(
        paragraph.contains("read the reason before retrying"),
        "{paragraph}"
    );
    for (tool, rule) in SHELL_TOOLS {
        if *rule == ToolRule::DeniedInGuardedProject {
            assert!(
                paragraph.contains(&format!("Officina refuses the {tool} tool here")),
                "D-171: {paragraph}"
            );
        }
    }
    assert!(!report.has("CLAUDE.md notes"), "notes is free");
    assert_paragraphs_follow_the_snapshot(&doctor);
}

#[test]
fn paragraph_follows_the_catalogue() {
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    doctor.env.approve();
    let before = assert_paragraphs_follow_the_snapshot(&doctor);
    doctor.write(&with_checked_lab());
    let pending = assert_paragraphs_follow_the_snapshot(&doctor);
    assert_eq!(pending, before, "a pending change is not in the paragraph");
    doctor.env.approve();
    let after = assert_paragraphs_follow_the_snapshot(&doctor);
    assert_ne!(after, before, "approving the change changes the paragraph");
}

#[test]
fn paragraph_without_approval_lists_nothing() {
    let doctor = Doctor::new();
    doctor.write(&with_checked_lab());
    let report = doctor.report(&[]);
    let paragraph = &report.line("CLAUDE.md lab").paste;
    assert!(listed_calls(paragraph).is_empty(), "{paragraph}");
    assert!(
        paragraph.contains("awaits the author's approval in the Officina window"),
        "{paragraph}"
    );
    for action in ["probe", "verify", "apply"] {
        assert!(
            !paragraph.contains(&format!("officina run {action}")),
            "{paragraph}"
        );
    }
}

// ---- AC 6: zero write ----

#[test]
fn doctor_writes_nothing_in_project_folders() {
    let doctor = Doctor::new();
    let git_dir = git_dir_or_skip("doctor_writes_nothing_in_project_folders (git leg)");
    if let Some(git_dir) = &git_dir {
        git(git_dir, &doctor.lab(), &["init", "-q"]);
        std::fs::write(doctor.lab().join("site.yml"), "- hosts: all\n").unwrap();
        git(git_dir, &doctor.lab(), &["add", "site.yml"]);
        git(git_dir, &doctor.lab(), &["commit", "-q", "-m", "first"]);
        std::fs::write(doctor.lab().join("site.yml"), "- hosts: lab\n").unwrap();
        std::fs::write(doctor.lab().join("notes.txt"), "untracked\n").unwrap();
    }
    doctor.write(&with_checked_lab());
    doctor.env.approve();
    let before = fingerprint(&doctor.work());
    let mut cmd = doctor.env.officina(&["doctor"]);
    if let Some(git_dir) = &git_dir {
        cmd.env("PATH", git_dir);
    }
    cmd.current_dir(doctor.lab());
    let report = Report::parse(&common::run_with(cmd, b""));
    if git_dir.is_some() {
        assert_eq!(
            report.line("relay tree lab").tag,
            "ok",
            "git ran in the project: {}",
            report.text
        );
    }
    assert_eq!(fingerprint(&doctor.work()), before);
}

/// AC 7 of story 2.14 (D-173): a decision the window did not make fails
/// doctor, here a stop flag deleted while the window holds the relay
/// stopped, until the author acknowledges it in the window.
#[test]
fn an_anomaly_fails_doctor_until_acknowledged() {
    use officina_host::relay::authority::{acknowledge_anomaly, set_relay_stopped};
    let doctor = Doctor::new();
    doctor.write(FLOOR);
    doctor.env.approve();
    doctor.register_hooks();
    assert_eq!(doctor.report(&[]).line("relay anomalies").tag, "ok");

    let home = doctor.env.relay_home();
    set_relay_stopped(&home, true).unwrap();
    std::fs::remove_file(home.stopped_file()).unwrap();
    let report = doctor.report(&[]);
    let line = report.line("relay anomalies");
    assert_eq!(line.tag, "FAIL", "{}", report.text);
    assert!(line.detail.contains("stop flag is gone"), "{}", line.detail);
    assert_eq!(report.code, Some(1));

    let (lines, _) = officina_host::relay::journal::read_recent(&home, now_ms());
    for anomaly in officina_host::relay::view::read_anomalies(
        &home,
        &lines,
        home.read_approved().approved().map(|a| a.hash.as_str()),
        home.relay_stopped(),
    ) {
        acknowledge_anomaly(&home, &anomaly.key).unwrap();
    }
    assert_eq!(doctor.report(&[]).line("relay anomalies").tag, "ok");
}

/// AC 1 of story 2.14 (D-173): without the `git` Git for Windows records,
/// a git work tree with actions that bind their files fails doctor, with
/// the reason and the refusal those actions give.
#[test]
fn doctor_fails_a_git_tree_it_cannot_bind() {
    let Some(git_dir) = git_dir_or_skip("doctor_fails_a_git_tree_it_cannot_bind") else {
        return;
    };
    let doctor = Doctor::new();
    git(&git_dir, &doctor.lab(), &["init", "-q"]);
    std::fs::write(doctor.lab().join("site.yml"), "- hosts: lab\n").unwrap();
    git(&git_dir, &doctor.lab(), &["add", "site.yml"]);
    git(&git_dir, &doctor.lab(), &["commit", "-q", "-m", "first"]);
    doctor.write(&with_checked_lab());
    doctor.env.approve();
    let mut cmd = doctor.env.officina(&["doctor"]);
    cmd.env(officina_host::relay::tree::ENV_TEST_GIT, "");
    let report = Report::parse(&common::run_with(cmd, b""));
    let tree = report.line("relay tree lab");
    assert_eq!(tree.tag, "FAIL", "{}", report.text);
    assert!(
        tree.detail.contains("no git is installed") && tree.detail.contains("git_unavailable"),
        "{tree:?}"
    );
    assert_eq!(report.code, Some(1));
}
