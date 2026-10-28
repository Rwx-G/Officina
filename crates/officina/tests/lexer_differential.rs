//! The gate's lexer against Git Bash's own parse (story 2.16 AC 4, D-175).
//!
//! Each command of the corpus, `tests/vectors/gate.json` and variants built
//! from the gate's constants (quoting, escapes, line continuations,
//! here-documents, process substitutions, compound commands, assignments
//! next to `officina`), is written as data to a script file in a temporary
//! folder, and Git Bash prints its parse of that file with `--pretty-print`:
//! the canonical form `declare -f` prints, line continuations joined,
//! comments dropped, every construct closed, without running anything, not
//! even a function definition. Defining `f() { <command>; }` to print it
//! with `declare -f f` would run the script's top level, and a command
//! holding a stray `}` would close the function and run there: the
//! pretty-printer prints the same form and runs nothing whatever the
//! command holds. The gate then decides both texts: when
//! bash's canonical form is denied and the command as written is not, the
//! lexer missed what bash reads, and the test fails. The other way round
//! (the lexer reports more) is the gate's design (AD-27: report more, never
//! less).
//!
//! `#[ignore]`d: it starts Git Bash once per command. The windows job of CI
//! runs it in a step of its own; without Git Bash it says so and passes.
//! `the_parse_runs_nothing` holds the "never runs a command" promise with
//! commands that would leave a marker file, and a positive control.

#![cfg(windows)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use officina_core::floor::parse_floor;
use officina_core::relay::catalogue::{DEFAULT_GUARDED_PROGRAMS, guarded_programs};
use officina_core::relay::gate::ToolRule;
use officina_core::relay::gate::{
    DenyKind, GateInputs, HOME_REDIRECTION_NAMES, OFFICINA_ENVIRONMENT_NAMES, OFFICINA_PROGRAM,
    decide, tool_rule,
};
use officina_host::platform::windows::{Hive, registry_string};
use officina_host::process::{Waited, hide_console, wait_until};
use officina_host::relay::tree::{GIT_FOR_WINDOWS_KEY, GIT_INSTALL_PATH_VALUE};

const VECTORS: &str = include_str!("../../../tests/vectors/gate.json");

/// Git Bash's own `bash.exe` under the install folder Git for Windows
/// records: the MSYS one, not the `bin` launcher that rewrites `PATH`.
const BASH_UNDER_INSTALL: &str = r"usr\bin\bash.exe";

/// How long one parse may take before the test gives up on it.
const PARSE_DEADLINE: Duration = Duration::from_secs(20);

/// How many parses run at once.
const WORKERS: usize = 8;

/// Git Bash, from the registry as the relay finds `git`, or why not.
fn git_bash() -> Result<PathBuf, String> {
    [Hive::LocalMachine, Hive::CurrentUser]
        .into_iter()
        .filter_map(|hive| registry_string(hive, GIT_FOR_WINDOWS_KEY, GIT_INSTALL_PATH_VALUE))
        .map(|install| Path::new(&install).join(BASH_UNDER_INSTALL))
        .find(|bash| bash.is_file())
        .ok_or_else(|| format!("no Git Bash under the {GIT_FOR_WINDOWS_KEY} install folder"))
}

/// What Git Bash made of a command.
#[derive(Debug)]
enum Parse {
    /// Its canonical form.
    Canonical(String),
    /// It is no valid bash: bash would run none of it.
    Refused,
}

/// Writes `command` to a script file in a fresh folder and has `bash` print
/// its parse: `--pretty-print` reads the file and prints each command it
/// parses, running none. The command is data in a file, never part of an
/// argument; bash starts with no rc file, an empty environment but a `PATH`
/// of one empty folder, the folder as its working directory, no console,
/// and a deadline.
fn parse(bash: &Path, command: &str) -> Parse {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let script = dir.path().join("command.sh");
    std::fs::write(&script, format!("{command}\n")).unwrap();
    let mut cmd = Command::new(bash);
    cmd.args(["--norc", "--noprofile", "--pretty-print"])
        .arg(script.to_string_lossy().replace('\\', "/"))
        .env_clear()
        .env("PATH", &empty)
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut cmd);
    let mut child = cmd.spawn().expect("Git Bash starts");
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |mut pipe: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = pipe.read_to_end(&mut text);
            text
        })
    };
    let (out, err) = (read(Box::new(stdout)), read(Box::new(stderr)));
    match wait_until(&mut child, Instant::now() + PARSE_DEADLINE) {
        Waited::Exited(status) => {
            let out = String::from_utf8_lossy(&out.join().unwrap()).into_owned();
            let err = String::from_utf8_lossy(&err.join().unwrap()).into_owned();
            if status.success() && err.trim().is_empty() {
                Parse::Canonical(out)
            } else {
                Parse::Refused
            }
        }
        Waited::Deadline | Waited::Lost(_) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Git Bash did not parse within {PARSE_DEADLINE:?}: {command:?}");
        }
    }
}

