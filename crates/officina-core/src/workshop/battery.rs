//! The escape battery (AD-117, FR-15): the attempts a fixed program plays
//! in a container built by the worker's own vector, the lines it prints,
//! the judgement of a run and the validity of a passing one. The outcome's
//! values are frozen by the workshop migration (D-212).

use serde::{Deserialize, Serialize};

use super::floor::CredentialKind;
use super::gateway::GatewayRefusal;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a battery ended (D-212).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BatteryOutcome {
        Passed,
        Failed,
        Stopped,
        Error,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// FR-15's families.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BatteryFamily {
        Egress,
        Gateway,
        Host,
        Files,
        Privileges,
        Credentials,
        Resources,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How an attempt is judged (AD-117).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Judged {
        /// The action must fail; a failure counts only after its family's
        /// control succeeded in the same run (D-235, R62).
        Failure,
        /// The action must fail and the gateway must log a `refused` line
        /// with the expected cause, matched in order.
        GatewayLine,
        /// A fact read in the container must hold (a uid, a status field,
        /// a path that must not exist).
        Fact,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The control that must succeed before a family's failures count
    /// (D-235).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Control {
        /// The gateway's name resolves and its port 3128 opens.
        GatewayReachable,
        /// A second probe container on the decoy's network connects to it.
        DecoyListening,
        /// Writes to `/tmp` and `/work` succeed.
        WritableScratch,
    }
}

impl BatteryFamily {
    /// The family's control, for a family judged from failures.
    pub const fn control(self) -> Option<Control> {
        match self {
            BatteryFamily::Egress | BatteryFamily::Gateway => Some(Control::GatewayReachable),
            BatteryFamily::Host => Some(Control::DecoyListening),
            BatteryFamily::Files => Some(Control::WritableScratch),
            BatteryFamily::Privileges | BatteryFamily::Credentials | BatteryFamily::Resources => {
                None
            }
        }
    }
}

/// When an attempt applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applies {
    /// Every battery.
    Always,
    /// A `login` floor's battery only (D-603, D-606).
    LoginFloor,
}

/// One attempt of the battery (AD-117).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Attempt {
    pub code: &'static str,
    pub family: BatteryFamily,
    pub judged: Judged,
    /// The gateway's refusal a [`Judged::GatewayLine`] attempt expects.
    pub expected: Option<GatewayRefusal>,
    /// Aimed at Docker Desktop's own names: `not_applicable` passes off
    /// Docker Desktop.
    pub docker_desktop_only: bool,
    /// Phase 2 plays only the deadline and limit attempts (AD-117).
    pub phase: u8,
    pub applies: Applies,
}

const fn attempt(code: &'static str, family: BatteryFamily, judged: Judged) -> Attempt {
    Attempt {
        code,
        family,
        judged,
        expected: None,
        docker_desktop_only: false,
        phase: 1,
        applies: Applies::Always,
    }
}

const fn refused(code: &'static str, expected: GatewayRefusal, phase: u8) -> Attempt {
    Attempt {
        code,
        family: BatteryFamily::Gateway,
        judged: Judged::GatewayLine,
        expected: Some(expected),
        docker_desktop_only: false,
        phase,
        applies: Applies::Always,
    }
}

const fn desktop(attempt: Attempt) -> Attempt {
    Attempt {
        docker_desktop_only: true,
        ..attempt
    }
}

const fn login(attempt: Attempt) -> Attempt {
    Attempt {
        applies: Applies::LoginFloor,
        ..attempt
    }
}

