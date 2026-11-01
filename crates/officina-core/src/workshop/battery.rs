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

/// What `officina-entry battery` is told (AD-117): the plan core builds
/// and the program parses back, so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub gateway: String,
    pub decoy_addresses: Vec<String>,
    pub decoy_port: u16,
    /// The battery mount's path in the container.
    pub mount: String,
    /// A `login` floor's login mount path (D-603, D-606).
    pub login_mount: Option<String>,
    /// The environment's names, sorted: the vector's, the credential's
    /// and the image's recorded `ENV` names (AD-102, PRD amendment 4).
    pub expected_env: Vec<String>,
    pub pids_limit: u32,
    pub memory_mib: u32,
}

/// The expected environment names (AD-117): `vector` (the battery
/// vector's names, the credential's among them for a token kind) and the
/// image's recorded `ENV` names, sorted and deduplicated.
pub fn expected_env(vector: &[String], image_env: &[String]) -> Vec<String> {
    let mut names: Vec<String> = vector.iter().chain(image_env).cloned().collect();
    names.sort();
    names.dedup();
    names
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
