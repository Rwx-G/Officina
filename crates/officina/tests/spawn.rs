//! The spawn module end to end (story 5.2; AD-79, AD-80, AD-95 spawn row,
//! D-128 as amended by D-561 and D-562): every kind through
//! `spawn::execute` against a copy of the fake claude, configured by its
//! sidecar (D-86), with a base environment the test builds. The fake logs
//! its argv, working directory and variable names (D-129).

mod common;
#[path = "../../../tests/support/spawn_cases.rs"]
mod spawn_cases;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use officina_core::spawn::{
    AGENT_VIEW_VAR, AGENTS_STDOUT_CAP, ClaudeVector, Console, CwdRule, EnvRule,
    SESSION_ENV_REMOVED, VectorKind, passed_to_contained, removed_from_sessions,
};
use officina_host::spawn::{BaseEnv, ResolvedClaude, SpawnError, SpawnOutcome, execute};

/// The names the author set for his own sessions, which a session kind
/// keeps (review F1).
const AUTHOR_SETTINGS: &[&str] = &["OFFICINA_HOME", "OFFICINA_FLOOR", "OFFICINA_NO_EXCERPTS"];

/// A name on no list.
const UNLISTED: &str = "OFFICINA_SPAWN_TEST_UNLISTED_NAME";

/// A temporary folder holding an Officina home and copies of the fake.
struct Setup {
    root: tempfile::TempDir,
}

impl Setup {
    fn new() -> Setup {
        let setup = Setup {
            root: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(setup.triage_workdir()).unwrap();
        setup
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("officina-home")
    }

    fn triage_workdir(&self) -> PathBuf {
        officina_host::registry::triage::workdir(&self.home())
    }

    fn log(&self, name: &str) -> PathBuf {
        self.root.path().join(format!("{name}.log"))
    }

    /// A copy of the fake in its own folder `name`, logging to
    /// [`Setup::log`] of the same name, configured by `sidecar` beside.
    fn fake(&self, name: &str, mut sidecar: serde_json::Value) -> ResolvedClaude {
        sidecar["log"] = self.log(name).to_string_lossy().into();
        sidecar["fixture"] = self
            .root
            .path()
            .join("agents.json")
            .to_string_lossy()
            .into();
        if !self.root.path().join("agents.json").exists() {
            std::fs::write(self.root.path().join("agents.json"), "[]").unwrap();
        }
        ResolvedClaude::new(common::fake_claude_in(
            &self.root.path().join(name),
            &sidecar,
        ))
    }

    /// The fake's mode that answers `kind` at once.
    fn fake_for(&self, kind: VectorKind) -> ResolvedClaude {
        let mode = match kind {
            VectorKind::Agents | VectorKind::Version => "ok",
            VectorKind::Attach => "interactive",
            VectorKind::TriageCall => "triage",
            VectorKind::WorkOrder => "bg",
        };
        self.fake(kind.code(), serde_json::json!({"mode": mode}))
    }

    /// The first golden case of `kind`, built with this setup's home.
    fn vector(&self, kind: VectorKind) -> ClaudeVector {
        spawn_cases::load()
            .cases
            .iter()
            .find(|case| case.kind() == kind)
            .map(|case| self.build(case))
            .expect("every kind has a golden case")
    }

    /// `case`'s vector built with this setup's home, the project folder a
    /// work order runs in created under it.
    fn build(&self, case: &spawn_cases::Case) -> ClaudeVector {
        let vector = case.build(Some(&self.home().to_string_lossy()));
        if let CwdRule::Project { folder } = vector.cwd() {
            std::fs::create_dir_all(folder).unwrap();
        }
        vector
    }
}

/// The base environment of AD-80's host test: what a child needs to start
/// on Windows, every name of `SESSION_ENV_REMOVED` (the first in mixed
/// case), the author's `OFFICINA_*` settings, agent view off and a name on
/// no list.
fn seeded_base() -> Vec<(String, String)> {
    let mut base = vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )];
    for (at, name) in SESSION_ENV_REMOVED.iter().enumerate() {
        let name = if at == 0 {
            name.chars()
                .enumerate()
                .map(|(i, c)| {
                    if i % 2 == 0 {
                        c.to_ascii_lowercase()
                    } else {
                        c
                    }
                })
                .collect()
        } else {
            (*name).to_owned()
        };
        base.push((name, "from-a-session".to_owned()));
    }
    for name in AUTHOR_SETTINGS {
        base.push(((*name).to_owned(), "the-author's".to_owned()));
    }
    base.push((AGENT_VIEW_VAR.to_owned(), "1".to_owned()));
    base.push((UNLISTED.to_owned(), "unlisted".to_owned()));
    base
}

