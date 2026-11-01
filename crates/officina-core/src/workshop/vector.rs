//! Container roles and vectors (AD-102, AD-104, D-206, D-243): one builder
//! per role, the only constructors of a [`ContainerVector`], which alone own
//! the fixed options and labels, so no caller can assemble a container's
//! arguments by hand.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::floor::CredentialKind;
use super::gateway::{GatewayConfig, MAX_TUNNELS};
use super::names::{ObjectName, login_volume};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a container is for (AD-102, AD-117, D-608, D-609).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ContainerRole {
        Worker,
        Gateway,
        Packer,
        Inventory,
        Battery,
        BatteryGateway,
        Decoy,
        AuthProbe,
        Scanner,
    }
}

/// The entry point every workshop container but the gateways runs (AD-108).
pub const ENTRY_PROGRAM: &str = "/opt/officina/bin/officina-entry";
/// Claude Code in the base image (AD-99).
pub const CLAUDE_PROGRAM: &str = "/opt/officina/bin/claude";
/// The gateway's program in its `FROM scratch` image (AD-99).
pub const GATEWAY_PROGRAM: &str = "/officina-gateway";
/// The user every container of the workshop's image runs as (AD-99).
pub const WORKER_USER: &str = "1000:1000";
/// The gateway's user (AD-102).
pub const GATEWAY_USER: &str = "65534:65534";
/// The run volume's place (AD-102).
pub const WORK_DIR: &str = "/work";
/// Where each mount is, read-only, by index (AD-102).
pub const MOUNTS_DIR: &str = "/work/.officina/mounts";
/// Where the floor's login is, read-only (D-606).
pub const LOGIN_DIR: &str = "/work/.officina/login";
/// Where the memory folder is, read-only (AD-115, Slice B).
pub const MEMORY_DIR: &str = "/work/.officina/memory";
/// The worker's home on its volume (AD-102).
pub const HOME_DIR: &str = "/work/.officina/home";
/// The worker's Claude config dir on its volume (AD-102).
pub const CONFIG_DIR: &str = "/work/.officina/config";
/// The gateway's port (AD-103).
pub const GATEWAY_PORT: u16 = 3128;
/// The hosts a worker reaches without its proxy (AD-102).
pub const NO_PROXY: &str = "localhost,127.0.0.1,::1";

/// A worker's processes (AD-102).
pub const WORKER_PIDS: u32 = 1024;
/// A worker's `/tmp`, in MiB (AD-102).
pub const WORKER_TMP_MIB: u32 = 512;
/// The processes beyond two threads per tunnel a gateway needs: its init,
/// its listener and the main thread, with room (D-234, review F7).
pub const GATEWAY_PIDS_HEADROOM: u32 = 8;

/// A gateway's processes: the pids controller counts threads, and each
/// tunnel takes two (D-234).
pub const fn gateway_pids() -> u32 {
    // A const fn cannot use `try_from`; MAX_TUNNELS is a small constant.
    let tunnels = MAX_TUNNELS as u32;
    2 * tunnels + GATEWAY_PIDS_HEADROOM
}

/// A container's CPUs, memory and processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// CPUs in tenths, so 0.5 is 5.
    pub cpu_tenths: u32,
    pub memory_mib: u32,
    pub pids: u32,
}

impl Limits {
    /// The fixed limits of a role whose limits do not come from the floor
    /// (AD-102, D-248, D-608).
    pub const fn fixed(role: ContainerRole) -> Option<Limits> {
        match role {
            ContainerRole::Gateway | ContainerRole::BatteryGateway => Some(Limits {
                cpu_tenths: 5,
                memory_mib: 128,
                pids: gateway_pids(),
            }),
            ContainerRole::Packer => Some(Limits {
                cpu_tenths: 10,
                memory_mib: 1024,
                pids: 256,
            }),
            ContainerRole::Inventory => Some(Limits {
                cpu_tenths: 10,
                memory_mib: 512,
                pids: 256,
            }),
            ContainerRole::AuthProbe | ContainerRole::Scanner => Some(Limits {
                cpu_tenths: 10,
                memory_mib: 512,
                pids: 64,
            }),
            ContainerRole::Decoy => Some(Limits {
                cpu_tenths: 5,
                memory_mib: 128,
                pids: 64,
            }),
            ContainerRole::Worker | ContainerRole::Battery => None,
        }
    }

    /// A worker's limits from `[workshop]` (AD-102).
    pub const fn worker(cpus: u32, memory_mib: u32) -> Limits {
        Limits {
            cpu_tenths: cpus * 10,
            memory_mib,
            pids: WORKER_PIDS,
        }
    }

    fn cpus(self) -> String {
        if self.cpu_tenths.is_multiple_of(10) {
            (self.cpu_tenths / 10).to_string()
        } else {
            format!("{}.{}", self.cpu_tenths / 10, self.cpu_tenths % 10)
        }
    }
}

/// Why a mount path cannot be a mount source (AD-102, AD-107).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPathUnsupported(pub String);

