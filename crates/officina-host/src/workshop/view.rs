//! The Workshop view on the host (AD-121): the engine's read half, read
//! with read calls only, the registry's and the journal's half, and
//! `officina workshop --json`'s whole view. The window reads the first on
//! thread `workshop` and builds the view on its collector thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use officina_core::relay::journal::JournalLine;
use officina_core::workshop::engine::{
    EngineCall, InfoFacts, ObjectRow, PsRow, engine_key, parse_object_rows, parse_ps,
};
use officina_core::workshop::floor::{CredentialKind, UpstreamProxy};
use officina_core::workshop::image::{GATEWAY_IMAGE, ImageKind, WORKSHOP_BASE};
use officina_core::workshop::launch::CredentialState;
use officina_core::workshop::lifecycle::{ContainerSeen, ReconcileAction, RowSeen, reconcile};
use officina_core::workshop::names::{FLOOR_LABEL, ROLE_LABEL, RUN_LABEL, login_volume};
use officina_core::workshop::view::{
    BoardRun, EngineHalf, EngineStatus, EngineView, ImageFacts, InventoryView, NetworkView,
    RegistryHalf, WorkshopCredential, WorkshopView, build_view, credential_action,
};

use crate::relay::credential::CredentialStore;
use crate::relay::home::RelayHome;
use crate::workshop::doctor::{ImageContext, ProxyInput, battery_facts, image_report, read_call};
use crate::workshop::engine::{EngineState, ResolveError, ResolvedDocker, read_engine_state};
use crate::workshop::launch::{CredentialStateSource, credential_present};
use crate::workshop::proxy::{ProxyError, SystemProxyReader, resolve};

/// What the engine half is read with: the window's or the command line's
/// own sources.
#[derive(Clone)]
pub struct EngineReader {
    pub relay: RelayHome,
    /// The executable's folder, where `linux/` sits (AD-100).
    pub exe_dir: PathBuf,
    /// The environment `docker.exe` runs in (D-616).
    pub env: Vec<(String, String)>,
    pub docker: Result<ResolvedDocker, ResolveError>,
    pub store: Arc<dyn CredentialStore + Send + Sync>,
    pub states: Arc<dyn CredentialStateSource + Send + Sync>,
    pub proxy: Arc<dyn SystemProxyReader>,
}

/// The engine half, the floor key it was read for and the containers the
/// engine listed, for the command line's orphans.
#[derive(Debug, Clone, Default)]
pub struct EngineRead {
    pub half: EngineHalf,
    pub floor_key: Option<String>,
    pub containers: Option<Vec<PsRow>>,
}

/// The approved catalogue's workshop, as doctor's image and proxy checks
/// take it; `None` while no valid `[workshop]` is approved.
fn approved_context(relay: &RelayHome, exe_dir: &Path) -> Option<(ImageContext, ProxyInput)> {
    let state = relay.read_approved();
    let canonical = (*state.approved()?.canonical).clone();
    canonical.workshop.as_ref()?;
    let floor = crate::relay::home::load_relay_floor(&relay.relay_floor(&state)).ok()?;
    let proxy = crate::workshop::doctor::proxy_input(&canonical)?;
    let project_dirs =
        officina_core::relay::catalogue::project_dirs(&floor.floor, &floor.floor_path)
            .into_iter()
            .map(|(name, dir)| (name, PathBuf::from(dir)))
            .collect();
    Some((
        ImageContext {
            exe_dir: exe_dir.to_path_buf(),
            canonical,
            floor_key: officina_core::workshop::names::floor_key(&floor.floor_path),
            project_dirs,
            claude_version: None,
        },
        proxy,
    ))
}

fn engine_view(state: &EngineState, key: Option<String>) -> EngineView {
    let (status, detail) = match state {
        EngineState::Unresolved(why) => (EngineStatus::Unresolved, Some(why.clone())),
        EngineState::Unreachable(why) => (EngineStatus::Unreachable, Some(why.clone())),
        EngineState::Unavailable(why) => (EngineStatus::Unavailable, Some(why.code().to_owned())),
        EngineState::Ready(_) => (EngineStatus::Ready, None),
    };
    EngineView {
        status,
        detail,
        key,
    }
}

