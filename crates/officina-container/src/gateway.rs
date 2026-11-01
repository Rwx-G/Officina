//! The egress gateway (AD-103, D-234, D-242, D-611): a CONNECT proxy that
//! lets a worker reach the allowed hosts on 443 over TLS, and nothing else.
//! Every rule is core's (`officina_core::workshop::gateway`); this module
//! holds the sockets and the threads. It holds no credential, writes no
//! file, runs no shell and reads no environment: its command line is its
//! whole configuration.

use std::io::{Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::workshop::gateway::tls::{HelloRead, MAX_HELLO_BYTES, server_name};
use officina_core::workshop::gateway::{
    GatewayConfig, GatewayLine, GatewayRefusal, LIMIT_EXIT_CODE, Limiter, MAX_REQUEST_BYTES,
    MAX_TUNNELS, RequestRead, TLS_PORT, UpstreamProbe, UpstreamState, addr, admit, parse_request,
    upstream_status,
};

/// The deadline of each of a connection's steps: the request, the
/// ClientHello, the connection onward (AD-103).
pub const STEP_DEADLINE: Duration = Duration::from_secs(10);
/// How often a relaying tunnel looks at the gateway's deadline.
const RELAY_TICK: Duration = Duration::from_millis(500);
/// The upstream probe's deadline at start.
const PROBE_DEADLINE: Duration = Duration::from_secs(5);

/// What every connection's thread shares.
struct Shared {
    config: GatewayConfig,
    started: Instant,
    deadline: Duration,
    state: Mutex<State>,
    out: Mutex<Box<dyn Write + Send>>,
}

struct State {
    limiter: Limiter,
    tunnels: usize,
}

impl Shared {
    fn line(&self, line: &GatewayLine) {
        if let Ok(mut out) = self.out.lock() {
            let _ = writeln!(out, "{}", line.to_line());
            let _ = out.flush();
        }
    }

    fn past_deadline(&self) -> bool {
        self.started.elapsed() >= self.deadline
    }

    fn now_ms(&self) -> i64 {
        i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX)
    }

    /// Records a refusal; at the limit the gateway writes `limit` and exits
    /// [`LIMIT_EXIT_CODE`] (AD-103).
    fn refused(&self, host: &str, port: u16, cause: GatewayRefusal, status: Option<u16>) {
        self.line(&GatewayLine::refused(host, port, cause, status));
        let tripped = self
            .state
            .lock()
            .map(|mut state| state.limiter.refused(self.now_ms()))
            .unwrap_or(false);
        if tripped {
            self.line(&GatewayLine::Limit {});
            std::process::exit(LIMIT_EXIT_CODE);
        }
    }
}

/// The reason phrase of a refusal's status.
fn reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        405 => "Method Not Allowed",
        _ => "Forbidden",
    }
}

/// Answers `cause`'s status and closes, before the tunnel was established.
fn answer_refusal(stream: &mut TcpStream, cause: GatewayRefusal) {
    let status = cause.status();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        reason(status)
    );
    let _ = stream.shutdown(Shutdown::Both);
}

