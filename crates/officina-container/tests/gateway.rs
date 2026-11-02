//! The real `officina-gateway`, built natively and run on loopback (story
//! 6.5 AC 6, AD-103, D-234, D-611): no Docker, no internet. A test CONNECT
//! proxy and a TLS-shaped echo server on loopback stand in for the
//! upstream and the host.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::workshop::gateway::tls::{client_hello, client_hello_with};
use officina_core::workshop::gateway::{
    GatewayConfig, GatewayLine, GatewayRefusal, LIMIT_EXIT_CODE, MAX_REQUEST_BYTES, MAX_TUNNELS,
    UpstreamState,
};

const GATEWAY: &str = env!("CARGO_BIN_EXE_officina-gateway");
const RUN: &str = "0123456789abcdef0123456789abcdef";

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A server that echoes every byte it receives, as a TLS host would answer.
fn echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 || stream.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

/// A CONNECT proxy answering `status`; on a 2xx it relays to `echo`. Every
/// request head it read is kept.
struct TestProxy {
    port: u16,
    heads: Arc<Mutex<Vec<String>>>,
}

fn test_proxy(status: u16, echo: u16) -> TestProxy {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let heads = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&heads);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let kept = Arc::clone(&kept);
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(1) => head.push(byte[0]),
                        _ => return,
                    }
                }
                kept.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&head).into_owned());
                let _ = write!(stream, "HTTP/1.1 {status} Test\r\n\r\n");
                if !(200..300).contains(&status) {
                    let _ = stream.shutdown(Shutdown::Both);
                    return;
                }
                let Ok(target) = TcpStream::connect(("127.0.0.1", echo)) else {
                    return;
                };
                let (mut a, mut b) = (stream.try_clone().unwrap(), target.try_clone().unwrap());
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut b, &mut a);
                });
                let (mut c, mut d) = (stream, target);
                let _ = std::io::copy(&mut c, &mut d);
            });
        }
    });
    TestProxy { port, heads }
}

/// The gateway running on loopback, its lines on a channel.
struct Gateway {
    child: Child,
    port: u16,
    lines: Receiver<String>,
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// How many ports [`Gateway::start`] tries: a port `free_port` found free
/// can be taken by a parallel test before the gateway binds it, and the
/// gateway then exits before its `started` line.
const START_ATTEMPTS: u32 = 5;

impl Gateway {
    fn start(config: GatewayConfig) -> Gateway {
        for _ in 1..START_ATTEMPTS {
            if let Some(gateway) = Gateway::try_start(config.clone()) {
                return gateway;
            }
        }
        Gateway::try_start(config).expect("the gateway started on a free port")
    }

    /// The gateway on a fresh port, or `None` when it exited before its
    /// first line.
    fn try_start(mut config: GatewayConfig) -> Option<Gateway> {
        let port = free_port();
        config.listen = format!("127.0.0.1:{port}");
        let mut child = Command::new(GATEWAY)
            .args(config.to_args())
            .env_clear()
            // Winsock needs it on Windows; the container gives none.
            .env(
                "SYSTEMROOT",
                std::env::var_os("SYSTEMROOT").unwrap_or_default(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let gateway = Gateway { child, port, lines };
        match gateway.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(started) => {
                assert!(started.contains("\"started\""), "{started}");
                Some(gateway)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => None,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("the gateway printed nothing"),
        }
    }

    fn next_line(&self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(20))
            .expect("a gateway line")
    }

    fn next(&self) -> GatewayLine {
        let line = self.next_line();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line}: {e}"))
    }

    fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        stream
    }
}

/// The answer's head, up to its blank line, or what came before the close.
fn head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&head).into_owned()
}

fn connect_request(host: &str) -> String {
    format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n")
}

fn config(hosts: &[&str], routes: &[(&str, u16)]) -> GatewayConfig {
    let hosts: Vec<String> = hosts.iter().map(|h| (*h).to_owned()).collect();
    let routes: BTreeMap<String, String> = routes
        .iter()
        .map(|(entry, port)| ((*entry).to_owned(), format!("127.0.0.1:{port}")))
        .collect();
    let mut config = GatewayConfig::new(RUN, &hosts, 3600, routes);
    config.refusal_limit = 1000;
    config
}

fn refused_cause(line: &GatewayLine) -> Option<GatewayRefusal> {
    match line {
        GatewayLine::Refused { cause, .. } => Some(*cause),
        _ => None,
    }
}

