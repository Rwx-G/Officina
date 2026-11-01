//! Doctor's workshop section on the host (AD-123, D-228): the facts gathered
//! with the engine's read half (doctor starts no container), turned into
//! lines by core's [`WorkshopCheck`] rules.

use std::path::{Path, PathBuf};

use officina_core::workshop::doctor::{
    CheckLevel, Finding, WorkshopCheck, docker_cli_config, in_order, orphans, overrides_set,
    plain_tcp_docker_host,
};
use officina_core::workshop::engine::{
    EngineCall, EngineUnavailable, InfoFacts, ObjectRow, PsRow, engine_key, parse_object_rows,
    parse_ps,
};

use crate::doctor::{Check, Level, check};
use crate::workshop::ENV_LINUX_BIN_DIR;
use crate::workshop::engine::{
    BinarySource, ENV_DOCKER_BIN, EngineState, ResolveError, ResolvedDocker, execute,
    read_engine_state, resolve_binary,
};
use crate::workshop::git::ENV_GIT_BIN;

/// The variables doctor names when set (D-228): those that choose the
/// engine, the CLI's configuration, or Officina's own binaries.
pub const OVERRIDE_NAMES: &[&str] = &[
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_CONFIG",
    ENV_DOCKER_BIN,
    ENV_GIT_BIN,
    ENV_LINUX_BIN_DIR,
];

/// What doctor gathered for the workshop section.
#[derive(Debug)]
pub struct WorkshopFacts {
    pub docker: Result<(PathBuf, BinarySource), ResolveError>,
    pub git: Result<(PathBuf, BinarySource), ResolveError>,
    /// The environment doctor runs in.
    pub env: Vec<(String, String)>,
    /// The Docker CLI's `config.json`, when it exists.
    pub docker_config: Option<String>,
    /// `None` when no `docker` was resolved.
    pub engine: Option<EngineState>,
    pub info: Option<InfoFacts>,
    pub containers: Option<Vec<PsRow>>,
    pub networks: Option<Vec<ObjectRow>>,
    pub volumes: Option<Vec<ObjectRow>>,
    /// The workshop's folder when the Officina home lies in a sync
    /// client's folder (AD-39's test).
    pub synced_home: Option<PathBuf>,
}

/// The workshop's folder of `home` when `home` lies in a sync client's
/// folder or carries a Cloud Files attribute, by AD-39's classification
/// with `probe` (D-228, review F44).
pub fn synced_home(
    home: &Path,
    onedrive_roots: &[PathBuf],
    probe: &dyn crate::registry::store::LocationProbe,
) -> Option<PathBuf> {
    match crate::registry::store::classify(home, onedrive_roots, probe) {
        crate::registry::store::Location::Synced { .. } => {
            Some(crate::workshop::workshop_dir(home))
        }
        crate::registry::store::Location::Local | crate::registry::store::Location::Network => None,
    }
}

fn env_value<'e>(env: &'e [(String, String)], name: &str) -> Option<&'e str> {
    env.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// The Docker CLI's `config.json`: under `DOCKER_CONFIG` when set, else
/// `.docker` in the user's profile.
fn docker_config_path(env: &[(String, String)]) -> Option<PathBuf> {
    if let Some(dir) = env_value(env, "DOCKER_CONFIG") {
        return Some(Path::new(dir).join("config.json"));
    }
    env_value(env, "USERPROFILE")
        .or_else(|| env_value(env, "HOME"))
        .map(|home| Path::new(home).join(".docker").join("config.json"))
}

/// The most of `config.json` read: it holds auth entries and settings, a few
/// KiB in practice.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

fn read_docker_config(env: &[(String, String)]) -> Option<String> {
    let path = docker_config_path(env)?;
    let len = std::fs::metadata(&path).ok()?.len();
    if len > MAX_CONFIG_BYTES {
        return Some(String::new());
    }
    std::fs::read_to_string(path).ok()
}

