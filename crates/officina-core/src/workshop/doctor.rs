//! Doctor's workshop section (AD-123, D-228): every finding is a variant of
//! [`WorkshopCheck`], with its level and title, and the section prints in
//! the enum's declaration order (story 6.24 orders the whole section). The
//! host gathers the facts; the rules that read them live here.

use serde::{Deserialize, Serialize};

use super::engine::{ObjectRow, PsRow};
use super::names::{LOGIN_ROLE, ROLE_LABEL, RUN_LABEL};

/// How much a finding weighs in doctor (D-228).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckLevel {
    /// The workshop cannot run until it is fixed.
    Fail,
    /// It runs, but something is degraded or exposed.
    Warn,
    /// A fact worth reading, nothing to fix.
    Inform,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// One finding of doctor's workshop section. The declaration order is
    /// the order the section prints in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum WorkshopCheck {
        /// `docker.exe` resolved, with its source.
        DockerResolved,
        /// The `docker` found is not a native `.exe` (AD-51).
        DockerShim,
        /// No `docker` was found.
        DockerMissing,
        /// The workshop's `git` resolved, with its source.
        GitResolved,
        /// The `git` found is not a native `.exe`.
        GitShim,
        /// No `git` was found.
        GitMissing,
        /// Variables that steer which engine or binary is used are set.
        OverridesSet,
        /// The Docker CLI's `config.json` has a `proxies` entry, which the
        /// CLI adds to every container it creates (D-206).
        DockerCliProxies,
        /// The Docker CLI's `config.json` names a current context.
        DockerCliContext,
        /// The Docker CLI's `config.json` adds plugin folders, whose
        /// programs every `docker` call may start.
        DockerCliPluginDirs,
        /// The Docker CLI's `config.json` exists and is not JSON.
        DockerCliConfigUnreadable,
        /// `DOCKER_HOST` names a TCP engine without TLS (review F40).
        PlainTcpDockerHost,
        /// A Linux engine answered, with its key.
        EngineReady,
        /// The engine does not run.
        EngineNotRunning,
        /// The engine runs Windows containers.
        EngineWindowsContainers,
        /// `docker version` gave no answer.
        EngineUnreachable,
        /// Networks labelled by Officina that no container of theirs uses.
        OrphanNetworks,
        /// Volumes labelled by Officina that no container of theirs uses.
        OrphanVolumes,
    }
}

impl WorkshopCheck {
    /// The finding's level.
    pub const fn level(self) -> CheckLevel {
        match self {
            WorkshopCheck::DockerShim
            | WorkshopCheck::DockerMissing
            | WorkshopCheck::GitShim
            | WorkshopCheck::GitMissing
            | WorkshopCheck::EngineWindowsContainers => CheckLevel::Fail,
            WorkshopCheck::DockerCliProxies
            | WorkshopCheck::DockerCliPluginDirs
            | WorkshopCheck::DockerCliConfigUnreadable
            | WorkshopCheck::PlainTcpDockerHost
            | WorkshopCheck::EngineNotRunning
            | WorkshopCheck::EngineUnreachable
            | WorkshopCheck::OrphanNetworks
            | WorkshopCheck::OrphanVolumes => CheckLevel::Warn,
            WorkshopCheck::DockerResolved
            | WorkshopCheck::GitResolved
            | WorkshopCheck::OverridesSet
            | WorkshopCheck::DockerCliContext
            | WorkshopCheck::EngineReady => CheckLevel::Inform,
        }
    }

    /// The line's title: findings about one thing share it.
    pub const fn title(self) -> &'static str {
        match self {
            WorkshopCheck::DockerResolved
            | WorkshopCheck::DockerShim
            | WorkshopCheck::DockerMissing => "workshop docker",
            WorkshopCheck::GitResolved | WorkshopCheck::GitShim | WorkshopCheck::GitMissing => {
                "workshop git"
            }
            WorkshopCheck::OverridesSet => "workshop overrides",
            WorkshopCheck::DockerCliProxies
            | WorkshopCheck::DockerCliContext
            | WorkshopCheck::DockerCliPluginDirs
            | WorkshopCheck::DockerCliConfigUnreadable => "workshop docker config",
            WorkshopCheck::PlainTcpDockerHost => "workshop docker host",
            WorkshopCheck::EngineReady
            | WorkshopCheck::EngineNotRunning
            | WorkshopCheck::EngineWindowsContainers
            | WorkshopCheck::EngineUnreachable => "workshop engine",
            WorkshopCheck::OrphanNetworks | WorkshopCheck::OrphanVolumes => "workshop orphans",
        }
    }

    /// Where the finding sits in the section.
    pub fn order(self) -> usize {
        WorkshopCheck::ALL
            .iter()
            .position(|check| *check == self)
            .unwrap_or(WorkshopCheck::ALL.len())
    }
}