/// Every attempt, seeded from FR-15's families (AD-117). The program, the
/// judgement and the vector set iterate this constant; the README names it
/// and lists none.
pub const ATTEMPTS: &[Attempt] = &[
    attempt("tcp_public", BatteryFamily::Egress, Judged::Failure),
    attempt("ipv6_socket", BatteryFamily::Egress, Judged::Failure),
    attempt("udp_dns_public", BatteryFamily::Egress, Judged::Failure),
    attempt(
        "resolve_outside_name",
        BatteryFamily::Egress,
        Judged::Failure,
    ),
    attempt("icmp", BatteryFamily::Egress, Judged::Failure),
    refused("off_list_host", GatewayRefusal::NotAllowed, 1),
    refused("ip_literal", GatewayRefusal::IpLiteral, 1),
    refused("port_80", GatewayRefusal::Port, 1),
    refused("port_22", GatewayRefusal::Port, 1),
    refused("absolute_get", GatewayRefusal::Method, 1),
    refused("malformed_request", GatewayRefusal::Malformed, 1),
    refused("oversized_request", GatewayRefusal::Oversized, 1),
    refused("sni_mismatch", GatewayRefusal::SniMismatch, 1),
    refused("loopback_name", GatewayRefusal::PrivateAddress, 1),
    refused("engine_host_name", GatewayRefusal::PrivateAddress, 1),
    refused("after_deadline", GatewayRefusal::Deadline, 2),
    refused("after_limit", GatewayRefusal::Limit, 2),
    desktop(attempt(
        "host_docker_internal",
        BatteryFamily::Host,
        Judged::Failure,
    )),
    desktop(attempt(
        "gateway_docker_internal",
        BatteryFamily::Host,
        Judged::Failure,
    )),
    attempt("network_gateways", BatteryFamily::Host, Judged::Failure),
    attempt("gateway_other_ports", BatteryFamily::Host, Judged::Failure),
    attempt("decoy_other_network", BatteryFamily::Host, Judged::Failure),
    attempt("engine_sockets", BatteryFamily::Host, Judged::Fact),
    attempt("write_battery_mount", BatteryFamily::Files, Judged::Failure),
    attempt("write_root", BatteryFamily::Files, Judged::Failure),
    attempt("write_usr", BatteryFamily::Files, Judged::Failure),
    attempt("write_officina_bin", BatteryFamily::Files, Judged::Failure),
    attempt("write_etc", BatteryFamily::Files, Judged::Failure),
    attempt("host_paths_absent", BatteryFamily::Files, Judged::Fact),
    attempt("mount_command", BatteryFamily::Files, Judged::Failure),
    login(attempt(
        "write_login_mount",
        BatteryFamily::Files,
        Judged::Failure,
    )),
    login(attempt(
        "create_in_login_mount",
        BatteryFamily::Files,
        Judged::Failure,
    )),
    attempt("uid_gid", BatteryFamily::Privileges, Judged::Fact),
    attempt("capabilities", BatteryFamily::Privileges, Judged::Fact),
    attempt("no_new_privs", BatteryFamily::Privileges, Judged::Fact),
    attempt("seccomp", BatteryFamily::Privileges, Judged::Fact),
    attempt("no_setuid_files", BatteryFamily::Privileges, Judged::Fact),
    attempt("su_sudo", BatteryFamily::Privileges, Judged::Fact),
    attempt("ssh_keys", BatteryFamily::Credentials, Judged::Fact),
    attempt("agent_socket", BatteryFamily::Credentials, Judged::Fact),
    attempt("git_credentials", BatteryFamily::Credentials, Judged::Fact),
    attempt("netrc", BatteryFamily::Credentials, Judged::Fact),
    attempt(
        "cloud_credentials",
        BatteryFamily::Credentials,
        Judged::Fact,
    ),
    attempt("engine_config", BatteryFamily::Credentials, Judged::Fact),
    attempt(
        "environment_names",
        BatteryFamily::Credentials,
        Judged::Fact,
    ),
    attempt("process_limit", BatteryFamily::Resources, Judged::Fact),
    attempt("memory_limit", BatteryFamily::Resources, Judged::Fact),
];

/// The attempts a battery of `kind` plays in `phase`, in [`ATTEMPTS`]'
/// order.
pub fn attempts_for(kind: CredentialKind, phase: u8) -> impl Iterator<Item = &'static Attempt> {
    ATTEMPTS.iter().filter(move |a| {
        a.phase == phase && (a.applies == Applies::Always || kind == CredentialKind::Login)
    })
}

/// The attempt `code`, if [`ATTEMPTS`] holds it.
pub fn attempt_named(code: &str) -> Option<&'static Attempt> {
    ATTEMPTS.iter().find(|a| a.code == code)
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the program saw of one attempt (AD-117).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum AttemptOutcome {
        Refused,
        Escaped,
        Inconclusive,
        NotApplicable,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A control's result (D-235).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ControlStatus {
        Ok,
        Failed,
    }
}

/// The most bytes of an attempt line's detail.
pub const MAX_DETAIL_BYTES: usize = 1024;
/// The most bytes of one line the program prints (D-216).
pub const MAX_LINE_BYTES: usize = 4096;

/// One attempt's line (AD-117).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptLine {
    pub family: BatteryFamily,
    pub attempt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<GatewayRefusal>,
    pub outcome: AttemptOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// One control's line (D-235).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlLine {
    pub family: BatteryFamily,
    pub control: Control,
    pub status: ControlStatus,
}

/// A line of the battery program: one JSON object, tagged by `line`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "line", rename_all = "snake_case")]
pub enum BatteryLine {
    Attempt(AttemptLine),
    Control(ControlLine),
}

impl BatteryLine {
    /// The line as the program prints it, its detail cut to
    /// [`MAX_DETAIL_BYTES`] on a character boundary.
    pub fn to_json(&self) -> String {
        let mut line = self.clone();
        if let BatteryLine::Attempt(AttemptLine {
            detail: Some(detail),
            ..
        }) = &mut line
        {
            let mut end = detail.len().min(MAX_DETAIL_BYTES);
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
        }
        serde_json::to_string(&line).unwrap_or_default()
    }
}

/// Parses one line of the program's output: `None` for anything that is
/// not a battery line within its bounds, which the judgement then misses.
pub fn parse_line(text: &str) -> Option<BatteryLine> {
    if text.len() > MAX_LINE_BYTES {
        return None;
    }
    let line: BatteryLine = serde_json::from_str(text.trim()).ok()?;
    match &line {
        BatteryLine::Attempt(a)
            if a.detail
                .as_ref()
                .is_some_and(|d| d.len() > MAX_DETAIL_BYTES) =>
        {
            None
        }
        _ => Some(line),
    }
}

