//! The escape battery on the host (AD-117, D-235): the decoy and its
//! probe, the two phases' gateways and battery workers, each created by
//! core's builders, labelled and removed afterwards; the lines collected,
//! judged by core, and recorded by the collector as one `workshop_battery`
//! row and its journal lines. Run escape battery's command grants it on the
//! collector thread and runs it on thread `workshop-build`.

use officina_core::workshop::battery::{BatteryRun, ValidityKey, is_valid};

use crate::registry::workshop::{BatteryKey, BatteryRecord};

impl From<&BatteryKey> for ValidityKey {
    fn from(key: &BatteryKey) -> ValidityKey {
        ValidityKey {
            engine_key: key.engine_key.clone(),
            image_id: key.image_id.clone(),
            gateway_image_id: key.gateway_image_id.clone(),
            officina_version: key.officina_version.clone(),
            vector_sha256: key.vector_sha256.clone(),
            programs_sha256: key.programs_sha256.clone(),
        }
    }
}

/// The recorded batteries as validity reads them.
pub fn runs_of(records: &[BatteryRecord]) -> Vec<BatteryRun> {
    records
        .iter()
        .map(|r| BatteryRun {
            key: ValidityKey::from(&r.key),
            outcome: r.outcome,
            finished_at_ms: r.finished_at_ms,
        })
        .collect()
}

/// The passing battery that makes `key` valid at `now_ms`, among `records`
/// (AD-117, D-232).
pub fn valid_battery<'r>(
    records: &'r [BatteryRecord],
    key: &ValidityKey,
    now_ms: i64,
) -> Option<&'r BatteryRecord> {
    let runs = runs_of(records);
    let found = is_valid(&runs, key, now_ms)?;
    records
        .iter()
        .find(|r| r.finished_at_ms == found.finished_at_ms && ValidityKey::from(&r.key) == *key)
}

#[cfg(feature = "window-authority")]
pub use window::*;

