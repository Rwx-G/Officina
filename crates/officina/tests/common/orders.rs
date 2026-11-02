//! The work order harness of the CLI tests (story 5.7): a home whose
//! floor holds a template in a project whose folder exists, approved
//! through the window's authority, and the window's collector thread on
//! it, its `claude` a copy of the fake configured by its sidecar (D-86).
//! Cards are issued, edited and launched through the functions the
//! window's commands call, so the tests drive the window's path without
//! Tauri.

use std::path::PathBuf;
use std::time::Duration;

use officina_core::orders::card::{
    CardAnswer, CardChange, CardRequest, LaunchAnswer, WorkOrderCard,
};
use officina_core::orders::proposal::{
    ProposalFile, ProposalView, ProposedSubject, WorkOrderProposal,
};
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::paths::Paths;
use officina_host::spawn::BaseEnv;
use officina_host::watch::{AUTHOR_WAIT, AuthorCommand, CollectorMsg, WatchHandle, ask, spawn};

use super::{Emitted, TestEnv, fake_claude_calls, fake_claude_in, sink};

/// A floor with one project, `{project}` standing for its folder, and two
/// templates, one with a model and one without.
pub const FLOOR: &str = r#"schema = 2
[[project]]
name = "backend"
path = "{project}"
[[template]]
name = "bump"
description = "Bump one dependency"
projects = ["backend"]
prompt = "Bump {package} to its latest {level} release."
model = "sonnet"
[template.params.package]
values = ["serde_json", "tokio"]
[template.params.level]
values = ["patch", "minor"]
[[template]]
name = "notes"
description = "Draft the release notes"
projects = ["backend"]
prompt = "Draft the release notes for {version}."
[template.params.version]
values = ["v0.4.0", "v0.5.0"]
"#;

/// A home with [`FLOOR`] approved and the window's collector running on it.
pub struct OrdersHome {
    pub env: TestEnv,
    handle: Option<WatchHandle>,
    pub emitted: Emitted,
    /// The fake's copy the collector runs, its sidecar beside it.
    pub claude: PathBuf,
}

impl OrdersHome {
    /// [`FLOOR`] approved, the collector's `claude` the fake with `sidecar`
    /// (its fixture and log are the environment's).
    pub fn start(sidecar: serde_json::Value) -> OrdersHome {
        OrdersHome::start_with(TestEnv::new(), sidecar, "claude")
    }