/// One finding and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub check: WorkshopCheck,
    pub detail: String,
}

impl Finding {
    pub fn new(check: WorkshopCheck, detail: impl Into<String>) -> Finding {
        Finding {
            check,
            detail: detail.into(),
        }
    }
}

/// The findings in section order, a check's several lines kept in the order
/// they came.
pub fn in_order(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by_key(|finding| finding.check.order());
    findings
}

/// The value of `name` in `env`, names compared ignoring ASCII case as
/// Windows does.
fn value<'e>(env: &'e [(String, String)], name: &str) -> Option<&'e str> {
    env.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// The names of `names` that `env` sets to a non-empty value, in `names`'
/// order, as doctor names them (never their values).
pub fn overrides_set(env: &[(String, String)], names: &[&str]) -> Option<Finding> {
    let set: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| value(env, name).is_some())
        .collect();
    (!set.is_empty()).then(|| {
        Finding::new(
            WorkshopCheck::OverridesSet,
            format!("set: {}", set.join(", ")),
        )
    })
}

/// A `DOCKER_HOST` on `tcp://` with neither `DOCKER_TLS_VERIFY` nor
/// `DOCKER_TLS` set: the engine's API in clear, which any process that can
/// reach the port may drive (review F40).
pub fn plain_tcp_docker_host(env: &[(String, String)]) -> Option<Finding> {
    let host = value(env, "DOCKER_HOST")?;
    let tls = value(env, "DOCKER_TLS_VERIFY").is_some() || value(env, "DOCKER_TLS").is_some();
    (host.to_ascii_lowercase().starts_with("tcp://") && !tls).then(|| {
        Finding::new(
            WorkshopCheck::PlainTcpDockerHost,
            "DOCKER_HOST is tcp:// without TLS: the engine's API is reachable in clear",
        )
    })
}

/// What the Docker CLI's `config.json` holds that changes what a `docker`
/// call does (D-228): read, never written; names only, never a proxy's
/// value.
pub fn docker_cli_config(text: &str) -> Vec<Finding> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![Finding::new(
            WorkshopCheck::DockerCliConfigUnreadable,
            "config.json is not JSON",
        )];
    };
    let mut findings = Vec::new();
    if value
        .get("proxies")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|proxies| !proxies.is_empty())
    {
        findings.push(Finding::new(
            WorkshopCheck::DockerCliProxies,
            "config.json has a proxies entry: the CLI adds its variables to every container it creates; the escape battery checks that none reaches a worker",
        ));
    }
    if let Some(context) = value
        .get("currentContext")
        .and_then(serde_json::Value::as_str)
        .filter(|c| !c.is_empty())
    {
        findings.push(Finding::new(
            WorkshopCheck::DockerCliContext,
            format!("current context {context}"),
        ));
    }
    let dirs: Vec<&str> = value
        .get("cliPluginsExtraDirs")
        .and_then(serde_json::Value::as_array)
        .map(|dirs| dirs.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    if !dirs.is_empty() {
        findings.push(Finding::new(
            WorkshopCheck::DockerCliPluginDirs,
            format!(
                "cliPluginsExtraDirs adds {}: their programs may start with any docker call",
                dirs.join(", ")
            ),
        ));
    }
    findings
}