/// The stdout of a read call, when it succeeded.
fn read_call(
    docker: &ResolvedDocker,
    call: &EngineCall,
    env: &[(String, String)],
    home: &Path,
) -> Option<String> {
    execute(docker, call, env, home, None)
        .ok()
        .filter(|outcome| outcome.succeeded())
        .map(|outcome| String::from_utf8_lossy(&outcome.stdout).into_owned())
}

/// Gathers the section's facts from `env` (doctor's environment) and the
/// registry, running only read calls in `home`.
pub fn gather(env: &[(String, String)], home: &Path) -> WorkshopFacts {
    let lookup = |name: &str| env_value(env, name).map(std::ffi::OsString::from);
    let docker = resolve_binary(
        "docker",
        lookup(ENV_DOCKER_BIN).as_deref(),
        crate::workshop::engine::hklm_docker(),
        lookup("PATH").as_deref(),
    );
    let git = resolve_binary(
        "git",
        lookup(ENV_GIT_BIN).as_deref(),
        crate::workshop::git::hklm_git(),
        lookup("PATH").as_deref(),
    );
    let mut facts = WorkshopFacts {
        docker,
        git,
        env: env.to_vec(),
        docker_config: read_docker_config(env),
        engine: None,
        info: None,
        containers: None,
        networks: None,
        volumes: None,
        synced_home: synced_home(
            home,
            &crate::registry::store::onedrive_roots_from_env(),
            &crate::registry::store::SystemProbe,
        ),
    };
    if let Ok((path, source)) = &facts.docker {
        let docker = ResolvedDocker::at(path.clone(), *source);
        let _ = std::fs::create_dir_all(home);
        let state = read_engine_state(Ok(&docker), env, home);
        if state.info().is_some() {
            facts.info = read_call(&docker, &EngineCall::info(), env, home)
                .and_then(|out| InfoFacts::parse(&out));
            facts.containers =
                read_call(&docker, &EngineCall::ps(), env, home).map(|out| parse_ps(&out));
            facts.networks = read_call(&docker, &EngineCall::network_ls(), env, home)
                .map(|out| parse_object_rows(&out));
            facts.volumes = read_call(&docker, &EngineCall::volume_ls(), env, home)
                .map(|out| parse_object_rows(&out));
        }
        facts.engine = Some(state);
    }
    facts
}

fn binary_findings(
    facts: &Result<(PathBuf, BinarySource), ResolveError>,
    resolved: WorkshopCheck,
    shim: WorkshopCheck,
    missing: WorkshopCheck,
) -> Finding {
    match facts {
        Ok((path, source)) => {
            Finding::new(resolved, format!("{} ({})", path.display(), source.text()))
        }
        Err(e @ ResolveError::NotExe(_)) => Finding::new(shim, e.to_string()),
        Err(e @ ResolveError::NotFound { .. }) => Finding::new(missing, e.to_string()),
    }
}

