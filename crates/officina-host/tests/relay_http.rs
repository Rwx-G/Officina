//! Credentials of HTTP actions at host level (story 2.11, AD-36, D-29,
//! D-37): the fake store stands for the Credential Manager and a stub server
//! on 127.0.0.1 for the endpoint. A credential needs `https`, which the stub
//! does not speak: the flows below read the secret, try the TLS handshake
//! and fail it, which is enough to prove when the secret is read and where
//! it goes; that the header reaches the wire is proved by sending a
//! hand-built plan over plain `http` to the stub, bypassing the catalogue's
//! rule on purpose.

#[path = "../../../tests/support/fake_store.rs"]
mod fake_store;
#[path = "../../../tests/support/stub_server.rs"]
mod stub_server;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fake_store::{FakeStore, MARKER_SECRET};
use officina_core::relay::approvals::FileKind;
use officina_core::relay::http::{CredentialBinding, HttpMethod, HttpPlan};
use officina_core::relay::journal::{JournalEntry, RefusalReason, RunOutcome};
use officina_host::files::now_ms;
use officina_host::relay::approvals::read_files;
use officina_host::relay::authority::{approve_catalogue, decide_approval};
use officina_host::relay::credential::Secret;
use officina_host::relay::home::{RelayHome, load_relay_floor};
use officina_host::relay::http::{HttpOutcome, send};
use officina_host::relay::journal::read_recent;
use officina_host::relay::run::{Call, EXIT_REFUSED, run_with};
use stub_server::{Reply, Stub};

/// A relay home with an approved floor whose project lists `health`, an
/// HTTP action sending the credential `monitoring` to the stub's address
/// over `https`, of class `class`.
struct Relay {
    root: tempfile::TempDir,
    home: RelayHome,
}

impl Relay {
    fn new(stub: &Stub, class: &str) -> Relay {
        let root = tempfile::tempdir().unwrap();
        let home = RelayHome::new(root.path().join("officina-home"), None);
        let relay = Relay { root, home };
        std::fs::create_dir_all(relay.project()).unwrap();
        let floor = format!(
            "schema = 2\n[[project]]\nname = \"monitoring\"\npath = \"{}\"\nactions = [\"health\"]\n[[action]]\nname = \"health\"\nkind = \"http\"\nclass = \"{class}\"\nmethod = \"GET\"\nurl = \"https://{}/api/health\"\ncredential = \"monitoring\"\ntimeout_s = 5\n[[credential]]\nname = \"monitoring\"\nhosts = [\"127.0.0.1\"]\nheader = \"Authorization\"\nscheme = \"Bearer\"\n",
            relay.project().to_string_lossy().replace('\\', "/"),
            stub.addr,
        );
        std::fs::create_dir_all(relay.home.officina_home()).unwrap();
        std::fs::write(relay.home.default_floor(), floor).unwrap();
        let loaded = load_relay_floor(&relay.home.default_floor()).unwrap();
        assert!(
            loaded.catalogue.issues.is_empty(),
            "{:#?}",
            loaded.catalogue.issues
        );
        approve_catalogue(&relay.home, &loaded.hash).unwrap();
        relay
    }

    fn project(&self) -> PathBuf {
        self.root.path().join("work").join("monitoring")
    }

    /// `officina run health` with `store`, the response captured.
    fn run(&self, store: &FakeStore, wait: Duration) -> (i32, Vec<u8>) {
        let params = BTreeMap::new();
        let mut out = Vec::new();
        let code = run_with(
            &self.home,
            &self.project().to_string_lossy(),
            &Call {
                action: "health",
                params: &params,
                session_id: None,
                wait,
            },
            store,
            &mut out,
        );
        (code, out)
    }

    fn entries(&self) -> Vec<JournalEntry> {
        let (lines, corrupt) = read_recent(&self.home, now_ms());
        assert_eq!(corrupt, 0);
        lines.into_iter().map(|l| l.entry).collect()
    }

    fn last_refusal(&self) -> Option<RefusalReason> {
        self.entries().into_iter().rev().find_map(|e| match e {
            JournalEntry::Refused(r) => Some(r.reason),
            _ => None,
        })
    }

    /// Approves the one pending request, playing the window.
    fn approve_pending(&self) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let files = read_files(&self.home);
            if let Some(id) = files.requests.keys().find(|id| !files.decided(id)) {
                decide_approval(&self.home, id, true).unwrap();
                return id.clone();
            }
            assert!(Instant::now() < deadline, "no request appeared");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn approval_state(&self, id: &str) -> (bool, bool) {
        let dir = self.home.approvals_dir();
        let exists = |kind| {
            dir.join(officina_core::relay::approvals::file_name(id, kind))
                .is_file()
        };
        (exists(FileKind::Approved), exists(FileKind::Consumed))
    }
}

/// Every file under `dir`, with its bytes.
fn files_under(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push((path.clone(), std::fs::read(&path).unwrap_or_default()));
        }
    }
}