/// Reads from `stream` into `buf` until `done` says the bytes are enough,
/// or `cap` bytes, or `deadline` passes; false on the deadline, a closed
/// stream or an error.
fn read_until(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    cap: usize,
    deadline: Duration,
    done: &dyn Fn(&[u8]) -> bool,
) -> bool {
    let start = Instant::now();
    let mut chunk = [0u8; 4096];
    while !done(buf) {
        if buf.len() >= cap {
            return true;
        }
        let left = deadline.saturating_sub(start.elapsed());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            return false;
        }
        let want = chunk.len().min(cap - buf.len());
        match stream.read(&mut chunk[..want]) {
            Ok(0) | Err(_) => return false,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    true
}

/// One connection, from its request to the end of its tunnel.
fn handle(shared: &Shared, mut client: TcpStream) {
    let mut buf = Vec::new();
    let read = read_until(
        &mut client,
        &mut buf,
        MAX_REQUEST_BYTES,
        STEP_DEADLINE,
        &|b| !matches!(parse_request(b), RequestRead::More),
    );
    let request = match parse_request(&buf) {
        RequestRead::Done(request) => request,
        RequestRead::Refused(cause) => {
            answer_refusal(&mut client, cause);
            return shared.refused("", 0, cause, None);
        }
        RequestRead::More => {
            let cause = if read {
                GatewayRefusal::Oversized
            } else {
                GatewayRefusal::Malformed
            };
            answer_refusal(&mut client, cause);
            return shared.refused("", 0, cause, None);
        }
    };
    if shared.past_deadline() {
        answer_refusal(&mut client, GatewayRefusal::Deadline);
        return shared.refused(&request.host, request.port, GatewayRefusal::Deadline, None);
    }
    let admitted = match admit(&request, &shared.config.allow) {
        Ok(admitted) => admitted,
        Err(cause) => {
            answer_refusal(&mut client, cause);
            return shared.refused(&request.host, request.port, cause, None);
        }
    };
    let host = admitted.host;
    let upstream = shared.config.routes.get(&admitted.entry).cloned();
    // Direct mode checks every address before answering (AD-103 step 3);
    // upstream mode leaves that to the proxy (FR-4).
    let target = match &upstream {
        Some(_) => None,
        None => match resolve(&host) {
            Ok(target) => Some(target),
            Err(cause) => {
                answer_refusal(&mut client, cause);
                return shared.refused(&host, request.port, cause, None);
            }
        },
    };
    if client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .is_err()
    {
        return;
    }
    let mut hello_buf = request.rest;
    read_until(
        &mut client,
        &mut hello_buf,
        MAX_HELLO_BYTES,
        STEP_DEADLINE,
        &|b| !matches!(server_name(b), HelloRead::More),
    );
    let hello = match server_name(&hello_buf) {
        HelloRead::Done(hello) => hello,
        HelloRead::Refused(cause) => {
            let _ = client.shutdown(Shutdown::Both);
            return shared.refused(&host, request.port, cause, None);
        }
        HelloRead::More => {
            let _ = client.shutdown(Shutdown::Both);
            return shared.refused(&host, request.port, GatewayRefusal::NotTls, None);
        }
    };
    let named = hello.server_name.to_ascii_lowercase();
    if named.strip_suffix('.').unwrap_or(&named) != host {
        let _ = client.shutdown(Shutdown::Both);
        return shared.refused(&host, request.port, GatewayRefusal::SniMismatch, None);
    }
    let onward = match (&upstream, target) {
        (Some(proxy), _) => through_upstream(proxy, &host),
        (None, Some(target)) => TcpStream::connect_timeout(&SocketAddr::V4(target), STEP_DEADLINE)
            .map_err(|_| (GatewayRefusal::ConnectFailed, None)),
        (None, None) => Err((GatewayRefusal::ConnectFailed, None)),
    };
    let mut server = match onward {
        Ok(server) => server,
        Err((cause, status)) => {
            let _ = client.shutdown(Shutdown::Both);
            return shared.refused(&host, request.port, cause, status);
        }
    };
    shared.line(&GatewayLine::Allowed {
        host: host.clone(),
        port: request.port,
        ech: hello.ech,
    });
    if server.write_all(&hello_buf).is_err() {
        return;
    }
    relay(shared, client, server);
}

/// The address direct mode connects to, from one resolution (AD-103 step
/// 3): every address checked, the first IPv4 one chosen.
fn resolve(host: &str) -> Result<SocketAddrV4, GatewayRefusal> {
    let addrs: Vec<IpAddr> = (host, TLS_PORT)
        .to_socket_addrs()
        .map_err(|_| GatewayRefusal::ResolveFailed)?
        .map(|a| a.ip())
        .collect();
    if addrs.is_empty() {
        return Err(GatewayRefusal::ResolveFailed);
    }
    addr::choose(&addrs).map(|v4| SocketAddrV4::new(v4, TLS_PORT))
}

/// A tunnel to `host` through the upstream proxy `proxy` (AD-103, D-611):
/// its own `CONNECT` with a `Host` header, at most 8 KiB of headers back, a
/// 2xx needed.
fn through_upstream(proxy: &str, host: &str) -> Result<TcpStream, (GatewayRefusal, Option<u16>)> {
    let connect_failed = (GatewayRefusal::ConnectFailed, None);
    let addr = proxy
        .to_socket_addrs()
        .map_err(|_| connect_failed)?
        .next()
        .ok_or(connect_failed)?;
    let mut stream =
        TcpStream::connect_timeout(&addr, STEP_DEADLINE).map_err(|_| connect_failed)?;
    write!(
        stream,
        "CONNECT {host}:{TLS_PORT} HTTP/1.1\r\nHost: {host}:{TLS_PORT}\r\n\r\n"
    )
    .map_err(|_| connect_failed)?;
    let mut head = Vec::new();
    // Byte by byte, so nothing past the headers is consumed.
    let start = Instant::now();
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_REQUEST_BYTES || start.elapsed() >= STEP_DEADLINE {
            return Err((GatewayRefusal::UpstreamRefused, upstream_status(&head)));
        }
        let _ = stream.set_read_timeout(Some(STEP_DEADLINE.saturating_sub(start.elapsed())));
        let mut byte = [0u8; 1];
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return Err((GatewayRefusal::UpstreamRefused, upstream_status(&head))),
        }
    }
    match upstream_status(&head) {
        Some(status) if (200..300).contains(&status) => Ok(stream),
        status => Err((GatewayRefusal::UpstreamRefused, status)),
    }
}

