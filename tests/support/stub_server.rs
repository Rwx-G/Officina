//! A scripted HTTP server for the tests of HTTP actions (story 2.11): one
//! listener on `127.0.0.1`, an ephemeral port, a thread answering each
//! connection with the next scripted reply (the last one repeats) and
//! recording each request's line and headers. Never the network. Included by
//! `#[path]` from the host and CLI test crates.

#![allow(dead_code)] // each test crate uses a different subset

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// What the stub answers to one connection.
#[derive(Debug, Clone)]
pub enum Reply {
    /// A status, extra headers and a body (`Content-Length` set from it; a
    /// `HEAD` request gets the headers only).
    Full {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// A 200 announcing `len` bytes, written until done or the client drops
    /// the connection.
    Large { len: u64 },
    /// Reads the request and never answers, until the stub is dropped.
    Hang,
    /// A 200 announcing a large body, then one byte every 200 ms: a server
    /// slow enough to outlast any timeout, never silent.
    Drip,
}

impl Reply {
    pub fn ok(body: &str) -> Reply {
        Reply::Full {
            status: 200,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn status(status: u16, body: &str) -> Reply {
        Reply::Full {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }
}

/// One request as it arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// `GET /path HTTP/1.1`.
    pub line: String,
    /// Names lowercase, values as sent.
    pub headers: Vec<(String, String)>,
}

impl Recorded {
    pub fn method(&self) -> &str {
        self.line.split(' ').next().unwrap_or_default()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Default)]
struct State {
    requests: Mutex<Vec<Recorded>>,
    connections: AtomicUsize,
    /// A write to the client failed: it dropped the connection.
    aborted: AtomicBool,
    /// Connections that opened with a TLS handshake, closed at once.
    tls_attempts: AtomicUsize,
    stop: AtomicBool,
}

/// A running stub; it stops when dropped.
pub struct Stub {
    pub addr: SocketAddr,
    state: Arc<State>,
}

impl Stub {
    /// Starts a stub answering connections with `replies` in order, the
    /// last one repeating.
    pub fn start(replies: Vec<Reply>) -> Stub {
        let _binding = PORTS.lock().unwrap_or_else(PoisonError::into_inner);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(State::default());
        let shared = Arc::clone(&state);
        std::thread::spawn(move || serve(&listener, &replies, &shared));
        Stub { addr, state }
    }

    /// `http://127.0.0.1:<port><path>`.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.state.requests.lock().unwrap().clone()
    }

    pub fn connections(&self) -> usize {
        self.state.connections.load(Ordering::SeqCst)
    }

    pub fn tls_attempts(&self) -> usize {
        self.state.tls_attempts.load(Ordering::SeqCst)
    }

    /// Waits up to `within` for the client to drop a large body.
    pub fn aborted_within(&self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if self.state.aborted.load(Ordering::SeqCst) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.state.aborted.load(Ordering::SeqCst)
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
    }
}

/// Taken by [`Stub::start`] to bind, and held by a [`ClosedPort`]: tests run
/// in parallel, and a port released by [`closed_port`] is the next one the
/// system hands a stub of the same binary.
static PORTS: Mutex<()> = Mutex::new(());

/// A port nothing listens on, while this value lives.
pub struct ClosedPort {
    pub port: u16,
    _no_stub_binds: MutexGuard<'static, ()>,
}

/// A port nothing listens on: bound, then released, with no stub of this
/// test binary able to bind until the [`ClosedPort`] is dropped.
pub fn closed_port() -> ClosedPort {
    let held = PORTS.lock().unwrap_or_else(PoisonError::into_inner);
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    ClosedPort {
        port,
        _no_stub_binds: held,
    }
}

fn serve(listener: &TcpListener, replies: &[Reply], state: &State) {
    let mut index = 0;
    while !state.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                state.connections.fetch_add(1, Ordering::SeqCst);
                let reply = replies[index.min(replies.len() - 1)].clone();
                index += 1;
                answer(stream, &reply, state);
            }
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

/// Reads one request head, or nothing when it opens with a TLS handshake.
fn read_head(stream: &mut TcpStream, state: &State) -> Option<Recorded> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        if head.is_empty() && byte[0] == 0x16 {
            state.tls_attempts.fetch_add(1, Ordering::SeqCst);
            return None;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.split("\r\n").filter(|l| !l.is_empty());
    let line = lines.next()?.to_owned();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    Some(Recorded { line, headers })
}

fn answer(mut stream: TcpStream, reply: &Reply, state: &State) {
    let Some(request) = read_head(&mut stream, state) else {
        return;
    };
    let head_only = request.method() == "HEAD";
    state.requests.lock().unwrap().push(request);
    match reply {
        Reply::Full {
            status,
            headers,
            body,
        } => {
            let mut text = format!(
                "HTTP/1.1 {status} Stub\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                text.push_str(&format!("{name}: {value}\r\n"));
            }
            text.push_str("\r\n");
            let _ = stream.write_all(text.as_bytes());
            if !head_only {
                let _ = stream.write_all(body);
            }
            let _ = stream.flush();
        }
        Reply::Large { len } => {
            let head =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
            if stream.write_all(head.as_bytes()).is_err() {
                state.aborted.store(true, Ordering::SeqCst);
                return;
            }
            let chunk = vec![b'x'; 64 * 1024];
            let mut written = 0u64;
            while written < *len {
                if state.stop.load(Ordering::SeqCst) {
                    return;
                }
                let n = chunk.len().min(usize::try_from(*len - written).unwrap());
                if stream.write_all(&chunk[..n]).is_err() {
                    state.aborted.store(true, Ordering::SeqCst);
                    return;
                }
                written += n as u64;
            }
        }
        Reply::Hang => {
            while !state.stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        Reply::Drip => {
            let head = "HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(head.as_bytes());
            while !state.stop.load(Ordering::SeqCst) {
                if stream
                    .write_all(b"x")
                    .and_then(|()| stream.flush())
                    .is_err()
                {
                    state.aborted.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}
