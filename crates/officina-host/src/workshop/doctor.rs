//! Doctor's workshop section on the host (AD-123, D-228): the facts gathered
//! with the engine's read half (doctor starts no container), turned into
//! lines by core's [`WorkshopCheck`] rules.

use std::path::{Path, PathBuf};

use officina_core::workshop::doctor::{
    CaRead, CheckLevel, Finding, HookFacts, ImageInput, WorkshopCheck, ca_findings,
    claude_version_finding, docker_cli_config, hook_findings, image_finding, in_order,
    memory_finding, orphans, overrides_set, plain_tcp_docker_host,
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
use crate::workshop::proxy::{ProxyError, SystemProxyReader, WinHttpReader, resolve};
use officina_core::workshop::floor::UpstreamProxy;
use officina_core::workshop::gateway::CLAUDE_CODE_HOSTS;
use officina_core::workshop::proxy::{Route, Routes};
use std::sync::Arc;

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
    /// The routes on this network, when the floor's workshop is valid.
    pub proxy: Option<ProxyFacts>,
    /// The Linux programs and the images, when the floor's workshop is
    /// valid (AD-123, story 6.8).
    pub images: Option<ImageReport>,
    /// Each workshop project's hooks against its recorded inventory
    /// (AD-123, story 6.9).
    pub hooks: Vec<HookFacts>,
    /// Each image's batteries (AD-123, story 6.10).
    pub batteries: Vec<officina_core::workshop::doctor::BatteryFacts>,
    /// When doctor read them, for the batteries' ages.
    pub now_ms: i64,
}

/// What doctor needs to compare the images (AD-99, AD-123).
#[derive(Debug, Clone)]
pub struct ImageContext {
    /// The executable's folder, where `linux/` sits (AD-100).
    pub exe_dir: PathBuf,
    pub canonical: officina_core::relay::catalogue::CanonicalCatalogue,
    pub floor_key: String,
    /// Each project's folder as the floor names it, for IO: the
    /// canonical path is case folded for comparison.
    pub project_dirs: std::collections::BTreeMap<String, PathBuf>,
    /// The host's Claude Code version, `x.y.z`.
    pub claude_version: Option<String>,
}

/// The programs, images, Claude Code versions, CA and memory doctor read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReport {
    /// The Linux programs' folder and digests, or why they are missing.
    pub programs: Result<(PathBuf, crate::workshop::build::ProgramDigests), String>,
    pub images: Vec<ImageInput>,
    pub host_claude: Option<String>,
    pub base_claude: Option<String>,
    pub ca: CaRead,
    pub ca_approved: Option<String>,
    pub base_ca_label: Option<String>,
    /// `max_workers` and `memory_mib`.
    pub workers: (u32, u32),
}

/// What doctor reads of each workshop project's hooks (AD-123, story 6.9
/// AC 5): the inventory recorded for the image the project runs on, the
/// names each mounted settings file exposes, and the mounted hook scripts
/// with CRLF line endings. Doctor runs no container and no `git`: tracked
/// files are the inventory's to read at a launch's base commit.
pub fn hook_facts(context: &ImageContext, home: &Path) -> Vec<HookFacts> {
    use officina_core::workshop::image::WORKSHOP_BASE;
    use officina_core::workshop::inventory::StoredInventory;
    use officina_core::workshop::names::project_image;
    let reader = crate::registry::store::open_reader(home, &[]).ok();
    let mut all = Vec::new();
    for (name, project) in &context.canonical.projects {
        let Some(workshop) = project.workshop.as_ref() else {
            continue;
        };
        let image = if workshop.dockerfile.is_some() {
            project_image(&context.floor_key, name)
        } else {
            WORKSHOP_BASE.to_owned()
        };
        let stored = reader
            .as_ref()
            .and_then(|r| r.workshop_inventory(&image, name).ok().flatten())
            .and_then(|record| {
                serde_json::from_str::<StoredInventory>(&record.inventory_json).ok()
            });
        let Some(dir) = context.project_dirs.get(name) else {
            continue;
        };
        all.push(project_hooks(name, dir, &workshop.mounts, stored));
    }
    all
}

