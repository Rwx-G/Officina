//! `officina-entry battery --phase <1|2> --plan <json>` (AD-117, D-235):
//! plays core's `battery::ATTEMPTS` of its phase in a container built by
//! the worker's own vector, each family's control first, and prints one
//! JSON line per control and per attempt on stdout. Std only; the ICMP
//! attempt goes through `python3`, the image's own. With `--root` (the
//! tests) every path is read under that folder and the egress family aims
//! at loopback, so no test reaches the outside.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use officina_core::workshop::battery::{
    Attempt, AttemptLine, AttemptOutcome, BatteryFamily, BatteryLine, Control, ControlLine,
    ControlStatus, ENGINE_HOST_NAME, HOST_PORTS, ICMP_TARGET, LITERAL_ADDRESS, LOOPBACK_NAME,
    OFF_LIST_HOST, OTHER_SNI_HOST, OUTSIDE_NAME, PUBLIC_IPV6, PUBLIC_RESOLVER, PUBLIC_TCP, Plan,
    TEST_HOST, attempts_for, parse_plan,
};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::gateway::tls::client_hello;
use officina_core::workshop::vector::{ENTRY_PROGRAM, HOME_DIR, WORK_DIR, WORKER_USER};

use super::{Layout, flag};

/// How long a connection attempt may take before it counts as failed.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the gateway may take to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// The most entries the setuid walk reads before it gives up.
const MAX_WALK_ENTRIES: usize = 500_000;
/// The children the process attempt spawns past the limit before it stops.
const SPAWN_MARGIN: u32 = 16;
/// The memory the hog touches past the limit.
const HOG_MARGIN_MIB: u32 = 64;
/// The gateway's other ports the host family tries.
const GATEWAY_OTHER_PORTS: &[u16] = &[22, 80, 443, 2375, 2376, 8080];
/// The engine's sockets, which must not be in the container.
const ENGINE_SOCKETS: &[&str] = &[
    "/var/run/docker.sock",
    "/run/docker.sock",
    "/var/run/docker.sock.raw",
    "/run/host-services/docker.proxy.sock",
    "/mnt/wsl/docker-desktop/shared-sockets/guest-services/docker.sock",
];
/// Docker Desktop's host paths, which must not be in the container; `/mnt`
/// must also be empty.
const HOST_PATHS: &[&str] = &[
    "/host_mnt",
    "/run/desktop",
    "/mnt/host",
    "/mnt/wsl",
    "/mnt/c",
];
/// The folders written to by the files family (each attempt names one).
const WRITE_TARGETS: &[(&str, &str)] = &[
    ("write_root", "/"),
    ("write_usr", "/usr"),
    ("write_officina_bin", "/opt/officina/bin"),
    ("write_etc", "/etc"),
];
/// The homes the credentials family looks in.
const HOMES: &[&str] = &[HOME_DIR, "/home/worker", "/root"];
/// The agent sockets Docker Desktop can forward.
const AGENT_SOCKETS: &[&str] = &["/run/host-services/ssh-auth.sock"];
/// The cloud tools' credential folders, under a home.
const CLOUD_DIRS: &[&str] = &[".aws", ".config/gcloud", ".azure", ".kube"];
/// The engine's client configuration, under a home.
const HOME_ENGINE_CONFIG: &str = ".docker/config.json";
/// The engine's own configuration folder.
const ENGINE_CONFIG_DIR: &str = "/etc/docker";
/// The status fields that must read zero capabilities.
const CAP_FIELDS: &[&str] = &["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"];
/// The probe file's name.
const PROBE: &str = "officina-battery-probe";

type Seen = (AttemptOutcome, Option<String>);

/// Where the egress family aims: the public targets in a container,
/// loopback under a test root.
struct Targets {
    tcp: Vec<String>,
    ipv6: String,
    resolver: String,
    outside_name: String,
    icmp: String,
}