/// Copies `from` to `to` until the end of stream, an error or the
/// gateway's deadline, then closes both.
fn copy(shared: &Shared, mut from: TcpStream, mut to: TcpStream) {
    let _ = from.set_read_timeout(Some(RELAY_TICK));
    let mut chunk = [0u8; 16 * 1024];
    loop {
        if shared.past_deadline() {
            break;
        }
        match from.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if to.write_all(&chunk[..n]).is_err() {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let _ = from.shutdown(Shutdown::Both);
    let _ = to.shutdown(Shutdown::Both);
}

/// Relays both ways, one direction on a second thread; when that thread
/// cannot start, the tunnel closes.
fn relay(shared: &Shared, client: TcpStream, server: TcpStream) {
    let (Ok(client_back), Ok(server_back)) = (client.try_clone(), server.try_clone()) else {
        return;
    };
    std::thread::scope(|scope| {
        let back = std::thread::Builder::new()
            .name("gateway-back".into())
            .spawn_scoped(scope, || copy(shared, server_back, client_back));
        if back.is_err() {
            let _ = client.shutdown(Shutdown::Both);
            let _ = server.shutdown(Shutdown::Both);
            return;
        }
        copy(shared, client, server);
    });
}

/// Probes each distinct upstream once (AD-103's `started` line).
fn probe_upstreams(config: &GatewayConfig) -> Vec<UpstreamProbe> {
    config
        .upstreams()
        .into_iter()
        .map(|upstream| {
            let reachable = upstream
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
                .is_some_and(|addr| TcpStream::connect_timeout(&addr, PROBE_DEADLINE).is_ok());
            UpstreamProbe {
                upstream,
                state: if reachable {
                    UpstreamState::Reachable
                } else {
                    UpstreamState::Unreachable
                },
            }
        })
        .collect()
}

/// Serves `config` until the process ends, its lines on `out`. Returns an
/// exit code when the listener cannot be opened; the refusal limit exits
/// the process itself.
pub fn serve(config: GatewayConfig, out: Box<dyn Write + Send>) -> i32 {
    let listener = match TcpListener::bind(&config.listen) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("officina-gateway: cannot listen on {}: {e}", config.listen);
            return 1;
        }
    };
    let deadline = Duration::from_secs(config.deadline_s);
    let upstreams = probe_upstreams(&config);
    let shared = Arc::new(Shared {
        started: Instant::now(),
        deadline,
        state: Mutex::new(State {
            limiter: Limiter::new(config.refusal_limit, config.refusal_window_s),
            tunnels: 0,
        }),
        out: Mutex::new(out),
        config,
    });
    shared.line(&GatewayLine::Started {
        digest: shared.config.digest(),
        upstreams,
    });
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let admitted = shared
            .state
            .lock()
            .map(|mut state| {
                if state.tunnels >= MAX_TUNNELS {
                    false
                } else {
                    state.tunnels += 1;
                    true
                }
            })
            .unwrap_or(false);
        if !admitted {
            answer_refusal(&mut stream, GatewayRefusal::Busy);
            shared.refused("", 0, GatewayRefusal::Busy, None);
            continue;
        }
        let worker = Arc::clone(&shared);
        let Ok(rescue) = stream.try_clone() else {
            release(&shared);
            continue;
        };
        let spawned = std::thread::Builder::new()
            .name("gateway-tunnel".into())
            .spawn(move || {
                handle(&worker, stream);
                release(&worker);
            });
        if spawned.is_err() {
            release(&shared);
            let mut rescue = rescue;
            answer_refusal(&mut rescue, GatewayRefusal::Busy);
            shared.refused("", 0, GatewayRefusal::Busy, None);
        }
    }
    0
}

fn release(shared: &Shared) {
    if let Ok(mut state) = shared.state.lock() {
        state.tunnels = state.tunnels.saturating_sub(1);
    }
}