#[test]
fn upstream_mode_relays_an_allowed_host() {
    let echo = echo_server();
    let proxy = test_proxy(200, echo);
    let gateway = Gateway::start(config(
        &["ok.example.com"],
        &[("ok.example.com", proxy.port)],
    ));
    let mut client = gateway.connect();
    client
        .write_all(connect_request("ok.example.com").as_bytes())
        .unwrap();
    assert!(head(&mut client).starts_with("HTTP/1.1 200"));
    let hello = client_hello("ok.example.com", false);
    client.write_all(&hello).unwrap();
    client.write_all(b"ping").unwrap();
    let mut back = vec![0u8; hello.len() + 4];
    client.read_exact(&mut back).unwrap();
    assert_eq!(&back[..hello.len()], &hello[..]);
    assert_eq!(&back[hello.len()..], b"ping");
    assert_eq!(
        gateway.next(),
        GatewayLine::Allowed {
            host: "ok.example.com".into(),
            port: 443,
            ech: false
        }
    );
    let heads = proxy.heads.lock().unwrap();
    assert!(heads[0].starts_with("CONNECT ok.example.com:443 HTTP/1.1\r\n"));
    assert!(heads[0].contains("\r\nHost: ok.example.com:443\r\n"));
}

#[test]
fn a_routed_entry_goes_upstream_and_another_goes_direct() {
    let echo = echo_server();
    let proxy = test_proxy(200, echo);
    let gateway = Gateway::start(config(
        &["ok.example.com", "localhost"],
        &[("ok.example.com", proxy.port)],
    ));
    let mut direct = gateway.connect();
    direct
        .write_all(connect_request("localhost").as_bytes())
        .unwrap();
    assert!(head(&mut direct).starts_with("HTTP/1.1 403"));
    assert_eq!(
        refused_cause(&gateway.next()),
        Some(GatewayRefusal::PrivateAddress),
        "the unrouted entry resolved directly"
    );
    assert!(
        proxy.heads.lock().unwrap().is_empty(),
        "the proxy saw nothing"
    );
    let mut routed = gateway.connect();
    routed
        .write_all(connect_request("ok.example.com").as_bytes())
        .unwrap();
    assert!(head(&mut routed).starts_with("HTTP/1.1 200"));
    routed
        .write_all(&client_hello("ok.example.com", false))
        .unwrap();
    assert!(matches!(gateway.next(), GatewayLine::Allowed { .. }));
    assert_eq!(proxy.heads.lock().unwrap().len(), 1);
}

#[test]
fn a_407_upstream_is_refused_with_its_status() {
    let proxy = test_proxy(407, echo_server());
    let gateway = Gateway::start(config(
        &["ok.example.com"],
        &[("ok.example.com", proxy.port)],
    ));
    let mut client = gateway.connect();
    client
        .write_all(connect_request("ok.example.com").as_bytes())
        .unwrap();
    assert!(head(&mut client).starts_with("HTTP/1.1 200"));
    client
        .write_all(&client_hello("ok.example.com", false))
        .unwrap();
    match gateway.next() {
        GatewayLine::Refused { cause, status, .. } => {
            assert_eq!(cause, GatewayRefusal::UpstreamRefused);
            assert_eq!(status, Some(407));
        }
        other => panic!("{other:?}"),
    }
}

/// What makes the gateway refuse with `cause` on a loopback gateway
/// configured by [`refusal_config`], when the test can reach it offline:
/// the request bytes, and the bytes sent after a 200. `None` for the causes
/// another test covers (`busy`, `deadline`, `limit`) and the one only a
/// public resolver gives (`no_ipv4`, story 6.10's battery).
fn stimulus(cause: GatewayRefusal) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    let request = |text: &str| text.as_bytes().to_vec();
    match cause {
        GatewayRefusal::Malformed => Some((request("CONNECT nonsense\r\n\r\n"), None)),
        GatewayRefusal::Oversized => Some((vec![b'a'; MAX_REQUEST_BYTES + 1], None)),
        GatewayRefusal::Method => {
            Some((request("GET http://ok.example.com/ HTTP/1.1\r\n\r\n"), None))
        }
        GatewayRefusal::Port => Some((request("CONNECT ok.example.com:80 HTTP/1.1\r\n\r\n"), None)),
        GatewayRefusal::IpLiteral => Some((request(&connect_request("127.0.0.1")), None)),
        GatewayRefusal::NotAllowed => Some((request(&connect_request("example.org")), None)),
        GatewayRefusal::PrivateAddress => Some((request(&connect_request("localhost")), None)),
        GatewayRefusal::ResolveFailed => Some((request(&connect_request("nothing.invalid")), None)),
        GatewayRefusal::NotTls => Some((
            request(&connect_request("ok.example.com")),
            Some(b"GET / HTTP/1.1\r\n\r\n".to_vec()),
        )),
        GatewayRefusal::SniMissing => Some((
            request(&connect_request("ok.example.com")),
            Some(client_hello_with(&[], false)),
        )),
        GatewayRefusal::SniMismatch => Some((
            request(&connect_request("ok.example.com")),
            Some(client_hello("other.example.com", false)),
        )),
        GatewayRefusal::ConnectFailed => Some((
            request(&connect_request("closed.example.com")),
            Some(client_hello("closed.example.com", false)),
        )),
        GatewayRefusal::UpstreamRefused => Some((
            request(&connect_request("deny.example.com")),
            Some(client_hello("deny.example.com", false)),
        )),
        GatewayRefusal::NoIpv4
        | GatewayRefusal::Busy
        | GatewayRefusal::Deadline
        | GatewayRefusal::Limit => None,
    }
}

