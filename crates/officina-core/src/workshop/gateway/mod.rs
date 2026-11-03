//! The gateway's rules (AD-103, D-234, D-242, D-611): the request, the
//! host and the allow-list, address classes, the ClientHello, the refusal
//! limiter, the arguments and the lines. `officina-gateway` (crate
//! `officina-container`) applies them; the host builds the arguments with
//! the same code, so the two cannot disagree on a rule.

pub mod addr;
pub mod tls;

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

/// The most of a request read before its header terminator (AD-103 step 1).
pub const MAX_REQUEST_BYTES: usize = 8 * 1024;
/// The only port a tunnel may name.
pub const TLS_PORT: u16 = 443;
/// Open tunnels beyond this answer `busy` (D-234); the gateway's
/// `--pids-limit` follows it (story 6.6).
pub const MAX_TUNNELS: usize = 64;
/// The longest host a `refused` line keeps.
pub const MAX_LINE_HOST_CHARS: usize = 253;
/// Where the gateway listens in its container.
pub const LISTEN: &str = "0.0.0.0:3128";
/// The refusals in [`REFUSAL_WINDOW_S`] that end the gateway (AD-103).
pub const REFUSAL_LIMIT: u32 = 20;
/// The limiter's sliding window, in seconds.
pub const REFUSAL_WINDOW_S: u64 = 300;
/// The gateway's exit code at the refusal limit (AD-109's `gateway_limit`).
pub const LIMIT_EXIT_CODE: i32 = 3;

/// The hosts every worker may reach, Claude Code's own (FR-4): the gate 3
/// entry decides the list (D-203); until it is recorded, the one host gate
/// 3's first half allowed (`api.anthropic.com`), and phase 2 refuses a
/// project host repeating one.
pub const CLAUDE_CODE_HOSTS: &[&str] = &["api.anthropic.com"];

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the gateway refused a request (AD-103).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum GatewayRefusal {
        Malformed,
        Oversized,
        Method,
        Port,
        IpLiteral,
        NotAllowed,
        PrivateAddress,
        NoIpv4,
        ResolveFailed,
        NotTls,
        SniMissing,
        SniMismatch,
        ConnectFailed,
        UpstreamRefused,
        Busy,
        Deadline,
        Limit,
    }
}

impl GatewayRefusal {
    /// The HTTP status the refusal answers before the gateway closes: 400
    /// for a request it cannot read, 405 for another method, 403 for the
    /// rest (AD-103).
    pub const fn status(self) -> u16 {
        match self {
            GatewayRefusal::Malformed | GatewayRefusal::Oversized => 400,
            GatewayRefusal::Method => 405,
            GatewayRefusal::Port
            | GatewayRefusal::IpLiteral
            | GatewayRefusal::NotAllowed
            | GatewayRefusal::PrivateAddress
            | GatewayRefusal::NoIpv4
            | GatewayRefusal::ResolveFailed
            | GatewayRefusal::NotTls
            | GatewayRefusal::SniMissing
            | GatewayRefusal::SniMismatch
            | GatewayRefusal::ConnectFailed
            | GatewayRefusal::UpstreamRefused
            | GatewayRefusal::Busy
            | GatewayRefusal::Deadline
            | GatewayRefusal::Limit => 403,
        }
    }
}

/// A `CONNECT` the gateway read: the host as written, its port, and the
/// bytes that followed the header terminator, which start the TLS buffer
/// (D-234, review F6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub host: String,
    pub port: u16,
    pub rest: Vec<u8>,
}

/// How far the bytes read so far go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestRead {
    Done(Request),
    /// No header terminator yet, within [`MAX_REQUEST_BYTES`].
    More,
    Refused(GatewayRefusal),
}

