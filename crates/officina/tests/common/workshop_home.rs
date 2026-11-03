//! The workshop launch harness (story 6.11): the window's collector on a
//! home whose floor declares the workshop, a project in a real git
//! repository (D-23), the images built and a battery passed through the
//! fake engine (D-223), and cards issued and launched through the
//! functions the window's commands call, with a test credential store and
//! test readers (D-86's pattern).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use officina_core::orders::card::{CardAnswer, CardRequest, LaunchAnswer, WorkOrderCard};
use officina_core::workshop::battery::BatteryAnswer;
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::BuildAnswer;
use officina_core::workshop::launch::CredentialState;
use officina_core::workshop::proxy::PacAnswer;
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::paths::Paths;
use officina_host::spawn::BaseEnv;
use officina_host::watch::{AUTHOR_WAIT, AuthorCommand, WatchHandle, ask, spawn};
use officina_host::workshop::battery::{BatteryContext, run_battery};
use officina_host::workshop::build::{BuildContext, build_images};
use officina_host::workshop::launch::{CredentialStateSource, WorkshopRuntime};
use officina_host::workshop::proxy::{IeProxyConfig, SystemProxyReader, WinHttpReader};
use officina_host::workshop::threads::{BUILD_THREAD, JobThread};

use super::TestEnv;
use super::workshop::{FakeEngine, SeededStore, battery_sidecar};

/// The credential the test store holds: it must reach no argument, log,
/// journal line or registry row.
pub const SEEDED: &str = "seeded-credential-value-7f3a9c2e5b1d";

fn base_env() -> Vec<(String, String)> {
    vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )]
}

/// What a home's floor and fake engine hold.
#[derive(Debug, Clone)]
pub struct Options {
    pub auth: &'static str,
    pub upstream_proxy: &'static str,
    /// The project mounts `.claude`; else it mounts nothing.
    pub mount: bool,
    pub max_workers: u32,
    /// A battery runs after the build.
    pub battery: bool,
    /// The project's repository uses SHA-256 object ids.
    pub sha256: bool,
    /// Lines added to `[project.workshop]` (`review`, `sensitive_paths`).
    pub project_workshop: &'static str,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            auth: "oauth-token",
            upstream_proxy: "none",
            mount: true,
            max_workers: 2,
            battery: true,
            sha256: false,
            project_workshop: "",
        }
    }
}

/// The floor of `options`: one project, one workshop template with base
/// `main`.
fn floor(project: &str, options: &Options) -> String {
    let workshop = format!(
        "[workshop]\nauth = \"{}\"\nupstream_proxy = \"{}\"\nmax_workers = {}\n\n",
        options.auth, options.upstream_proxy, options.max_workers
    );
    let mounts = if options.mount { "\".claude\"" } else { "" };
    let extra = options.project_workshop;
    format!(
        "schema = 2\n{workshop}[[project]]\nname = \"api\"\npath = \"{project}\"\n\n[project.workshop]\nmounts = [{mounts}]\nhosts = [\"index.crates.io\"]\n{extra}\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"api\"]\nprompt = \"Bump {{package}}.\"\n\n[template.params.package]\nvalues = [\"serde_json\"]\n\n[template.workshop]\nbase = [\"main\"]\nmax_minutes = 30\n"
    )
}

/// What [`TestReader`] answers.
pub mod proxy_mode {
    pub const NONE: u8 = 0;
    pub const STATIC: u8 = 1;
    pub const OTHER_STATIC: u8 = 2;
    /// A PAC that cannot be evaluated.
    pub const PAC_FAILS: u8 = 3;
    /// A PAC naming a SOCKS proxy.
    pub const PAC_SOCKS: u8 = 4;
}

/// A proxy reader of the test's own, switchable between issue and launch.
#[derive(Debug, Default)]
pub struct TestReader {
    pub mode: AtomicU8,
}

impl SystemProxyReader for TestReader {
    fn config(&self) -> Result<IeProxyConfig, String> {
        let mode = self.mode.load(Ordering::SeqCst);
        Ok(IeProxyConfig {
            auto_detect: false,
            pac_url: matches!(mode, proxy_mode::PAC_FAILS | proxy_mode::PAC_SOCKS)
                .then(|| "http://wpad.example.com/proxy.pac".to_owned()),
            static_proxy: match mode {
                proxy_mode::STATIC => Some("proxy.example.com:8080".to_owned()),
                proxy_mode::OTHER_STATIC => Some("proxy2.example.com:3128".to_owned()),
                _ => None,
            },
            bypass: None,
        })
    }

    fn pac(&self, _url: &str, _config: &IeProxyConfig) -> PacAnswer {
        match self.mode.load(Ordering::SeqCst) {
            proxy_mode::PAC_SOCKS => PacAnswer::Answer("SOCKS socks.example.com:1080".to_owned()),
            _ => PacAnswer::Failed,
        }
    }

    fn evaluates_pac(&self) -> bool {
        true
    }
}

