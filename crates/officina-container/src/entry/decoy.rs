//! `officina-entry decoy` (AD-117, D-235): on its own plain bridge, it
//! resolves Docker Desktop's names and its own, prints the addresses,
//! listens, connects to its own port and prints `listening`, then accepts
//! until it is stopped. `decoy --probe <address:port>`, the host family's
//! control, connects to it from a second container on the same bridge.
//! `--port` moves the port for the tests.

use std::io::Write;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

use officina_core::workshop::battery::{
    DECOY_PORT, DOCKER_DESKTOP_NAMES, DecoyAddresses, DecoyLine,
};

use super::{Layout, flag};

/// How long the probe and the self-connect wait for the decoy.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

fn print(line: &DecoyLine) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", line.to_json());
    let _ = out.flush();
}

/// Every address `name` resolves to, deduplicated, in order.
fn resolve(name: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for addr in (name, 0).to_socket_addrs().into_iter().flatten() {
        let text = addr.ip().to_string();
        if !found.contains(&text) {
            found.push(text);
        }
    }
    found
}

/// The decoy: addresses, listener, self-connect, then accepts forever.
pub fn decoy(layout: &Layout, args: &[String]) -> i32 {
    if let Some(target) = flag(args, "--probe") {
        return probe(target);
    }
    let port = flag(args, "--port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DECOY_PORT);
    let hostname = std::fs::read_to_string(layout.at("/etc/hostname"))
        .map(|h| h.trim().to_owned())
        .unwrap_or_default();
    let own = if hostname.is_empty() {
        Vec::new()
    } else {
        resolve(&hostname)
            .into_iter()
            .filter(|a| !a.starts_with("127.") && a != "::1")
            .collect()
    };
    let mut resolved = DOCKER_DESKTOP_NAMES.iter().map(|name| resolve(name));
    print(&DecoyLine::Addresses(DecoyAddresses {
        own,
        host_docker_internal: resolved.next().unwrap_or_default(),
        gateway_docker_internal: resolved.next().unwrap_or_default(),
    }));
    let listener = match TcpListener::bind(("0.0.0.0", port)) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("officina-entry decoy: cannot listen on {port}: {e}");
            return 1;
        }
    };
    let own_target = format!("127.0.0.1:{port}");
    let connected = own_target
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok());
    if !connected {
        eprintln!("officina-entry decoy: its own port did not answer");
        return 1;
    }
    print(&DecoyLine::Listening { port });
    for stream in listener.incoming() {
        drop(stream);
    }
    0
}

/// The probe: `connected` and exit 0 when `target` accepts, else
/// `unreachable` and exit 1.
fn probe(target: &str) -> i32 {
    let accepted = target
        .to_socket_addrs()
        .into_iter()
        .flatten()
        .any(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok());
    if accepted {
        print(&DecoyLine::Connected);
        0
    } else {
        print(&DecoyLine::Unreachable);
        1
    }
}