fn proxy_error(error: &ProxyError) -> String {
    match error {
        ProxyError::Refused(refusal) => format!("{}: {}", refusal.cause.code(), refusal.entry),
        ProxyError::Unreadable(why) => format!("proxy_unresolved: {why}"),
    }
}

/// The kind of an image doctor's report lists by tag.
fn kind_of(tag: &str) -> ImageKind {
    match tag {
        GATEWAY_IMAGE => ImageKind::Gateway,
        WORKSHOP_BASE => ImageKind::Workshop,
        _ => ImageKind::Project,
    }
}

/// The credential's state as the view shows it (D-607): the source's
/// state, `missing` when a token floor has no Credential Manager entry or
/// a `login` floor no login volume on an engine that answered.
fn credential_state(
    kind: CredentialKind,
    source: CredentialState,
    present: bool,
    login_volume_seen: Option<bool>,
) -> CredentialState {
    let missing = match kind {
        CredentialKind::Login => login_volume_seen == Some(false),
        CredentialKind::OauthToken | CredentialKind::ApiKey => !present,
    };
    if missing && !source.needs_new() {
        CredentialState::Missing
    } else {
        source
    }
}

/// Reads the engine half with read calls only (AD-121): the engine's
/// state and key, each image's check, inventories and battery, the limits,
/// the credential's state and action, and the routes; `building` while
/// Build images runs. Nothing is written but `docker.exe`'s working
/// folder, the Officina home.
pub fn read_engine_half(reader: &EngineReader, building: bool) -> EngineRead {
    let home = reader.relay.officina_home().to_path_buf();
    let env = reader.env.as_slice();
    let mut read = EngineRead::default();
    if reader.docker.is_ok() {
        let _ = std::fs::create_dir_all(&home);
    }
    let state = read_engine_state(reader.docker.as_ref(), env, &home);
    let docker = reader
        .docker
        .as_ref()
        .ok()
        .filter(|_| state.info().is_some());
    let info = docker
        .and_then(|d| read_call(d, &EngineCall::info(), env, &home))
        .and_then(|out| InfoFacts::parse(&out));
    let key = state
        .info()
        .zip(info.as_ref())
        .map(|(engine, facts)| engine_key(engine, facts));
    read.containers =
        docker.and_then(|d| read_call(d, &EngineCall::ps(), env, &home).map(|out| parse_ps(&out)));
    let volumes: Option<Vec<ObjectRow>> = docker.and_then(|d| {
        read_call(d, &EngineCall::volume_ls(), env, &home).map(|out| parse_object_rows(&out))
    });
    read.half.engine = Some(engine_view(&state, key.clone()));
    read.half.building = building;
    let Some((context, proxy)) = approved_context(&reader.relay, &reader.exe_dir) else {
        return read;
    };
    let Some(workshop) = context.canonical.workshop.as_ref() else {
        return read;
    };
    read.half.enabled = true;
    read.half.limits = Some((workshop.max_workers, workshop.cpus, workshop.memory_mib));
    let batteries = battery_facts(&context, &home, key.as_deref());
    let registry = crate::registry::store::open_reader(&home, &[]).ok();
    let inventories = |tag: &str| -> Vec<InventoryView> {
        context
            .canonical
            .projects
            .keys()
            .filter_map(|project| {
                registry
                    .as_ref()?
                    .workshop_inventory(tag, project)
                    .ok()
                    .flatten()
                    .map(|record| InventoryView {
                        project: record.project,
                        at_ms: record.inventory_at_ms,
                    })
            })
            .collect()
    };
    if let Some(report) = image_report(&context, env, &home, docker) {
        read.half.images = report
            .images
            .into_iter()
            .map(|check| {
                let kind = kind_of(&check.tag);
                ImageFacts {
                    inventories: if kind == ImageKind::Gateway {
                        Vec::new()
                    } else {
                        inventories(&check.tag)
                    },
                    battery: batteries
                        .iter()
                        .find(|b| b.image == check.tag)
                        .cloned()
                        .or_else(|| {
                            (kind != ImageKind::Gateway).then(|| {
                                officina_core::workshop::doctor::BatteryFacts {
                                    image: check.tag.clone(),
                                    current: None,
                                    runs: Vec::new(),
                                    newest_skew_ms: None,
                                }
                            })
                        }),
                    kind,
                    check,
                }
            })
            .collect();
    }
    let floor_key = context.floor_key.clone();
    let present = credential_present(reader.store.as_ref(), workshop.auth, &floor_key);
    let login_seen = volumes.as_ref().map(|rows| {
        let name = login_volume(&floor_key);
        rows.iter().any(|row| row.name == name.as_str())
    });
    let routes = resolve(&proxy.setting, &proxy.allow, Arc::clone(&reader.proxy));
    let gateway_routes = routes
        .as_ref()
        .map(officina_core::workshop::proxy::Routes::gateway_routes)
        .unwrap_or_default();
    let state = credential_state(
        workshop.auth,
        reader.states.state(&floor_key, workshop.auth),
        present,
        login_seen,
    );
    read.half.credential = Some(WorkshopCredential {
        kind: workshop.auth,
        state,
        action: credential_action(workshop.auth, state, &floor_key, &gateway_routes),
    });
    read.half.network = Some(NetworkView::of(
        UpstreamProxy::setting(&proxy.setting),
        routes.as_ref().map_err(proxy_error),
    ));
    read.floor_key = Some(floor_key);
    read
}