/// A credential state source answering `state` (D-607).
#[derive(Debug, Clone, Copy)]
pub struct FixedState(pub CredentialState);

impl CredentialStateSource for FixedState {
    fn state(&self, _floor_key: &str, _kind: CredentialKind) -> CredentialState {
        self.0
    }
}

pub struct Home {
    pub env: TestEnv,
    handle: Option<WatchHandle>,
    pub engine: FakeEngine,
    pub exe_dir: PathBuf,
    pub git: PathBuf,
    pub git_dir: PathBuf,
    pub reader: Arc<TestReader>,
}

impl Home {
    /// The home of `options`, built and its battery passed; none when this
    /// machine has no `git`.
    pub fn start(options: &Options) -> Option<Home> {
        let git_dir = super::git_dir_or_skip("workshop launch")?;
        let env = TestEnv::new();
        let project = env.root.path().join("work").join("Api");
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::write(project.join(".gitignore"), ".claude/\n").unwrap();
        std::fs::write(project.join("README.md"), "api\n").unwrap();
        std::fs::write(project.join(".claude").join("notes.md"), "local\n").unwrap();
        let format = if options.sha256 { "sha256" } else { "sha1" };
        super::git(
            &git_dir,
            &project,
            &["init", "-q", &format!("--object-format={format}")],
        );
        super::git(&git_dir, &project, &["add", "-A"]);
        super::git(&git_dir, &project, &["commit", "-q", "-m", "base"]);
        env.write_floor(&floor(
            &project.to_string_lossy().replace('\\', "/"),
            options,
        ));
        env.approve();
        env.write_agents("[]");
        let claude_sidecar = serde_json::json!({
            "version": "2.1.316 (Claude Code)",
            "fixture": env.fixture().to_string_lossy(),
            "log": env.fake_log().to_string_lossy(),
        });
        let claude = super::fake_claude_in(&env.root.path().join("bin"), &claude_sidecar);
        let paths = Paths {
            officina_home: env.officina_home(),
            claude_config_dir: env.claude_config(),
            claude_config_override: None,
            floor_file: None,
        };
        let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(claude))
            .with_base_env(BaseEnv::new(base_env()));
        let (board_sink, emitted) = super::sink(|_| {});
        let handle = spawn(collector, board_sink).unwrap();
        emitted
            .boards
            .recv_timeout(Duration::from_secs(10))
            .expect("first board");
        let exe_dir = env.root.path().join("exe");
        let linux = exe_dir.join("linux");
        std::fs::create_dir_all(&linux).unwrap();
        std::fs::write(linux.join("officina-entry"), b"entry").unwrap();
        std::fs::write(linux.join("officina-gateway"), b"gateway").unwrap();
        let kind = CredentialKind::from_code(options.auth).unwrap_or_default();
        let home = Home {
            env,
            handle: Some(handle),
            engine: FakeEngine::new(
                &battery_sidecar(kind, &serde_json::json!({})),
                &serde_json::json!({}),
            ),
            exe_dir,
            git: git_dir.join(super::git_support::GIT_EXE),
            git_dir,
            reader: Arc::new(TestReader::default()),
        };
        home.build();
        if options.battery {
            home.battery();
        }
        Some(home)
    }

    /// Sets sidecar key `key` of the fake engine, once the build and the
    /// battery ran (their own calls would match it too).
    pub fn set_engine(&self, key: &str, value: serde_json::Value) {
        let path = self.engine.dir.path().join("fake-docker.json");
        let mut sidecar: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        sidecar[key] = value;
        std::fs::write(&path, sidecar.to_string()).unwrap();
    }

    /// Replaces the fake engine's state key `key`.
    pub fn set_engine_state(&self, key: &str, value: serde_json::Value) {
        let path = self.engine.dir.path().join("fake-docker-state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        state[key] = value;
        std::fs::write(&path, state.to_string()).unwrap();
    }

    pub fn set_proxy(&self, mode: u8) {
        self.reader.mode.store(mode, Ordering::SeqCst);
    }

    pub fn sender(&self) -> &std::sync::mpsc::Sender<officina_host::watch::CollectorMsg> {
        &self.handle.as_ref().unwrap().sender
    }

    pub fn project(&self) -> PathBuf {
        self.env.root.path().join("work").join("Api")
    }

    /// Runs `git <args>` in the project.
    pub fn git(&self, args: &[&str]) {
        super::git(&self.git_dir, &self.project(), args);
    }

    pub fn settle(&self) {
        ask(self.sender(), AuthorCommand::ListDiscarded, AUTHOR_WAIT).expect("an answer");
    }

    fn build(&self) {
        assert_eq!(self.build_answer(), BuildAnswer::Queued);
    }

    /// What a Build images click answers; once queued, the build ran.
    pub fn build_answer(&self) -> BuildAnswer {
        let thread = JobThread::spawn(BUILD_THREAD).unwrap();
        let (done, finished) = std::sync::mpsc::channel();
        let answer = build_images(
            self.sender(),
            &thread,
            BuildContext {
                exe_dir: Some(self.exe_dir.clone()),
                linux_override: None,
                docker: Ok(self.engine.docker()),
                reader: Arc::new(WinHttpReader),
                busy: Arc::new(AtomicBool::new(false)),
                now_ms: officina_host::files::now_ms,
            },
            Box::new(move || {
                let _ = done.send(());
            }),
        )
        .expect("an answer");
        if answer == BuildAnswer::Queued {
            finished.recv_timeout(Duration::from_secs(60)).unwrap();
            self.settle();
        }
        answer
    }

    fn battery(&self) {
        let answer = self.battery_answer();
        assert!(matches!(answer, BatteryAnswer::Queued(_)), "{answer:?}");
    }

    /// What a Run escape battery click answers; once queued, it ran.
    pub fn battery_answer(&self) -> BatteryAnswer {
        let thread = JobThread::spawn(BUILD_THREAD).unwrap();
        let (done, finished) = std::sync::mpsc::channel();
        let answer = run_battery(
            self.sender(),
            &thread,
            BatteryContext {
                docker: Ok(self.engine.docker()),
                busy: Arc::new(AtomicBool::new(false)),
                now_ms: officina_host::files::now_ms,
            },
            None,
            Box::new(move || {
                let _ = done.send(());
            }),
        )
        .expect("an answer");
        if matches!(answer, BatteryAnswer::Queued(_)) {
            finished.recv_timeout(Duration::from_secs(60)).unwrap();
            self.settle();
        }
        answer
    }

    pub fn runtime(&self, store: SeededStore) -> WorkshopRuntime {
        let mut runtime = super::workshop::runtime(
            Some(self.engine.docker()),
            Some(self.git.clone()),
            store,
            self.exe_dir.clone(),
            None,
        );
        runtime.reader = Arc::clone(&self.reader) as Arc<dyn SystemProxyReader>;
        runtime
    }

    /// The runtime with the seeded credential.
    pub fn seeded(&self) -> WorkshopRuntime {
        self.runtime(SeededStore(Some(SEEDED.to_owned())))
    }

    /// What `issue_card` answers for the template.
    pub fn issue_answer(&self, runtime: &WorkshopRuntime) -> CardAnswer {
        let request = CardRequest {
            template: "bump".to_owned(),
            project: "api".to_owned(),
            values: vec![("package".to_owned(), "serde_json".to_owned())],
            subject: None,
        };
        officina_host::orders::issue_card(self.sender(), request, runtime).unwrap()
    }

    pub fn issue(&self, runtime: &WorkshopRuntime) -> WorkOrderCard {
        match self.issue_answer(runtime) {
            CardAnswer::Issued(card) => *card,
            refused => panic!("{refused:?}"),
        }
    }

    /// What `launch_work_order` answers for `card_id`, the hashes given.
    pub fn launch_with(
        &self,
        card_id: &str,
        prompt_sha256: &str,
        plan_sha256: Option<&str>,
        runtime: &WorkshopRuntime,
    ) -> LaunchAnswer {
        let answer = officina_host::orders::launch_card(
            self.sender(),
            card_id,
            prompt_sha256,
            plan_sha256,
            runtime,
        )
        .unwrap();
        self.settle();
        answer
    }

    /// The click on `card`'s Launch: its own hashes.
    pub fn launch(&self, card: &WorkOrderCard, runtime: &WorkshopRuntime) -> LaunchAnswer {
        let plan = card.workshop.as_ref().and_then(|w| w.plan_sha256.clone());
        self.launch_with(&card.id, &card.prompt_sha256, plan.as_deref(), runtime)
    }

    /// The engine calls logged since `from`, as their argument vectors.
    pub fn calls(&self, from: usize) -> Vec<Vec<String>> {
        self.engine.log()[from..]
            .iter()
            .map(|line| {
                line["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_owned())
                    .collect()
            })
            .collect()
    }

    pub fn count(&self, from: usize, prefix: &[&str]) -> usize {
        self.calls(from)
            .iter()
            .filter(|argv| argv.len() >= prefix.len() && argv[..prefix.len()] == *prefix)
            .count()
    }

    pub fn run(&self, id: &str) -> officina_host::registry::workshop::RunRecord {
        officina_host::registry::store::open_reader(&self.env.officina_home(), &[])
            .unwrap()
            .workshop_run(id)
            .unwrap()
            .expect("the run's row")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle
                .sender
                .send(officina_host::watch::CollectorMsg::Shutdown);
            let _ = handle.thread.join();
        }
    }
}

/// Every call that creates, starts or copies (AD-107).
pub const CHANGES: &[&[&str]] = &[
    &["create"],
    &["start"],
    &["network", "create"],
    &["network", "connect"],
    &["volume", "create"],
    &["cp"],
];

/// How many changing calls the engine logged since `from`.
pub fn changes(home: &Home, from: usize) -> usize {
    CHANGES.iter().map(|p| home.count(from, p)).sum()
}
