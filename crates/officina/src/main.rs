//! `officina`: the console binary. Claude Code runs `hook` and `quota write`;
//! the author runs `doctor` and `board --json`. Arguments are parsed by hand
//! (AD-16): four subcommands do not justify a parser in the hook's image.
//!
//! Exit codes never use 2: Claude Code reads a hook exiting 2 as "block",
//! so a mistyped hook command must not be able to stop sessions. Usage errors
//! exit 64 (EX_USAGE), runtime failures 1, a stale board 3.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use officina_host::collector::Collector;
use officina_host::doctor::{self, Level};
use officina_host::files::now_ms;
use officina_host::paths::Paths;
use officina_host::relay::gate::{self, GATE_EVENT, GateEnv};
use officina_host::relay::home::RelayHome;
use officina_host::writers::{hook_keeps_excerpts, record_hook_payload, record_quota};

const USAGE: &str = "\
usage: officina <command>

  board --json [--floor <path>]   print the board once (exit 3 when stale)
  doctor [--floor <path>]         check the setup, print what to paste
  hook                            record a Claude Code hook payload (stdin)
  quota write [--tee]             record a statusline payload (stdin)
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
    officina_host::files::retention_pass(&collector.paths.events_dir(), now);
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

/// Extracts `--floor <path>` from the remaining arguments.
fn floor_arg(args: &[String]) -> Result<Option<PathBuf>, ()> {
    match args.iter().position(|a| a == "--floor") {
        None => Ok(None),
        Some(i) => args.get(i + 1).map(|p| Some(PathBuf::from(p))).ok_or(()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("hook") => hook(),
        Some("quota") if rest.first().map(String::as_str) == Some("write") => {
            quota_write(rest.iter().any(|a| a == "--tee"))
        }
        Some("board") if rest.iter().any(|a| a == "--json") => match floor_arg(rest) {
            Ok(floor) => board(floor.as_ref()),
            Err(()) => usage_error(),
        },
        Some("doctor") => match floor_arg(rest) {
            Ok(floor) => doctor_cmd(floor.as_ref()),
            Err(()) => usage_error(),
        },
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