/// One spelling of a list entry: a prefix entry (`ansible*`) as a name it
/// matches.
fn instance(entry: &str) -> String {
    entry
        .strip_suffix('*')
        .map_or_else(|| entry.to_owned(), |stem| format!("{stem}-x"))
}

/// Commands built from the gate's constants, each one way of writing a
/// guarded program in command position, or a variable next to `officina`,
/// that bash reads; and ways of writing it that bash does not run (a
/// comment, a quoted here-document), which only make the lexer report more.
fn generated() -> Vec<String> {
    let mut commands = Vec::new();
    for entry in DEFAULT_GUARDED_PROGRAMS {
        let p = instance(entry);
        let (head, tail) = p.split_at(p.len() / 2);
        let first = &p[..1];
        let rest = &p[1..];
        commands.extend([
            format!("{p} host01"),
            format!("\"{p}\" host01"),
            format!("'{p}' host01"),
            format!("{p}\"\" host01"),
            format!("\\{first}{rest} host01"),
            format!("{head}\\\n{tail} host01"),
            format!("{head}\"{tail}\" host01"),
            format!("{head}'{tail}' host01"),
            format!("{p}\\\n host01"),
            format!("echo ok && \\\n  {p} host01"),
            format!("echo a; \\\n{p} host01"),
            format!("cat <<EOF\nline\nEOF\n{p} host01"),
            format!("cat <<-EOF\n\tline\n\tEOF\n{p} host01"),
            format!("cat <<EOF; {p} host01\nline\nEOF"),
            format!("cat <<EOF\n$({p} host01)\nEOF"),
            format!("cat <<'EOF'\n$({p} host01)\nEOF"),
            format!("cat <<EOF\n`{p} host01`\nEOF"),
            format!("diff <({p} host01 cat /etc/hosts) /etc/hosts"),
            format!("cat < <({p} host01)"),
            format!("tee >({p} host01) < /dev/null"),
            format!("x=$({p} host01)"),
            format!("echo `{p} host01`"),
            format!("echo \"$({p} host01)\""),
            format!("{{ {p} host01; }}"),
            format!("({p} host01)"),
            format!("if true; then {p} host01; fi"),
            format!("case x in x) {p} host01;; esac"),
            format!("for i in 1; do {p} host01; done"),
            format!("while false; do {p} host01; done"),
            format!("{p} host01 &"),
            format!("! {p} host01"),
            format!("time {p} host01"),
            format!("true | {p} host01"),
            format!("true |& {p} host01"),
            format!("[[ -n x ]] && {p} host01"),
            format!("(( 1 )) && {p} host01"),
            format!("coproc {p} host01"),
            format!("echo x # {p} host01"),
            format!("echo {p}"),
            // A line break ends a command as `;` does: what follows is in
            // command position, a reserved word included.
            format!("echo ok\n{{ {p} host01; }}"),
            format!("echo ok\nfunction g {{ {p} host01; }}\ng"),
            format!("echo ok\n\nif true; then {p} host01; fi"),
            format!("echo ok\n( {p} host01 )"),
            format!("echo ok\n! {p} host01"),
            format!("echo ok\ntime {p} host01"),
            format!("echo ok\ncoproc {p} host01"),
            format!("echo ok\nwhile false; do {p} host01; done"),
        ]);
    }
    let names = HOME_REDIRECTION_NAMES
        .iter()
        .chain(OFFICINA_ENVIRONMENT_NAMES)
        .map(|name| name.replace('*', "DIR"));
    for name in names {
        let (head, tail) = name.split_at(name.len() / 2);
        let run = format!("{OFFICINA_PROGRAM} run probe");
        commands.extend([
            format!("{name}=/tmp {run}"),
            format!("{head}\\\n{tail}=/tmp {run}"),
            format!("{name}=/tmp \\\n {run}"),
            format!("export {name}=/tmp; {run}"),
            format!("export {name}=/tmp\n{run}"),
            format!("env {name}=/tmp {run}"),
            format!("{name}=\"$(echo /tmp)\" {run}"),
        ]);
    }
    commands
}

/// The commands of the gate's vector set that a lexed shell tool runs, and
/// each with a line continuation inside its first word and behind a
/// here-document, two places bash reads differently from the text.
fn corpus() -> Vec<String> {
    let set: serde_json::Value = serde_json::from_str(VECTORS).expect("gate.json parses");
    let mut commands = Vec::new();
    for case in set["cases"].as_array().expect("cases") {
        let tool = case["tool"].as_str().unwrap_or("Bash");
        let Some(command) = case["command"].as_str() else {
            continue;
        };
        if tool_rule(tool) != Some(ToolRule::Lexed) || command.is_empty() {
            continue;
        }
        commands.push(command.to_owned());
        let word_end = command
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(command.len());
        if word_end >= 2 {
            commands.push(format!("{}\\\n{}", &command[..1], &command[1..]));
        }
        commands.push(format!("cat <<EOF\nline\nEOF\n{command}"));
    }
    commands
}

