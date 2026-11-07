//! The owner's window checks of v0.6's Slice A (story 6.20 AC 3 to 5, story
//! 6.19 AC 8; D-201, D-202, D-610): the checks only the window and the
//! owner's own projects and credential can run. Run by the owner, by hand,
//! from his own console, beside a running Officina window; never by a test,
//! never part of a shipped binary.
//!
//! Build: `cargo build -p officina-host --example slice_a_check`. It runs
//! `officina workshop --json` (`--officina <path>`, else `officina` on the
//! `PATH`) and reads the journal lines of the Officina home (`OFFICINA_HOME`,
//! else `.officina` in the user's home) written since it started; it reads
//! no other file, starts no container and never sees a credential. Each
//! subcommand walks the owner through the window's buttons, asks what only
//! he sees, and appends the steps, his answers, the view's facts and the
//! journal's workshop lines to `slice-a-check-report.md` beside this
//! executable, for the agent to read and redact into the research notes.
//!
//! - `gate3`: story 6.19 AC 8, gate 3's window half: Build images, Run
//!   escape battery, one launch on a scratch repository with no hook, the
//!   review panel's package, the gateway's hosts, no dialog.
//! - `stop`: AC 3, Stop all with two runs live, the time the section shows,
//!   the state after a window restart, Resume collecting one run and
//!   removing the other.
//! - `hooks`: AC 4, gate 4's second half: one run per owner project that
//!   declares hooks, each event's hook counts from `workshop_finished`.
//! - `slice-a`: AC 5, Slice A end to end on one owner project: Build
//!   images, a fresh battery, a launch from a card, the review with any
//!   sensitive group approved, Import; once per credential kind built.

use std::fmt::Write as _;
use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use officina_host::files::now_ms;
use officina_host::paths::officina_home_from_env;
use serde_json::Value;

const USAGE: &str = "usage: slice_a_check gate3|stop|hooks|slice-a [--officina <path>]";

/// Present in every process a Claude Code session starts: the check
/// refuses to run there, since its questions are the owner's.
const SESSION_MARKERS: &[&str] = &["CLAUDECODE", "CLAUDE_CODE_SESSION_ID"];

/// The journal lines the report carries: the workshop's own kinds.
const KIND_PREFIXES: &[&str] = &["workshop_", "review_", "gateway_", "escape_", "sensitive_"];

/// The fields of a journal line the report keeps: never a prompt, a
/// value or a report tail.
const KEPT_FIELDS: &[&str] = &[
    "kind",
    "ts",
    "run_id",
    "template",
    "project",
    "image",
    "outcome",
    "cause",
    "hooks",
    "refused",
    "hosts",
    "allowed",
    "host",
    "commits",
    "files",
    "groups",
    "sensitive",
    "branch",
    "reason",
    "containers",
    "scope",
    "collected",
    "removed",
    "relay_resumed",
    "state",
    "credential_kind",
    "duration_ms",
    "exit_code",
];

struct Check {
    officina: String,
    started_ms: i64,
    report: String,
}

impl Check {
    fn line(&mut self, text: &str) {
        println!("{text}");
        self.report.push_str(text);
        self.report.push('\n');
    }

    fn step(&mut self, text: &str) {
        self.line(&format!("\n### {text}"));
        print!("Press Enter when done. ");
        let _ = std::io::stdout().flush();
        read_line();
    }

    fn ask(&mut self, question: &str) -> bool {
        loop {
            print!("\n{question}\n[y]es / [n]o, a note may follow: ");
            let _ = std::io::stdout().flush();
            let line = read_line();
            let (key, note) = line.split_once(' ').unwrap_or((line.as_str(), ""));
            let yes = match key {
                "y" | "Y" => true,
                "n" | "N" => false,
                _ => continue,
            };
            let note = if note.is_empty() {
                String::new()
            } else {
                format!(" ({note})")
            };
            self.line(&format!(
                "- {question} **{}**{note}",
                if yes { "yes" } else { "no" }
            ));
            return yes;
        }
    }