/// The value the battery worker's credential name carries (D-206): never
/// a credential, so nothing a battery prints can leak one.
pub const DUMMY_CREDENTIAL: &str = "officina-battery-dummy-credential";
/// The allow-listed test host the gateway family aims at (AD-117).
pub const TEST_HOST: &str = "officina-allowed.test";
/// The allow-listed name gateway A resolves to loopback (AD-117).
pub const LOOPBACK_NAME: &str = "officina-loopback.test";
/// The allow-listed name gateway A resolves to the engine host (AD-117).
pub const ENGINE_HOST_NAME: &str = "officina-host.test";

/// The public address gateway A resolves [`TEST_HOST`] to (D-1140): the
/// SNI attempt needs a name that passes every check before the
/// ClientHello, and the gateway refuses that attempt before it connects,
/// so nothing is ever sent there.
pub const TEST_HOST_ADDRESS: &str = "1.1.1.1";
/// The egress family's public targets (AD-117): TCP addresses, an IPv6
/// one, a public resolver for the UDP query, a name to resolve and an
/// address to ping. Public anycast resolvers, so a success means a route
/// out, not a reachable test service.
pub const PUBLIC_TCP: &[&str] = &["1.1.1.1:443", "8.8.8.8:53"];
pub const PUBLIC_IPV6: &str = "[2606:4700:4700::1111]:443";
pub const PUBLIC_RESOLVER: &str = "1.1.1.1:53";
pub const OUTSIDE_NAME: &str = "example.com";
pub const ICMP_TARGET: &str = "1.1.1.1";
/// The off-list name the gateway family asks for (AD-117).
pub const OFF_LIST_HOST: &str = "officina-off-list.test";
/// The name the SNI attempt's ClientHello carries instead of the test
/// host (AD-117).
pub const OTHER_SNI_HOST: &str = "officina-other.test";
/// The documentation address the IP-literal attempt asks for.
pub const LITERAL_ADDRESS: &str = "192.0.2.10";
/// The port the decoy listens on (AD-117).
pub const DECOY_PORT: u16 = 47_821;
/// The ports the host family tries on each address it aims at (AD-117).
pub const HOST_PORTS: &[u16] = &[22, 80, 2375, 2376];
/// Docker Desktop's names the decoy resolves (AD-117), in the order of
/// [`DecoyAddresses`]' fields.
pub const DOCKER_DESKTOP_NAMES: &[&str] = &["host.docker.internal", "gateway.docker.internal"];
/// The names the container runtime itself sets in every container, beside
/// the vector's and the image's: `docker create` adds `HOSTNAME` (D-1141).
pub const RUNTIME_ENV: &[&str] = &["HOSTNAME"];

/// What the decoy saw (AD-117): its own addresses on its bridge, and what
/// each of [`DOCKER_DESKTOP_NAMES`] resolved to (empty off Docker Desktop).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecoyAddresses {
    pub own: Vec<String>,
    pub host_docker_internal: Vec<String>,
    pub gateway_docker_internal: Vec<String>,
}

/// A line of `officina-entry decoy` (AD-117): its addresses, then
/// `listening` once it connected to its own port; a probe prints
/// `connected` or `unreachable`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "line", rename_all = "snake_case")]
pub enum DecoyLine {
    Addresses(DecoyAddresses),
    Listening { port: u16 },
    Connected,
    Unreachable,
}

impl DecoyLine {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The decoy's lines in `output`, those within [`MAX_LINE_BYTES`] that
/// parse; anything else is skipped.
pub fn parse_decoy(output: &str) -> Vec<DecoyLine> {
    output
        .lines()
        .filter(|l| l.len() <= MAX_LINE_BYTES)
        .filter_map(|l| serde_json::from_str(l.trim()).ok())
        .collect()
}

/// What `officina-entry battery` is told (AD-117): the plan core builds
/// and the program parses back, so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The gateway's name on the battery's internal network.
    pub gateway: String,
    pub gateway_port: u16,
    pub decoy: DecoyAddresses,
    pub decoy_port: u16,
    /// The battery mount's path in the container.
    pub mount: String,
    /// A `login` floor's login mount path (D-603, D-606).
    pub login_mount: Option<String>,
    /// The environment's names, sorted: the vector's, the credential's,
    /// the image's recorded `ENV` names and [`RUNTIME_ENV`] (AD-102, PRD
    /// amendment 4).
    pub expected_env: Vec<String>,
    pub pids_limit: u32,
    pub memory_mib: u32,
    /// Phase 2 waits this long from its own start before it tries, so the
    /// gateway started before it is past its deadline (AD-117).
    pub wait_s: u64,
    /// The bad requests phase 2 sends after its try (AD-117).
    pub bad_requests: u32,
}

