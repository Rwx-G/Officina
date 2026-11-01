//! Build images end to end with the fake engine (story 6.8 AC 3 to 5,
//! AD-99, AD-100, D-205, D-208, D-233, D-611, D-1100): the window's
//! collector grants the click, thread `workshop-build` checks the engine
//! and the files, writes the contexts and builds in AD-99's order, measures
//! the programs through a container it never starts, and the collector
//! records each image and journals it.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::{
    BuildAnswer, BuildOutcome, BuildRefusal, GATEWAY_IMAGE, LABEL_BASE, LABEL_CA,
    NO_PROXY_BUILD_ARG, PROXY_BUILD_ARGS, RESERVED_ENV_PREFIXES, WORKSHOP_BASE,
};
use officina_core::workshop::inventory::StoredInventory;
use officina_core::workshop::names::project_image;
use officina_core::workshop::vector::{credential_name, reserved_env_names};
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::paths::Paths;
use officina_host::registry::workshop::{ImageRecord, InventoryRecord};
use officina_host::spawn::BaseEnv;
use officina_host::watch::{AUTHOR_WAIT, AuthorCommand, WatchHandle, ask, spawn};
use officina_host::workshop::build::{BuildContext, CA_NAME as CA_NAME_IN_CONTEXT, build_images};
use officina_host::workshop::inventory::{
    InventoryEngine, InventoryFailure, InventoryProject, LaunchInventory, for_launch, workshop_git,
};
use officina_host::workshop::proxy::WinHttpReader;
use officina_host::workshop::threads::{BUILD_THREAD, JobThread};

use common::TestEnv;
use common::workshop::FakeEngine;

const PROJECT_DOCKERFILE: &str = "FROM officina-workshop:local\nRUN true\n";
const ENTRY: &[u8] = b"the entry point's bytes";
const GATEWAY: &[u8] = b"the gateway's bytes";
const CA: &str = "-----BEGIN CERTIFICATE-----\nTUlJQmV4YW1wbGU=\n-----END CERTIFICATE-----\n";

struct Options {
    upstream_proxy: &'static str,
    ca: bool,
    dockerfile: &'static str,
    programs: bool,
    /// Projects `api` (a Dockerfile, a mounted, git-ignored `.claude`) and
    /// `web` (no Dockerfile, tracked settings and a tracked guard script)
    /// in git repositories, and a workshop template naming both with base
    /// `main` (story 6.9).
    inventory: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            upstream_proxy: "none",
            ca: false,
            dockerfile: PROJECT_DOCKERFILE,
            programs: true,
            inventory: false,
        }
    }
}

/// `api`'s mounted guard (story 6.9).
const API_GUARD: &str = "python \"$CLAUDE_PROJECT_DIR/.claude/hooks/guard.py\"";
/// `web`'s tracked guard script.
const WEB_GUARD: &str = "$CLAUDE_PROJECT_DIR/tools/guard.sh";