    fn ask_text(&mut self, question: &str) -> String {
        print!("\n{question} ");
        let _ = std::io::stdout().flush();
        let text = read_line();
        self.line(&format!("- {question} `{text}`"));
        text
    }

    /// The Workshop view as `officina workshop --json` prints it.
    #[expect(
        clippy::disallowed_methods,
        reason = "a workstation check reads the view through the CLI by hand, outside spawn (D-128)"
    )]
    fn view(&mut self) -> Option<Value> {
        let output = Command::new(&self.officina)
            .args(["workshop", "--json"])
            .output();
        match output {
            Ok(out) if out.status.success() => serde_json::from_slice(&out.stdout).ok(),
            Ok(out) => {
                self.line(&format!(
                    "- `officina workshop --json` exited {:?}",
                    out.status.code()
                ));
                None
            }
            Err(e) => {
                self.line(&format!(
                    "- `officina workshop --json` could not start: {e}"
                ));
                None
            }
        }
    }

    /// The view's facts this check records: engine, flags, images, runs,
    /// credential state, never an action's text.
    fn record_view(&mut self, label: &str) {
        let Some(view) = self.view() else { return };
        let mut text = format!("- view {label}: engine `{}`", view["engine"]["status"]);
        let _ = write!(
            text,
            ", workshop stopped `{}`, relay stopped `{}`, credential `{}` `{}`",
            view["workshop_stopped"],
            view["relay_stopped"],
            view["credential"]["kind"],
            view["credential"]["state"]
        );
        for image in view["images"].as_array().into_iter().flatten() {
            let _ = write!(
                text,
                "; image `{}` {} battery {} valid {}",
                image["tag"],
                image["state"],
                image["battery"]["outcome"],
                image["battery"]["valid"]
            );
        }
        for list in ["live", "kept"] {
            let runs: Vec<String> = view["runs"][list]
                .as_array()
                .into_iter()
                .flatten()
                .map(|run| format!("{} {}", run["id"], run["state"]))
                .collect();
            let _ = write!(text, "; {list} [{}]", runs.join(", "));
        }
        let _ = write!(
            text,
            "; orphans {}",
            view["runs"]["orphans"].as_array().map_or(0, Vec::len)
        );
        self.line(&text);
    }

    /// The journal's workshop lines since the check started, their kept
    /// fields only.
    fn record_journal(&mut self) {
        let Some(home) = officina_home_from_env() else {
            self.line("- no Officina home found");
            return;
        };
        let mut lines: Vec<Value> = journal_lines(&home.join("journal"))
            .into_iter()
            .filter(|line| line["ts"].as_i64().is_some_and(|ts| ts >= self.started_ms))
            .filter(|line| {
                line["kind"]
                    .as_str()
                    .is_some_and(|kind| KIND_PREFIXES.iter().any(|p| kind.starts_with(p)))
            })
            .collect();
        lines.sort_by_key(|line| line["ts"].as_i64());
        self.line(&format!(
            "\n#### Journal since the start ({} lines)\n",
            lines.len()
        ));
        for line in lines {
            let kept: serde_json::Map<String, Value> = line
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(key, _)| KEPT_FIELDS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            self.line(&format!("- `{}`", Value::Object(kept)));
        }
    }
}

fn journal_lines(dir: &Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .flat_map(|text| {
            text.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect::<Vec<Value>>()
        })
        .collect()
}

fn read_line() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_owned()
}

fn gate3(check: &mut Check) {
    check.line("\n## Gate 3, window half (story 6.19 AC 8)");
    check.step(
        "Make a scratch git repository with one committed file and `.claude/` in its `.gitignore`, no hook anywhere; declare it as a project with a `[project.<name>.workshop]` table and a template whose prompt edits the file, runs `git status` and commits; approve the catalogue",
    );
    check.step("In the Orchestrator view's Workshop section, click Build images and wait until every image reads ready");
    check.step(
        "Click Run escape battery and wait until each image's battery reads passed and valid",
    );
    check.record_view("after the battery");
    check.step("Compose the template on the scratch project and click Launch on the card");
    check.step("Wait for the run's card to reach Needs you, then click Receive");
    check.ask("Does the review panel show the run's commit in its package?");
    check.ask("Did any dialog or prompt appear during the run (in the window, a console or Docker Desktop)?");
    check.ask_text("The hosts the review panel lists as allowed and refused:");
    check.record_view("after the run");
    check.record_journal();
}