/// The run containers of `floor_key` no row owns (AD-109), by
/// reconciliation's own rule: listed, never adopted.
pub fn orphans(containers: &[PsRow], floor_key: &str, runs: &[BoardRun]) -> Vec<String> {
    let seen: Vec<ContainerSeen> = containers
        .iter()
        .filter(|row| row.labels.get(FLOOR_LABEL).map(String::as_str) == Some(floor_key))
        .map(|row| ContainerSeen {
            name: row.name.clone(),
            run_id: row.labels.get(RUN_LABEL).cloned(),
            role: row.labels.get(ROLE_LABEL).cloned().unwrap_or_default(),
            running: row.state == "running",
        })
        .collect();
    // Every row owns its containers here: reconciliation's actions on the
    // rows themselves are the window's, not the view's.
    let rows: Vec<RowSeen> = runs
        .iter()
        .map(|run| RowSeen {
            id: run.id.clone(),
            state: officina_core::workshop::run::RunState::Imported,
            started_at_ms: run.started_at_ms,
            deadline_ms: 0,
            has_package: false,
        })
        .collect();
    reconcile(&rows, &seen, 0)
        .into_iter()
        .filter_map(|action| match action {
            ReconcileAction::Orphan { name } => Some(name),
            _ => None,
        })
        .collect()
}

/// Every journal line the view counts (AD-121): the days retention keeps.
pub fn journal_lines(relay: &RelayHome, now_ms: i64) -> Vec<JournalLine> {
    crate::relay::journal::read_days(
        relay,
        now_ms,
        officina_core::relay::journal::JOURNAL_RETENTION_DAYS,
    )
    .0
}

/// The journal lines the window's view counts, read again only when a
/// file of the journal or the day changed, so each view build lists the
/// folder rather than reading 90 days.
#[derive(Debug, Default)]
pub struct JournalCache {
    stamp: Vec<(String, u64, Option<std::time::SystemTime>)>,
    lines: Vec<JournalLine>,
}

