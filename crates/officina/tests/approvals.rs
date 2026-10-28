//! `human` actions end to end (story 2.8, FR-8, FR-9, AD-33, D-27 to D-29,
//! D-170): requests, the wait, approvals used once, rejections, the
//! journal, forged requests, and a dry run bound to the tree. The window is
//! played through `authority::decide_approval`, as it decides. Windows only,
//! like `run.rs`: a `native` program is a Windows absolute path (AD-22).

#![cfg(windows)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use common::relay::{Relay, assert_refused, assert_refused_after, stderr_text as stderr};
use common::{git, git_dir_or_skip};
use officina_core::relay::approvals::{
    ApprovalRequest, DEFAULT_WAIT_S, Decision, FileKind, KEPT_OUTPUT_BYTES, file_name,
    kept_output_name, request_id,
};
use officina_core::relay::classes::WAIT_RANGE_S;
use officina_core::relay::journal::{JournalEntry, RefusalReason, RunOutcome};
use officina_host::relay::authority::{AuthorityError, decide_approval, set_relay_stopped};

const FLOOR: &str = r#"schema = 2

[[project]]
name = "lab"
path = "{project}"
actions = ["restart", "missing", "verify", "apply-sensitive"]

[[action]]
name = "restart"
description = "Restart one service"
class = "human"
program = "{tool}"
args = ["restart", "{service}"]

[action.params.service]
values = ["web01", "db01"]

[[action]]
name = "missing"
class = "human"
program = "C:/path/to/officina-no-such-tool.exe"

[[action]]
name = "verify"
class = "free"
program = "{tool}"
args = ["--check", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply-sensitive"
class = "human"
check = "verify"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"
"#;

impl Relay {
    fn new() -> Relay {
        Relay::approved(FLOOR)
    }

    /// A relay whose project is a git work tree with one commit, which
    /// `officina run` lists with the `git` Git for Windows records (D-173);
    /// `None`, with the reason printed, without either (D-23).
    fn in_git(test: &str) -> Option<Relay> {
        let git_dir = git_dir_or_skip(test)?;
        if officina_host::relay::tree::resolve_git().is_none() {
            println!("skipped {test}: Git for Windows records no install folder here");
            return None;
        }
        let mut relay = Relay::new();
        git(&git_dir, &relay.project(), &["init", "-q"]);
        std::fs::write(relay.project().join("site.yml"), "- hosts: lab\n").unwrap();
        git(&git_dir, &relay.project(), &["add", "site.yml"]);
        git(&git_dir, &relay.project(), &["commit", "-q", "-m", "first"]);
        relay.git_dir = Some(git_dir);
        Some(relay)
    }

    fn approvals_dir(&self) -> PathBuf {
        self.env.relay_home().approvals_dir()
    }

    fn spawn(&self, args: &[&str]) -> Child {
        self.command(&self.project(), args).spawn().unwrap()
    }

    /// The ids of the files of `kind`, sorted.
    fn ids(&self, kind: FileKind) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.approvals_dir()) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .strip_suffix(kind.suffix())
                    .map(str::to_owned)
            })
            .collect();
        ids.sort();
        ids
    }

    fn request(&self, id: &str) -> ApprovalRequest {
        let path = self.approvals_dir().join(file_name(id, FileKind::Request));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    /// Waits for a request other than `known` and returns its id.
    fn next_request(&self, known: &[String]) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(id) = self
                .ids(FileKind::Request)
                .into_iter()
                .find(|id| !known.contains(id))
            {
                return id;
            }
            assert!(Instant::now() < deadline, "no request appeared");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn decide(&self, id: &str, approve: bool) {
        decide_approval(&self.env.relay_home(), id, approve).unwrap();
    }

    /// Moves a decision `ago` into the past, as time would.
    fn age_decision(&self, id: &str, kind: FileKind, ago: Duration) {
        let path = self.approvals_dir().join(file_name(id, kind));
        let mut decision: Decision =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        decision.decided_at_ms -= i64::try_from(ago.as_millis()).unwrap();
        std::fs::write(&path, serde_json::to_vec(&decision).unwrap()).unwrap();
    }

    fn refusals(&self) -> Vec<RefusalReason> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::Refused(r) => Some(r.reason),
                _ => None,
            })
            .collect()
    }
}

