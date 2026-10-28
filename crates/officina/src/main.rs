//! `officina`: the console binary. Claude Code runs `hook` and `quota write`;
//! sessions run `run`; the author runs `doctor`, `board --json` and
//! `relay --json`.
//! Arguments are parsed by hand (AD-16, AD-29): a few subcommands do not
//! justify a parser in the hook's image.
//!
//! Exit codes never use 2: Claude Code reads a hook exiting 2 as "block",
//! so a mistyped hook command must not be able to stop sessions. Usage errors
//! exit 64 (EX_USAGE), runtime failures 1, a stale board 3; `run` passes the
//! program's code through and uses 126, 124 and 127 as a shell does.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use officina_host::collector::Collector;
use officina_host::doctor::{self, Level};
use officina_host::files::now_ms;
use officina_host::paths::Paths;
use officina_host::relay::gate::{self, GATE_EVENT, GateEnv};
use officina_host::relay::home::RelayHome;
use officina_host::relay::run::{self as relay_run, RunArgs};
use officina_host::relay::view::read_view;
use officina_host::writers::{hook_keeps_excerpts, record_hook_payload, record_quota};

const USAGE: &str = "\
usage: officina <command>

  board --json [--floor <path>]   print the board once (exit 3 when stale)
  doctor [--floor <path>]         check the setup, print what to paste
  hook                            record a Claude Code hook payload (stdin)
  quota write [--tee]             record a statusline payload (stdin)
  relay --json                    print the relay's state once: approved
                                  catalogue, pending change, recent activity
  run <action> [name=value ...] [--wait <s>]
                                  run an approved relay action from its project;
                                  a human action waits for the author's approval
                                  up to <s> seconds (exit: its own code, 126
                                  refused, 124 timed out, 127 not started)
  run --list                      list the actions this folder's project may run
  version                         print the version";

fn read_stdin() -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut buf);
    buf
}

/// `officina hook`: exits 0 on every path, panics included (FR-3), so a broken
/// Officina can never block or slow a session beyond its own runtime.
///
/// stdin is parsed once (AD-26): `PreToolUse` goes to the gate, which prints
/// a deny or nothing and is never appended to the event buffer (volume, and
/// `tool_input` can carry secrets); every other event is recorded as in v0.1.
fn hook() -> ExitCode {
    let _ = std::panic::catch_unwind(|| {
        let stdin = read_stdin();
        let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&stdin) else {
            return;
        };
        if payload
            .get("hook_event_name")
            .and_then(serde_json::Value::as_str)
            == Some(GATE_EVENT)
        {
            gate_hook(&payload);
            return;
        }
        if let Some(paths) = Paths::from_env(None) {
            let claude_pid = std::env::var("CLAUDE_PID")
                .ok()
                .and_then(|v| v.parse().ok());
            let _ = record_hook_payload(
                &paths,
                &payload,
                now_ms(),
                hook_keeps_excerpts(&paths),
                claude_pid,
            );
        }
    });
    ExitCode::SUCCESS
}

/// The gate's half of the hook: prints the deny JSON on stdout, or nothing.
fn gate_hook(payload: &serde_json::Value) {
    let Some(home) = RelayHome::from_env() else {
        return;
    };
    if let Some(deny) = gate::answer(&home, payload, &GateEnv::from_env(), now_ms()) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{deny}");
        let _ = out.flush();
    }
}

/// `officina quota write [--tee]`: silent unless `--tee`, which copies stdin
/// to stdout unchanged so it can sit in front of an existing statusline.
fn quota_write(tee: bool) -> ExitCode {
    let stdin = read_stdin();
    if tee {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(&stdin);
        let _ = out.flush();
    }
    let _ = std::panic::catch_unwind(|| {
        if let Some(paths) = Paths::from_env(None) {
            let _ = record_quota(&paths, &stdin, now_ms() / 1000);
        }
    });
    ExitCode::SUCCESS
}

const EXIT_USAGE: u8 = 64;

fn collector(floor: Option<&PathBuf>) -> Result<Collector, ExitCode> {
    Collector::new(floor.map(PathBuf::as_path)).map_err(|e| {
        eprintln!("officina: {e}");
        ExitCode::FAILURE
    })
}