impl JournalCache {
    /// The lines of [`journal_lines`] at `now_ms`.
    pub fn lines(&mut self, relay: &RelayHome, now_ms: i64) -> &[JournalLine] {
        let mut stamp: Vec<(String, u64, Option<std::time::SystemTime>)> =
            std::fs::read_dir(relay.journal_dir())
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|entry| {
                            let meta = entry.metadata().ok()?;
                            Some((
                                entry.file_name().to_string_lossy().into_owned(),
                                meta.len(),
                                meta.modified().ok(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
        stamp.push((officina_core::dates::utc_day_text(now_ms), 0, None));
        stamp.sort();
        if stamp != self.stamp {
            self.lines = journal_lines(relay, now_ms);
            self.stamp = stamp;
        }
        &self.lines
    }
}

/// Queues a read of the engine half on thread `workshop` (AD-98: the
/// collector never runs `docker`), its result sent to the collector as
/// [`AuthorCommand::WorkshopRead`], a request it never drops; `building`
/// tells whether Build images runs. False when the thread has stopped.
///
/// [`AuthorCommand::WorkshopRead`]: crate::watch::AuthorCommand::WorkshopRead
#[cfg(feature = "window-authority")]
pub fn reread_on_thread(
    thread: &crate::workshop::threads::JobThread,
    reader: EngineReader,
    building: Arc<std::sync::atomic::AtomicBool>,
    collector: std::sync::mpsc::Sender<crate::watch::CollectorMsg>,
) -> bool {
    use crate::watch::{AUTHOR_WAIT, AuthorCommand, AuthorRequest, CollectorMsg};
    thread.submit(move || {
        let read = read_engine_half(&reader, building.load(std::sync::atomic::Ordering::SeqCst));
        let (request, _answer) = AuthorRequest::new(
            AuthorCommand::WorkshopRead(Box::new(read.half)),
            AUTHOR_WAIT,
        );
        let _ = collector.send(CollectorMsg::Author(request));
    })
}

/// Why `officina workshop --json` printed no view: the registry could not
/// be read (D-47).
#[derive(Debug)]
pub struct RegistryUnread(pub String);

/// `officina workshop --json`'s view (AD-121): the read-only registry, and
/// the engine's read half, whatever the engine answers.
pub fn cli_view(reader: &EngineReader, now_ms: i64) -> Result<WorkshopView, RegistryUnread> {
    let home = reader.relay.officina_home();
    let registry = crate::registry::store::open_reader(
        home,
        &crate::registry::store::onedrive_roots_from_env(),
    )
    .map_err(|failure| RegistryUnread(format!("{failure:?}")))?;
    let runs = registry
        .workshop_board_runs()
        .map_err(|e| RegistryUnread(e.to_string()))?;
    let read = read_engine_half(reader, false);
    let orphans = match (&read.containers, &read.floor_key) {
        (Some(containers), Some(key)) => orphans(containers, key, &runs),
        _ => Vec::new(),
    };
    let lines = journal_lines(&reader.relay, now_ms);
    let flags = reader.relay.stop_flags();
    Ok(build_view(
        &read.half,
        &RegistryHalf {
            runs: &runs,
            orphans: &orphans,
            lines: &lines,
            relay_stopped: flags.relay,
            workshop_stopped: flags.workshop,
        },
        now_ms,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn container(name: &str, floor: &str, run: Option<&str>, role: &str) -> PsRow {
        let mut labels = BTreeMap::new();
        labels.insert(FLOOR_LABEL.to_owned(), floor.to_owned());
        labels.insert(ROLE_LABEL.to_owned(), role.to_owned());
        if let Some(run) = run {
            labels.insert(RUN_LABEL.to_owned(), run.to_owned());
        }
        PsRow {
            id: name.to_owned(),
            name: name.to_owned(),
            state: "exited".to_owned(),
            labels,
        }
    }

    fn run(id: &str) -> BoardRun {
        BoardRun {
            id: id.to_owned(),
            template: "bump".into(),
            project: "api".into(),
            name: "bump-1".into(),
            state: officina_core::workshop::run::RunState::Review,
            state_at_ms: 1,
            started_at_ms: 1,
        }
    }

    /// AD-109: a run container of this floor whose run the registry does
    /// not hold is an orphan; another floor's, and a known run's, are not.
    #[test]
    fn orphans_are_unowned_run_containers_of_this_floor() {
        use officina_core::workshop::vector::ContainerRole;
        let worker = ContainerRole::Worker.code();
        let containers = [
            container("known", "f1", Some("a"), worker),
            container("lost", "f1", Some("b"), worker),
            container("other-floor", "f2", Some("c"), worker),
        ];
        assert_eq!(orphans(&containers, "f1", &[run("a")]), ["lost"]);
    }

    /// D-607: a token floor without its entry and a `login` floor without
    /// its volume are `missing`; an expired state is kept, never cleared.
    #[test]
    fn a_missing_credential_is_computed() {
        use CredentialState::{Missing, NeedsNewCredential, Unchecked};
        assert_eq!(
            credential_state(CredentialKind::OauthToken, Unchecked, false, None),
            Missing
        );
        assert_eq!(
            credential_state(CredentialKind::OauthToken, Unchecked, true, None),
            Unchecked
        );
        assert_eq!(
            credential_state(CredentialKind::Login, Unchecked, false, None),
            Unchecked
        );
        assert_eq!(
            credential_state(CredentialKind::Login, Unchecked, false, Some(false)),
            Missing
        );
        assert_eq!(
            credential_state(CredentialKind::ApiKey, NeedsNewCredential, false, None),
            NeedsNewCredential
        );
    }
}