/// The expected environment names (AD-117): `vector` (the battery
/// vector's names, the credential's among them for a token kind), the
/// image's recorded `ENV` names and [`RUNTIME_ENV`], sorted and
/// deduplicated.
pub fn expected_env(vector: &[String], image_env: &[String]) -> Vec<String> {
    let mut names: Vec<String> = vector
        .iter()
        .chain(image_env)
        .cloned()
        .chain(RUNTIME_ENV.iter().map(|n| (*n).to_owned()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// How long gateway A lives, from its own start (AD-117, D-242).
pub const PHASE_1_DEADLINE_S: u64 = 600;
/// Gateway A's refusal limit: high enough that phase 1 never trips it.
pub const PHASE_1_LIMIT: u32 = 1000;
/// Gateway B's deadline, from its own start (AD-117).
pub const PHASE_2_DEADLINE_S: u64 = 15;
/// Gateway B's refusal limit (AD-117), the gateway's own.
pub const PHASE_2_LIMIT: u32 = super::gateway::REFUSAL_LIMIT;
/// The margin phase 2 waits past gateway B's deadline.
pub const PHASE_2_MARGIN_S: u64 = 1;

/// A phase's time limit in milliseconds, which the attached start's
/// deadline adds 60 s to (D-205): phase 1 its gateway's deadline, phase 2
/// its wait.
pub const fn phase_limit_ms(phase: u8) -> u64 {
    match phase {
        1 => PHASE_1_DEADLINE_S * 1000,
        _ => (PHASE_2_DEADLINE_S + PHASE_2_MARGIN_S) * 1000,
    }
}

/// The battery gateway of `phase` (AD-117): the test names allow-listed,
/// direct mode (P-55), gateway A's long deadline and high limit, gateway
/// B's short ones.
pub fn gateway_config(run: &str, phase: u8) -> super::gateway::GatewayConfig {
    let hosts = [TEST_HOST, LOOPBACK_NAME, ENGINE_HOST_NAME].map(str::to_owned);
    let (deadline_s, limit) = match phase {
        1 => (PHASE_1_DEADLINE_S, PHASE_1_LIMIT),
        _ => (PHASE_2_DEADLINE_S, PHASE_2_LIMIT),
    };
    let mut config = super::gateway::GatewayConfig::new(
        run,
        &hosts,
        deadline_s,
        std::collections::BTreeMap::new(),
    );
    config.refusal_limit = limit;
    config
}

/// The gateways' refusals in `output`, in order (AD-117): each `refused`
/// line's cause, and a `limit` line as [`GatewayRefusal::Limit`]; other
/// lines and anything over [`MAX_LINE_BYTES`] are skipped.
pub fn gateway_refusals(output: &str) -> Vec<GatewayRefusal> {
    use super::gateway::GatewayLine;
    output
        .lines()
        .filter(|l| l.len() <= MAX_LINE_BYTES)
        .filter_map(|l| serde_json::from_str::<GatewayLine>(l.trim()).ok())
        .filter_map(|line| match line {
            GatewayLine::Refused { cause, .. } => Some(cause),
            GatewayLine::Limit {} => Some(GatewayRefusal::Limit),
            GatewayLine::Started { .. } | GatewayLine::Allowed { .. } => None,
        })
        .collect()
}

/// The program's lines in `output` that parse, in order.
pub fn battery_lines(output: &str) -> Vec<BatteryLine> {
    output.lines().filter_map(parse_line).collect()
}

/// The plan as it keys validity (D-232): the run's names and what the
/// decoy saw cleared, so two batteries of one build and image agree.
pub fn canonical_plan(plan: &Plan) -> Plan {
    Plan {
        gateway: String::new(),
        decoy: DecoyAddresses::default(),
        ..plan.clone()
    }
}

/// `vector_sha256` (D-232): SHA-256 over D-210's normalised `Battery`
/// vector and the canonical plan, one per line.
pub fn vector_sha256(normalised_vector: &[String], plan: &Plan) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in normalised_vector {
        hasher.update(part.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(serde_json::to_string(&canonical_plan(plan)).unwrap_or_default());
    crate::hex(&hasher.finalize())
}

/// The work-order label value of a battery's objects (D-215).
pub const BATTERY_WORK_ORDER: &str = "battery";
/// The commit identity a battery worker carries: a battery commits nothing,
/// and a fixed one keeps its vector's digest the same across batteries.
pub const BATTERY_GIT_NAME: &str = "Officina battery";
pub const BATTERY_GIT_EMAIL: &str = "battery@officina.invalid";
/// A battery id for the canonical digest: names are normalised away, so
/// any valid id gives the same digest.
const CANONICAL_ID: &str = "00000000000000000000000000000000";

/// What a battery is built from (AD-117, D-232): the floor's credential
/// kind and worker limits, the image and the build that run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatterySpec {
    pub kind: CredentialKind,
    pub cpus: u32,
    pub memory_mib: u32,
    pub floor_key: String,
    pub version: String,
    pub image_id: String,
    /// The image's recorded `ENV` names (D-233).
    pub image_env: Vec<String>,
}

/// One phase's battery worker and its plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseWorker {
    pub vector: super::vector::ContainerVector,
    pub plan: Plan,
}

impl BatterySpec {
    fn limits(&self) -> super::vector::Limits {
        super::vector::Limits::worker(self.cpus, self.memory_mib)
    }

    /// The labels of the battery's object in `role` (D-215).
    pub fn labels(
        &self,
        battery: &super::names::RunId,
        role: super::vector::ContainerRole,
    ) -> std::collections::BTreeMap<String, String> {
        super::names::run_labels(
            &self.floor_key,
            &self.version,
            battery,
            role,
            BATTERY_WORK_ORDER,
        )
    }

    /// The credential source of the battery worker (D-206, D-603): the
    /// kind's name, or the battery's own empty login volume.
    pub fn credential(
        &self,
        names: &super::names::BatteryNames,
    ) -> super::vector::CredentialSource {
        match super::vector::credential_name(self.kind) {
            Some(name) => super::vector::CredentialSource::Name(name),
            None => super::vector::CredentialSource::Volume(names.login_volume.clone()),
        }
    }

    /// The battery worker of `phase` (AD-117): the `Worker` vector's shape
    /// with the battery's command, its one mount at `mount_source`, and the
    /// plan it is told, its gateway the phase's and `decoy` what the decoy
    /// printed.
    pub fn worker(
        &self,
        battery: &super::names::RunId,
        phase: u8,
        mount_source: &str,
        decoy: &DecoyAddresses,
    ) -> PhaseWorker {
        use super::vector::{
            ContainerRole, GATEWAY_PORT, GitIdentity, LOGIN_DIR, MOUNTS_DIR, WorkerShape,
        };
        let names = super::names::BatteryNames::of(battery);
        let (name, gateway) = match phase {
            1 => (&names.run.worker, &names.run.gateway),
            _ => (&names.worker_b, &names.gateway_b),
        };
        let labels = self.labels(battery, ContainerRole::Battery);
        let credential = self.credential(&names);
        let git = GitIdentity {
            name: BATTERY_GIT_NAME.to_owned(),
            email: BATTERY_GIT_EMAIL.to_owned(),
        };
        let mounts = [mount_source.to_owned()];
        let shape = WorkerShape {
            name,
            labels: &labels,
            image_id: &self.image_id,
            network: &names.run.internal,
            volume: &names.run.volume,
            gateway,
            limits: self.limits(),
            mounts: &mounts,
            credential: &credential,
            git: &git,
            memory: None,
        };
        let env = super::vector::battery(&shape, phase, &[]).env_names();
        let limits = self.limits();
        let plan = Plan {
            gateway: gateway.as_str().to_owned(),
            gateway_port: GATEWAY_PORT,
            decoy: decoy.clone(),
            decoy_port: DECOY_PORT,
            mount: format!("{MOUNTS_DIR}/0"),
            login_mount: (self.kind == CredentialKind::Login).then(|| LOGIN_DIR.to_owned()),
            expected_env: expected_env(&env, &self.image_env),
            pids_limit: limits.pids,
            memory_mib: limits.memory_mib,
            wait_s: PHASE_2_DEADLINE_S + PHASE_2_MARGIN_S,
            bad_requests: PHASE_2_LIMIT,
        };
        PhaseWorker {
            vector: super::vector::battery(&shape, phase, &self::plan(&plan)),
            plan,
        }
    }

    /// `vector_sha256` of this spec (D-232): the canonical battery's, so
    /// the window can tell a stale image without running one.
    pub fn vector_sha256(&self) -> String {
        let Some(id) = super::names::RunId::parse(CANONICAL_ID) else {
            return String::new();
        };
        let worker = self.worker(&id, 1, "", &DecoyAddresses::default());
        vector_sha256(&super::vector::normalised(&worker.vector), &worker.plan)
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why Run escape battery did not start (AD-117).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum BatteryRefusal {
        /// The workshop's stop flag is set (AD-116).
        WorkshopStopped,
        /// The floor declares no valid `[workshop]`.
        WorkshopOff,
        /// No catalogue was approved, or the window's record does not vouch
        /// for it (D-175).
        NotApproved,
        /// The floor changed since its approval (D-244).
        CatalogueChanged,
        /// The image asked for, or the gateway image, has no successful
        /// build on record.
        ImageMissing,
        /// No image's battery is stale.
        NothingStale,
        /// The engine is not a reachable Linux engine (AD-98).
        EngineUnavailable,
        /// A build or a battery is already running on thread
        /// `workshop-build`.
        Busy,
    }
}

/// How `run_battery` was answered: queued on thread `workshop-build`, each
/// battery's row and lines reaching the registry and the journal, or
/// refused before any container starts, with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum BatteryAnswer {
    /// The images whose battery is queued, by tag.
    Queued(Vec<String>),
    Refused(BatteryRefusal),
}

/// The engine's clock against the window's around the `info` call
/// (D-242): the engine's `SystemTime` minus the call's midpoint, a
/// diagnostic only; `None` when the time does not parse.
pub fn clock_skew_ms(system_time: &str, before_ms: i64, after_ms: i64) -> Option<i64> {
    let engine = crate::dates::parse_rfc3339_ms(system_time)?;
    Some(engine - (before_ms + (after_ms - before_ms) / 2))
}

/// The plan as arguments after `--phase <n>` (AD-117): one `--plan` and
/// its JSON, which the program parses with [`parse_plan`].
pub fn plan(plan: &Plan) -> Vec<String> {
    vec![
        "--plan".to_owned(),
        serde_json::to_string(plan).unwrap_or_default(),
    ]
}

/// The plan from the program's arguments after `--phase <n>`.
pub fn parse_plan(args: &[String]) -> Option<Plan> {
    match args {
        [flag, json] if flag == "--plan" => serde_json::from_str(json).ok(),
        _ => None,
    }
}

/// The exit code gateway B must give: its refusal limit reached (AD-103).
pub const GATEWAY_B_EXIT: i32 = 3;

/// What a run gives the judgement (AD-117).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeInput {
    pub kind: CredentialKind,
    /// The battery workers' lines, phase 1 then phase 2.
    pub lines: Vec<BatteryLine>,
    /// The gateways' `refused` causes in order, gateway A then gateway B.
    pub refusals: Vec<GatewayRefusal>,
    pub gateway_b_exit: Option<i32>,
    /// The engine's platform name says Docker Desktop.
    pub docker_desktop: bool,
}

/// The counts a battery row and its `escape_battery` line hold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub attempts: u32,
    pub refused: u32,
    pub escaped: u32,
    pub inconclusive: u32,
    pub not_applicable: u32,
}

