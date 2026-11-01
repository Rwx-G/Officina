//! The engine's calls and outputs (AD-98, D-204, D-205, D-206): every
//! `docker` invocation Officina makes is an [`EngineCall`] built here from
//! typed inputs, and every output it reads is parsed here. The host's
//! `workshop::engine::execute` only runs what this module built.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::names::{MANAGED_LABEL, ObjectName};
use super::vector::{ContainerRole, ContainerVector};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// One use of a `docker` subcommand (AD-98 with D-205's three reads).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum EngineCallKind {
        Version,
        Info,
        Ps,
        Inspect,
        ImageInspect,
        NetworkLs,
        VolumeLs,
        Logs,
        Stop,
        Build,
        NetworkCreate,
        NetworkConnect,
        NetworkRm,
        VolumeCreate,
        VolumeRm,
        Create,
        CpIn,
        CpOut,
        Start,
        StartAttached,
        Rm,
    }
}

impl EngineCallKind {
    /// True for a kind that creates, starts, copies, builds or removes
    /// (D-204): compiled into the window only. Reads, the followers and
    /// `stop`, which only reduces, are the read half `officina.exe` keeps.
    pub const fn is_launch(self) -> bool {
        match self {
            EngineCallKind::Version
            | EngineCallKind::Info
            | EngineCallKind::Ps
            | EngineCallKind::Inspect
            | EngineCallKind::ImageInspect
            | EngineCallKind::NetworkLs
            | EngineCallKind::VolumeLs
            | EngineCallKind::Logs
            | EngineCallKind::Stop => false,
            EngineCallKind::Build
            | EngineCallKind::NetworkCreate
            | EngineCallKind::NetworkConnect
            | EngineCallKind::NetworkRm
            | EngineCallKind::VolumeCreate
            | EngineCallKind::VolumeRm
            | EngineCallKind::Create
            | EngineCallKind::CpIn
            | EngineCallKind::CpOut
            | EngineCallKind::Start
            | EngineCallKind::StartAttached
            | EngineCallKind::Rm => true,
        }
    }

    /// The deadline of a call of this kind (AD-98, D-205); `None` for the
    /// followers, which run as long as their container, and for an
    /// attached start, which takes its role's ([`attached_deadline_ms`]).
    pub const fn deadline_ms(self) -> Option<u64> {
        match self {
            EngineCallKind::Logs | EngineCallKind::StartAttached => None,
            EngineCallKind::Create | EngineCallKind::Start => Some(30_000),
            EngineCallKind::CpIn | EngineCallKind::CpOut => Some(120_000),
            EngineCallKind::Build => Some(30 * 60 * 1000),
            EngineCallKind::Version
            | EngineCallKind::Info
            | EngineCallKind::Ps
            | EngineCallKind::Inspect
            | EngineCallKind::ImageInspect
            | EngineCallKind::NetworkLs
            | EngineCallKind::VolumeLs
            | EngineCallKind::Stop
            | EngineCallKind::NetworkCreate
            | EngineCallKind::NetworkConnect
            | EngineCallKind::NetworkRm
            | EngineCallKind::VolumeCreate
            | EngineCallKind::VolumeRm
            | EngineCallKind::Rm => Some(10_000),
        }
    }
}

/// An attached start's deadline by role (D-205): the packer 120 s, the
/// inventory 60 s, a battery phase its limit plus 60 s; the other roles
/// start detached.
pub const fn attached_deadline_ms(role: ContainerRole, battery_limit_ms: u64) -> Option<u64> {
    match role {
        ContainerRole::Packer => Some(120_000),
        ContainerRole::Inventory | ContainerRole::AuthProbe | ContainerRole::Scanner => {
            Some(60_000)
        }
        ContainerRole::Battery => Some(battery_limit_ms.saturating_add(60_000)),
        ContainerRole::Worker
        | ContainerRole::Gateway
        | ContainerRole::BatteryGateway
        | ContainerRole::Decoy => None,
    }
}

/// The most stdout kept of a captured call (AD-98).
pub const STDOUT_CAP: usize = 1024 * 1024;
/// The stderr kept of a failed call (AD-98).
pub const STDERR_CAP: usize = 2 * 1024;

/// The `--format` of `inspect` (D-206, review F14): the state and the
/// labels only, never the object, whose `Config.Env` holds the credential.
pub const INSPECT_FORMAT: &str = r#"{"state":{{json .State}},"labels":{{json .Config.Labels}}}"#;
/// The `--format` of `image inspect`: the id, the labels, the `ENV` lines.
pub const IMAGE_INSPECT_FORMAT: &str =
    r#"{"id":{{json .Id}},"labels":{{json .Config.Labels}},"env":{{json .Config.Env}}}"#;