/// Reads a request from the bytes received so far (AD-103 step 1):
/// `CONNECT <host>:<port> HTTP/1.x` and its headers, nothing else.
pub fn parse_request(buf: &[u8]) -> RequestRead {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() >= MAX_REQUEST_BYTES {
            RequestRead::Refused(GatewayRefusal::Oversized)
        } else {
            RequestRead::More
        };
    };
    if end + 4 > MAX_REQUEST_BYTES {
        return RequestRead::Refused(GatewayRefusal::Oversized);
    }
    let Ok(head) = std::str::from_utf8(&buf[..end]) else {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    };
    let mut lines = head.split("\r\n");
    let line = lines.next().unwrap_or_default();
    if lines.any(|header| !header.contains(':') || header.chars().any(char::is_control)) {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    }
    let parts: Vec<&str> = line.split(' ').collect();
    let [method, authority, version] = parts.as_slice() else {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    };
    if *method != "CONNECT" {
        return RequestRead::Refused(if method.chars().all(|c| c.is_ascii_uppercase()) {
            GatewayRefusal::Method
        } else {
            GatewayRefusal::Malformed
        });
    }
    if !matches!(*version, "HTTP/1.0" | "HTTP/1.1") {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    }
    let Some((host, port)) = authority.rsplit_once(':') else {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    };
    let Ok(port) = port.parse::<u16>() else {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    };
    if host.is_empty() {
        return RequestRead::Refused(GatewayRefusal::Malformed);
    }
    RequestRead::Done(Request {
        host: host.to_owned(),
        port,
        rest: buf[end + 4..].to_vec(),
    })
}

/// True when `label` spells a number: decimal digits, or `0x` and hex
/// digits, the forms an IPv4 parser accepts.
fn is_numeric_label(label: &str) -> bool {
    let hex = label
        .strip_prefix("0x")
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_hexdigit()));
    hex || (!label.is_empty() && label.chars().all(|c| c.is_ascii_digit()))
}

/// The host a request names, normalised (AD-103 step 2): lowercased, a
/// trailing dot dropped, at most 253 characters of labels of 1 to 63
/// letters, digits and hyphens, never starting or ending with one. An IP
/// literal in any spelling (a bracketed or colon form, or a last label
/// that spells a number) is `ip_literal`; anything else not a name is
/// `malformed`.
pub fn normalise_host(host: &str) -> Result<String, GatewayRefusal> {
    if host.contains(':') || host.starts_with('[') {
        return Err(GatewayRefusal::IpLiteral);
    }
    let lower = host.to_ascii_lowercase();
    let name = lower.strip_suffix('.').unwrap_or(&lower);
    if name.is_empty() || name.len() > MAX_LINE_HOST_CHARS {
        return Err(GatewayRefusal::Malformed);
    }
    let labels: Vec<&str> = name.split('.').collect();
    if labels.last().is_some_and(|last| is_numeric_label(last)) {
        return Err(GatewayRefusal::IpLiteral);
    }
    let valid = labels.iter().all(|label| {
        (1..=63).contains(&label.len())
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    if valid {
        Ok(name.to_owned())
    } else {
        Err(GatewayRefusal::Malformed)
    }
}

/// The allow-list entry `host` (normalised) matches: an equal entry, or a
/// `*.suffix` entry with one or more labels before the suffix.
pub fn matching_entry<'a>(host: &str, allow: &'a [String]) -> Option<&'a str> {
    allow
        .iter()
        .find(|entry| match entry.strip_prefix("*.") {
            Some(suffix) => host
                .strip_suffix(suffix)
                .is_some_and(|head| head.len() > 1 && head.ends_with('.')),
            None => host == entry.as_str(),
        })
        .map(String::as_str)
}

/// A request checked against the rules of AD-103 steps 1 and 2: its host
/// normalised and the allow-list entry it matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    pub host: String,
    pub entry: String,
}

