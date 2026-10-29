//! `officina doctor` on the floor's view keys (story 4.2 AC 4, AD-168): a
//! bad value is a floor line naming the key and its line, the default in
//! force.

mod common;

use common::TestEnv;

#[test]
fn doctor_reports_a_bad_priority_and_threshold() {
    let env = TestEnv::new();
    env.write_agents("[]");
    env.write_floor(
        "schema = 1\n\n[alert]\nafter_minutes = 0\n\n[[project]]\nname = \"api\"\npath = \"Api\"\npriority = \"urgent\"\n",
    );
    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|line| line.contains("floor") && line.contains("priority"))
        .unwrap_or_else(|| panic!("no floor line naming priority: {text}"));
    assert!(line.contains("[warn]"), "{line}");
    assert!(line.contains("line 9: `priority`"), "{line}");
    assert!(line.contains("line 4: `after_minutes`"), "{line}");

    env.write_floor(
        "schema = 1\n\n[[project]]\nname = \"api\"\npath = \"Api\"\npriority = \"high\"\n",
    );
    let clean = String::from_utf8_lossy(&env.run(&["doctor"], b"").stdout).into_owned();
    let floor = clean
        .lines()
        .find(|line| line.contains("floor") && line.contains("projects"))
        .unwrap_or_else(|| panic!("no floor line: {clean}"));
    assert!(floor.starts_with("[ok"), "{floor}");
}

/// Story 4.12 AC 2 (AD-176, AD-16 as amended): the printed hooks block
/// registers the sub-agent events like the other buffered events, and the
/// hooks check fails until the author has pasted them; doctor writes
/// nothing under the Claude config dir.
#[test]
fn doctor_asks_for_the_sub_agent_events() {
    use officina_core::events::SUBAGENT_EVENTS;

    let env = TestEnv::new();
    env.write_agents("[]");
    let hooks_line = |stdout: &[u8]| -> String {
        String::from_utf8_lossy(stdout)
            .lines()
            .find(|line| line.contains("] hooks"))
            .unwrap_or_else(|| panic!("no hooks line"))
            .to_owned()
    };
    let block = common::printed_hooks_block(&env.run(&["doctor"], b"").stdout);
    for event in SUBAGENT_EVENTS {
        let entry = &block["hooks"][event][0]["hooks"][0];
        assert_eq!(entry["type"], "command", "{event}: {entry}");
        assert_eq!(entry["timeout"], 5, "{event}: {entry}");
        assert!(entry.get("onFailure").is_none(), "{event}: {entry}");
        assert!(
            entry["command"].as_str().unwrap().ends_with(" hook"),
            "{entry}"
        );
    }

    let settings = env.claude_config().join("settings.json");
    let mut without = block.clone();
    for event in SUBAGENT_EVENTS {
        without["hooks"].as_object_mut().unwrap().remove(event);
    }
    std::fs::write(&settings, without.to_string()).unwrap();
    let before = common::fingerprint(&env.claude_config());
    let out = env.run(&["doctor"], b"");
    let line = hooks_line(&out.stdout);
    assert!(line.starts_with("[FAIL"), "{line}");
    for event in SUBAGENT_EVENTS {
        assert!(line.contains(event), "{line}");
    }
    assert_eq!(common::fingerprint(&env.claude_config()), before);

    std::fs::write(&settings, block.to_string()).unwrap();
    let line = hooks_line(&env.run(&["doctor"], b"").stdout);
    assert!(line.starts_with("[ok"), "{line}");
}

/// Story 5.22 AC 6 (D-578, D-580): the router's hook path is inert. The
/// block doctor prints and the paste file it writes register no
/// `PermissionRequest`, whose registration is the routing story's.
#[test]
fn the_printed_hooks_block_registers_no_permission_request() {
    use officina_core::relay::prompts::PROMPT_EVENT;
    let env = TestEnv::new();
    env.write_agents("[]");
    let out = env.run(&["doctor"], b"");
    let block = common::printed_hooks_block(&out.stdout);
    assert!(block["hooks"].is_object(), "{block}");
    assert!(block["hooks"].get(PROMPT_EVENT).is_none(), "{block}");
    let saved = std::fs::read_to_string(env.officina_home().join("doctor").join("user-hooks.json"))
        .unwrap();
    assert!(!saved.contains(PROMPT_EVENT), "{saved}");
}