/// One attempt as judged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptVerdict {
    pub attempt: String,
    pub family: BatteryFamily,
    pub outcome: AttemptOutcome,
    /// Why the judgement differs from the line, or what failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A battery's judgement (AD-117).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judgement {
    pub outcome: BatteryOutcome,
    pub totals: Totals,
    pub attempts: Vec<AttemptVerdict>,
    /// What failed the battery, in order; empty when it passed.
    pub failures: Vec<String>,
}

/// Matches `expected` against the gateway's refusals from `next` on, in
/// order: a match moves `next` past it; a miss leaves it, so one missing
/// line fails one attempt, not every one after it.
fn match_refusal(refusals: &[GatewayRefusal], next: &mut usize, expected: GatewayRefusal) -> bool {
    match refusals.iter().skip(*next).position(|r| *r == expected) {
        Some(offset) => {
            *next += offset + 1;
            true
        }
        None => false,
    }
}

/// Judges a run (AD-117, D-235): it passes only when every control
/// succeeded, every expected attempt is `refused` (or `not_applicable`
/// off Docker Desktop for an attempt aimed at its names), each gateway-line
/// attempt is matched in order by a gateway refusal with its cause, and
/// gateway B exited [`GATEWAY_B_EXIT`]. A missing line or a failed control
/// makes the attempt `inconclusive`. Clock skew is no input (D-242).
pub fn judge(input: &JudgeInput) -> Judgement {
    let mut failures = Vec::new();
    let controls: Vec<&ControlLine> = input
        .lines
        .iter()
        .filter_map(|l| match l {
            BatteryLine::Control(c) => Some(c),
            BatteryLine::Attempt(_) => None,
        })
        .collect();
    let control_ok = |family: BatteryFamily| match family.control() {
        None => true,
        Some(control) => {
            controls.iter().any(|c| {
                c.family == family && c.control == control && c.status == ControlStatus::Ok
            }) && !controls
                .iter()
                .any(|c| c.family == family && c.status == ControlStatus::Failed)
        }
    };
    let line_of = |code: &str| {
        input.lines.iter().find_map(|l| match l {
            BatteryLine::Attempt(a) if a.attempt == code => Some(a),
            _ => None,
        })
    };
    let mut next_refusal = 0;
    let mut judged = Vec::new();
    let mut totals = Totals::default();
    for phase in [1, 2] {
        for attempt in attempts_for(input.kind, phase) {
            let mut reason = None;
            let mut outcome = match line_of(attempt.code) {
                None => {
                    reason = Some("no line".to_owned());
                    AttemptOutcome::Inconclusive
                }
                Some(line) => line.outcome,
            };
            if outcome == AttemptOutcome::Refused
                && attempt.judged != Judged::Fact
                && !control_ok(attempt.family)
            {
                outcome = AttemptOutcome::Inconclusive;
                reason = Some("its family's control failed".to_owned());
            }
            if outcome == AttemptOutcome::NotApplicable
                && (input.docker_desktop || !attempt.docker_desktop_only)
            {
                reason = Some("not applicable here".to_owned());
            }
            if outcome == AttemptOutcome::Refused
                && let Some(expected) = attempt.expected
                && !match_refusal(&input.refusals, &mut next_refusal, expected)
            {
                outcome = AttemptOutcome::Inconclusive;
                reason = Some(format!("no gateway refusal {}", expected.code()));
            }
            totals.attempts += 1;
            match outcome {
                AttemptOutcome::Refused => totals.refused += 1,
                AttemptOutcome::Escaped => totals.escaped += 1,
                AttemptOutcome::Inconclusive => totals.inconclusive += 1,
                AttemptOutcome::NotApplicable => totals.not_applicable += 1,
            }
            let passes = match outcome {
                AttemptOutcome::Refused => true,
                AttemptOutcome::NotApplicable => reason.is_none(),
                AttemptOutcome::Escaped | AttemptOutcome::Inconclusive => false,
            };
            if !passes {
                failures.push(format!(
                    "{}: {}{}",
                    attempt.code,
                    outcome.code(),
                    reason
                        .as_deref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                ));
            }
            judged.push(AttemptVerdict {
                attempt: attempt.code.to_owned(),
                family: attempt.family,
                outcome,
                reason,
            });
        }
    }
    if input.gateway_b_exit != Some(GATEWAY_B_EXIT) {
        failures.push(format!(
            "gateway B exited {:?}, not {GATEWAY_B_EXIT}",
            input.gateway_b_exit
        ));
    }
    Judgement {
        outcome: if failures.is_empty() {
            BatteryOutcome::Passed
        } else {
            BatteryOutcome::Failed
        },
        totals,
        attempts: judged,
        failures,
    }
}

