//! `officina doctor` on the registry (story 3.1 AC 13; FR-1 as amended,
//! AD-53, D-47, D-82, D-91): the network folder fails end to end, an absent
//! registry is reported and not created, and the database file is left as
//! it was.

mod common;

use common::{TestEnv, fingerprint};
use officina_core::registry::types::{SettingKey, Source};
use officina_host::registry::RegistryPaths;
use officina_host::registry::store::open_writer;

fn doctor_text(env: &TestEnv, home: Option<&str>) -> String {
    env.write_agents("[]");
    let mut cmd = env.officina(&["doctor"]);
    if let Some(home) = home {
        cmd.env("OFFICINA_HOME", home);
    }
    let out = common::run_with(cmd, b"");
    assert!(out.status.code().is_some_and(|code| code != 2));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// D-91: the classification calls a UNC path network without asking the
/// system, so no remote host is waited on; the other checks may fail on it,
/// only the registry line is asserted.
#[test]
fn unc_officina_home_fails_the_registry_location() {
    let env = TestEnv::new();
    let text = doctor_text(&env, Some(r"\\localhost\officina-missing-share\home"));
    let line = text
        .lines()
        .find(|line| line.contains("registry location"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(line.starts_with("[FAIL] registry location"), "{line}");
    assert!(line.contains("network folder"), "{line}");
}

#[test]
fn absent_registry_is_reported_not_created() {
    let env = TestEnv::new();
    let text = doctor_text(&env, None);
    assert!(text.contains("[ok  ] registry: absent"), "{text}");
    assert!(
        !RegistryPaths::new(&env.officina_home()).dir().exists(),
        "doctor created the registry"
    );
}

/// D-82: a reader may create `-wal` and `-shm`; everything else under
/// `registry/` is compared byte for byte.
#[test]
fn doctor_leaves_the_database_file_unchanged() {
    let env = TestEnv::new();
    let now = common::now_ms();
    let mut writer = open_writer(&env.officina_home(), &[], now)
        .writer()
        .unwrap();
    writer
        .set_setting(SettingKey::AutoArchive, "false", Source::Author, now)
        .unwrap();
    writer.copy_if_due(now).unwrap();
    drop(writer);
    let registry = RegistryPaths::new(&env.officina_home());
    let files = || {
        fingerprint(registry.dir())
            .into_iter()
            .filter(|(name, _)| !name.ends_with("-wal") && !name.ends_with("-shm"))
            .collect::<Vec<_>>()
    };
    let before = files();
    assert!(before.iter().any(|(name, _)| name == "registry.db"));
    let text = doctor_text(&env, None);
    assert!(text.contains("[ok  ] registry: schema version"), "{text}");
    assert!(text.contains("[ok  ] registry integrity"), "{text}");
    assert!(
        text.contains("[ok  ] registry copies: 1 daily copies"),
        "{text}"
    );
    assert_eq!(files(), before);
}