    /// The same on `env`, the fake's copy named `name` (`claude.cmd` for a
    /// shim the spawn module refuses).
    pub fn start_with(env: TestEnv, mut sidecar: serde_json::Value, name: &str) -> OrdersHome {
        let project = env.root.path().join("work").join("Backend");
        std::fs::create_dir_all(&project).unwrap();
        env.write_floor(&FLOOR.replace("{project}", &project.to_string_lossy().replace('\\', "/")));
        env.approve();
        env.write_agents("[]");
        sidecar["fixture"] = env.fixture().to_string_lossy().into();
        sidecar["log"] = env.fake_log().to_string_lossy().into();
        let bin = env.root.path().join("bin");
        let fake = fake_claude_in(&bin, &sidecar);
        let claude = if name == "claude" {
            fake
        } else {
            let renamed = bin.join(name);
            std::fs::copy(&fake, &renamed).unwrap();
            renamed
        };
        let paths = Paths {
            officina_home: env.officina_home(),
            claude_config_dir: env.claude_config(),
            claude_config_override: None,
            floor_file: None,
        };
        let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(claude.clone()))
            .with_base_env(BaseEnv::new(vec![(
                "SYSTEMROOT".to_owned(),
                std::env::var("SYSTEMROOT").unwrap_or_default(),
            )]));
        let (board_sink, emitted) = sink(|_| {});
        let handle = spawn(collector, board_sink).unwrap();
        emitted
            .boards
            .recv_timeout(Duration::from_secs(10))
            .expect("first board");
        OrdersHome {
            env,
            handle: Some(handle),
            emitted,
            claude,
        }
    }

    pub fn handle(&self) -> &WatchHandle {
        self.handle.as_ref().unwrap()
    }

    /// The project's folder.
    pub fn project(&self) -> PathBuf {
        self.env.root.path().join("work").join("Backend")
    }

    /// The card `request` gets, as `issue_card` answers.
    pub fn issue(&self, request: &CardRequest) -> CardAnswer {
        officina_host::orders::issue_card(
            &self.handle().sender,
            request.clone(),
            &super::workshop::host_runtime(),
        )
        .expect("the collector answers")
    }

    /// A card of `bump` with these values, its default subject.
    pub fn bump(&self, package: &str, level: &str) -> WorkOrderCard {
        match self.issue(&bump_request(package, level)) {
            CardAnswer::Issued(card) => *card,
            refused => panic!("{refused:?}"),
        }
    }

    /// What `edit_card` answers.
    pub fn edit(&self, card_id: &str, change: CardChange) -> CardAnswer {
        ask(
            &self.handle().sender,
            AuthorCommand::EditCard {
                card_id: card_id.to_owned(),
                change,
            },
            AUTHOR_WAIT,
        )
        .expect("the collector answers")
        .into_card()
        .unwrap()
    }

    /// What `set_work_order_permission` answers for card `card_id`.
    pub fn set_permission(
        &self,
        card_id: &str,
        permission: officina_core::orders::permission::WorkOrderPermission,
    ) -> CardAnswer {
        ask(
            &self.handle().sender,
            AuthorCommand::SetWorkOrderPermission {
                card_id: card_id.to_owned(),
                permission,
            },
            AUTHOR_WAIT,
        )
        .expect("the collector answers")
        .into_card()
        .unwrap()
    }

    /// What `launch_work_order` answers, through the function it calls.
    pub fn launch(&self, card_id: &str, prompt_sha256: &str) -> LaunchAnswer {
        officina_host::orders::launch_card(
            &self.handle().sender,
            card_id,
            prompt_sha256,
            None,
            &super::workshop::host_runtime(),
        )
        .expect("the collector answers")
    }

    /// `<home>/orchestrator/proposals`, where the proposal tools write.
    pub fn proposals_dir(&self) -> PathBuf {
        officina_host::paths::orchestrator_proposals_dir(&self.env.officina_home())
    }

    /// Writes `bytes` as the file `name` of the proposals' folder, as a
    /// hand would, and waits for the window's collector to take it: the
    /// file is gone.
    pub fn propose_file(&self, name: &str, bytes: &[u8]) {
        let dir = self.proposals_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        self.wait_gone(&path);
    }

    /// Files under the subjects the proposal checks name, through the
    /// window's own path: an event line per session, then the author's
    /// actions. Returns an open subject of `backend`, an archived one of
    /// `backend` and an open one of `lab`.
    pub fn seed_subjects(&self) -> [i64; 3] {
        use officina_core::registry::confirm::{AuthorAction, SubjectOrigin};
        let now = super::now_ms();
        let events = self.env.events_dir();
        std::fs::create_dir_all(&events).unwrap();
        let lines: String = [
            ("s-open", "backend"),
            ("s-old", "backend"),
            ("s-lab", "lab"),
        ]
        .iter()
        .map(|(session, folder)| {
            format!(
                "{}\n",
                serde_json::json!({"v": 1, "ts": now - 60_000, "event": "SessionStart",
                        "session_id": session, "cwd": format!("C:/work/{folder}")})
            )
        })
        .collect();
        std::fs::write(
            events.join(officina_core::dates::event_file_name(now)),
            lines,
        )
        .unwrap();
        let act = |action: AuthorAction| {
            ask(
                &self.handle().sender,
                AuthorCommand::Act(action),
                AUTHOR_WAIT,
            )
            .and_then(|answer| answer.into_done())
        };
        let create = |name: &str, session: &str| -> i64 {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                let made = act(AuthorAction::Create {
                    name: name.to_owned(),
                    origin: SubjectOrigin::Sessions(vec![session.to_owned()]),
                });
                match made {
                    Ok(Some(id)) => return id,
                    _ if std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    other => panic!("subject {name}: {other:?}"),
                }
            }
        };
        let open = create("upgrades", "s-open");
        let old = create("old work", "s-old");
        let lab = create("lab notes", "s-lab");
        act(AuthorAction::Archive {
            subject: old,
            routine: false,
        })
        .unwrap();
        [open, old, lab]
    }

    /// Writes `file` through `officina mcp`'s writer at `at` and waits for
    /// the window to ingest it; its id.
    pub fn propose(&self, file: &ProposalFile, at: i64) -> String {
        let id = officina_host::mcp::write_proposal(&self.proposals_dir(), file, at).unwrap();
        self.wait_gone(
            &self
                .proposals_dir()
                .join(officina_core::orders::proposal::file_name(&id)),
        );
        id
    }

    fn wait_gone(&self, path: &std::path::Path) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while path.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the window did not ingest {}",
                path.display()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The proposal rows, newest first, read as a command line reads them.
    pub fn proposals(&self) -> Vec<ProposalView> {
        officina_host::registry::store::open_reader(&self.env.officina_home(), &[])
            .unwrap()
            .proposals()
            .unwrap()
    }

    /// The row of proposal `id`.
    pub fn proposal(&self, id: &str) -> ProposalView {
        self.proposals()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("no row for proposal {id}"))
    }

    /// What `issue_card_from_proposal` answers.
    pub fn issue_from_proposal(&self, proposal_id: &str) -> CardAnswer {
        ask(
            &self.handle().sender,
            AuthorCommand::IssueCardFromProposal {
                proposal_id: proposal_id.to_owned(),
            },
            AUTHOR_WAIT,
        )
        .expect("the collector answers")
        .into_card()
        .unwrap()
    }

    /// What `dismiss_proposal` answers.
    pub fn dismiss(&self, proposal_id: &str) -> Result<(), officina_core::ipc::IpcError> {
        ask(
            &self.handle().sender,
            AuthorCommand::DismissProposal {
                proposal_id: proposal_id.to_owned(),
            },
            AUTHOR_WAIT,
        )
        .and_then(|answer| answer.into_done().map(|_| ()))
    }

    /// The fake's logged calls holding `--bg`: the launches.
    pub fn bg_calls(&self) -> Vec<serde_json::Value> {
        fake_claude_calls(&self.env.fake_log())
            .into_iter()
            .filter(|call| {
                call["argv"]
                    .as_array()
                    .is_some_and(|argv| argv.iter().any(|a| a == "--bg"))
            })
            .collect()
    }

    /// Stops the collector thread.
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.sender.send(CollectorMsg::Shutdown);
            let _ = handle.thread.join();
        }
    }
}

impl Drop for OrdersHome {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A work order proposal of `bump` in `backend` with these values, its
/// subject `subject` and its rationale `rationale`, written at `at`.
pub fn bump_proposal(
    package: &str,
    level: &str,
    subject: ProposedSubject,
    rationale: &str,
    at: i64,
) -> ProposalFile {
    ProposalFile::work_order(
        WorkOrderProposal {
            template: "bump".to_owned(),
            project: "backend".to_owned(),
            values: [
                ("package".to_owned(), package.to_owned()),
                ("level".to_owned(), level.to_owned()),
            ]
            .into(),
            subject,
            rationale: rationale.to_owned(),
        },
        Some("s-orchestrator".to_owned()),
        at,
    )
}

/// A request for `bump` in `backend` with these values, its default
/// subject.
pub fn bump_request(package: &str, level: &str) -> CardRequest {
    CardRequest {
        template: "bump".to_owned(),
        project: "backend".to_owned(),
        values: vec![
            ("package".to_owned(), package.to_owned()),
            ("level".to_owned(), level.to_owned()),
        ],
        subject: None,
    }
}