/// How long a passing battery stays valid (AD-117).
pub const VALIDITY_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// What a battery's validity keys on (AD-117, D-232).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidityKey {
    pub engine_key: String,
    pub image_id: String,
    pub gateway_image_id: String,
    pub officina_version: String,
    pub vector_sha256: String,
    pub programs_sha256: String,
}

impl ValidityKey {
    /// The names of the fields that differ from `other`, in declaration
    /// order (doctor's `battery_stale`, AD-123).
    pub fn moved(&self, other: &ValidityKey) -> Vec<&'static str> {
        [
            ("engine", self.engine_key == other.engine_key),
            ("image", self.image_id == other.image_id),
            (
                "gateway image",
                self.gateway_image_id == other.gateway_image_id,
            ),
            (
                "Officina version",
                self.officina_version == other.officina_version,
            ),
            ("vector", self.vector_sha256 == other.vector_sha256),
            (
                "measured programs",
                self.programs_sha256 == other.programs_sha256,
            ),
        ]
        .into_iter()
        .filter_map(|(name, same)| (!same).then_some(name))
        .collect()
    }
}

/// One recorded battery, as validity reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryRun {
    pub key: ValidityKey,
    pub outcome: Option<BatteryOutcome>,
    pub finished_at_ms: Option<i64>,
}

