//! E2E fixture tool (D-72): plays the window's registry writer on an
//! Officina home whose event files the fixture script wrote, at an explicit
//! `now_ms`, then exits, so `officina subjects --json` and `officina board
//! --json` print the fixtures with the window closed (D-82). Never SQL from
//! Node, never a command line write path. Built only with
//! `window-authority`, linked into neither shipped binary.
//!
//! Usage: `registry_fixture <officina home> <steps.json>`, where the file
//! holds `{"now_ms": <ms>, "steps": [...]}` and each step is one of:
//!
//! - `"ingest"`: one ingestion pass over the home's event files;
//! - `{"agents": [<claude agents records>]}`: one successful session list;
//! - `{"author": <AuthorAction>}`: one author action, which must succeed;
//! - `"copy"`: the daily copy, as the hourly pass takes it;
//! - `"damage"`: the database overwritten with bytes that are no database,
//!   then opened again, so the window's restore runs from the copy (D-93);
//! - `{"hook_fields": "<claude version>"}`: one ingestion pass, whose hook
//!   sightings are folded into that version's record of
//!   `state/contract.json`, as the window folds them (AD-56); the record
//!   must exist already (`officina board --json` starts it).
//!
//! Exit 0, 1 when a step fails, 64 on usage.

use std::path::Path;
use std::process::ExitCode;

use officina_core::agents::parse_agents;
use officina_core::registry::confirm::AuthorAction;
use officina_core::registry::fold::AgentsSnapshot;
use officina_host::collector::record_hook_fields;
use officina_host::registry::RegistryPaths;
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::{Writer, open_writer};
use serde::Deserialize;

const USAGE: &str = "usage: registry_fixture <officina home> <steps.json>";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    now_ms: i64,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Step {
    Ingest,
    Agents(Vec<serde_json::Value>),
    Author(AuthorAction),
    Copy,
    Damage,
    HookFields(String),
}

fn writer(home: &Path, now_ms: i64) -> Result<Writer, String> {
    open_writer(home, &[], now_ms)
        .writer()
        .ok_or_else(|| "the registry did not open writable".to_owned())
}

fn run(home: &Path, script: Script) -> Result<(), String> {
    let now_ms = script.now_ms;
    let events = home.join("events");
    let mut writer = writer(home, now_ms)?;
    let mut snapshot: Option<AgentsSnapshot> = None;
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: home,
        projects: &[],
        no_excerpts: false,
    };
    for (index, step) in script.steps.into_iter().enumerate() {
        let failed = |error: String| format!("step {index}: {error}");
        match step {
            Step::Ingest => {
                writer
                    .ingest(&ctx, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::HookFields(version) => {
                let report = writer
                    .ingest(&ctx, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
                let contract = home.join("state").join("contract.json");
                record_hook_fields(&contract, &version, &report.hook_sightings)
                    .ok_or_else(|| failed(format!("no contract record for {version}")))?;
            }
            Step::Agents(records) => {
                let text = serde_json::to_string(&records).map_err(|e| failed(e.to_string()))?;
                let records = parse_agents(&text).map_err(|e| failed(e.to_string()))?;
                writer
                    .apply_agents(&mut snapshot, &records, &[], now_ms)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::Author(action) => {
                let created = writer
                    .author(&action, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
                if let Some(id) = created {
                    println!("created subject {id}");
                }
            }
            Step::Copy => {
                writer
                    .copy_if_due(now_ms)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::Damage => {
                drop(writer);
                let database = RegistryPaths::new(home).database();
                std::fs::write(&database, "not a database, ".repeat(300))
                    .map_err(|e| failed(e.to_string()))?;
                writer = self::writer(home, now_ms).map_err(failed)?;
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, steps] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let script = match std::fs::read_to_string(steps)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str::<Script>(&text).map_err(|e| e.to_string()))
    {
        Ok(script) => script,
        Err(error) => {
            eprintln!("registry_fixture: {steps}: {error}");
            return ExitCode::from(64);
        }
    };
    match run(Path::new(home), script) {
        Ok(()) => {
            println!("done");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("registry_fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