fn board(floor: Option<&PathBuf>) -> ExitCode {
    let mut collector = match collector(floor) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let now = now_ms();
    officina_host::files::retention_pass(&collector.paths().events_dir(), now);
    collector.refresh_agents(now);
    let board = collector.board(now);
    match serde_json::to_string_pretty(&board) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("officina: {e}");
            return ExitCode::FAILURE;
        }
    }
    if board.stale.is_some() {
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    }
}

fn doctor_cmd(floor: Option<&PathBuf>) -> ExitCode {
    let collector = match collector(floor) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("officina"));
    let checks = doctor::run(&collector, &exe);
    for c in &checks {
        let tag = match c.level {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        };
        println!("[{tag}] {}: {}", c.title, c.detail);
        if let Some(paste) = &c.paste {
            println!("{paste}");
        }
    }
    ExitCode::from(u8::try_from(doctor::exit_code(&checks)).unwrap_or(1))
}

fn usage_error() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(EXIT_USAGE)
}

/// `officina run`: the relay's authority (AD-29). The exit code is the
/// program's own, full Windows value included, so it leaves through
/// `std::process::exit` rather than an 8-bit `ExitCode`.
fn run_cmd(args: &[String]) -> ExitCode {
    let parsed = match relay_run::parse_args(args) {
        Ok(parsed) => parsed,
        Err(problem) => {
            eprintln!("officina: run: {problem}");
            return usage_error();
        }
    };
    let Some(home) = RelayHome::from_env() else {
        eprintln!(
            "officina: refused: no Officina home, set OFFICINA_HOME or USERPROFILE (no_home)"
        );
        std::process::exit(relay_run::EXIT_REFUSED);
    };
    // An unreadable working directory lies in no project: refused.
    let cwd = std::env::current_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    match parsed {
        RunArgs::List => {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(relay_run::list(&home, &cwd).as_bytes());
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        RunArgs::Action {
            action,
            params,
            wait_s,
        } => std::process::exit(relay_run::run(
            &home,
            &cwd,
            &action,
            &params,
            relay_run::session_id_from_env(),
            std::time::Duration::from_secs(u64::from(wait_s)),
        )),
    }
}

/// `officina relay --json` (D-24): the relay view the window shows, read
/// only. Exit 0, or 1 when the Officina home cannot be determined or read.
fn relay_json() -> ExitCode {
    let Some(home) = RelayHome::from_env() else {
        eprintln!("officina: no Officina home, set OFFICINA_HOME or USERPROFILE");
        return ExitCode::FAILURE;
    };
    let view = match read_view(&home, now_ms()) {
        Ok(view) => view,
        Err(e) => {
            eprintln!("officina: {e}");
            return ExitCode::FAILURE;
        }
    };
    match serde_json::to_string_pretty(&view) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("officina: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--floor` given as the last argument, without its path: a usage error.
struct FloorWithoutPath;

/// Extracts `--floor <path>` from the remaining arguments.
fn floor_arg(args: &[String]) -> Result<Option<PathBuf>, FloorWithoutPath> {
    match args.iter().position(|a| a == "--floor") {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .map(|p| Some(PathBuf::from(p)))
            .ok_or(FloorWithoutPath),
    }
}

impl FloorWithoutPath {
    /// Says what is missing, then the usage, and exits 64.
    fn usage_error(self) -> ExitCode {
        eprintln!("officina: --floor takes a path");
        usage_error()
    }
}

fn main() -> ExitCode {
    // The hook exits 0 whatever follows it (FR-3).
    if std::env::args_os().nth(1).is_some_and(|a| a == "hook") {
        return hook();
    }
    // An argument that is not Unicode (a lone surrogate on Windows) is a
    // usage error, not the panic `std::env::args` would raise.
    let Ok(args) = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<Vec<String>, _>>()
    else {
        return usage_error();
    };
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("hook") => hook(),
        Some("quota") if rest.first().map(String::as_str) == Some("write") => {
            quota_write(rest.iter().any(|a| a == "--tee"))
        }
        Some("board") if rest.iter().any(|a| a == "--json") => match floor_arg(rest) {
            Ok(floor) => board(floor.as_ref()),
            Err(missing) => missing.usage_error(),
        },
        Some("doctor") => match floor_arg(rest) {
            Ok(floor) => doctor_cmd(floor.as_ref()),
            Err(missing) => missing.usage_error(),
        },
        Some("run") => run_cmd(rest),
        Some("relay") if rest == ["--json"] => relay_json(),
        Some("version" | "--version") => {
            println!("officina {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => usage_error(),
    }
}
