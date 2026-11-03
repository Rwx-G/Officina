//! Stand-in for `docker.exe` in tests (D-223). Never shipped: `test =
//! false`, and the release packages an explicit file list.
//!
//! Configured by `fake-docker.json` next to the executable (D-86's sidecar:
//! the engine's environment is an allow list, so no `FAKE_*` variable
//! reaches it), it keeps the engine's state in `fake-docker-state.json`
//! beside the sidecar and appends one line per call to `fake-docker.log`:
//! `{"argv": [...], "env_names": [...]}`, names sorted, never a value.
//!
//! Sidecar keys: `version` (`linux`, `windows` or `down`), `sleep_ms` (every
//! call waits that long first), `stdout_bytes` (every call also prints that
//! many bytes). State keys: `containers` (`id`, `name`, `state`, `labels`,
//! `exit_code`, `oom_killed`, `env`), `images` (`id`, `name`, `labels`,
//! `env`), `networks` and `volumes` (`name`, `labels`). Modes here:
//! `version`, `info`, `ps`, `inspect` (formatted to state and labels when
//! asked, the whole object with `Config.Env` otherwise), `image inspect`,
//! `network ls`, `volume ls`; story 6.8's `buildx ls` (sidecar `builders`,
//! else one current `docker`-driver builder), `build` (see [`build`]),
//! `create` of a recorded image, `cp <container>:<path> -` streaming the
//! program its image holds, `rm` and `image rm`; story 6.9's attached
//! `start` (see [`start_attached`]); story 6.10's battery: a detached
//! `start`, `logs` without `--follow`, `stop`, `network` and `volume`
//! `create` and `rm` kept in the state, and the battery worker's, the
//! gateways' and the decoy's output from the sidecar (see
//! [`battery_output`], [`logs_once`]); story 6.11's launch: `fail_when`
//! (`prefix`, and `contains`) fails every call matching it, exit 1;
//! `touch_when` (`prefix`, `contains`, `path`) writes an empty file at
//! `path` when a call matches, as a hand setting a flag between two steps
//! would; `start_latency_ms` holds a detached `start` that long after the
//! engine accepted it, as a slow client does, and `stop_latency_ms` holds a
//! `stop` that long before it acts (story 6.17); story 6.12's followers
//! (see [`follow_logs`]); story 6.13's packer copy (see
//! [`copy_package`]). Any other call is logged
//! and exits 0. Later stories add theirs.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SIDECAR: &str = "fake-docker.json";
const STATE: &str = "fake-docker-state.json";
const LOG: &str = "fake-docker.log";

fn beside(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join(name)
}