const RESTART: [&str; 4] = ["restart", "service=web01", "--wait", "1"];

// ---- AC 2: request, reuse, rejection, approval ----

#[test]
fn first_call_creates_a_request() {
    let relay = Relay::new();
    let out = relay.run(&RESTART);
    assert_refused(&relay, &out, RefusalReason::ApprovalPending);
    assert!(
        stderr(&out).contains("approval still pending; run the same command again to keep waiting")
    );
    let ids = relay.ids(FileKind::Request);
    assert_eq!(ids.len(), 1);
    let request = relay.request(&ids[0]);
    assert_eq!(
        ids[0],
        request_id(&request.request_key, request.created_at_ms)
    );
    assert_eq!(
        (request.project.as_str(), request.action.as_str()),
        ("lab", "restart")
    );
    assert_eq!(request.params["service"], "web01");
    assert_eq!(
        request.argv,
        [
            common::fake_tool_path(),
            "restart".to_owned(),
            "web01".to_owned()
        ]
    );
    assert!(relay.launches().is_empty());
}

#[test]
fn second_call_reuses_the_pending_request() {
    let relay = Relay::new();
    relay.run(&RESTART);
    relay.run(&RESTART);
    assert_eq!(relay.ids(FileKind::Request).len(), 1);
    relay.run(&["restart", "service=db01", "--wait", "1"]);
    assert_eq!(
        relay.ids(FileKind::Request).len(),
        2,
        "other parameters, another request"
    );
}

/// Adversarial pass: identical calls at once make one request and one line,
/// not one each.
#[test]
fn concurrent_identical_calls_make_one_request() {
    let relay = Relay::new();
    let children: Vec<Child> = (0..4).map(|_| relay.spawn(&RESTART)).collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert_refused(&relay, &out, RefusalReason::ApprovalPending);
    }
    assert_eq!(relay.ids(FileKind::Request).len(), 1);
    let requested = relay
        .entries()
        .into_iter()
        .filter(|e| matches!(e, JournalEntry::ApprovalRequested(_)))
        .count();
    assert_eq!(requested, 1);
}

#[test]
fn rejection_blocks_retries_for_15_min() {
    let relay = Relay::new();
    relay.run(&RESTART);
    let id = relay.ids(FileKind::Request).remove(0);
    relay.decide(&id, false);
    let out = relay.run(&RESTART);
    assert_refused(&relay, &out, RefusalReason::ApprovalRejected);
    assert!(stderr(&out).contains("rejected in Officina; do not retry"));
    assert_eq!(relay.ids(FileKind::Request).len(), 1, "no new request");

    relay.age_decision(&id, FileKind::Rejected, Duration::from_secs(15 * 60));
    assert_refused(&relay, &relay.run(&RESTART), RefusalReason::ApprovalPending);
    assert_eq!(relay.ids(FileKind::Request).len(), 2, "a new request");
}

#[test]
fn approval_older_than_15_min_is_not_used() {
    let relay = Relay::new();
    relay.run(&RESTART);
    let id = relay.ids(FileKind::Request).remove(0);
    relay.decide(&id, true);
    relay.age_decision(&id, FileKind::Approved, Duration::from_secs(15 * 60));
    assert_refused(&relay, &relay.run(&RESTART), RefusalReason::ApprovalPending);
    assert!(relay.launches().is_empty());
    assert_eq!(relay.ids(FileKind::Approved), [id], "left unused");
    assert_eq!(relay.ids(FileKind::Request).len(), 2);
}

#[test]
fn approval_is_single_use() {
    let relay = Relay::new();
    relay.run(&RESTART);
    let id = relay.ids(FileKind::Request).remove(0);
    relay.decide(&id, true);
    let out = relay.run(&RESTART);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(relay.launches().len(), 1);
    assert_eq!(relay.ids(FileKind::Consumed), std::slice::from_ref(&id));
    assert!(relay.ids(FileKind::Approved).is_empty());
    assert_refused_after(
        &relay,
        &relay.run(&RESTART),
        RefusalReason::ApprovalPending,
        1,
    );
    assert_eq!(relay.launches().len(), 1, "the approval ran once");
    assert_eq!(relay.ids(FileKind::Request).len(), 2);
}