fn hooks_json(event: &str, command: &str) -> String {
    serde_json::json!({
        "hooks": { event: [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": command }] }] }
    })
    .to_string()
}

/// Makes `dir` a repository on branch `main` holding every file in it.
fn commit_all(dir: &std::path::Path) {
    let git = common::git_dir_or_skip("workshop inventory").expect("git on PATH");
    common::git(&git, dir, &["init", "-q"]);
    common::git(&git, dir, &["add", "-A"]);
    common::git(&git, dir, &["commit", "-q", "-m", "base"]);
}

/// The tracked repositories of [`Options::inventory`].
fn inventory_projects(root: &std::path::Path) -> (PathBuf, PathBuf) {
    let api = root.join("work").join("Api");
    std::fs::write(api.join(".gitignore"), ".claude/\n").unwrap();
    let hooks = api.join(".claude").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("guard.py"), "print('ok')\n").unwrap();
    std::fs::write(
        api.join(".claude").join("settings.json"),
        hooks_json("PreToolUse", API_GUARD),
    )
    .unwrap();
    commit_all(&api);
    let web = root.join("work").join("Web");
    std::fs::create_dir_all(web.join(".claude")).unwrap();
    std::fs::create_dir_all(web.join("tools")).unwrap();
    std::fs::write(
        web.join(".claude").join("settings.json"),
        hooks_json("PreToolUse", WEB_GUARD),
    )
    .unwrap();
    std::fs::write(web.join("tools").join("guard.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    commit_all(&web);
    let git = common::git_dir_or_skip("workshop inventory").expect("git on PATH");
    common::git(
        &git,
        &web,
        &["update-index", "--chmod=+x", "tools/guard.sh"],
    );
    common::git(&git, &web, &["commit", "-q", "-m", "executable"]);
    (api, web)
}

/// A home with a workshop floor approved, project `api` with a Dockerfile,
/// the window's collector running on it and the fake engine.
struct BuildHome {
    env: TestEnv,
    handle: Option<WatchHandle>,
    engine: FakeEngine,
    exe_dir: PathBuf,
    thread: JobThread,
    busy: Arc<AtomicBool>,
}

impl BuildHome {
    fn start(options: &Options, sidecar: &serde_json::Value) -> BuildHome {
        let env = TestEnv::new();
        let project = env.root.path().join("work").join("Api");
        // A context holds no subfolder (AD-101): beside a repository, the
        // Dockerfile sits in a folder of its own.
        let dockerfile = if options.inventory {
            "docker/Dockerfile"
        } else {
            "Dockerfile"
        };
        let context = project.join(dockerfile).parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&context).unwrap();
        std::fs::write(context.join("Dockerfile"), options.dockerfile).unwrap();
        std::fs::write(context.join("requirements.txt"), "pytest\n").unwrap();
        let ca = env.root.path().join("ca.pem");
        std::fs::write(&ca, CA).unwrap();
        let ca_line = if options.ca {
            format!("ca_file = '{}'\n", ca.display())
        } else {
            String::new()
        };
        let mut floor = format!(
            "schema = 2\n[workshop]\nauth = \"oauth-token\"\nupstream_proxy = \"{}\"\n{ca_line}[[project]]\nname = \"api\"\npath = '{}'\n[project.workshop]\ndockerfile = \"{dockerfile}\"\n",
            options.upstream_proxy,
            project.display()
        );
        if options.inventory {
            let (_, web) = inventory_projects(env.root.path());
            floor.push_str(&format!(
                "mounts = [\".claude\"]\n[[project]]\nname = \"web\"\npath = '{}'\n[project.workshop]\n[[template]]\nname = \"tidy\"\ndescription = \"Tidy\"\nprojects = [\"api\", \"web\"]\nprompt = \"Tidy the code.\"\n[template.workshop]\nbase = [\"main\"]\nmax_minutes = 30\n",
                web.display()
            ));
        }
        env.write_floor(&floor);
        env.approve();
        env.write_agents("[]");
        let claude_sidecar = serde_json::json!({
            "version": "2.1.316 (Claude Code)",
            "fixture": env.fixture().to_string_lossy(),
            "log": env.fake_log().to_string_lossy(),
        });
        let claude = common::fake_claude_in(&env.root.path().join("bin"), &claude_sidecar);
        let paths = Paths {
            officina_home: env.officina_home(),
            claude_config_dir: env.claude_config(),
            claude_config_override: None,
            floor_file: None,
        };
        let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(claude))
            .with_base_env(BaseEnv::new(vec![(
                "SYSTEMROOT".to_owned(),
                std::env::var("SYSTEMROOT").unwrap_or_default(),
            )]));
        let (board_sink, emitted) = common::sink(|_| {});
        let handle = spawn(collector, board_sink).unwrap();
        emitted
            .boards
            .recv_timeout(Duration::from_secs(10))
            .expect("first board");
        let exe_dir = env.root.path().join("exe");
        if options.programs {
            let linux = exe_dir.join("linux");
            std::fs::create_dir_all(&linux).unwrap();
            std::fs::write(linux.join("officina-entry"), ENTRY).unwrap();
            std::fs::write(linux.join("officina-gateway"), GATEWAY).unwrap();
        }
        let engine = FakeEngine::new(sidecar, &serde_json::json!({}));
        BuildHome {
            env,
            handle: Some(handle),
            engine,
            exe_dir,
            thread: JobThread::spawn(BUILD_THREAD).unwrap(),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    fn handle(&self) -> &WatchHandle {
        self.handle.as_ref().unwrap()
    }

    fn set_sidecar(&self, sidecar: &serde_json::Value) {
        std::fs::write(
            self.engine.dir.path().join("fake-docker.json"),
            sidecar.to_string(),
        )
        .unwrap();
    }

    /// Clicks Build images and waits for every image's record: the
    /// collector handles requests in order, so a read answered after the
    /// job's end follows every report.
    fn build(&self) -> BuildAnswer {
        for _ in 0..100 {
            let (done, finished) = std::sync::mpsc::channel();
            let context = BuildContext {
                exe_dir: Some(self.exe_dir.clone()),
                linux_override: None,
                docker: Ok(self.engine.docker()),
                reader: Arc::new(WinHttpReader),
                busy: Arc::clone(&self.busy),
                now_ms: officina_host::files::now_ms,
            };
            let answer = build_images(
                &self.handle().sender,
                &self.thread,
                context,
                Box::new(move || {
                    let _ = done.send(());
                }),
            )
            .expect("the window answers");
            // The collector reads `claude --version` on its first pass.
            if answer == BuildAnswer::Refused(BuildRefusal::ClaudeVersionUnknown) {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            if answer == BuildAnswer::Queued {
                finished
                    .recv_timeout(Duration::from_secs(60))
                    .expect("the build job ends");
            }
            ask(
                &self.handle().sender,
                AuthorCommand::ListDiscarded,
                AUTHOR_WAIT,
            )
            .expect("the collector answers");
            return answer;
        }
        panic!("the collector never read the Claude Code version");
    }

    fn inventory(&self, image: &str, project: &str) -> Option<InventoryRecord> {
        officina_host::registry::store::open_reader(&self.env.officina_home(), &[])
            .expect("a registry")
            .workshop_inventory(image, project)
            .expect("a read")
    }

    fn floor_key(&self) -> String {
        officina_core::workshop::names::floor_key(&self.env.floor_file().to_string_lossy())
    }

    fn inventory_engine(&self) -> InventoryEngine {
        InventoryEngine {
            docker: self.engine.docker(),
            base_env: base_env(),
            home: self.env.officina_home(),
            floor_key: self.floor_key(),
        }
    }

    /// What a launch of `web` on the base image would judge (AD-118).
    fn web_at_launch(&self) -> Result<LaunchInventory, InventoryFailure> {
        let home = self.env.officina_home();
        let git = workshop_git(&home, &base_env()).expect("the workshop's git");
        let web = InventoryProject {
            name: "web".into(),
            path: self.env.root.path().join("work").join("Web"),
            mounts: Vec::new(),
        };
        let base = self.image(WORKSHOP_BASE).unwrap().image_id.unwrap();
        let commit = git.branch_commit(&web.path, "main").unwrap();
        let stored = self.inventory(WORKSHOP_BASE, "web");
        for_launch(
            &self.inventory_engine(),
            &git,
            (WORKSHOP_BASE, &base),
            &web,
            &commit,
            stored.as_ref(),
            officina_host::files::now_ms(),
        )
    }

    fn image(&self, name: &str) -> Option<ImageRecord> {
        officina_host::registry::store::open_reader(&self.env.officina_home(), &[])
            .expect("a registry")
            .workshop_image(name)
            .expect("a read")
    }

    fn calls(&self, first: &str) -> Vec<Vec<String>> {
        self.engine
            .log()
            .iter()
            .map(|line| {
                line["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_owned())
                    .collect::<Vec<String>>()
            })
            .filter(|argv| argv.first().map(String::as_str) == Some(first))
            .collect()
    }

    fn builds(&self) -> Vec<Vec<String>> {
        self.calls("build")
    }

    fn built_lines(&self) -> Vec<officina_core::relay::journal::WorkshopImageBuilt> {
        self.env
            .journal()
            .into_iter()
            .filter_map(|line| match line.entry {
                JournalEntry::WorkshopImageBuilt(built) => Some(built),
                _ => None,
            })
            .collect()
    }

    fn project_tag(&self) -> String {
        let floor = self.env.floor_file();
        project_image(
            &officina_core::workshop::names::floor_key(&floor.to_string_lossy()),
            "api",
        )
    }
}

impl Drop for BuildHome {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle
                .sender
                .send(officina_host::watch::CollectorMsg::Shutdown);
            let _ = handle.thread.join();
        }
    }
}

fn base_env() -> Vec<(String, String)> {
    vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )]
}

fn tag_of(argv: &[String]) -> &str {
    argv.windows(2)
        .find(|w| w[0] == "--tag")
        .map(|w| w[1].as_str())
        .unwrap()
}

fn values<'a>(argv: &'a [String], flag: &str) -> Vec<&'a str> {
    argv.windows(2)
        .filter(|w| w[0] == flag)
        .map(|w| w[1].as_str())
        .collect()
}