/// The `--format` of every listing and of `version` and `info`.
pub const JSON_FORMAT: &str = "{{json .}}";

/// One `docker` call: its kind, its arguments after `docker`, its deadline
/// and whether it writes to the child's stdin. Fields are private; the
/// per-kind builders below are the only constructors.
///
/// ```compile_fail,E0451
/// use officina_core::workshop::engine::{EngineCall, EngineCallKind};
/// let call = EngineCall {
///     kind: EngineCallKind::Version,
///     args: vec!["run".to_owned(), "--privileged".to_owned()],
///     deadline_ms: None,
///     stdin: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineCall {
    kind: EngineCallKind,
    args: Vec<String>,
    deadline_ms: Option<u64>,
    stdin: bool,
}

fn strings<const N: usize>(parts: [&str; N]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_owned()).collect()
}

fn label_args(labels: &BTreeMap<String, String>) -> Vec<String> {
    labels
        .iter()
        .flat_map(|(k, v)| ["--label".to_owned(), format!("{k}={v}")])
        .collect()
}

fn managed_filter() -> String {
    format!("label={}={}", MANAGED_LABEL.0, MANAGED_LABEL.1)
}

/// What a build is given (AD-99; story 6.8 fills it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInputs {
    pub tag: String,
    /// The build context's folder, as the host writes or reads it.
    pub context: String,
    /// The Dockerfile, when it is not the context's `Dockerfile`.
    pub dockerfile: Option<String>,
    pub labels: BTreeMap<String, String>,
    pub build_args: BTreeMap<String, String>,
    /// The engine's `docker`-driver builder (the gate 1 entry, D-203).
    pub builder: Option<String>,
}

impl EngineCall {
    fn new(kind: EngineCallKind, args: Vec<String>) -> EngineCall {
        EngineCall {
            kind,
            args,
            deadline_ms: kind.deadline_ms(),
            stdin: false,
        }
    }

    pub fn kind(&self) -> EngineCallKind {
        self.kind
    }

    /// The arguments after `docker`.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// True when the host writes to the child's stdin (an attached start
    /// fed a list or records).
    pub fn writes_stdin(&self) -> bool {
        self.stdin
    }

    /// `docker version` as JSON: the engine's identity (AD-98).
    pub fn version() -> EngineCall {
        EngineCall::new(
            EngineCallKind::Version,
            strings(["version", "--format", JSON_FORMAT]),
        )
    }

    /// `docker info` as JSON: memory, CPUs, clock and settings (D-205).
    pub fn info() -> EngineCall {
        EngineCall::new(
            EngineCallKind::Info,
            strings(["info", "--format", JSON_FORMAT]),
        )
    }

    /// Every container Officina labelled, any state.
    pub fn ps() -> EngineCall {
        let mut args = strings(["ps", "--all", "--no-trunc", "--filter"]);
        args.push(managed_filter());
        args.extend(strings(["--format", JSON_FORMAT]));
        EngineCall::new(EngineCallKind::Ps, args)
    }

    /// One container's state and labels, never its environment (D-206).
    pub fn inspect(container: &ObjectName) -> EngineCall {
        let mut args = strings(["inspect", "--type", "container", "--format", INSPECT_FORMAT]);
        args.push(container.as_str().to_owned());
        EngineCall::new(EngineCallKind::Inspect, args)
    }

    /// One image's id, labels and `ENV` lines.
    pub fn image_inspect(image: &ObjectName) -> EngineCall {
        let mut args = strings(["image", "inspect", "--format", IMAGE_INSPECT_FORMAT]);
        args.push(image.as_str().to_owned());
        EngineCall::new(EngineCallKind::ImageInspect, args)
    }

    /// Every network Officina labelled (D-205).
    pub fn network_ls() -> EngineCall {
        let mut args = strings(["network", "ls", "--no-trunc", "--filter"]);
        args.push(managed_filter());
        args.extend(strings(["--format", JSON_FORMAT]));
        EngineCall::new(EngineCallKind::NetworkLs, args)
    }

    /// Every volume Officina labelled (D-205).
    pub fn volume_ls() -> EngineCall {
        let mut args = strings(["volume", "ls", "--filter"]);
        args.push(managed_filter());
        args.extend(strings(["--format", JSON_FORMAT]));
        EngineCall::new(EngineCallKind::VolumeLs, args)
    }

    /// A follower: the container's output from its start, timestamped
    /// (AD-109).
    pub fn logs(container: &ObjectName) -> EngineCall {
        let mut args = strings(["logs", "--follow", "--timestamps"]);
        args.push(container.as_str().to_owned());
        EngineCall::new(EngineCallKind::Logs, args)
    }