/// The calls the fake logged in `log`, waiting up to 15 s for `count` of
/// them: a console kind is not waited on, so its line comes later.
fn logged(log: &Path, count: usize) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let calls = common::fake_claude_calls(log);
        if calls.len() >= count || Instant::now() > deadline {
            return calls;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn env_names(call: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = call["env_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

/// The names `vector`'s rule keeps of `base`, then those it sets, sorted.
fn expected_names(vector: &ClaudeVector, base: &[(String, String)]) -> Vec<String> {
    let mut names: Vec<String> = base
        .iter()
        .map(|(name, _)| name)
        .filter(|name| match vector.env_rule() {
            EnvRule::Session => !removed_from_sessions(name),
            EnvRule::Contained => passed_to_contained(name),
        })
        .chain(vector.env_set().iter().map(|(name, _)| name))
        .cloned()
        .collect();
    names.sort();
    names
}

/// A console kind opens a console on Windows only.
fn runs_here(kind: VectorKind, setup: &Setup) -> bool {
    cfg!(windows) || setup.vector(kind).console() == Console::None
}

/// Runs `kind` with `base` and returns the fake's logged call.
fn run_kind(setup: &Setup, kind: VectorKind, base: &[(String, String)]) -> serde_json::Value {
    let fake = setup.fake_for(kind);
    let outcome = execute(&fake, &setup.vector(kind), &BaseEnv::new(base.to_vec()));
    assert!(outcome.is_ok(), "{kind:?}: {outcome:?}");
    let calls = logged(&setup.log(kind.code()), 1);
    assert_eq!(calls.len(), 1, "{kind:?}: {calls:?}");
    calls.into_iter().next().unwrap()
}

/// AD-80, D-128 review F13: no name of `SESSION_ENV_REMOVED` reaches any
/// kind, whatever its case, and neither does agent view off.
#[test]
fn seeded_session_variables_never_reach_the_child() {
    let setup = Setup::new();
    let base = seeded_base();
    for kind in VectorKind::ALL.iter().copied() {
        if !runs_here(kind, &setup) {
            continue;
        }
        let names = env_names(&run_kind(&setup, kind, &base));
        for removed in SESSION_ENV_REMOVED.iter().chain(&[AGENT_VIEW_VAR]) {
            assert!(
                names.iter().all(|n| !n.eq_ignore_ascii_case(removed)),
                "{kind:?}: {removed} reached the child"
            );
        }
        assert_eq!(
            names,
            expected_names(&setup.vector(kind), &base),
            "{kind:?}"
        );
    }
}

/// D-561, review F1: a session kind keeps the author's `OFFICINA_*`
/// settings and every other name of the base, as a session the author
/// starts would.
#[test]
fn session_kinds_keep_the_authors_officina_settings() {
    let setup = Setup::new();
    let base = seeded_base();
    let session_kinds = VectorKind::ALL
        .iter()
        .copied()
        .filter(|k| k.env_rule() == EnvRule::Session && runs_here(*k, &setup));
    for kind in session_kinds {
        let names = env_names(&run_kind(&setup, kind, &base));
        for kept in AUTHOR_SETTINGS.iter().chain(&[UNLISTED, "SYSTEMROOT"]) {
            assert!(names.iter().any(|n| n == kept), "{kind:?}: {kept} dropped");
        }
    }
}

/// D-561: a contained kind receives the allow list's names of the base
/// and what it sets, nothing else: no session variable, no `OFFICINA_*`
/// name, no unlisted name.
#[test]
fn contained_kinds_receive_the_allow_list_only() {
    let setup = Setup::new();
    let base = seeded_base();
    let contained = VectorKind::ALL
        .iter()
        .copied()
        .filter(|k| k.env_rule() == EnvRule::Contained);
    for kind in contained {
        let vector = setup.vector(kind);
        let names = env_names(&run_kind(&setup, kind, &base));
        for name in &names {
            assert!(
                passed_to_contained(name) || vector.env_set().iter().any(|(set, _)| set == name),
                "{kind:?}: {name} reached the child"
            );
        }
        assert!(names.iter().any(|n| n == "SYSTEMROOT"), "{kind:?}");
    }
}

/// AD-79, AD-95: every golden case reaches the fake as written: its
/// argument vector element by element, its working directory and its
/// variable names.
#[test]
fn every_kind_reaches_the_fake_claude_as_its_golden_vector() {
    let setup = Setup::new();
    let base = vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )];
    for (at, case) in spawn_cases::load().cases.iter().enumerate() {
        let kind = case.kind();
        let vector = setup.build(case);
        if !cfg!(windows) && vector.console() == Console::New {
            continue;
        }
        let name = format!("case-{at}");
        let mode = match kind {
            VectorKind::Attach => "interactive",
            VectorKind::TriageCall => "triage",
            VectorKind::WorkOrder => "bg",
            VectorKind::Agents | VectorKind::Version => "ok",
        };
        let fake = setup.fake(&name, serde_json::json!({"mode": mode}));
        let outcome = execute(&fake, &vector, &BaseEnv::new(base.clone()));
        assert!(outcome.is_ok(), "{}: {outcome:?}", case.name);
        let calls = logged(&setup.log(&name), 1);
        let call = calls
            .first()
            .unwrap_or_else(|| panic!("{}: no call", case.name));
        let expected: Vec<String> = serde_json::from_value(case.expect["args"].clone()).unwrap();
        let argv = common::logged_argv(call);
        assert_eq!(argv.len(), expected.len(), "{}", case.name);
        for (got, want) in argv.iter().zip(&expected) {
            assert_eq!(got, want, "{}", case.name);
        }
        let cwd = PathBuf::from(call["cwd"].as_str().unwrap());
        let expected_cwd = match vector.cwd() {
            CwdRule::Inherited => std::env::current_dir().unwrap(),
            CwdRule::BinaryFolder => fake.path().parent().unwrap().to_path_buf(),
            CwdRule::Officina { .. } => setup.triage_workdir(),
            CwdRule::Project { folder } => PathBuf::from(folder),
        };
        assert_eq!(
            std::fs::canonicalize(&cwd).unwrap(),
            std::fs::canonicalize(&expected_cwd).unwrap(),
            "{}",
            case.name
        );
        assert_eq!(
            env_names(call),
            expected_names(&vector, &base),
            "{}",
            case.name
        );
        let config = case.input.get("config_dir").and_then(|v| v.as_str());
        assert_eq!(call["config_dir"].as_str(), config, "{}", case.name);
    }
}

/// D-129, AD-81's binding: the fake in `bg` mode answers a work order as
/// `claude --bg` does, and the session it started is what `claude agents
/// --json` lists next, with the order's name, its project folder and its
/// start time.
#[test]
fn bg_mode_lists_the_new_session_in_agents() {
    let setup = Setup::new();
    let fake = setup.fake("bg", serde_json::json!({"mode": "bg"}));
    let order = setup.vector(VectorKind::WorkOrder);
    let base = BaseEnv::new(seeded_base());
    let before = common::now_ms();
    let outcome = execute(&fake, &order, &base);
    let after = common::now_ms();
    let Ok(SpawnOutcome::Captured(ran)) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(ran.status.success(), "{}", ran.quoted_stderr());
    let stdout = String::from_utf8(ran.stdout).unwrap();
    let printed: Vec<&str> = stdout.lines().next().unwrap().split(" \u{b7} ").collect();
    let [word, id, printed_name] = printed[..] else {
        panic!("{stdout}");
    };
    assert_eq!(word, "backgrounded");
    let name_at = order.args().iter().position(|a| a == "--name").unwrap();
    let name = &order.args()[name_at + 1];
    assert_eq!(printed_name, name);

    let listed = execute(&fake, &setup.vector(VectorKind::Agents), &base);
    let Ok(SpawnOutcome::Captured(listed)) = listed else {
        panic!("{listed:?}");
    };
    let records =
        officina_core::agents::parse_agents(&String::from_utf8(listed.stdout).unwrap()).unwrap();
    let session = records
        .iter()
        .find(|r| r.id.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("{id} not listed: {records:?}"));
    assert_eq!(session.name.as_deref(), Some(name.as_str()));
    assert_eq!(
        session.kind,
        Some(officina_core::agents::SessionKind::Background)
    );
    let CwdRule::Project { folder } = order.cwd() else {
        panic!("{:?}", order.cwd());
    };
    assert_eq!(
        std::fs::canonicalize(session.cwd.as_deref().unwrap()).unwrap(),
        std::fs::canonicalize(folder).unwrap()
    );
    let started = session.started_at_ms.unwrap();
    assert!(
        (before..=after).contains(&started),
        "{before} {started} {after}"
    );
}

/// D-128 review F10: a work order runs in no job, and its deadline ends
/// `claude` alone; what it started (for the real one, the supervisor of
/// every background session) keeps running. The vector is tightened to
/// 2 s; the fake hangs with a grandchild beating.
#[test]
fn a_work_order_deadline_ends_the_child_alone() {
    let setup = Setup::new();
    let heartbeat = setup.root.path().join("heartbeat");
    let fake = setup.fake(
        "tree",
        serde_json::json!({"mode": "tree", "heartbeat": heartbeat}),
    );
    let order = setup
        .vector(VectorKind::WorkOrder)
        .tightened(2_000, usize::MAX);
    let started = Instant::now();
    let outcome = execute(&fake, &order, &BaseEnv::new(seeded_base()));
    let timed_out =
        matches!(&outcome, Err(error) if matches!(error.cause(), SpawnError::Timeout(_)));
    assert!(timed_out, "{outcome:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(common::heartbeat_len(&heartbeat) > 0, "the grandchild ran");
    let running = !common::heartbeat_stopped(&heartbeat);
    common::stop_heartbeat(&heartbeat);
    assert!(running, "the deadline ended what claude started");
}

/// D-97, D-562: a captured kind keeps at most its cap and still drains
/// both streams to their end, so a child writing more is never blocked; a
/// failure's stderr is kept for its message.
#[test]
fn a_captured_kind_is_capped_and_both_streams_drain() {
    let setup = Setup::new();
    std::fs::write(
        setup.root.path().join("agents.json"),
        vec![b' '; AGENTS_STDOUT_CAP + 512 * 1024],
    )
    .unwrap();
    let fake = setup.fake("big", serde_json::json!({"mode": "ok"}));
    let started = Instant::now();
    let outcome = execute(
        &fake,
        &setup.vector(VectorKind::Agents),
        &BaseEnv::new(seeded_base()),
    );
    let Ok(SpawnOutcome::Captured(ran)) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(ran.status.success());
    assert!(ran.stdout_over);
    assert_eq!(ran.stdout.len(), AGENTS_STDOUT_CAP);
    assert!(started.elapsed() < officina_host::spawn::AGENTS_TIMEOUT);

    let failing = setup.fake("failing", serde_json::json!({"mode": "exit1"}));
    let outcome = execute(
        &failing,
        &setup.vector(VectorKind::Agents),
        &BaseEnv::new(seeded_base()),
    );
    let Ok(SpawnOutcome::Captured(ran)) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(ran.quoted_stderr(), "fake failure");
}

/// AD-73, D-128 review F10: a captured kind past its deadline is ended
/// with every process it started (`Agents`' job, terminated with its
/// tree on timeout).
#[cfg(windows)]
#[test]
fn the_deadline_ends_a_captured_kind_and_its_tree() {
    let setup = Setup::new();
    let heartbeat = setup.root.path().join("heartbeat");
    let fake = setup.fake(
        "tree",
        serde_json::json!({"mode": "tree", "heartbeat": heartbeat}),
    );
    let started = Instant::now();
    let outcome = execute(
        &fake,
        &setup.vector(VectorKind::Agents),
        &BaseEnv::new(seeded_base()),
    );
    assert!(
        matches!(outcome, Err(SpawnError::Timeout(_))),
        "{outcome:?}"
    );
    assert!(started.elapsed() < officina_host::spawn::AGENTS_TIMEOUT * 3);
    assert!(
        common::heartbeat_len(&heartbeat) > 0,
        "the grandchild was running"
    );
    let stopped = common::heartbeat_stopped(&heartbeat);
    common::stop_heartbeat(&heartbeat);
    assert!(stopped, "the grandchild outlived the deadline");
}

/// The console kinds (`Attach` here), with the console they start in.
#[cfg(windows)]
fn console_kinds() -> impl Iterator<Item = VectorKind> {
    let setup = Setup::new();
    VectorKind::ALL
        .iter()
        .copied()
        .filter(move |k| setup.vector(*k).console() == Console::New)
}

/// AD-79, D-128: a console kind is not waited on; it gives back its pid
/// and creation time while it still runs.
#[cfg(windows)]
#[test]
fn a_console_kind_is_not_waited() {
    for kind in console_kinds() {
        let setup = Setup::new();
        let fake = setup.fake("hang", serde_json::json!({"mode": "hang", "hang_ms": 4000}));
        let started = Instant::now();
        let outcome = execute(&fake, &setup.vector(kind), &BaseEnv::new(seeded_base()));
        let elapsed = started.elapsed();
        let Ok(SpawnOutcome::Started { pid, created }) = outcome else {
            panic!("{kind:?}: {outcome:?}");
        };
        assert!(elapsed < Duration::from_secs(2), "{kind:?}: {elapsed:?}");
        assert!(pid > 0 && created.is_some(), "{kind:?}");
        assert_eq!(logged(&setup.log("hang"), 1).len(), 1, "{kind:?} ran");
    }
}

/// Review F11, story 5.2 AC 5: a console kind reads and writes the console
/// it gets, whatever this process's own standard handles are (a test's are
/// pipes or files): the fake in `interactive` mode finds a console on its
/// stdin and its stdout.
#[cfg(windows)]
#[test]
fn a_console_kind_gets_a_console() {
    for kind in console_kinds() {
        let setup = Setup::new();
        let call = run_kind(&setup, kind, &seeded_base());
        assert_eq!(
            call["console"],
            serde_json::json!({"stdin": true, "stdout": true}),
            "{kind:?}"
        );
    }
    // The control: the same fake started on pipes reports no console, so
    // the probe can tell.
    let setup = Setup::new();
    let fake = setup.fake("piped", serde_json::json!({"mode": "interactive"}));
    let out = std::process::Command::new(fake.path())
        .arg("attach")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        logged(&setup.log("piped"), 1)[0]["console"],
        serde_json::json!({"stdin": false, "stdout": false})
    );
}

/// E13: the console path builds its command line itself; each argument
/// reaches the child whole, the answers and prompts of later kinds
/// included (quotes, spaces, a newline, trailing backslashes).
#[cfg(windows)]
#[test]
fn a_console_start_keeps_each_argument_whole() {
    let setup = Setup::new();
    let fake = setup.fake("quoted", serde_json::json!({"mode": "interactive"}));
    let args: Vec<String> = [
        "--resume",
        "",
        "say \"hi\" twice",
        "line one\nline two\tend",
        "C:\\my dir\\",
        "C:\\dir\\",
        "100% & ^ | < > !",
    ]
    .map(str::to_owned)
    .to_vec();
    let env = vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )];
    officina_host::platform::windows::start_in_new_console(
        fake.path(),
        &args,
        None,
        &env,
        officina_host::spawn::CREATE_NEW_CONSOLE,
    )
    .unwrap();
    let calls = logged(&setup.log("quoted"), 1);
    assert_eq!(common::logged_argv(&calls[0]), args);
}

