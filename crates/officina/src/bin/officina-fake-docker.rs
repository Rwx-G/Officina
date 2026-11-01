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
//! `network ls`, `volume ls`; any other call is logged and exits 0. Later
//! stories add theirs.

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
        let _ = writeln!(file, "{line}");
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
    let state = read_json(&beside(STATE));
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
        _ => 0,
    };
    std::process::exit(code);
}
