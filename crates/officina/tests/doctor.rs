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
