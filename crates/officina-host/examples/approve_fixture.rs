//! Test and fixture tool (D-25): approves the relay floor of an Officina home,
//! or stops or resumes its relay, through the same `authority` functions the
//! window calls, so fixtures never recompute the catalogue hash elsewhere and
//! no command line path of `officina` approves (AD-21). Built only with
//! `window-authority`, linked into neither shipped binary.
//!
//! Usage: `approve_fixture <officina home> approve|stop|resume`. The relay
//! floor is resolved as `officina run` resolves it in that home, without
//! `OFFICINA_FLOOR` (AD-24). Exit 0, 1 when the authority refuses, 64 on
//! usage.

use std::process::ExitCode;

use officina_host::relay::authority::{approve_catalogue, set_relay_stopped};
use officina_host::relay::home::{RelayHome, load_relay_floor};

const USAGE: &str = "usage: approve_fixture <officina home> approve|stop|resume";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, verb] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let home = RelayHome::new(home, None);
    let done = match verb.as_str() {
        "approve" => load_relay_floor(&home.relay_floor(&home.read_approved()))
            .map_err(|e| e.to_string())
            .and_then(|floor| {
                approve_catalogue(&home, &floor.hash)
                    .map(|approved| format!("approved {}", approved.hash))
                    .map_err(|e| e.to_string())
            }),
        "stop" | "resume" => set_relay_stopped(&home, verb == "stop")
            .map(|changed| format!("{verb}: changed {changed}"))
            .map_err(|e| e.to_string()),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(64);
        }
    };
    match done {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("approve_fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