    /// Stops containers with AD-102's 3 s grace (AD-116).
    pub fn stop(containers: &[ObjectName]) -> EngineCall {
        let mut args = strings(["stop", "--time", "3"]);
        args.extend(containers.iter().map(|c| c.as_str().to_owned()));
        EngineCall::new(EngineCallKind::Stop, args)
    }

    /// A build on the engine's default network (R61), never pulling the
    /// base it names (the gate 1 entry).
    pub fn build(inputs: &BuildInputs) -> EngineCall {
        let mut args = strings([
            "build",
            "--progress",
            "plain",
            "--network",
            "default",
            "--pull=false",
        ]);
        if let Some(builder) = &inputs.builder {
            args.extend(["--builder".to_owned(), builder.clone()]);
        }
        args.extend(label_args(&inputs.labels));
        for (name, value) in &inputs.build_args {
            args.extend(["--build-arg".to_owned(), format!("{name}={value}")]);
        }
        if let Some(dockerfile) = &inputs.dockerfile {
            args.extend(["--file".to_owned(), dockerfile.clone()]);
        }
        args.extend([
            "--tag".to_owned(),
            inputs.tag.clone(),
            inputs.context.clone(),
        ]);
        EngineCall::new(EngineCallKind::Build, args)
    }

    /// A network; `internal` gives it no route out (AD-104).
    pub fn network_create(
        network: &ObjectName,
        internal: bool,
        labels: &BTreeMap<String, String>,
    ) -> EngineCall {
        let mut args = strings(["network", "create"]);
        if internal {
            args.push("--internal".to_owned());
        }
        args.extend(label_args(labels));
        args.push(network.as_str().to_owned());
        EngineCall::new(EngineCallKind::NetworkCreate, args)
    }

    pub fn network_connect(network: &ObjectName, container: &ObjectName) -> EngineCall {
        let mut args = strings(["network", "connect"]);
        args.extend([network.as_str().to_owned(), container.as_str().to_owned()]);
        EngineCall::new(EngineCallKind::NetworkConnect, args)
    }

    pub fn network_rm(networks: &[ObjectName]) -> EngineCall {
        let mut args = strings(["network", "rm"]);
        args.extend(networks.iter().map(|n| n.as_str().to_owned()));
        EngineCall::new(EngineCallKind::NetworkRm, args)
    }

    pub fn volume_create(volume: &ObjectName, labels: &BTreeMap<String, String>) -> EngineCall {
        let mut args = strings(["volume", "create"]);
        args.extend(label_args(labels));
        args.push(volume.as_str().to_owned());
        EngineCall::new(EngineCallKind::VolumeCreate, args)
    }

    pub fn volume_rm(volumes: &[ObjectName]) -> EngineCall {
        let mut args = strings(["volume", "rm"]);
        args.extend(volumes.iter().map(|v| v.as_str().to_owned()));
        EngineCall::new(EngineCallKind::VolumeRm, args)
    }

    /// `docker create` of a vector, its arguments copied (D-243).
    pub fn create(vector: &ContainerVector) -> EngineCall {
        let mut args = strings(["create", "--name"]);
        args.push(vector.name().as_str().to_owned());
        args.extend(vector.options().iter().cloned());
        args.push(vector.image().to_owned());
        args.extend(vector.command().iter().cloned());
        EngineCall::new(EngineCallKind::Create, args)
    }

    /// Copies a host folder's content into a created container (AD-107).
    pub fn cp_in(source: &str, container: &ObjectName, target: &str) -> EngineCall {
        EngineCall::new(
            EngineCallKind::CpIn,
            vec![
                "cp".to_owned(),
                source.to_owned(),
                format!("{}:{target}", container.as_str()),
            ],
        )
    }

    /// Streams a container's folder out as a tar on stdout (D-218).
    pub fn cp_out(container: &ObjectName, source: &str) -> EngineCall {
        EngineCall::new(
            EngineCallKind::CpOut,
            vec![
                "cp".to_owned(),
                format!("{}:{source}", container.as_str()),
                "-".to_owned(),
            ],
        )
    }

    /// Starts a created container and returns (AD-107).
    pub fn start(container: &ObjectName) -> EngineCall {
        let mut args = strings(["start"]);
        args.push(container.as_str().to_owned());
        EngineCall::new(EngineCallKind::Start, args)
    }

