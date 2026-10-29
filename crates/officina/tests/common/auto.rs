//! The `auto` review harness of the CLI tests (story 5.13): a relay whose
//! floor holds a check and an `auto` action naming it, approved through the
//! window's authority; real requests made by `officina run` from a session;
//! a busy session on the board whose last prompt carries the fake review's
//! decision token; and the window's collector thread on it, its `claude` a
//! copy of the fake in a review mode (D-86, D-582).

use std::path::PathBuf;
use std::process::Output;
use std::time::{Duration, Instant};

use officina_core::registry::types::{ReviewMode, ReviewOutcome};
use officina_core::relay::auto::{EscalationCause, ReviewDecision};
use officina_host::collector::{Collector, load_floor};
use officina_host::paths::Paths;
use officina_host::registry::review::{NewReview, ReviewRow};
use officina_host::registry::store::{open_reader, open_writer};
use officina_host::spawn::BaseEnv;
use officina_host::watch::{CollectorMsg, WatchHandle, spawn};

use super::relay::Relay;
use super::{Emitted, fake_claude_in, now_ms, run_with, sink};

/// The relay floor: `check`, free, and `apply`, `auto` after it, with
/// `{cap}` approvals an hour and `{review}` standing for an
/// `[auto_review]` table or nothing.
pub const FLOOR: &str = r#"schema = 2
{review}
[[project]]
name = "supervision"
path = "{project}"
actions = ["check", "apply"]

[[action]]
name = "check"
class = "free"
program = "{tool}"
args = ["--check", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
description = "Apply the playbook checked by its dry run"
class = "auto"
check = "check"
auto_max_per_hour = {cap}
program = "{tool}"
args = ["--apply", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"
"#;

/// The session whose requests are reviewed, on the board.
pub const SESSION: &str = "00000000-0000-0000-0000-00000000a0a0";

/// How long a test waits for the window to write what it expects.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// One `auto` review set-up.
pub struct Auto {
    pub relay: Relay,
    /// What the fake review read on stdin, one JSON line per call.
    pub review_log: PathBuf,
}

/// How a set-up differs from the plain one.
#[derive(Debug, Clone)]
pub struct Options {
    /// The fake's mode.
    pub mode: &'static str,
    /// The `auto` action's `auto_max_per_hour`.
    pub cap: u32,
    /// An `[auto_review]` table, or nothing.
    pub review_table: &'static str,
    /// `no_excerpts = true` in the floor.
    pub no_excerpts: bool,
    /// How long `review-hang` sleeps.
    pub hang_ms: u64,
    /// How a triage call is answered meanwhile, when one runs.
    pub triage_mode: Option<&'static str>,
    /// The author's last prompt on the board.
    pub prompt: String,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: "review",
            cap: 6,
            review_table: "",
            no_excerpts: false,
            hang_ms: 60_000,
            triage_mode: None,
            prompt: "Apply the lab playbook. FAKE-REVIEW:escalate".to_owned(),
        }
    }
}

impl Auto {
    pub fn new(options: &Options) -> Auto {
        let floor = FLOOR.replace("{cap}", &options.cap.to_string()).replace(
            "{review}",
            &if options.no_excerpts {
                format!("no_excerpts = true\n{}", options.review_table)
            } else {
                options.review_table.to_owned()
            },
        );
        let relay = Relay::approved(&floor);
        let review_log = relay.env.root.path().join("review.log");
        let auto = Auto { relay, review_log };
        let now = now_ms();
        auto.relay.env.write_agents(
            &serde_json::json!([{
                "pid": 4242, "cwd": auto.relay.project(), "kind": "interactive",
                "startedAt": now - 600_000, "sessionId": SESSION, "status": "busy",
            }])
            .to_string(),
        );
        auto.prompt(&options.prompt, now - 60_000);
        auto
    }

    /// Writes a `UserPromptSubmit` of the session, the author's prompt.
    pub fn prompt(&self, text: &str, ts: i64) {
        self.event(serde_json::json!({
            "v": 1, "ts": ts, "event": "UserPromptSubmit", "session_id": SESSION,
            "cwd": self.relay.project(), "prompt": text,
        }));
    }