/// The source of a mount: the project folder joined with the mount path,
/// forward slashes, refused when it holds `,`, `"` or a control character,
/// since `--mount` reads its value as comma-separated fields and a comma
/// could add one (AD-102). Existence, ignore status and reparse points are
/// the host's checks (story 6.11).
pub fn mount_source(project_folder: &str, mount: &str) -> Result<String, MountPathUnsupported> {
    let folder = project_folder.replace('\\', "/");
    let joined = format!(
        "{}/{}",
        folder.trim_end_matches('/'),
        mount.trim_matches('/')
    );
    if joined
        .chars()
        .any(|c| c == ',' || c == '"' || c.is_control())
    {
        return Err(MountPathUnsupported(joined));
    }
    Ok(joined)
}

/// The name a credential kind passes to the worker (AD-105, D-603): one
/// name for a token kind, none for the login, which is a mounted file.
pub const fn credential_name(kind: CredentialKind) -> Option<&'static str> {
    match kind {
        CredentialKind::OauthToken => Some("CLAUDE_CODE_OAUTH_TOKEN"),
        CredentialKind::ApiKey => Some("ANTHROPIC_API_KEY"),
        CredentialKind::Login => None,
    }
}

/// The commit identity a worker's commits carry, from the project's
/// configuration (FR-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

/// The arguments of one `docker create`: its name, the options, the image
/// (by id, D-207) and the command. Its fields are private; the builders of
/// this module are the only constructors.
///
/// ```compile_fail,E0451
/// use officina_core::workshop::vector::{ContainerRole, ContainerVector};
/// let vector = ContainerVector {
///     role: ContainerRole::Worker,
///     name: todo!(),
///     options: vec!["--privileged".to_owned()],
///     image: "debian".to_owned(),
///     command: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerVector {
    role: ContainerRole,
    name: ObjectName,
    options: Vec<String>,
    image: String,
    command: Vec<String>,
}

impl ContainerVector {
    pub fn role(&self) -> ContainerRole {
        self.role
    }