    /// Starts a container and waits for it, its output on the call's
    /// (D-205); `stdin` feeds it.
    pub fn start_attached(
        container: &ObjectName,
        role: ContainerRole,
        battery_limit_ms: u64,
        stdin: bool,
    ) -> EngineCall {
        let mut args = strings(["start", "--attach"]);
        if stdin {
            args.push("--interactive".to_owned());
        }
        args.push(container.as_str().to_owned());
        EngineCall {
            kind: EngineCallKind::StartAttached,
            args,
            deadline_ms: attached_deadline_ms(role, battery_limit_ms),
            stdin,
        }
    }

    /// Removes containers and their anonymous volumes.
    pub fn rm(containers: &[ObjectName]) -> EngineCall {
        let mut args = strings(["rm", "--force", "--volumes"]);
        args.extend(containers.iter().map(|c| c.as_str().to_owned()));
        EngineCall::new(EngineCallKind::Rm, args)
    }
}

/// What reaches a `docker.exe` child of each kind (D-206 as D-616 amends
/// it): an allow list, never a removal list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineEnvRule {
    /// The spawn module's contained system names found in the base, every
    /// `DOCKER_*` name, and `NoDefaultCurrentDirectoryInExePath=1`.
    SystemAndDocker,
}

/// The environment rule of `kind`, one exhaustive `match` (D-206): every
/// kind takes the same allow list until a later story's role rule adds the
/// one credential name to a worker's or a battery's `create` (AD-105).
pub const fn env_rule(kind: EngineCallKind) -> EngineEnvRule {
    match kind {
        EngineCallKind::Version
        | EngineCallKind::Info
        | EngineCallKind::Ps
        | EngineCallKind::Inspect
        | EngineCallKind::ImageInspect
        | EngineCallKind::NetworkLs
        | EngineCallKind::VolumeLs
        | EngineCallKind::Logs
        | EngineCallKind::Stop
        | EngineCallKind::Build
        | EngineCallKind::NetworkCreate
        | EngineCallKind::NetworkConnect
        | EngineCallKind::NetworkRm
        | EngineCallKind::VolumeCreate
        | EngineCallKind::VolumeRm
        | EngineCallKind::Create
        | EngineCallKind::CpIn
        | EngineCallKind::CpOut
        | EngineCallKind::Start
        | EngineCallKind::StartAttached
        | EngineCallKind::Rm => EngineEnvRule::SystemAndDocker,
    }
}

/// The prefix of the names `docker.exe` reads for itself (`DOCKER_HOST`,
/// `DOCKER_CONTEXT`, `DOCKER_CONFIG`...), the author's to set.
pub const DOCKER_ENV_PREFIX: &str = "DOCKER_";

/// The environment of a `docker.exe` child of `kind`, from `base`: names
/// compared ignoring ASCII case, the base's spelling kept, and
/// [`crate::spawn::NO_CURRENT_DIR_VAR`] set to `1`.
pub fn engine_env(kind: EngineCallKind, base: &[(String, String)]) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = match env_rule(kind) {
        EngineEnvRule::SystemAndDocker => base
            .iter()
            .filter(|(name, _)| {
                crate::spawn::CONTAINED_ENV_SYSTEM
                    .iter()
                    .any(|kept| kept.eq_ignore_ascii_case(name))
                    || name.to_ascii_uppercase().starts_with(DOCKER_ENV_PREFIX)
            })
            .cloned()
            .collect(),
    };
    env.push((crate::spawn::NO_CURRENT_DIR_VAR.to_owned(), "1".to_owned()));
    env
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the engine cannot run the workshop (AD-98).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum EngineUnavailable {
        /// `docker version` printed no `Server`: the engine is not running.
        NotRunning,
        /// The engine runs Windows containers.
        WindowsContainers,
    }
}

/// The engine's identity from `docker version` (AD-98).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineInfo {
    pub platform: String,
    pub version: String,
    pub kernel: String,
}

impl EngineInfo {
    /// Parses `docker version --format "{{json .}}"`.
    pub fn parse(json: &str) -> Result<EngineInfo, EngineUnavailable> {
        let value: serde_json::Value =
            serde_json::from_str(json.trim()).map_err(|_| EngineUnavailable::NotRunning)?;
        let server = value
            .get("Server")
            .filter(|s| s.is_object())
            .ok_or(EngineUnavailable::NotRunning)?;
        let text = |v: Option<&serde_json::Value>| {
            v.and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        if server.get("Os").and_then(serde_json::Value::as_str) != Some("linux") {
            return Err(EngineUnavailable::WindowsContainers);
        }
        Ok(EngineInfo {
            platform: text(server.get("Platform").and_then(|p| p.get("Name"))),
            version: text(server.get("Version")),
            kernel: text(server.get("KernelVersion")),
        })
    }
}

/// What `docker info` tells (D-205, D-232, D-242).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InfoFacts {
    pub memory_bytes: u64,
    pub cpus: u32,
    /// The engine's clock, as it prints it (RFC 3339).
    pub system_time: String,
    /// SHA-256 over the security options, the runtimes' names and the proxy
    /// fields: a Docker Desktop setting that changes exposure voids the
    /// battery (D-232).
    pub settings_digest: String,
}