impl Targets {
    fn of(layout_is_test: bool) -> Targets {
        if layout_is_test {
            return Targets {
                tcp: vec!["127.0.0.1:9".to_owned()],
                ipv6: "[::1]:9".to_owned(),
                resolver: "127.0.0.1:9".to_owned(),
                outside_name: "officina-unresolvable.invalid".to_owned(),
                icmp: "127.0.0.1".to_owned(),
            };
        }
        Targets {
            tcp: PUBLIC_TCP.iter().map(|t| (*t).to_owned()).collect(),
            ipv6: PUBLIC_IPV6.to_owned(),
            resolver: PUBLIC_RESOLVER.to_owned(),
            outside_name: OUTSIDE_NAME.to_owned(),
            icmp: ICMP_TARGET.to_owned(),
        }
    }
}

struct Battery<'a> {
    layout: &'a Layout,
    plan: Plan,
    targets: Targets,
    started: Instant,
}

fn print(line: &BatteryLine) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", line.to_json());
    let _ = out.flush();
}

fn seen(outcome: AttemptOutcome, detail: impl Into<String>) -> Seen {
    (outcome, Some(detail.into()))
}

fn refused() -> Seen {
    (AttemptOutcome::Refused, None)
}

/// The first address of `target` that accepts a TCP connection.
fn connects(target: &str) -> Option<SocketAddr> {
    target
        .to_socket_addrs()
        .ok()?
        .find(|addr| TcpStream::connect_timeout(addr, CONNECT_TIMEOUT).is_ok())
}

/// The first `address:port` of `addresses` x `ports` that accepts.
fn any_connects(addresses: &[String], ports: &[u16]) -> Option<String> {
    addresses.iter().find_map(|address| {
        ports.iter().find_map(|port| {
            let target = if address.contains(':') {
                format!("[{address}]:{port}")
            } else {
                format!("{address}:{port}")
            };
            connects(&target).map(|_| target)
        })
    })
}

/// Escaped when one target accepts, refused when none does.
fn none_connect(addresses: &[String], ports: &[u16]) -> Seen {
    match any_connects(addresses, ports) {
        Some(target) => seen(AttemptOutcome::Escaped, format!("connected to {target}")),
        None => refused(),
    }
}

/// Escaped when one of `paths` exists, refused when none does.
fn none_exist(paths: &[PathBuf]) -> Seen {
    match paths.iter().find(|p| p.symlink_metadata().is_ok()) {
        Some(path) => seen(
            AttemptOutcome::Escaped,
            format!("{} exists", path.display()),
        ),
        None => refused(),
    }
}

/// Creates and removes a probe file in `dir`: true when the write held.
fn writes(dir: &Path) -> bool {
    let probe = dir.join(PROBE);
    let wrote = fs::write(&probe, b"x").is_ok();
    if wrote {
        let _ = fs::remove_file(&probe);
    }
    wrote
}