/// The gate's decision on `text` in a guarded project with the default
/// list.
fn denied(text: &str, list: &[String]) -> Option<DenyKind> {
    decide(
        text,
        &GateInputs {
            guarded_programs: list,
            protected_dirs: &[],
            cwd_guarded: true,
        },
    )
    .map(|deny| deny.kind)
}

/// What the run found.
#[derive(Default)]
struct Tally {
    compared: usize,
    refused_by_bash: usize,
    reported_more: usize,
    misses: Vec<String>,
}

/// AC 4: every command bash reads as starting a guarded program, or
/// assigning a variable next to `officina`, is denied as written; a miss is
/// printed with bash's canonical form, and fails the test.
#[test]
#[ignore = "starts Git Bash once per command; run by the windows job of CI"]
fn the_lexer_misses_nothing_bash_parses() {
    let bash = match git_bash() {
        Ok(bash) => bash,
        Err(why) => {
            println!("skipped: {why}");
            return;
        }
    };
    let list = guarded_programs(&parse_floor("schema = 2\n").unwrap());
    let mut commands: Vec<String> = corpus().into_iter().chain(generated()).collect();
    let mut seen = BTreeSet::new();
    commands.retain(|command| seen.insert(command.clone()));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let tally = std::sync::Mutex::new(Tally::default());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(command) = commands.get(i) else {
                        break;
                    };
                    let parsed = parse(&bash, command);
                    let mut tally = tally.lock().unwrap();
                    match parsed {
                        Parse::Refused => tally.refused_by_bash += 1,
                        Parse::Canonical(canonical) => {
                            tally.compared += 1;
                            let read = denied(&canonical, &list);
                            let written = denied(command, &list);
                            match (read, written) {
                                (Some(kind), None) => tally.misses.push(format!(
                                    "{command:?}: bash reads {canonical:?}, which the gate denies ({kind:?}); as written, no decision"
                                )),
                                (None, Some(_)) => tally.reported_more += 1,
                                _ => {}
                            }
                        }
                    }
                }
            });
        }
    });
    let tally = tally.into_inner().unwrap();
    println!(
        "{} commands: {} compared, {} refused by bash, {} where the lexer reports more, {} misses",
        commands.len(),
        tally.compared,
        tally.refused_by_bash,
        tally.reported_more,
        tally.misses.len()
    );
    assert!(
        tally.compared > commands.len() / 2,
        "bash parsed too few commands for the comparison to mean anything"
    );
    assert!(
        tally.misses.is_empty(),
        "the lexer misses what bash reads:\n{}",
        tally.misses.join("\n")
    );
}

/// The parse runs nothing: commands that would leave a marker file if they
/// ran, top-level, in a substitution, in a function body or closing one,
/// leave none; the same file run by bash without `--pretty-print` does
/// (positive control: the commands are real).
#[test]
#[ignore = "starts Git Bash; run by the windows job of CI"]
fn the_parse_runs_nothing() {
    let bash = match git_bash() {
        Ok(bash) => bash,
        Err(why) => {
            println!("skipped: {why}");
            return;
        }
    };
    let dir = tempfile::tempdir().unwrap();
    let marker = |name: &str| dir.path().join(name).to_string_lossy().replace('\\', "/");
    let commands = [
        format!(": > '{}'", marker("top")),
        format!("x=$(: > '{}')", marker("substitution")),
        format!("f() {{ :; }}; : > '{}'; g() {{ :; }}", marker("between")),
        format!("for i in $(: > '{}'); do :; done", marker("for-list")),
        format!("exec 3> '{}'", marker("exec")),
    ];
    for command in &commands {
        assert!(
            matches!(parse(&bash, command), Parse::Canonical(_)),
            "{command}"
        );
    }
    for name in ["top", "substitution", "between", "for-list", "exec"] {
        assert!(
            !Path::new(&marker(name)).exists(),
            "--pretty-print ran {name}"
        );
    }
    let script = dir.path().join("control.sh");
    std::fs::write(&script, format!("{}\n", commands[0])).unwrap();
    let mut control = Command::new(&bash);
    control
        .args(["--norc", "--noprofile"])
        .arg(script.to_string_lossy().replace('\\', "/"))
        .env_clear()
        .current_dir(dir.path())
        .stdin(Stdio::null());
    hide_console(&mut control);
    assert!(control.status().unwrap().success());
    assert!(
        Path::new(&marker("top")).exists(),
        "positive control: the same command, run, leaves its marker"
    );
}