impl InfoFacts {
    /// Parses `docker info --format "{{json .}}"`; `None` when it is not a
    /// JSON object.
    pub fn parse(json: &str) -> Option<InfoFacts> {
        let value: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
        let object = value.as_object()?;
        let mut runtimes: Vec<&String> = object
            .get("Runtimes")
            .and_then(serde_json::Value::as_object)
            .map(|r| r.keys().collect())
            .unwrap_or_default();
        runtimes.sort();
        let settings = serde_json::json!({
            "security_options": object.get("SecurityOptions"),
            "runtimes": runtimes,
            "http_proxy": object.get("HttpProxy"),
            "https_proxy": object.get("HttpsProxy"),
            "no_proxy": object.get("NoProxy"),
        });
        Some(InfoFacts {
            memory_bytes: object
                .get("MemTotal")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            cpus: object
                .get("NCPU")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0),
            system_time: object
                .get("SystemTime")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            settings_digest: crate::sha256_hex(settings.to_string().as_bytes()),
        })
    }
}

/// What a battery's validity keys on (AD-117, D-232): the platform, the
/// engine's version, its kernel and its settings digest, joined.
pub fn engine_key(info: &EngineInfo, facts: &InfoFacts) -> String {
    format!(
        "{}|{}|{}|{}",
        info.platform, info.version, info.kernel, facts.settings_digest
    )
}

/// Labels as `docker` lists them (`a=b,c=d`), split.
fn label_map(text: &str) -> BTreeMap<String, String> {
    text.split(',')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (k.to_owned(), v.to_owned()),
            None => (pair.to_owned(), String::new()),
        })
        .collect()
}

fn field<'v>(value: &'v serde_json::Value, key: &str) -> &'v str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// One container of `docker ps`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PsRow {
    pub id: String,
    pub name: String,
    pub state: String,
    pub labels: BTreeMap<String, String>,
}

/// One network or volume of a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectRow {
    pub name: String,
    pub labels: BTreeMap<String, String>,
}

/// The JSON lines of a listing; a line that is not an object is skipped.
fn json_lines(output: &str) -> impl Iterator<Item = serde_json::Value> + '_ {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(serde_json::Value::is_object)
}

/// Parses `docker ps --format "{{json .}}"`.
pub fn parse_ps(output: &str) -> Vec<PsRow> {
    json_lines(output)
        .map(|row| PsRow {
            id: field(&row, "ID").to_owned(),
            name: field(&row, "Names").to_owned(),
            state: field(&row, "State").to_owned(),
            labels: label_map(field(&row, "Labels")),
        })
        .collect()
}

/// Parses `docker network ls` or `docker volume ls` as JSON lines.
pub fn parse_object_rows(output: &str) -> Vec<ObjectRow> {
    json_lines(output)
        .map(|row| ObjectRow {
            name: field(&row, "Name").to_owned(),
            labels: label_map(field(&row, "Labels")),
        })
        .collect()
}

/// A container as [`INSPECT_FORMAT`] shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerState {
    pub status: String,
    pub running: bool,
    pub exit_code: Option<i64>,
    pub oom_killed: bool,
    pub labels: BTreeMap<String, String>,
}

/// Parses `inspect` with [`INSPECT_FORMAT`]; `None` for any other shape.
pub fn parse_inspect(output: &str) -> Option<ContainerState> {
    let value: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    let state = value.get("state")?;
    let labels = value
        .get("labels")
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    Some(ContainerState {
        status: field(state, "Status").to_owned(),
        running: state
            .get("Running")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        exit_code: state.get("ExitCode").and_then(serde_json::Value::as_i64),
        oom_killed: state
            .get("OOMKilled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        labels,
    })
}

/// An image as [`IMAGE_INSPECT_FORMAT`] shows it: its `ENV` names only,
/// never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageFacts {
    pub id: String,
    pub labels: BTreeMap<String, String>,
    pub env_names: Vec<String>,
}