#[cfg(feature = "window-authority")]
mod window {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use officina_core::relay::journal::{
        EscapeAttempt, EscapeBattery, GatewayRefused, JournalEntry,
    };
    use officina_core::workshop::battery::{
        AttemptVerdict, BatteryAnswer, BatteryLine, BatteryOutcome, BatteryRefusal, BatterySpec,
        Control, ControlLine, ControlStatus, DECOY_PORT, DUMMY_CREDENTIAL, DecoyAddresses,
        DecoyLine, JudgeInput, Totals, ValidityKey, battery_lines, clock_skew_ms, gateway_config,
        gateway_refusals, judge, parse_decoy, phase_limit_ms,
    };
    use officina_core::workshop::engine::{
        EngineCall, EngineInfo, InfoFacts, engine_key, parse_inspect,
    };
    use officina_core::workshop::floor::CredentialKind;
    use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};
    use officina_core::workshop::names::{BatteryNames, ObjectName, RunId};
    use officina_core::workshop::vector::{self, ContainerRole, credential_name};

    use crate::registry::workshop::{BatteryKey, BatteryRecord, BatteryTotals};
    use crate::workshop::build::VERSION;
    use crate::workshop::engine::{
        EngineError, EngineOutcome, ResolvedDocker, create_with_credential, execute,
    };

    /// How often the decoy's and the probe's output is read while waiting.
    const POLL: Duration = Duration::from_millis(500);
    /// How many reads the decoy and its probe get before they count as
    /// failed.
    const POLLS: u32 = 40;
    /// The file the battery mount holds, so the mount is not empty.
    const MOUNT_NOTE: &str = "officina-battery.txt";

    /// The engine a battery runs on.
    #[derive(Debug, Clone)]
    pub struct BatteryEngine {
        pub docker: ResolvedDocker,
        pub base_env: Vec<(String, String)>,
        pub home: PathBuf,
    }

    /// One battery to run: the image, as its build recorded it, and what
    /// the floor gives a worker.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BatteryJob {
        pub id: RunId,
        pub image: String,
        pub spec: BatterySpec,
        pub programs_sha256: String,
        pub gateway_image_id: String,
    }

    /// One refusal a gateway logged, for the journal (AD-120).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LoggedRefusal {
        pub host: String,
        pub port: u16,
        pub cause: GatewayRefusal,
    }

    /// What a battery gives the collector to record.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BatteryReport {
        pub record: BatteryRecord,
        pub attempts: Vec<AttemptVerdict>,
        pub refusals: Vec<LoggedRefusal>,
    }

    /// Why a battery stopped before its judgement.
    enum Abort {
        Stopped,
        Engine(String),
    }

    /// What the two phases left to judge.
    #[derive(Default)]
    struct Played {
        lines: Vec<BatteryLine>,
        refusals: Vec<GatewayRefusal>,
        logged: Vec<LoggedRefusal>,
        gateway_b_exit: Option<i32>,
    }

    struct Runner<'a> {
        engine: &'a BatteryEngine,
        job: &'a BatteryJob,
        names: BatteryNames,
    }

    fn outcome_text(outcome: &EngineOutcome) -> String {
        format!("exit {:?}: {}", outcome.code, outcome.stderr.trim())
    }

    impl Runner<'_> {
        fn call(&self, call: &EngineCall) -> Result<EngineOutcome, Abort> {
            execute(
                &self.engine.docker,
                call,
                &self.engine.base_env,
                &self.engine.home,
                None,
            )
            .map_err(|e: EngineError| Abort::Engine(e.to_string()))
        }

        /// A call that must succeed.
        fn must(&self, call: &EngineCall, what: &str) -> Result<EngineOutcome, Abort> {
            let outcome = self.call(call)?;
            if outcome.succeeded() {
                Ok(outcome)
            } else {
                Err(Abort::Engine(format!("{what}: {}", outcome_text(&outcome))))
            }
        }

        /// Refused while the workshop's stop flag is set (AD-116).
        fn not_stopped(&self) -> Result<(), Abort> {
            if crate::workshop::workshop_stopped(&self.engine.home) {
                Err(Abort::Stopped)
            } else {
                Ok(())
            }
        }

        fn labels(&self, role: ContainerRole) -> std::collections::BTreeMap<String, String> {
            self.job.spec.labels(&self.job.id, role)
        }

        fn create(&self, container: &vector::ContainerVector, what: &str) -> Result<(), Abort> {
            self.must(&EngineCall::create(container), what).map(|_| ())
        }

        fn start(&self, container: &ObjectName, what: &str) -> Result<(), Abort> {
            self.not_stopped()?;
            self.must(&EngineCall::start(container), what).map(|_| ())
        }

        fn logs(&self, container: &ObjectName) -> Result<String, Abort> {
            let outcome = self.call(&EngineCall::logs_once(container))?;
            Ok(String::from_utf8_lossy(&outcome.stdout).into_owned())
        }

        /// The battery's read-only mount, recreated per battery (AD-117).
        fn mount_source(&self) -> Result<PathBuf, Abort> {
            let mount = crate::workshop::battery_dir(&self.engine.home).join("mount");
            let _ = std::fs::remove_dir_all(&mount);
            std::fs::create_dir_all(&mount)
                .and_then(|()| {
                    std::fs::write(
                        mount.join(MOUNT_NOTE),
                        "The escape battery's mount: every write here must fail.\n",
                    )
                })
                .map_err(|e| Abort::Engine(format!("the battery mount: {e}")))?;
            Ok(mount)
        }

        fn networks_and_volumes(&self) -> Result<(), Abort> {
            let names = &self.names;
            self.must(
                &EngineCall::network_create(
                    &names.decoy_network,
                    false,
                    &self.labels(ContainerRole::Decoy),
                ),
                "the decoy's network",
            )?;
            self.must(
                &EngineCall::network_create(
                    &names.run.internal,
                    true,
                    &self.labels(ContainerRole::Battery),
                ),
                "the internal network",
            )?;
            self.must(
                &EngineCall::network_create(
                    &names.run.egress,
                    false,
                    &self.labels(ContainerRole::BatteryGateway),
                ),
                "the egress network",
            )?;
            self.must(
                &EngineCall::volume_create(&names.run.volume, &self.labels(ContainerRole::Battery)),
                "the work volume",
            )?;
            if self.job.spec.kind == CredentialKind::Login {
                self.must(
                    &EngineCall::volume_create(
                        &names.login_volume,
                        &self.labels(ContainerRole::Battery),
                    ),
                    "the battery's login volume",
                )?;
            }
            Ok(())
        }

        /// The decoy started and listening (AD-117): what it resolved,
        /// and whether its `listening` line came.
        fn decoy(&self) -> Result<(DecoyAddresses, bool), Abort> {
            let decoy = vector::decoy(
                &self.names.decoy,
                &self.labels(ContainerRole::Decoy),
                &self.job.spec.image_id,
                &self.names.decoy_network,
            );
            self.create(&decoy, "the decoy")?;
            self.start(&self.names.decoy, "the decoy")?;
            for _ in 0..POLLS {
                let lines = parse_decoy(&self.logs(&self.names.decoy)?);
                if lines
                    .iter()
                    .any(|l| matches!(l, DecoyLine::Listening { .. }))
                {
                    let addresses = lines
                        .into_iter()
                        .find_map(|l| match l {
                            DecoyLine::Addresses(a) => Some(a),
                            _ => None,
                        })
                        .unwrap_or_default();
                    return Ok((addresses, true));
                }
                std::thread::sleep(POLL);
            }
            Ok((DecoyAddresses::default(), false))
        }

        /// The host family's control (D-235): a second container on the
        /// decoy's network connects to it.
        fn decoy_probe(&self, decoy: &DecoyAddresses) -> Result<bool, Abort> {
            let Some(address) = decoy.own.first() else {
                return Ok(false);
            };
            let probe = vector::decoy_probe(
                &self.names.decoy_probe,
                &self.labels(ContainerRole::Decoy),
                &self.job.spec.image_id,
                &self.names.decoy_network,
                &format!("{address}:{DECOY_PORT}"),
            );
            self.create(&probe, "the decoy's probe")?;
            self.start(&self.names.decoy_probe, "the decoy's probe")?;
            for _ in 0..POLLS {
                let state = self.call(&EngineCall::inspect(&self.names.decoy_probe))?;
                if let Some(state) = parse_inspect(&String::from_utf8_lossy(&state.stdout))
                    && !state.running
                    && state.status != "created"
                {
                    return Ok(state.exit_code == Some(0));
                }
                std::thread::sleep(POLL);
            }
            Ok(false)
        }

        /// One phase (AD-117): its gateway on the egress network, joined
        /// to the internal one, then its battery worker run attached.
        fn phase(
            &self,
            phase: u8,
            mount: &Path,
            decoy: &DecoyAddresses,
            played: &mut Played,
        ) -> Result<(), Abort> {
            let (gateway, worker) = match phase {
                1 => (&self.names.run.gateway, &self.names.run.worker),
                _ => (&self.names.gateway_b, &self.names.worker_b),
            };
            let config = gateway_config(self.job.id.as_str(), phase);
            let gateway_vector = vector::battery_gateway(
                gateway,
                &self.labels(ContainerRole::BatteryGateway),
                &self.job.gateway_image_id,
                &self.names.run.egress,
                &config,
            );
            self.create(&gateway_vector, "a battery gateway")?;
            self.must(
                &EngineCall::network_connect(&self.names.run.internal, gateway),
                "a battery gateway's internal network",
            )?;
            self.start(gateway, "a battery gateway")?;
            let source = mount.to_string_lossy();
            let planned = self.job.spec.worker(&self.job.id, phase, &source, decoy);
            let create = EngineCall::create(&planned.vector);
            let created = match credential_name(self.job.spec.kind) {
                Some(name) => create_with_credential(
                    &self.engine.docker,
                    &create,
                    &self.engine.base_env,
                    &self.engine.home,
                    (name, DUMMY_CREDENTIAL),
                )
                .map_err(|e| Abort::Engine(e.to_string()))?,
                None => self.call(&create)?,
            };
            if !created.succeeded() {
                return Err(Abort::Engine(format!(
                    "a battery worker: {}",
                    outcome_text(&created)
                )));
            }
            self.not_stopped()?;
            let ran = self.call(&EngineCall::start_attached(
                worker,
                ContainerRole::Battery,
                phase_limit_ms(phase),
                false,
            ))?;
            played
                .lines
                .extend(battery_lines(&String::from_utf8_lossy(&ran.stdout)));
            if phase == 2 {
                let state = self.call(&EngineCall::inspect(gateway))?;
                let state = parse_inspect(&String::from_utf8_lossy(&state.stdout));
                played.gateway_b_exit = state
                    .filter(|s| !s.running)
                    .and_then(|s| s.exit_code)
                    .and_then(|c| i32::try_from(c).ok());
            }
            let _ = self.call(&EngineCall::stop(std::slice::from_ref(gateway)));
            let output = self.logs(gateway)?;
            played.refusals.extend(gateway_refusals(&output));
            played.logged.extend(output.lines().filter_map(|line| {
                match serde_json::from_str::<GatewayLine>(line.trim()).ok()? {
                    GatewayLine::Refused {
                        host, port, cause, ..
                    } => Some(LoggedRefusal { host, port, cause }),
                    _ => None,
                }
            }));
            Ok(())
        }

        fn play(&self) -> Result<Played, Abort> {
            let mount = self.mount_source()?;
            self.networks_and_volumes()?;
            let (decoy, listening) = self.decoy()?;
            let probed = listening && self.decoy_probe(&decoy)?;
            let mut played = Played::default();
            played.lines.push(BatteryLine::Control(ControlLine {
                family: officina_core::workshop::battery::BatteryFamily::Host,
                control: Control::DecoyListening,
                status: if probed {
                    ControlStatus::Ok
                } else {
                    ControlStatus::Failed
                },
            }));
            self.phase(1, &mount, &decoy, &mut played)?;
            self.phase(2, &mount, &decoy, &mut played)?;
            Ok(played)
        }

        /// Removes every object of the battery, whatever happened.
        fn remove(&self) {
            let names = &self.names;
            let containers: Vec<ObjectName> = names.containers().into_iter().cloned().collect();
            let _ = self.call(&EngineCall::rm(&containers));
            let networks: Vec<ObjectName> = names.networks().into_iter().cloned().collect();
            let _ = self.call(&EngineCall::network_rm(&networks));
            let mut volumes = vec![names.run.volume.clone()];
            if self.job.spec.kind == CredentialKind::Login {
                volumes.push(names.login_volume.clone());
            }
            let _ = self.call(&EngineCall::volume_rm(&volumes));
        }
    }

    fn totals(t: Totals) -> BatteryTotals {
        BatteryTotals {
            attempts: t.attempts,
            refused: t.refused,
            escaped: t.escaped,
            inconclusive: t.inconclusive,
            not_applicable: t.not_applicable,
        }
    }

    /// What the engine is, as a battery keys on it (D-232), and its clock
    /// against the window's (D-242).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EngineIdentity {
        pub key: String,
        pub docker_desktop: bool,
        pub clock_skew_ms: Option<i64>,
    }

    /// Reads `docker version` and `docker info` (D-205).
    pub fn engine_identity(
        engine: &BatteryEngine,
        now_ms: &dyn Fn() -> i64,
    ) -> Result<EngineIdentity, String> {
        let call = |call: &EngineCall, what: &str| {
            let outcome = execute(&engine.docker, call, &engine.base_env, &engine.home, None)
                .map_err(|e| format!("{what}: {e}"))?;
            if outcome.succeeded() {
                Ok(String::from_utf8_lossy(&outcome.stdout).into_owned())
            } else {
                Err(format!("{what}: {}", outcome_text(&outcome)))
            }
        };
        let info = EngineInfo::parse(&call(&EngineCall::version(), "docker version")?)
            .map_err(|why| format!("the engine: {}", why.code()))?;
        let before = now_ms();
        let facts = call(&EngineCall::info(), "docker info")?;
        let after = now_ms();
        let facts =
            InfoFacts::parse(&facts).ok_or_else(|| "docker info gave no object".to_owned())?;
        Ok(EngineIdentity {
            key: engine_key(&info, &facts),
            docker_desktop: info.platform.contains("Docker Desktop"),
            clock_skew_ms: clock_skew_ms(&facts.system_time, before, after),
        })
    }

    /// Runs one battery (AD-117, D-205, D-206, D-235, D-242): the engine
    /// read, the decoy and its probe, phase 1 then phase 2, every object
    /// removed afterwards, the lines judged by core. Nothing starts while
    /// the workshop's stop flag is set.
    pub fn run(
        engine: &BatteryEngine,
        job: &BatteryJob,
        now_ms: &dyn Fn() -> i64,
    ) -> BatteryReport {
        let started_at_ms = now_ms();
        let runner = Runner {
            engine,
            job,
            names: BatteryNames::of(&job.id),
        };
        let mut key = BatteryKey {
            image_id: job.spec.image_id.clone(),
            gateway_image_id: job.gateway_image_id.clone(),
            engine_key: String::new(),
            officina_version: VERSION.to_owned(),
            vector_sha256: job.spec.vector_sha256(),
            programs_sha256: job.programs_sha256.clone(),
        };
        let mut skew = None;
        let mut verdicts = Vec::new();
        let mut logged = Vec::new();
        let ended = |abort: Abort| match abort {
            Abort::Stopped => (
                BatteryOutcome::Stopped,
                Totals::default(),
                Some("the workshop is stopped".to_owned()),
            ),
            Abort::Engine(why) => (BatteryOutcome::Error, Totals::default(), Some(why)),
        };
        let identity = runner
            .not_stopped()
            .and_then(|()| engine_identity(engine, now_ms).map_err(Abort::Engine));
        let (outcome, counts, cause) = match identity {
            Err(abort) => ended(abort),
            Ok(identity) => {
                key.engine_key = identity.key;
                skew = identity.clock_skew_ms;
                let played = runner.play();
                runner.remove();
                match played {
                    Err(abort) => ended(abort),
                    Ok(played) => {
                        let judgement = judge(&JudgeInput {
                            kind: job.spec.kind,
                            lines: played.lines,
                            refusals: played.refusals,
                            gateway_b_exit: played.gateway_b_exit,
                            docker_desktop: identity.docker_desktop,
                        });
                        verdicts = judgement.attempts;
                        logged = played.logged;
                        let cause =
                            (!judgement.failures.is_empty()).then(|| judgement.failures.join("; "));
                        (judgement.outcome, judgement.totals, cause)
                    }
                }
            }
        };
        BatteryReport {
            record: BatteryRecord {
                id: job.id.as_str().to_owned(),
                image: job.image.clone(),
                key,
                clock_skew_ms: skew,
                started_at_ms,
                finished_at_ms: Some(now_ms()),
                outcome: Some(outcome),
                totals: totals(counts),
                cause,
            },
            attempts: verdicts,
            refusals: logged,
        }
    }

    /// The validity key a battery of `job` would record on an engine whose
    /// key is `engine_key`, for the stale test.
    pub fn validity_key(job: &BatteryJob, engine_key: &str) -> ValidityKey {
        ValidityKey {
            engine_key: engine_key.to_owned(),
            image_id: job.spec.image_id.clone(),
            gateway_image_id: job.gateway_image_id.clone(),
            officina_version: VERSION.to_owned(),
            vector_sha256: job.spec.vector_sha256(),
            programs_sha256: job.programs_sha256.clone(),
        }
    }

    /// The journal lines of a battery (AD-120): one `gateway_refused` per
    /// refusal the gateways logged, one `escape_attempt` per attempt, then
    /// one `escape_battery`.
    pub fn journal_lines(report: &BatteryReport) -> Vec<JournalEntry> {
        let record = &report.record;
        let mut lines: Vec<JournalEntry> = report
            .refusals
            .iter()
            .map(|r| {
                JournalEntry::GatewayRefused(GatewayRefused {
                    run_id: None,
                    battery_id: Some(record.id.clone()),
                    host: r.host.clone(),
                    port: r.port,
                    cause: r.cause,
                })
            })
            .collect();
        lines.extend(report.attempts.iter().map(|a| {
            JournalEntry::EscapeAttempt(EscapeAttempt {
                battery_id: record.id.clone(),
                image: record.image.clone(),
                family: a.family,
                attempt: a.attempt.clone(),
                outcome: a.outcome,
                reason: a.reason.clone(),
            })
        }));
        lines.push(JournalEntry::EscapeBattery(EscapeBattery {
            battery_id: record.id.clone(),
            image: record.image.clone(),
            image_id: record.key.image_id.clone(),
            gateway_image_id: record.key.gateway_image_id.clone(),
            engine_key: record.key.engine_key.clone(),
            officina_version: record.key.officina_version.clone(),
            totals: Totals {
                attempts: record.totals.attempts,
                refused: record.totals.refused,
                escaped: record.totals.escaped,
                inconclusive: record.totals.inconclusive,
                not_applicable: record.totals.not_applicable,
            },
            outcome: record.outcome.unwrap_or(BatteryOutcome::Error),
            cause: record.cause.clone(),
        }));
        lines
    }

    /// One image a battery may run on, as its build recorded it, and its
    /// newest batteries.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BatteryCandidate {
        pub image: String,
        pub image_id: String,
        pub programs_sha256: String,
        pub image_env: Vec<String>,
        pub batteries: Vec<BatteryRecord>,
    }

    /// What the collector granted (AD-117): the floor's worker, the
    /// images built, the gateway image, and the one image asked for.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BatteryPlan {
        pub home: PathBuf,
        pub base_env: Vec<(String, String)>,
        pub floor_key: String,
        pub kind: CredentialKind,
        pub cpus: u32,
        pub memory_mib: u32,
        pub gateway_image_id: String,
        pub candidates: Vec<BatteryCandidate>,
        pub only: Option<String>,
    }

    /// Grants a Run escape battery click on the collector thread
    /// (AD-117): the stop flag, the approved catalogue equal to the floor
    /// read now, the gateway image and the image asked for built.
    pub fn grant(
        relay: &crate::relay::home::RelayHome,
        home: &Path,
        registry: &mut crate::registry::window::WindowRegistry,
        only: Option<&str>,
        base_env: &[(String, String)],
    ) -> Result<BatteryPlan, BatteryRefusal> {
        use officina_core::relay::template::TemplateUnavailable;
        use officina_core::workshop::image::{BuildOutcome, ImageKind};
        if crate::workshop::workshop_stopped(home) {
            return Err(BatteryRefusal::WorkshopStopped);
        }
        let (approved, current) = crate::orders::catalogue(relay).map_err(|why| match why {
            TemplateUnavailable::NotApproved => BatteryRefusal::NotApproved,
            TemplateUnavailable::CatalogueChanged | TemplateUnavailable::WorkshopOnly => {
                BatteryRefusal::CatalogueChanged
            }
        })?;
        if approved.hash != current.hash {
            return Err(BatteryRefusal::CatalogueChanged);
        }
        let workshop = current
            .canonical
            .workshop
            .as_ref()
            .ok_or(BatteryRefusal::WorkshopOff)?;
        let inputs = registry
            .workshop_battery_inputs()
            .map_err(|_| BatteryRefusal::ImageMissing)?;
        let built = |image: &crate::registry::workshop::ImageRecord| {
            image.outcome == BuildOutcome::Built && image.image_id.is_some()
        };
        let gateway_image_id = inputs
            .iter()
            .find(|(image, _)| image.kind == ImageKind::Gateway && built(image))
            .and_then(|(image, _)| image.image_id.clone())
            .ok_or(BatteryRefusal::ImageMissing)?;
        let candidates: Vec<BatteryCandidate> = inputs
            .into_iter()
            .filter(|(image, _)| image.kind != ImageKind::Gateway && built(image))
            .filter_map(|(image, batteries)| {
                Some(BatteryCandidate {
                    image_env: image
                        .env_names_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str(json).ok())
                        .unwrap_or_default(),
                    image_id: image.image_id?,
                    programs_sha256: image.programs_sha256?,
                    image: image.name,
                    batteries,
                })
            })
            .collect();
        if let Some(name) = only
            && !candidates.iter().any(|c| c.image == name)
        {
            return Err(BatteryRefusal::ImageMissing);
        }
        Ok(BatteryPlan {
            home: home.to_path_buf(),
            base_env: base_env.to_vec(),
            floor_key: officina_core::workshop::names::floor_key(&current.floor_path),
            kind: workshop.auth,
            cpus: workshop.cpus,
            memory_mib: workshop.memory_mib,
            gateway_image_id,
            candidates,
            only: only.map(str::to_owned),
        })
    }

    /// A battery id (D-215): 32 lowercase hex, drawn as card ids are.
    pub fn new_battery_id(now_ms: i64, counter: u64) -> Option<RunId> {
        use std::hash::{BuildHasher, Hasher};
        let half = || {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_i64(now_ms);
            hasher.write_u64(counter);
            hasher.finish()
        };
        RunId::parse(&format!("{:016x}{:016x}", half(), half()))
    }

    /// The batteries `plan` runs on an engine whose key is `engine_key`
    /// at `now_ms`: the image asked for, or every image whose battery is
    /// stale (AD-117, D-232).
    pub fn jobs(plan: &BatteryPlan, engine_key: &str, now_ms: i64) -> Vec<BatteryJob> {
        plan.candidates
            .iter()
            .enumerate()
            .filter_map(|(i, candidate)| {
                let job = BatteryJob {
                    id: new_battery_id(now_ms, u64::try_from(i).unwrap_or(0))?,
                    image: candidate.image.clone(),
                    spec: BatterySpec {
                        kind: plan.kind,
                        cpus: plan.cpus,
                        memory_mib: plan.memory_mib,
                        floor_key: plan.floor_key.clone(),
                        version: VERSION.to_owned(),
                        image_id: candidate.image_id.clone(),
                        image_env: candidate.image_env.clone(),
                    },
                    programs_sha256: candidate.programs_sha256.clone(),
                    gateway_image_id: plan.gateway_image_id.clone(),
                };
                let wanted = match &plan.only {
                    Some(name) => *name == candidate.image,
                    None => super::valid_battery(
                        &candidate.batteries,
                        &validity_key(&job, engine_key),
                        now_ms,
                    )
                    .is_none(),
                };
                wanted.then_some(job)
            })
            .collect()
    }

    /// What a Run escape battery click runs with.
    pub struct BatteryContext {
        pub docker: Result<ResolvedDocker, crate::workshop::engine::ResolveError>,
        /// Thread `workshop-build`'s busy flag, which Build images shares.
        pub busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
        pub now_ms: fn() -> i64,
    }

    /// Clears the busy flag when the job ends, however it ends.
    struct BusyGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl Drop for BusyGuard {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// The `run_battery` command (AD-117, D-249; the pointer click is the
    /// window's): one job at a time on thread `workshop-build`; the
    /// collector grants it; the thread reads the engine and picks the
    /// batteries, and the answer waits for that, so a refusal names its
    /// cause; then each battery runs there, its report sent to the
    /// collector as [`AuthorCommand::BatteryRecorded`].
    pub fn run_battery(
        sender: &std::sync::mpsc::Sender<crate::watch::CollectorMsg>,
        thread: &crate::workshop::threads::JobThread,
        context: BatteryContext,
        only: Option<String>,
        on_done: Box<dyn FnOnce() + Send>,
    ) -> Result<BatteryAnswer, officina_core::ipc::IpcError> {
        use crate::watch::{AUTHOR_WAIT, AuthorCommand, AuthorRequest, CollectorMsg, ask};
        use std::sync::atomic::Ordering;
        if context.busy.swap(true, Ordering::SeqCst) {
            return Ok(BatteryAnswer::Refused(BatteryRefusal::Busy));
        }
        let guard = BusyGuard(std::sync::Arc::clone(&context.busy));
        let plan = match ask(sender, AuthorCommand::GrantBattery(only), AUTHOR_WAIT)?
            .into_battery_grant()?
        {
            Ok(plan) => plan,
            Err(refusal) => return Ok(BatteryAnswer::Refused(refusal)),
        };
        let Ok(docker) = context.docker else {
            return Ok(BatteryAnswer::Refused(BatteryRefusal::EngineUnavailable));
        };
        let (picked_tx, picked_rx) = std::sync::mpsc::channel();
        let collector = sender.clone();
        let now_ms = context.now_ms;
        let submitted = thread.submit(move || {
            let _guard = guard;
            let engine = BatteryEngine {
                docker,
                base_env: plan.base_env.clone(),
                home: plan.home.clone(),
            };
            let picked = engine_identity(&engine, &now_ms)
                .map_err(|_| BatteryRefusal::EngineUnavailable)
                .map(|identity| jobs(&plan, &identity.key, now_ms()))
                .and_then(|jobs| {
                    if jobs.is_empty() {
                        Err(BatteryRefusal::NothingStale)
                    } else {
                        Ok(jobs)
                    }
                });
            let _ = picked_tx.send(
                picked
                    .as_ref()
                    .map(|jobs| jobs.iter().map(|j| j.image.clone()).collect::<Vec<_>>())
                    .map_err(|r| *r),
            );
            for job in picked.unwrap_or_default() {
                let report = run(&engine, &job, &now_ms);
                let (request, _answer) = AuthorRequest::new(
                    AuthorCommand::BatteryRecorded(Box::new(report)),
                    AUTHOR_WAIT,
                );
                let _ = collector.send(CollectorMsg::Author(request));
            }
            on_done();
        });
        if !submitted {
            return Err(officina_core::ipc::IpcError::internal(
                "thread workshop-build has stopped",
            ));
        }
        Ok(match picked_rx.recv() {
            Ok(Ok(images)) => BatteryAnswer::Queued(images),
            Ok(Err(refusal)) => BatteryAnswer::Refused(refusal),
            Err(_) => BatteryAnswer::Refused(BatteryRefusal::EngineUnavailable),
        })
    }

    /// Records one battery on the collector thread (AD-38): its row, then
    /// its journal lines (AD-120).
    pub fn record(
        registry: &mut crate::registry::window::WindowRegistry,
        relay: &crate::relay::home::RelayHome,
        report: &BatteryReport,
        now_ms: i64,
    ) -> Result<(), officina_core::ipc::IpcError> {
        registry.record_workshop_battery(&report.record)?;
        let _ = crate::relay::journal::record(relay, &journal_lines(report), now_ms);
        Ok(())
    }
}