/// AD-79, D-562, review F28: whatever `claude` names, every kind refuses
/// anything but a native executable before spawning: a `.cmd` wrapper
/// around the fake and a `.com` copy of it never run.
#[test]
fn a_non_exe_claude_is_refused_for_every_kind() {
    let setup = Setup::new();
    let fake = setup.fake("refused", serde_json::json!({"mode": "interactive"}));
    let folder = fake.path().parent().unwrap();
    let wrapper = folder.join("claude.cmd");
    std::fs::write(&wrapper, format!("@\"{}\" %*\r\n", fake.path().display())).unwrap();
    let copy = folder.join("claude.com");
    std::fs::copy(fake.path(), &copy).unwrap();
    for refused in [wrapper, copy] {
        let claude = ResolvedClaude::new(refused.clone());
        assert!(!claude.is_native(), "{}", refused.display());
        for kind in VectorKind::ALL.iter().copied() {
            let error = execute(&claude, &setup.vector(kind), &BaseEnv::new(seeded_base()))
                .expect_err("refused");
            assert!(
                matches!(error.cause(), SpawnError::NotNative(_)),
                "{kind:?} {}: {error:?}",
                refused.display()
            );
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(!setup.log("refused").exists(), "the fake was started");
}

/// D-128 review F10, story 5.7 AC 4: `claude --bg` that exits 0 while a
/// process it started (the supervisor) still holds the output pipes is a
/// started launch, with what was read; the process it left runs on. The
/// fake's `bg-hold` mode leaves a heartbeat copy holding them.
#[test]
fn held_pipes_after_exit_0_are_started() {
    let setup = Setup::new();
    let heartbeat = setup.root.path().join("heartbeat");
    let fake = setup.fake(
        "hold",
        serde_json::json!({"mode": "bg-hold", "heartbeat": heartbeat}),
    );
    let started = Instant::now();
    let ran = execute(
        &fake,
        &setup.vector(VectorKind::WorkOrder),
        &BaseEnv::new(seeded_base()),
    );
    let held = matches!(&ran, Ok(SpawnOutcome::Captured(c)) if c.status.success() && c.output_held);
    assert!(held, "{ran:?}");
    assert!(started.elapsed() < Duration::from_secs(15));
    let result = officina_host::orders::launch_result(ran);
    assert_eq!(
        result.outcome,
        officina_core::registry::types::LaunchOutcome::Started
    );
    let printed = result.printed.unwrap_or_default();
    assert!(printed.starts_with("backgrounded"), "{printed:?}");
    let running = !common::heartbeat_stopped(&heartbeat);
    common::stop_heartbeat(&heartbeat);
    assert!(running, "what claude left running was ended");
}

/// D-128 review F10, story 5.7 AC 4: past a work order's deadline the
/// launch is `failed` with `timeout`, and only `claude` is ended: the
/// process it started survives. The vector is tightened to 2 s; the fake
/// in `bg-hold` hangs after starting its holder.
#[test]
fn a_work_order_timeout_kills_only_claude() {
    let setup = Setup::new();
    let heartbeat = setup.root.path().join("heartbeat");
    let fake = setup.fake(
        "hold-hang",
        serde_json::json!({"mode": "bg-hold", "heartbeat": heartbeat, "hang_ms": 30_000}),
    );
    let order = setup
        .vector(VectorKind::WorkOrder)
        .tightened(2_000, usize::MAX);
    let started = Instant::now();
    let ran = execute(&fake, &order, &BaseEnv::new(seeded_base()));
    assert!(started.elapsed() < Duration::from_secs(10), "{ran:?}");
    let result = officina_host::orders::launch_result(ran);
    assert_eq!(
        (result.outcome, result.error.as_deref()),
        (
            officina_core::registry::types::LaunchOutcome::Failed,
            Some("timeout")
        )
    );
    assert!(common::heartbeat_len(&heartbeat) > 0, "the holder ran");
    let running = !common::heartbeat_stopped(&heartbeat);
    common::stop_heartbeat(&heartbeat);
    assert!(running, "the deadline ended what claude started");
}