fn stop(check: &mut Check) {
    check.line("\n## Stop all on the workstation (story 6.20 AC 3)");
    check
        .step("Launch two workshop runs from the window and wait until the header's count reads 2");
    check.record_view("with two runs live");
    check.step("Click Stop all in the header");
    check.ask_text("The time the section's Runs line shows after `Stopped ... in` (seconds):");
    check.ask("Did every worker and gateway container stop (Docker Desktop shows none running)?");
    check.ask("Does the Relay view read stopped?");
    check.ask("Are your editor's own Claude Code sessions untouched?");
    check.step("Close the Officina window and start it again");
    check.ask("Does the Workshop section still read stopped?");
    check.record_view("after the restart");
    check.step("Click Resume; in the dialog keep one run on Collect, set the other to Remove, keep the relay checked, click Resume");
    check.ask("Did one run go to collection and the other disappear from the kept list?");
    check.record_view("after Resume");
    check.record_journal();
}

fn hooks(check: &mut Check) {
    check.line("\n## Gate 4, second half (story 6.20 AC 4)");
    let projects = check.ask_text(
        "Your projects that declare hooks, separated by commas (names stay in this report):",
    );
    for project in projects.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        check.step(&format!("Build {project}'s project image, then launch one run on {project} from the window and wait until it is collected"));
        check.ask(&format!(
            "Does {project}'s review panel show every configured event's hooks ran?"
        ));
        check.ask_text(&format!("Any hook of {project} that did not run, or a source the mounts cannot carry that it relies on (empty for none):"));
    }
    check.record_journal();
}

fn slice_a(check: &mut Check) {
    check.line("\n## Slice A usable alone (story 6.20 AC 5)");
    check.ask_text("The floor's credential kind for this pass (setup-token or login):");
    if check.ask("Is this pass on a `login` floor?") {
        check.step("In the Workshop section's Credential part, click Log in for the workshop, run the command it shows in your console and log in");
        check.step("Click Check login");
        check.ask("Did Check login read logged in?");
    }
    check.step("On one of your projects with its toolchain image: click Build images and wait until every image reads ready");
    check.step("Click Run escape battery and wait until the battery reads passed and valid");
    check.record_view("before the launch");
    check.step("Compose a template on that project, click Launch on the card, and wait until the run's card reaches Needs you");
    check.step("Click Receive; approve any sensitive group; click Import");
    check.ask("Did Import create a branch named `officina/<name>` in the project?");
    check.ask_text("The run's id (the review panel's header):");
    check.record_view("after Import");
    check.record_journal();
}

fn main() -> ExitCode {
    if SESSION_MARKERS
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        eprintln!("slice_a_check: run it from your own console, not from a Claude Code session");
        return ExitCode::from(64);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().cloned() else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let officina = match args.get(1).map(String::as_str) {
        Some("--officina") => match args.get(2) {
            Some(path) => path.clone(),
            None => {
                eprintln!("{USAGE}");
                return ExitCode::from(64);
            }
        },
        Some(_) => {
            eprintln!("{USAGE}");
            return ExitCode::from(64);
        }
        None => "officina".to_owned(),
    };
    let mut check = Check {
        officina,
        started_ms: now_ms(),
        report: String::new(),
    };
    match command.as_str() {
        "gate3" => gate3(&mut check),
        "stop" => stop(&mut check),
        "hooks" => hooks(&mut check),
        "slice-a" => slice_a(&mut check),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(64);
        }
    }
    let path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("slice-a-check-report.md");
    let appended = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(check.report.as_bytes()));
    match appended {
        Ok(()) => {
            println!("\nReport appended to {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("the report could not be written to {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}