fn holds(bytes: &[u8], needle: &str) -> bool {
    bytes
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn bound_plan(stub: &Stub) -> HttpPlan {
    HttpPlan {
        method: HttpMethod::Get,
        url: stub.url("/api/health"),
        host: "127.0.0.1".to_owned(),
        credential: Some(CredentialBinding {
            name: "monitoring".to_owned(),
            header: "Authorization".to_owned(),
            scheme: "Bearer".to_owned(),
        }),
        timeout_s: 5,
        max_body_bytes: 1024,
    }
}

#[test]
fn credential_header_reaches_the_bound_host() {
    let stub = Stub::start(vec![Reply::ok("up")]);
    let mut out = Vec::new();
    let outcome = send(
        &bound_plan(&stub),
        Some(&Secret::new(MARKER_SECRET.to_owned())),
        &mut out,
    );
    assert!(
        matches!(outcome, HttpOutcome::Response { status: 200, .. }),
        "{outcome:?}"
    );
    let requests = stub.requests();
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {MARKER_SECRET}").as_str())
    );
    assert_eq!(
        requests[0]
            .headers
            .iter()
            .filter(|(_, v)| v.contains(MARKER_SECRET))
            .count(),
        1,
        "the secret goes in one header only"
    );
    assert!(!holds(&out, MARKER_SECRET));

    let mut unbound = bound_plan(&stub);
    unbound.credential = None;
    send(
        &unbound,
        Some(&Secret::new(MARKER_SECRET.to_owned())),
        &mut Vec::new(),
    );
    assert_eq!(stub.requests()[1].header("authorization"), None);
}

#[test]
fn missing_credential_refuses_before_any_connection() {
    let stub = Stub::start(vec![Reply::ok("up")]);
    let relay = Relay::new(&stub, "free");
    let store = FakeStore::empty();
    let (code, out) = relay.run(&store, Duration::from_secs(1));
    assert_eq!(code, EXIT_REFUSED);
    assert!(out.is_empty());
    assert_eq!(store.reads(), 1);
    assert_eq!(stub.connections(), 0, "a connection was made");
    assert_eq!(relay.last_refusal(), Some(RefusalReason::CredentialMissing));
    assert!(
        !relay
            .entries()
            .iter()
            .any(|e| matches!(e, JournalEntry::RunStarted(_))),
        "nothing started"
    );
}

#[test]
fn unreadable_credential_refuses_before_any_connection() {
    let stub = Stub::start(vec![Reply::ok("up")]);
    let relay = Relay::new(&stub, "free");
    let (code, _) = relay.run(&FakeStore::unreadable("monitoring"), Duration::from_secs(1));
    assert_eq!(code, EXIT_REFUSED);
    assert_eq!(stub.connections(), 0);
    assert_eq!(
        relay.last_refusal(),
        Some(RefusalReason::CredentialUnreadable)
    );
}

#[test]
fn missing_credential_does_not_burn_the_approval() {
    let stub = Stub::start(vec![Reply::ok("up")]);
    let relay = Relay::new(&stub, "human");
    let empty = FakeStore::empty();
    let (code, id) = std::thread::scope(|scope| {
        let call = scope.spawn(|| relay.run(&empty, Duration::from_secs(10)).0);
        let id = relay.approve_pending();
        (call.join().unwrap(), id)
    });
    assert_eq!(code, EXIT_REFUSED);
    assert_eq!(relay.last_refusal(), Some(RefusalReason::CredentialMissing));
    assert_eq!(
        relay.approval_state(&id),
        (true, false),
        "the approval stays unused"
    );
    assert_eq!(stub.connections(), 0);

    // With the credential stored, the same call uses that approval at once.
    let stored = FakeStore::with("monitoring", MARKER_SECRET);
    let (code, _) = relay.run(&stored, Duration::from_secs(1));
    assert_eq!(code, 127, "the stub speaks no TLS: no response");
    assert_eq!(relay.approval_state(&id), (false, true), "used now");
    assert_eq!(stub.tls_attempts(), 1, "the request went out once");
    let started = relay
        .entries()
        .into_iter()
        .find_map(|e| match e {
            JournalEntry::RunStarted(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(started.approval_id.as_deref(), Some(id.as_str()));
    assert_eq!(started.credential.as_deref(), Some("monitoring"));
    let finished = relay.entries().into_iter().find_map(|e| match e {
        JournalEntry::RunFinished(f) => Some(f),
        _ => None,
    });
    assert_eq!(finished.unwrap().outcome, RunOutcome::ConnectFailed);
}

#[test]
fn secret_never_leaves_the_header() {
    let stub = Stub::start(vec![Reply::ok("up")]);
    let relay = Relay::new(&stub, "human");
    let store = FakeStore::with("monitoring", MARKER_SECRET);
    let mut captured: Vec<Vec<u8>> = Vec::new();

    // A human call: request, approval, the secret read, TLS refused.
    let (code, out) = std::thread::scope(|scope| {
        let call = scope.spawn(|| relay.run(&store, Duration::from_secs(10)));
        relay.approve_pending();
        call.join().unwrap()
    });
    assert_eq!(code, 127);
    captured.push(out);
    // The same request on the wire, over plain http to the stub.
    let mut out = Vec::new();
    send(
        &bound_plan(&stub),
        Some(&Secret::new(MARKER_SECRET.to_owned())),
        &mut out,
    );
    captured.push(out);
    assert!(
        stub.requests().iter().any(|r| r
            .header("authorization")
            .is_some_and(|v| v.contains(MARKER_SECRET))),
        "positive control: the header carried it"
    );
    assert!(store.reads() >= 1);

    let view = officina_host::relay::view::read_view(&relay.home, now_ms()).unwrap();
    captured.push(serde_json::to_vec(&view).unwrap());
    let mut files = Vec::new();
    files_under(relay.root.path(), &mut files);
    assert!(
        files
            .iter()
            .any(|(p, _)| p.to_string_lossy().contains("journal")),
        "the scan sees the journal"
    );
    for (path, bytes) in &files {
        assert!(
            !holds(bytes, MARKER_SECRET),
            "{} holds the secret",
            path.display()
        );
    }
    for bytes in &captured {
        assert!(!holds(bytes, MARKER_SECRET), "an output holds the secret");
    }
}