    pub fn name(&self) -> &ObjectName {
        &self.name
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn image(&self) -> &str {
        &self.image
    }

    pub fn command(&self) -> &[String] {
        &self.command
    }

    /// The names of `--env` options, in order.
    pub fn env_names(&self) -> Vec<String> {
        self.options
            .windows(2)
            .filter(|pair| pair[0] == "--env")
            .map(|pair| {
                pair[1]
                    .split_once('=')
                    .map_or(pair[1].as_str(), |(n, _)| n)
                    .to_owned()
            })
            .collect()
    }

    /// The vector with one option replaced, for the comparison's control
    /// (story 6.6 AC 7).
    #[cfg(test)]
    pub(crate) fn with_option_replaced(&self, from: &str, to: &str) -> ContainerVector {
        let mut changed = self.clone();
        for option in &mut changed.options {
            if option == from {
                to.clone_into(option);
            }
        }
        changed
    }
}

/// What every container shares (AD-102): the security options, AD-98's log
/// options, the labels, the user, the entry point and the limits.
pub(crate) fn fixed_options(
    user: &str,
    entrypoint: &str,
    limits: Limits,
    labels: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut options: Vec<String> = [
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--init",
        "--stop-timeout",
        "3",
        "--log-driver",
        "local",
        "--log-opt",
        "max-size=32m",
        "--log-opt",
        "max-file=3",
        "--user",
        user,
        "--entrypoint",
        entrypoint,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let memory = format!("{}m", limits.memory_mib);
    options.extend([
        "--cpus".to_owned(),
        limits.cpus(),
        "--memory".to_owned(),
        memory.clone(),
        "--memory-swap".to_owned(),
        memory,
        "--pids-limit".to_owned(),
        limits.pids.to_string(),
    ]);
    for (key, value) in labels {
        options.push("--label".to_owned());
        options.push(format!("{key}={value}"));
    }
    options
}

fn push(options: &mut Vec<String>, flag: &str, value: impl Into<String>) {
    options.push(flag.to_owned());
    options.push(value.into());
}

fn bind_ro(source: &str, target: &str) -> String {
    format!("type=bind,source={source},target={target},readonly")
}

fn volume(source: &ObjectName, target: &str, readonly: bool) -> String {
    let ro = if readonly { ",readonly" } else { "" };
    format!("type=volume,source={},target={target}{ro}", source.as_str())
}

fn tmpfs(target: &str, mib: u32) -> String {
    format!("{target}:rw,nosuid,nodev,size={mib}m")
}

/// Where the credential of a worker-shaped container comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    /// A token kind: its one name, its value set by the host in the
    /// environment of the `create` child only (AD-105).
    Name(&'static str),
    /// The login kind: a volume mounted read-only where the login is
    /// (D-606); the battery mounts its own empty one (story 6.10).
    Volume(ObjectName),
}

impl CredentialSource {
    /// The source a floor of `kind` gives its workers.
    pub fn of(kind: CredentialKind, floor_key: &str) -> CredentialSource {
        match credential_name(kind) {
            Some(name) => CredentialSource::Name(name),
            None => CredentialSource::Volume(login_volume(floor_key)),
        }
    }
}

/// What a worker-shaped container is made of (AD-102): the `Worker` of a
/// run and the battery's `Battery` share it, so the two differ only where
/// they must (FR-15, D-210).
#[derive(Debug, Clone)]
pub struct WorkerShape<'a> {
    pub name: &'a ObjectName,
    pub labels: &'a BTreeMap<String, String>,
    pub image_id: &'a str,
    pub network: &'a ObjectName,
    pub volume: &'a ObjectName,
    pub gateway: &'a ObjectName,
    pub limits: Limits,
    /// Each mount's source, in the floor's order.
    pub mounts: &'a [String],
    pub credential: &'a CredentialSource,
    pub git: &'a GitIdentity,
    /// The memory folder's source and its name (Slice B, AD-115).
    pub memory: Option<(&'a str, &'a str)>,
}

/// The environment of a worker-shaped container (AD-102, D-206), in
/// order: the vector's names win over the image's `ENV`.
fn worker_env(shape: &WorkerShape) -> Vec<(String, Option<String>)> {
    let proxy = format!("http://{}:{GATEWAY_PORT}", shape.gateway.as_str());
    let mut env: Vec<(String, Option<String>)> = vec![
        ("HOME".into(), Some(HOME_DIR.into())),
        ("CLAUDE_CONFIG_DIR".into(), Some(CONFIG_DIR.into())),
        ("HTTPS_PROXY".into(), Some(proxy.clone())),
        ("https_proxy".into(), Some(proxy.clone())),
        ("HTTP_PROXY".into(), Some(proxy.clone())),
        ("http_proxy".into(), Some(proxy)),
        ("NO_PROXY".into(), Some(NO_PROXY.into())),
        ("no_proxy".into(), Some(NO_PROXY.into())),
        (
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
            Some("1".into()),
        ),
        ("DISABLE_AUTOUPDATER".into(), Some("1".into())),
        ("CLAUDE_CODE_DISABLE_ARTIFACT".into(), Some("1".into())),
        ("CLAUDE_CODE_DISABLE_AGENT_VIEW".into(), Some("1".into())),
        ("GIT_AUTHOR_NAME".into(), Some(shape.git.name.clone())),
        ("GIT_AUTHOR_EMAIL".into(), Some(shape.git.email.clone())),
        ("GIT_COMMITTER_NAME".into(), Some(shape.git.name.clone())),
        ("GIT_COMMITTER_EMAIL".into(), Some(shape.git.email.clone())),
    ];
    if let Some((_, name)) = shape.memory {
        env.push(("CLAUDE_CODE_PROJECT_DIR_NAME".into(), Some(name.to_owned())));
    }
    if let CredentialSource::Name(name) = shape.credential {
        env.push(((*name).to_owned(), None));
    }
    env
}

fn push_env(options: &mut Vec<String>, env: &[(String, Option<String>)]) {
    for (name, value) in env {
        match value {
            Some(value) => push(options, "--env", format!("{name}={value}")),
            None => push(options, "--env", name.clone()),
        }
    }
}

fn worker_shaped(
    role: ContainerRole,
    shape: &WorkerShape,
    command: Vec<String>,
) -> ContainerVector {
    let mut options = fixed_options(WORKER_USER, ENTRY_PROGRAM, shape.limits, shape.labels);
    push(&mut options, "--network", shape.network.as_str());
    push(&mut options, "--tmpfs", tmpfs("/tmp", WORKER_TMP_MIB));
    push(
        &mut options,
        "--mount",
        volume(shape.volume, WORK_DIR, false),
    );
    for (i, source) in shape.mounts.iter().enumerate() {
        push(
            &mut options,
            "--mount",
            bind_ro(source, &format!("{MOUNTS_DIR}/{i}")),
        );
    }
    if let Some((source, _)) = shape.memory {
        push(&mut options, "--mount", bind_ro(source, MEMORY_DIR));
    }
    if let CredentialSource::Volume(login) = shape.credential {
        push(&mut options, "--mount", volume(login, LOGIN_DIR, true));
    }
    push_env(&mut options, &worker_env(shape));
    ContainerVector {
        role,
        name: shape.name.clone(),
        options,
        image: shape.image_id.to_owned(),
        command,
    }
}

/// A run's worker (AD-102): `officina-entry run` on the run's internal
/// network. Story 6.11's launch builds it through
/// [`super::launch::worker`], from a validated launch only.
pub(crate) fn worker(shape: &WorkerShape) -> ContainerVector {
    worker_shaped(ContainerRole::Worker, shape, vec!["run".to_owned()])
}

/// The battery's worker (AD-117): the worker's shape with the battery's
/// command, mount and credential source.
pub fn battery(shape: &WorkerShape, phase: u8, plan: &[String]) -> ContainerVector {
    let mut command = vec![
        "battery".to_owned(),
        "--phase".to_owned(),
        phase.to_string(),
    ];
    command.extend(plan.iter().cloned());
    worker_shaped(ContainerRole::Battery, shape, command)
}

/// A gateway (AD-102, AD-103): user 65534, no volume, no environment,
/// created on the egress network.
fn gateway_shaped(
    role: ContainerRole,
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    image_id: &str,
    egress: &ObjectName,
    config: &GatewayConfig,
    extra_hosts: &[(&str, &str)],
) -> ContainerVector {
    let limits = Limits::fixed(role).unwrap_or(Limits::worker(1, 128));
    let mut options = fixed_options(GATEWAY_USER, GATEWAY_PROGRAM, limits, labels);
    push(&mut options, "--network", egress.as_str());
    for (host, address) in extra_hosts {
        push(&mut options, "--add-host", format!("{host}:{address}"));
    }
    ContainerVector {
        role,
        name: name.clone(),
        options,
        image: image_id.to_owned(),
        command: config.to_args(),
    }
}

/// A run's gateway (AD-103, AD-104): created on `officina-out-<id>`, then
/// connected to the internal network (a separate `network connect`).
pub fn gateway(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    image_id: &str,
    egress: &ObjectName,
    config: &GatewayConfig,
) -> ContainerVector {
    gateway_shaped(
        ContainerRole::Gateway,
        name,
        labels,
        image_id,
        egress,
        config,
        &[],
    )
}

/// The battery's test names its gateway resolves itself (AD-117): one to
/// loopback, one to the engine host, both allow-listed and expected
/// `private_address`.
pub const BATTERY_TEST_HOSTS: &[(&str, &str)] = &[
    ("officina-loopback.test", "127.0.0.1"),
    ("officina-host.test", "host-gateway"),
];

/// A battery gateway (AD-117), the gateway's vector with the test names.
pub fn battery_gateway(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    image_id: &str,
    egress: &ObjectName,
    config: &GatewayConfig,
) -> ContainerVector {
    gateway_shaped(
        ContainerRole::BatteryGateway,
        name,
        labels,
        image_id,
        egress,
        config,
        BATTERY_TEST_HOSTS,
    )
}

/// The packer (AD-110, D-248): the run's image and volume, no network, no
/// credential, no mount; its branch and base from its command, never from
/// the volume's worker-writable `plan.json`.
pub fn packer(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    image_id: &str,
    run_volume: &ObjectName,
    branch: &str,
    base_commit: &str,
    clone_path: &str,
) -> ContainerVector {
    let limits = Limits::fixed(ContainerRole::Packer).unwrap_or(Limits::worker(1, 1024));
    let mut options = fixed_options(WORKER_USER, ENTRY_PROGRAM, limits, labels);
    push(&mut options, "--network", "none");
    push(&mut options, "--mount", volume(run_volume, WORK_DIR, false));
    push_env(
        &mut options,
        &[
            ("HOME".into(), Some(HOME_DIR.into())),
            ("GIT_CONFIG_NOSYSTEM".into(), Some("1".into())),
            ("GIT_CONFIG_GLOBAL".into(), Some("/dev/null".into())),
        ],
    );
    ContainerVector {
        role: ContainerRole::Packer,
        name: name.clone(),
        options,
        image: image_id.to_owned(),
        command: vec![
            "pack".to_owned(),
            "--branch".to_owned(),
            branch.to_owned(),
            "--base".to_owned(),
            base_commit.to_owned(),
            "--clone".to_owned(),
            clone_path.to_owned(),
        ],
    }
}

/// The inventory (AD-118, D-211): no network, the project's mounts
/// read-only, a 16 MiB tmpfs at `/work/<project>` where the entry point
/// links them, the list on stdin.
pub fn inventory(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    image_id: &str,
    project: &str,
    mounts: &[String],
) -> ContainerVector {
    let limits = Limits::fixed(ContainerRole::Inventory).unwrap_or(Limits::worker(1, 512));
    let mut options = fixed_options(WORKER_USER, ENTRY_PROGRAM, limits, labels);
    push(&mut options, "--network", "none");
    options.push("--interactive".to_owned());
    let project_dir = format!("{WORK_DIR}/{project}");
    push(&mut options, "--tmpfs", tmpfs(&project_dir, 16));
    for (i, source) in mounts.iter().enumerate() {
        push(
            &mut options,
            "--mount",
            bind_ro(source, &format!("{MOUNTS_DIR}/{i}")),
        );
    }
    push_env(
        &mut options,
        &[("CLAUDE_PROJECT_DIR".into(), Some(project_dir))],
    );
    ContainerVector {
        role: ContainerRole::Inventory,
        name: name.clone(),
        options,
        image: image_id.to_owned(),
        command: vec!["inventory".to_owned()],
    }
}

/// The battery's decoy (AD-117): the run image on its own plain bridge,
/// resolving Docker Desktop's names and listening.
pub fn decoy(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    image_id: &str,
    network: &ObjectName,
) -> ContainerVector {
    let limits = Limits::fixed(ContainerRole::Decoy).unwrap_or(Limits::worker(1, 128));
    let mut options = fixed_options(WORKER_USER, ENTRY_PROGRAM, limits, labels);
    push(&mut options, "--network", network.as_str());
    ContainerVector {
        role: ContainerRole::Decoy,
        name: name.clone(),
        options,
        image: image_id.to_owned(),
        command: vec!["decoy".to_owned()],
    }
}

/// A container reading the floor's login (D-608, D-609): the base image,
/// no network, the login read-only, a tmpfs `/tmp`.
fn login_reader(
    role: ContainerRole,
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    base_image_id: &str,
    floor_key: &str,
    command: &str,
) -> ContainerVector {
    let limits = Limits::fixed(role).unwrap_or(Limits::worker(1, 512));
    let mut options = fixed_options(WORKER_USER, ENTRY_PROGRAM, limits, labels);
    push(&mut options, "--network", "none");
    if role == ContainerRole::Scanner {
        options.push("--interactive".to_owned());
    }
    push(&mut options, "--tmpfs", tmpfs("/tmp", 64));
    push(
        &mut options,
        "--mount",
        volume(&login_volume(floor_key), LOGIN_DIR, true),
    );
    ContainerVector {
        role,
        name: name.clone(),
        options,
        image: base_image_id.to_owned(),
        command: vec![command.to_owned()],
    }
}

/// The auth probe (D-608): `officina-entry auth-status` on the login.
pub fn auth_probe(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    base_image_id: &str,
    floor_key: &str,
) -> ContainerVector {
    login_reader(
        ContainerRole::AuthProbe,
        name,
        labels,
        base_image_id,
        floor_key,
        "auth-status",
    )
}

/// The scanner of a login run (D-609): `officina-entry scan`, records on
/// stdin.
pub fn scanner(
    name: &ObjectName,
    labels: &BTreeMap<String, String>,
    base_image_id: &str,
    floor_key: &str,
) -> ContainerVector {
    login_reader(
        ContainerRole::Scanner,
        name,
        labels,
        base_image_id,
        floor_key,
        "scan",
    )
}

/// The names the builders set in a worker-shaped container, derived from
/// them, for story 6.8's image check: an image whose `ENV` meets one is
/// refused `not_from_base` (D-233, review F13).
pub fn reserved_env_names() -> Vec<String> {
    let Some(name) = ObjectName::parse("officina-reserved-names") else {
        return Vec::new();
    };
    let git = GitIdentity {
        name: String::new(),
        email: String::new(),
    };
    let labels = BTreeMap::new();
    let mut names: Vec<String> = Vec::new();
    for kind in CredentialKind::ALL {
        let credential = CredentialSource::of(*kind, "0000000000000000");
        let shape = WorkerShape {
            name: &name,
            labels: &labels,
            image_id: "",
            network: &name,
            volume: &name,
            gateway: &name,
            limits: Limits::worker(1, 1),
            mounts: &[],
            credential: &credential,
            git: &git,
            memory: Some(("/m", "m")),
        };
        for (env_name, _) in worker_env(&shape) {
            if !names.contains(&env_name) {
                names.push(env_name);
            }
        }
    }
    names
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::workshop::names::{RunId, RunNames, run_labels};

    pub(crate) const RUN: &str = "0123456789abcdef0123456789abcdef";
    pub(crate) const FLOOR_KEY: &str = "0123456789abcdef";
    pub(crate) const IMAGE_ID: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    pub(crate) const GATEWAY_ID: &str =
        "sha256:2222222222222222222222222222222222222222222222222222222222222222";

    fn names() -> RunNames {
        RunNames::of(&RunId::parse(RUN).unwrap())
    }

    fn labels(role: ContainerRole) -> BTreeMap<String, String> {
        run_labels(
            FLOOR_KEY,
            "0.1.0",
            &RunId::parse(RUN).unwrap(),
            role,
            "bump-1",
        )
    }

    fn git() -> GitIdentity {
        GitIdentity {
            name: "User One".into(),
            email: "user01@example.com".into(),
        }
    }

    /// The worker of a floor of `kind` with `mounts` and `memory`.
    pub(crate) fn sample_worker(
        kind: CredentialKind,
        mounts: &[String],
        memory: Option<(&str, &str)>,
    ) -> ContainerVector {
        let names = names();
        let labels = labels(ContainerRole::Worker);
        let credential = CredentialSource::of(kind, FLOOR_KEY);
        let git = git();
        worker(&WorkerShape {
            name: &names.worker,
            labels: &labels,
            image_id: IMAGE_ID,
            network: &names.internal,
            volume: &names.volume,
            gateway: &names.gateway,
            limits: Limits::worker(2, 4096),
            mounts,
            credential: &credential,
            git: &git,
            memory,
        })
    }

    /// One vector of each role, every role of [`ContainerRole::ALL`].
    pub(crate) fn every_role(kind: CredentialKind) -> Vec<ContainerVector> {
        let names = names();
        let config = GatewayConfig::new(RUN, &[], 3600, BTreeMap::new());
        let mounts = ["C:/path/to/api/.claude".to_owned()];
        let credential = CredentialSource::of(kind, FLOOR_KEY);
        let git = git();
        let battery_labels = labels(ContainerRole::Battery);
        ContainerRole::ALL
            .iter()
            .map(|role| match role {
                ContainerRole::Worker => sample_worker(kind, &mounts, None),
                ContainerRole::Gateway => gateway(
                    &names.gateway,
                    &labels(*role),
                    GATEWAY_ID,
                    &names.egress,
                    &config,
                ),
                ContainerRole::Packer => packer(
                    &names.packer,
                    &labels(*role),
                    IMAGE_ID,
                    &names.volume,
                    "officina/bump-1",
                    "abc",
                    "/work/api",
                ),
                ContainerRole::Inventory => {
                    inventory(&names.worker, &labels(*role), IMAGE_ID, "api", &mounts)
                }
                ContainerRole::Battery => battery(
                    &WorkerShape {
                        name: &names.worker,
                        labels: &battery_labels,
                        image_id: IMAGE_ID,
                        network: &names.internal,
                        volume: &names.volume,
                        gateway: &names.gateway,
                        limits: Limits::worker(2, 4096),
                        mounts: &mounts,
                        credential: &credential,
                        git: &git,
                        memory: None,
                    },
                    1,
                    &[],
                ),
                ContainerRole::BatteryGateway => battery_gateway(
                    &names.gateway,
                    &labels(*role),
                    GATEWAY_ID,
                    &names.egress,
                    &config,
                ),
                ContainerRole::Decoy => {
                    decoy(&names.worker, &labels(*role), IMAGE_ID, &names.egress)
                }
                ContainerRole::AuthProbe => {
                    auth_probe(&names.worker, &labels(*role), IMAGE_ID, FLOOR_KEY)
                }
                ContainerRole::Scanner => {
                    scanner(&names.worker, &labels(*role), IMAGE_ID, FLOOR_KEY)
                }
            })
            .collect()
    }

    fn has_pair(vector: &ContainerVector, flag: &str, value: &str) -> bool {
        vector
            .options()
            .windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value)
    }