/// Checks `request` against `allow` (AD-103 step 2): the port, then the
/// host, then the allow-list.
pub fn admit(request: &Request, allow: &[String]) -> Result<Admitted, GatewayRefusal> {
    let host = normalise_host(&request.host)?;
    if request.port != TLS_PORT {
        return Err(GatewayRefusal::Port);
    }
    let entry = matching_entry(&host, allow).ok_or(GatewayRefusal::NotAllowed)?;
    Ok(Admitted {
        entry: entry.to_owned(),
        host,
    })
}

/// The refusal limiter (AD-103): a sliding window of refusal times; at
/// [`REFUSAL_LIMIT`] inside it, the gateway stops.
#[derive(Debug, Clone)]
pub struct Limiter {
    limit: u32,
    window_ms: i64,
    times: VecDeque<i64>,
}

impl Limiter {
    pub fn new(limit: u32, window_s: u64) -> Limiter {
        Limiter {
            limit,
            window_ms: i64::try_from(window_s.saturating_mul(1000)).unwrap_or(i64::MAX),
            times: VecDeque::new(),
        }
    }

    /// Counts one refusal at `now_ms`; true when the limit is reached.
    pub fn refused(&mut self, now_ms: i64) -> bool {
        while self
            .times
            .front()
            .is_some_and(|t| *t <= now_ms - self.window_ms)
        {
            self.times.pop_front();
        }
        self.times.push_back(now_ms);
        self.times.len() >= usize::try_from(self.limit).unwrap_or(usize::MAX)
    }
}

/// The gateway's configuration, its whole command line (AD-103, D-242,
/// D-611): no file, no environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayConfig {
    pub run: String,
    pub listen: String,
    /// [`CLAUDE_CODE_HOSTS`], then the project's hosts.
    pub allow: Vec<String>,
    /// Counted from the gateway's start (D-242).
    pub deadline_s: u64,
    pub refusal_limit: u32,
    pub refusal_window_s: u64,
    /// Per allow-list entry, the upstream proxy it goes through; an entry
    /// without one goes direct (D-611).
    pub routes: BTreeMap<String, String>,
}

/// Why a command line is not a gateway's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("unknown or incomplete argument {0}")]
    Unknown(String),
    #[error("{0} is missing")]
    Missing(&'static str),
    #[error("{0} is not a number")]
    NotANumber(String),
    #[error("the route {0} names no allow-list entry")]
    RouteWithoutEntry(String),
    #[error("the route {0} is not <entry>=<host:port>")]
    BadRoute(String),
}

impl GatewayConfig {
    /// A run's gateway: [`CLAUDE_CODE_HOSTS`] then `hosts`, the fixed
    /// limits, and `routes` as the window resolved them.
    pub fn new(
        run: &str,
        hosts: &[String],
        deadline_s: u64,
        routes: BTreeMap<String, String>,
    ) -> GatewayConfig {
        let mut allow: Vec<String> = CLAUDE_CODE_HOSTS.iter().map(|h| (*h).to_owned()).collect();
        allow.extend(
            hosts
                .iter()
                .filter(|h| !allow.contains(h))
                .cloned()
                .collect::<Vec<_>>(),
        );
        GatewayConfig {
            run: run.to_owned(),
            listen: LISTEN.to_owned(),
            allow,
            deadline_s,
            refusal_limit: REFUSAL_LIMIT,
            refusal_window_s: REFUSAL_WINDOW_S,
            routes,
        }
    }

