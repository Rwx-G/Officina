//! Build images (AD-99, AD-100, D-205, D-208, D-233): the Linux programs,
//! the build contexts written fresh under `<home>/workshop/build/`, the
//! builds in AD-99's order on the engine's `docker`-driver builder, each
//! image's labels, `ENV` names and measured programs, and the record and
//! journal line of each. The rules (labels, reserved names, build
//! arguments, the builder, the tar digest) are core's.
//!
//! The window's flow: [`grant`] on the collector thread reads the approved
//! catalogue; [`preflight`] on thread `workshop-build` checks the engine
//! and the files and writes the contexts; [`build_all`] runs the builds
//! there and hands each [`BuildReport`] back to the collector, which
//! [`record`]s it. Only the finding of the programs is compiled into
//! `officina.exe`, for doctor.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use officina_core::workshop::image::BuildRefusal;

/// The gateway program's file name in the Linux programs' folder (AD-100).
pub const GATEWAY_PROGRAM: &str = "officina-gateway";
/// The entry point's file name in the Linux programs' folder (AD-100).
pub const ENTRY_PROGRAM: &str = "officina-entry";
/// The folder beside the window's executable holding both (AD-100).
pub const LINUX_DIR: &str = "linux";

/// The two Linux programs a build copies into its contexts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxPrograms {
    pub dir: PathBuf,
    pub gateway: PathBuf,
    pub entry: PathBuf,
}

/// Where the Linux programs are (AD-100, D-167): the folder
/// `OFFICINA_LINUX_BIN_DIR` names when set, else `<exe dir>/linux/`; both
/// files must be there, regular files, else Build images is refused
/// `linux_programs_missing`. The executable directory is an argument.
pub fn find_programs(
    exe_dir: Option<&Path>,
    override_dir: Option<&OsStr>,
) -> Result<LinuxPrograms, BuildRefusal> {
    let dir = match override_dir.filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => exe_dir
            .ok_or(BuildRefusal::LinuxProgramsMissing)?
            .join(LINUX_DIR),
    };
    let found = |name: &str| {
        let path = dir.join(name);
        std::fs::symlink_metadata(&path)
            .is_ok_and(|meta| meta.is_file())
            .then_some(path)
    };
    match (found(GATEWAY_PROGRAM), found(ENTRY_PROGRAM)) {
        (Some(gateway), Some(entry)) => Ok(LinuxPrograms {
            dir,
            gateway,
            entry,
        }),
        _ => Err(BuildRefusal::LinuxProgramsMissing),
    }
}

/// The Officina version every label carries (AD-99): the workspace's.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The source revision a release build records in the OCI labels.
pub const REVISION: &str = match option_env!("OFFICINA_REVISION") {
    Some(revision) => revision,
    None => "unknown",
};

/// The SHA-256 of the two Linux programs, which every image's labels carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramDigests {
    pub entry: String,
    pub gateway: String,
}

/// What the running Officina expects of an image of `kind` (AD-99,
/// D-233), for the launch's check and doctor: the labels it would build
/// with now (the build date, revision and Claude Code version are never
/// compared), a project image's base id, and the base's measured programs.
pub fn expected(
    kind: officina_core::workshop::image::ImageKind,
    digests: &ProgramDigests,
    ca_sha256: Option<&str>,
    project: Option<officina_core::workshop::image::ProjectLabels>,
    base_programs_sha256: Option<&str>,
) -> officina_core::workshop::image::Expected {
    use officina_core::workshop::image::{Expected, LabelInputs, labels};
    let base_id = project.as_ref().map(|p| p.base_id.clone());
    Expected {
        labels: labels(&LabelInputs {
            kind,
            version: VERSION.to_owned(),
            revision: REVISION.to_owned(),
            created: String::new(),
            entry_sha256: digests.entry.clone(),
            gateway_sha256: digests.gateway.clone(),
            claude_version: String::new(),
            ca_sha256: ca_sha256.map(str::to_owned),
            project,
        }),
        base_id,
        base_programs_sha256: base_programs_sha256.map(str::to_owned),
    }
}

#[cfg(feature = "window-authority")]
pub use window::*;

#[cfg(feature = "window-authority")]
mod window {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use officina_core::floor::FloorFile;
    use officina_core::relay::journal::{JournalEntry, WorkshopImageBuilt};
    use officina_core::relay::template::TemplateUnavailable;
    use officina_core::workshop::engine::{
        BuildInputs, EngineCall, ImageFacts, docker_builder, parse_image_inspect,
    };
    use officina_core::workshop::floor::{
        CatalogueFiles, ContextFile, FileContent, UpstreamProxy, context_sha256, file_requests,
    };
    use officina_core::workshop::image::{
        self, BuildOutcome, BuildRefusal, ENTRY_PATH, GATEWAY_DOCKERFILE, GATEWAY_IMAGE,
        INSTALLER_HOST, ImageKind, LabelInputs, MEASURED_PROGRAMS, NotFromBase, ProjectLabels,
        WORKSHOP_BASE, WORKSHOP_DOCKERFILE, build_args, final_base, labels, programs_sha256,
        reserved_env, stages,
    };
    use officina_core::workshop::names::{
        ObjectName, ROLE_LABEL, base_labels, floor_key, project_image,
    };
    use officina_core::workshop::proxy::Route;
    use officina_core::workshop::tar::FileDigest;