    fn value_of<'v>(vector: &'v ContainerVector, flag: &str) -> Option<&'v str> {
        vector
            .options()
            .windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
    }

    #[test]
    fn every_role_carries_the_fixed_options() {
        for vector in every_role(CredentialKind::OauthToken) {
            let role = vector.role();
            for flag in ["--read-only", "--init"] {
                assert!(
                    vector.options().iter().any(|o| o == flag),
                    "{role:?} {flag}"
                );
            }
            assert!(has_pair(&vector, "--cap-drop", "ALL"), "{role:?}");
            assert!(
                has_pair(&vector, "--security-opt", "no-new-privileges:true"),
                "{role:?}"
            );
            assert!(has_pair(&vector, "--stop-timeout", "3"), "{role:?}");
            assert!(has_pair(&vector, "--log-driver", "local"), "{role:?}");
            assert_eq!(
                value_of(&vector, "--memory"),
                value_of(&vector, "--memory-swap"),
                "{role:?}"
            );
            assert!(value_of(&vector, "--pids-limit").is_some(), "{role:?}");
            assert!(
                has_pair(&vector, "--label", "officina.managed=1"),
                "{role:?}"
            );
            assert!(
                has_pair(
                    &vector,
                    "--label",
                    &format!("officina.role={}", role.code())
                ),
                "{role:?}"
            );
        }
        let inventory = every_role(CredentialKind::OauthToken)
            .into_iter()
            .find(|v| v.role() == ContainerRole::Inventory)
            .unwrap();
        assert_eq!(value_of(&inventory, "--pids-limit"), Some("256"));
    }

    #[test]
    fn no_role_carries_a_forbidden_option() {
        for kind in CredentialKind::ALL {
            for vector in every_role(*kind) {
                for option in vector.options() {
                    assert!(
                        ![
                            "--privileged",
                            "--device",
                            "--pid",
                            "--ipc",
                            "--userns",
                            "--cap-add",
                            "-v",
                            "--volume"
                        ]
                        .contains(&option.as_str()),
                        "{:?}: {option}",
                        vector.role()
                    );
                    assert!(!option.contains("docker.sock") && !option.contains("docker_engine"));
                }
                assert!(!has_pair(&vector, "--network", "host"));
                let security: Vec<&str> = vector
                    .options()
                    .windows(2)
                    .filter(|p| p[0] == "--security-opt")
                    .map(|p| p[1].as_str())
                    .collect();
                assert_eq!(security, ["no-new-privileges:true"], "{:?}", vector.role());
            }
        }
    }

    #[test]
    fn entrypoint_user_and_command_are_always_overridden() {
        for vector in every_role(CredentialKind::OauthToken) {
            assert!(
                value_of(&vector, "--entrypoint").is_some(),
                "{:?}",
                vector.role()
            );
            assert!(value_of(&vector, "--user").is_some(), "{:?}", vector.role());
            assert!(!vector.command().is_empty(), "{:?}", vector.role());
        }
    }

    #[test]
    fn images_are_named_by_id() {
        for vector in every_role(CredentialKind::OauthToken) {
            assert!(vector.image().starts_with("sha256:"), "{:?}", vector.role());
        }
    }

    const PROXY_NAMES: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];

    fn credential_names() -> Vec<&'static str> {
        CredentialKind::ALL
            .iter()
            .filter_map(|k| credential_name(*k))
            .collect()
    }

    #[test]
    fn only_worker_and_battery_carry_proxy_or_credential_names() {
        for kind in CredentialKind::ALL {
            for vector in every_role(*kind) {
                let names = vector.env_names();
                let carries = names.iter().any(|n| {
                    PROXY_NAMES.contains(&n.as_str()) || credential_names().contains(&n.as_str())
                });
                let may = matches!(
                    vector.role(),
                    ContainerRole::Worker | ContainerRole::Battery
                );
                assert!(!carries || may, "{:?}: {names:?}", vector.role());
            }
        }
    }

    #[test]
    fn exactly_one_credential_source_per_auth() {
        for kind in CredentialKind::ALL {
            let worker = sample_worker(*kind, &[], None);
            let names = worker.env_names();
            let carried: Vec<&str> = credential_names()
                .into_iter()
                .filter(|c| names.iter().any(|n| n == c))
                .collect();
            let login = worker.options().iter().any(|o| o.contains(LOGIN_DIR));
            match credential_name(*kind) {
                Some(name) => assert!(carried == [name] && !login, "{kind:?}"),
                None => assert!(carried.is_empty() && login, "{kind:?}"),
            }
        }
    }

    #[test]
    fn a_login_worker_mounts_the_login_read_only_and_carries_no_credential_name() {
        let worker = sample_worker(CredentialKind::Login, &[], None);
        let mount = format!(
            "type=volume,source={},target={LOGIN_DIR},readonly",
            login_volume(FLOOR_KEY).as_str()
        );
        assert!(has_pair(&worker, "--mount", &mount));
        assert!(
            worker
                .env_names()
                .iter()
                .all(|n| !credential_names().contains(&n.as_str()))
        );
    }

    #[test]
    fn the_credential_value_never_appears_in_a_vector() {
        for kind in CredentialKind::ALL {
            for vector in every_role(*kind) {
                for option in vector.options() {
                    for name in credential_names() {
                        assert!(
                            option == name || !option.starts_with(&format!("{name}=")),
                            "{option}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn reserved_names_are_derived_from_the_builders() {
        let reserved = reserved_env_names();
        for kind in CredentialKind::ALL {
            let worker = sample_worker(*kind, &[], Some(("C:/m", "memory")));
            for name in worker.env_names() {
                assert!(reserved.contains(&name), "{name}");
            }
        }
        for name in credential_names() {
            assert!(reserved.iter().any(|r| r == name));
        }
    }

    #[test]
    fn the_gateway_pids_limit_covers_its_threads() {
        let tunnels = u32::try_from(MAX_TUNNELS).unwrap();
        assert!(gateway_pids() > 2 * tunnels);
        for vector in every_role(CredentialKind::OauthToken) {
            if matches!(
                vector.role(),
                ContainerRole::Gateway | ContainerRole::BatteryGateway
            ) {
                assert_eq!(
                    value_of(&vector, "--pids-limit"),
                    Some(gateway_pids().to_string().as_str())
                );
            }
        }
    }

    #[test]
    fn the_packer_takes_branch_and_base_from_its_command() {
        let names = names();
        let packer = packer(
            &names.packer,
            &labels(ContainerRole::Packer),
            IMAGE_ID,
            &names.volume,
            "officina/bump-7",
            "0a0b",
            "/work/api",
        );
        assert_eq!(
            packer.command(),
            [
                "pack",
                "--branch",
                "officina/bump-7",
                "--base",
                "0a0b",
                "--clone",
                "/work/api"
            ]
        );
        assert!(has_pair(&packer, "--network", "none"));
        assert!(!packer.options().iter().any(|o| o.contains("plan.json")));
    }

    #[test]
    fn only_worker_battery_probe_and_scanner_mount_the_login() {
        for kind in CredentialKind::ALL {
            for vector in every_role(*kind) {
                let mounts_login = vector.options().iter().any(|o| o.contains(LOGIN_DIR));
                let may = matches!(
                    vector.role(),
                    ContainerRole::Worker
                        | ContainerRole::Battery
                        | ContainerRole::AuthProbe
                        | ContainerRole::Scanner
                );
                assert!(!mounts_login || may, "{:?}", vector.role());
            }
        }
    }

    #[test]
    fn the_login_is_never_mounted_writable() {
        for kind in CredentialKind::ALL {
            for vector in every_role(*kind) {
                for option in vector.options().iter().filter(|o| o.contains(LOGIN_DIR)) {
                    assert!(
                        option.ends_with(",readonly"),
                        "{:?}: {option}",
                        vector.role()
                    );
                }
            }
        }
    }

    #[test]
    fn a_comma_in_a_mount_path_is_refused() {
        for bad in ["a,b", "a\"b", "a\nb", "a\u{7}b"] {
            assert!(mount_source("C:/path/to/api", bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn spaces_and_accents_are_kept() {
        assert_eq!(
            mount_source(r"C:\path\to\Café projet", "_notes de réunion").unwrap(),
            "C:/path/to/Café projet/_notes de réunion"
        );
    }

    #[test]
    fn every_project_shape_differs_from_the_battery_only_in_mount_count_and_memory() {
        let one = ["C:/p/m0".to_owned()];
        let eight: Vec<String> = (0..8).map(|i| format!("C:/p/m{i}")).collect();
        let base = sample_worker(CredentialKind::OauthToken, &one, None);
        let wide = sample_worker(CredentialKind::OauthToken, &eight, Some(("C:/mem", "mem")));
        let extra: Vec<&String> = wide
            .options()
            .iter()
            .filter(|o| !base.options().contains(o))
            .collect();
        for option in extra {
            assert!(
                option.starts_with("type=bind,source=C:/p/m")
                    || option.starts_with(&format!("type=bind,source=C:/mem,target={MEMORY_DIR}"))
                    || option.starts_with("CLAUDE_CODE_PROJECT_DIR_NAME="),
                "{option}"
            );
        }
        for option in wide.options().iter().filter(|o| o.starts_with("type=bind")) {
            assert!(option.ends_with(",readonly"), "{option}");
        }
    }

    // ---- the `vectors` section of tests/vectors/workshop.json (AC 2 to 4, 7) ----

    fn vectors_file() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../tests/vectors/workshop.json"))
            .expect("workshop.json")
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "a test's write mode for a reviewed change of the golden file"
    )]
    fn writing() -> bool {
        std::env::var("OFFICINA_WRITE_VECTORS").is_ok_and(|v| v == "1")
    }

    /// What the `vectors` section holds: each role's create arguments, the
    /// worker's plan, and a run's ordered network calls.
    fn built_vectors() -> serde_json::Value {
        use crate::workshop::engine::EngineCall;
        let roles: serde_json::Map<String, serde_json::Value> =
            every_role(CredentialKind::OauthToken)
                .into_iter()
                .map(|v| {
                    (
                        v.role().code().to_owned(),
                        serde_json::json!(EngineCall::create(&v).args()),
                    )
                })
                .collect();
        let login = sample_worker(
            CredentialKind::Login,
            &["C:/path/to/api/.claude".to_owned()],
            None,
        );
        let names = names();
        let labels = crate::workshop::names::base_labels(FLOOR_KEY, "0.1.0");
        let networks = [
            EngineCall::network_create(&names.egress, false, &labels),
            EngineCall::network_create(&names.internal, true, &labels),
            EngineCall::network_connect(&names.internal, &names.gateway),
        ];
        let launch = crate::workshop::launch::tests::sample_launch();
        let routed = BTreeMap::from([(
            crate::workshop::launch::LOGIN_ROUTE_HOST.to_owned(),
            "proxy.example.org:3128".to_owned(),
        )]);
        serde_json::json!({
            "roles": roles,
            "login_worker": EngineCall::create(&login).args(),
            "networks": networks.iter().map(|c| c.args().to_vec()).collect::<Vec<_>>(),
            "worker_plan": crate::workshop::launch::run_plan(&launch),
            "launch_worker": EngineCall::create(&crate::workshop::launch::worker(&launch)).args(),
            "plan_sha256": crate::workshop::launch::plan_sha256(&launch),
            "login_command": {
                "direct": crate::workshop::launch::login_command(FLOOR_KEY, "officina-workshop:local", &BTreeMap::new()),
                "routed": crate::workshop::launch::login_command(FLOOR_KEY, "officina-workshop:local", &routed),
            },
        })
    }

    #[test]
    fn every_role_has_a_case() {
        let file = vectors_file();
        for role in ContainerRole::ALL {
            assert!(
                file["vectors"]["roles"].get(role.code()).is_some(),
                "{role:?} has no case"
            );
        }
    }

    #[test]
    fn vector_cases_match_their_builders() {
        let built = built_vectors();
        if writing() {
            write_vectors(built);
            return;
        }
        assert_eq!(vectors_file()["vectors"], built);
    }

    #[test]
    fn a_run_creates_its_networks_in_order() {
        let calls = &vectors_file()["vectors"]["networks"];
        let names = names();
        assert_eq!(
            calls[0].as_array().unwrap().last().unwrap(),
            names.egress.as_str()
        );
        assert!(
            !calls[0]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "--internal")
        );
        assert!(
            calls[1]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "--internal")
        );
        assert_eq!(calls[2][0], "network");
        assert_eq!(calls[2][1], "connect");
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "a test's write mode for a reviewed change of the golden file"
    )]
    fn write_vectors(value: serde_json::Value) {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/workshop.json"
        );
        let mut file: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        file["vectors"] = value;
        std::fs::write(path, serde_json::to_string_pretty(&file).unwrap() + "\n").unwrap();
    }

    /// FR-15, D-210: after normalising what must differ, the worker and the
    /// battery of the same image and the battery's input are equal.
    pub(crate) fn normalised(vector: &ContainerVector) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut skip_next = false;
        for (i, option) in vector.options().iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }
            let next = vector.options().get(i + 1);
            match (option.as_str(), next) {
                ("--label", Some(label))
                    if ["officina.role=", "officina.run=", "officina.work_order="]
                        .iter()
                        .any(|p| label.starts_with(p)) =>
                {
                    out.push(format!("--label {}", label.split('=').next().unwrap_or("")));
                    skip_next = true;
                }
                ("--network", Some(_)) => {
                    out.push("--network <net>".into());
                    skip_next = true;
                }
                ("--mount", Some(mount)) => {
                    let without_source: Vec<&str> = mount
                        .split(',')
                        .filter(|f| !f.starts_with("source="))
                        .collect();
                    out.push(format!("--mount {}", without_source.join(",")));
                    skip_next = true;
                }
                ("--env", Some(env))
                    if env.starts_with("HTTPS_PROXY=")
                        || env.starts_with("https_proxy=")
                        || env.starts_with("HTTP_PROXY=")
                        || env.starts_with("http_proxy=") =>
                {
                    out.push(format!("--env {}", env.split('=').next().unwrap_or("")));
                    skip_next = true;
                }
                _ => out.push(option.clone()),
            }
        }
        out.push(vector.image().to_owned());
        out
    }

    fn battery_like_worker() -> (ContainerVector, ContainerVector) {
        let roles = every_role(CredentialKind::OauthToken);
        let pick = |role| roles.iter().find(|v| v.role() == role).unwrap().clone();
        (pick(ContainerRole::Worker), pick(ContainerRole::Battery))
    }

    #[test]
    fn worker_and_battery_differ_only_where_they_must() {
        let (worker, battery) = battery_like_worker();
        assert_eq!(normalised(&worker), normalised(&battery));
        assert_ne!(worker.command(), battery.command());
    }

    #[test]
    fn the_comparison_sees_one_changed_option() {
        let (worker, battery) = battery_like_worker();
        let changed = battery.with_option_replaced("--read-only", "--read-only=false");
        assert_ne!(normalised(&worker), normalised(&changed));
    }
}