fn read_json(path: &Path) -> serde_json::Value {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn log_call(argv: &[String]) {
    let mut names: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    names.sort();
    let line = serde_json::json!({ "argv": argv, "env_names": names });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(beside(LOG))
    {
        // One write per line: followers run beside other calls, and a
        // line written in pieces interleaves with theirs.
        let _ = file.write_all(format!("{line}\n").as_bytes());
    }
}

fn labels_text(labels: &serde_json::Value) -> String {
    labels
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

fn find<'s>(state: &'s serde_json::Value, key: &str, name: &str) -> Option<&'s serde_json::Value> {
    state[key]
        .as_array()?
        .iter()
        .find(|item| item["name"] == name || item["id"] == name)
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    log_call(&argv);
    let sidecar = read_json(&beside(SIDECAR));
    let mut state = read_json(&beside(STATE));
    if let Some(ms) = sidecar["sleep_ms"].as_u64() {
        std::thread::sleep(Duration::from_millis(ms));
    }
    if let Some(bytes) = sidecar["stdout_bytes"].as_u64() {
        let chunk = vec![b'x'; 64 * 1024];
        let mut left = usize::try_from(bytes).unwrap_or(0);
        let mut out = std::io::stdout().lock();
        while left > 0 {
            let n = left.min(chunk.len());
            if out.write_all(&chunk[..n]).is_err() {
                break;
            }
            left -= n;
        }
    }
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let joined = words.join(" ");
    let matches = |key: &str| {
        sidecar[key]["prefix"]
            .as_str()
            .is_some_and(|prefix| joined.starts_with(prefix))
            && sidecar[key]["contains"]
                .as_str()
                .is_none_or(|part| joined.contains(part))
    };
    if matches("touch_when")
        && let Some(path) = sidecar["touch_when"]["path"].as_str()
    {
        let _ = std::fs::write(path, b"");
    }
    if matches("fail_when") {
        eprintln!("Error: the fake engine fails this call");
        std::process::exit(1);
    }
    if words.first() == Some(&"stop")
        && let Some(ms) = sidecar["stop_latency_ms"].as_u64()
    {
        std::thread::sleep(Duration::from_millis(ms));
        // What other calls changed while this one waited stands.
        state = read_json(&beside(STATE));
    }
    let code = match words.as_slice() {
        ["version", ..] => match sidecar["version"].as_str().unwrap_or("linux") {
            "down" => {
                println!(r#"{{"Client":{{"Version":"29.8.2"}},"Server":null}}"#);
                eprintln!("failed to connect to the docker API");
                1
            }
            os => {
                let os = if os == "windows" { "windows" } else { "linux" };
                println!(
                    r#"{{"Client":{{"Version":"29.8.2"}},"Server":{{"Platform":{{"Name":"Docker Desktop 4.95.0 (242831)"}},"Version":"29.8.2","Os":"{os}","KernelVersion":"6.18.33.2-microsoft-standard-WSL2"}}}}"#
                );
                0
            }
        },
        ["info", ..] => {
            println!(
                r#"{{"MemTotal":8589934592,"NCPU":8,"SystemTime":"2026-01-01T00:00:00Z","SecurityOptions":["name=seccomp,profile=builtin"],"Runtimes":{{"runc":{{}}}},"HttpProxy":"","HttpsProxy":"","NoProxy":""}}"#
            );
            0
        }
        ["ps", ..] => {
            for container in state["containers"].as_array().into_iter().flatten() {
                let row = serde_json::json!({
                    "ID": container["id"],
                    "Names": container["name"],
                    "State": container["state"],
                    "Labels": labels_text(&container["labels"]),
                });
                println!("{row}");
            }
            0
        }
        ["inspect", rest @ ..] => {
            let formatted = rest.contains(&"--format");
            match rest
                .last()
                .and_then(|name| find(&state, "containers", name))
            {
                Some(container) => {
                    let docker_state = serde_json::json!({
                        "Status": container["state"],
                        "Running": container["state"] == "running",
                        "ExitCode": container["exit_code"],
                        "OOMKilled": container["oom_killed"].as_bool().unwrap_or(false),
                    });
                    if formatted {
                        println!(
                            "{}",
                            serde_json::json!({ "state": docker_state, "labels": container["labels"] })
                        );
                    } else {
                        println!(
                            "{}",
                            serde_json::json!([{ "State": docker_state, "Config": { "Labels": container["labels"], "Env": container["env"] } }])
                        );
                    }
                    0
                }
                None => {
                    eprintln!("Error: No such container");
                    1
                }
            }
        }
        ["image", "inspect", .., name] => match find(&state, "images", name) {
            Some(image) => {
                println!(
                    "{}",
                    serde_json::json!({ "id": image["id"], "labels": image["labels"], "env": image["env"] })
                );
                0
            }
            None => {
                eprintln!("Error: No such image");
                1
            }
        },
        ["network", "ls", ..] | ["volume", "ls", ..] => {
            let key = if words[0] == "network" {
                "networks"
            } else {
                "volumes"
            };
            for item in state[key].as_array().into_iter().flatten() {
                let row = serde_json::json!({ "Name": item["name"], "Labels": labels_text(&item["labels"]) });
                println!("{row}");
            }
            0
        }
        ["buildx", "ls", ..] => {
            let default = serde_json::json!([{
                "Name": "desktop-linux", "Driver": "docker", "Current": true,
                "Nodes": [{"Endpoint": "desktop-linux", "Name": "desktop-linux"}],
            }]);
            let builders = if sidecar["builders"].is_array() {
                &sidecar["builders"]
            } else {
                &default
            };
            for builder in builders.as_array().into_iter().flatten() {
                println!("{builder}");
            }
            0
        }
        ["build", rest @ ..] => build(rest, &sidecar, &mut state),
        ["create", rest @ ..] => create(rest, &sidecar, &mut state),
        ["cp", source, "-"] => copy_out(source, &sidecar, &state),
        ["start", "--attach", .., name] => start_attached(name, &sidecar, &mut state),
        ["start", name] => {
            let code = start_detached(name, &sidecar, &mut state);
            if let Some(ms) = sidecar["start_latency_ms"].as_u64() {
                std::thread::sleep(Duration::from_millis(ms));
            }
            code
        }
        ["logs", "--follow", "--timestamps", name] => follow_logs(name, &sidecar),
        ["logs", name] => logs_once(name, &sidecar, &state),
        ["stop", rest @ ..] => {
            for container in state["containers"].as_array_mut().into_iter().flatten() {
                if rest.iter().any(|n| container["name"] == *n) && container["state"] == "running" {
                    container["state"] = "exited".into();
                    container["exit_code"] = 0.into();
                }
            }
            save(&state);
            0
        }
        ["network", "create", rest @ ..] | ["volume", "create", rest @ ..] => {
            let key = if words[0] == "network" {
                "networks"
            } else {
                "volumes"
            };
            let item = serde_json::json!({
                "name": rest.last().copied().unwrap_or_default(),
                "labels": pairs(&values(rest, "--label")),
            });
            push(&mut state, key, item);
            save(&state);
            0
        }
        ["network", "rm", names @ ..] | ["volume", "rm", names @ ..] => {
            let key = if words[0] == "network" {
                "networks"
            } else {
                "volumes"
            };
            if let Some(items) = state[key].as_array_mut() {
                items.retain(|i| !names.iter().any(|n| i["name"] == *n));
            }
            save(&state);
            0
        }
        ["rm", rest @ ..] => {
            let names: Vec<&str> = rest
                .iter()
                .copied()
                .filter(|a| !a.starts_with("--"))
                .collect();
            if let Some(containers) = state["containers"].as_array_mut() {
                containers.retain(|c| !names.iter().any(|n| c["name"] == *n || c["id"] == *n));
            }
            save(&state);
            0
        }
        ["image", "rm", name] => {
            if let Some(images) = state["images"].as_array_mut() {
                images.retain(|i| i["name"] != *name && i["id"] != *name);
            }
            save(&state);
            0
        }
        _ => 0,
    };
    std::process::exit(code);
}

fn save(state: &serde_json::Value) {
    let _ = std::fs::write(beside(STATE), state.to_string());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .filter_map(|i| u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok())
        .collect()
}

/// The value after each `flag` in `args`.
fn values<'a>(args: &[&'a str], flag: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == flag)
        .map(|w| w[1])
        .collect()
}