    /// The command line after the program's name.
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![
            "--run".to_owned(),
            self.run.clone(),
            "--listen".to_owned(),
            self.listen.clone(),
        ];
        for host in &self.allow {
            args.push("--allow".to_owned());
            args.push(host.clone());
        }
        args.extend([
            "--deadline-s".to_owned(),
            self.deadline_s.to_string(),
            "--refusal-limit".to_owned(),
            self.refusal_limit.to_string(),
            "--refusal-window-s".to_owned(),
            self.refusal_window_s.to_string(),
        ]);
        for (entry, upstream) in &self.routes {
            args.push("--route".to_owned());
            args.push(format!("{entry}={upstream}"));
        }
        args
    }

    /// Parses [`GatewayConfig::to_args`]' output.
    pub fn from_args(args: &[String]) -> Result<GatewayConfig, ArgsError> {
        fn number<T: std::str::FromStr>(value: &str) -> Result<T, ArgsError> {
            value
                .parse()
                .map_err(|_| ArgsError::NotANumber(value.to_owned()))
        }
        let (mut run, mut listen, mut deadline_s, mut limit, mut window) =
            (None, None, None, None, None);
        let mut allow = Vec::new();
        let mut routes = BTreeMap::new();
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let value = it.next().ok_or_else(|| ArgsError::Unknown(flag.clone()))?;
            match flag.as_str() {
                "--run" => run = Some(value.clone()),
                "--listen" => listen = Some(value.clone()),
                "--allow" => allow.push(value.clone()),
                "--deadline-s" => deadline_s = Some(number(value)?),
                "--refusal-limit" => limit = Some(number(value)?),
                "--refusal-window-s" => window = Some(number(value)?),
                "--route" => {
                    let (entry, upstream) = value
                        .split_once('=')
                        .filter(|(e, u)| !e.is_empty() && u.contains(':'))
                        .ok_or_else(|| ArgsError::BadRoute(value.clone()))?;
                    routes.insert(entry.to_owned(), upstream.to_owned());
                }
                _ => return Err(ArgsError::Unknown(flag.clone())),
            }
        }
        if let Some(entry) = routes.keys().find(|entry| !allow.contains(entry)) {
            return Err(ArgsError::RouteWithoutEntry(entry.clone()));
        }
        Ok(GatewayConfig {
            run: run.ok_or(ArgsError::Missing("--run"))?,
            listen: listen.ok_or(ArgsError::Missing("--listen"))?,
            allow,
            deadline_s: deadline_s.ok_or(ArgsError::Missing("--deadline-s"))?,
            refusal_limit: limit.ok_or(ArgsError::Missing("--refusal-limit"))?,
            refusal_window_s: window.ok_or(ArgsError::Missing("--refusal-window-s"))?,
            routes,
        })
    }

    /// The SHA-256 of the configuration, which the `started` line carries.
    pub fn digest(&self) -> String {
        crate::sha256_hex(self.to_args().join("\n").as_bytes())
    }

    /// The distinct upstreams of the routes, in order.
    pub fn upstreams(&self) -> Vec<String> {
        let mut upstreams: Vec<String> = self.routes.values().cloned().collect();
        upstreams.sort();
        upstreams.dedup();
        upstreams
    }
}

/// Whether an upstream answered the gateway's probe at start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamState {
    Reachable,
    Unreachable,
}

/// One upstream and its state in the `started` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamProbe {
    pub upstream: String,
    pub state: UpstreamState,
}

/// One line of the gateway's stdout (AD-103), one JSON object each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "line", rename_all = "snake_case")]
pub enum GatewayLine {
    Started {
        digest: String,
        upstreams: Vec<UpstreamProbe>,
    },
    Allowed {
        host: String,
        port: u16,
        ech: bool,
    },
    Refused {
        host: String,
        port: u16,
        cause: GatewayRefusal,
        /// The upstream's status, for `upstream_refused`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<u16>,
    },
    Limit {},
}

impl GatewayLine {
    /// A `refused` line, its host made printable ASCII (anything else
    /// becomes `?`) and cut to [`MAX_LINE_HOST_CHARS`].
    pub fn refused(
        host: &str,
        port: u16,
        cause: GatewayRefusal,
        status: Option<u16>,
    ) -> GatewayLine {
        GatewayLine::Refused {
            host: printable(host),
            port,
            cause,
            status,
        }
    }