/// One project's [`HookFacts`] from its folder: the mounted settings files
/// read, bounded, for the names they expose and the scripts their hooks
/// name under a mount, each checked for CRLF line endings.
pub fn project_hooks(
    name: &str,
    dir: &Path,
    mounts: &[String],
    stored: Option<officina_core::workshop::inventory::StoredInventory>,
) -> HookFacts {
    use crate::workshop::inventory::SETTINGS_FILES;
    use officina_core::workshop::inventory::{
        MAX_INPUT_BYTES, SettingsFile, exposed_names, extract, has_crlf, mounted_scripts,
        under_mount,
    };
    let read = |path: &Path| {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        let mut bytes = Vec::new();
        let limit = u64::try_from(MAX_INPUT_BYTES).ok()?;
        file.take(limit).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    };
    let mut exposed = Vec::new();
    let mut mounted = Vec::new();
    for (path, source, _) in SETTINGS_FILES {
        if !under_mount(path, mounts) {
            continue;
        }
        if let Some(bytes) = read(&dir.join(path)) {
            exposed.push(((*path).to_owned(), exposed_names(&bytes)));
            mounted.push(SettingsFile {
                source: *source,
                bytes,
            });
        }
    }
    let commands = extract(&mounted, None, mounts).unwrap_or_default();
    let crlf = mounted_scripts(&commands, mounts)
        .into_iter()
        .filter(|script| read(&dir.join(script)).is_some_and(|bytes| has_crlf(&bytes)))
        .collect();
    HookFacts {
        project: name.to_owned(),
        stored,
        exposed,
        crlf,
    }
}

fn digest_of(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| officina_core::sha256_hex(&bytes))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads what [`ImageReport`] holds: the programs found as Build images
/// finds them, each image's facts with `image inspect` (a read call) when
/// the engine answers, and its registry row.
pub fn image_report(
    context: &ImageContext,
    env: &[(String, String)],
    home: &Path,
    docker: Option<&ResolvedDocker>,
) -> Option<ImageReport> {
    use officina_core::workshop::engine::parse_image_inspect;
    use officina_core::workshop::image::{
        GATEWAY_IMAGE, ImageKind, LABEL_CA, LABEL_CLAUDE_VERSION, ProjectLabels, Recorded,
        WORKSHOP_BASE,
    };
    use officina_core::workshop::names::{ObjectName, project_image};
    let workshop = context.canonical.workshop.as_ref()?;
    let linux_override = env_value(env, ENV_LINUX_BIN_DIR).map(std::ffi::OsString::from);
    let programs =
        crate::workshop::build::find_programs(Some(&context.exe_dir), linux_override.as_deref())
            .map_err(|refusal| refusal.code().to_owned())
            .and_then(|found| {
                Ok((
                    found.dir.clone(),
                    crate::workshop::build::ProgramDigests {
                        entry: digest_of(&found.entry)?,
                        gateway: digest_of(&found.gateway)?,
                    },
                ))
            });
    let reader = crate::registry::store::open_reader(home, &[]).ok();
    let row = |tag: &str| {
        reader
            .as_ref()
            .and_then(|r| r.workshop_image(tag).ok().flatten())
    };
    let inspect = |tag: &str| -> Option<officina_core::workshop::engine::ImageFacts> {
        let docker = docker?;
        let name = ObjectName::parse(tag)?;
        read_call(docker, &EngineCall::image_inspect(&name), env, home)
            .and_then(|out| parse_image_inspect(&out))
    };
    let recorded = |tag: &str| {
        row(tag).map(|r| Recorded {
            image_id: r.image_id,
            programs_sha256: r.programs_sha256,
            built: r.outcome == officina_core::workshop::image::BuildOutcome::Built,
        })
    };
    let base = inspect(WORKSHOP_BASE);
    let base_programs = row(WORKSHOP_BASE).and_then(|r| r.programs_sha256);
    let mut images = Vec::new();
    if let Ok((_, digests)) = &programs {
        let ca = workshop.ca_sha256.as_deref();
        for (tag, kind) in [
            (GATEWAY_IMAGE.to_owned(), ImageKind::Gateway),
            (WORKSHOP_BASE.to_owned(), ImageKind::Workshop),
        ] {
            images.push(ImageInput {
                expected: crate::workshop::build::expected(kind, digests, ca, None, None),
                inspected: inspect(&tag),
                recorded: recorded(&tag),
                tag,
            });
        }
        for (name, project) in &context.canonical.projects {
            let Some(w) = project.workshop.as_ref().filter(|w| w.dockerfile.is_some()) else {
                continue;
            };
            let tag = project_image(&context.floor_key, name);
            let labels = ProjectLabels {
                floor_key: context.floor_key.clone(),
                project: name.clone(),
                dockerfile_sha256: w.dockerfile_sha256.clone().unwrap_or_default(),
                context_sha256: w.context_sha256.clone().unwrap_or_default(),
                base_id: base.as_ref().map(|b| b.id.clone()).unwrap_or_default(),
            };
            images.push(ImageInput {
                expected: crate::workshop::build::expected(
                    ImageKind::Project,
                    digests,
                    ca,
                    Some(labels),
                    base_programs.as_deref(),
                ),
                inspected: inspect(&tag),
                recorded: recorded(&tag),
                tag,
            });
        }
    }
    let ca = match &workshop.ca_file {
        None => CaRead::None,
        Some(path) => match digest_of(Path::new(path)) {
            Ok(sha) => CaRead::Sha256(sha),
            Err(why) => CaRead::Unreadable(why),
        },
    };
    Some(ImageReport {
        programs,
        images,
        host_claude: context.claude_version.clone(),
        base_claude: base
            .as_ref()
            .and_then(|b| b.labels.get(LABEL_CLAUDE_VERSION).cloned()),
        ca,
        ca_approved: workshop.ca_sha256.clone(),
        base_ca_label: base.as_ref().and_then(|b| b.labels.get(LABEL_CA).cloned()),
        workers: (workshop.max_workers, workshop.memory_mib),
    })
}

