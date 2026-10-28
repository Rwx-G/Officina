//! HTTP actions through `officina run` (story 2.11, AD-36, D-35, D-36),
//! against a stub server on 127.0.0.1 inside the test process; never the
//! network. These actions carry no credential: the CLI never reads the real
//! Credential Manager in a test; credentials are proved at host level with a
//! fake store (`crates/officina-host/tests/relay_http.rs`).

mod common;
#[path = "../../../tests/support/stub_server.rs"]
mod stub_server;

use std::path::PathBuf;
use std::process::Output;
use std::time::{Duration, Instant};

use common::TestEnv;
use officina_core::relay::http::HttpMethod;
use officina_core::relay::journal::{JournalEntry, RunFinished, RunOutcome, RunStarted};
use officina_core::relay::sha256_hex;
use stub_server::{Reply, Stub, closed_port};

/// An approved relay whose project `monitoring` lists the given HTTP
/// actions, each a `(name, table body)` pair.
struct Http {
    env: TestEnv,
}

impl Http {
    fn new(actions: &[(&str, String)]) -> Http {
        let env = TestEnv::new();
        let http = Http { env };
        std::fs::create_dir_all(http.project()).unwrap();
        let names: Vec<String> = actions.iter().map(|(n, _)| format!("\"{n}\"")).collect();
        let mut floor = format!(
            "schema = 2\n[[project]]\nname = \"monitoring\"\npath = \"{}\"\nactions = [{}]\n",
            http.project().to_string_lossy().replace('\\', "/"),
            names.join(", ")
        );
        for (name, body) in actions {
            floor.push_str(&format!(
                "\n[[action]]\nname = \"{name}\"\nkind = \"http\"\nclass = \"free\"\n{body}"
            ));
        }
        http.env.write_floor(&floor);
        http.env.approve();
        http
    }

    fn project(&self) -> PathBuf {
        self.env.root.path().join("work").join("monitoring")
    }