    /// The line as written: one JSON object.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// `text` as printable ASCII, cut to [`MAX_LINE_HOST_CHARS`].
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .take(MAX_LINE_HOST_CHARS)
        .collect()
}

/// The detail a refusal's line gives (D-611): an upstream answering 407
/// wants authentication, which v0.6 leaves out.
pub fn refusal_detail(cause: GatewayRefusal, status: Option<u16>) -> Option<&'static str> {
    (cause == GatewayRefusal::UpstreamRefused && status == Some(407))
        .then_some("proxy_auth_required")
}

/// The status code of an upstream's answer to `CONNECT`, from the bytes
/// up to its header terminator; `None` when it is not an HTTP status line.
pub fn upstream_status(head: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(head).ok()?;
    let line = text.split("\r\n").next()?;
    let mut parts = line.split(' ');
    let version = parts.next()?;
    if !version.starts_with("HTTP/1.") {
        return None;
    }
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limiter_trips_inside_the_window_only() {
        let mut limiter = Limiter::new(3, 10);
        assert!(!limiter.refused(0));
        assert!(!limiter.refused(1_000));
        assert!(!limiter.refused(10_000), "the first fell out of the window");
        assert!(limiter.refused(10_500));
    }

    fn config() -> GatewayConfig {
        GatewayConfig::new(
            "0123456789abcdef0123456789abcdef",
            &["index.crates.io".to_owned(), "*.example.com".to_owned()],
            3600,
            BTreeMap::from([(
                "index.crates.io".to_owned(),
                "proxy.example.org:3128".to_owned(),
            )]),
        )
    }

    #[test]
    fn args_round_trip() {
        let config = config();
        assert_eq!(GatewayConfig::from_args(&config.to_args()), Ok(config));
    }

    #[test]
    fn the_allow_list_starts_with_the_fixed_hosts() {
        let config = config();
        let fixed: Vec<String> = CLAUDE_CODE_HOSTS.iter().map(|h| (*h).to_owned()).collect();
        assert_eq!(config.allow[..fixed.len()], fixed[..]);
        let repeated = GatewayConfig::new("x", &fixed, 1, BTreeMap::new());
        assert_eq!(repeated.allow, fixed, "a fixed host is listed once");
    }

    #[test]
    fn a_refused_host_is_made_printable() {
        let long = "a".repeat(400);
        let GatewayLine::Refused { host, .. } = GatewayLine::refused(
            &format!("evil\u{202e}\n{long}"),
            443,
            GatewayRefusal::NotAllowed,
            None,
        ) else {
            panic!()
        };
        assert!(host.starts_with("evil??a"));
        assert_eq!(host.chars().count(), MAX_LINE_HOST_CHARS);
    }

    #[test]
    fn the_deadline_is_relative_seconds() {
        let args = config().to_args();
        let at = args.iter().position(|a| a == "--deadline-s").unwrap();
        assert_eq!(args[at + 1], "3600");
        assert!(!args.iter().any(|a| a == "--deadline-ms"));
    }

    #[test]
    fn a_route_must_name_an_allow_entry() {
        let mut args = config().to_args();
        args.extend([
            "--route".to_owned(),
            "elsewhere.example.org=proxy:3128".to_owned(),
        ]);
        assert_eq!(
            GatewayConfig::from_args(&args),
            Err(ArgsError::RouteWithoutEntry("elsewhere.example.org".into()))
        );
    }

    #[test]
    fn a_407_reads_proxy_auth_required() {
        assert_eq!(
            refusal_detail(GatewayRefusal::UpstreamRefused, Some(407)),
            Some("proxy_auth_required")
        );
        assert_eq!(
            refusal_detail(GatewayRefusal::UpstreamRefused, Some(502)),
            None
        );
        assert_eq!(
            upstream_status(b"HTTP/1.1 407 Proxy Authentication Required"),
            Some(407)
        );
        assert_eq!(upstream_status(b"SSH-2.0"), None);
    }
}