fn image_findings(report: &ImageReport, info: Option<&InfoFacts>) -> Vec<Finding> {
    let mut found = vec![match &report.programs {
        Ok((dir, _)) => Finding::new(WorkshopCheck::LinuxPrograms, dir.display().to_string()),
        Err(why) => Finding::new(
            WorkshopCheck::LinuxProgramsMissing,
            format!(
                "{why}: officina-gateway and officina-entry belong in linux/ beside officina.exe, or in {ENV_LINUX_BIN_DIR}"
            ),
        ),
    }];
    found.extend(report.images.iter().map(image_finding));
    found.extend(claude_version_finding(
        report.host_claude.as_deref(),
        report.base_claude.as_deref(),
    ));
    found.extend(ca_findings(
        &report.ca,
        report.ca_approved.as_deref(),
        report.base_ca_label.as_deref(),
    ));
    if let Some(info) = info {
        found.extend(memory_finding(
            report.workers.0,
            report.workers.1,
            info.memory_bytes,
        ));
    }
    found
}

/// What doctor needs of the floor to resolve the routes (D-611).
#[derive(Debug, Clone)]
pub struct ProxyInput {
    pub setting: UpstreamProxy,
    /// Every entry any workshop project's worker may reach.
    pub allow: Vec<String>,
    pub ca_file: bool,
}

/// The routes doctor resolved, and what it knows of the resolution.
#[derive(Debug, Clone)]
pub struct ProxyFacts {
    pub setting: String,
    pub routes: Result<Routes, ProxyError>,
    /// The reader evaluates PAC files and automatic detection.
    pub evaluates_pac: bool,
    /// Windows names a PAC URL or turns automatic detection on.
    pub pac_or_auto_detect: bool,
    pub ca_file: bool,
}

/// What doctor resolves for a valid `[workshop]` (D-611): the setting,
/// every entry any workshop project's worker may reach, and whether a CA
/// file is set; `None` while the workshop is off.
pub fn proxy_input(
    canonical: &officina_core::relay::catalogue::CanonicalCatalogue,
) -> Option<ProxyInput> {
    let workshop = canonical.workshop.as_ref()?;
    let setting = UpstreamProxy::parse(&workshop.upstream_proxy).ok()?;
    let mut allow = officina_core::workshop::launch::allow_list(workshop.auth, &[]);
    for project in canonical.projects.values() {
        if let Some(project) = &project.workshop {
            for host in officina_core::workshop::launch::allow_list(workshop.auth, &project.hosts) {
                if !allow.contains(&host) {
                    allow.push(host);
                }
            }
        }
    }
    Some(ProxyInput {
        setting,
        allow,
        ca_file: workshop.ca_file.is_some(),
    })
}

/// Resolves `input` with `reader`, for doctor.
pub fn proxy_facts(input: &ProxyInput, reader: Arc<dyn SystemProxyReader>) -> ProxyFacts {
    let pac_or_auto_detect = input.setting == UpstreamProxy::System
        && reader
            .config()
            .is_ok_and(|c| c.auto_detect || c.pac_url.is_some());
    ProxyFacts {
        setting: input.setting.setting().to_owned(),
        evaluates_pac: reader.evaluates_pac(),
        routes: resolve(&input.setting, &input.allow, reader),
        pac_or_auto_detect,
        ca_file: input.ca_file,
    }
}