/// The section's findings from `facts`, in section order.
pub fn findings(facts: &WorkshopFacts) -> Vec<Finding> {
    let mut found = vec![
        binary_findings(
            &facts.docker,
            WorkshopCheck::DockerResolved,
            WorkshopCheck::DockerShim,
            WorkshopCheck::DockerMissing,
        ),
        binary_findings(
            &facts.git,
            WorkshopCheck::GitResolved,
            WorkshopCheck::GitShim,
            WorkshopCheck::GitMissing,
        ),
    ];
    found.extend(overrides_set(&facts.env, OVERRIDE_NAMES));
    if let Some(text) = &facts.docker_config {
        found.extend(docker_cli_config(text));
    }
    found.extend(plain_tcp_docker_host(&facts.env));
    match &facts.engine {
        None => {}
        Some(EngineState::Ready(info)) => {
            let key = facts.info.as_ref().map_or_else(
                || "key unknown: docker info gave no answer".to_owned(),
                |facts| format!("key {}", engine_key(info, facts)),
            );
            found.push(Finding::new(
                WorkshopCheck::EngineReady,
                format!(
                    "{}, engine {}, kernel {}; {key}",
                    info.platform, info.version, info.kernel
                ),
            ));
        }
        Some(EngineState::Unavailable(EngineUnavailable::NotRunning)) => found.push(Finding::new(
            WorkshopCheck::EngineNotRunning,
            "start Docker Desktop to launch workers",
        )),
        Some(EngineState::Unavailable(EngineUnavailable::WindowsContainers)) => {
            found.push(Finding::new(
                WorkshopCheck::EngineWindowsContainers,
                "the engine runs Windows containers; switch Docker Desktop to Linux containers",
            ));
        }
        Some(EngineState::Unreachable(why) | EngineState::Unresolved(why)) => {
            found.push(Finding::new(WorkshopCheck::EngineUnreachable, why.clone()));
        }
    }
    if let Some(containers) = &facts.containers {
        for (rows, check, what) in [
            (&facts.networks, WorkshopCheck::OrphanNetworks, "network"),
            (&facts.volumes, WorkshopCheck::OrphanVolumes, "volume"),
        ] {
            let count = rows.as_deref().map_or(0, |rows| orphans(containers, rows));
            if count > 0 {
                found.push(Finding::new(
                    check,
                    format!("{count} {what}(s) labelled officina.managed belong to no container"),
                ));
            }
        }
    }
    if let Some(dir) = &facts.synced_home {
        found.push(Finding::new(
            WorkshopCheck::HomeSynced,
            format!(
                "the Officina home lies in a synced folder: exclude {} from the sync client, or move the home (OFFICINA_HOME); base bundles of up to 1 GiB would sync",
                dir.display()
            ),
        ));
    }
    in_order(found)
}