fn label<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
    values(argv, "--label")
        .into_iter()
        .find_map(|l| l.strip_prefix(&format!("{name}=")))
}

#[test]
fn builds_run_in_order_with_their_labels() {
    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let builds = home.builds();
    let tags: Vec<&str> = builds.iter().map(|b| tag_of(b)).collect();
    let project = home.project_tag();
    assert_eq!(tags, [GATEWAY_IMAGE, WORKSHOP_BASE, project.as_str()]);
    let entry = officina_core::sha256_hex(ENTRY);
    let gateway = officina_core::sha256_hex(GATEWAY);
    for build in &builds {
        assert_eq!(label(build, "officina.entry.sha256"), Some(entry.as_str()));
        assert_eq!(
            label(build, "officina.gateway.sha256"),
            Some(gateway.as_str())
        );
        assert_eq!(label(build, LABEL_CA), Some("none"));
        assert_eq!(label(build, "officina.claude.version"), Some("2.1.316"));
        assert!(build.iter().any(|a| a == "--network") && build.iter().any(|a| a == "default"));
    }
    assert_eq!(label(&builds[0], LABEL_BASE), Some("gateway"));
    assert_eq!(label(&builds[1], LABEL_BASE), Some("workshop"));
    assert_eq!(label(&builds[2], LABEL_BASE), None, "inherited, never set");
    assert_eq!(label(&builds[2], "officina.project"), Some("api"));
    assert_eq!(
        values(&builds[1], "--build-arg"),
        ["CLAUDE_CODE_VERSION=2.1.316"]
    );
    assert!(values(&builds[2], "--build-arg").is_empty());
    // D-1100: every build runs without buildx's git provenance.
    for line in home.engine.log() {
        let names: Vec<&str> = line["env_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let is_build = line["argv"][0] == "build";
        assert_eq!(names.contains(&"BUILDX_GIT_INFO"), is_build, "{line}");
    }
    let base = home.image(WORKSHOP_BASE).expect("the base's row");
    let built_project = home.image(&project).expect("the project's row");
    assert_eq!(base.outcome, BuildOutcome::Built);
    assert_eq!(built_project.outcome, BuildOutcome::Built);
    assert!(base.programs_sha256.is_some());
    assert_eq!(built_project.programs_sha256, base.programs_sha256);
    assert!(
        built_project
            .labels_json
            .as_deref()
            .unwrap()
            .contains(&format!(
                "\"officina.base.id\":\"{}\"",
                base.image_id.unwrap()
            ))
    );
    assert!(base.env_names_json.unwrap().contains("PATH"));
    let gateway_row = home.image(GATEWAY_IMAGE).expect("the gateway's row");
    assert_eq!(gateway_row.outcome, BuildOutcome::Built);
    assert_eq!(gateway_row.programs_sha256, None, "nothing to measure");
    let lines = home.built_lines();
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| l.outcome == BuildOutcome::Built));
    // The contexts are written under the home, the project's from the
    // approved bytes.
    let build_dir = home.env.officina_home().join("workshop").join("build");
    let project_dir = build_dir.join(project.trim_end_matches(":local"));
    assert_eq!(
        std::fs::read_to_string(project_dir.join("Dockerfile")).unwrap(),
        PROJECT_DOCKERFILE
    );
    assert!(project_dir.join("requirements.txt").is_file());
    assert_eq!(
        std::fs::read(build_dir.join("officina-workshop").join("officina-entry")).unwrap(),
        ENTRY
    );
}

