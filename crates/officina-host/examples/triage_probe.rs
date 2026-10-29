//! The triage call's workstation gate (story 3.9 AC 13; AD-51, AD-58,
//! D-80, D-91): one real `claude -p` call with the argument vector exactly
//! as Rust quotes it, since E16 verified the flags through a shell, run by
//! `spawn::execute` from core's `TriageCall` vector (D-128). Run by
//! the owner, by hand; never by a test. It spends one Haiku call (about
//! USD 0.001, E16).
//!
//! It passes when the answer is a schema-valid verdict, no transcript
//! exists under the config dir for the session id the call reports
//! (`--no-session-persistence`), and its input tokens are within 20 % of
//! E16's figure. The `claude` process keeps its own state like any session
//! (E17): the probe does not fingerprint the config dir.
//!
//! Usage: `cargo run -p officina-host --features window-authority
//! --example triage_probe`. `claude` is found as the window finds it
//! (`OFFICINA_CLAUDE_BIN`, then `PATH`, then `~/.local/bin`); the config
//! dir is `CLAUDE_CONFIG_DIR`, else `~/.claude`. The work folder is a fresh
//! one under the system temp folder. Prints what it found and the verdict;
//! exit 0 on a pass, 1 on a failure.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use officina_core::registry::triage_call::{TRIAGE_MODEL, TriageCase, prompt, read_output, schema};
use officina_core::registry::types::TriagePurpose;
use officina_host::paths::{ENV_CLAUDE_CONFIG_DIR, env_path, user_home};
use officina_host::registry::triage::{CallLimits, TriageJob, refusal, workdir};
use officina_host::spawn::{BaseEnv, SpawnOutcome, execute, resolve_claude, version};

/// E16's measured input tokens for one short case, cached system prompt
/// included.
const E16_INPUT_TOKENS: i64 = 4_500;

/// How far from E16's figure the call may land (D-91).
const TOLERANCE_PERCENT: i64 = 20;

/// The nonce of the probe's one prompt: any 16 hex digits the case does
/// not hold.
const NONCE: &str = "5ca1ab1e0ddba11e";

/// Every file under `dir` whose name starts with `stem`.
fn files_named(dir: &Path, stem: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_named(&path, stem, found);
        } else if entry.file_name().to_string_lossy().starts_with(stem) {
            found.push(path);
        }
    }
}

fn main() -> ExitCode {
    let Some(claude) = resolve_claude() else {
        eprintln!("triage_probe: claude not found");
        return ExitCode::FAILURE;
    };
    let config_dir = env_path(ENV_CLAUDE_CONFIG_DIR)
        .or_else(|| user_home().map(|home| home.join(".claude")))
        .unwrap_or_default();
    let home = std::env::temp_dir().join(format!("officina-triage-probe-{}", std::process::id()));
    let job = TriageJob {
        purpose: TriagePurpose::DryRun,
        case_key: "probe".into(),
        case: TriageCase {
            last_prompt: Some("Add a CSV export to the invoice list, with tests".into()),
            answer_start: Some("The export is in place and both tests pass.".into()),
            answer_end: None,
            end: Some("prompt_input_exit".into()),
            cut_mid_turn: false,
        },
        claude: Some(claude.path().to_path_buf()),
        officina_home: home.clone(),
        config_dir: env_path(ENV_CLAUDE_CONFIG_DIR),
        no_excerpts: false,
        base_env: BaseEnv::from_process(),
        limits: CallLimits::default(),
    };
    println!("claude: {}", claude.path().display());
    println!(
        "version: {}",
        version(&claude, job.config_dir.as_deref(), &job.base_env)
            .unwrap_or_else(|e| e.to_string())
    );
    if let Err(e) = std::fs::create_dir_all(workdir(&home)) {
        eprintln!("triage_probe: the work folder could not be created: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(refused) = refusal(&job) {
        eprintln!("triage_probe: refused before starting: {}", refused.code());
        return ExitCode::FAILURE;
    }
    let text = match prompt(&job.case, NONCE) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("triage_probe: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = job
        .config_dir
        .as_deref()
        .map(|dir| dir.to_string_lossy().into_owned());
    let vector = officina_core::spawn::triage_call(
        TRIAGE_MODEL,
        &schema(),
        config.as_deref(),
        &home.to_string_lossy(),
    )
    .with_stdin(text.into_bytes());
    let started = Instant::now();
    let ran = match execute(&claude, &vector, &job.base_env) {
        Ok(SpawnOutcome::Captured(ran)) => ran,
        Ok(SpawnOutcome::Started { .. }) => {
            eprintln!("triage_probe: the call was not captured");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("triage_probe: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (stdout, stderr) = (ran.stdout, ran.stderr);
    println!(
        "duration: {:?}",
        started.elapsed().min(Duration::from_secs(600))
    );
    println!("exit: {:?}", ran.status);
    if !stderr.is_empty() {
        println!("stderr: {}", String::from_utf8_lossy(&stderr).trim());
    }
    let (answer, usage) = read_output(&stdout);
    let session_id = serde_json::from_slice::<serde_json::Value>(&stdout)
        .ok()
        .and_then(|v| v.get("session_id")?.as_str().map(str::to_owned));
    println!("answer: {answer:?}");
    println!("usage: {usage:?}");
    println!("session id: {session_id:?}");
    let mut passed = answer.is_ok();
    match &session_id {
        Some(id) => {
            let mut transcripts = Vec::new();
            files_named(&config_dir, id, &mut transcripts);
            println!(
                "transcripts under {} for this session: {}",
                config_dir.display(),
                transcripts.len()
            );
            for found in &transcripts {
                println!("  {}", found.display());
            }
            passed &= transcripts.is_empty();
        }
        None => {
            println!("no session id in the answer: the transcript check cannot run");
            passed = false;
        }
    }
    match usage.input_tokens {
        Some(tokens) => {
            let off = (tokens - E16_INPUT_TOKENS).abs() * 100 / E16_INPUT_TOKENS;
            println!(
                "input tokens: {tokens}, {off} % from E16's {E16_INPUT_TOKENS} (tolerance {TOLERANCE_PERCENT} %)"
            );
            passed &= off <= TOLERANCE_PERCENT;
        }
        None => {
            println!("no input tokens in the answer");
            passed = false;
        }
    }
    let _ = std::fs::remove_dir_all(&home);
    println!("{}", if passed { "PASS" } else { "FAIL" });
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