    /// Appends one event line to the day's file.
    pub fn event(&self, line: serde_json::Value) {
        use std::io::Write;
        let ts = line["ts"].as_i64().unwrap();
        let day = officina_core::dates::event_file_name(ts);
        let dir = self.relay.env.events_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(day))
            .unwrap();
        writeln!(file, "{line}").unwrap();
    }

    /// `officina run <args>` from the project, from the session.
    pub fn run(&self, args: &[&str], session: Option<&str>) -> Output {
        let mut cmd = self.relay.command(&self.relay.project(), args);
        if let Some(session) = session {
            cmd.env("CLAUDE_CODE_SESSION_ID", session);
        }
        run_with(cmd, b"")
    }

    /// The check of `group`, then the `auto` request of `group` from
    /// `session`, which waits a second and stays pending. Returns the
    /// request's id.
    pub fn request(&self, group: &str, session: Option<&str>) -> String {
        let known = self.request_ids();
        let check = self.run(&["check", &format!("group={group}")], None);
        assert_eq!(check.status.code(), Some(0), "{:?}", check);
        let apply = self.run(
            &["apply", &format!("group={group}"), "--wait", "1"],
            session,
        );
        assert_eq!(apply.status.code(), Some(126), "{:?}", apply);
        self.request_ids()
            .into_iter()
            .find(|id| !known.contains(id))
            .expect("a new request")
    }

    /// The ids of the request files.
    pub fn request_ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.relay.env.relay_home().approvals_dir()) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .strip_suffix(".request.json")
                    .map(str::to_owned)
            })
            .collect();
        ids.sort();
        ids
    }

    pub fn paths(&self) -> Paths {
        Paths {
            officina_home: self.relay.env.officina_home(),
            claude_config_dir: self.relay.env.claude_config(),
            claude_config_override: None,
            floor_file: Some(self.relay.env.floor_file()),
        }
    }

    /// The fake in `options.mode`, in a folder of its own named `name`.
    pub fn fake(&self, options: &Options, name: &str) -> PathBuf {
        fake_claude_in(
            &self.relay.env.root.path().join(name),
            &serde_json::json!({
                "fixture": self.relay.env.fixture(),
                "log": self.relay.env.fake_log(),
                "mode": options.mode,
                "triage_log": self.review_log,
                "hang_ms": options.hang_ms,
                "triage_mode": options.triage_mode,
            }),
        )
    }

    /// The window's collector on this home with `claude`, its review calls
    /// given `timeout` at most.
    pub fn collector(&self, claude: Option<PathBuf>, timeout: Duration) -> Collector {
        let base = vec![(
            "SYSTEMROOT".to_owned(),
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        )];
        Collector::from_parts(
            self.paths(),
            load_floor(Some(&self.relay.env.floor_file())),
            claude,
        )
        .with_base_env(BaseEnv::new(base))
        .with_review_timeout(timeout)
    }

    /// The window's collector thread on this home with the fake of
    /// `options`.
    pub fn window(&self, options: &Options) -> (WatchHandle, Emitted) {
        let fake = self.fake(options, "bin");
        self.window_with(self.collector(Some(fake), Duration::from_secs(10)))
    }

    pub fn window_with(&self, collector: Collector) -> (WatchHandle, Emitted) {
        let (sink, emitted) = sink(|_| {});
        (spawn(collector, sink).unwrap(), emitted)
    }

    /// The review rows, newest first.
    pub fn rows(&self) -> Vec<ReviewRow> {
        open_reader(&self.relay.env.officina_home(), &[])
            .map(|reader| reader.review_rows(0).unwrap_or_default())
            .unwrap_or_default()
    }

    /// The row of `approval_id`, once the window wrote it.
    pub fn row_of(&self, approval_id: &str) -> ReviewRow {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(row) = self
                .rows()
                .into_iter()
                .find(|r| r.approval_id == approval_id)
            {
                return row;
            }
            assert!(Instant::now() < deadline, "no row for {approval_id}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The review calls the fake answered: one JSON line each, its argv,
    /// environment and stdin.
    pub fn calls(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.review_log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// Seeds review rows before the window starts, as earlier windows wrote
    /// them: `count` rows of `apply` requested `ago` ms before now, with a
    /// call and `decision`.
    pub fn seed_rows(&self, count: usize, ago: i64, decision: Option<ReviewDecision>) {
        use officina_host::registry::store::WriterOpen;
        let now = now_ms();
        let WriterOpen::Writable { mut writer, .. } =
            open_writer(&self.relay.env.officina_home(), &[], now)
        else {
            panic!("the registry did not open writable");
        };
        for at in 0..count {
            let at = i64::try_from(at).unwrap();
            writer
                .insert_review(&NewReview {
                    approval_id: format!("seeded-{at}"),
                    request_key: "seeded".into(),
                    project: "supervision".into(),
                    action: "apply".into(),
                    requested_at_ms: now - ago - at,
                    mode: ReviewMode::Shadow,
                    outcome: ReviewOutcome::Answered,
                    cause: Some(EscalationCause::AutoApprovalOff),
                    decision,
                    reason: None,
                    rule_baseline: false,
                    call_started_at_ms: Some(now - ago - at),
                    duration_ms: Some(1),
                    input_tokens: None,
                    output_tokens: None,
                    cost_microusd: None,
                })
                .unwrap();
        }
    }
}

/// Stops the collector thread.
pub fn stop(handle: WatchHandle) {
    handle.sender.send(CollectorMsg::Shutdown).unwrap();
    handle.thread.join().unwrap();
}
