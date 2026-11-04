//! E2E fixture tool (story 6.18 AC 5, D-25, D-181, D-1230): writes the
//! workshop runs of an Officina home whose floor with a workshop template
//! the fixture script wrote and approved through `approve_fixture`, through
//! the window's own writer at explicit times: each run's `work_order` and
//! `workshop_run` rows as a launch records them (AD-107 step 1), then its
//! state as the run's last move left it. The script then prints the
//! payloads with the real `officina workshop --json` and `board --json`.
//! Never SQL from Node, never a command line write path. Built only with
//! `window-authority`, linked into neither shipped binary.
//!
//! Usage: `workshop_fixture <officina home> <script.json>`, where the file
//! holds a [`Script`]. Exit 0, 1 when a step fails, 64 on usage.

use std::path::Path;
use std::process::ExitCode;

use officina_core::orders::permission::WorkOrderPermission;
use officina_core::orders::work_order::{SubjectChoice, WorkOrder};
use officina_core::registry::types::WorkOrderSource;
use officina_core::relay::template::{ParamName, ProjectName, TemplateName};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::launch::ApprovedWorkshopTemplate;
use officina_core::workshop::run::RunState;
use officina_host::registry::orders::LaunchRecord;
use officina_host::registry::store::open_writer;
use officina_host::registry::workshop::RunInsert;
use officina_host::relay::home::{RelayHome, load_relay_floor};
use serde::Deserialize;

/// One run: its card id, its work order's template, project, values and
/// `seq`, and the state its last move left it in, at `state_at_ms`.
#[derive(Deserialize)]
struct RunStep {
    id: String,
    template: String,
    project: String,
    seq: u32,
    values: Vec<(String, String)>,
    state: RunState,
    started_at_ms: i64,
    state_at_ms: i64,
}

#[derive(Deserialize)]
struct Script {
    runs: Vec<RunStep>,
}

fn write(home: &Path, script: &Script) -> Result<(), String> {
    let relay = RelayHome::new(home, None);
    let state = relay.read_approved();
    let approved = state.approved().ok_or("no approved catalogue")?;
    let current = load_relay_floor(&relay.relay_floor(&state)).map_err(|e| format!("{e:?}"))?;
    let started = script
        .runs
        .iter()
        .map(|r| r.started_at_ms)
        .min()
        .unwrap_or(0);
    let mut writer = open_writer(home, &[], started)
        .writer()
        .ok_or("the registry cannot be written")?;
    let argv = vec!["-p".to_owned()];
    for run in &script.runs {
        let name = TemplateName::parse(&run.template).ok_or("a template name")?;
        let template = ApprovedWorkshopTemplate::new(approved, &current, &name)
            .map_err(|e| format!("{}: {e:?}", run.template))?;
        let values: Vec<(ParamName, &str)> = run
            .values
            .iter()
            .map(|(name, value)| {
                Ok((
                    ParamName::parse(name).ok_or("a param name")?,
                    value.as_str(),
                ))
            })
            .collect::<Result<_, &str>>()?;
        let order = WorkOrder::validate(
            template.template(),
            &ProjectName::parse(&run.project).ok_or("a project name")?,
            &values,
        )
        .map_err(|e| format!("{}: {e:?}", run.id))?;
        let branch = format!("officina/{}-{}", run.template, run.seq);
        writer
            .insert_workshop_run(
                &LaunchRecord {
                    order: &order,
                    card_id: &run.id,
                    seq: run.seq,
                    source: WorkOrderSource::Composer,
                    subject: &SubjectChoice::New("dependency upgrades".into()),
                    proposal_id: None,
                    permission: WorkOrderPermission::Project,
                },
                &RunInsert {
                    image: officina_core::workshop::image::WORKSHOP_BASE,
                    image_id: "sha256:0",
                    gateway_image_id: "sha256:1",
                    base_branch: "main",
                    base_commit: &"0".repeat(40),
                    parent_run_id: None,
                    branch: &branch,
                    uncommitted: 0,
                    plan_json: "{}",
                    plan_sha256: &"0".repeat(64),
                    credential_kind: CredentialKind::OauthToken,
                    deadline_ms: 30 * 60_000,
                    battery_id: &"0".repeat(32),
                    argv: &argv,
                },
                run.started_at_ms,
            )
            .map_err(|e| e.to_string())?
            .map_err(|taken| format!("{}: {taken:?}", run.id))?;
        if run.state != RunState::Launching {
            writer
                .set_run_state(&run.id, run.state, run.state_at_ms)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, script] = args.as_slice() else {
        eprintln!("usage: workshop_fixture <officina home> <script.json>");
        return ExitCode::from(64);
    };
    let script: Script = match std::fs::read(script)
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
    {
        Ok(script) => script,
        Err(e) => {
            eprintln!("workshop_fixture: {e}");
            return ExitCode::from(64);
        }
    };
    match write(Path::new(home), &script) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("workshop_fixture: {e}");
            ExitCode::FAILURE
        }
    }
}
