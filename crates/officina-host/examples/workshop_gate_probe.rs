//! The owner's steps of the v0.6 workstation gates (story 6.1 AC 4, 6 to 9,
//! 11, 14, 15 and 16; D-201, D-202, D-610, D-613): the gates an agent cannot
//! run, because they need the owner's browser, his token, one of his folders
//! or the work network. Run by the owner, by hand, from his own console
//! (PowerShell or `cmd`, never Git Bash, which rewrites `docker` arguments);
//! never by a test, never part of a shipped binary.
//!
//! Build: `cargo build -p officina-host --example workshop_gate_probe`. It
//! needs only `docker.exe` (and `claude.exe` for its version, else
//! `--claude-version`), so `target\debug\examples\workshop_gate_probe.exe`
//! can be copied alone to a work machine. Subcommands:
//!
//! - `network [--proxy <host:port>] [--ca <file.pem>]`: the work network's
//!   W3. Builds the probe base through the proxy (Docker's proxy build
//!   arguments, the CA in the image's store), then runs `curl -v
//!   https://api.anthropic.com` as uid 1000 through the proxy and records
//!   the issuer and the outcome; asks whether this is the work floor's
//!   network.
//! - `login [--proxy <host:port>] [--ca <file.pem>]`: the login check
//!   (D-610 steps 1 to 5 and 7; step 6 is the agent's). `/login` inside the
//!   probe base, `auth status` with and without the network, a worker from
//!   the stored login behind a logging stub proxy, whether a run's copy
//!   changes, a second run, then logout and a run from a saved copy. On the
//!   work network, `--proxy` gives W4.
//! - `token`: gates 3, 4, 5 and 6, then question 10. Needs
//!   `CLAUDE_CODE_OAUTH_TOKEN` set in this console only (from `claude
//!   setup-token`); the probe passes it to the worker's `docker` by name,
//!   never prints it, and removes it from every other child.
//! - `mount`: gate 1's owner half. A git-ignored folder of one of the
//!   owner's projects mounted read-only; reads, a rewrite and a create; then,
//!   with his consent, a junction inside it pointing outside (D-236).
//!
//! Every step prints its exact `docker` command before running it, labels
//! every object `officina-probe=1`, asks a yes or no question where only the
//! owner sees the outcome, and appends the step, the command, the outcome
//! and the answer to `workshop-gate-probe-report.md` beside this executable.
//! Saved streams and logs go beside it too, for the agent to read. At the
//! end it lists and removes its probe objects (`--keep-images` keeps the two
//! images for the next subcommand). `--dry-run` prints the commands and
//! runs nothing. It reads no file under the Officina home or a Claude
//! config dir, never prints a token, and compares credential files only
//! inside a container, printing `same` or `different`.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use officina_core::dates::utc_date;
use officina_host::files::now_ms;

const USAGE: &str = "usage: workshop_gate_probe network|login|token|mount [--proxy <host:port>] [--ca <file.pem>] [--claude-version <x.y.z>] [--keep-images] [--dry-run]";

/// Every object the probe creates carries this label, and only those are
/// removed at the end.
const PROBE_LABEL: &str = "officina-probe=1";
const BASE_IMAGE: &str = "officina-probe-base:local";
const STUB_IMAGE: &str = "officina-probe-stub:local";
const INTERNAL_NET: &str = "officina-probe-int";
const EGRESS_NET: &str = "officina-probe-out";
const LOGIN_VOLUME: &str = "officina-probe-login";
const SAVED_VOLUME: &str = "officina-probe-login-saved";

/// The credential names a worker may carry (AD-105, D-603); every child but
/// the one worker `docker run` of a token gate runs without them.
const CREDENTIAL_NAMES: &[&str] = &["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY"];
const TOKEN_NAME: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Present in every process a Claude Code session starts: the probe refuses
/// to run there, since its output and its token would reach that session.
const SESSION_MARKERS: &[&str] = &["CLAUDECODE", "CLAUDE_CODE_SESSION_ID"];