/// How many of `rows` (networks or volumes Officina labelled) belong to no
/// container `containers` lists: a run's object whose run has no container
/// left, or an object of no run. The floor's login volume is the author's,
/// never an orphan (D-605).
pub fn orphans(containers: &[PsRow], rows: &[ObjectRow]) -> usize {
    rows.iter()
        .filter(|row| row.labels.get(ROLE_LABEL).map(String::as_str) != Some(LOGIN_ROLE))
        .filter(|row| match row.labels.get(RUN_LABEL) {
            Some(run) => !containers
                .iter()
                .any(|c| c.labels.get(RUN_LABEL) == Some(run)),
            None => true,
        })
        .count()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn every_check_has_a_title_and_a_place() {
        for (i, check) in WorkshopCheck::ALL.iter().enumerate() {
            assert!(check.title().starts_with("workshop "), "{check:?}");
            assert_eq!(check.order(), i);
            let _ = check.level();
        }
    }

    #[test]
    fn findings_sort_into_the_declared_order() {
        let mut findings: Vec<Finding> = WorkshopCheck::ALL
            .iter()
            .rev()
            .map(|c| Finding::new(*c, c.code()))
            .collect();
        findings = in_order(findings);
        let order: Vec<WorkshopCheck> = findings.iter().map(|f| f.check).collect();
        assert_eq!(order, WorkshopCheck::ALL);
    }

    #[test]
    fn overrides_are_named_never_valued() {
        let names = ["DOCKER_HOST", "OFFICINA_DOCKER_BIN", "DOCKER_CONTEXT"];
        let finding = overrides_set(
            &env(&[
                ("docker_host", "npipe:////./pipe/secret-pipe"),
                ("OFFICINA_DOCKER_BIN", "C:/x/docker.exe"),
                ("DOCKER_CONTEXT", ""),
            ]),
            &names,
        )
        .unwrap();
        assert_eq!(finding.detail, "set: DOCKER_HOST, OFFICINA_DOCKER_BIN");
        assert!(overrides_set(&env(&[("PATH", "x")]), &names).is_none());
    }

    #[test]
    fn a_plain_tcp_host_warns_unless_tls_is_on() {
        assert!(plain_tcp_docker_host(&env(&[("DOCKER_HOST", "tcp://192.0.2.10:2375")])).is_some());
        assert!(plain_tcp_docker_host(&env(&[("DOCKER_HOST", "TCP://192.0.2.10:2375")])).is_some());
        assert!(
            plain_tcp_docker_host(&env(&[
                ("DOCKER_HOST", "tcp://192.0.2.10:2376"),
                ("DOCKER_TLS_VERIFY", "1")
            ]))
            .is_none()
        );
        assert!(
            plain_tcp_docker_host(&env(&[("DOCKER_HOST", "npipe:////./pipe/docker_engine")]))
                .is_none()
        );
        assert!(plain_tcp_docker_host(&env(&[])).is_none());
    }

    #[test]
    fn docker_cli_config_entries_are_named() {
        let findings = docker_cli_config(
            r#"{"proxies":{"default":{"httpsProxy":"http://proxy.internal.example.org:3128"}},"currentContext":"desktop-linux","cliPluginsExtraDirs":["C:/plugins"]}"#,
        );
        let checks: Vec<WorkshopCheck> = findings.iter().map(|f| f.check).collect();
        assert_eq!(
            checks,
            [
                WorkshopCheck::DockerCliProxies,
                WorkshopCheck::DockerCliContext,
                WorkshopCheck::DockerCliPluginDirs
            ]
        );
        assert!(
            findings
                .iter()
                .all(|f| !f.detail.contains("proxy.internal"))
        );
        assert!(docker_cli_config(r#"{"auths":{}}"#).is_empty());
        assert!(docker_cli_config(r#"{"proxies":{}}"#).is_empty());
        assert_eq!(
            docker_cli_config("not json")[0].check,
            WorkshopCheck::DockerCliConfigUnreadable
        );
    }

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn orphans_are_objects_whose_run_has_no_container() {
        let containers = [PsRow {
            id: "a".into(),
            name: "officina-worker-live".into(),
            state: "running".into(),
            labels: labels(&[(RUN_LABEL, "live")]),
        }];
        let row = |name: &str, l: &[(&str, &str)]| ObjectRow {
            name: name.into(),
            labels: labels(l),
        };
        let rows = [
            row("officina-int-live", &[(RUN_LABEL, "live")]),
            row("officina-int-gone", &[(RUN_LABEL, "gone")]),
            row("officina-login-x", &[(ROLE_LABEL, LOGIN_ROLE)]),
            row("stray", &[]),
        ];
        assert_eq!(orphans(&containers, &rows), 2);
    }
}
