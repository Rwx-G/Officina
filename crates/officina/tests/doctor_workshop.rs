//! Doctor's workshop section end to end (story 6.3 AC 10, AD-123, D-228,
//! D-1113): shown only for a floor that declares `[workshop]`, gathered
//! with the engine's read half against the fake engine, never a launch
//! kind.

mod common;

use officina_core::workshop::engine::EngineCallKind;

fn line<'t>(text: &'t str, title: &str) -> Option<&'t str> {
    text.lines().find(|l| l.contains(&format!("] {title}: ")))
}

fn doctor(env: &common::TestEnv, engine: &common::workshop::FakeEngine) -> String {
    let mut cmd = env.officina(&["doctor"]);
    cmd.env("OFFICINA_DOCKER_BIN", &engine.exe);
    let out = common::run_with(cmd, b"");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A floor that declares the workshop; its table need not validate.
const WORKSHOP_FLOOR: &str = "schema = 2

[workshop]
auth = \"oauth-token\"
";

fn state() -> serde_json::Value {
    serde_json::json!({
        "networks": [{"name": "officina-int-0123456789abcdef0123456789abcdef",
                      "labels": {"officina.managed": "1", "officina.run": "0123456789abcdef0123456789abcdef"}}],
        "volumes": [{"name": "officina-login-0123456789abcdef",
                     "labels": {"officina.managed": "1", "officina.role": "login"}}],
    })
}

#[test]
fn a_floor_without_a_workshop_runs_no_engine_call() {
    let env = common::TestEnv::new();
    env.write_floor("schema = 2\n");
    let engine = common::workshop::FakeEngine::new(&serde_json::json!({}), &state());
    let text = doctor(&env, &engine);
    assert!(line(&text, "workshop docker").is_none(), "{text}");
    assert!(engine.log().is_empty(), "{:?}", engine.log());
}

#[test]
fn the_workshop_section_reads_the_engine_and_counts_orphans() {
    let env = common::TestEnv::new();
    env.write_floor(WORKSHOP_FLOOR);
    let engine = common::workshop::FakeEngine::new(&serde_json::json!({}), &state());
    let text = doctor(&env, &engine);
    let docker = line(&text, "workshop docker").unwrap_or_else(|| panic!("docker line: {text}"));
    assert!(
        docker.starts_with("[ok  ]") && docker.ends_with("(override)"),
        "{docker}"
    );
    let overrides = line(&text, "workshop overrides").expect("overrides line");
    assert!(overrides.contains("OFFICINA_DOCKER_BIN"), "{overrides}");
    let ready = line(&text, "workshop engine").expect("engine line");
    assert!(
        ready.contains("Docker Desktop") && ready.contains("key "),
        "{ready}"
    );
    let orphans = line(&text, "workshop orphans").expect("orphans line");
    assert!(
        orphans.starts_with("[warn]") && orphans.contains("1 network"),
        "{orphans}"
    );
    assert!(
        !orphans.contains("volume"),
        "the login volume is never an orphan: {orphans}"
    );
    let calls: Vec<String> = engine
        .log()
        .iter()
        .map(|call| {
            let argv = call["argv"].as_array().unwrap();
            let word = |i: usize| argv.get(i).and_then(|w| w.as_str()).unwrap_or("");
            match word(0) {
                "network" | "volume" | "image" => format!("{} {}", word(0), word(1)),
                first => first.to_owned(),
            }
        })
        .collect();
    let read: Vec<&str> = EngineCallKind::ALL
        .iter()
        .filter(|k| !k.is_launch())
        .map(|k| k.code())
        .collect();
    for call in &calls {
        let code = call.replace(' ', "_");
        assert!(
            read.contains(&code.as_str()),
            "doctor ran {call}, not a read kind"
        );
    }
    for wanted in ["version", "info", "ps", "network ls", "volume ls"] {
        assert!(
            calls.iter().any(|c| c == wanted),
            "{wanted} missing: {calls:?}"
        );
    }
}

#[test]
fn windows_containers_fail_doctor() {
    let env = common::TestEnv::new();
    env.write_floor(WORKSHOP_FLOOR);
    let engine =
        common::workshop::FakeEngine::new(&serde_json::json!({"version": "windows"}), &state());
    let text = doctor(&env, &engine);
    let engine_line = line(&text, "workshop engine").expect("engine line");
    assert!(engine_line.starts_with("[FAIL]"), "{engine_line}");
}