fn proxy_findings(facts: &ProxyFacts) -> Vec<Finding> {
    let mut found = Vec::new();
    match &facts.routes {
        Ok(routes) => {
            let each: Vec<String> = routes
                .routes
                .iter()
                .map(|(entry, route)| match route {
                    Route::Direct => format!("{entry} direct"),
                    Route::Proxy(proxy) => format!("{entry} via {proxy}"),
                })
                .collect();
            found.push(Finding::new(
                WorkshopCheck::ProxyRoutes,
                format!(
                    "upstream_proxy = \"{}\", resolved from {}: {}",
                    facts.setting,
                    routes.source.code(),
                    each.join(", ")
                ),
            ));
            let api_proxied = CLAUDE_CODE_HOSTS
                .iter()
                .any(|host| matches!(routes.routes.get(*host), Some(Route::Proxy(_))));
            if api_proxied && !facts.ca_file {
                found.push(Finding::new(
                    WorkshopCheck::ProxyCaNote,
                    "Claude Code's host goes through a proxy and no ca_file is set: if this proxy inspects TLS, export its root with the README's one-liner",
                ));
            }
        }
        Err(ProxyError::Refused(refusal)) => found.push(Finding::new(
            WorkshopCheck::ProxyRefused,
            format!("{} for {}", refusal.cause.code(), refusal.entry),
        )),
        Err(ProxyError::Unreadable(why)) => found.push(Finding::new(
            WorkshopCheck::ProxyRefused,
            format!("proxy_unresolved: {why}"),
        )),
    }
    if facts.pac_or_auto_detect && !facts.evaluates_pac {
        found.push(Finding::new(
            WorkshopCheck::ProxyNotEvaluated,
            "Windows names a PAC file or automatic detection, which this reader cannot evaluate: a PAC URL refuses launches, automatic detection is taken as finding none",
        ));
    }
    found
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
pub fn gather(
    env: &[(String, String)],
    home: &Path,
    proxy: Option<&ProxyInput>,
    images: Option<&ImageContext>,
) -> WorkshopFacts {
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
        proxy: proxy.map(|input| proxy_facts(input, Arc::new(WinHttpReader))),
        images: None,
        hooks: images
            .map(|context| hook_facts(context, home))
            .unwrap_or_default(),
        batteries: Vec::new(),
        now_ms: crate::files::now_ms(),
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
        let ready = state.info().is_some();
        let key = state
            .info()
            .zip(facts.info.as_ref())
            .map(|(info, read)| engine_key(info, read));
        if let Some(context) = images {
            facts.batteries = battery_facts(context, home, key.as_deref());
        }
        facts.engine = Some(state);
        facts.images =
            images.and_then(|context| image_report(context, env, home, ready.then_some(&docker)));
    } else {
        facts.images = images.and_then(|context| image_report(context, env, home, None));
        if let Some(context) = images {
            facts.batteries = battery_facts(context, home, None);
        }
    }
    facts
}