    use super::{LinuxPrograms, REVISION, VERSION};
    use crate::registry::window::WindowRegistry;
    use crate::registry::workshop::{ImageRecord, InventoryRecord};
    use crate::relay::home::RelayHome;
    use crate::workshop::engine::{
        EngineState, ResolvedDocker, build, execute, read_engine_state, stream_out,
    };
    use crate::workshop::inventory::{
        InventoryEngine, InventoryProject, WorkshopGit, take as take_inventory,
    };
    use crate::workshop::proxy::{SystemProxyReader, resolve};

    /// The CA file's name in an Officina image's `ca/` folder (AD-99).
    pub const CA_NAME: &str = "officina-ca.crt";
    /// The most a measured program may weigh in its `docker cp` stream;
    /// past it the copy is stopped (D-218's reader, sized for Claude
    /// Code's native binary).
    pub const MAX_PROGRAM_BYTES: u64 = 1024 * 1024 * 1024;
    /// The role label of the container a measurement creates and never
    /// starts (D-233).
    pub const MEASURE_ROLE: &str = "measure";

    /// One project image to build, as the approved catalogue holds it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProjectPlan {
        pub name: String,
        pub dockerfile_sha256: String,
        pub context_sha256: String,
    }

    /// What the collector granted (AD-99): the approved catalogue's
    /// workshop facts, read on its thread, for thread `workshop-build`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct BuildPlan {
        pub home: PathBuf,
        pub floor: FloorFile,
        pub floor_path: String,
        pub floor_key: String,
        pub upstream_proxy: UpstreamProxy,
        pub ca_sha256: Option<String>,
        pub projects: Vec<ProjectPlan>,
        /// Every workshop project, for the inventory after its image.
        pub inventories: Vec<PlannedInventory>,
        pub claude_version: String,
        pub base_env: Vec<(String, String)>,
    }

    /// One project whose inventory follows the build of its image (AD-118).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PlannedInventory {
        pub project: InventoryProject,
        /// Built from its own Dockerfile, else run on the base image.
        pub has_dockerfile: bool,
        /// The branch whose tip the build's inventory reads tracked files
        /// at (D-1137): the first base of the first workshop template that
        /// names the project; `None` when no template names it.
        pub base_branch: Option<String>,
    }

    /// The workshop's stop flag refuses a build (AD-116).
    fn stopped(home: &Path) -> bool {
        crate::workshop::workshop_stopped(home)
    }

    /// Grants a Build images click on the collector thread (AD-99): the
    /// stop flag, the approved catalogue the window's record vouches for
    /// and equal to the floor read now (its workshop files included,
    /// D-244), and the host's Claude Code version as `claude --version`
    /// printed it. `base_env` is the window's environment (D-86).
    pub fn grant(
        relay: &RelayHome,
        home: &Path,
        claude_version: Option<&str>,
        base_env: &[(String, String)],
    ) -> Result<BuildPlan, BuildRefusal> {
        if stopped(home) {
            return Err(BuildRefusal::WorkshopStopped);
        }
        let (approved, current) = crate::orders::catalogue(relay).map_err(|why| match why {
            TemplateUnavailable::NotApproved => BuildRefusal::NotApproved,
            TemplateUnavailable::CatalogueChanged | TemplateUnavailable::WorkshopOnly => {
                BuildRefusal::CatalogueChanged
            }
        })?;
        if approved.hash != current.hash {
            return Err(BuildRefusal::CatalogueChanged);
        }
        plan_from(current, home, claude_version, base_env)
    }

    /// The plan of a floor read with its workshop files (`load_relay_floor`):
    /// its workshop table, the project images its Dockerfiles make, the
    /// host's Claude Code version as `claude --version` printed it. The
    /// window calls it through [`grant`] on the approved catalogue only;
    /// `examples/battery_ci.rs` on the floor it is given.
    pub fn plan_from(
        current: officina_core::relay::catalogue::RelayFloor,
        home: &Path,
        claude_version: Option<&str>,
        base_env: &[(String, String)],
    ) -> Result<BuildPlan, BuildRefusal> {
        let workshop = current
            .canonical
            .workshop
            .as_ref()
            .ok_or(BuildRefusal::WorkshopOff)?;
        let upstream_proxy = UpstreamProxy::parse(&workshop.upstream_proxy)
            .map_err(|_| BuildRefusal::CatalogueChanged)?;
        let ca_sha256 = workshop.ca_sha256.clone();
        let projects = current
            .canonical
            .projects
            .iter()
            .filter_map(|(name, project)| {
                let w = project.workshop.as_ref()?;
                w.dockerfile.as_ref()?;
                Some(ProjectPlan {
                    name: name.clone(),
                    dockerfile_sha256: w.dockerfile_sha256.clone()?,
                    context_sha256: w.context_sha256.clone()?,
                })
            })
            .collect();
        let inventories = current
            .canonical
            .projects
            .iter()
            .filter_map(|(name, project)| {
                let w = project.workshop.as_ref()?;
                let base_branch = current
                    .canonical
                    .templates
                    .values()
                    .filter(|t| t.projects.contains(name))
                    .find_map(|t| t.workshop.as_ref()?.base.first().cloned());
                // The canonical path is normalised for comparison (case
                // folded); IO takes the floor's own path, as the
                // Dockerfile reads do.
                let raw = current.floor.projects.iter().find(|p| &p.name == name)?;
                let path = officina_core::floor::join_project_path(
                    officina_core::floor::floor_dir_of(&current.floor_path),
                    &raw.path,
                );
                Some(PlannedInventory {
                    project: InventoryProject {
                        name: name.clone(),
                        path: PathBuf::from(path),
                        mounts: w.mounts.clone(),
                    },
                    has_dockerfile: w.dockerfile.is_some(),
                    base_branch,
                })
            })
            .collect();
        let claude_version = claude_version
            .and_then(officina_core::contract::claude_version)
            .map(|[a, b, c]| format!("{a}.{b}.{c}"))
            .ok_or(BuildRefusal::ClaudeVersionUnknown)?;
        Ok(BuildPlan {
            home: home.to_path_buf(),
            floor_key: floor_key(&current.floor_path),
            floor: current.floor,
            floor_path: current.floor_path,
            upstream_proxy,
            ca_sha256,
            projects,
            inventories,
            claude_version,
            base_env: base_env.to_vec(),
        })
    }

    /// One image's build, its context written and its labels decided.
    #[derive(Debug, Clone)]
    pub struct ImageJob {
        pub kind: ImageKind,
        pub tag: String,
        pub project: Option<ProjectPlan>,
        pub context: PathBuf,
        /// The Dockerfile's name in the context, when not `Dockerfile`.
        pub dockerfile: Option<String>,
        /// The project Dockerfile's text, whose stages are checked before
        /// the build.
        pub dockerfile_text: Option<String>,
    }

    /// What [`preflight`] prepared for [`build_all`].
    #[derive(Debug, Clone)]
    pub struct Prepared {
        pub docker: ResolvedDocker,
        pub base_env: Vec<(String, String)>,
        pub home: PathBuf,
        pub floor_key: String,
        pub builder: String,
        pub build_args: BTreeMap<String, String>,
        pub entry_sha256: String,
        pub gateway_sha256: String,
        pub claude_version: String,
        pub ca_sha256: Option<String>,
        pub jobs: Vec<ImageJob>,
        pub inventories: Vec<PlannedInventory>,
        /// The workshop's `git`, when it resolves: without it no
        /// inventory follows the builds and the launch runs it.
        pub git: Option<WorkshopGit>,
    }

    fn dir_name(tag: &str) -> String {
        tag.split(':').next().unwrap_or(tag).to_owned()
    }

    /// Writes `files` into `dir`, emptied first (AD-124: recreated per
    /// build).
    fn write_context(dir: &Path, files: &[(String, Vec<u8>)]) -> std::io::Result<()> {
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        std::fs::create_dir_all(dir)?;
        for (name, bytes) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
        }
        Ok(())
    }

    /// The bytes of a read file whose SHA-256 is `expected`, else `None`.
    fn approved_bytes(content: Option<&FileContent>, expected: &str) -> Option<Vec<u8>> {
        match content? {
            FileContent::Bytes(bytes) if officina_core::sha256_hex(bytes) == expected => {
                Some(bytes.clone())
            }
            _ => None,
        }
    }

    /// The project context exactly as approved: the files phase 2 read,
    /// whose folder digest is the approved one (AD-101). Written fresh, so
    /// a file changed after the check never reaches the build.
    fn approved_context(
        files: &CatalogueFiles,
        project: &ProjectPlan,
    ) -> Option<(Vec<ContextFile>, String)> {
        let read = files.dockerfiles.get(&project.name)?;
        let dockerfile = approved_bytes(Some(&read.dockerfile), &project.dockerfile_sha256)?;
        let context = read.context.as_ref().ok()?;
        (context_sha256(context) == project.context_sha256).then(|| {
            (
                context.clone(),
                String::from_utf8_lossy(&dockerfile).into_owned(),
            )
        })
    }

    /// Checks what a build needs before any build starts, on thread
    /// `workshop-build` (AD-98): the engine is a Linux engine with a
    /// `docker`-driver builder, the workshop files still hash to the
    /// approved digests, the installer host's route is decided (D-611);
    /// then writes every context fresh under `<home>/workshop/build/`
    /// (AD-124), the CA file as `ca/officina-ca.crt` (D-612).
    pub fn preflight(
        plan: &BuildPlan,
        programs: &LinuxPrograms,
        docker: &ResolvedDocker,
        reader: Arc<dyn SystemProxyReader>,
    ) -> Result<Prepared, BuildRefusal> {
        let home = &plan.home;
        std::fs::create_dir_all(home).map_err(|_| BuildRefusal::ContextUnwritable)?;
        if !matches!(
            read_engine_state(Ok(docker), &plan.base_env, home),
            EngineState::Ready(_)
        ) {
            return Err(BuildRefusal::EngineUnavailable);
        }
        let builders = execute(
            docker,
            &EngineCall::builder_ls(),
            &plan.base_env,
            home,
            None,
        )
        .map_err(|_| BuildRefusal::EngineUnavailable)?;
        let builder = docker_builder(&String::from_utf8_lossy(&builders.stdout))
            .ok_or(BuildRefusal::BuilderMissing)?;
        let files = crate::workshop::files::read(&plan.floor, &plan.floor_path);
        let ca = match &plan.ca_sha256 {
            Some(digest) => Some(
                approved_bytes(files.ca_file.as_ref(), digest)
                    .ok_or(BuildRefusal::CatalogueChanged)?,
            ),
            None => None,
        };
        let mut projects = Vec::new();
        for project in &plan.projects {
            let found = approved_context(&files, project).ok_or(BuildRefusal::CatalogueChanged)?;
            projects.push((project, found));
        }
        let route = resolve(&plan.upstream_proxy, &[INSTALLER_HOST.to_owned()], reader)
            .map_err(|_| BuildRefusal::ProxyRefused)?;
        let proxy = match route.routes.get(INSTALLER_HOST) {
            Some(Route::Proxy(proxy)) => Some(proxy.clone()),
            Some(Route::Direct) | None => None,
        };
        let read =
            |path: &Path| std::fs::read(path).map_err(|_| BuildRefusal::LinuxProgramsMissing);
        let gateway = read(&programs.gateway)?;
        let entry = read(&programs.entry)?;
        let build_dir = crate::workshop::build_dir(home);
        let unwritable = |_| BuildRefusal::ContextUnwritable;
        let mut jobs = Vec::new();
        let gateway_dir = build_dir.join(dir_name(GATEWAY_IMAGE));
        write_context(
            &gateway_dir,
            &[
                (
                    "Dockerfile".to_owned(),
                    GATEWAY_DOCKERFILE.as_bytes().to_vec(),
                ),
                (super::GATEWAY_PROGRAM.to_owned(), gateway.clone()),
            ],
        )
        .map_err(unwritable)?;
        jobs.push(ImageJob {
            kind: ImageKind::Gateway,
            tag: GATEWAY_IMAGE.to_owned(),
            project: None,
            context: gateway_dir,
            dockerfile: None,
            dockerfile_text: None,
        });
        let base_dir = build_dir.join(dir_name(WORKSHOP_BASE));
        let mut base_files = vec![
            (
                "Dockerfile".to_owned(),
                WORKSHOP_DOCKERFILE.as_bytes().to_vec(),
            ),
            (super::ENTRY_PROGRAM.to_owned(), entry.clone()),
        ];
        if let Some(ca) = &ca {
            base_files.push((format!("ca/{CA_NAME}"), ca.clone()));
        }
        write_context(&base_dir, &base_files).map_err(unwritable)?;
        std::fs::create_dir_all(base_dir.join("ca")).map_err(unwritable)?;
        jobs.push(ImageJob {
            kind: ImageKind::Workshop,
            tag: WORKSHOP_BASE.to_owned(),
            project: None,
            context: base_dir,
            dockerfile: None,
            dockerfile_text: None,
        });
        for (project, (context, dockerfile_text)) in projects {
            let tag = project_image(&plan.floor_key, &project.name);
            let dir = build_dir.join(dir_name(&tag));
            let files: Vec<(String, Vec<u8>)> = context
                .iter()
                .map(|f| (f.name.clone(), f.bytes.clone()))
                .collect();
            write_context(&dir, &files).map_err(unwritable)?;
            let dockerfile = file_requests(&plan.floor, &plan.floor_path)
                .dockerfiles
                .into_iter()
                .find(|r| r.project == project.name)
                .and_then(|r| {
                    Path::new(&r.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .filter(|name| name != "Dockerfile");
            jobs.push(ImageJob {
                kind: ImageKind::Project,
                tag,
                project: Some(project.clone()),
                context: dir,
                dockerfile,
                dockerfile_text: Some(dockerfile_text),
            });
        }
        Ok(Prepared {
            docker: docker.clone(),
            base_env: plan.base_env.clone(),
            home: home.clone(),
            floor_key: plan.floor_key.clone(),
            builder,
            build_args: build_args(&plan.claude_version, proxy.as_deref()),
            entry_sha256: officina_core::sha256_hex(&entry),
            gateway_sha256: officina_core::sha256_hex(&gateway),
            claude_version: plan.claude_version.clone(),
            ca_sha256: plan.ca_sha256.clone(),
            jobs,
            inventories: plan.inventories.clone(),
            git: crate::workshop::inventory::workshop_git(home, &plan.base_env),
        })
    }

    /// One image's outcome, for the collector to record and journal.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BuildReport {
        pub record: ImageRecord,
        /// The build's last output, untrusted, as it came.
        pub tail: Option<String>,
        pub detail: Option<NotFromBase>,
        pub duration_ms: i64,
        /// The inventories taken on the image once built (AD-118).
        pub inventories: Vec<InventoryRecord>,
    }

    /// The labels `job` is built with (AD-99).
    pub fn job_labels(
        prepared: &Prepared,
        job: &ImageJob,
        base_id: Option<&str>,
        created: &str,
    ) -> BTreeMap<String, String> {
        labels(&LabelInputs {
            kind: job.kind,
            version: VERSION.to_owned(),
            revision: REVISION.to_owned(),
            created: created.to_owned(),
            entry_sha256: prepared.entry_sha256.clone(),
            gateway_sha256: prepared.gateway_sha256.clone(),
            claude_version: prepared.claude_version.clone(),
            ca_sha256: prepared.ca_sha256.clone(),
            project: job.project.as_ref().map(|p| ProjectLabels {
                floor_key: prepared.floor_key.clone(),
                project: p.name.clone(),
                dockerfile_sha256: p.dockerfile_sha256.clone(),
                context_sha256: p.context_sha256.clone(),
                base_id: base_id.unwrap_or_default().to_owned(),
            }),
        })
    }

    /// An RFC 3339 UTC time for the OCI `created` label.
    fn rfc3339(now_ms: i64) -> String {
        let secs = now_ms.div_euclid(1000);
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Howard Hinnant's days-to-civil.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            rem / 3600,
            rem % 3600 / 60,
            rem % 60
        )
    }

    struct Ran {
        outcome: BuildOutcome,
        facts: Option<ImageFacts>,
        programs: Option<String>,
        detail: Option<NotFromBase>,
        tail: Option<String>,
    }

    impl Ran {
        fn failed(tail: String) -> Ran {
            Ran {
                outcome: BuildOutcome::Failed,
                facts: None,
                programs: None,
                detail: None,
                tail: Some(tail),
            }
        }
    }

    fn image_facts(prepared: &Prepared, tag: &str) -> Option<ImageFacts> {
        let name = ObjectName::parse(tag)?;
        let out = execute(
            &prepared.docker,
            &EngineCall::image_inspect(&name),
            &prepared.base_env,
            &prepared.home,
            None,
        )
        .ok()
        .filter(|o| o.succeeded())?;
        parse_image_inspect(&String::from_utf8_lossy(&out.stdout))
    }

    fn remove_image(prepared: &Prepared, tag: &str) {
        if let Some(name) = ObjectName::parse(tag) {
            let _ = execute(
                &prepared.docker,
                &EngineCall::image_rm(&name),
                &prepared.base_env,
                &prepared.home,
                None,
            );
        }
    }

    /// Measures `image_id`'s programs (D-233): a container of it created,
    /// never started, each program streamed out through the capped reader
    /// into core's tar digest, the container removed whatever happened.
    /// Returns each program's SHA-256 in [`MEASURED_PROGRAMS`]' order.
    pub fn measure(prepared: &Prepared, image_id: &str) -> Result<[String; 2], String> {
        let image = ObjectName::parse(image_id).ok_or("the image id is not a name")?;
        let short: String = image_id
            .trim_start_matches("sha256:")
            .chars()
            .take(16)
            .collect();
        let container = ObjectName::parse(&format!("officina-measure-{short}"))
            .ok_or("the measuring container's name")?;
        let mut container_labels = base_labels(&prepared.floor_key, VERSION);
        container_labels.insert(ROLE_LABEL.to_owned(), MEASURE_ROLE.to_owned());
        let remove = || {
            let _ = execute(
                &prepared.docker,
                &EngineCall::rm(std::slice::from_ref(&container)),
                &prepared.base_env,
                &prepared.home,
                None,
            );
        };
        // A container a crash left behind under the same name goes first.
        remove();
        let created = execute(
            &prepared.docker,
            &EngineCall::create_unstarted(&container, &image, &container_labels),
            &prepared.base_env,
            &prepared.home,
            None,
        )
        .map_err(|e| e.to_string())?;
        if !created.succeeded() {
            remove();
            return Err(format!("create: {}", created.stderr.trim()));
        }
        let mut digests = Vec::new();
        for path in MEASURED_PROGRAMS {
            let mut digest = FileDigest::new();
            let streamed = stream_out(
                &prepared.docker,
                &EngineCall::cp_out(&container, path),
                &prepared.base_env,
                &prepared.home,
                MAX_PROGRAM_BYTES,
                &mut |chunk| digest.push(chunk).map_err(|e| e.to_string()),
            );
            let measured = match streamed {
                Ok(done) if done.status.success() => digest.finish().map_err(|e| e.to_string()),
                Ok(done) => Err(format!(
                    "cp {path}: {}",
                    String::from_utf8_lossy(&done.stderr).trim()
                )),
                Err(e) => Err(format!("cp {path}: {e}")),
            };
            match measured {
                Ok((sha, _)) => digests.push(sha),
                Err(why) => {
                    remove();
                    return Err(why);
                }
            }
        }
        remove();
        let [entry, claude]: [String; 2] =
            digests.try_into().map_err(|_| "two programs".to_owned())?;
        Ok([entry, claude])
    }

    fn build_one(
        prepared: &Prepared,
        job: &ImageJob,
        base: Option<(&str, &str)>,
        created: &str,
    ) -> Ran {
        if let Some(text) = &job.dockerfile_text
            && stages(text).ok().as_deref().and_then(final_base).is_none()
        {
            return Ran {
                outcome: BuildOutcome::NotFromBase,
                facts: None,
                programs: None,
                detail: Some(NotFromBase::Stages),
                tail: None,
            };
        }
        let base_id = base.map(|(id, _)| id);
        let inputs = BuildInputs {
            tag: job.tag.clone(),
            context: job.context.to_string_lossy().into_owned(),
            dockerfile: job
                .dockerfile
                .as_ref()
                .map(|name| job.context.join(name).to_string_lossy().into_owned()),
            labels: job_labels(prepared, job, base_id, created),
            // The proxy reaches every build (D-611): a project's own steps
            // download too; the Claude Code version only the bases install.
            build_args: match job.kind {
                ImageKind::Workshop | ImageKind::Browser => prepared.build_args.clone(),
                ImageKind::Gateway | ImageKind::Project => {
                    let mut args = prepared.build_args.clone();
                    args.remove(image::CLAUDE_VERSION_ARG);
                    args
                }
            },
            builder: Some(prepared.builder.clone()),
        };
        let ran = match build(
            &prepared.docker,
            &EngineCall::build(&inputs),
            &prepared.base_env,
            &prepared.home,
        ) {
            Ok(ran) => ran,
            Err(e) => return Ran::failed(e.to_string()),
        };
        let tail = String::from_utf8_lossy(&ran.tail).into_owned();
        if !ran.status.success() {
            return Ran::failed(tail);
        }
        let Some(facts) = image_facts(prepared, &job.tag) else {
            return Ran::failed(format!("{tail}\nthe built image cannot be inspected"));
        };
        let refuse = |detail: NotFromBase, facts: ImageFacts, tail: String| {
            remove_image(prepared, &job.tag);
            Ran {
                outcome: BuildOutcome::NotFromBase,
                facts: Some(facts),
                programs: None,
                detail: Some(detail),
                tail: Some(tail),
            }
        };
        if job.kind == ImageKind::Project {
            let inherited = facts.labels.get(image::LABEL_BASE).map(String::as_str);
            if !matches!(inherited, Some("workshop" | "browser")) {
                return refuse(NotFromBase::NoBaseLabel, facts, tail);
            }
        }
        if reserved_env(&facts.env_names).is_some() {
            return refuse(NotFromBase::ReservedEnv, facts, tail);
        }
        if job.kind == ImageKind::Gateway {
            return Ran {
                outcome: BuildOutcome::Built,
                facts: Some(facts),
                programs: None,
                detail: None,
                tail: Some(tail),
            };
        }
        let digests = match measure(prepared, &facts.id) {
            Ok(digests) => digests,
            Err(why) => {
                remove_image(prepared, &job.tag);
                return Ran::failed(format!("{tail}\nthe programs could not be measured: {why}"));
            }
        };
        let programs = programs_sha256(&digests);
        match base {
            None if digests[0] != prepared.entry_sha256 => {
                remove_image(prepared, &job.tag);
                Ran::failed(format!(
                    "{tail}\nthe measured {ENTRY_PATH} is not the program Officina shipped"
                ))
            }
            Some((_, base_programs)) if programs != base_programs => {
                refuse(NotFromBase::ProgramsReplaced, facts, tail)
            }
            _ => Ran {
                outcome: BuildOutcome::Built,
                facts: Some(facts),
                programs: Some(programs),
                detail: None,
                tail: Some(tail),
            },
        }
    }

    fn report(job: &ImageJob, ran: Ran, started_ms: i64, now_ms: i64) -> BuildReport {
        let facts = ran.facts.as_ref();
        let built = ran.outcome == BuildOutcome::Built;
        BuildReport {
            record: ImageRecord {
                name: job.tag.clone(),
                kind: job.kind,
                project: job.project.as_ref().map(|p| p.name.clone()),
                image_id: facts.filter(|_| built).map(|f| f.id.clone()),
                labels_json: facts.map(|f| serde_json::to_string(&f.labels).unwrap_or_default()),
                env_names_json: facts
                    .map(|f| serde_json::to_string(&f.env_names).unwrap_or_default()),
                programs_sha256: ran.programs,
                built_at_ms: now_ms,
                outcome: ran.outcome,
                build_tail: None,
            },
            tail: ran.tail,
            detail: ran.detail,
            duration_ms: now_ms - started_ms,
            inventories: Vec::new(),
        }
    }

    /// The inventories that follow `job`'s image (AD-118): the base serves
    /// every project without a Dockerfile, a project image its own
    /// project. A project with no base branch, an unresolved branch or a
    /// failed run is skipped: nothing is recorded and its launch runs the
    /// inventory.
    fn inventories_after(
        prepared: &Prepared,
        job: &ImageJob,
        image_id: &str,
        now_ms: &dyn Fn() -> i64,
    ) -> Vec<InventoryRecord> {
        let Some(git) = &prepared.git else {
            return Vec::new();
        };
        let engine = InventoryEngine {
            docker: prepared.docker.clone(),
            base_env: prepared.base_env.clone(),
            home: prepared.home.clone(),
            floor_key: prepared.floor_key.clone(),
        };
        prepared
            .inventories
            .iter()
            .filter(|planned| match (&job.kind, &job.project) {
                (ImageKind::Workshop, _) => !planned.has_dockerfile,
                (ImageKind::Project, Some(project)) => planned.project.name == project.name,
                _ => false,
            })
            .filter_map(|planned| {
                let branch = planned.base_branch.as_deref()?;
                let commit = git.branch_commit(&planned.project.path, branch)?;
                take_inventory(
                    &engine,
                    git,
                    (&job.tag, image_id),
                    &planned.project,
                    &commit,
                    now_ms(),
                )
                .ok()
            })
            .collect()
    }

    /// Runs every build of `prepared` in AD-99's order (gateway, base,
    /// project images), each image's report handed to `sink` as it ends.
    /// The stop flag is read before each build: once set, the images left
    /// are reported `stopped`. A project image waits for the base: without
    /// a built base it is reported `failed` unbuilt.
    pub fn build_all(
        prepared: &Prepared,
        now_ms: &dyn Fn() -> i64,
        sink: &mut dyn FnMut(BuildReport),
    ) {
        let mut base: Option<(String, String)> = None;
        for job in &prepared.jobs {
            let started = now_ms();
            let ran = if stopped(&prepared.home) {
                Ran {
                    outcome: BuildOutcome::Stopped,
                    facts: None,
                    programs: None,
                    detail: None,
                    tail: None,
                }
            } else if job.kind == ImageKind::Project && base.is_none() {
                Ran::failed(format!("{WORKSHOP_BASE} did not build"))
            } else {
                let base_ref = base.as_ref().map(|(id, p)| (id.as_str(), p.as_str()));
                let base_ref = if job.kind == ImageKind::Project {
                    base_ref
                } else {
                    None
                };
                build_one(prepared, job, base_ref, &rfc3339(started))
            };
            if job.kind == ImageKind::Workshop && ran.outcome == BuildOutcome::Built {
                base = ran
                    .facts
                    .as_ref()
                    .map(|f| f.id.clone())
                    .zip(ran.programs.clone());
            }
            let built_id = ran
                .facts
                .as_ref()
                .filter(|_| ran.outcome == BuildOutcome::Built)
                .map(|f| f.id.clone());
            let mut done = report(job, ran, started, now_ms());
            if let Some(id) = built_id {
                done.inventories = inventories_after(prepared, job, &id, now_ms);
            }
            sink(done);
        }
    }

    /// Records one image's report on the collector thread (AD-38): its
    /// `workshop_image` row, then its `workshop_image_built` line (AD-120).
    pub fn record(
        registry: &mut WindowRegistry,
        relay: &RelayHome,
        report: &BuildReport,
        now_ms: i64,
    ) -> Result<(), officina_core::ipc::IpcError> {
        registry.record_workshop_image(&report.record, report.tail.as_deref())?;
        for inventory in &report.inventories {
            registry.record_workshop_inventory(inventory)?;
        }
        let line = JournalEntry::WorkshopImageBuilt(WorkshopImageBuilt {
            image: report.record.name.clone(),
            image_id: report.record.image_id.clone(),
            image_kind: report.record.kind,
            outcome: report.record.outcome,
            detail: report.detail,
            duration_ms: report.duration_ms,
        });
        let _ = crate::relay::journal::record(relay, &[line], now_ms);
        Ok(())
    }

    /// What a Build images click runs with, the window's own or a test's.
    pub struct BuildContext {
        /// The window's executable directory (D-167).
        pub exe_dir: Option<PathBuf>,
        /// `OFFICINA_LINUX_BIN_DIR` as the window read it.
        pub linux_override: Option<std::ffi::OsString>,
        pub docker: Result<ResolvedDocker, crate::workshop::engine::ResolveError>,
        pub reader: Arc<dyn SystemProxyReader>,
        /// Set while a build runs on thread `workshop-build`.
        pub busy: Arc<std::sync::atomic::AtomicBool>,
        pub now_ms: fn() -> i64,
    }

    /// Clears the busy flag when the build job ends, however it ends.
    struct BusyGuard(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for BusyGuard {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// The `build_images` command (AD-99, D-28's pointer click is the
    /// window's): one build at a time; the collector grants it; the Linux
    /// programs are found; thread `workshop-build` runs [`preflight`] and
    /// the answer waits for it, so a refusal names its cause; then the
    /// builds run there, each report sent to the collector as
    /// [`AuthorCommand::ImageBuilt`]. `on_done` runs once the job ends.
    pub fn build_images(
        sender: &std::sync::mpsc::Sender<crate::watch::CollectorMsg>,
        thread: &crate::workshop::threads::JobThread,
        context: BuildContext,
        on_done: Box<dyn FnOnce() + Send>,
    ) -> Result<image::BuildAnswer, officina_core::ipc::IpcError> {
        use crate::watch::{AUTHOR_WAIT, AuthorCommand, AuthorRequest, CollectorMsg, ask};
        use image::BuildAnswer;
        use std::sync::atomic::Ordering;
        if context.busy.swap(true, Ordering::SeqCst) {
            return Ok(BuildAnswer::Refused(BuildRefusal::Busy));
        }
        let guard = BusyGuard(Arc::clone(&context.busy));
        let plan = match ask(sender, AuthorCommand::GrantBuild, AUTHOR_WAIT)?.into_build_grant()? {
            Ok(plan) => plan,
            Err(refusal) => return Ok(BuildAnswer::Refused(refusal)),
        };
        let programs = match super::find_programs(
            context.exe_dir.as_deref(),
            context.linux_override.as_deref(),
        ) {
            Ok(programs) => programs,
            Err(refusal) => return Ok(BuildAnswer::Refused(refusal)),
        };
        let Ok(docker) = context.docker else {
            return Ok(BuildAnswer::Refused(BuildRefusal::EngineUnavailable));
        };
        let (prepared_tx, prepared_rx) = std::sync::mpsc::channel();
        let collector = sender.clone();
        let reader = context.reader;
        let now_ms = context.now_ms;
        let submitted = thread.submit(move || {
            let _guard = guard;
            let prepared = preflight(&plan, &programs, &docker, reader);
            let _ = prepared_tx.send(prepared.as_ref().map(|_| ()).map_err(|r| *r));
            if let Ok(prepared) = prepared {
                build_all(&prepared, &now_ms, &mut |report| {
                    let (request, _answer) = AuthorRequest::new(
                        AuthorCommand::ImageBuilt(Box::new(report)),
                        AUTHOR_WAIT,
                    );
                    let _ = collector.send(CollectorMsg::Author(request));
                });
            }
            on_done();
        });
        if !submitted {
            return Err(officina_core::ipc::IpcError::internal(
                "thread workshop-build has stopped",
            ));
        }
        Ok(match prepared_rx.recv() {
            Ok(Ok(())) => BuildAnswer::Queued,
            Ok(Err(refusal)) => BuildAnswer::Refused(refusal),
            Err(_) => BuildAnswer::Refused(BuildRefusal::EngineUnavailable),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_created_label_is_rfc3339_utc() {
            assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
            assert_eq!(rfc3339(1_793_491_200_000), "2026-11-01T00:00:00Z");
            assert_eq!(rfc3339(951_782_400_000 + 3_661_000), "2000-02-29T01:01:01Z");
        }

        #[test]
        fn a_context_is_written_fresh() {
            let dir = tempfile::tempdir().unwrap();
            let context = dir.path().join("ctx");
            std::fs::create_dir_all(&context).unwrap();
            std::fs::write(context.join("stale"), b"x").unwrap();
            write_context(&context, &[("ca/officina-ca.crt".into(), b"pem".to_vec())]).unwrap();
            assert!(!context.join("stale").exists());
            assert_eq!(
                std::fs::read(context.join("ca").join(CA_NAME)).unwrap(),
                b"pem"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs_in(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(GATEWAY_PROGRAM), b"gateway").unwrap();
        std::fs::write(dir.join(ENTRY_PROGRAM), b"entry").unwrap();
    }

    #[test]
    fn programs_come_from_the_exe_dir() {
        let exe = tempfile::tempdir().unwrap();
        programs_in(&exe.path().join(LINUX_DIR));
        let found = find_programs(Some(exe.path()), None).unwrap();
        assert_eq!(found.dir, exe.path().join(LINUX_DIR));
        assert_eq!(
            found.gateway,
            exe.path().join(LINUX_DIR).join(GATEWAY_PROGRAM)
        );
        assert_eq!(found.entry, exe.path().join(LINUX_DIR).join(ENTRY_PROGRAM));
    }

    #[test]
    fn the_override_wins() {
        let exe = tempfile::tempdir().unwrap();
        programs_in(&exe.path().join(LINUX_DIR));
        let other = tempfile::tempdir().unwrap();
        programs_in(other.path());
        let found = find_programs(Some(exe.path()), Some(other.path().as_os_str())).unwrap();
        assert_eq!(found.dir, other.path());
        let empty = std::ffi::OsString::new();
        assert_eq!(
            find_programs(Some(exe.path()), Some(&empty)).unwrap().dir,
            exe.path().join(LINUX_DIR),
            "an empty override is unset"
        );
    }

    #[test]
    fn missing_programs_refuse_the_build() {
        let exe = tempfile::tempdir().unwrap();
        assert_eq!(
            find_programs(Some(exe.path()), None),
            Err(BuildRefusal::LinuxProgramsMissing)
        );
        let linux = exe.path().join(LINUX_DIR);
        std::fs::create_dir_all(&linux).unwrap();
        std::fs::write(linux.join(GATEWAY_PROGRAM), b"g").unwrap();
        assert_eq!(
            find_programs(Some(exe.path()), None),
            Err(BuildRefusal::LinuxProgramsMissing),
            "one program alone"
        );
        std::fs::create_dir_all(linux.join(ENTRY_PROGRAM)).unwrap();
        assert_eq!(
            find_programs(Some(exe.path()), None),
            Err(BuildRefusal::LinuxProgramsMissing),
            "a folder is not a program"
        );
        assert_eq!(
            find_programs(None, None),
            Err(BuildRefusal::LinuxProgramsMissing)
        );
    }
}