// ---- AC 3: forged requests ----

#[test]
fn forged_request_argv_is_not_shown_or_approvable() {
    let relay = Relay::new();
    relay.run(&RESTART);
    let real = relay.request(&relay.ids(FileKind::Request).remove(0));
    // Another parameter set's key, a harmless command, a fresh id.
    let mut forged = real.clone();
    forged
        .params
        .insert("service".to_owned(), "db01".to_owned());
    forged.argv = vec!["C:/Windows/System32/whoami.exe".to_owned()];
    forged.created_at_ms += 1;
    let id = request_id(&forged.request_key, forged.created_at_ms);
    std::fs::write(
        relay
            .approvals_dir()
            .join(file_name(&id, FileKind::Request)),
        serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();

    relay.env.write_agents("[]");
    let out = relay.env.run(&["board", "--json"], b"");
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let shown = board["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id.as_str())
        .unwrap();
    assert_eq!(shown["valid"], false);
    assert_eq!(shown["argv"], serde_json::json!([]));
    assert!(shown["invalid_reason"].as_str().unwrap().contains("key"));
    assert!(!out.stdout.windows(10).any(|w| w == b"whoami.exe"));
    assert_eq!(board["waiting_count"], 1, "the real request only");

    let refused = decide_approval(&relay.env.relay_home(), &id, true);
    assert!(
        matches!(refused, Err(AuthorityError::Decide(_))),
        "{refused:?}"
    );
    assert!(relay.ids(FileKind::Approved).is_empty());
}

// ---- Story 2.11 AC 11: HTTP requests, rebuilt (D-27) ----

/// [`Relay::new`] whose project also lists `health`, a `human` HTTP action
/// with no credential (a CLI test never reads the Credential Manager); its
/// requests only wait, so nothing is ever sent.
fn http_relay() -> Relay {
    let relay = Relay::new();
    let floor = FLOOR
        .replace(
            "{project}",
            &relay.project().to_string_lossy().replace('\\', "/"),
        )
        .replace(
            "actions = [\"restart\",",
            "actions = [\"health\", \"restart\",",
        )
        + "\n[[action]]\nname = \"health\"\ndescription = \"Read one check\"\nkind = \"http\"\nclass = \"human\"\nmethod = \"GET\"\nurl = \"https://monitoring.example.com/api/{check}?verbose=1\"\n[action.params.check]\nvalues = [\"a b\", \"ready\"]\n";
    relay.env.write_floor(&floor);
    relay.env.approve();
    relay
}

fn board_approvals(relay: &Relay) -> (Vec<serde_json::Value>, Vec<u8>) {
    relay.env.write_agents("[]");
    let out = relay.env.run(&["board", "--json"], b"");
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    (board["approvals"].as_array().unwrap().clone(), out.stdout)
}

#[test]
fn http_request_shows_rebuilt_method_and_url() {
    let relay = http_relay();
    assert_refused(
        &relay,
        &relay.run(&["health", "check=a b", "--wait", "1"]),
        RefusalReason::ApprovalPending,
    );
    let id = relay.ids(FileKind::Request).remove(0);
    let request = relay.request(&id);
    assert!(request.argv.is_empty());
    let expected = serde_json::json!({
        "method": "GET",
        "url": "https://monitoring.example.com/api/a%20b?verbose=1",
        "credential": null,
    });
    assert_eq!(serde_json::to_value(&request.http).unwrap(), expected);

    let (approvals, _) = board_approvals(&relay);
    let shown = approvals.iter().find(|a| a["id"] == id.as_str()).unwrap();
    assert_eq!(shown["valid"], true, "{shown}");
    assert_eq!(shown["http"], expected);
    assert_eq!(shown["argv"], serde_json::json!([]));
    assert_eq!(shown["description"], "Read one check");
}

#[test]
fn forged_http_request_is_not_approvable() {
    let relay = http_relay();
    relay.run(&["health", "check=ready", "--wait", "1"]);
    let real = relay.request(&relay.ids(FileKind::Request).remove(0));
    // Same key, same parameters: only the stored URL is forged.
    let mut forged = real.clone();
    let mut http = serde_json::to_value(&forged.http).unwrap();
    http["url"] = serde_json::json!("https://attacker.example.com/steal");
    forged.http = serde_json::from_value(http).unwrap();
    forged.created_at_ms += 1;
    let id = request_id(&forged.request_key, forged.created_at_ms);
    std::fs::write(
        relay
            .approvals_dir()
            .join(file_name(&id, FileKind::Request)),
        serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();

    let (approvals, stdout) = board_approvals(&relay);
    let shown = approvals.iter().find(|a| a["id"] == id.as_str()).unwrap();
    assert_eq!(shown["valid"], false);
    assert!(shown["http"].is_null(), "{shown}");
    assert!(
        shown["invalid_reason"]
            .as_str()
            .unwrap()
            .contains("differs")
    );
    assert!(!stdout.windows(8).any(|w| w == b"attacker"));
    assert!(matches!(
        decide_approval(&relay.env.relay_home(), &id, true),
        Err(AuthorityError::Decide(_))
    ));
}

// ---- AC 4, AC 5: the wait ----

#[test]
fn wait_out_of_range_is_usage() {
    let relay = Relay::new();
    let past = (WAIT_RANGE_S.end() + 1).to_string();
    let below = (WAIT_RANGE_S.start() - 1).to_string();
    for wait in [past.as_str(), below.as_str(), "x", "1.5"] {
        let out = relay.run(&["restart", "service=web01", "--wait", wait]);
        assert_eq!(out.status.code(), Some(64), "{wait}");
        assert!(stderr(&out).contains("--wait"), "{wait}");
    }
    assert!(relay.ids(FileKind::Request).is_empty());
    assert!(DEFAULT_WAIT_S > 1 && WAIT_RANGE_S.contains(&DEFAULT_WAIT_S));
}

#[test]
fn wait_times_out_with_the_retry_message() {
    let relay = Relay::new();
    let started = Instant::now();
    let out = relay.run(&["restart", "service=web01", "--wait", "2"]);
    let took = started.elapsed();
    assert_refused(&relay, &out, RefusalReason::ApprovalPending);
    assert!(stderr(&out).contains("run the same command again to keep waiting"));
    assert!(took >= Duration::from_secs(2), "{took:?}");
    assert!(took < Duration::from_secs(6), "{took:?}");
}

#[test]
fn stop_during_a_wait_refuses() {
    let relay = Relay::new();
    let child = relay.spawn(&["restart", "service=web01", "--wait", "30"]);
    relay.next_request(&[]);
    let stopped_at = Instant::now();
    assert!(set_relay_stopped(&relay.env.relay_home(), true).unwrap());
    let out = child.wait_with_output().unwrap();
    assert!(stopped_at.elapsed() < Duration::from_secs(2));
    assert_refused(&relay, &out, RefusalReason::Stopped);
    assert!(relay.launches().is_empty());
}

#[test]
fn approve_while_waiting_runs_once_under_2_s() {
    let relay = Relay::new();
    let child = relay.spawn(&["restart", "service=web01", "--wait", "30"]);
    let id = relay.next_request(&[]);
    let approved_at = Instant::now();
    relay.decide(&id, true);
    while relay.launches().is_empty() {
        assert!(approved_at.elapsed() < Duration::from_secs(10), "never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
    let took = approved_at.elapsed();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        took < Duration::from_secs(2),
        "{took:?} from the click to the start"
    );
    assert_eq!(relay.launches().len(), 1);

    assert_refused_after(
        &relay,
        &relay.run(&RESTART),
        RefusalReason::ApprovalPending,
        1,
    );
    assert_eq!(
        relay.ids(FileKind::Request).len(),
        2,
        "the next call asks again"
    );
    assert_eq!(relay.launches().len(), 1);
}

// ---- AC 7: a failure after consumption burns the approval ----

#[test]
fn failure_after_consume_burns_the_approval() {
    let relay = Relay::new();
    let missing = ["missing", "--wait", "1"];
    relay.run(&missing);
    let id = relay.ids(FileKind::Request).remove(0);
    relay.decide(&id, true);
    let out = relay.run(&missing);
    assert_eq!(out.status.code(), Some(127), "{}", stderr(&out));
    assert_eq!(relay.ids(FileKind::Consumed), [id], "consumed, then burned");
    let failed = relay
        .entries()
        .into_iter()
        .any(|e| matches!(e, JournalEntry::RunFinished(f) if f.outcome == RunOutcome::SpawnFailed));
    assert!(failed);
    assert_refused(&relay, &relay.run(&missing), RefusalReason::ApprovalPending);
    assert_eq!(relay.ids(FileKind::Request).len(), 2, "a new request");
}

// ---- AC 9: the journal ----

#[test]
fn every_request_and_decision_leaves_exactly_one_line() {
    let relay = Relay::new();
    relay.run(&RESTART);
    relay.run(&RESTART);
    let first = relay.ids(FileKind::Request).remove(0);
    relay.decide(&first, true);
    assert_eq!(relay.run(&RESTART).status.code(), Some(0));
    relay.run(&RESTART);
    let second = relay
        .ids(FileKind::Request)
        .into_iter()
        .find(|id| *id != first)
        .unwrap();
    relay.decide(&second, false);

    let mut requested = Vec::new();
    let mut given = Vec::new();
    let mut rejected = Vec::new();
    let mut run_with = Vec::new();
    for entry in relay.entries() {
        match entry {
            JournalEntry::ApprovalRequested(e) => requested.push(e),
            JournalEntry::ApprovalGiven(e) => given.push(e),
            JournalEntry::ApprovalRejected(e) => rejected.push(e),
            JournalEntry::RunStarted(s) => run_with.push(s.approval_id),
            _ => {}
        }
    }
    let ids = |events: &[officina_core::relay::journal::ApprovalEvent]| -> Vec<String> {
        events.iter().map(|e| e.approval_id.clone()).collect()
    };
    assert_eq!(ids(&requested), [first.clone(), second.clone()]);
    assert_eq!(ids(&given), std::slice::from_ref(&first));
    assert_eq!(ids(&rejected), std::slice::from_ref(&second));
    assert_eq!(run_with, [Some(first.clone())]);
    assert_eq!(
        requested[0].request_key,
        relay.request(&first).request_key,
        "each line carries the request key"
    );
    assert_eq!(given[0].request_key, requested[0].request_key);
    // Story 2.13 AC 5 (D-172): every line names its project and action; a
    // request says whether the action has a dry run.
    for event in requested.iter().chain(&given).chain(&rejected) {
        assert_eq!(
            (event.project.as_deref(), event.action.as_deref()),
            (Some("lab"), Some("restart"))
        );
    }
    assert!(requested.iter().all(|e| e.dry_run == Some(false)));
    assert!(given.iter().chain(&rejected).all(|e| e.dry_run.is_none()));
}

/// Story 2.13 AC 5: a request of an action with a dry run says so.
#[test]
fn a_request_with_a_dry_run_says_so() {
    let Some(relay) = Relay::in_git("a_request_with_a_dry_run_says_so") else {
        return;
    };
    dry_run(&relay, "lab-web");
    relay.run(&["apply-sensitive", "group=lab-web", "--wait", "1"]);
    let requested: Vec<_> = relay
        .entries()
        .into_iter()
        .filter_map(|e| match e {
            JournalEntry::ApprovalRequested(e) => Some(e),
            _ => None,
        })
        .collect();
    assert_eq!(requested.len(), 1);
    assert_eq!(requested[0].dry_run, Some(true));
    assert_eq!(requested[0].action.as_deref(), Some("apply-sensitive"));
}

// ---- AC 19 to 21: a dry run (D-170) ----

#[test]
fn human_with_check_needs_the_check_first() {
    let relay = Relay::new();
    let out = relay.run(&["apply-sensitive", "group=lab-web", "--wait", "1"]);
    assert_refused(&relay, &out, RefusalReason::CheckRequired);
    assert!(
        stderr(&out).contains("run `verify` first"),
        "{}",
        stderr(&out)
    );
    assert!(relay.ids(FileKind::Request).is_empty(), "no request");
}

/// Runs the dry run and returns its run id.
fn dry_run(relay: &Relay, group: &str) -> String {
    let out = relay.run(&["verify", &format!("group={group}")]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    relay
        .entries()
        .into_iter()
        .filter_map(|e| match e {
            JournalEntry::RunStarted(s) if s.action == "verify" => Some(s.run_id),
            _ => None,
        })
        .next_back()
        .unwrap()
}

#[test]
fn request_records_the_check_run_and_tree() {
    let Some(relay) = Relay::in_git("request_records_the_check_run_and_tree") else {
        return;
    };
    let run_id = dry_run(&relay, "lab-web");
    let tree = relay
        .entries()
        .into_iter()
        .find_map(|e| match e {
            JournalEntry::RunStarted(s) if s.run_id == run_id => s.tree,
            _ => None,
        })
        .expect("the check recorded its tree");
    relay.run(&["apply-sensitive", "group=lab-web", "--wait", "1"]);
    let request = relay.request(&relay.ids(FileKind::Request).remove(0));
    assert_eq!(request.check_run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(request.tree, Some(tree));
}

#[test]
fn a_new_check_gives_a_new_request() {
    let Some(relay) = Relay::in_git("a_new_check_gives_a_new_request") else {
        return;
    };
    let apply = ["apply-sensitive", "group=lab-web", "--wait", "1"];
    dry_run(&relay, "lab-web");
    relay.run(&apply);
    relay.run(&apply);
    assert_eq!(relay.ids(FileKind::Request).len(), 1, "the same dry run");
    dry_run(&relay, "lab-web");
    relay.run(&apply);
    let requests = relay.ids(FileKind::Request);
    assert_eq!(requests.len(), 2, "a later dry run, a new request");
    let keys: Vec<String> = requests
        .iter()
        .map(|id| relay.request(id).request_key)
        .collect();
    assert_ne!(keys[0], keys[1]);
}

#[test]
fn check_output_is_kept_and_capped() {
    let relay = Relay::new();
    let kept = |run_id: &str| {
        std::fs::read(
            relay
                .env
                .relay_home()
                .outputs_dir()
                .join(kept_output_name(run_id)),
        )
        .unwrap()
    };
    let mut small = relay.command(&relay.project(), &["verify", "group=lab-web"]);
    small.env("FAKE_TOOL_MODE", "bytes:10");
    assert_eq!(common::run_with(small, b"").status.code(), Some(0));
    let run_id = dry_run_id(&relay);
    let mut expected = common::fake_output::stream(10, common::fake_output::stdout_byte);
    expected.extend(common::fake_output::stream(
        10,
        common::fake_output::stderr_byte,
    ));
    assert_eq!(kept(&run_id), expected, "stdout, then stderr");

    let big = KEPT_OUTPUT_BYTES as u64;
    let mut large = relay.command(&relay.project(), &["verify", "group=lab-web"]);
    large.env("FAKE_TOOL_MODE", format!("bytes:{big}"));
    assert_eq!(common::run_with(large, b"").status.code(), Some(0));
    let text = kept(&dry_run_id(&relay));
    let marker = format!("[officina: output cut at 256 KiB, {big} bytes dropped]\n");
    assert!(
        text.ends_with(marker.as_bytes()),
        "{:?}",
        &text[text.len() - 80..]
    );
    assert!(text.len() <= KEPT_OUTPUT_BYTES + marker.len() + 1);
    assert!(
        !relay
            .env
            .relay_home()
            .outputs_dir()
            .read_dir()
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".tmp")),
        "written under a temporary name, then renamed"
    );

    let probe_run = relay.run(&["restart", "service=web01", "--wait", "1"]);
    assert_eq!(probe_run.status.code(), Some(126));
    assert_eq!(
        relay
            .env
            .relay_home()
            .outputs_dir()
            .read_dir()
            .unwrap()
            .count(),
        2,
        "only check runs keep their output"
    );
}

/// The run id of the last `verify` run.
fn dry_run_id(relay: &Relay) -> String {
    relay
        .entries()
        .into_iter()
        .filter_map(|e| match e {
            JournalEntry::RunStarted(s) if s.action == "verify" => Some(s.run_id),
            _ => None,
        })
        .next_back()
        .unwrap()
}

#[test]
fn an_approval_is_not_consumed_after_a_tree_change() {
    let Some(relay) = Relay::in_git("an_approval_is_not_consumed_after_a_tree_change") else {
        return;
    };
    dry_run(&relay, "lab-web");
    let launches = relay.launches().len();
    let child = relay.spawn(&["apply-sensitive", "group=lab-web", "--wait", "30"]);
    let id = relay.next_request(&[]);
    std::fs::write(relay.project().join("site.yml"), "- hosts: all\n").unwrap();
    relay.decide(&id, true);
    let out = child.wait_with_output().unwrap();
    assert_refused_after(&relay, &out, RefusalReason::CheckNotSatisfied, launches);
    assert_eq!(relay.launches().len(), launches, "nothing ran");
    assert_eq!(
        relay.ids(FileKind::Approved),
        [id],
        "the approval is left unconsumed"
    );
    assert!(relay.ids(FileKind::Consumed).is_empty());
    assert_eq!(
        relay.refusals().last(),
        Some(&RefusalReason::CheckNotSatisfied)
    );
    assert!(Path::new(&relay.approvals_dir()).is_dir());
}

/// AC 5 of story 2.14 (D-173): a `human` action that names no check is
/// bound to its project's files all the same: the request records them, and
/// the approved run starts on them.
#[test]
fn a_human_action_without_check_is_bound_to_its_tree() {
    let Some(relay) = Relay::in_git("a_human_action_without_check_is_bound_to_its_tree") else {
        return;
    };
    let child = relay.spawn(&["restart", "service=web01", "--wait", "30"]);
    let id = relay.next_request(&[]);
    let request = relay.request(&id);
    let bound = request.tree.clone().expect("the request binds the tree");
    assert_eq!(request.check_run_id, None, "no dry run");
    assert_eq!(
        Some(&bound),
        officina_host::relay::tree::fingerprint(&relay.project())
            .unwrap()
            .as_ref()
    );
    relay.decide(&id, true);
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let started = relay
        .entries()
        .into_iter()
        .find_map(|e| match e {
            JournalEntry::RunStarted(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        started.tree,
        Some(bound),
        "it ran on the files it was asked for"
    );
    assert_eq!(started.approval_id.as_deref(), Some(id.as_str()));

    let outside = Relay::new();
    outside.run(&RESTART);
    let request = outside.request(&outside.ids(FileKind::Request).remove(0));
    assert_eq!(request.tree, None, "outside git, nothing to bind");
}

/// AC 5 of story 2.14, the audits' second chain: the session edits the file
/// a `human` action runs while its call waits; the author approves what the
/// card showed, and the run refuses `tree_changed`, the approval left
/// unconsumed for the files it was given for.
#[test]
fn an_edit_while_waiting_refuses_tree_changed() {
    let Some(relay) = Relay::in_git("an_edit_while_waiting_refuses_tree_changed") else {
        return;
    };
    let child = relay.spawn(&["restart", "service=web01", "--wait", "30"]);
    let id = relay.next_request(&[]);
    std::fs::write(relay.project().join("site.yml"), "- hosts: everything\n").unwrap();
    relay.decide(&id, true);
    let out = child.wait_with_output().unwrap();
    assert_refused(&relay, &out, RefusalReason::TreeChanged);
    assert!(relay.launches().is_empty(), "nothing ran");
    assert_eq!(relay.ids(FileKind::Approved), [id], "left unconsumed");
    assert!(relay.ids(FileKind::Consumed).is_empty());
    assert_eq!(relay.refusals().last(), Some(&RefusalReason::TreeChanged));

    // Put back, the files are the ones the approval was given for again.
    std::fs::write(relay.project().join("site.yml"), "- hosts: lab\n").unwrap();
    let out = relay.run(&["restart", "service=web01", "--wait", "1"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(relay.launches().len(), 1);
}