/// Parses `image inspect` with [`IMAGE_INSPECT_FORMAT`].
pub fn parse_image_inspect(output: &str) -> Option<ImageFacts> {
    let value: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    let id = value.get("id")?.as_str()?.to_owned();
    let labels = value
        .get("labels")
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let mut env_names: Vec<String> = value
        .get("env")
        .and_then(serde_json::Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|line| line.split_once('=').map_or(line, |(n, _)| n).to_owned())
                .collect()
        })
        .unwrap_or_default();
    env_names.sort();
    env_names.dedup();
    Some(ImageFacts {
        id,
        labels,
        env_names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> ObjectName {
        ObjectName::parse(text).unwrap()
    }

    #[test]
    fn is_launch_names_every_kind() {
        let reads: Vec<EngineCallKind> = EngineCallKind::ALL
            .iter()
            .copied()
            .filter(|k| !k.is_launch())
            .collect();
        assert!(reads.contains(&EngineCallKind::Stop), "stop only reduces");
        assert!(reads.contains(&EngineCallKind::Logs));
        for kind in EngineCallKind::ALL {
            let creates = matches!(
                kind.code().split('_').next(),
                Some("build" | "create" | "cp" | "start" | "rm")
            ) || kind.code().ends_with("_create")
                || kind.code().ends_with("_rm")
                || kind.code().ends_with("_connect");
            assert_eq!(kind.is_launch(), creates, "{kind:?}");
        }
    }

    #[test]
    fn windows_containers_are_unavailable() {
        let json =
            r#"{"Client":{"Version":"29.8.2"},"Server":{"Os":"windows","Version":"29.8.2"}}"#;
        assert_eq!(
            EngineInfo::parse(json),
            Err(EngineUnavailable::WindowsContainers)
        );
    }

    #[test]
    fn a_missing_server_is_not_running() {
        for json in [
            r#"{"Client":{"Version":"29.8.2"},"Server":null}"#,
            "",
            "not json",
        ] {
            assert_eq!(
                EngineInfo::parse(json),
                Err(EngineUnavailable::NotRunning),
                "{json}"
            );
        }
    }

    #[test]
    fn a_changed_security_option_changes_the_engine_key() {
        let info = EngineInfo {
            platform: "Docker Desktop 4.95.0 (242831)".to_owned(),
            version: "29.8.2".to_owned(),
            kernel: "6.18.33.2-microsoft-standard-WSL2".to_owned(),
        };
        let base = r#"{"MemTotal":1,"NCPU":2,"SystemTime":"t","SecurityOptions":["name=seccomp,profile=builtin"],"Runtimes":{"runc":{}},"HttpProxy":"","HttpsProxy":"","NoProxy":""}"#;
        let key = engine_key(&info, &InfoFacts::parse(base).unwrap());
        for changed in [
            base.replace("profile=builtin", "profile=unconfined"),
            base.replace(r#""runc":{}"#, r#""runc":{},"sysbox":{}"#),
            base.replace(
                r#""HttpProxy":"""#,
                r#""HttpProxy":"proxy.example.com:8080""#,
            ),
        ] {
            assert_ne!(engine_key(&info, &InfoFacts::parse(&changed).unwrap()), key);
        }
        let clock = base.replace(r#""SystemTime":"t""#, r#""SystemTime":"u""#);
        assert_eq!(
            engine_key(&info, &InfoFacts::parse(&clock).unwrap()),
            key,
            "the clock is not a setting"
        );
    }

    #[test]
    fn inspect_never_selects_the_environment() {
        let call = EngineCall::inspect(&name("officina-worker-0a"));
        let joined = call.args().join(" ");
        assert!(!joined.contains("Env"), "{joined}");
        assert!(joined.contains(".State") && joined.contains(".Config.Labels"));
        let full = r#"{"state":{"Status":"exited","Running":false,"ExitCode":3,"OOMKilled":true},"labels":{"officina.managed":"1"}}"#;
        let parsed = parse_inspect(full).unwrap();
        assert_eq!(
            (parsed.exit_code, parsed.oom_killed, parsed.running),
            (Some(3), true, false)
        );
    }

    #[test]
    fn a_name_cannot_read_as_an_option() {
        for bad in ["-x", "--privileged", "A", "a b", "a,b", "a\"b", ""] {
            assert!(ObjectName::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn create_copies_the_vector() {
        let vector = ContainerVector::sample(ContainerRole::Worker);
        let call = EngineCall::create(&vector);
        assert_eq!(call.kind(), EngineCallKind::Create);
        assert_eq!(&call.args()[..2], ["create", "--name"]);
        assert_eq!(call.args()[2], vector.name().as_str());
        assert!(call.args().ends_with(vector.command()));
        assert!(call.args().contains(&vector.image().to_owned()));
    }

    #[test]
    fn image_env_keeps_names_only() {
        let facts = parse_image_inspect(
            r#"{"id":"sha256:ab","labels":{"officina.base":"workshop"},"env":["PATH=/usr/bin","SECRET=value","PATH=/x"]}"#,
        )
        .unwrap();
        assert_eq!(facts.env_names, ["PATH", "SECRET"]);
        assert!(!serde_json::to_string(&facts).unwrap().contains("value"));
    }

    /// D-616: for every kind, the session's names, every `OFFICINA_*`
    /// name, both credential names in any case and the proxy never reach
    /// `docker.exe`; every system name present and every `DOCKER_*` name
    /// do.
    #[test]
    fn the_environment_rule_names_every_kind() {
        use crate::spawn::{CONTAINED_ENV_SYSTEM, NO_CURRENT_DIR_VAR, SESSION_ENV_REMOVED};
        let mut base: Vec<(String, String)> = SESSION_ENV_REMOVED
            .iter()
            .map(|n| ((*n).to_owned(), "x".to_owned()))
            .collect();
        for name in [
            "OFFICINA_HOME",
            "Claude_Code_OAuth_Token",
            "anthropic_api_key",
            "HTTPS_PROXY",
            "UNRELATED",
            "DOCKER_HOST",
            "docker_context",
        ] {
            base.push((name.to_owned(), "x".to_owned()));
        }
        base.extend(
            CONTAINED_ENV_SYSTEM
                .iter()
                .map(|n| (n.to_lowercase(), "x".to_owned())),
        );
        for kind in EngineCallKind::ALL {
            let env = engine_env(*kind, &base);
            let names: Vec<&str> = env.iter().map(|(n, _)| n.as_str()).collect();
            for name in &names {
                let allowed = CONTAINED_ENV_SYSTEM
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(name))
                    || name.to_ascii_uppercase().starts_with(DOCKER_ENV_PREFIX)
                    || *name == NO_CURRENT_DIR_VAR;
                assert!(allowed, "{kind:?} passes {name}");
            }
            for system in CONTAINED_ENV_SYSTEM {
                assert!(
                    names.iter().any(|n| n.eq_ignore_ascii_case(system)),
                    "{kind:?} drops {system}"
                );
            }
            assert!(names.contains(&"DOCKER_HOST") && names.contains(&"docker_context"));
            assert_eq!(
                names.iter().filter(|n| **n == NO_CURRENT_DIR_VAR).count(),
                1
            );
        }
    }

    /// One builder call per kind, the golden cases of `workshop.json`'s
    /// `engine` section (D-204).
    fn sample_calls() -> Vec<(String, EngineCall)> {
        let worker = name("officina-worker-0123456789abcdef0123456789abcdef");
        let network = name("officina-int-0123456789abcdef0123456789abcdef");
        let volume = name("officina-vol-0123456789abcdef0123456789abcdef");
        let labels = BTreeMap::from([
            ("officina.managed".to_owned(), "1".to_owned()),
            ("officina.floor".to_owned(), "00000000000000aa".to_owned()),
        ]);
        let build = BuildInputs {
            tag: "officina-workshop:local".to_owned(),
            context: "C:/path/to/home/workshop/build/officina-workshop".to_owned(),
            dockerfile: None,
            labels: labels.clone(),
            build_args: BTreeMap::from([("CLAUDE_CODE_VERSION".to_owned(), "2.1.316".to_owned())]),
            builder: Some("desktop-linux".to_owned()),
        };
        vec![
            ("version".to_owned(), EngineCall::version()),
            ("info".to_owned(), EngineCall::info()),
            ("ps".to_owned(), EngineCall::ps()),
            ("inspect".to_owned(), EngineCall::inspect(&worker)),
            (
                "image inspect".to_owned(),
                EngineCall::image_inspect(&name("officina-workshop:local")),
            ),
            ("network ls".to_owned(), EngineCall::network_ls()),
            ("volume ls".to_owned(), EngineCall::volume_ls()),
            ("logs".to_owned(), EngineCall::logs(&worker)),
            (
                "stop".to_owned(),
                EngineCall::stop(std::slice::from_ref(&worker)),
            ),
            ("build".to_owned(), EngineCall::build(&build)),
            (
                "network create".to_owned(),
                EngineCall::network_create(&network, true, &labels),
            ),
            (
                "network connect".to_owned(),
                EngineCall::network_connect(&network, &worker),
            ),
            (
                "network rm".to_owned(),
                EngineCall::network_rm(std::slice::from_ref(&network)),
            ),
            (
                "volume create".to_owned(),
                EngineCall::volume_create(&volume, &labels),
            ),
            (
                "volume rm".to_owned(),
                EngineCall::volume_rm(std::slice::from_ref(&volume)),
            ),
            (
                "create".to_owned(),
                EngineCall::create(&ContainerVector::sample(ContainerRole::Worker)),
            ),
            (
                "cp in".to_owned(),
                EngineCall::cp_in(
                    "C:/path/to/home/workshop/runs/x/in/.",
                    &worker,
                    "/work/.officina/in",
                ),
            ),
            (
                "cp out".to_owned(),
                EngineCall::cp_out(&worker, "/work/.officina/out"),
            ),
            ("start".to_owned(), EngineCall::start(&worker)),
            (
                "start attached packer".to_owned(),
                EngineCall::start_attached(&worker, ContainerRole::Packer, 0, false),
            ),
            (
                "rm".to_owned(),
                EngineCall::rm(std::slice::from_ref(&worker)),
            ),
        ]
    }

    const VECTORS: &str = include_str!("../../../../tests/vectors/workshop.json");

    fn vectors() -> serde_json::Value {
        serde_json::from_str(VECTORS).expect("workshop.json parses")
    }

    fn built_calls() -> serde_json::Value {
        let cases: Vec<serde_json::Value> = sample_calls()
            .into_iter()
            .map(|(name, call)| {
                serde_json::json!({
                    "name": name,
                    "kind": call.kind(),
                    "launch": call.kind().is_launch(),
                    "args": call.args(),
                    "deadline_ms": call.deadline_ms(),
                    "stdin": call.writes_stdin(),
                })
            })
            .collect();
        serde_json::Value::Array(cases)
    }

    #[test]
    fn every_engine_call_kind_has_a_case() {
        let vectors = vectors();
        let cases = vectors["engine"]["calls"].as_array().expect("calls");
        for kind in EngineCallKind::ALL {
            assert!(
                cases.iter().any(|c| c["kind"] == serde_json::json!(kind)),
                "{kind:?} has no case in tests/vectors/workshop.json"
            );
        }
    }

    /// The golden cases equal what the builders make now. With
    /// `OFFICINA_WRITE_VECTORS=1` the test writes the section instead,
    /// for a reviewed change of a builder.
    #[test]
    fn engine_cases_match_their_builders() {
        let built = built_calls();
        if writing() {
            write_section("calls", built);
            return;
        }
        assert_eq!(vectors()["engine"]["calls"], built);
    }

    /// Each recorded output parses to what the file says; with
    /// `OFFICINA_WRITE_VECTORS=1` the expectations are written instead.
    #[test]
    fn engine_outputs_parse_as_recorded() {
        let mut vectors = vectors();
        let writing = writing();
        let outputs = vectors["engine"]["outputs"]
            .as_array_mut()
            .expect("outputs");
        let mut parsers = std::collections::BTreeSet::new();
        for case in outputs.iter_mut() {
            let input = case["input"].as_str().expect("input");
            let parsed = match case["parser"].as_str().expect("parser") {
                "version" => {
                    serde_json::to_value(EngineInfo::parse(input).map_err(|e| e.code())).unwrap()
                }
                "info" => serde_json::to_value(InfoFacts::parse(input)).unwrap(),
                "ps" => serde_json::to_value(parse_ps(input)).unwrap(),
                "inspect" => serde_json::to_value(parse_inspect(input)).unwrap(),
                "image inspect" => serde_json::to_value(parse_image_inspect(input)).unwrap(),
                "network ls" | "volume ls" => {
                    serde_json::to_value(parse_object_rows(input)).unwrap()
                }
                other => panic!("unknown parser {other}"),
            };
            parsers.insert(case["parser"].as_str().unwrap().to_owned());
            if writing {
                case["expect"] = parsed;
            } else {
                assert_eq!(parsed, case["expect"], "{}", case["name"]);
            }
        }
        if writing {
            let outputs = vectors["engine"]["outputs"].clone();
            write_section("outputs", outputs);
        }
        for parser in [
            "version",
            "info",
            "ps",
            "inspect",
            "image inspect",
            "network ls",
            "volume ls",
        ] {
            assert!(parsers.contains(parser), "{parser} has no output case");
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "a test's write mode for a reviewed change of the golden file"
    )]
    fn writing() -> bool {
        std::env::var("OFFICINA_WRITE_VECTORS").is_ok_and(|v| v == "1")
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "a test's write mode for a reviewed change of the golden file"
    )]
    fn write_section(key: &str, value: serde_json::Value) {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/workshop.json"
        );
        let mut all = vectors();
        all["engine"][key] = value;
        let text = serde_json::to_string_pretty(&all).expect("JSON") + "\n";
        std::fs::write(path, text).expect("written");
    }
}