fn pairs(items: &[&str]) -> serde_json::Map<String, serde_json::Value> {
    items
        .iter()
        .filter_map(|item| item.split_once('='))
        .map(|(k, v)| (k.to_owned(), serde_json::Value::String(v.to_owned())))
        .collect()
}

fn push(state: &mut serde_json::Value, key: &str, item: serde_json::Value) {
    if !state[key].is_array() {
        state[key] = serde_json::json!([]);
    }
    if let Some(items) = state[key].as_array_mut() {
        items.push(item);
    }
}

/// `build`: an image recorded under its tag (D-223), its labels the
/// `--label`s over those of the image its Dockerfile's last `FROM` names,
/// its `ENV` the parent's (a fixed pair for a base on Debian, none on
/// scratch), its programs the parent's, or for a base the context's
/// `officina-entry` and a `claude` naming the version argument. Sidecar
/// `build.<tag>`: `fail`, `env` (lines added), `replace` (program path to
/// text), `drop_base_label`; `build_output` is printed first.
fn build(args: &[&str], sidecar: &serde_json::Value, state: &mut serde_json::Value) -> i32 {
    let tag = values(args, "--tag")
        .first()
        .copied()
        .unwrap_or_default()
        .to_owned();
    let context = PathBuf::from(args.last().copied().unwrap_or_default());
    let dockerfile = values(args, "--file")
        .first()
        .map_or_else(|| context.join("Dockerfile"), PathBuf::from);
    let settings = &sidecar["build"][&tag];
    if let Some(output) = sidecar["build_output"].as_str() {
        eprintln!("{output}");
    }
    eprintln!("#1 building {tag}");
    if settings["fail"].as_bool().unwrap_or(false) {
        eprintln!("ERROR: failed to solve: process did not complete successfully");
        return 1;
    }
    let text = std::fs::read_to_string(&dockerfile).unwrap_or_default();
    let from = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("FROM "))
        .next_back()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_default()
        .to_owned();
    let parent = find(state, "images", &from).cloned();
    let mut labels = parent
        .as_ref()
        .and_then(|p| p["labels"].as_object().cloned())
        .unwrap_or_default();
    labels.extend(pairs(&values(args, "--label")));
    if settings["drop_base_label"].as_bool().unwrap_or(false) {
        labels.remove("officina.base");
    }
    let mut env: Vec<serde_json::Value> = match &parent {
        Some(p) => p["env"].as_array().cloned().unwrap_or_default(),
        None if from == "scratch" => Vec::new(),
        None => vec![
            "PATH=/opt/officina/bin:/usr/bin:/bin".into(),
            "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt".into(),
        ],
    };
    env.extend(settings["env"].as_array().cloned().unwrap_or_default());
    let build_args = pairs(&values(args, "--build-arg"));
    let mut programs = match &parent {
        Some(p) => p["programs"].as_object().cloned().unwrap_or_default(),
        None if from == "scratch" => serde_json::Map::new(),
        None => {
            let entry = std::fs::read(context.join("officina-entry")).unwrap_or_default();
            let version = build_args
                .get("CLAUDE_CODE_VERSION")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            serde_json::Map::from_iter([
                (
                    "/opt/officina/bin/officina-entry".to_owned(),
                    hex(&entry).into(),
                ),
                (
                    "/opt/officina/bin/claude".to_owned(),
                    hex(format!("claude {version}").as_bytes()).into(),
                ),
            ])
        }
    };
    for (path, content) in settings["replace"].as_object().into_iter().flatten() {
        programs.insert(
            path.clone(),
            hex(content.as_str().unwrap_or_default().as_bytes()).into(),
        );
    }
    let next = state["next_id"].as_u64().unwrap_or(1);
    state["next_id"] = (next + 1).into();
    let image = serde_json::json!({
        "id": format!("sha256:{next:064x}"),
        "name": tag,
        "labels": labels,
        "env": env,
        "programs": programs,
    });
    if let Some(images) = state["images"].as_array_mut() {
        images.retain(|i| i["name"] != tag);
    }
    push(state, "images", image);
    save(state);
    0
}