#[test]
fn a_project_image_not_from_the_base_is_removed() {
    let other_base = Options {
        dockerfile: "FROM debian:trixie-slim\nRUN true\n",
        ..Options::default()
    };
    let home = BuildHome::start(&other_base, &serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let project = home.project_tag();
    // Phase 2 reads the same stages (AD-101): the project's workshop
    // leaves the catalogue, so nothing of it is built; the build's own
    // check of the approved bytes is the second line behind it.
    let tags: Vec<String> = home.builds().iter().map(|b| tag_of(b).to_owned()).collect();
    assert_eq!(tags, [GATEWAY_IMAGE, WORKSHOP_BASE]);
    assert!(home.image(&project).is_none());

    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    let project = home.project_tag();
    home.set_sidecar(&serde_json::json!({"build": {project.clone(): {"drop_base_label": true}}}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    assert_eq!(
        home.image(&project).unwrap().outcome,
        BuildOutcome::NotFromBase
    );
    assert!(
        home.calls("image")
            .iter()
            .any(|c| c[1] == "rm" && c[2] == project),
        "the image is removed"
    );
}

#[test]
fn the_ca_file_reaches_every_context() {
    let home = BuildHome::start(
        &Options {
            ca: true,
            ..Options::default()
        },
        &serde_json::json!({}),
    );
    assert_eq!(home.build(), BuildAnswer::Queued);
    let build_dir = home.env.officina_home().join("workshop").join("build");
    assert_eq!(
        std::fs::read_to_string(
            build_dir
                .join("officina-workshop")
                .join("ca")
                .join(CA_NAME_IN_CONTEXT)
        )
        .unwrap(),
        CA
    );
    assert!(
        !build_dir.join("officina-gateway").join("ca").exists(),
        "the gateway holds no CA (D-612)"
    );
    let digest = officina_core::sha256_hex(CA.as_bytes());
    for build in home.builds() {
        assert_eq!(label(&build, LABEL_CA), Some(digest.as_str()), "{build:?}");
    }
}

#[test]
fn a_resolved_proxy_reaches_every_build_as_build_args() {
    let home = BuildHome::start(
        &Options {
            upstream_proxy: "proxy.example.com:8080",
            ..Options::default()
        },
        &serde_json::json!({}),
    );
    assert_eq!(home.build(), BuildAnswer::Queued);
    let builds = home.builds();
    assert_eq!(builds.len(), 3);
    for build in &builds {
        let args = values(build, "--build-arg");
        for name in PROXY_BUILD_ARGS {
            assert!(
                args.contains(&format!("{name}=http://proxy.example.com:8080").as_str()),
                "{build:?}"
            );
        }
        assert!(
            args.iter()
                .any(|a| a.starts_with(&format!("{NO_PROXY_BUILD_ARG}=")))
        );
    }
}

#[test]
fn no_route_means_no_proxy_build_arg() {
    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    for build in home.builds() {
        for arg in values(&build, "--build-arg") {
            assert!(
                !PROXY_BUILD_ARGS
                    .iter()
                    .any(|n| arg.starts_with(&format!("{n}=")))
                    && !arg.starts_with(NO_PROXY_BUILD_ARG),
                "{arg}"
            );
        }
    }
}

#[test]
fn builds_name_the_docker_driver_builder() {
    let builder = |name: &str, driver: &str, current: bool, endpoint: &str| {
        serde_json::json!({"Name": name, "Driver": driver, "Current": current,
                           "Nodes": [{"Endpoint": endpoint, "Name": name}]})
    };
    let home = BuildHome::start(
        &Options::default(),
        &serde_json::json!({"builders": [
            builder("desktop-linux", "docker", false, "desktop-linux"),
            builder("mybuilder", "docker-container", true, "desktop-linux"),
        ]}),
    );
    assert_eq!(home.build(), BuildAnswer::Queued);
    for build in home.builds() {
        assert_eq!(values(&build, "--builder"), ["desktop-linux"]);
    }
    let home = BuildHome::start(
        &Options::default(),
        &serde_json::json!({"builders": [
            builder("remote", "remote", true, "tcp://buildkit.example.com:1234"),
        ]}),
    );
    assert_eq!(
        home.build(),
        BuildAnswer::Refused(BuildRefusal::BuilderMissing)
    );
    assert!(home.builds().is_empty());
}

#[test]
fn nothing_builds_while_stopped() {
    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    let flag = home.env.officina_home().join("workshop").join("stopped");
    std::fs::create_dir_all(flag.parent().unwrap()).unwrap();
    std::fs::write(&flag, "{}").unwrap();
    assert_eq!(
        home.build(),
        BuildAnswer::Refused(BuildRefusal::WorkshopStopped)
    );
    assert!(home.engine.log().is_empty(), "{:?}", home.engine.log());
    assert!(home.image(WORKSHOP_BASE).is_none());
}

#[test]
fn missing_linux_programs_refuse_the_build() {
    let home = BuildHome::start(
        &Options {
            programs: false,
            ..Options::default()
        },
        &serde_json::json!({}),
    );
    assert_eq!(
        home.build(),
        BuildAnswer::Refused(BuildRefusal::LinuxProgramsMissing)
    );
    assert!(home.engine.log().is_empty());
}

/// Builds with the project image replacing `path` and asserts the refusal.
fn replaced_program_is_refused(path: &str) {
    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    let project = home.project_tag();
    home.set_sidecar(&serde_json::json!({"build": {project.clone(): {"replace": {path: "a program of the project's"}}}}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let row = home.image(&project).unwrap();
    assert_eq!(row.outcome, BuildOutcome::NotFromBase);
    assert_eq!(row.image_id, None);
    assert_eq!(
        home.built_lines().pop().unwrap().detail,
        Some(officina_core::workshop::image::NotFromBase::ProgramsReplaced)
    );
    assert!(
        home.calls("image")
            .iter()
            .any(|c| c[1] == "rm" && c[2] == project)
    );
}

#[test]
fn a_project_image_replacing_the_entry_point_is_refused() {
    replaced_program_is_refused("/opt/officina/bin/officina-entry");
}

#[test]
fn a_project_image_replacing_claude_is_refused() {
    replaced_program_is_refused("/opt/officina/bin/claude");
}

/// Builds once per name, the project image setting it, and asserts each
/// refusal.
fn reserved_names_are_refused(names: &[String]) {
    assert!(!names.is_empty());
    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    let project = home.project_tag();
    for name in names {
        home.set_sidecar(
            &serde_json::json!({"build": {project.clone(): {"env": [format!("{name}=x")]}}}),
        );
        assert_eq!(home.build(), BuildAnswer::Queued, "{name}");
        let row = home.image(&project).unwrap();
        assert_eq!(row.outcome, BuildOutcome::NotFromBase, "{name}");
        assert_eq!(
            home.built_lines().pop().unwrap().detail,
            Some(officina_core::workshop::image::NotFromBase::ReservedEnv),
            "{name}"
        );
    }
}

#[test]
fn a_project_image_setting_a_credential_name_is_refused() {
    let mut names: Vec<String> = CredentialKind::ALL
        .iter()
        .filter_map(|k| credential_name(*k))
        .map(str::to_owned)
        .collect();
    names.extend(RESERVED_ENV_PREFIXES.iter().map(|p| format!("{p}BASE_URL")));
    reserved_names_are_refused(&names);
}

#[test]
fn a_project_image_setting_a_vector_name_is_refused() {
    reserved_names_are_refused(&reserved_env_names());
}

#[test]
fn the_measuring_container_is_never_started() {
    let home = BuildHome::start(&Options::default(), &serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let creates = home.calls("create");
    assert_eq!(creates.len(), 2, "the base and the project measured");
    for create in &creates {
        assert!(
            create
                .windows(2)
                .any(|w| w[0] == "--network" && w[1] == "none")
        );
        assert!(create[2].starts_with("officina-measure-"));
    }
    assert!(home.calls("start").is_empty(), "nothing is started");
    let copies = home.calls("cp");
    assert_eq!(copies.len(), 4, "two programs per image");
    let removed: Vec<String> = home
        .calls("rm")
        .into_iter()
        .flat_map(|c| c.into_iter().skip(1))
        .collect();
    for create in &creates {
        assert!(removed.contains(&create[2]), "{} removed", create[2]);
    }
    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.engine.dir.path().join("fake-docker-state.json")).unwrap(),
    )
    .unwrap();
    assert!(
        state["containers"].as_array().is_none_or(|c| c.is_empty()),
        "{state}"
    );
}

fn inventory_home(sidecar: &serde_json::Value) -> BuildHome {
    BuildHome::start(
        &Options {
            inventory: true,
            ..Options::default()
        },
        sidecar,
    )
}

fn stored(record: &InventoryRecord) -> StoredInventory {
    serde_json::from_str(&record.inventory_json).unwrap()
}

/// Story 6.9 AC 3: after the builds, `api`'s inventory runs on its project
/// image and `web`'s on the base, each attached with no network, the list
/// on stdin, the container removed; each is recorded with its key.
#[test]
fn an_inventory_follows_each_build_per_project() {
    let home = inventory_home(&serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let project_tag = home.project_tag();
    let api = home
        .inventory(&project_tag, "api")
        .expect("api's inventory on its image");
    let project_id = home.image(&project_tag).unwrap().image_id.unwrap();
    assert_eq!(api.image_id, project_id);
    let api_stored = stored(&api);
    assert_eq!(api_stored.commands[0].command, API_GUARD);
    assert!(api_stored.consequences().refused.is_empty());
    let web = home
        .inventory(WORKSHOP_BASE, "web")
        .expect("web's inventory on the base");
    let web_stored = stored(&web);
    let tracked = web_stored.commands[0].tracked.as_ref().expect("tracked");
    assert!(tracked.present && tracked.executable, "{tracked:?}");
    assert_eq!(tracked.first_line.as_deref(), Some("#!/bin/sh"));
    let creates: Vec<Vec<String>> = home
        .calls("create")
        .into_iter()
        .filter(|c| c.iter().any(|a| a == "officina.role=inventory"))
        .collect();
    assert_eq!(creates.len(), 2, "one per project");
    for create in &creates {
        assert!(
            create
                .windows(2)
                .any(|w| w[0] == "--network" && w[1] == "none")
        );
        assert!(create.iter().any(|a| a == "--interactive"));
    }
    let api_create = creates
        .iter()
        .find(|c| c.iter().any(|a| a.ends_with("-api")))
        .unwrap();
    assert!(
        api_create
            .windows(2)
            .any(|w| w[0] == "--mount" && w[1].contains("readonly")),
        "the mounted .claude, read-only: {api_create:?}"
    );
    let starts = home.calls("start");
    assert_eq!(starts.len(), 2);
    assert!(starts.iter().all(|s| s.contains(&"--attach".to_owned())));
}

/// Story 6.9 AC 4 through the build: a missing guard program is recorded
/// and refuses.
#[test]
fn a_missing_guard_is_recorded_and_refuses() {
    let home = inventory_home(&serde_json::json!({
        "inventory": { API_GUARD: { "resolution": "missing", "detail": "not_on_path" } }
    }));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let api = home.inventory(&home.project_tag(), "api").unwrap();
    assert_eq!(stored(&api).consequences().refused, vec![0]);
}

/// A garbled answer records nothing, so the launch runs the inventory.
#[test]
fn a_garbled_inventory_records_nothing() {
    let home = inventory_home(&serde_json::json!({ "inventory_output": "{\"index\": 0}\n" }));
    assert_eq!(home.build(), BuildAnswer::Queued);
    assert!(home.inventory(&home.project_tag(), "api").is_none());
    assert!(home.image(&home.project_tag()).unwrap().image_id.is_some());
}

#[test]
fn an_unchanged_key_does_not_rerun() {
    let home = inventory_home(&serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let starts = home.calls("start").len();
    let launch = home.web_at_launch().unwrap();
    assert!(launch.fresh.is_none());
    assert_eq!(
        launch.inventory,
        stored(&home.inventory(WORKSHOP_BASE, "web").unwrap())
    );
    assert_eq!(home.calls("start").len(), starts, "no container started");
}

#[test]
fn a_changed_hook_reruns_the_inventory_at_launch() {
    let home = inventory_home(&serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let before = home.inventory(WORKSHOP_BASE, "web").unwrap();
    let web = home.env.root.path().join("work").join("Web");
    std::fs::write(web.join("tools").join("guard.sh"), "#!/bin/bash\nexit 0\n").unwrap();
    let git = common::git_dir_or_skip("workshop inventory").unwrap();
    common::git(&git, &web, &["commit", "-q", "-am", "edit the guard"]);
    let starts = home.calls("start").len();
    let launch = home.web_at_launch().unwrap();
    let fresh = launch.fresh.expect("the inventory ran again");
    assert_ne!(fresh.inventory_key, before.inventory_key);
    assert_eq!(home.calls("start").len(), starts + 1);
    assert_eq!(
        launch.inventory.commands[0]
            .tracked
            .as_ref()
            .unwrap()
            .first_line
            .as_deref(),
        Some("#!/bin/bash")
    );
}

#[test]
fn no_inventory_runs_while_stopped() {
    let home = inventory_home(&serde_json::json!({}));
    assert_eq!(home.build(), BuildAnswer::Queued);
    let web = home.env.root.path().join("work").join("Web");
    std::fs::write(web.join("tools").join("guard.sh"), "#!/bin/bash\n").unwrap();
    let git = common::git_dir_or_skip("workshop inventory").unwrap();
    common::git(&git, &web, &["commit", "-q", "-am", "edit"]);
    let flag = home.env.officina_home().join("workshop").join("stopped");
    std::fs::write(&flag, "{}").unwrap();
    let calls = home.engine.log().len();
    assert_eq!(home.web_at_launch(), Err(InventoryFailure::Stopped));
    assert_eq!(home.engine.log().len(), calls, "no engine call at all");
}