    fn run_with(&self, args: &[&str], vars: &[(&str, String)]) -> Output {
        let mut full = vec!["run"];
        full.extend_from_slice(args);
        let mut cmd = self.env.officina(&full);
        cmd.current_dir(self.project());
        for (key, value) in vars {
            cmd.env(key, value);
        }
        common::run_with(cmd, b"")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    fn started(&self) -> Vec<RunStarted> {
        self.env
            .journal_after_approvals()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::RunStarted(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    fn finished(&self) -> Vec<RunFinished> {
        self.env
            .journal_after_approvals()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::RunFinished(f) => Some(f),
                _ => None,
            })
            .collect()
    }
}

fn get(url: &str) -> String {
    format!("method = \"GET\"\nurl = \"{url}\"\n")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn get_prints_status_and_body() {
    let stub = Stub::start(vec![Reply::ok("{\"status\":\"up\"}")]);
    let http = Http::new(&[("health", get(&stub.url("/api/health")))]);
    let out = http.run(&["health"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(stdout(&out), "HTTP 200 OK\n{\"status\":\"up\"}");
    assert!(out.stderr.is_empty(), "{:?}", stderr_lines(&out));
    let requests = stub.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].line, "GET /api/health HTTP/1.1");
    assert!(
        requests[0]
            .header("user-agent")
            .unwrap()
            .starts_with("officina/")
    );
    assert_eq!(requests[0].header("authorization"), None);
}

#[test]
fn head_sends_head_and_reads_no_body() {
    let stub = Stub::start(vec![Reply::ok("never sent")]);
    let http = Http::new(&[(
        "ping",
        format!("method = \"HEAD\"\nurl = \"{}\"\n", stub.url("/")),
    )]);
    let out = http.run(&["ping"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(stdout(&out), "HTTP 200 OK\n");
    assert_eq!(stub.requests()[0].method(), "HEAD");
    let finished = http.finished();
    assert_eq!(finished[0].body_bytes, Some(0));
    assert_eq!(http.started()[0].method, Some(HttpMethod::Head));
}

#[test]
fn redirect_is_not_followed() {
    let elsewhere = Stub::start(vec![Reply::ok("followed")]);
    let stub = Stub::start(vec![Reply::Full {
        status: 302,
        headers: vec![("Location".to_owned(), elsewhere.url("/"))],
        body: Vec::new(),
    }]);
    let http = Http::new(&[("health", get(&stub.url("/old")))]);
    let out = http.run(&["health"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stderr_lines(&out), ["officina: http 302"]);
    assert_eq!(elsewhere.connections(), 0, "the redirect was followed");
    assert_eq!(http.finished()[0].status, Some(302));
}

#[test]
fn proxy_environment_is_ignored() {
    let proxy = Stub::start(vec![Reply::ok("from the proxy")]);
    let stub = Stub::start(vec![Reply::ok("direct")]);
    let http = Http::new(&[("health", get(&stub.url("/")))]);
    let proxy_url = format!("http://{}", proxy.addr);
    let vars: Vec<(&str, String)> = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ]
    .into_iter()
    .map(|key| (key, proxy_url.clone()))
    .collect();
    let out = http.run_with(&["health"], &vars);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(stdout(&out), "HTTP 200 OK\ndirect");
    assert_eq!(proxy.connections(), 0, "the proxy was used");
    assert_eq!(stub.requests().len(), 1);
}

#[test]
fn body_is_capped_and_the_connection_dropped() {
    let stub = Stub::start(vec![Reply::Large { len: 256 << 20 }]);
    let http = Http::new(&[(
        "dump",
        format!("{}max_output_kib = 1\n", get(&stub.url("/dump"))),
    )]);
    let started = Instant::now();
    let out = http.run(&["dump"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert!(started.elapsed() < Duration::from_secs(20));
    let text = stdout(&out);
    let body = text.strip_prefix("HTTP 200 OK\n").unwrap();
    let (kept, marker) = body.split_at(1024);
    assert_eq!(kept, "x".repeat(1024));
    assert_eq!(
        marker,
        "\n[officina: body cut at 1 KiB, connection dropped]\n"
    );
    assert!(
        stub.aborted_within(Duration::from_secs(10)),
        "the connection was kept open"
    );
    let finished = &http.finished()[0];
    assert_eq!(finished.body_bytes, Some(1024));
    assert_eq!(
        finished.body_sha256.as_deref(),
        Some(sha256_hex("x".repeat(1024).as_bytes()).as_str())
    );
    assert!(finished.detail.as_deref().unwrap().contains("cut at 1 KiB"));
}

#[test]
fn timeout_exits_124() {
    let stub = Stub::start(vec![Reply::Hang]);
    let http = Http::new(&[(
        "slow",
        format!("{}timeout_s = 1\n", get(&stub.url("/slow"))),
    )]);
    let started = Instant::now();
    let out = http.run(&["slow"]);
    assert_eq!(out.status.code(), Some(124), "{:?}", stderr_lines(&out));
    assert!(started.elapsed() < Duration::from_secs(10));
    let lines = stderr_lines(&out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("officina: timeout: slow"), "{lines:?}");
    assert_eq!(http.finished()[0].outcome, RunOutcome::Timeout);
}

/// Adversarial pass: a server that keeps the body coming, one byte at a
/// time, still ends at the timeout, since it bounds the whole exchange.
#[test]
fn a_dripping_body_ends_at_the_timeout() {
    let stub = Stub::start(vec![Reply::Drip]);
    let http = Http::new(&[(
        "slow",
        format!("{}timeout_s = 1\n", get(&stub.url("/drip"))),
    )]);
    let started = Instant::now();
    let out = http.run(&["slow"]);
    assert_eq!(out.status.code(), Some(124), "{:?}", stderr_lines(&out));
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(http.finished()[0].outcome, RunOutcome::Timeout);
}

#[test]
fn no_listener_exits_127() {
    let url = format!("http://127.0.0.1:{}/", closed_port());
    let http = Http::new(&[("health", get(&url))]);
    let out = http.run(&["health"]);
    assert_eq!(out.status.code(), Some(127), "{:?}", stderr_lines(&out));
    let lines = stderr_lines(&out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("officina: no response from 127.0.0.1"),
        "{lines:?}"
    );
    let finished = &http.finished()[0];
    assert_eq!(finished.outcome, RunOutcome::ConnectFailed);
    assert!(finished.detail.is_some() && finished.status.is_none());
}

#[test]
fn non_2xx_status_exits_1() {
    for status in [404, 500, 503] {
        let stub = Stub::start(vec![Reply::status(status, "down")]);
        let http = Http::new(&[("health", get(&stub.url("/")))]);
        let out = http.run(&["health"]);
        assert_eq!(out.status.code(), Some(1), "{status}");
        assert_eq!(stderr_lines(&out), [format!("officina: http {status}")]);
        assert!(stdout(&out).starts_with(&format!("HTTP {status} ")));
    }
    let created = Stub::start(vec![Reply::status(204, "")]);
    let http = Http::new(&[("health", get(&created.url("/")))]);
    assert_eq!(http.run(&["health"]).status.code(), Some(0), "any 2xx");
}

#[test]
fn http_run_journals_method_url_and_status() {
    let stub = Stub::start(vec![Reply::ok("up")]);
    let template = stub.url("/api/{check}?host={host}");
    let http = Http::new(&[(
        "health",
        format!(
            "{}[action.params.check]\nvalues = [\"a b/c\"]\n[action.params.host]\npattern = \"[a-z0-9&=]+\"\n",
            get(&template)
        ),
    )]);
    let out = http.run(&["health", "check=a b/c", "host=web01&x=1"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        stub.requests()[0].line,
        "GET /api/a%20b%2Fc?host=web01%26x%3D1 HTTP/1.1"
    );
    let started = &http.started()[0];
    assert!(started.argv.is_empty());
    assert_eq!(started.method, Some(HttpMethod::Get));
    assert_eq!(
        started.url.as_deref(),
        Some(stub.url("/api/a%20b%2Fc?host=web01%26x%3D1").as_str())
    );
    assert_eq!(started.credential, None);
    let finished = &http.finished()[0];
    assert_eq!(finished.run_id, started.run_id);
    assert_eq!(finished.outcome, RunOutcome::Http);
    assert_eq!(finished.status, Some(200));
    assert_eq!(finished.body_bytes, Some(2));
    assert_eq!(
        finished.body_sha256.as_deref(),
        Some(sha256_hex(b"up").as_str())
    );
    assert!(finished.stdout.is_none() && finished.exit_code.is_none());

    let journal = std::fs::read_dir(http.env.relay_home().journal_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect::<String>();
    let start_line = journal
        .lines()
        .find(|l| l.contains("\"run_started\""))
        .unwrap();
    assert!(!start_line.contains("\"argv\""), "{start_line}");
}