/// `create`: a container, `created`, of the image an argument names.
fn create(args: &[&str], sidecar: &serde_json::Value, state: &mut serde_json::Value) -> i32 {
    let name = values(args, "--name")
        .first()
        .copied()
        .unwrap_or_default()
        .to_owned();
    if find(state, "containers", &name).is_some() {
        eprintln!("Error: the container name is already in use");
        return 125;
    }
    let image = args
        .iter()
        .find_map(|a| find(state, "images", a).map(|i| i["id"].clone()))
        .unwrap_or_default();
    let container = serde_json::json!({
        "id": format!("c{name}"),
        "name": name,
        "state": "created",
        "image": image,
        "labels": pairs(&values(args, "--label")),
        "phase": values(args, "--phase").first(),
        "plan": values(args, "--plan").first(),
        "probe": args.contains(&"--probe"),
    });
    // Whether a credential the create received is the battery's dummy, as
    // a yes or no: the value itself is never kept (D-223).
    for name in ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        if let Some(value) = std::env::var_os(name) {
            let seen = serde_json::json!({
                "container": name_of(&container),
                "name": name,
                "dummy": sidecar["dummy_credential"].as_str().is_some_and(|d| value == d),
            });
            push(state, "credentials", seen);
        }
    }
    push(state, "containers", container);
    save(state);
    0
}