/// The passing run that makes `key` valid at `now_ms` (AD-117, D-232): the
/// newest passing row with every key equal, finished in the last
/// [`VALIDITY_MS`].
pub fn is_valid<'r>(
    runs: &'r [BatteryRun],
    key: &ValidityKey,
    now_ms: i64,
) -> Option<&'r BatteryRun> {
    runs.iter()
        .filter(|run| run.outcome == Some(BatteryOutcome::Passed) && run.key == *key)
        .filter(|run| {
            run.finished_at_ms
                .is_some_and(|at| at <= now_ms && now_ms - at < VALIDITY_MS)
        })
        .max_by_key(|run| run.finished_at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> ValidityKey {
        ValidityKey {
            engine_key: "e".repeat(64),
            image_id: format!("sha256:{}", "a".repeat(64)),
            gateway_image_id: format!("sha256:{}", "b".repeat(64)),
            officina_version: "0.6.0".into(),
            vector_sha256: "v".repeat(64),
            programs_sha256: "p".repeat(64),
        }
    }

    fn passed(key: ValidityKey, at: i64) -> BatteryRun {
        BatteryRun {
            key,
            outcome: Some(BatteryOutcome::Passed),
            finished_at_ms: Some(at),
        }
    }

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn validity_needs_every_key() {
        let runs = [passed(key(), NOW - 1000)];
        assert!(is_valid(&runs, &key(), NOW).is_some());
        let edits: [fn(&mut ValidityKey); 6] = [
            |k| k.engine_key.push('x'),
            |k| k.image_id.push('x'),
            |k| k.gateway_image_id.push('x'),
            |k| k.officina_version.push('x'),
            |k| k.vector_sha256.push('x'),
            |k| k.programs_sha256.push('x'),
        ];
        for edit in edits {
            let mut moved = key();
            edit(&mut moved);
            assert!(is_valid(&runs, &moved, NOW).is_none(), "{moved:?}");
            assert_eq!(key().moved(&moved).len(), 1);
        }
    }

    #[test]
    fn validity_needs_the_vector_digest() {
        let mut other = key();
        other.vector_sha256 = "w".repeat(64);
        assert!(is_valid(&[passed(key(), NOW)], &other, NOW).is_none());
        assert_eq!(key().moved(&other), ["vector"]);
    }

    #[test]
    fn validity_needs_the_programs_digest() {
        let mut other = key();
        other.programs_sha256 = "q".repeat(64);
        assert!(is_valid(&[passed(key(), NOW)], &other, NOW).is_none());
        assert_eq!(key().moved(&other), ["measured programs"]);
    }

    #[test]
    fn validity_lasts_30_days() {
        assert!(is_valid(&[passed(key(), NOW - VALIDITY_MS + 1)], &key(), NOW).is_some());
        assert!(is_valid(&[passed(key(), NOW - VALIDITY_MS)], &key(), NOW).is_none());
        let runs = [passed(key(), NOW - 5000), passed(key(), NOW - 10)];
        let newest = is_valid(&runs, &key(), NOW).unwrap();
        assert_eq!(newest.finished_at_ms, Some(NOW - 10));
    }

    #[test]
    fn a_failed_battery_never_validates() {
        for outcome in BatteryOutcome::ALL
            .iter()
            .filter(|o| **o != BatteryOutcome::Passed)
        {
            let run = BatteryRun {
                outcome: Some(*outcome),
                ..passed(key(), NOW)
            };
            assert!(is_valid(&[run], &key(), NOW).is_none(), "{outcome:?}");
        }
    }

    #[test]
    fn a_line_round_trips_and_its_detail_is_bounded() {
        let line = BatteryLine::Attempt(AttemptLine {
            family: BatteryFamily::Gateway,
            attempt: "port_22".into(),
            expected: Some(GatewayRefusal::Port),
            outcome: AttemptOutcome::Refused,
            detail: Some("é".repeat(MAX_DETAIL_BYTES)),
        });
        let text = line.to_json();
        let parsed = parse_line(&text).expect("a battery line");
        let BatteryLine::Attempt(parsed) = parsed else {
            panic!("an attempt");
        };
        assert!(parsed.detail.unwrap().len() <= MAX_DETAIL_BYTES);
        assert!(parse_line("{\"line\":\"attempt\"}").is_none());
        assert!(parse_line(&"x".repeat(MAX_LINE_BYTES + 1)).is_none());
    }

    fn spec(kind: CredentialKind) -> BatterySpec {
        BatterySpec {
            kind,
            cpus: 2,
            memory_mib: 4096,
            floor_key: "0123456789abcdef".into(),
            version: "0.6.0".into(),
            image_id: format!("sha256:{}", "a".repeat(64)),
            image_env: vec!["PATH".into(), "SSL_CERT_FILE".into()],
        }
    }

    /// D-232: a real battery's digest, either phase, equals the one the
    /// window computes to tell a stale image, whatever its id, mount
    /// source and decoy addresses.
    #[test]
    fn the_canonical_digest_is_every_real_batterys() {
        use crate::workshop::names::RunId;
        for kind in CredentialKind::ALL {
            let spec = spec(*kind);
            let id = RunId::parse("fedcba9876543210fedcba9876543210").unwrap();
            let decoy = DecoyAddresses {
                own: vec!["172.18.0.2".into()],
                host_docker_internal: vec!["192.168.65.254".into()],
                gateway_docker_internal: vec!["192.168.65.1".into()],
            };
            for phase in [1, 2] {
                let real = spec.worker(&id, phase, "C:/officina/workshop/battery/mount", &decoy);
                assert_eq!(
                    vector_sha256(
                        &crate::workshop::vector::normalised(&real.vector),
                        &real.plan
                    ),
                    spec.vector_sha256(),
                    "{kind:?} phase {phase}"
                );
            }
        }
    }

    #[test]
    fn the_digest_moves_with_the_vector_and_the_image_env() {
        let base = spec(CredentialKind::OauthToken);
        let digests = [
            base.vector_sha256(),
            BatterySpec {
                memory_mib: 2048,
                ..base.clone()
            }
            .vector_sha256(),
            BatterySpec {
                image_env: vec!["PATH".into()],
                ..base.clone()
            }
            .vector_sha256(),
            spec(CredentialKind::Login).vector_sha256(),
        ];
        for (i, a) in digests.iter().enumerate() {
            for b in &digests[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    /// AD-117, D-603: a token battery expects its credential's name, a
    /// login one none, and plays its login mount.
    #[test]
    fn the_plan_expects_the_kinds_credential_name_only() {
        use crate::workshop::names::RunId;
        let id = RunId::parse("fedcba9876543210fedcba9876543210").unwrap();
        let token =
            spec(CredentialKind::OauthToken).worker(&id, 1, "m", &DecoyAddresses::default());
        assert!(
            token
                .plan
                .expected_env
                .contains(&"CLAUDE_CODE_OAUTH_TOKEN".to_owned())
        );
        assert!(token.plan.expected_env.contains(&"HOSTNAME".to_owned()));
        assert_eq!(token.plan.login_mount, None);
        let login = spec(CredentialKind::Login).worker(&id, 1, "m", &DecoyAddresses::default());
        assert!(
            !login
                .plan
                .expected_env
                .iter()
                .any(|n| n.ends_with("_TOKEN") || n.ends_with("_API_KEY"))
        );
        assert_eq!(
            login.plan.login_mount.as_deref(),
            Some(crate::workshop::vector::LOGIN_DIR)
        );
        assert_eq!(parse_plan(&plan(&login.plan)), Some(login.plan.clone()));
    }

    #[test]
    fn gateway_lines_give_the_refusals_in_order() {
        use crate::workshop::gateway::GatewayLine;
        let output = [
            GatewayLine::Started {
                digest: "d".into(),
                upstreams: Vec::new(),
            },
            GatewayLine::refused("a.test", 443, GatewayRefusal::NotAllowed, None),
            GatewayLine::Allowed {
                host: "api.anthropic.com".into(),
                port: 443,
                ech: false,
            },
            GatewayLine::refused("", 0, GatewayRefusal::Malformed, None),
            GatewayLine::Limit {},
        ]
        .iter()
        .map(GatewayLine::to_line)
        .chain(["not json".to_owned(), "x".repeat(MAX_LINE_BYTES + 1)])
        .collect::<Vec<_>>()
        .join("\n");
        assert_eq!(
            gateway_refusals(&output),
            [
                GatewayRefusal::NotAllowed,
                GatewayRefusal::Malformed,
                GatewayRefusal::Limit
            ]
        );
    }

    #[test]
    fn the_skew_is_the_engine_against_the_calls_midpoint() {
        assert_eq!(clock_skew_ms("1970-01-01T00:00:01Z", 900, 1100), Some(0));
        assert_eq!(
            clock_skew_ms("1970-01-01T00:00:03.5Z", 1000, 1000),
            Some(2500)
        );
        assert_eq!(clock_skew_ms("not a time", 0, 0), None);
    }

    #[test]
    fn attempt_codes_are_unique() {
        let mut codes: Vec<&str> = ATTEMPTS.iter().map(|a| a.code).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(before, codes.len());
        for attempt in ATTEMPTS {
            assert_eq!(
                attempt.expected.is_some(),
                attempt.judged == Judged::GatewayLine,
                "{}",
                attempt.code
            );
        }
    }
}