fn refusal_gateway() -> Gateway {
    let echo = echo_server();
    let ok = test_proxy(200, echo);
    let deny = test_proxy(403, echo);
    let closed = free_port();
    let gateway = Gateway::start(config(
        &[
            "ok.example.com",
            "deny.example.com",
            "closed.example.com",
            "localhost",
            "nothing.invalid",
        ],
        &[
            ("ok.example.com", ok.port),
            ("deny.example.com", deny.port),
            ("closed.example.com", closed),
        ],
    ));
    // The proxies' threads live with their listeners, which the threads own.
    std::mem::forget(ok);
    std::mem::forget(deny);
    gateway
}

#[test]
fn every_refusal_reachable_offline_is_refused_with_its_cause() {
    let gateway = refusal_gateway();
    for cause in GatewayRefusal::ALL {
        let Some((request, after)) = stimulus(*cause) else {
            continue;
        };
        let mut client = gateway.connect();
        let _ = client.write_all(&request);
        let answer = head(&mut client);
        match after {
            Some(bytes) => {
                assert!(answer.starts_with("HTTP/1.1 200"), "{cause:?}: {answer}");
                let _ = client.write_all(&bytes);
            }
            None => {
                let status = cause.status();
                assert!(
                    answer.starts_with(&format!("HTTP/1.1 {status}")),
                    "{cause:?}: {answer}"
                );
            }
        }
        assert_eq!(refused_cause(&gateway.next()), Some(*cause), "{cause:?}");
    }
}

#[test]
fn a_mismatched_server_name_is_refused() {
    let gateway = refusal_gateway();
    let (request, after) = stimulus(GatewayRefusal::SniMismatch).unwrap();
    let mut client = gateway.connect();
    client.write_all(&request).unwrap();
    assert!(head(&mut client).starts_with("HTTP/1.1 200"));
    client.write_all(&after.unwrap()).unwrap();
    assert_eq!(
        refused_cause(&gateway.next()),
        Some(GatewayRefusal::SniMismatch)
    );
    let mut rest = Vec::new();
    let _ = client.read_to_end(&mut rest);
    assert!(rest.is_empty(), "nothing came back from a host");
}

#[test]
fn a_pipelined_client_hello_is_checked() {
    let echo = echo_server();
    let proxy = test_proxy(200, echo);
    let gateway = Gateway::start(config(
        &["ok.example.com"],
        &[("ok.example.com", proxy.port)],
    ));
    let mut client = gateway.connect();
    let good = client_hello("ok.example.com", false);
    let mut first = connect_request("ok.example.com").into_bytes();
    first.extend(&good);
    client.write_all(&first).unwrap();
    assert!(head(&mut client).starts_with("HTTP/1.1 200"));
    let bad = client_hello("evil.example.com", false);
    client.write_all(&bad).unwrap();
    assert!(
        matches!(gateway.next(), GatewayLine::Allowed { host, .. } if host == "ok.example.com")
    );
    let mut back = vec![0u8; good.len()];
    client.read_exact(&mut back).unwrap();
    assert_eq!(
        back, good,
        "the hello in the request's write is the one checked and forwarded first"
    );
}

#[test]
fn an_oversized_request_is_refused() {
    let gateway = Gateway::start(config(&[], &[]));
    let mut client = gateway.connect();
    let _ = client.write_all(&vec![b'a'; MAX_REQUEST_BYTES + 1]);
    assert!(head(&mut client).starts_with("HTTP/1.1 400"));
    assert_eq!(
        refused_cause(&gateway.next()),
        Some(GatewayRefusal::Oversized)
    );
}