/// `start --attach [--interactive] <container>` (story 6.9): the container
/// ends `exited`. An `inventory` container reads its list on stdin, keeps
/// it in `fake-docker-inventory.json` and prints one result per command:
/// sidecar `inventory.<command text>` when set, else `found` at
/// `/usr/bin/<first word>`; sidecar `inventory_output` replaces the whole
/// output.
fn start_attached(name: &str, sidecar: &serde_json::Value, state: &mut serde_json::Value) -> i32 {
    let Some(container) = state["containers"]
        .as_array_mut()
        .and_then(|all| all.iter_mut().find(|c| c["name"] == name))
    else {
        eprintln!("Error: No such container: {name}");
        return 1;
    };
    container["state"] = "exited".into();
    container["exit_code"] = 0.into();
    let role = container["labels"]["officina.role"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let started = container.clone();
    if role == "battery" {
        battery_output(&started, sidecar, state);
    }
    save(state);
    if role != "inventory" {
        return 0;
    }
    let mut text = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
    let _ = std::fs::write(beside("fake-docker-inventory.json"), &text);
    if let Some(output) = sidecar["inventory_output"].as_str() {
        print!("{output}");
        return 0;
    }
    let input: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    for (index, command) in input["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let written = command["command"].as_str().unwrap_or_default();
        let mut result = sidecar["inventory"][written].clone();
        if !result.is_object() {
            let program = written.split_whitespace().next().unwrap_or_default();
            result = serde_json::json!({
                "resolution": "found",
                "path": format!("/usr/bin/{program}"),
            });
        }
        result["index"] = index.into();
        println!("{result}");
    }
    0
}

fn name_of(container: &serde_json::Value) -> String {
    container["name"].as_str().unwrap_or_default().to_owned()
}

fn role_of(container: &serde_json::Value) -> &str {
    container["labels"]["officina.role"]
        .as_str()
        .unwrap_or_default()
}

/// The prefix of the battery's second gateway (story 6.10).
const GATEWAY_B: &str = "officina-gateway-b-";

/// `start <container>` (story 6.10): a decoy probe ends at once, its exit
/// code sidecar `decoy_probe_exit` (0); anything else runs.
fn start_detached(name: &str, sidecar: &serde_json::Value, state: &mut serde_json::Value) -> i32 {
    let Some(container) = state["containers"]
        .as_array_mut()
        .and_then(|all| all.iter_mut().find(|c| c["name"] == name))
    else {
        eprintln!("Error: No such container: {name}");
        return 1;
    };
    if container["probe"].as_bool().unwrap_or(false) {
        container["state"] = "exited".into();
        container["exit_code"] = sidecar["decoy_probe_exit"].as_i64().unwrap_or(0).into();
    } else {
        container["state"] = "running".into();
    }
    save(state);
    0
}

/// A battery worker's output (story 6.10): sidecar `battery_lines.<phase>`,
/// which the test derives from core's attempts, the phase read from the
/// container's `--phase`. Phase 2 ends gateway B with sidecar
/// `gateway_b_exit` (3).
fn battery_output(
    container: &serde_json::Value,
    sidecar: &serde_json::Value,
    state: &mut serde_json::Value,
) {
    let phase = container["phase"].as_str().unwrap_or("1");
    if phase == "2" {
        for gateway in state["containers"].as_array_mut().into_iter().flatten() {
            if gateway["name"]
                .as_str()
                .is_some_and(|n| n.starts_with(GATEWAY_B))
            {
                gateway["state"] = "exited".into();
                gateway["exit_code"] = sidecar["gateway_b_exit"].as_i64().unwrap_or(3).into();
            }
        }
    }
    if let Some(lines) = sidecar["battery_lines"][phase].as_str() {
        print!("{lines}");
    }
}

/// `logs <container>` (story 6.10): a decoy prints sidecar `decoy_logs`, a
/// probe nothing; a battery gateway prints sidecar `gateway_logs.a` or
/// `gateway_logs.b`.
fn logs_once(name: &str, sidecar: &serde_json::Value, state: &serde_json::Value) -> i32 {
    let Some(container) = find(state, "containers", name) else {
        eprintln!("Error: No such container: {name}");
        return 1;
    };
    let text = match role_of(container) {
        "decoy" if !container["probe"].as_bool().unwrap_or(false) => sidecar["decoy_logs"].as_str(),
        "battery_gateway" => {
            let key = if name.starts_with(GATEWAY_B) {
                "b"
            } else {
                "a"
            };
            sidecar["gateway_logs"][key].as_str()
        }
        _ => None,
    };
    print!("{}", text.unwrap_or_default());
    0
}

/// The timestamp the fake engine puts before each followed line.
const STAMP: &str = "2026-01-01T00:00:00.000000000Z";

/// `logs --follow --timestamps <container>` (story 6.12): sidecar
/// `follow.<role>.stdout` and `.stderr` printed with a timestamp, then
/// `follow.<role>.bulk_bytes` of 64 KiB lines on stdout; the follower then
/// runs while the container does, as the engine's would, reading the
/// state again every 20 ms. With `follow_cut_first` set, a container's
/// first follow ends at once while it still runs, as a follower the
/// engine dropped would (counted in `fake-docker-follows.json`).
fn follow_logs(name: &str, sidecar: &serde_json::Value) -> i32 {
    let state = read_json(&beside(STATE));
    let Some(container) = find(&state, "containers", name) else {
        eprintln!("Error: No such container: {name}");
        return 1;
    };
    let settings = &sidecar["follow"][role_of(container)];
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    for line in settings["stdout"].as_str().unwrap_or_default().lines() {
        let _ = writeln!(out, "{STAMP} {line}");
    }
    let bulk = settings["bulk_bytes"].as_u64().unwrap_or(0);
    let row = format!("{STAMP} {}\n", "x".repeat(64 * 1024));
    let mut written = 0u64;
    while written < bulk {
        if out.write_all(row.as_bytes()).is_err() {
            return 0;
        }
        written += row.len() as u64;
    }
    for line in settings["stderr"].as_str().unwrap_or_default().lines() {
        let _ = writeln!(err, "{STAMP} {line}");
    }
    let _ = out.flush();
    let _ = err.flush();
    drop((out, err));
    let follows_path = beside("fake-docker-follows.json");
    let mut follows = read_json(&follows_path);
    let seen = follows[name].as_u64().unwrap_or(0);
    follows[name] = (seen + 1).into();
    let _ = std::fs::write(&follows_path, follows.to_string());
    if seen == 0 && sidecar["follow_cut_first"].as_bool().unwrap_or(false) {
        return 0;
    }
    for _ in 0..3000 {
        if !beside(STATE).exists() {
            return 0;
        }
        let now = read_json(&beside(STATE));
        match find(&now, "containers", name) {
            Some(c) if c["state"] == "running" => {}
            // A state file caught mid-write reads empty: look again.
            None if now.as_object().is_none_or(serde_json::Map::is_empty) => {}
            _ => return 0,
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    0
}

/// A packer's `out/` (story 6.13): the bytes of the file sidecar
/// `package` names, then sidecar `package_extra_bytes` zero bytes, so a
/// test can pass the copy's cap without writing that much.
fn copy_package(sidecar: &serde_json::Value) -> i32 {
    let Some(bytes) = sidecar["package"]
        .as_str()
        .and_then(|path| std::fs::read(path).ok())
    else {
        eprintln!("Error: Could not find the file /work/.officina/out");
        return 1;
    };
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(&bytes).is_err() {
        return 1;
    }
    let mut extra = sidecar["package_extra_bytes"].as_u64().unwrap_or(0);
    let zeros = vec![0u8; 64 * 1024];
    while extra > 0 {
        let n = usize::try_from(extra.min(zeros.len() as u64)).unwrap_or(zeros.len());
        if stdout.write_all(&zeros[..n]).is_err() {
            return 1;
        }
        extra -= n as u64;
    }
    0
}

/// `cp <container>:<path> -`: the program the container's image holds at
/// `path`, as a one-file ustar stream on stdout; a packer's `out/` from
/// [`copy_package`].
fn copy_out(source: &str, sidecar: &serde_json::Value, state: &serde_json::Value) -> i32 {
    let Some((name, path)) = source.split_once(':') else {
        return 1;
    };
    if name.starts_with("officina-pack-") {
        return copy_package(sidecar);
    }
    let content = find(state, "containers", name)
        .and_then(|c| find(state, "images", c["image"].as_str().unwrap_or_default()))
        .and_then(|i| i["programs"][path].as_str())
        .map(unhex);
    let Some(bytes) = content else {
        eprintln!("Error: Could not find the file {path} in container {name}");
        return 1;
    };
    let file = path.rsplit('/').next().unwrap_or(path).as_bytes();
    let mut header = [0u8; 512];
    let n = file.len().min(100);
    header[..n].copy_from_slice(&file[..n]);
    header[100..107].copy_from_slice(b"0000755");
    header[108..115].copy_from_slice(b"0000000");
    header[116..123].copy_from_slice(b"0000000");
    header[124..135].copy_from_slice(format!("{:011o}", bytes.len()).as_bytes());
    header[136..147].copy_from_slice(b"00000000000");
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut out = header.to_vec();
    out.extend_from_slice(&bytes);
    out.resize(out.len().div_ceil(512) * 512 + 1024, 0);
    let mut stdout = std::io::stdout().lock();
    i32::from(stdout.write_all(&out).is_err())
}