/// The section's doctor lines from `facts`.
pub fn checks(facts: &WorkshopFacts) -> Vec<Check> {
    findings(facts)
        .into_iter()
        .map(|finding| {
            let level = match finding.check.level() {
                CheckLevel::Fail => Level::Fail,
                CheckLevel::Warn => Level::Warn,
                CheckLevel::Inform => Level::Ok,
            };
            check(level, finding.check.title(), finding.detail)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::workshop::engine::EngineInfo;

    fn facts() -> WorkshopFacts {
        WorkshopFacts {
            docker: Ok((PathBuf::from("C:/Docker/docker.exe"), BinarySource::Hklm)),
            git: Ok((PathBuf::from("C:/Git/cmd/git.exe"), BinarySource::Path)),
            env: Vec::new(),
            docker_config: None,
            engine: Some(EngineState::Ready(EngineInfo {
                platform: "Docker Desktop 4.95.0 (242831)".into(),
                version: "29.8.2".into(),
                kernel: "6.18.33.2-microsoft-standard-WSL2".into(),
            })),
            info: None,
            containers: Some(Vec::new()),
            networks: Some(Vec::new()),
            volumes: Some(Vec::new()),
            synced_home: None,
        }
    }

    struct Probe {
        placeholder: PathBuf,
    }

    impl crate::registry::store::LocationProbe for Probe {
        fn is_remote_drive(&self, _root: &str) -> bool {
            false
        }
        fn exists(&self, _path: &Path) -> bool {
            false
        }
        fn is_cloud_placeholder(&self, path: &Path) -> bool {
            path == self.placeholder
        }
    }

    #[test]
    fn a_home_in_a_sync_root_warns() {
        let synced = Path::new("C:/Users/user01/Sync");
        let probe = Probe {
            placeholder: synced.to_path_buf(),
        };
        let home = synced.join("officina");
        assert_eq!(
            synced_home(&home, &[], &probe),
            Some(crate::workshop::workshop_dir(&home))
        );
        let local = Path::new("C:/Users/user01/.officina");
        assert_eq!(synced_home(local, &[], &probe), None);
        let mut facts = facts();
        facts.synced_home = synced_home(&home, &[], &probe);
        let line = checks(&facts)
            .into_iter()
            .find(|c| c.title == "workshop home")
            .unwrap();
        assert_eq!(line.level, Level::Warn);
    }

    fn of(facts: &WorkshopFacts, check: WorkshopCheck) -> Option<Finding> {
        findings(facts).into_iter().find(|f| f.check == check)
    }

    #[test]
    fn docker_and_git_sources_are_reported() {
        let facts = facts();
        let docker = of(&facts, WorkshopCheck::DockerResolved).unwrap();
        assert!(docker.detail.ends_with("(hklm)"), "{docker:?}");
        let git = of(&facts, WorkshopCheck::GitResolved).unwrap();
        assert!(git.detail.ends_with("(path)"), "{git:?}");
        assert!(checks(&facts).iter().all(|c| c.level == Level::Ok));
    }

    #[test]
    fn a_docker_shim_fails() {
        let mut facts = facts();
        facts.docker = Err(ResolveError::NotExe("C:/bin/docker.cmd".into()));
        facts.git = Err(ResolveError::NotExe("C:/bin/git.bat".into()));
        let lines = checks(&facts);
        for title in ["workshop docker", "workshop git"] {
            let line = lines.iter().find(|c| c.title == title).unwrap();
            assert_eq!(line.level, Level::Fail, "{line:?}");
        }
    }

    #[test]
    fn overrides_are_named() {
        let mut facts = facts();
        facts.env = OVERRIDE_NAMES
            .iter()
            .map(|n| ((*n).to_owned(), "value-never-printed".to_owned()))
            .collect();
        let line = of(&facts, WorkshopCheck::OverridesSet).unwrap();
        for name in OVERRIDE_NAMES {
            assert!(line.detail.contains(name), "{name} missing: {line:?}");
        }
        assert!(!line.detail.contains("value-never-printed"));
    }

    #[test]
    fn docker_cli_config_entries_are_named() {
        let mut facts = facts();
        facts.docker_config = Some(
            r#"{"proxies":{"default":{"httpProxy":"http://proxy.example.com:3128"}},"currentContext":"desktop-linux","cliPluginsExtraDirs":["C:/plugins"]}"#
                .into(),
        );
        for check in [
            WorkshopCheck::DockerCliProxies,
            WorkshopCheck::DockerCliContext,
            WorkshopCheck::DockerCliPluginDirs,
        ] {
            assert!(of(&facts, check).is_some(), "{check:?}");
        }
    }

    #[test]
    fn a_plain_tcp_docker_host_warns() {
        let mut facts = facts();
        facts.env = vec![("DOCKER_HOST".into(), "tcp://192.0.2.10:2375".into())];
        let line = checks(&facts)
            .into_iter()
            .find(|c| c.title == "workshop docker host")
            .unwrap();
        assert_eq!(line.level, Level::Warn);
    }

    #[test]
    fn windows_containers_fail() {
        let mut facts = facts();
        facts.engine = Some(EngineState::Unavailable(
            EngineUnavailable::WindowsContainers,
        ));
        let line = checks(&facts)
            .into_iter()
            .find(|c| c.title == "workshop engine")
            .unwrap();
        assert_eq!(line.level, Level::Fail);
    }

    #[test]
    fn the_engine_key_is_reported_when_info_answers() {
        let mut facts = facts();
        facts.info = InfoFacts::parse(
            r#"{"MemTotal":1,"NCPU":2,"SystemTime":"2026-01-01T00:00:00Z","SecurityOptions":[],"Runtimes":{}}"#,
        );
        let line = of(&facts, WorkshopCheck::EngineReady).unwrap();
        assert!(
            line.detail.contains("key Docker Desktop 4.95.0"),
            "{line:?}"
        );
    }

    #[test]
    fn the_config_path_follows_docker_config() {
        let env = vec![
            ("DOCKER_CONFIG".to_owned(), "D:/cfg".to_owned()),
            ("USERPROFILE".to_owned(), "C:/Users/user01".to_owned()),
        ];
        assert_eq!(
            docker_config_path(&env),
            Some(Path::new("D:/cfg").join("config.json"))
        );
        assert_eq!(
            docker_config_path(&env[1..]),
            Some(
                Path::new("C:/Users/user01")
                    .join(".docker")
                    .join("config.json")
            )
        );
    }
}