impl Battery<'_> {
    fn is_test(&self) -> bool {
        self.layout.at("/") != Path::new("/")
    }

    fn gateway_target(&self) -> String {
        format!("{}:{}", self.plan.gateway, self.plan.gateway_port)
    }

    /// One request to the gateway: its status line, or why none came.
    fn ask_gateway(&self, request: &[u8]) -> Result<(String, TcpStream), String> {
        let addr = self
            .gateway_target()
            .to_socket_addrs()
            .map_err(|e| format!("the gateway's name: {e}"))?
            .next()
            .ok_or("the gateway's name resolves to nothing")?;
        let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
            .map_err(|e| format!("the gateway's port: {e}"))?;
        let _ = stream.set_read_timeout(Some(ANSWER_TIMEOUT));
        stream
            .write_all(request)
            .map_err(|e| format!("the request: {e}"))?;
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while head.len() < 256 && !head.ends_with(b"\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                Ok(_) => break,
                Err(e) if head.is_empty() => return Err(format!("no answer: {e}")),
                Err(_) => break,
            }
        }
        if head.is_empty() {
            return Err("closed without an answer".to_owned());
        }
        Ok((String::from_utf8_lossy(&head).trim().to_owned(), stream))
    }

    fn connect_request(host: &str, port: u16) -> Vec<u8> {
        format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n").into_bytes()
    }

    /// A request the gateway must refuse with a status.
    fn gateway_refuses(&self, request: &[u8]) -> Seen {
        match self.ask_gateway(request) {
            Ok((status, _)) if status.starts_with("HTTP/1.1 200") => {
                seen(AttemptOutcome::Escaped, status)
            }
            Ok((status, _)) => seen(AttemptOutcome::Refused, status),
            Err(why) => seen(AttemptOutcome::Inconclusive, why),
        }
    }

    /// D-235: the gateway's name resolves, its port opens and it answers
    /// an allowed `CONNECT`.
    fn gateway_reachable(&self) -> bool {
        matches!(
            self.ask_gateway(&Self::connect_request(TEST_HOST, 443)),
            Ok((status, _)) if status.starts_with("HTTP/1.1 200")
        )
    }

    /// D-235: writes to `/tmp` and `/work` succeed.
    fn scratch_writable(&self) -> bool {
        writes(&self.layout.at("/tmp")) && writes(&self.layout.at(WORK_DIR))
    }

    fn control(&self, family: BatteryFamily, control: Control) -> Option<ControlLine> {
        let ok = match control {
            Control::GatewayReachable => self.gateway_reachable(),
            Control::WritableScratch => self.scratch_writable(),
            // The host's: a second container on the decoy's network.
            Control::DecoyListening => return None,
        };
        Some(ControlLine {
            family,
            control,
            status: if ok {
                ControlStatus::Ok
            } else {
                ControlStatus::Failed
            },
        })
    }

    fn sni_mismatch(&self) -> Seen {
        let (status, mut stream) = match self.ask_gateway(&Self::connect_request(TEST_HOST, 443)) {
            Ok(answer) => answer,
            Err(why) => return seen(AttemptOutcome::Inconclusive, why),
        };
        if !status.starts_with("HTTP/1.1 200") {
            return seen(AttemptOutcome::Inconclusive, status);
        }
        let mut rest = [0u8; 2];
        let _ = stream.read(&mut rest);
        if stream
            .write_all(&client_hello(OTHER_SNI_HOST, false))
            .is_err()
        {
            return refused();
        }
        let mut answer = [0u8; 64];
        match stream.read(&mut answer) {
            Ok(0) => refused(),
            Ok(n) => seen(
                AttemptOutcome::Escaped,
                format!("{n} bytes came back past the ClientHello"),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => refused(),
            Err(e) => seen(AttemptOutcome::Inconclusive, format!("no close: {e}")),
        }
    }

    fn after_deadline(&self) -> Seen {
        let wait = Duration::from_secs(self.plan.wait_s);
        if let Some(left) = wait.checked_sub(self.started.elapsed()) {
            std::thread::sleep(left);
        }
        self.gateway_refuses(&Self::connect_request(TEST_HOST, 443))
    }

    fn after_limit(&self) -> Seen {
        let bad = Self::connect_request_with_version(TEST_HOST, "HTTP/9.9");
        for _ in 0..self.plan.bad_requests {
            let _ = self.ask_gateway(&bad);
        }
        match self.ask_gateway(&Self::connect_request(TEST_HOST, 443)) {
            Ok((status, _)) => seen(
                AttemptOutcome::Escaped,
                format!("the gateway still answers: {status}"),
            ),
            Err(_) => refused(),
        }
    }

    fn connect_request_with_version(host: &str, version: &str) -> Vec<u8> {
        format!("CONNECT {host}:443 {version}\r\nHost: {host}:443\r\n\r\n").into_bytes()
    }

    fn udp_dns(&self) -> Seen {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            return refused();
        };
        let _ = socket.set_read_timeout(Some(ANSWER_TIMEOUT));
        let mut query = vec![0x0b, 0x0a, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in self.targets.outside_name.split('.') {
            query.push(u8::try_from(label.len()).unwrap_or(0));
            query.extend_from_slice(label.as_bytes());
        }
        query.extend_from_slice(&[0, 0, 1, 0, 1]);
        if socket.send_to(&query, &self.targets.resolver).is_err() {
            return refused();
        }
        let mut answer = [0u8; 512];
        match socket.recv_from(&mut answer) {
            Ok((n, from)) => seen(AttemptOutcome::Escaped, format!("{n} bytes from {from}")),
            Err(_) => refused(),
        }
    }

    fn icmp(&self) -> Seen {
        const SCRIPT: &str = "import socket, sys\n\
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_ICMP)\n\
s.settimeout(3)\n\
s.sendto(b'\\x08\\x00\\x00\\x00\\x00\\x01\\x00\\x01officina', (sys.argv[1], 0))\n\
s.recv(64)\n\
print('reply')\n";
        let ran = Command::new("python3")
            .args(["-c", SCRIPT, &self.targets.icmp])
            .stdin(Stdio::null())
            .output();
        match ran {
            Err(e) => seen(AttemptOutcome::Inconclusive, format!("python3: {e}")),
            Ok(out) if String::from_utf8_lossy(&out.stdout).contains("reply") => {
                seen(AttemptOutcome::Escaped, "an echo reply came back")
            }
            Ok(_) => refused(),
        }
    }

    /// The addresses the internal network's routes name: the default
    /// gateway, and the first address of each directly connected network.
    fn network_gateways(&self) -> Vec<String> {
        let table = fs::read_to_string(self.layout.at("/proc/net/route")).unwrap_or_default();
        let mut found = Vec::new();
        for line in table.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [_, destination, gateway, _, _, _, _, mask, ..] = fields.as_slice() else {
                continue;
            };
            // The kernel prints each address as the hex of its bytes read
            // in host order: back to network order before any arithmetic.
            let read = |hex: &str| {
                u32::from_str_radix(hex, 16)
                    .ok()
                    .map(|v| u32::from_be_bytes(v.to_le_bytes()))
            };
            let (Some(destination), Some(gateway), Some(mask)) =
                (read(destination), read(gateway), read(mask))
            else {
                continue;
            };
            let candidate = if gateway != 0 {
                gateway
            } else if mask != 0 {
                (destination & mask).wrapping_add(1)
            } else {
                continue;
            };
            let text = std::net::Ipv4Addr::from(candidate).to_string();
            if !found.contains(&text) {
                found.push(text);
            }
        }
        found
    }

    fn setuid_files(&self) -> Seen {
        let root = self.layout.at("/");
        let Ok(device) = fs::metadata(&root).map(|m| m.dev()) else {
            return seen(AttemptOutcome::Inconclusive, "the root cannot be read");
        };
        let skip: Vec<PathBuf> = ["/proc", "/sys", "/dev", WORK_DIR, "/tmp"]
            .iter()
            .map(|p| self.layout.at(p))
            .collect();
        let mut stack = vec![root];
        let mut read = 0usize;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                read += 1;
                if read > MAX_WALK_ENTRIES {
                    return seen(AttemptOutcome::Inconclusive, "too many files to walk");
                }
                let path = entry.path();
                let Ok(meta) = entry.path().symlink_metadata() else {
                    continue;
                };
                if meta.is_dir() {
                    if meta.dev() == device && !skip.contains(&path) {
                        stack.push(path);
                    }
                } else if meta.is_file() && meta.permissions().mode() & 0o6000 != 0 {
                    return seen(
                        AttemptOutcome::Escaped,
                        format!("{} is setuid or setgid", path.display()),
                    );
                }
            }
        }
        refused()
    }

    fn su_sudo(&self) -> Seen {
        for (program, args) in [
            ("su", &["-c", "id -u"][..]),
            ("sudo", &["-n", "id", "-u"][..]),
        ] {
            let ran = Command::new("timeout")
                .arg("5")
                .arg(program)
                .args(args)
                .stdin(Stdio::null())
                .output();
            if let Ok(out) = ran
                && out.status.success()
                && String::from_utf8_lossy(&out.stdout).trim() == "0"
            {
                return seen(AttemptOutcome::Escaped, format!("{program} gave uid 0"));
            }
        }
        refused()
    }

    /// `/proc/self/status`'s fields.
    fn status(&self) -> Vec<(String, String)> {
        fs::read_to_string(self.layout.at("/proc/self/status"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect()
    }

    fn status_holds(&self, check: impl Fn(&[(String, String)]) -> Result<(), String>) -> Seen {
        let status = self.status();
        if status.is_empty() {
            return seen(AttemptOutcome::Inconclusive, "no /proc/self/status");
        }
        match check(&status) {
            Ok(()) => refused(),
            Err(why) => seen(AttemptOutcome::Escaped, why),
        }
    }

    fn homes(&self) -> Vec<PathBuf> {
        HOMES.iter().map(|h| self.layout.at(h)).collect()
    }

    fn under_homes(&self, names: &[&str]) -> Vec<PathBuf> {
        self.homes()
            .iter()
            .flat_map(|home| names.iter().map(move |n| home.join(n)))
            .collect()
    }

    fn ssh_keys(&self) -> Seen {
        for dir in self.under_homes(&[".ssh"]) {
            if fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some()) {
                return seen(
                    AttemptOutcome::Escaped,
                    format!("{} holds files", dir.display()),
                );
            }
        }
        refused()
    }

    fn git_credentials(&self) -> Seen {
        let stores = self.under_homes(&[".git-credentials", ".config/git/credentials"]);
        if let (AttemptOutcome::Escaped, detail) = none_exist(&stores) {
            return (AttemptOutcome::Escaped, detail);
        }
        let mut configs = self.under_homes(&[".gitconfig", ".config/git/config"]);
        configs.push(self.layout.at("/etc/gitconfig"));
        for config in configs {
            if fs::read_to_string(&config).is_ok_and(|t| t.to_lowercase().contains("credential")) {
                return seen(
                    AttemptOutcome::Escaped,
                    format!("{} names a credential helper", config.display()),
                );
            }
        }
        refused()
    }

    fn environment_names(&self) -> Seen {
        let mut names: Vec<String> = std::env::vars_os()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        let extra: Vec<&str> = names
            .iter()
            .filter(|n| !self.plan.expected_env.contains(n))
            .map(String::as_str)
            .collect();
        let missing: Vec<&str> = self
            .plan
            .expected_env
            .iter()
            .filter(|n| !names.contains(n))
            .map(String::as_str)
            .collect();
        if extra.is_empty() && missing.is_empty() {
            refused()
        } else {
            seen(
                AttemptOutcome::Escaped,
                format!("extra: {}; missing: {}", extra.join(","), missing.join(",")),
            )
        }
    }

    fn process_limit(&self) -> Seen {
        let bound = self.plan.pids_limit.saturating_add(SPAWN_MARGIN);
        let mut children: Vec<Child> = Vec::new();
        let mut failed = false;
        for _ in 0..bound {
            match Command::new("sleep")
                .arg("60")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => children.push(child),
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        let spawned = children.len();
        for mut child in children {
            let _ = child.kill();
            let _ = child.wait();
        }
        let spawned = u32::try_from(spawned).unwrap_or(u32::MAX);
        if failed && spawned <= self.plan.pids_limit {
            seen(
                AttemptOutcome::Refused,
                format!("a spawn failed after {spawned} children"),
            )
        } else {
            seen(
                AttemptOutcome::Escaped,
                format!("{spawned} children spawned"),
            )
        }
    }

    fn memory_limit(&self) -> Seen {
        let program = if self.is_test() {
            std::env::current_exe().unwrap_or_else(|_| PathBuf::from(ENTRY_PROGRAM))
        } else {
            PathBuf::from(ENTRY_PROGRAM)
        };
        let mib = self.plan.memory_mib.saturating_add(HOG_MARGIN_MIB);
        let ran = Command::new(program)
            .args(["battery-hog", &mib.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match ran {
            Ok(status) if status.signal() == Some(9) => refused(),
            Ok(status) if status.success() => seen(
                AttemptOutcome::Escaped,
                format!("{mib} MiB touched without a kill"),
            ),
            Ok(status) => seen(
                AttemptOutcome::Inconclusive,
                format!("the hog ended {status}"),
            ),
            Err(e) => seen(AttemptOutcome::Inconclusive, format!("the hog: {e}")),
        }
    }

    fn mount_command(&self) -> Seen {
        let target = self.layout.at("/tmp/officina-battery-mnt");
        let _ = fs::create_dir_all(&target);
        let ran = Command::new("mount")
            .args(["-t", "tmpfs", "officina-battery"])
            .arg(&target)
            .stdin(Stdio::null())
            .output();
        match ran {
            Ok(out) if out.status.success() => {
                let _ = Command::new("umount").arg(&target).status();
                seen(AttemptOutcome::Escaped, "a tmpfs was mounted")
            }
            Ok(_) => refused(),
            Err(e) => seen(AttemptOutcome::Refused, format!("mount: {e}")),
        }
    }

    fn login_write(&self, create_dir: bool) -> Seen {
        let Some(login) = &self.plan.login_mount else {
            return seen(AttemptOutcome::Inconclusive, "no login mount in the plan");
        };
        let dir = self.layout.at(login);
        let held = if create_dir {
            let path = dir.join(PROBE);
            let made = fs::create_dir(&path).is_ok();
            if made {
                let _ = fs::remove_dir(&path);
            }
            made
        } else {
            writes(&dir)
        };
        if held {
            seen(AttemptOutcome::Escaped, "the login mount took a write")
        } else {
            refused()
        }
    }

    fn write_attempt(&self, dir: &Path) -> Seen {
        if writes(dir) {
            seen(
                AttemptOutcome::Escaped,
                format!("{} took a write", dir.display()),
            )
        } else {
            refused()
        }
    }

    fn host_paths(&self) -> Seen {
        let mut paths: Vec<PathBuf> = HOST_PATHS.iter().map(|p| self.layout.at(p)).collect();
        if let Ok(entries) = fs::read_dir(self.layout.at("/mnt")) {
            paths.extend(entries.flatten().map(|e| e.path()));
        }
        none_exist(&paths)
    }

    fn play(&self, attempt: &Attempt) -> Seen {
        let plan = &self.plan;
        match attempt.code {
            "tcp_public" => match self.targets.tcp.iter().find_map(|t| connects(t)) {
                Some(addr) => seen(AttemptOutcome::Escaped, format!("connected to {addr}")),
                None => refused(),
            },
            "ipv6_socket" => match connects(&self.targets.ipv6) {
                Some(addr) => seen(AttemptOutcome::Escaped, format!("connected to {addr}")),
                None => refused(),
            },
            "udp_dns_public" => self.udp_dns(),
            "resolve_outside_name" => {
                match (self.targets.outside_name.as_str(), 443).to_socket_addrs() {
                    Ok(mut found) => match found.next() {
                        Some(addr) => seen(AttemptOutcome::Escaped, format!("resolved to {addr}")),
                        None => refused(),
                    },
                    Err(_) => refused(),
                }
            }
            "icmp" => self.icmp(),
            "off_list_host" => self.gateway_refuses(&Self::connect_request(OFF_LIST_HOST, 443)),
            "ip_literal" => self.gateway_refuses(&Self::connect_request(LITERAL_ADDRESS, 443)),
            "port_80" => self.gateway_refuses(&Self::connect_request(TEST_HOST, 80)),
            "port_22" => self.gateway_refuses(&Self::connect_request(TEST_HOST, 22)),
            "absolute_get" => self.gateway_refuses(
                format!("GET http://{TEST_HOST}/ HTTP/1.1\r\nHost: {TEST_HOST}\r\n\r\n").as_bytes(),
            ),
            "malformed_request" => {
                self.gateway_refuses(&Self::connect_request_with_version(TEST_HOST, "HTTP/9.9"))
            }
            "oversized_request" => {
                let mut request =
                    format!("CONNECT {TEST_HOST}:443 HTTP/1.1\r\nX-Pad: ").into_bytes();
                request.resize(request.len() + 9 * 1024, b'a');
                self.gateway_refuses(&request)
            }
            "sni_mismatch" => self.sni_mismatch(),
            "loopback_name" => self.gateway_refuses(&Self::connect_request(LOOPBACK_NAME, 443)),
            "engine_host_name" => {
                self.gateway_refuses(&Self::connect_request(ENGINE_HOST_NAME, 443))
            }
            "after_deadline" => self.after_deadline(),
            "after_limit" => self.after_limit(),
            "host_docker_internal" | "gateway_docker_internal" => {
                let addresses = if attempt.code == "host_docker_internal" {
                    &plan.decoy.host_docker_internal
                } else {
                    &plan.decoy.gateway_docker_internal
                };
                if addresses.is_empty() {
                    seen(AttemptOutcome::NotApplicable, "the name did not resolve")
                } else {
                    none_connect(addresses, HOST_PORTS)
                }
            }
            "network_gateways" => {
                let gateways = self.network_gateways();
                if gateways.is_empty() {
                    seen(AttemptOutcome::Inconclusive, "no route names a gateway")
                } else {
                    none_connect(&gateways, HOST_PORTS)
                }
            }
            "gateway_other_ports" => match self.gateway_target().to_socket_addrs() {
                Ok(addrs) => {
                    let addresses: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
                    none_connect(&addresses, GATEWAY_OTHER_PORTS)
                }
                Err(e) => seen(
                    AttemptOutcome::Inconclusive,
                    format!("the gateway's name: {e}"),
                ),
            },
            "decoy_other_network" => {
                if plan.decoy.own.is_empty() {
                    seen(AttemptOutcome::Inconclusive, "the decoy printed no address")
                } else {
                    none_connect(&plan.decoy.own, &[plan.decoy_port])
                }
            }
            "engine_sockets" => none_exist(
                &ENGINE_SOCKETS
                    .iter()
                    .map(|p| self.layout.at(p))
                    .collect::<Vec<_>>(),
            ),
            "write_battery_mount" => self.write_attempt(&self.layout.at(&plan.mount)),
            "host_paths_absent" => self.host_paths(),
            "mount_command" => self.mount_command(),
            "write_login_mount" => self.login_write(false),
            "create_in_login_mount" => self.login_write(true),
            "uid_gid" => {
                let want = WORKER_USER.split(':').next().unwrap_or_default().to_owned();
                self.status_holds(|status| {
                    for field in ["Uid", "Gid"] {
                        let value = status
                            .iter()
                            .find(|(k, _)| k == field)
                            .map(|(_, v)| v.as_str())
                            .unwrap_or_default();
                        if value.is_empty() || value.split_whitespace().any(|id| id != want) {
                            return Err(format!("{field}: {value}"));
                        }
                    }
                    Ok(())
                })
            }
            "capabilities" => self.status_holds(|status| {
                for field in CAP_FIELDS {
                    let value = status.iter().find(|(k, _)| k == field).map(|(_, v)| v);
                    if value.is_none_or(|v| v.chars().any(|c| c != '0')) {
                        return Err(format!("{field}: {}", value.map_or("absent", |v| v)));
                    }
                }
                Ok(())
            }),
            "no_new_privs" => {
                self.status_holds(
                    |status| match status.iter().find(|(k, _)| k == "NoNewPrivs") {
                        Some((_, v)) if v == "1" => Ok(()),
                        other => Err(format!("NoNewPrivs: {:?}", other.map(|(_, v)| v))),
                    },
                )
            }
            "seccomp" => {
                self.status_holds(|status| match status.iter().find(|(k, _)| k == "Seccomp") {
                    Some((_, v)) if v == "2" => Ok(()),
                    other => Err(format!("Seccomp: {:?}", other.map(|(_, v)| v))),
                })
            }
            "no_setuid_files" => self.setuid_files(),
            "su_sudo" => self.su_sudo(),
            "ssh_keys" => self.ssh_keys(),
            "agent_socket" => {
                if std::env::var_os("SSH_AUTH_SOCK").is_some() {
                    seen(AttemptOutcome::Escaped, "SSH_AUTH_SOCK is set")
                } else {
                    none_exist(
                        &AGENT_SOCKETS
                            .iter()
                            .map(|p| self.layout.at(p))
                            .collect::<Vec<_>>(),
                    )
                }
            }
            "git_credentials" => self.git_credentials(),
            "netrc" => none_exist(&self.under_homes(&[".netrc", "_netrc"])),
            "cloud_credentials" => none_exist(&self.under_homes(CLOUD_DIRS)),
            "engine_config" => {
                let mut paths = self.under_homes(&[HOME_ENGINE_CONFIG]);
                paths.push(self.layout.at(ENGINE_CONFIG_DIR));
                none_exist(&paths)
            }
            "environment_names" => self.environment_names(),
            "process_limit" => self.process_limit(),
            "memory_limit" => self.memory_limit(),
            code => match WRITE_TARGETS.iter().find(|(c, _)| *c == code) {
                Some((_, dir)) => self.write_attempt(&self.layout.at(dir)),
                None => seen(AttemptOutcome::Inconclusive, "this program cannot play it"),
            },
        }
    }
}

/// Runs phase `phase` of the battery `args` plan; exit 0 once every line is
/// printed, 64 on a usage error.
pub fn battery(layout: &Layout, args: &[String]) -> i32 {
    let started = Instant::now();
    let phase = match flag(args, "--phase").and_then(|p| p.parse::<u8>().ok()) {
        Some(phase @ (1 | 2)) => phase,
        _ => {
            eprintln!("officina-entry battery: --phase 1 or 2 is required");
            return 64;
        }
    };
    let plan =
        flag(args, "--plan").and_then(|json| parse_plan(&["--plan".to_owned(), json.to_owned()]));
    let Some(plan) = plan else {
        eprintln!("officina-entry battery: --plan is missing or does not parse");
        return 64;
    };
    let kind = if plan.login_mount.is_some() {
        CredentialKind::Login
    } else {
        CredentialKind::OauthToken
    };
    let battery = Battery {
        layout,
        targets: Targets::of(layout.at("/") != Path::new("/")),
        plan,
        started,
    };
    for family in BatteryFamily::ALL {
        let attempts: Vec<&Attempt> = attempts_for(kind, phase)
            .filter(|a| a.family == *family)
            .collect();
        if attempts.is_empty() {
            continue;
        }
        if let Some(control) = family.control()
            && let Some(line) = battery.control(*family, control)
        {
            print(&BatteryLine::Control(line));
        }
        for attempt in attempts {
            let (outcome, detail) = battery.play(attempt);
            print(&BatteryLine::Attempt(AttemptLine {
                family: attempt.family,
                attempt: attempt.code.to_owned(),
                expected: attempt.expected,
                outcome,
                detail,
            }));
        }
    }
    0
}

/// `officina-entry battery-hog <mib>`: touches `mib` MiB, one byte per
/// page, so the memory attempt's child is the one the limit kills.
pub fn hog(args: &[String]) -> i32 {
    let Some(mib) = args.get(1).and_then(|m| m.parse::<usize>().ok()) else {
        return 64;
    };
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    for _ in 0..mib {
        let mut block = vec![0u8; 1024 * 1024];
        for page in block.iter_mut().step_by(4096) {
            *page = 1;
        }
        blocks.push(std::hint::black_box(block));
    }
    i32::from(std::hint::black_box(&blocks).len() != mib)
}