/// The probe base, drafted to AD-99 (gate 1's first half, the gate 1
/// entry): the pinned Debian base, the packages sorted, the CA folder before
/// any HTTPS download (D-612), the documented installer at the host's
/// version in a build stage, uv by checksum, user `worker` uid 1000,
/// setuid and setgid bits removed. `/work/.officina/login` exists, owned by
/// uid 1000, so an empty login volume mounted there takes that owner
/// (D-605).
const PROBE_DOCKERFILE: &str = r#"FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4 AS base
RUN apt-get update \
 && apt-get install -y --no-install-recommends bash ca-certificates coreutils curl git python-is-python3 python3 \
 && rm -rf /var/lib/apt/lists/*
COPY ca/ /usr/local/share/ca-certificates/
RUN update-ca-certificates

FROM base AS claude
ARG CLAUDE_CODE_VERSION
RUN useradd -m -u 1000 builder
USER builder
WORKDIR /home/builder
RUN curl -fsSL https://claude.ai/install.sh -o install.sh \
 && bash install.sh "${CLAUDE_CODE_VERSION}" \
 && cp -L /home/builder/.local/bin/claude /home/builder/claude-bin

FROM base
ARG UV_VERSION=0.13.6
ARG UV_SHA256=f29c3e94ffabd7e6f3ba521dba169a9b7ec12100bf3257087f0f24f6123f3645
RUN curl -fsSL -o /tmp/uv.tar.gz "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-x86_64-unknown-linux-gnu.tar.gz" \
 && echo "${UV_SHA256}  /tmp/uv.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/uv.tar.gz -C /usr/local/bin --strip-components=1 \
 && rm /tmp/uv.tar.gz
COPY --from=claude --chown=0:0 --chmod=0755 /home/builder/claude-bin /opt/officina/bin/claude
RUN useradd -m -u 1000 -s /bin/bash worker \
 && mkdir -p /work/.officina/in /work/.officina/out /work/.officina/home /work/.officina/config /work/.officina/mounts /work/.officina/login \
 && chown -R worker:worker /work \
 && (find / -xdev -perm /6000 -type f -exec chmod a-s {} + || true)
ENV PATH=/opt/officina/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt \
    NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt \
    UV_NATIVE_TLS=1
WORKDIR /work
USER worker
"#;

/// The stub CONNECT proxy (D-202): allow-list and logging only, never the
/// gateway. `any` allows every host (used for builds, to log the hosts);
/// `--upstream host:port` sends each allowed tunnel through that proxy, for
/// a network with no direct route out.
const STUB_PY: &str = r#"import socket, sys, threading, time
args = sys.argv[1:]
upstream = None
if len(args) >= 2 and args[0] == "--upstream":
    h, _, p = args[1].rpartition(":")
    upstream = (h, int(p))
    args = args[2:]
allow = [a.lower() for a in args]
def log(line):
    sys.stdout.write(time.strftime("%H:%M:%S ") + line + "\n")
    sys.stdout.flush()
def allowed(host):
    return any(a == "any" or host == a or (a.startswith("*.") and host.endswith(a[1:])) for a in allow)
def pump(src, dst):
    try:
        while True:
            b = src.recv(65536)
            if not b:
                break
            dst.sendall(b)
    except OSError:
        pass
    finally:
        for s in (src, dst):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
def open_tunnel(host, port):
    if upstream is None:
        return socket.create_connection((host, port), timeout=15)
    u = socket.create_connection(upstream, timeout=15)
    u.sendall(("CONNECT %s:%d HTTP/1.1\r\nHost: %s:%d\r\n\r\n" % (host, port, host, port)).encode())
    data = b""
    while b"\r\n\r\n" not in data and len(data) < 8192:
        b = u.recv(4096)
        if not b:
            break
        data += b
    status = data.split(b"\r\n")[0].decode("latin-1")
    if " 200" not in status:
        u.close()
        raise OSError("upstream answered " + status)
    return u
def handle(c):
    try:
        c.settimeout(15)
        data = b""
        while b"\r\n\r\n" not in data and len(data) < 8192:
            b = c.recv(4096)
            if not b:
                return
            data += b
        head, _, rest = data.partition(b"\r\n\r\n")
        parts = head.split(b"\r\n")[0].decode("latin-1").split(" ")
        if len(parts) != 3:
            log("REFUSED malformed")
            c.sendall(b"HTTP/1.1 400 Bad Request\r\n\r\n")
            return
        method, target, _ = parts
        if method != "CONNECT":
            log("REFUSED method " + method)
            c.sendall(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")
            return
        host, _, port = target.rpartition(":")
        host = host.strip("[]").lower()
        if not allowed(host):
            log("REFUSED " + host[:253] + ":" + port)
            c.sendall(b"HTTP/1.1 403 Forbidden\r\n\r\n")
            return
        try:
            u = open_tunnel(host, int(port))
        except (OSError, ValueError) as e:
            log("FAILED " + host + ":" + port + " " + str(e)[:120])
            c.sendall(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
            return
        log("ALLOWED " + host + ":" + port)
        c.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
        if rest:
            u.sendall(rest)
        c.settimeout(None)
        u.settimeout(None)
        threading.Thread(target=pump, args=(u, c), daemon=True).start()
        pump(c, u)
    except OSError:
        pass
    finally:
        c.close()
srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("0.0.0.0", 3128))
srv.listen(64)
log("LISTENING")
while True:
    conn, _ = srv.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
"#;

const STUB_DOCKERFILE: &str = r#"FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates python3 && rm -rf /var/lib/apt/lists/*
COPY stub.py /stub.py
USER 65534:65534
ENTRYPOINT ["python3", "-u", "/stub.py"]
"#;

/// What a worker container runs (gates 3 to 6, login steps 3 to 5): a
/// scratch repository cloned into the work volume with a git-ignored
/// `.claude`, either an empty folder or a link to the read-only mount
/// (gate 4, AD-108 step 2), the credential files copied from the login
/// mount when `PROBE_CREDENTIALS` names them (D-606), then FR-7's argument
/// vector with the prompt on stdin. The stream is stdout; `PROBE` lines on
/// stderr carry the exit code and the commit count.
const RUN_SH: &str = r#"#!/bin/bash
set -u
mkdir -p "$HOME" "$CLAUDE_CONFIG_DIR"
for f in ${PROBE_CREDENTIALS:-}; do
  if [ -f "/work/.officina/login/$f" ]; then
    cp "/work/.officina/login/$f" "$CLAUDE_CONFIG_DIR/$f" && chmod 600 "$CLAUDE_CONFIG_DIR/$f"
  else
    echo "PROBE missing_credential_file=$f" >&2
  fi
done
git init -q -b main /work/seed
cd /work/seed
printf '# Probe\n' > README.md
printf '.claude/\n' > .gitignore
git add README.md .gitignore
git -c user.name=probe -c user.email=probe@example.com commit -q -m base
git clone -q /work/seed /work/repo
cd /work/repo
if [ -d /work/.officina/mounts/0 ]; then
  ln -s /work/.officina/mounts/0 /work/repo/.claude
  echo ".claude" >> .git/info/exclude
else
  mkdir .claude
fi
budget=()
if [ -n "${PROBE_BUDGET:-}" ]; then budget=(--max-budget-usd "$PROBE_BUDGET"); fi
timeout --kill-after=30 900 /opt/officina/bin/claude -p --output-format stream-json --verbose --include-hook-events --permission-mode bypassPermissions --permission-prompts none --disallowedTools WebSearch,WebFetch "${budget[@]}" < /probe/prompt.txt
code=$?
echo "PROBE exit=$code" >&2
echo "PROBE commits=$(git rev-list --count HEAD)" >&2
echo "PROBE dirty=$(git status --porcelain | wc -l)" >&2
exit $code
"#;

const PROMPT_COMMIT: &str = "Create a file named hello.txt containing the word hello, run git status, then commit it with the message 'probe: hello'. Reply with the commit's short id.\n";
const PROMPT_BUDGET: &str = "Do these steps one at a time, each with its own tool call: create step1.txt to step8.txt, each with a different sentence about bridges; after each file run git status; then read every file back and write a summary.md of all eight; then commit everything with the message 'probe: budget'.\n";
const PROMPT_TOOL: &str = "Run git status with the Bash tool and reply with its first line.\n";

/// The hook events of gate 4 (AD-126 row 4).
const GATE4_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SessionEnd",
];

/// What a read-only mount must refuse (gate 1's owner half): reads, a
/// rewrite of an existing file, a create, and placeholder files counted.
const MOUNT_CHECK_PY: &str = r#"import os, sys
root = sys.argv[1]
files = []
for d, dirs, names in os.walk(root):
    dirs[:] = [x for x in dirs if x != "officina-probe-junction"]
    for n in names:
        files.append(os.path.join(d, n))
        if len(files) >= 5000:
            break
    if len(files) >= 5000:
        break
ok = failed = 0
for p in files[:500]:
    try:
        with open(p, "rb") as f:
            f.read(64)
        ok += 1
    except OSError:
        failed += 1
print("files seen: %d, read ok: %d, read failed: %d" % (len(files), ok, failed))
if files:
    try:
        with open(files[0], "ab") as f:
            f.write(b"")
        print("rewrite: SUCCEEDED")
    except OSError as e:
        print("rewrite: " + e.strerror)
try:
    with open(os.path.join(root, "officina-probe-create.txt"), "w") as f:
        f.write("x")
    print("create: SUCCEEDED")
except OSError as e:
    print("create: " + e.strerror)
link = os.path.join(root, "officina-probe-junction", "link")
if os.path.lexists(link):
    try:
        print("junction entries: " + ",".join(sorted(os.listdir(link))))
    except OSError as e:
        print("junction: " + e.strerror)
"#;

/// The outcome of one stream-json run, from fields only: names, counts and
/// the values a gate entry records (subtypes, error categories, exit codes,
/// cost), never a message text.
#[derive(Debug, Default, PartialEq)]
struct StreamSummary {
    lines: usize,
    not_json: usize,
    init_model: bool,
    init_tools: Option<usize>,
    init_mcp_servers: Option<usize>,
    init_permission_mode: Option<String>,
    api_retry_errors: Vec<String>,
    /// `(subtype, event name, exit code)` per hook line, the event name and
    /// exit code as the stream names them, if it does.
    hook_lines: Vec<(String, Option<String>, Option<i64>)>,
    /// The key names seen on hook lines, for gate 5's field list.
    hook_keys: BTreeSet<String>,
    result_subtype: Option<String>,
    result_is_error: Option<bool>,
    result_cost_usd: Option<f64>,
    result_turns: Option<i64>,
    result_duration: bool,
    result_session_id: bool,
    result_terminal_reason: Option<String>,
    result_api_error_status: Option<i64>,
}

impl StreamSummary {
    fn from_stream(text: &str) -> StreamSummary {
        let mut s = StreamSummary::default();
        for raw in text.lines().filter(|l| !l.trim().is_empty()) {
            s.lines += 1;
            let Ok(serde_json::Value::Object(o)) = serde_json::from_str::<serde_json::Value>(raw)
            else {
                s.not_json += 1;
                continue;
            };
            let kind = o.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let subtype = o.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
            match (kind, subtype) {
                ("system", "init") if s.init_tools.is_none() => {
                    s.init_model = o.get("model").is_some_and(|v| v.is_string());
                    s.init_tools = o.get("tools").and_then(|v| v.as_array()).map(Vec::len);
                    s.init_mcp_servers = o
                        .get("mcp_servers")
                        .and_then(|v| v.as_array())
                        .map(Vec::len);
                    s.init_permission_mode = o
                        .get("permissionMode")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                }
                ("system", "api_retry") => s.api_retry_errors.push(
                    o.get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(none)")
                        .to_owned(),
                ),
                ("system", sub) if sub.contains("hook") => {
                    s.hook_keys.extend(o.keys().cloned());
                    let event = ["hook_event_name", "hook_event", "event", "hook_name"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(|v| v.as_str()))
                        .map(str::to_owned);
                    let exit = ["exit_code", "exitCode"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(serde_json::Value::as_i64));
                    s.hook_lines.push((sub.to_owned(), event, exit));
                }
                ("result", sub) => {
                    s.result_subtype = Some(sub.to_owned());
                    s.result_is_error = o.get("is_error").and_then(serde_json::Value::as_bool);
                    s.result_cost_usd = o.get("total_cost_usd").and_then(serde_json::Value::as_f64);
                    s.result_turns = o.get("num_turns").and_then(serde_json::Value::as_i64);
                    s.result_duration = o.get("duration_ms").is_some();
                    s.result_session_id = o.get("session_id").is_some_and(|v| v.is_string());
                    s.result_terminal_reason = o
                        .get("terminal_reason")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    s.result_api_error_status = o
                        .get("api_error_status")
                        .and_then(serde_json::Value::as_i64);
                }
                _ => {}
            }
        }
        s
    }

    fn report_lines(&self) -> Vec<String> {
        let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "absent".to_owned());
        let mut out = vec![
            format!("lines: {} (not JSON: {})", self.lines, self.not_json),
            format!(
                "system/init: model {}, tools {}, MCP servers {}, permission mode {}",
                if self.init_model { "present" } else { "absent" },
                self.init_tools
                    .map_or("absent".to_owned(), |n| n.to_string()),
                self.init_mcp_servers
                    .map_or("absent".to_owned(), |n| n.to_string()),
                opt(&self.init_permission_mode)
            ),
            format!(
                "system/api_retry errors: {}",
                if self.api_retry_errors.is_empty() {
                    "none".to_owned()
                } else {
                    self.api_retry_errors.join(", ")
                }
            ),
            format!(
                "result: subtype {}, is_error {}, cost {}, turns {}, duration {}, session id {}, terminal reason {}, api error status {}",
                opt(&self.result_subtype),
                self.result_is_error
                    .map_or("absent".to_owned(), |b| b.to_string()),
                self.result_cost_usd
                    .map_or("absent".to_owned(), |c| format!("{c:.4} USD")),
                self.result_turns
                    .map_or("absent".to_owned(), |n| n.to_string()),
                if self.result_duration {
                    "present"
                } else {
                    "absent"
                },
                if self.result_session_id {
                    "present"
                } else {
                    "absent"
                },
                opt(&self.result_terminal_reason),
                self.result_api_error_status
                    .map_or("absent".to_owned(), |n| n.to_string())
            ),
        ];
        out.push(format!("hook lines: {}", self.hook_lines.len()));
        for (subtype, event, exit) in &self.hook_lines {
            out.push(format!(
                "  {subtype} event {} exit {}",
                event.as_deref().unwrap_or("?"),
                exit.map_or("absent".to_owned(), |c| c.to_string())
            ));
        }
        if !self.hook_keys.is_empty() {
            out.push(format!(
                "hook line keys: {}",
                self.hook_keys.iter().cloned().collect::<Vec<_>>().join(" ")
            ));
        }
        out
    }
}

/// The hosts a stub log allowed and refused (D-610 step 7), each once,
/// sorted.
fn stub_hosts(log: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut allowed = BTreeSet::new();
    let mut refused = BTreeSet::new();
    for line in log.lines() {
        let mut words = line.split_whitespace().skip(1);
        match (words.next(), words.next()) {
            (Some("ALLOWED"), Some(target)) => {
                allowed.insert(target.to_owned());
            }
            (Some("REFUSED"), Some(target)) => {
                refused.insert(target.to_owned());
            }
            _ => {}
        }
    }
    (allowed, refused)
}

/// `claude auth status`'s answer reduced to what the login entry records:
/// the exit code, `loggedIn` and `authMethod`. Every other field (the email,
/// the organisation) is dropped here and never printed.
fn auth_status_summary(stdout: &str, exit: Option<i32>) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();
    let field = |name: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(name))
            .map_or("absent".to_owned(), |v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
    };
    format!(
        "exit {}, loggedIn {}, authMethod {}",
        exit.map_or("none".to_owned(), |c| c.to_string()),
        field("loggedIn"),
        field("authMethod")
    )
}

/// `host:port` with a non-empty host free of spaces, schemes and commas,
/// and a port that parses.
fn parse_proxy(text: &str) -> Option<String> {
    let text = text.strip_prefix("http://").unwrap_or(text);
    let text = text.trim_end_matches('/');
    let (host, port) = text.rsplit_once(':')?;
    let valid_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    (valid_host && port.parse::<u16>().is_ok_and(|p| p > 0)).then(|| format!("{host}:{port}"))
}

/// Docker's predefined proxy build arguments (D-611), kept out of the
/// image's history by the engine.
fn proxy_build_args(proxy: Option<&str>) -> Vec<String> {
    let Some(proxy) = proxy else {
        return Vec::new();
    };
    let url = format!("http://{proxy}");
    let mut args = Vec::new();
    for name in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        args.push("--build-arg".to_owned());
        args.push(format!("{name}={url}"));
    }
    args.push("--build-arg".to_owned());
    args.push("NO_PROXY=localhost,127.0.0.1,::1".to_owned());
    args
}

/// The proxy variables in both cases (addendum section 1 item 6) and
/// `NO_PROXY`, as `--env` arguments of a container.
fn proxy_env_args(url: &str) -> Vec<String> {
    let mut args = Vec::new();
    for name in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        args.push("--env".to_owned());
        args.push(format!("{name}={url}"));
    }
    for name in ["NO_PROXY", "no_proxy"] {
        args.push("--env".to_owned());
        args.push(format!("{name}=localhost,127.0.0.1,::1"));
    }
    args
}

/// AD-102's fixed options with the `Worker` limits, a tmpfs `/tmp` and the
/// `local` log driver (AD-98).
fn worker_options() -> Vec<String> {
    [
        "--label",
        PROBE_LABEL,
        "--user",
        "1000:1000",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--init",
        "--stop-timeout",
        "3",
        "--pids-limit",
        "1024",
        "--memory",
        "2g",
        "--memory-swap",
        "2g",
        "--cpus",
        "2",
        "--log-driver",
        "local",
        "--log-opt",
        "max-size=32m",
        "--log-opt",
        "max-file=3",
        "--tmpfs",
        "/tmp:size=512m",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

/// AD-102's worker environment for a probe run: the work volume's home and
/// config dir, the gateway as proxy, Claude Code's quiet switches and the
/// commit identity.
fn worker_env(proxy_url: &str) -> Vec<String> {
    let mut args = Vec::new();
    for pair in [
        "HOME=/work/.officina/home",
        "CLAUDE_CONFIG_DIR=/work/.officina/config",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1",
        "DISABLE_AUTOUPDATER=1",
        "CLAUDE_CODE_DISABLE_ARTIFACT=1",
        "CLAUDE_CODE_DISABLE_AGENT_VIEW=1",
        "GIT_AUTHOR_NAME=probe",
        "GIT_AUTHOR_EMAIL=probe@example.com",
        "GIT_COMMITTER_NAME=probe",
        "GIT_COMMITTER_EMAIL=probe@example.com",
    ] {
        args.push("--env".to_owned());
        args.push(pair.to_owned());
    }
    args.extend(proxy_env_args(proxy_url));
    args
}

/// The `--mount` value of a bind mount; `None` when the path holds a
/// character `--mount` would read as a field separator or quote (AD-102).
fn bind_mount(source: &Path, target: &str, readonly: bool) -> Option<String> {
    let source = source.to_str()?;
    if source.contains([',', '"']) || source.chars().any(char::is_control) {
        return None;
    }
    let ro = if readonly { ",readonly" } else { "" };
    Some(format!("type=bind,src={source},dst={target}{ro}"))
}

fn volume_mount(volume: &str, target: &str, readonly: bool) -> String {
    let ro = if readonly { ",readonly" } else { "" };
    format!("type=volume,src={volume},dst={target}{ro}")
}

/// The `PROBE name=value` lines a worker's stderr ends with.
fn probe_value(stderr: &str, name: &str) -> Option<String> {
    let prefix = format!("PROBE {name}=");
    stderr
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(&prefix).map(str::to_owned))
}

/// The issuer and the HTTP status line of a `curl -v` transcript.
fn curl_summary(stderr: &str) -> (Option<String>, Option<String>) {
    let issuer = stderr.lines().find_map(|l| {
        l.trim()
            .strip_prefix("*  issuer:")
            .map(|s| s.trim().to_owned())
    });
    let status = stderr.lines().rev().find_map(|l| {
        l.trim()
            .strip_prefix("< HTTP/")
            .map(|s| format!("HTTP/{}", s.trim()))
    });
    (issuer, status)
}

/// A command line as the owner could paste it: arguments with spaces or
/// quotes quoted.
fn quoted(program: &str, args: &[String]) -> String {
    let mut line = program.to_owned();
    for arg in args {
        line.push(' ');
        if arg.is_empty() || arg.contains([' ', '"', '\t']) {
            let _ = write!(line, "\"{}\"", arg.replace('"', "\\\""));
        } else {
            line.push_str(arg);
        }
    }
    line
}

/// The gate 4 settings file: one hook per event, each appending its event's
/// name to `$HOME/hooks-gate.log` on the work volume.
fn gate4_settings() -> String {
    let mut hooks = serde_json::Map::new();
    for event in GATE4_EVENTS {
        let command = format!("echo {event} >> \"$HOME/hooks-gate.log\"");
        let mut entry = serde_json::json!({ "hooks": [{ "type": "command", "command": command }] });
        if matches!(*event, "PreToolUse" | "PostToolUse") {
            entry["matcher"] = serde_json::json!("*");
        }
        hooks.insert((*event).to_owned(), serde_json::json!([entry]));
    }
    serde_json::to_string_pretty(&serde_json::json!({ "hooks": hooks })).unwrap_or_default()
}

/// Gate 5's settings file: a `PreToolUse` hook whose program does not exist.
fn gate5_settings() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "hooks": { "PreToolUse": [{ "matcher": "*", "hooks": [
            { "type": "command", "command": "officina-probe-missing-program --check" }
        ] }] }
    }))
    .unwrap_or_default()
}

/// Lines per event in the gate 4 log, in [`GATE4_EVENTS`] order.
fn hook_log_counts(log: &str) -> Vec<(&'static str, usize)> {
    GATE4_EVENTS
        .iter()
        .map(|event| (*event, log.lines().filter(|l| l.trim() == *event).count()))
        .collect()
}

/// `find -printf '%m %P'` lines: the modes and names of the files in the
/// login volume, nothing else (D-610 step 1).
fn files_listing(find_output: &str) -> Vec<String> {
    let mut lines: Vec<String> = find_output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    lines.sort();
    lines
}

struct Options {
    proxy: Option<String>,
    ca: Option<PathBuf>,
    claude_version: Option<String>,
    keep_images: bool,
    dry_run: bool,
}

struct Probe {
    docker: PathBuf,
    /// This console's environment without the credential names: every
    /// child's, except the one worker run of a token gate.
    env: Vec<(String, String)>,
    has_token: bool,
    dir: PathBuf,
    scratch: PathBuf,
    report: String,
    opts: Options,
    profile: Option<String>,
}

struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Probe {
    fn redact(&self, text: &str) -> String {
        match &self.profile {
            Some(p) if !p.is_empty() => text
                .replace(p.as_str(), "%USERPROFILE%")
                .replace(&p.replace('\\', "/"), "%USERPROFILE%"),
            _ => text.to_owned(),
        }
    }

    fn line(&mut self, text: &str) {
        let text = self.redact(text);
        println!("{text}");
        self.report.push_str(&text);
        self.report.push('\n');
    }

    fn step(&mut self, title: &str) {
        let title = format!("\n## {title}\n");
        self.line(&title);
    }

    fn block(&mut self, title: &str, text: &str) {
        let text = self.redact(text);
        println!("{title}:\n{}", text.trim_end());
        let _ = write!(
            self.report,
            "\n{title}:\n\n```\n{}\n```\n\n",
            text.trim_end()
        );
    }

    fn save(&mut self, name: &str, content: &str) {
        let path = self.dir.join(name);
        match std::fs::write(&path, content) {
            Ok(()) => self.line(&format!("- saved `{name}` beside the probe")),
            Err(e) => self.line(&format!("- could not save `{name}`: {e}")),
        }
    }

    fn write_report(&self) {
        let path = self.dir.join("workshop-gate-probe-report.md");
        let previous = std::fs::read_to_string(&path).unwrap_or_default();
        match std::fs::write(&path, format!("{previous}{}", self.report)) {
            Ok(()) => println!("\nReport appended to {}", path.display()),
            Err(e) => eprintln!("workshop_gate_probe: cannot write {}: {e}", path.display()),
        }
    }

    /// Runs `docker <args>` with this console's environment minus the
    /// credential names, unless `with_token`; prints the command first.
    #[expect(
        clippy::disallowed_methods,
        reason = "a workstation probe drives docker by hand, outside the engine driver (D-128)"
    )]
    fn docker(&mut self, args: &[String], with_token: bool, interactive: bool) -> Output {
        self.line(&format!("\n`{}`", quoted("docker", args)));
        if self.opts.dry_run {
            return Output {
                code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
        let mut command = Command::new(&self.docker);
        command
            .args(args)
            .current_dir(&self.scratch)
            .env_clear()
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .env("MSYS_NO_PATHCONV", "1");
        if with_token && let Ok(value) = std::env::var(TOKEN_NAME) {
            command.env(TOKEN_NAME, value);
        }
        if interactive {
            let status = command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status();
            return Output {
                code: status.ok().and_then(|s| s.code()),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
        match command.stdin(Stdio::null()).output() {
            Ok(out) => Output {
                code: out.status.code(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            },
            Err(e) => Output {
                code: None,
                stdout: String::new(),
                stderr: format!("docker could not start: {e}"),
            },
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn versions(&mut self) {
        let out = self.docker(
            &Self::args(&["version", "--format", "{{json .}}"]),
            false,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap_or_default();
        let server = &v["Server"];
        self.line(&format!(
            "- Docker Desktop: {}; engine {}; kernel {}; OS {}",
            server["Platform"]["Name"].as_str().unwrap_or("unknown"),
            server["Version"].as_str().unwrap_or("unknown"),
            server["KernelVersion"].as_str().unwrap_or("unknown"),
            server["Os"].as_str().unwrap_or("unknown"),
        ));
        self.line(&format!(
            "- Claude Code in the image: {}",
            self.claude_version()
        ));
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "reads the host claude's version, outside spawn (D-128)"
    )]
    fn claude_version(&self) -> String {
        if let Some(v) = &self.opts.claude_version {
            return v.clone();
        }
        Command::new("claude.exe")
            .arg("--version")
            .env_clear()
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .next()
                    .map(str::to_owned)
            })
            .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or_else(|| "stable".to_owned())
    }

    /// Writes the build contexts and builds the probe base and the stub
    /// image, through the proxy when one is given.
    fn build_images(&mut self) -> bool {
        self.step("Images: the probe base (AD-99 draft) and the stub proxy");
        let base_dir = self.scratch.join("base");
        let ca_dir = base_dir.join("ca");
        let stub_dir = self.scratch.join("stub");
        let written = std::fs::create_dir_all(&ca_dir)
            .and_then(|()| std::fs::create_dir_all(&stub_dir))
            .and_then(|()| std::fs::write(base_dir.join("Dockerfile"), PROBE_DOCKERFILE))
            .and_then(|()| std::fs::write(stub_dir.join("Dockerfile"), STUB_DOCKERFILE))
            .and_then(|()| std::fs::write(stub_dir.join("stub.py"), STUB_PY));
        if let Err(e) = written {
            self.line(&format!("- the build contexts could not be written: {e}"));
            return false;
        }
        if let Some(ca) = self.opts.ca.clone() {
            match std::fs::read_to_string(&ca) {
                Ok(pem) if pem.contains("-----BEGIN CERTIFICATE-----") => {
                    if let Err(e) = std::fs::write(ca_dir.join("officina-ca.crt"), pem) {
                        self.line(&format!("- the CA could not be copied: {e}"));
                        return false;
                    }
                    self.line("- CA: a PEM certificate file, baked into the image's store");
                }
                _ => {
                    self.line("- CA: the file is not a PEM certificate; stopping");
                    return false;
                }
            }
        }
        let version = self.claude_version();
        let mut args = Self::args(&["build", "--progress", "plain", "--label", PROBE_LABEL]);
        args.push("--build-arg".to_owned());
        args.push(format!("CLAUDE_CODE_VERSION={version}"));
        args.extend(proxy_build_args(self.opts.proxy.as_deref()));
        args.extend(Self::args(&["-t", BASE_IMAGE]));
        args.push(base_dir.to_string_lossy().into_owned());
        let out = self.docker(&args, false, false);
        let tail: String = out
            .stderr
            .lines()
            .rev()
            .take(15)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        if out.code != Some(0) {
            self.block("Base build failed, last lines", &tail);
            return false;
        }
        self.line("- probe base built");
        let mut args = Self::args(&["build", "-q", "--label", PROBE_LABEL]);
        args.extend(proxy_build_args(self.opts.proxy.as_deref()));
        args.extend(Self::args(&["-t", STUB_IMAGE]));
        args.push(stub_dir.to_string_lossy().into_owned());
        let out = self.docker(&args, false, false);
        if out.code != Some(0) {
            self.block("Stub build failed", &out.stderr);
            return false;
        }
        self.line("- stub image built");
        true
    }

    fn networks(&mut self) {
        self.docker(
            &Self::args(&[
                "network",
                "create",
                "--internal",
                "--label",
                PROBE_LABEL,
                INTERNAL_NET,
            ]),
            false,
            false,
        );
        self.docker(
            &Self::args(&["network", "create", "--label", PROBE_LABEL, EGRESS_NET]),
            false,
            false,
        );
    }

    /// Starts a stub on the egress network, connected to the internal one,
    /// allowing `hosts` (and going through `--proxy` when given).
    fn start_stub(&mut self, name: &str, hosts: &[&str]) {
        let mut args = Self::args(&[
            "run",
            "-d",
            "--name",
            name,
            "--label",
            PROBE_LABEL,
            "--network",
            EGRESS_NET,
            STUB_IMAGE,
        ]);
        if let Some(proxy) = self.opts.proxy.clone() {
            args.push("--upstream".to_owned());
            args.push(proxy);
        }
        args.extend(hosts.iter().map(|h| (*h).to_owned()));
        self.docker(&args, false, false);
        self.docker(
            &Self::args(&["network", "connect", INTERNAL_NET, name]),
            false,
            false,
        );
    }

    fn stub_report(&mut self, name: &str) -> BTreeSet<String> {
        let out = self.docker(&Self::args(&["logs", name]), false, false);
        let (allowed, refused) = stub_hosts(&out.stdout);
        let join = |s: &BTreeSet<String>| {
            if s.is_empty() {
                "none".to_owned()
            } else {
                s.iter().cloned().collect::<Vec<_>>().join(", ")
            }
        };
        self.line(&format!("- stub allowed: {}", join(&allowed)));
        self.line(&format!("- stub refused: {}", join(&refused)));
        refused
    }

    /// One worker run: a fresh work volume, the probe folder (script,
    /// prompt) mounted read-only, the optional read-only `.claude` mount and
    /// login volume; the stream saved; the hooks log copied out before
    /// `docker rm` (gate 4, D-202).
    #[expect(
        clippy::too_many_arguments,
        reason = "one probe run, every input named at the call"
    )]
    fn worker_run(
        &mut self,
        tag: &str,
        stub: &str,
        prompt: &str,
        claude_mount: Option<&str>,
        login: Option<(&str, &str)>,
        budget: Option<&str>,
        with_token: bool,
    ) -> (StreamSummary, Output) {
        let probe_dir = self.scratch.join(format!("probe-{tag}"));
        let wrote = std::fs::create_dir_all(&probe_dir)
            .and_then(|()| std::fs::write(probe_dir.join("run.sh"), RUN_SH))
            .and_then(|()| std::fs::write(probe_dir.join("prompt.txt"), prompt));
        if let Err(e) = wrote {
            self.line(&format!("- the probe folder could not be written: {e}"));
            return (
                StreamSummary::default(),
                Output {
                    code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            );
        }
        let name = format!("officina-probe-worker-{tag}");
        let volume = format!("officina-probe-work-{tag}");
        self.docker(
            &Self::args(&["volume", "create", "--label", PROBE_LABEL, &volume]),
            false,
            false,
        );
        let mut args = Self::args(&["run", "--name", &name, "--network", INTERNAL_NET]);
        args.extend(worker_options());
        args.extend(worker_env(&format!("http://{stub}:3128")));
        if with_token {
            args.push("--env".to_owned());
            args.push(TOKEN_NAME.to_owned());
        }
        if let Some((_, files)) = login {
            args.push("--env".to_owned());
            args.push(format!("PROBE_CREDENTIALS={files}"));
        }
        if let Some(budget) = budget {
            args.push("--env".to_owned());
            args.push(format!("PROBE_BUDGET={budget}"));
        }
        args.push("--mount".to_owned());
        args.push(volume_mount(&volume, "/work", false));
        let Some(probe_mount) = bind_mount(&probe_dir, "/probe", true) else {
            self.line("- the scratch folder's path cannot be mounted");
            return (
                StreamSummary::default(),
                Output {
                    code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            );
        };
        args.push("--mount".to_owned());
        args.push(probe_mount);
        if let Some(settings) = claude_mount {
            let claude_dir = self.scratch.join(format!("claude-{tag}"));
            let _ = std::fs::create_dir_all(&claude_dir);
            let _ = std::fs::write(claude_dir.join("settings.json"), settings);
            if let Some(mount) = bind_mount(&claude_dir, "/work/.officina/mounts/0", true) {
                args.push("--mount".to_owned());
                args.push(mount);
            }
        }
        if let Some((volume, _)) = login {
            args.push("--mount".to_owned());
            args.push(volume_mount(volume, "/work/.officina/login", true));
        }
        args.extend(Self::args(&[
            "--entrypoint",
            "/bin/bash",
            BASE_IMAGE,
            "/probe/run.sh",
        ]));
        let out = self.docker(&args, with_token, false);
        let summary = StreamSummary::from_stream(&out.stdout);
        for l in summary.report_lines() {
            self.line(&format!("- {l}"));
        }
        self.line(&format!(
            "- worker exit {}, commits {} (1 is the base), uncommitted files {}",
            probe_value(&out.stderr, "exit").unwrap_or_else(|| "unknown".to_owned()),
            probe_value(&out.stderr, "commits").unwrap_or_else(|| "unknown".to_owned()),
            probe_value(&out.stderr, "dirty").unwrap_or_else(|| "unknown".to_owned()),
        ));
        if let Some(missing) = probe_value(&out.stderr, "missing_credential_file") {
            self.line(&format!(
                "- credential file missing in the login: {missing}"
            ));
        }
        self.save(&format!("workshop-gate-probe-{tag}.jsonl"), &out.stdout);
        let logs = self.docker(&Self::args(&["logs", "--timestamps", &name]), false, false);
        self.save(
            &format!("workshop-gate-probe-{tag}-logs.txt"),
            &format!("{}{}", logs.stdout, logs.stderr),
        );
        if claude_mount.is_some() {
            let dest = self
                .dir
                .join(format!("workshop-gate-probe-{tag}-hooks.log"));
            let cp = self.docker(
                &[
                    "cp".to_owned(),
                    format!("{name}:/work/.officina/home/hooks-gate.log"),
                    dest.to_string_lossy().into_owned(),
                ],
                false,
                false,
            );
            if cp.code == Some(0) {
                let log = std::fs::read_to_string(&dest).unwrap_or_default();
                for (event, count) in hook_log_counts(&log) {
                    self.line(&format!("- hooks log: {event} {count} line(s)"));
                }
            } else {
                self.line("- hooks log: absent (no hook wrote it)");
            }
        }
        self.docker(&Self::args(&["rm", "-v", &name]), false, false);
        (summary, out)
    }

    fn remove_volume(&mut self, volume: &str) {
        self.docker(&Self::args(&["volume", "rm", volume]), false, false);
    }

    /// Lists every object labelled [`PROBE_LABEL`], removes them (images
    /// unless `--keep-images`), and records the listings after.
    fn cleanup(&mut self) {
        self.step("Cleanup");
        let filter = format!("label={PROBE_LABEL}");
        let containers = self.docker(
            &["ps".into(), "-aq".into(), "--filter".into(), filter.clone()],
            false,
            false,
        );
        let ids: Vec<String> = containers
            .stdout
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if !ids.is_empty() {
            let mut args = Self::args(&["rm", "-f", "-v"]);
            args.extend(ids);
            self.docker(&args, false, false);
        }
        for (list, remove) in [
            (["network", "ls", "-q"], ["network", "rm"]),
            (["volume", "ls", "-q"], ["volume", "rm"]),
        ] {
            let mut args = Self::args(&list);
            args.push("--filter".to_owned());
            args.push(filter.clone());
            let out = self.docker(&args, false, false);
            let ids: Vec<String> = out.stdout.split_whitespace().map(str::to_owned).collect();
            if !ids.is_empty() {
                let mut args = Self::args(&remove);
                args.extend(ids);
                self.docker(&args, false, false);
            }
        }
        if !self.opts.keep_images {
            let out = self.docker(
                &[
                    "image".into(),
                    "ls".into(),
                    "-q".into(),
                    "--filter".into(),
                    filter.clone(),
                ],
                false,
                false,
            );
            let ids: BTreeSet<String> = out.stdout.split_whitespace().map(str::to_owned).collect();
            if !ids.is_empty() {
                let mut args = Self::args(&["image", "rm", "-f"]);
                args.extend(ids);
                self.docker(&args, false, false);
            }
        }
        let mut left = String::new();
        for kind in ["ps -a", "network ls", "volume ls", "image ls"] {
            let mut args: Vec<String> = kind.split(' ').map(str::to_owned).collect();
            args.extend(["-q".to_owned(), "--filter".to_owned(), filter.clone()]);
            let out = self.docker(&args, false, false);
            let _ = writeln!(left, "{kind}: {}", out.stdout.split_whitespace().count());
        }
        self.block("Probe objects left (images kept with --keep-images)", &left);
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// One line of the owner's input; the end of input stops the probe.
fn read_line() -> String {
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => {
            eprintln!("\nworkshop_gate_probe: input closed, stopping");
            std::process::exit(1);
        }
        Ok(_) => line.trim().to_owned(),
    }
}

/// Asks one observation: `y`, `n` or `o` (other), a short note may follow.
fn ask(probe: &mut Probe, question: &str) -> bool {
    if probe.opts.dry_run {
        probe.line(&format!("- {question} (dry run: not asked)"));
        return true;
    }
    loop {
        print!("\n{question}\n[y]es / [n]o / [o]ther, a note may follow: ");
        let _ = std::io::stdout().flush();
        let line = read_line();
        let (key, note) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let answer = match key {
            "y" | "Y" => "yes",
            "n" | "N" => "no",
            "o" | "O" => "other",
            _ => continue,
        };
        let note = if answer == "other" && note.is_empty() {
            print!("What did you see? ");
            let _ = std::io::stdout().flush();
            read_line()
        } else {
            note.to_owned()
        };
        let suffix = if note.is_empty() {
            String::new()
        } else {
            format!(" ({note})")
        };
        let text = format!("- {question} **{answer}**{suffix}");
        let text = probe.redact(&text);
        probe.report.push_str(&text);
        probe.report.push('\n');
        return answer == "yes";
    }
}

fn ask_text(probe: &mut Probe, question: &str) -> String {
    if probe.opts.dry_run {
        probe.line(&format!("- {question} (dry run: not asked)"));
        return String::new();
    }
    print!("\n{question} ");
    let _ = std::io::stdout().flush();
    let text = read_line();
    let line = probe.redact(&format!("- {question} `{text}`"));
    probe.report.push_str(&line);
    probe.report.push('\n');
    text
}

/// W3 (D-613): the probe base built through the proxy, then curl as uid
/// 1000 through it.
fn network(probe: &mut Probe) {
    probe.step("W3: a build and a container through the network's proxy");
    probe.line(&format!(
        "- proxy: {}",
        if probe.opts.proxy.is_some() {
            "given with --proxy (its address stays out of this report)"
        } else {
            "none (direct)"
        }
    ));
    let built = probe.build_images();
    probe.line(&format!(
        "- build through the proxy: {}",
        if built { "worked" } else { "FAILED" }
    ));
    if built {
        let mut args = Probe::args(&["run", "--rm", "--label", PROBE_LABEL, "--user", "1000:1000"]);
        if let Some(proxy) = probe.opts.proxy.clone() {
            args.extend(proxy_env_args(&format!("http://{proxy}")));
        }
        args.extend(Probe::args(&[
            "--entrypoint",
            "curl",
            BASE_IMAGE,
            "-sS",
            "-v",
            "-o",
            "/dev/null",
            "https://api.anthropic.com/",
        ]));
        let out = probe.docker(&args, false, false);
        let (issuer, status) = curl_summary(&out.stderr);
        probe.line(&format!(
            "- curl exit {}; HTTP answer {}",
            out.code.map_or("none".to_owned(), |c| c.to_string()),
            status.unwrap_or_else(|| "none".to_owned())
        ));
        let issuer_kind = match issuer.as_deref() {
            Some(i) if i.contains("Google Trust Services") || i.contains("Let's Encrypt") => {
                "a public root (the chain is not inspected)"
            }
            Some(_) => {
                "another issuer: TLS is probably inspected (its name stays out of this report)"
            }
            None => "not seen",
        };
        probe.line(&format!("- issuer: {issuer_kind}"));
    }
    ask(probe, "Is this the network the work floor uses?");
}

/// The login check, D-610 steps 1 to 5 and 7.
fn login(probe: &mut Probe) {
    if !probe.build_images() {
        return;
    }
    probe.networks();
    let proxy_env: Vec<String> = probe
        .opts
        .proxy
        .clone()
        .map(|p| proxy_env_args(&format!("http://{p}")))
        .unwrap_or_default();
    probe.docker(
        &Probe::args(&["volume", "create", "--label", PROBE_LABEL, LOGIN_VOLUME]),
        false,
        false,
    );
    let login_container = |extra: &[&str], network: &str, volume: &str, ro: bool| {
        let mut args = Probe::args(&["run", "--rm", "--network", network]);
        args.extend(worker_options());
        args.extend(Probe::args(&[
            "--tmpfs",
            "/work/.officina/home:uid=1000,gid=1000",
            "--env",
            "HOME=/work/.officina/home",
            "--env",
            "CLAUDE_CONFIG_DIR=/work/.officina/login",
            "--env",
            "DISABLE_AUTOUPDATER=1",
            "--env",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1",
            "--mount",
        ]));
        args.push(volume_mount(volume, "/work/.officina/login", ro));
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        args
    };

    probe.step("Login step 1: /login inside the probe base (D-605's command)");
    probe.line("- open the URL it prints in your browser on this machine, sign in, and paste the code it shows back here");
    let mut args = login_container(&["-it"], "bridge", LOGIN_VOLUME, false);
    let image_at = args.len();
    args.splice(image_at..image_at, proxy_env.clone());
    args.extend(Probe::args(&[
        "--entrypoint",
        "/opt/officina/bin/claude",
        BASE_IMAGE,
        "auth",
        "login",
    ]));
    let out = probe.docker(&args, false, true);
    probe.line(&format!(
        "- exit {}",
        out.code.map_or("none".to_owned(), |c| c.to_string())
    ));
    let step1 = ask(
        probe,
        "Did the URL and pasted-code flow complete, and did \"Login successful\" show?",
    );
    let mut args = login_container(&[], "none", LOGIN_VOLUME, true);
    args.extend(Probe::args(&[
        "--entrypoint",
        "find",
        BASE_IMAGE,
        "/work/.officina/login",
        "-mindepth",
        "1",
        "-maxdepth",
        "2",
        "-printf",
        "%m %P\\n",
    ]));
    let out = probe.docker(&args, false, false);
    probe.block(
        "Files written by the login (mode and name only)",
        &files_listing(&out.stdout).join("\n"),
    );
    if !step1 {
        probe.line("- step 1 failed: D-610's branch, the login kind stays unbuilt");
        return;
    }

    probe.step("Login step 2: claude auth status, with and without the network");
    for network in ["bridge", "none"] {
        let mut args = login_container(&[], network, LOGIN_VOLUME, true);
        if network == "bridge" {
            let at = args.len();
            args.splice(at..at, proxy_env.clone());
        }
        args.extend(Probe::args(&[
            "--entrypoint",
            "/opt/officina/bin/claude",
            BASE_IMAGE,
            "auth",
            "status",
        ]));
        let out = probe.docker(&args, false, false);
        probe.line(&format!(
            "- network {network}: {}",
            auth_status_summary(&out.stdout, out.code)
        ));
    }

    probe.step("Login step 3: a worker from the stored login only (.credentials.json)");
    probe.start_stub(
        "officina-probe-stub-login",
        &["api.anthropic.com", "platform.claude.com"],
    );
    let (s3, _) = probe.worker_run(
        "login-3",
        "officina-probe-stub-login",
        PROMPT_COMMIT,
        None,
        Some((LOGIN_VOLUME, ".credentials.json")),
        None,
        false,
    );
    let mut files = ".credentials.json";
    let mut authenticated = s3.result_is_error == Some(false);
    if !authenticated {
        probe.line("- not authenticated with .credentials.json alone: again with .claude.json too");
        files = ".credentials.json .claude.json";
        let (s3b, _) = probe.worker_run(
            "login-3b",
            "officina-probe-stub-login",
            PROMPT_COMMIT,
            None,
            Some((LOGIN_VOLUME, files)),
            None,
            false,
        );
        authenticated = s3b.result_is_error == Some(false);
    }
    probe.line(&format!(
        "- step 3: {}",
        if authenticated {
            format!("authenticated with {files}")
        } else {
            "NOT authenticated".to_owned()
        }
    ));

    probe.step("Login step 4: does a run change its copy, and does the stored login still work");
    let run_volume = "officina-probe-work-login-4";
    probe.docker(
        &Probe::args(&["volume", "create", "--label", PROBE_LABEL, run_volume]),
        false,
        false,
    );
    let compare = "import hashlib,sys\nh=lambda p: hashlib.sha256(open(p,'rb').read()).hexdigest()\ntry:\n    print('same' if h(sys.argv[1])==h(sys.argv[2]) else 'different')\nexcept OSError:\n    print('missing')\n";
    let mut args = Probe::args(&["run", "--rm", "--network", INTERNAL_NET]);
    args.extend(worker_options());
    args.extend(worker_env("http://officina-probe-stub-login:3128"));
    args.extend(Probe::args(&["--mount"]));
    args.push(volume_mount(run_volume, "/work", false));
    args.extend(Probe::args(&["--mount"]));
    args.push(volume_mount(LOGIN_VOLUME, "/work/.officina/login", true));
    let probe_dir = probe.scratch.join("probe-login-4");
    let _ = std::fs::create_dir_all(&probe_dir);
    let _ = std::fs::write(probe_dir.join("run.sh"), RUN_SH);
    let _ = std::fs::write(probe_dir.join("prompt.txt"), PROMPT_COMMIT);
    if let Some(mount) = bind_mount(&probe_dir, "/probe", true) {
        args.extend(["--mount".to_owned(), mount]);
    }
    args.extend(Probe::args(&["--env"]));
    args.push(format!("PROBE_CREDENTIALS={files}"));
    args.extend(Probe::args(&[
        "--entrypoint",
        "/bin/bash",
        BASE_IMAGE,
        "/probe/run.sh",
    ]));
    let out = probe.docker(&args, false, false);
    let s4 = StreamSummary::from_stream(&out.stdout);
    probe.line(&format!(
        "- the run: result {} is_error {}",
        s4.result_subtype.as_deref().unwrap_or("absent"),
        s4.result_is_error
            .map_or("absent".to_owned(), |b| b.to_string())
    ));
    let mut args = Probe::args(&["run", "--rm", "--network", "none"]);
    args.extend(worker_options());
    args.extend([
        "--mount".to_owned(),
        volume_mount(run_volume, "/work", true),
    ]);
    args.extend([
        "--mount".to_owned(),
        volume_mount(LOGIN_VOLUME, "/work/.officina/login", true),
    ]);
    args.extend(Probe::args(&[
        "--entrypoint",
        "python3",
        BASE_IMAGE,
        "-c",
        compare,
        "/work/.officina/config/.credentials.json",
        "/work/.officina/login/.credentials.json",
    ]));
    let out = probe.docker(&args, false, false);
    probe.line(&format!(
        "- the run's copy against the stored file: {}",
        out.stdout.trim()
    ));
    probe.remove_volume(run_volume);
    let (s4b, _) = probe.worker_run(
        "login-4b",
        "officina-probe-stub-login",
        PROMPT_COMMIT,
        None,
        Some((LOGIN_VOLUME, files)),
        None,
        false,
    );
    probe.line(&format!(
        "- a second run from the stored file: {}",
        if s4b.result_is_error == Some(false) {
            "authenticated"
        } else {
            "NOT authenticated"
        }
    ));

    probe
        .step("Login step 5: logout (revokes this probe login only), then a run from a saved copy");
    probe.docker(
        &Probe::args(&["volume", "create", "--label", PROBE_LABEL, SAVED_VOLUME]),
        false,
        false,
    );
    let mut args = Probe::args(&["run", "--rm", "--network", "none"]);
    args.extend(worker_options());
    args.extend([
        "--mount".to_owned(),
        volume_mount(LOGIN_VOLUME, "/work/.officina/login", true),
    ]);
    args.extend([
        "--mount".to_owned(),
        volume_mount(SAVED_VOLUME, "/work/.officina/out", false),
    ]);
    args.extend(Probe::args(&[
        "--entrypoint",
        "cp",
        BASE_IMAGE,
        "-a",
        "/work/.officina/login/.",
        "/work/.officina/out/",
    ]));
    probe.docker(&args, false, false);
    if ask(
        probe,
        "Log out the probe login now? It revokes this probe's login only, not your own sessions.",
    ) {
        let mut args = login_container(&[], "bridge", LOGIN_VOLUME, false);
        let at = args.len();
        args.splice(at..at, proxy_env.clone());
        args.extend(Probe::args(&[
            "--entrypoint",
            "/opt/officina/bin/claude",
            BASE_IMAGE,
            "auth",
            "logout",
        ]));
        let out = probe.docker(&args, false, false);
        probe.line(&format!(
            "- logout exit {}",
            out.code.map_or("none".to_owned(), |c| c.to_string())
        ));
        let (s5, out) = probe.worker_run(
            "login-5",
            "officina-probe-stub-login",
            PROMPT_COMMIT,
            None,
            Some((SAVED_VOLUME, files)),
            None,
            false,
        );
        probe.line(&format!(
            "- the revoked login's shape: api_retry errors [{}], result {} is_error {}, exit {}",
            s5.api_retry_errors.join(", "),
            s5.result_subtype.as_deref().unwrap_or("absent"),
            s5.result_is_error
                .map_or("absent".to_owned(), |b| b.to_string()),
            probe_value(&out.stderr, "exit").unwrap_or_else(|| "unknown".to_owned())
        ));
        for network in ["bridge", "none"] {
            let mut args = login_container(&[], network, SAVED_VOLUME, true);
            if network == "bridge" {
                let at = args.len();
                args.splice(at..at, proxy_env.clone());
            }
            args.extend(Probe::args(&[
                "--entrypoint",
                "/opt/officina/bin/claude",
                BASE_IMAGE,
                "auth",
                "status",
            ]));
            let out = probe.docker(&args, false, false);
            probe.line(&format!(
                "- auth status on the saved copy, network {network}: {}",
                auth_status_summary(&out.stdout, out.code)
            ));
        }
    }
    probe.remove_volume(SAVED_VOLUME);

    probe.step("Login step 7: hosts the stub allowed and refused");
    probe.stub_report("officina-probe-stub-login");
}

/// Gates 3 to 6, then question 10.
fn token(probe: &mut Probe) {
    if !probe.has_token && !probe.opts.dry_run {
        probe.line(&format!(
            "{TOKEN_NAME} is not set in this console: run `claude setup-token`, then set it in this console only ($env:{TOKEN_NAME} = '...' in PowerShell), and start the probe again"
        ));
        return;
    }
    if !probe.build_images() {
        return;
    }
    probe.networks();

    probe.step(
        "Gate 3: a worker authenticates with the setup token (stub allows api.anthropic.com alone)",
    );
    probe.start_stub("officina-probe-stub-g3", &["api.anthropic.com"]);
    let (g3, out) = probe.worker_run(
        "gate3",
        "officina-probe-stub-g3",
        PROMPT_COMMIT,
        None,
        None,
        None,
        true,
    );
    let refused = probe.stub_report("officina-probe-stub-g3");
    let passed = g3.result_is_error == Some(false)
        && probe_value(&out.stderr, "exit").as_deref() == Some("0")
        && probe_value(&out.stderr, "commits").as_deref() == Some("2");
    probe.line(&format!(
        "- gate 3: {}",
        if passed { "pass" } else { "FAIL" }
    ));
    if !passed && !refused.is_empty() {
        let hosts: Vec<String> = refused
            .iter()
            .filter_map(|t| t.rsplit_once(':').map(|(h, _)| h.to_owned()))
            .collect();
        probe.line(&format!(
            "- again with the refused hosts added: {}",
            hosts.join(", ")
        ));
        let mut allow: Vec<&str> = vec!["api.anthropic.com"];
        allow.extend(hosts.iter().map(String::as_str));
        probe.start_stub("officina-probe-stub-g3b", &allow);
        probe.worker_run(
            "gate3b",
            "officina-probe-stub-g3b",
            PROMPT_COMMIT,
            None,
            None,
            None,
            true,
        );
        probe.stub_report("officina-probe-stub-g3b");
    }
    let help = probe.docker(
        &Probe::args(&[
            "run",
            "--rm",
            "--label",
            PROBE_LABEL,
            "--network",
            "none",
            "--entrypoint",
            "/opt/officina/bin/claude",
            BASE_IMAGE,
            "--help",
        ]),
        false,
        false,
    );
    probe.line(&format!(
        "- claude --help lists --permission-prompts: {}",
        help.stdout.contains("--permission-prompts")
    ));

    probe.step("Gate 4: hooks from a read-only mount (six events)");
    let (g4, _) = probe.worker_run(
        "gate4",
        "officina-probe-stub-g3",
        PROMPT_TOOL,
        Some(&gate4_settings()),
        None,
        None,
        true,
    );
    probe.line(&format!(
        "- hook lines in the stream: {}",
        g4.hook_lines.len()
    ));
    probe.line("- in one real Claude Code session on your work machine, type /memory, then /hooks");
    ask_text(
        probe,
        "Kinds of source the instruction files come from (user, project, managed, imported...), no paths:",
    );
    ask_text(
        probe,
        "Kinds of source the hooks come from (user settings, project settings, local, managed, plugin...), no paths:",
    );

    probe.step("Gate 5: stream fields, and a PreToolUse hook whose program is missing");
    let (g5, _) = probe.worker_run(
        "gate5",
        "officina-probe-stub-g3",
        PROMPT_TOOL,
        Some(&gate5_settings()),
        None,
        None,
        true,
    );
    let exits: Vec<String> = g5
        .hook_lines
        .iter()
        .filter_map(|(_, _, code)| code.map(|c| c.to_string()))
        .collect();
    probe.line(&format!(
        "- hook exit codes reported: {}",
        if exits.is_empty() {
            "none".to_owned()
        } else {
            exits.join(", ")
        }
    ));

    probe.step("Gate 6: the budget cap (--max-budget-usd 0.01, a multi-step prompt)");
    let (g6, _) = probe.worker_run(
        "gate6",
        "officina-probe-stub-g3",
        PROMPT_BUDGET,
        None,
        None,
        Some("0.01"),
        true,
    );
    probe.line(&format!(
        "- result subtype {}, spend {}",
        g6.result_subtype.as_deref().unwrap_or("absent"),
        g6.result_cost_usd
            .map_or("absent".to_owned(), |c| format!("{c:.4} USD"))
    ));

    probe.step("Question 10: revoking a setup-token token");
    ask_text(
        probe,
        "Where, in your account's settings, can a setup-token token be revoked (menu path, or 'not found')?",
    );
    probe.line(&format!(
        "- close this console now: {TOKEN_NAME} lives in it only; store the token later with the cmdkey line Officina's doctor prints"
    ));
}

/// Gate 1's owner half: a git-ignored folder of one of his projects,
/// mounted read-only; then the junction question (D-236).
fn mount(probe: &mut Probe) {
    probe.step("Gate 1, owner half: a git-ignored folder of an owner project inside the synced tree, mounted read-only");
    if !probe.build_images() {
        return;
    }
    let text = ask_text(
        probe,
        "Full path of a git-ignored folder of one of your projects inside the synced tree (it is never written in this report):",
    );
    probe.report = probe.report.replace(&text, "<the folder>");
    let folder = PathBuf::from(text.trim().trim_matches('"'));
    if !probe.opts.dry_run && !folder.is_dir() {
        probe.line("- not a folder; stopping");
        return;
    }
    let ignored = ask(probe, "Is this folder git-ignored in its project?");
    let Some(source) = bind_mount(&folder, "/work/.officina/mounts/0", true) else {
        probe.line("- the path holds a comma, a quote or a control character: mount_path_unsupported (AD-102)");
        return;
    };
    let run_check = |probe: &mut Probe| {
        let mut args = Probe::args(&["run", "--rm", "--network", "none"]);
        args.extend(worker_options());
        args.extend(["--mount".to_owned(), source.clone()]);
        args.extend(Probe::args(&[
            "--entrypoint",
            "python3",
            BASE_IMAGE,
            "-c",
            MOUNT_CHECK_PY,
            "/work/.officina/mounts/0",
        ]));
        let out = probe.docker(&args, false, false);
        let text = format!("{}{}", out.stdout, out.stderr);
        probe.block("Inside the container", &text);
        text
    };
    let first = run_check(probe);
    let read_only = first.contains("rewrite: Read-only file system")
        && first.contains("create: Read-only file system");
    probe.line(&format!(
        "- read-only: {}; ignored: {ignored}",
        if read_only { "yes" } else { "NO" }
    ));
    if ask(
        probe,
        "Create, inside that folder, a scratch folder officina-probe-junction holding a junction to a scratch folder outside it (both removed afterwards)?",
    ) && !probe.opts.dry_run
    {
        let inside = folder.join("officina-probe-junction");
        let target = probe.scratch.join("junction-target");
        let link = inside.join("link");
        let made = std::fs::create_dir_all(&inside)
            .and_then(|()| std::fs::create_dir_all(&target))
            .and_then(|()| std::fs::write(target.join("officina-probe-marker.txt"), "marker"));
        if made.is_ok() && make_junction(&link, &target) {
            let second = run_check(probe);
            let followed = second.contains("junction entries: officina-probe-marker.txt");
            probe.line(&format!(
                "- the junction inside the mount is {}",
                if followed {
                    "FOLLOWED (doctor counts reparse points inside each mount, D-236)"
                } else {
                    "not followed"
                }
            ));
        } else {
            probe.line("- the junction could not be made");
        }
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&inside);
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "mklink is a cmd built-in; the probe makes the junction by hand (D-128)"
)]
fn make_junction(link: &Path, target: &Path) -> bool {
    Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn docker_exe() -> PathBuf {
    #[cfg(windows)]
    {
        use officina_host::platform::windows::{Hive, registry_string};
        if let Some(bin) = registry_string(
            Hive::LocalMachine,
            "SOFTWARE\\Docker Inc.\\Docker\\1.0",
            "BinPath",
        ) {
            let exe = Path::new(&bin).join("docker.exe");
            if exe.is_file() {
                return exe;
            }
        }
    }
    PathBuf::from("docker.exe")
}

fn parse_options(args: &[String]) -> Result<(String, Options), String> {
    let mut sub = None;
    let mut opts = Options {
        proxy: None,
        ca: None,
        claude_version: None,
        keep_images: false,
        dry_run: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--proxy" => {
                let value = it.next().ok_or("--proxy needs host:port")?;
                opts.proxy = Some(parse_proxy(value).ok_or("--proxy needs host:port")?);
            }
            "--ca" => opts.ca = Some(PathBuf::from(it.next().ok_or("--ca needs a file")?)),
            "--claude-version" => {
                opts.claude_version =
                    Some(it.next().ok_or("--claude-version needs x.y.z")?.clone());
            }
            "--keep-images" => opts.keep_images = true,
            "--dry-run" => opts.dry_run = true,
            "network" | "login" | "token" | "mount" if sub.is_none() => sub = Some(arg.clone()),
            _ => return Err(USAGE.to_owned()),
        }
    }
    sub.map(|s| (s, opts)).ok_or_else(|| USAGE.to_owned())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (sub, opts) = match parse_options(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(64);
        }
    };
    let all_env: Vec<(String, String)> = std::env::vars().collect();
    let inside = SESSION_MARKERS
        .iter()
        .any(|m| all_env.iter().any(|(k, _)| k.eq_ignore_ascii_case(m)));
    if inside && !opts.dry_run {
        eprintln!(
            "workshop_gate_probe: run this from your own console, not from inside a Claude Code session"
        );
        return ExitCode::from(64);
    }
    let has_token = all_env
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case(TOKEN_NAME) && !v.is_empty());
    let env: Vec<(String, String)> = all_env
        .into_iter()
        .filter(|(k, _)| !CREDENTIAL_NAMES.iter().any(|c| k.eq_ignore_ascii_case(c)))
        .collect();
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let scratch = std::env::temp_dir()
        .join("officina-gate-probe")
        .join(format!("{sub}-{}", now_ms() / 1000));
    if let Err(e) = std::fs::create_dir_all(&scratch) {
        eprintln!(
            "workshop_gate_probe: cannot create {}: {e}",
            scratch.display()
        );
        return ExitCode::FAILURE;
    }
    let mut probe = Probe {
        docker: docker_exe(),
        env,
        has_token,
        dir,
        scratch,
        report: String::new(),
        opts,
        profile: std::env::var("USERPROFILE").ok(),
    };
    let (y, m, d) = utc_date(now_ms());
    probe.line(&format!(
        "\n# workshop_gate_probe {sub}, {y:04}-{m:02}-{d:02} (UTC)\n"
    ));
    probe.line(&format!("- dry run: {}", probe.opts.dry_run));
    probe.versions();
    match sub.as_str() {
        "network" => network(&mut probe),
        "login" => login(&mut probe),
        "token" => token(&mut probe),
        _ => mount(&mut probe),
    }
    probe.cleanup();
    probe.write_report();
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_summary_keeps_fields_and_drops_texts() {
        let stream = concat!(
            r#"{"type":"system","subtype":"init","model":"m","tools":["Bash","Read"],"mcp_servers":[],"permissionMode":"bypassPermissions"}"#,
            "\n",
            r#"{"type":"system","subtype":"api_retry","error":"authentication_failed","error_status":401}"#,
            "\n",
            "not json\n",
            r#"{"type":"system","subtype":"hook_response","hook_event_name":"PreToolUse","exit_code":127,"output":"secret text"}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":true,"total_cost_usd":0.0,"num_turns":1,"duration_ms":5,"session_id":"s","terminal_reason":"api_error","api_error_status":401,"result":"Failed"}"#,
            "\n",
        );
        let s = StreamSummary::from_stream(stream);
        assert_eq!(s.lines, 5);
        assert_eq!(s.not_json, 1);
        assert!(s.init_model);
        assert_eq!(s.init_tools, Some(2));
        assert_eq!(s.init_mcp_servers, Some(0));
        assert_eq!(s.api_retry_errors, vec!["authentication_failed"]);
        assert_eq!(
            s.hook_lines,
            vec![(
                "hook_response".to_owned(),
                Some("PreToolUse".to_owned()),
                Some(127)
            )]
        );
        assert_eq!(s.result_subtype.as_deref(), Some("success"));
        assert_eq!(s.result_is_error, Some(true));
        assert_eq!(s.result_api_error_status, Some(401));
        let report = s.report_lines().join("\n");
        assert!(!report.contains("secret text"));
        assert!(!report.contains("Failed"));
        assert!(report.contains("exit 127"));
    }

    #[test]
    fn only_the_first_init_counts() {
        let stream = concat!(
            r#"{"type":"system","subtype":"init","model":"m","tools":["A"],"mcp_servers":[]}"#,
            "\n",
            r#"{"type":"system","subtype":"init","model":"m","tools":["A","B","C"],"mcp_servers":[1]}"#,
            "\n",
        );
        assert_eq!(StreamSummary::from_stream(stream).init_tools, Some(1));
    }

    #[test]
    fn stub_hosts_are_split_and_deduplicated() {
        let log = "10:00:00 LISTENING\n10:00:01 ALLOWED api.anthropic.com:443\n10:00:02 ALLOWED api.anthropic.com:443\n10:00:03 REFUSED example.org:443\n10:00:04 FAILED x:443 boom\n";
        let (allowed, refused) = stub_hosts(log);
        assert_eq!(
            allowed.into_iter().collect::<Vec<_>>(),
            vec!["api.anthropic.com:443"]
        );
        assert_eq!(
            refused.into_iter().collect::<Vec<_>>(),
            vec!["example.org:443"]
        );
    }

    #[test]
    fn auth_status_keeps_only_the_method_and_the_state() {
        let json = r#"{"loggedIn":true,"authMethod":"claude.ai","email":"user@example.com","orgName":"Org"}"#;
        let line = auth_status_summary(json, Some(0));
        assert_eq!(line, "exit 0, loggedIn true, authMethod claude.ai");
        assert!(!line.contains("example.com"));
        assert_eq!(
            auth_status_summary("not json", Some(1)),
            "exit 1, loggedIn absent, authMethod absent"
        );
    }

    #[test]
    fn a_proxy_is_host_and_port() {
        assert_eq!(
            parse_proxy("proxy.example.com:8080").as_deref(),
            Some("proxy.example.com:8080")
        );
        assert_eq!(
            parse_proxy("http://10.0.0.10:3128/").as_deref(),
            Some("10.0.0.10:3128")
        );
        for bad in [
            "",
            "host",
            ":80",
            "host:0",
            "host:99999",
            "a,b:80",
            "a b:80",
            "https://x:1",
        ] {
            assert_eq!(parse_proxy(bad), None, "{bad}");
        }
    }

    #[test]
    fn proxy_arguments_set_both_cases() {
        let build = proxy_build_args(Some("proxy.example.com:8080"));
        for name in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            assert!(build.contains(&format!("{name}=http://proxy.example.com:8080")));
        }
        assert!(proxy_build_args(None).is_empty());
        let env = proxy_env_args("http://gw:3128");
        assert!(env.contains(&"no_proxy=localhost,127.0.0.1,::1".to_owned()));
        assert_eq!(env.iter().filter(|a| *a == "--env").count(), 6);
    }

    #[test]
    fn the_worker_options_hold_the_fixed_set() {
        let options = worker_options().join(" ");
        for fixed in [
            "--read-only",
            "--cap-drop ALL",
            "--security-opt no-new-privileges:true",
            "--init",
            "--user 1000:1000",
            "--log-driver local",
            "--label officina-probe=1",
        ] {
            assert!(options.contains(fixed), "{fixed}");
        }
        assert!(!options.contains("--privileged"));
    }

    #[test]
    fn a_mount_path_with_a_field_separator_is_refused() {
        assert_eq!(
            bind_mount(Path::new("C:/scratch/a b"), "/m", true).as_deref(),
            Some("type=bind,src=C:/scratch/a b,dst=/m,readonly")
        );
        assert_eq!(
            bind_mount(Path::new("C:/a,readonly=false"), "/m", true),
            None
        );
        assert_eq!(bind_mount(Path::new("C:/a\"b"), "/m", true), None);
        assert_eq!(
            volume_mount("v", "/work", false),
            "type=volume,src=v,dst=/work"
        );
    }

    #[test]
    fn probe_values_read_the_last_line() {
        let stderr = "noise\nPROBE exit=1\nPROBE exit=0\nPROBE commits=2\n";
        assert_eq!(probe_value(stderr, "exit").as_deref(), Some("0"));
        assert_eq!(probe_value(stderr, "commits").as_deref(), Some("2"));
        assert_eq!(probe_value(stderr, "dirty"), None);
    }

    #[test]
    fn curl_summary_finds_the_issuer_and_the_status() {
        let stderr = "*  subject: CN=api.anthropic.com\n*  issuer: C=US; O=Google Trust Services; CN=WE1\n> GET / HTTP/1.1\n< HTTP/1.1 404 Not Found\n";
        let (issuer, status) = curl_summary(stderr);
        assert_eq!(
            issuer.as_deref(),
            Some("C=US; O=Google Trust Services; CN=WE1")
        );
        assert_eq!(status.as_deref(), Some("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn quoted_lines_quote_spaces() {
        let args = vec!["run".to_owned(), "a b".to_owned(), String::new()];
        assert_eq!(quoted("docker", &args), "docker run \"a b\" \"\"");
    }

    #[test]
    fn gate4_settings_declare_every_event_once() {
        let settings: serde_json::Value = serde_json::from_str(&gate4_settings()).expect("json");
        let hooks = settings["hooks"].as_object().expect("hooks");
        assert_eq!(hooks.len(), GATE4_EVENTS.len());
        for event in GATE4_EVENTS {
            let command = hooks[*event][0]["hooks"][0]["command"]
                .as_str()
                .expect("command");
            assert!(command.contains(event) && command.contains("$HOME/hooks-gate.log"));
        }
        let log = "SessionStart\nPreToolUse\nPreToolUse\n";
        let counts = hook_log_counts(log);
        assert_eq!(counts.len(), GATE4_EVENTS.len());
        assert!(counts.contains(&("PreToolUse", 2)));
        assert!(counts.contains(&("SessionEnd", 0)));
        let missing: serde_json::Value = serde_json::from_str(&gate5_settings()).expect("json");
        assert!(
            missing["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
                .as_str()
                .is_some_and(|c| c.starts_with("officina-probe-missing-program"))
        );
    }

    #[test]
    fn options_parse_and_refuse() {
        let args: Vec<String> = ["login", "--proxy", "p.example.com:8080", "--keep-images"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let (sub, opts) = parse_options(&args).expect("parsed");
        assert_eq!(sub, "login");
        assert_eq!(opts.proxy.as_deref(), Some("p.example.com:8080"));
        assert!(opts.keep_images);
        for bad in [&["login", "token"][..], &["--proxy"], &["nope"], &[]] {
            let args: Vec<String> = bad.iter().map(|s| (*s).to_owned()).collect();
            assert!(parse_options(&args).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_listing_is_sorted_names_and_modes() {
        assert_eq!(
            files_listing("600 .credentials.json\n\n644 .claude.json\n"),
            vec!["600 .credentials.json", "644 .claude.json"]
        );
    }

    #[test]
    fn the_probe_dockerfile_pins_its_base_and_makes_the_login_folder() {
        assert!(PROBE_DOCKERFILE.contains("debian:trixie-slim@sha256:"));
        assert!(STUB_DOCKERFILE.contains("debian:trixie-slim@sha256:"));
        assert!(PROBE_DOCKERFILE.contains("/work/.officina/login"));
        assert!(PROBE_DOCKERFILE.contains("sha256sum -c"));
        let ca = PROBE_DOCKERFILE.find("update-ca-certificates").expect("ca");
        let first_https = PROBE_DOCKERFILE.find("https://").expect("https");
        assert!(
            ca < first_https,
            "the CA lands before any HTTPS download (D-612)"
        );
    }
}