#[test]
fn after_the_deadline_everything_is_refused() {
    let echo = echo_server();
    let proxy = test_proxy(200, echo);
    let mut config = config(&["ok.example.com"], &[("ok.example.com", proxy.port)]);
    config.deadline_s = 1;
    let gateway = Gateway::start(config);
    // A tunnel open before the deadline closes at it.
    let mut open = gateway.connect();
    open.write_all(connect_request("ok.example.com").as_bytes())
        .unwrap();
    assert!(head(&mut open).starts_with("HTTP/1.1 200"));
    open.write_all(&client_hello("ok.example.com", false))
        .unwrap();
    assert!(matches!(gateway.next(), GatewayLine::Allowed { .. }));
    std::thread::sleep(Duration::from_millis(1500));
    let mut late = gateway.connect();
    late.write_all(connect_request("ok.example.com").as_bytes())
        .unwrap();
    assert!(head(&mut late).starts_with("HTTP/1.1 403"));
    assert_eq!(
        refused_cause(&gateway.next()),
        Some(GatewayRefusal::Deadline)
    );
    let start = Instant::now();
    let mut rest = Vec::new();
    let _ = open.read_to_end(&mut rest);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "the open tunnel closed"
    );
}

#[test]
fn the_limit_exits_3() {
    let mut config = config(&[], &[]);
    config.refusal_limit = 3;
    let mut gateway = Gateway::start(config);
    for _ in 0..3 {
        let mut client = gateway.connect();
        client
            .write_all(connect_request("example.org").as_bytes())
            .unwrap();
        let _ = head(&mut client);
    }
    for _ in 0..3 {
        assert_eq!(
            refused_cause(&gateway.next()),
            Some(GatewayRefusal::NotAllowed)
        );
    }
    assert_eq!(gateway.next(), GatewayLine::Limit {});
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = gateway.child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "the gateway did not exit");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(LIMIT_EXIT_CODE));
}

#[test]
fn one_more_than_max_tunnels_is_busy() {
    let gateway = Gateway::start(config(&[], &[]));
    // Each holds its thread, waiting for a request.
    let held: Vec<TcpStream> = (0..MAX_TUNNELS).map(|_| gateway.connect()).collect();
    std::thread::sleep(Duration::from_millis(500));
    let mut extra = gateway.connect();
    assert!(head(&mut extra).starts_with("HTTP/1.1 403"));
    assert_eq!(refused_cause(&gateway.next()), Some(GatewayRefusal::Busy));
    drop(held);
}

#[test]
fn lines_are_one_json_object_each() {
    let gateway = refusal_gateway();
    let mut client = gateway.connect();
    client
        .write_all(connect_request("example.org").as_bytes())
        .unwrap();
    let _ = head(&mut client);
    let line = gateway.next_line();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(value.is_object());
    assert_eq!(value["line"], "refused");
    assert!(!line.contains('\n'));
}

#[test]
fn direct_mode_refuses_localhost_as_private_address() {
    let gateway = Gateway::start(config(&["localhost"], &[]));
    let mut client = gateway.connect();
    client
        .write_all(connect_request("localhost").as_bytes())
        .unwrap();
    assert!(head(&mut client).starts_with("HTTP/1.1 403"));
    assert_eq!(
        refused_cause(&gateway.next()),
        Some(GatewayRefusal::PrivateAddress)
    );
}

#[test]
fn the_started_line_reports_the_upstream_state() {
    let proxy = test_proxy(200, echo_server());
    let closed = free_port();
    let mut config = config(
        &["ok.example.com", "closed.example.com"],
        &[
            ("ok.example.com", proxy.port),
            ("closed.example.com", closed),
        ],
    );
    config.listen = format!("127.0.0.1:{}", free_port());
    let mut child = Command::new(GATEWAY)
        .args(config.to_args())
        .env_clear()
        .env(
            "SYSTEMROOT",
            std::env::var_os("SYSTEMROOT").unwrap_or_default(),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let _ = child.kill();
    let _ = child.wait();
    let GatewayLine::Started { upstreams, digest } = serde_json::from_str(&first).unwrap() else {
        panic!("{first}")
    };
    assert_eq!(digest, config.digest());
    let state = |port: u16| {
        upstreams
            .iter()
            .find(|u| u.upstream == format!("127.0.0.1:{port}"))
            .map(|u| u.state)
    };
    assert_eq!(state(proxy.port), Some(UpstreamState::Reachable));
    assert_eq!(state(closed), Some(UpstreamState::Unreachable));
}

/// AD-100: the Linux programs link core and serde_json only, so the musl
/// build needs no C toolchain (read from `cargo metadata`).
#[test]
fn the_crate_depends_on_core_and_serde_json_only() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--manifest-path",
            manifest,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "officina-container")
        .unwrap();
    let normal: std::collections::BTreeSet<&str> = package["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"].is_null())
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        normal,
        ["officina-core", "serde_json"].into_iter().collect()
    );
}