/// Each built image's batteries from the registry, and, when the engine's
/// key is known, what a battery of it would key on now (AD-117, D-232).
pub fn battery_facts(
    context: &ImageContext,
    home: &Path,
    engine_key: Option<&str>,
) -> Vec<officina_core::workshop::doctor::BatteryFacts> {
    use officina_core::workshop::battery::{BatterySpec, ValidityKey};
    use officina_core::workshop::image::{BuildOutcome, ImageKind};
    let Some(workshop) = context.canonical.workshop.as_ref() else {
        return Vec::new();
    };
    let Ok(reader) = crate::registry::store::open_reader(home, &[]) else {
        return Vec::new();
    };
    let rows = reader.workshop_images().unwrap_or_default();
    let built = |row: &&crate::registry::workshop::ImageRecord| {
        row.outcome == BuildOutcome::Built && row.image_id.is_some()
    };
    let gateway_id = rows
        .iter()
        .filter(built)
        .find(|row| row.kind == ImageKind::Gateway)
        .and_then(|row| row.image_id.clone());
    rows.iter()
        .filter(built)
        .filter(|row| row.kind != ImageKind::Gateway)
        .map(|row| {
            let records = reader.workshop_batteries(&row.name).unwrap_or_default();
            let image_id = row.image_id.clone().unwrap_or_default();
            let current = engine_key
                .zip(gateway_id.as_ref())
                .zip(row.programs_sha256.as_ref())
                .map(|((engine_key, gateway_id), programs)| {
                    let spec = BatterySpec {
                        kind: workshop.auth,
                        cpus: workshop.cpus,
                        memory_mib: workshop.memory_mib,
                        floor_key: context.floor_key.clone(),
                        version: crate::workshop::build::VERSION.to_owned(),
                        image_id: image_id.clone(),
                        image_env: row
                            .env_names_json
                            .as_deref()
                            .and_then(|json| serde_json::from_str(json).ok())
                            .unwrap_or_default(),
                    };
                    ValidityKey {
                        engine_key: engine_key.to_owned(),
                        image_id: image_id.clone(),
                        gateway_image_id: gateway_id.clone(),
                        officina_version: crate::workshop::build::VERSION.to_owned(),
                        vector_sha256: spec.vector_sha256(),
                        programs_sha256: programs.clone(),
                    }
                });
            officina_core::workshop::doctor::BatteryFacts {
                image: row.name.clone(),
                current,
                newest_skew_ms: records.first().and_then(|r| r.clock_skew_ms),
                runs: crate::workshop::battery::runs_of(&records),
            }
        })
        .collect()
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
    if let Some(proxy) = &facts.proxy {
        found.extend(proxy_findings(proxy));
    }
    if let Some(report) = &facts.images {
        found.extend(image_findings(report, facts.info.as_ref()));
    }
    for hooks in &facts.hooks {
        found.extend(hook_findings(hooks));
    }
    found.extend(officina_core::workshop::doctor::battery_findings(
        &facts.batteries,
        facts.now_ms,
    ));
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
            proxy: None,
            images: None,
            hooks: Vec::new(),
            batteries: Vec::new(),
            now_ms: 1_800_000_000_000,
        }
    }

    fn report() -> ImageReport {
        use officina_core::workshop::image::{
            Expected, ImageKind, LABEL_CA, LABEL_CLAUDE_VERSION, Recorded,
        };
        let digests = crate::workshop::build::ProgramDigests {
            entry: "e".repeat(64),
            gateway: "g".repeat(64),
        };
        let expected: Expected =
            crate::workshop::build::expected(ImageKind::Workshop, &digests, None, None, None);
        let mut labels = expected.labels.clone();
        labels.insert(LABEL_CLAUDE_VERSION.into(), "2.1.316".into());
        ImageReport {
            programs: Ok((PathBuf::from("C:/Officina/linux"), digests)),
            images: vec![ImageInput {
                tag: "officina-workshop:local".into(),
                expected,
                inspected: Some(officina_core::workshop::engine::ImageFacts {
                    id: "sha256:aa".into(),
                    labels: labels.clone(),
                    env_names: vec!["PATH".into()],
                }),
                recorded: Some(Recorded {
                    image_id: Some("sha256:aa".into()),
                    programs_sha256: Some("p".repeat(64)),
                    built: true,
                }),
            }],
            host_claude: Some("2.1.316".into()),
            base_claude: Some("2.1.316".into()),
            ca: CaRead::None,
            ca_approved: None,
            base_ca_label: labels.get(LABEL_CA).cloned(),
            workers: (2, 4096),
        }
    }

    /// A project folder with `.claude` mounted, `settings` as its
    /// `settings.json` and `files` beside it.
    fn hook_project(settings: &str, files: &[(&str, &[u8])]) -> (tempfile::TempDir, HookFacts) {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(claude.join("hooks")).unwrap();
        std::fs::write(claude.join("settings.json"), settings).unwrap();
        for (path, bytes) in files {
            std::fs::write(dir.path().join(path), bytes).unwrap();
        }
        let facts = project_hooks("api", dir.path(), &[".claude".to_owned()], None);
        (dir, facts)
    }

    fn hook_lines(hooks: HookFacts) -> Vec<Check> {
        let mut facts = facts();
        facts.hooks = vec![hooks];
        checks(&facts)
    }

    fn line_of(lines: &[Check], check_kind: WorkshopCheck) -> Option<&Check> {
        lines
            .iter()
            .find(|line| line.title == check_kind.title() && line.level == level_of(check_kind))
    }

    fn level_of(check_kind: WorkshopCheck) -> Level {
        match check_kind.level() {
            CheckLevel::Fail => Level::Fail,
            CheckLevel::Warn => Level::Warn,
            CheckLevel::Inform => Level::Ok,
        }
    }

    fn battery_key() -> officina_core::workshop::battery::ValidityKey {
        officina_core::workshop::battery::ValidityKey {
            engine_key: "Docker Desktop|29.8.2|6.18|digest".into(),
            image_id: format!("sha256:{}", "a".repeat(64)),
            gateway_image_id: format!("sha256:{}", "b".repeat(64)),
            officina_version: "0.6.0".into(),
            vector_sha256: "v".repeat(64),
            programs_sha256: "p".repeat(64),
        }
    }

    fn battery_lines(batteries: Vec<officina_core::workshop::doctor::BatteryFacts>) -> Vec<Check> {
        checks(&WorkshopFacts {
            batteries,
            ..facts()
        })
    }

    /// Story 6.10 AC 8 (AD-117, D-232): a battery whose key moved since
    /// its pass is stale, named with the key; a valid one is not.
    #[test]
    fn a_stale_battery_names_the_key_that_moved() {
        use officina_core::workshop::battery::{BatteryOutcome, BatteryRun};
        use officina_core::workshop::doctor::BatteryFacts;
        let now = facts().now_ms;
        let passed = BatteryRun {
            key: battery_key(),
            outcome: Some(BatteryOutcome::Passed),
            finished_at_ms: Some(now - 3 * 60 * 60 * 1000),
        };
        let image = |current| BatteryFacts {
            image: "officina-workshop:local".into(),
            current: Some(current),
            runs: vec![passed.clone()],
            newest_skew_ms: None,
        };
        let valid = battery_lines(vec![image(battery_key())]);
        assert!(line_of(&valid, WorkshopCheck::BatteryStale).is_none());
        let last = line_of(&valid, WorkshopCheck::BatteryLast).unwrap();
        assert!(last.detail.contains("passed 3 h ago"), "{}", last.detail);
        let mut moved = battery_key();
        moved.engine_key = "Docker Desktop|29.9.0|6.18|digest".into();
        let stale = battery_lines(vec![image(moved)]);
        let line = line_of(&stale, WorkshopCheck::BatteryStale).unwrap();
        assert_eq!(line.level, level_of(WorkshopCheck::BatteryStale));
        assert!(line.detail.contains("battery_stale"), "{}", line.detail);
        assert!(line.detail.contains("the engine moved"), "{}", line.detail);
        let never = battery_lines(vec![BatteryFacts {
            runs: Vec::new(),
            ..image(battery_key())
        }]);
        assert!(
            line_of(&never, WorkshopCheck::BatteryStale)
                .unwrap()
                .detail
                .contains("no escape battery has run")
        );
    }

    /// Story 6.10 AC 8 (D-242): the newest battery's clock skew is shown,
    /// a diagnostic only.
    #[test]
    fn the_last_clock_skew_is_shown() {
        use officina_core::workshop::battery::{BatteryOutcome, BatteryRun};
        use officina_core::workshop::doctor::BatteryFacts;
        let now = facts().now_ms;
        let run = |at| BatteryRun {
            key: battery_key(),
            outcome: Some(BatteryOutcome::Passed),
            finished_at_ms: Some(at),
        };
        let lines = battery_lines(vec![
            BatteryFacts {
                image: "officina-workshop:local".into(),
                current: None,
                runs: vec![run(now - 10_000)],
                newest_skew_ms: Some(-1500),
            },
            BatteryFacts {
                image: "officina-project-01234567-api:local".into(),
                current: None,
                runs: vec![run(now - 99_000)],
                newest_skew_ms: Some(40),
            },
        ]);
        let skew = line_of(&lines, WorkshopCheck::BatteryClockSkew).unwrap();
        assert_eq!(skew.level, level_of(WorkshopCheck::BatteryClockSkew));
        assert!(skew.detail.contains("-1500 ms"), "{}", skew.detail);
    }

    #[test]
    fn a_missing_pretooluse_program_fails() {
        use officina_core::workshop::inventory::{
            MissingDetail, Resolution, Resolved, SettingsFile, SettingsSource, StoredInventory,
            extract,
        };
        let settings = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"python guard.py"}]}],"Stop":[{"hooks":[{"type":"command","command":"notify-send done"}]}]}}"#;
        let commands = extract(
            &[SettingsFile {
                source: SettingsSource::MountedSettings,
                bytes: settings.as_bytes().to_vec(),
            }],
            None,
            &[],
        )
        .unwrap();
        let missing = |index| Resolved {
            index,
            resolution: Resolution::Missing,
            path: None,
            detail: Some(MissingDetail::NotOnPath.code().to_owned()),
        };
        let (_dir, mut hooks) = hook_project("{}", &[]);
        hooks.stored = Some(StoredInventory {
            commands,
            results: vec![missing(0), missing(1)],
        });
        let lines = hook_lines(hooks);
        let fail = line_of(&lines, WorkshopCheck::HookProgramMissing).expect("the guard fails");
        assert!(fail.detail.contains("PreToolUse") && fail.detail.contains("not_on_path"));
        let warn = line_of(&lines, WorkshopCheck::HookProgramWarning).expect("the Stop hook warns");
        assert!(warn.detail.contains("Stop"));
        let (_dir, absent) = hook_project("{}", &[]);
        assert!(line_of(&hook_lines(absent), WorkshopCheck::InventoryAbsent).is_some());
    }

    #[test]
    fn an_env_block_in_a_mounted_settings_file_warns_by_name() {
        let (_dir, hooks) = hook_project(
            r#"{"env":{"DEPLOY_TOKEN":"value-never-printed"},"apiKeyHelper":"/bin/key"}"#,
            &[],
        );
        let lines = hook_lines(hooks);
        let warn = line_of(&lines, WorkshopCheck::SettingsExposed).expect("a warning");
        assert!(warn.detail.contains("DEPLOY_TOKEN") && warn.detail.contains("apiKeyHelper"));
        assert!(
            lines
                .iter()
                .all(|l| !l.detail.contains("value-never-printed")),
            "names only"
        );
    }

    #[test]
    fn a_crlf_hook_script_warns() {
        let settings = r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"$CLAUDE_PROJECT_DIR/.claude/hooks/a.sh"},{"type":"command","command":"$CLAUDE_PROJECT_DIR/.claude/hooks/b.sh"}]}]}}"#;
        let (_dir, hooks) = hook_project(
            settings,
            &[
                (".claude/hooks/a.sh", b"#!/bin/sh\r\nexit 0\r\n"),
                (".claude/hooks/b.sh", b"#!/bin/sh\nexit 0\n"),
            ],
        );
        assert_eq!(hooks.crlf, [".claude/hooks/a.sh"]);
        let lines = hook_lines(hooks);
        let warn = line_of(&lines, WorkshopCheck::HookScriptCrlf).expect("a warning");
        assert!(warn.detail.contains("a.sh"));
    }

    fn image_lines(report: ImageReport, info: Option<InfoFacts>) -> Vec<Finding> {
        let mut facts = facts();
        facts.info = info;
        facts.images = Some(report);
        findings(&facts)
    }

    fn found(lines: &[Finding], check: WorkshopCheck) -> Option<&Finding> {
        lines.iter().find(|f| f.check == check)
    }

    #[test]
    fn matching_images_and_programs_are_named() {
        let lines = image_lines(report(), None);
        assert!(found(&lines, WorkshopCheck::LinuxPrograms).is_some());
        let image = found(&lines, WorkshopCheck::ImageMatches).expect("the base matches");
        assert!(image.detail.contains("sha256:aa"));
        for quiet in [
            WorkshopCheck::ImageMismatch,
            WorkshopCheck::ClaudeVersionDiffers,
            WorkshopCheck::CaLabelMismatch,
            WorkshopCheck::WorkersOverMemory,
        ] {
            assert!(found(&lines, quiet).is_none(), "{quiet:?}");
        }
        let label = found(&lines, WorkshopCheck::CaLabel).expect("the CA label found (D-208)");
        assert!(label.detail.ends_with("none"));
    }

    #[test]
    fn missing_linux_programs_fail() {
        let mut report = report();
        report.programs = Err("linux_programs_missing".into());
        let lines = image_lines(report, None);
        let missing = found(&lines, WorkshopCheck::LinuxProgramsMissing).unwrap();
        assert_eq!(missing.check.level(), CheckLevel::Fail);
        assert!(missing.detail.contains(ENV_LINUX_BIN_DIR));
    }

    #[test]
    fn a_label_mismatch_is_named() {
        let mut report = report();
        report.images[0]
            .expected
            .labels
            .insert("officina.version".into(), "9.9.9".into());
        let lines = image_lines(report, None);
        let mismatch = found(&lines, WorkshopCheck::ImageMismatch).unwrap();
        assert_eq!(mismatch.check.level(), CheckLevel::Fail);
        assert!(mismatch.detail.contains("officina.version"), "{mismatch:?}");
        let mut absent = self::report();
        absent.images[0].inspected = None;
        let lines = image_lines(absent, None);
        assert_eq!(
            found(&lines, WorkshopCheck::ImageAbsent)
                .unwrap()
                .check
                .level(),
            CheckLevel::Warn
        );
    }

    #[test]
    fn a_claude_version_difference_warns() {
        let mut report = report();
        report.host_claude = Some("2.1.400".into());
        let lines = image_lines(report, None);
        let differs = found(&lines, WorkshopCheck::ClaudeVersionDiffers).unwrap();
        assert_eq!(differs.check.level(), CheckLevel::Warn);
        assert!(differs.detail.contains("2.1.316") && differs.detail.contains("2.1.400"));
        assert!(
            found(&lines, WorkshopCheck::ImageMatches).is_some(),
            "the version is not compared at launch (AD-99)"
        );
    }

    #[test]
    fn a_ca_label_mismatch_is_named() {
        let mut report = report();
        report.ca = CaRead::Sha256("c".repeat(64));
        report.ca_approved = Some("c".repeat(64));
        let lines = image_lines(report, None);
        let mismatch = found(&lines, WorkshopCheck::CaLabelMismatch).unwrap();
        assert_eq!(mismatch.check.level(), CheckLevel::Fail);
        assert!(mismatch.detail.contains("D-208"));
        let mut unreadable = self::report();
        unreadable.ca = CaRead::Unreadable("C:/path/to/ca.pem: not found".into());
        let lines = image_lines(unreadable, None);
        assert_eq!(
            found(&lines, WorkshopCheck::CaUnreadable)
                .unwrap()
                .check
                .level(),
            CheckLevel::Fail
        );
    }

    #[test]
    fn workers_times_memory_over_the_engine_warns() {
        let info = |gib: u64| InfoFacts {
            memory_bytes: gib * 1024 * 1024 * 1024,
            cpus: 8,
            system_time: String::new(),
            settings_digest: String::new(),
        };
        let lines = image_lines(report(), Some(info(6)));
        let over = found(&lines, WorkshopCheck::WorkersOverMemory).unwrap();
        assert_eq!(over.check.level(), CheckLevel::Warn);
        assert!(over.detail.contains("8192 MiB"), "{over:?}");
        let lines = image_lines(report(), Some(info(16)));
        assert!(found(&lines, WorkshopCheck::WorkersOverMemory).is_none());
    }

    use crate::workshop::proxy::IeProxyConfig;
    use officina_core::workshop::proxy::PacAnswer;

    struct Reader(IeProxyConfig, PacAnswer, bool);

    impl SystemProxyReader for Reader {
        fn config(&self) -> Result<IeProxyConfig, String> {
            Ok(self.0.clone())
        }
        fn pac(&self, _url: &str, _config: &IeProxyConfig) -> PacAnswer {
            self.1.clone()
        }
        fn evaluates_pac(&self) -> bool {
            self.2
        }
    }

    fn proxy_lines(reader: Reader, ca_file: bool) -> Vec<Check> {
        let mut facts = facts();
        facts.proxy = Some(proxy_facts(
            &ProxyInput {
                setting: UpstreamProxy::System,
                allow: vec!["api.anthropic.com".into(), "index.crates.io".into()],
                ca_file,
            },
            Arc::new(reader),
        ));
        checks(&facts)
            .into_iter()
            .filter(|c| c.title == "workshop proxy")
            .collect()
    }

    #[test]
    fn the_proxy_source_and_routes_are_named() {
        let lines = proxy_lines(
            Reader(
                IeProxyConfig {
                    static_proxy: Some("proxy.example.com:8080".into()),
                    bypass: Some("*.crates.io".into()),
                    ..IeProxyConfig::default()
                },
                PacAnswer::NoPac,
                true,
            ),
            true,
        );
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].detail.contains("resolved from static"),
            "{lines:?}"
        );
        assert!(
            lines[0]
                .detail
                .contains("api.anthropic.com via proxy.example.com:8080")
        );
        assert!(lines[0].detail.contains("index.crates.io direct"));
    }

    #[test]
    fn a_socks_answer_fails_with_its_entry() {
        let lines = proxy_lines(
            Reader(
                IeProxyConfig {
                    pac_url: Some("http://wpad.example.com/proxy.pac".into()),
                    ..IeProxyConfig::default()
                },
                PacAnswer::Answer("SOCKS socks.example.com:1080".into()),
                true,
            ),
            true,
        );
        assert_eq!(lines[0].level, Level::Fail);
        assert!(
            lines[0]
                .detail
                .contains("proxy_unsupported for api.anthropic.com")
        );
    }

    #[test]
    fn a_proxied_api_host_without_a_ca_file_notes_the_export() {
        let reader = || {
            Reader(
                IeProxyConfig {
                    static_proxy: Some("proxy.example.com:8080".into()),
                    ..IeProxyConfig::default()
                },
                PacAnswer::NoPac,
                true,
            )
        };
        assert!(
            proxy_lines(reader(), false)
                .iter()
                .any(|c| c.detail.contains("export its root"))
        );
        assert!(
            !proxy_lines(reader(), true)
                .iter()
                .any(|c| c.detail.contains("export its root"))
        );
    }

    #[test]
    fn an_unevaluated_auto_detect_is_named() {
        let lines = proxy_lines(
            Reader(
                IeProxyConfig {
                    auto_detect: true,
                    ..IeProxyConfig::default()
                },
                PacAnswer::NoPac,
                false,
            ),
            true,
        );
        assert!(
            lines
                .iter()
                .any(|c| c.level == Level::Warn && c.detail.contains("cannot evaluate"))
        );
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
