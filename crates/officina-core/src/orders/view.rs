//! The Orchestrator view's payload (AD-92, P-26, D-136): the window's
//! briefing, the approved templates the composer offers, the templates a
//! pending catalogue change would add or change, the orchestrator's
//! proposals and the latest work orders with their binding. Built here,
//! from what the collector already holds, and emitted by the window when
//! its bytes change (AD-5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::Board;
use crate::orders::briefing::{self, Briefing, BriefingInput, Cursor, SessionBrief};
use crate::orders::work_order::BIND_WINDOW_MS;
use crate::quota::normalise_path;
use crate::registry::types::{
    LaunchOutcome, SubjectRecord, SubjectRef, WorkOrderRecord, WorkOrderSource,
};
use crate::registry::view::RegistryView;
use crate::relay::catalogue::CanonicalCatalogue;
use crate::relay::template::ModelAlias;
use crate::relay::view::{PendingChange, RelayView};
use crate::untrusted::Untrusted;

/// The payload's version; P-13's rule moves it.
pub const ORCHESTRATOR_VIEW_SCHEMA: u32 = 2;

pub use crate::orders::orchestrator::OrchestratorPresence;

pub use crate::orders::proposal::ProposalView;

use crate::orders::proposal::{self, DeletedFiles};

/// An approved template, as the composer offers it (AD-93): every value
/// comes from its closed list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TemplateOption {
    pub name: String,
    pub description: String,
    /// The catalogue projects it may run in.
    pub projects: Vec<String>,
    /// Each parameter's closed list.
    pub params: BTreeMap<String, Vec<String>>,
    pub model: Option<ModelAlias>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a pending catalogue change does to a template.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum TemplateChange {
        Added,
        Changed,
        Removed,
    }
}

/// A template the author has not approved yet, from the pending diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AwaitingTemplate {
    pub name: String,
    pub change: TemplateChange,
}

/// One work order of the history (AD-92, D-136).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkOrderView {
    pub id: String,
    pub name: String,
    pub template: String,
    pub project: String,
    pub values: BTreeMap<String, String>,
    pub source: WorkOrderSource,
    /// The subject it files under, once it has one.
    pub subject: Option<SubjectRef>,
    /// The new subject's name the author chose, while none was created.
    pub new_subject_name: Option<String>,
    pub launched_at_ms: i64,
    pub outcome: LaunchOutcome,
    pub exit_code: Option<i64>,
    pub error: Option<Untrusted<String>>,
    pub printed: Option<Untrusted<String>>,
    /// The binding (AD-81 step 4).
    pub bg_id: Option<String>,
    pub bound_at_ms: Option<i64>,
    /// The bound session as it stands.
    pub session: Option<SessionBrief>,
    /// The bound session's folder when it is not the order's own (D-1002):
    /// a git worktree the session made under the project, say, where its
    /// changes then are. The session chose it, so it is shown as text.
    pub session_folder: Option<Untrusted<String>>,
    /// `started`, and still not bound [`BIND_WINDOW_MS`] after its launch
    /// (D-136).
    pub unbound: bool,
    /// What `claude --bg` printed does not hold the id it bound to (D-136).
    pub printed_id_differs: bool,
    /// Where it runs (AD-121): on the host, or in the workshop.
    pub runner: crate::workshop::run::Runner,
    /// A workshop run's state; none for a host order.
    pub run_state: Option<crate::workshop::run::RunState>,
}

/// The Orchestrator view (AD-92).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OrchestratorView {
    pub schema: u32,
    pub generated_at_ms: i64,
    /// The orchestrator while an agents entry shows it (AD-89), as the
    /// board carries it; null while it is absent.
    pub presence: Option<OrchestratorPresence>,
    /// The window's briefing, from its last reading.
    pub briefing: Briefing,
    /// The approved snapshot's templates, by name.
    pub templates: Vec<TemplateOption>,
    /// The templates a pending catalogue change adds, changes or removes.
    pub awaiting_templates: Vec<AwaitingTemplate>,
    /// The orchestrator's proposals (AD-91, D-155): every pending one and
    /// the latest invalid and decided ones, newest first.
    pub proposals: Vec<ProposalView>,
    /// The files of `proposals/` deleted unread since the window started.
    pub deleted_proposal_files: DeletedFiles,
    /// The latest work orders, newest first.
    pub work_orders: Vec<WorkOrderView>,
}

/// What the view is built from: the briefing's input, the relay view's
/// approved snapshot and pending change, the board's orchestrator, and the
/// proposal files ingestion deleted.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewInput<'a> {
    pub briefing: BriefingInput<'a>,
    pub presence: Option<OrchestratorPresence>,
    pub catalogue: Option<&'a CanonicalCatalogue>,
    pub pending: Option<&'a PendingChange>,
    pub deleted: DeletedFiles,
}

impl<'a> ViewInput<'a> {
    /// The input from the window's payloads and orders state; a payload
    /// not built yet is empty.
    pub fn of(
        board: Option<&'a Board>,
        relay: Option<&'a RelayView>,
        registry: Option<&'a RegistryView>,
        subjects: &'a [SubjectRecord],
        work_orders: &'a [WorkOrderRecord],
        proposals: &'a [ProposalView],
    ) -> ViewInput<'a> {
        ViewInput {
            briefing: BriefingInput {
                columns: board.map(|b| &b.columns),
                approvals: board.map_or(&[], |b| &b.approvals),
                recent: relay.map_or(&[], |r| &r.recent),
                open_subjects: registry.map_or(&[], |r| &r.subjects),
                triage: registry.map_or(&[], |r| &r.triage),
                subjects,
                work_orders,
                proposals,
            },
            presence: board.and_then(|b| b.orchestrator),
            catalogue: relay.and_then(|r| r.catalogue.as_ref()),
            pending: relay.and_then(|r| r.pending.as_ref()),
            deleted: DeletedFiles::default(),
        }
    }
}

/// The approved snapshot's templates as the composer and `officina mcp`'s
/// `templates` tool offer them, by name.
pub fn template_options(catalogue: Option<&CanonicalCatalogue>) -> Vec<TemplateOption> {
    catalogue.map_or_else(Vec::new, |catalogue| {
        catalogue
            .templates
            .iter()
            .map(|(name, template)| TemplateOption {
                name: name.clone(),
                description: template.description.clone(),
                projects: template.projects.clone(),
                params: template.params.clone(),
                model: template.model,
            })
            .collect()
    })
}

fn awaiting(pending: Option<&PendingChange>) -> Vec<AwaitingTemplate> {
    let Some(pending) = pending else {
        return Vec::new();
    };
    let section = &pending.diff.templates;
    let of = |names: &mut dyn Iterator<Item = &String>, change| {
        names
            .map(|name| AwaitingTemplate {
                name: name.clone(),
                change,
            })
            .collect::<Vec<_>>()
    };
    let mut out = of(&mut section.added.iter(), TemplateChange::Added);
    out.extend(of(
        &mut section.changed.iter().map(|entry| &entry.name),
        TemplateChange::Changed,
    ));
    out.extend(of(&mut section.removed.iter(), TemplateChange::Removed));
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The bound session's folder, the board's live one else the registry's
/// last, when it differs from the folder the order ran in as
/// [`normalise_path`] reads both; `None` while unbound or unknown.
fn session_folder(input: &BriefingInput, order: &WorkOrderRecord) -> Option<Untrusted<String>> {
    let session_id = order.session_id.as_deref()?;
    let folder = input
        .card_of(session_id)
        .and_then(|card| card.cwd.clone())
        .or_else(|| {
            order
                .session
                .as_ref()
                .and_then(|record| record.cwd.as_ref())
                .map(|cwd| cwd.value.clone())
        })?;
    (normalise_path(&folder) != normalise_path(&order.cwd)).then(|| Untrusted::new(folder))
}

/// `order` as the history shows it at `now_ms`, `unbound` and "printed id
/// differs" computed from its stored columns, never stored (D-136).
fn order_view(input: &BriefingInput, order: &WorkOrderRecord, now_ms: i64) -> WorkOrderView {
    let unbound = order.outcome == LaunchOutcome::Started
        && order.bound_at_ms.is_none()
        && now_ms - order.launched_at_ms >= BIND_WINDOW_MS;
    let printed_id_differs = match (&order.bg_id, &order.printed) {
        (Some(id), Some(printed)) => printed.first_line_matching(&[id.as_str()]).is_none(),
        _ => false,
    };
    WorkOrderView {
        id: order.id.clone(),
        name: order.name.clone(),
        template: order.template.clone(),
        project: order.project.clone(),
        values: order.values.clone(),
        source: order.source,
        subject: order.subject_id.and_then(|id| {
            input
                .subjects
                .iter()
                .find(|s| s.id == id)
                .map(|s| SubjectRef {
                    id,
                    name: s.name.clone(),
                })
        }),
        new_subject_name: order
            .subject_id
            .is_none()
            .then(|| order.new_subject_name.clone())
            .flatten(),
        launched_at_ms: order.launched_at_ms,
        outcome: order.outcome,
        exit_code: order.exit_code,
        error: order.error.clone(),
        printed: order.printed.clone(),
        bg_id: order.bg_id.clone(),
        bound_at_ms: order.bound_at_ms,
        session: briefing::order_session(input, order),
        session_folder: session_folder(input, order),
        unbound,
        printed_id_differs,
        runner: order.runner,
        run_state: order.run_state,
    }
}

/// The Orchestrator view at `now_ms`, its briefing from `cursor`.
pub fn build(input: &ViewInput, cursor: Cursor, now_ms: i64) -> OrchestratorView {
    OrchestratorView {
        schema: ORCHESTRATOR_VIEW_SCHEMA,
        generated_at_ms: now_ms,
        presence: input.presence,
        briefing: briefing::build(&input.briefing, cursor, now_ms),
        templates: template_options(input.catalogue),
        awaiting_templates: awaiting(input.pending),
        proposals: proposal::shown(input.briefing.proposals),
        deleted_proposal_files: input.deleted,
        work_orders: input
            .briefing
            .work_orders
            .iter()
            .map(|order| order_view(&input.briefing, order, now_ms))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::types::{Source, Sourced, SubjectStatus};
    use crate::relay::catalogue::{diff, load};
    use crate::relay::template::sample;

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn templates_come_from_the_approved_snapshot() {
        let approved = sample::approved();
        let input = ViewInput {
            catalogue: Some(&approved.canonical),
            ..ViewInput::default()
        };
        let view = build(&input, Cursor::None, NOW);
        let names: Vec<&str> = view.templates.iter().map(|t| t.name.as_str()).collect();
        let approved_names: Vec<&str> = approved
            .canonical
            .templates
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(names, approved_names);
        let template = &view.templates[0];
        let canonical = &approved.canonical.templates[sample::TEMPLATE];
        assert_eq!(template.projects, canonical.projects);
        assert_eq!(template.params, canonical.params, "each list whole");
        assert_eq!(template.model, canonical.model);
        assert_eq!(template.description, canonical.description);
        // No snapshot, no template, whatever the floor says.
        assert!(
            build(&ViewInput::default(), Cursor::None, NOW)
                .templates
                .is_empty()
        );
    }

    #[test]
    fn awaiting_templates_come_from_the_pending_diff() {
        let approved = sample::approved();
        let edited = sample::FLOOR.replace("Bump one dependency", "Bump one dependency, carefully")
            + r#"
[[template]]
name = "lint-sweep"
description = "Fix lint warnings"
projects = ["backend"]
prompt = "Fix the {level} lint warnings."
[template.params.level]
values = ["minor"]
"#;
        let current = load(&edited, sample::FLOOR_PATH).unwrap();
        assert!(
            current.catalogue.issues.is_empty(),
            "{:?}",
            current.catalogue.issues
        );
        let pending = PendingChange {
            hash: current.hash.clone(),
            diff: diff(&approved.canonical, &current.canonical),
            current: current.canonical.clone(),
        };
        let input = ViewInput {
            catalogue: Some(&approved.canonical),
            pending: Some(&pending),
            ..ViewInput::default()
        };
        let view = build(&input, Cursor::None, NOW);
        assert_eq!(
            view.awaiting_templates,
            [
                AwaitingTemplate {
                    name: sample::TEMPLATE.into(),
                    change: TemplateChange::Changed
                },
                AwaitingTemplate {
                    name: "lint-sweep".into(),
                    change: TemplateChange::Added
                },
            ]
        );
        assert_eq!(
            view.templates.len(),
            1,
            "the composer offers the approved only"
        );
        // A removal awaits approval too.
        let without = load(
            &sample::FLOOR[..sample::FLOOR.find("[[template]]").unwrap()],
            sample::FLOOR_PATH,
        )
        .unwrap();
        let removal = PendingChange {
            hash: without.hash.clone(),
            diff: diff(&approved.canonical, &without.canonical),
            current: without.canonical.clone(),
        };
        let removed = awaiting(Some(&removal));
        assert_eq!(removed[0].change, TemplateChange::Removed);
    }

    fn order(id: &str, outcome: LaunchOutcome, launched_at_ms: i64) -> WorkOrderRecord {
        WorkOrderRecord {
            id: id.to_owned(),
            seq: 1,
            template: sample::TEMPLATE.to_owned(),
            project: "backend".to_owned(),
            cwd: "C:/work/backend".to_owned(),
            values: BTreeMap::from([("level".to_owned(), "minor".to_owned())]),
            argv: Vec::new(),
            prompt_sha256: String::new(),
            catalogue_hash: String::new(),
            name: format!("{}-1", sample::TEMPLATE),
            subject_id: Some(4),
            new_subject_name: None,
            source: WorkOrderSource::Composer,
            proposal_id: None,
            launched_at_ms,
            outcome,
            exit_code: Some(0),
            error: None,
            printed: Some(Untrusted::new(
                "backgrounded · bg123 · dependency-bump-1".to_owned(),
            )),
            bg_id: None,
            session_id: None,
            bound_at_ms: None,
            session: None,
            runner: Default::default(),
            run_state: None,
        }
    }

    #[test]
    fn unbound_and_printed_id_differs_are_derived() {
        let launched = NOW - BIND_WINDOW_MS;
        let fresh = order("fresh", LaunchOutcome::Started, NOW - BIND_WINDOW_MS + 1);
        let late = order("late", LaunchOutcome::Started, launched);
        let failed = order("failed", LaunchOutcome::Failed, launched);
        let mut bound = order("bound", LaunchOutcome::Started, launched);
        bound.bg_id = Some("bg123".into());
        bound.session_id = Some("s-1".into());
        bound.bound_at_ms = Some(launched + 1_000);
        let mut other = bound.clone();
        other.id = "other".into();
        other.bg_id = Some("bg999".into());
        let mut cleared = other.clone();
        cleared.id = "cleared".into();
        cleared.printed = None;
        let orders = vec![fresh, late, failed, bound, other, cleared];
        let subjects = vec![SubjectRecord {
            id: 4,
            project: "backend".into(),
            name: "Upgrades".into(),
            status: Sourced {
                value: SubjectStatus::Active,
                src: Source::Author,
                at_ms: NOW,
            },
            created_at_ms: NOW,
            routine: false,
            merged_into: None,
        }];
        let input = ViewInput::of(None, None, None, &subjects, &orders, &[]);
        let view = build(&input, Cursor::None, NOW);
        let flags: Vec<(&str, bool, bool)> = view
            .work_orders
            .iter()
            .map(|o| (o.id.as_str(), o.unbound, o.printed_id_differs))
            .collect();
        assert_eq!(
            flags,
            [
                ("fresh", false, false),
                ("late", true, false),
                ("failed", false, false),
                ("bound", false, false),
                ("other", false, true),
                ("cleared", false, false),
            ]
        );
        assert_eq!(
            view.work_orders[0].subject,
            Some(SubjectRef {
                id: 4,
                name: "Upgrades".into()
            })
        );
        // Derived, never stored: the record has no such column.
        let bound = &view.work_orders[3];
        assert_eq!(
            bound.session.as_ref().map(|s| s.phase),
            Some(briefing::SessionPhase::Unknown)
        );
        assert_eq!(view.schema, ORCHESTRATOR_VIEW_SCHEMA);
        assert_eq!(view.presence, None);
        assert!(view.proposals.is_empty());
    }

    /// D-1002, gate 4's window run: a session that made its own git
    /// worktree under the project worked there, not in the order's folder,
    /// and the history says where; the order's own folder in another
    /// spelling says nothing, nor does an order not bound.
    #[test]
    fn a_session_working_in_another_folder_is_shown_where() {
        let worktree = "C:/work/backend/.claude/worktrees/officina-probe";
        let in_folder = |cwd: &str| {
            let mut bound = order("bound", LaunchOutcome::Started, NOW - 60_000);
            bound.bg_id = Some("bg123".into());
            bound.session_id = Some("s-1".into());
            bound.bound_at_ms = Some(NOW - 58_000);
            bound.session = Some(crate::registry::types::SessionRecord {
                session_id: "s-1".into(),
                cwd: Some(Sourced {
                    value: cwd.to_owned(),
                    src: Source::Agents,
                    at_ms: NOW,
                }),
                ..Default::default()
            });
            bound
        };
        let orders = vec![
            in_folder(worktree),
            in_folder(r"c:\work\Backend\"),
            order("unbound", LaunchOutcome::Started, NOW - 60_000),
        ];
        let view = build(
            &ViewInput::of(None, None, None, &[], &orders, &[]),
            Cursor::None,
            NOW,
        );
        let folders: Vec<Option<Untrusted<String>>> = view
            .work_orders
            .iter()
            .map(|o| o.session_folder.clone())
            .collect();
        assert_eq!(
            folders,
            [Some(Untrusted::new(worktree.to_owned())), None, None]
        );
    }

    /// Story 6.11 AC 6 (AD-121): a history row names where it runs, and a
    /// workshop row its run's state.
    #[test]
    fn history_rows_name_their_runner() {
        use crate::workshop::run::{RunState, Runner};
        let host = order("host", LaunchOutcome::Started, NOW - 60_000);
        let mut workshop = order("workshop", LaunchOutcome::Started, NOW - 30_000);
        workshop.runner = Runner::Workshop;
        workshop.run_state = Some(RunState::Running);
        let view = build(
            &ViewInput::of(None, None, None, &[], &[host, workshop], &[]),
            Cursor::None,
            NOW,
        );
        let rows: Vec<(Runner, Option<RunState>)> = view
            .work_orders
            .iter()
            .map(|o| (o.runner, o.run_state))
            .collect();
        assert_eq!(
            rows,
            [
                (Runner::Host, None),
                (Runner::Workshop, Some(RunState::Running))
            ]
        );
        let json = serde_json::to_value(&view.work_orders[1]).unwrap();
        assert_eq!(json["runner"], "workshop");
        assert_eq!(json["run_state"], "running");
    }

    /// Story 5.17 AC 5 (AD-92, D-155): the view lists every pending
    /// proposal, the latest 50 invalid and the latest 50 decided, newest
    /// first, and carries the deleted files' counts.
    #[test]
    fn proposals_are_pending_invalid_and_last_50_decided() {
        use crate::orders::proposal::{
            PROPOSALS_SHOWN_DECIDED, PROPOSALS_SHOWN_INVALID, proposal_id,
        };
        use crate::registry::types::ProposalStatus;
        let mut rows: Vec<ProposalView> = Vec::new();
        let statuses = [
            ProposalStatus::Pending,
            ProposalStatus::Invalid,
            ProposalStatus::Accepted,
            ProposalStatus::Expired,
        ];
        for (at, status) in (0..240).zip(statuses.iter().cycle()) {
            rows.push(ProposalView {
                status: *status,
                received_at_ms: Some(NOW - at),
                ..ProposalView::unreadable_file(&proposal_id(NOW - at, 1))
            });
        }
        let mut input = ViewInput::of(None, None, None, &[], &[], &rows);
        input.deleted.misnamed = 3;
        let view = build(&input, Cursor::None, NOW);
        let count = |wanted: &[ProposalStatus]| {
            view.proposals
                .iter()
                .filter(|p| wanted.contains(&p.status))
                .count()
        };
        assert_eq!(count(&[ProposalStatus::Pending]), 60);
        assert_eq!(count(&[ProposalStatus::Invalid]), PROPOSALS_SHOWN_INVALID);
        assert_eq!(
            count(&[ProposalStatus::Accepted, ProposalStatus::Expired]),
            PROPOSALS_SHOWN_DECIDED
        );
        assert!(view.proposals.windows(2).all(|w| w[0].id > w[1].id));
        assert_eq!(view.deleted_proposal_files.misnamed, 3);
        assert_eq!(view.briefing.proposals.count, 60);
    }

    /// Story 5.16 AC 5 (AD-92): the view's presence is the board's.
    #[test]
    fn the_view_carries_the_boards_presence() {
        use crate::agents::parse_agents;
        use crate::board::{BoardInput, build_board};
        let home = "C:/path/to/home";
        let input = BoardInput {
            agents: Some(
                parse_agents(
                    r#"[{"pid":9,"sessionId":"o","name":"officina-orchestrator","kind":"interactive","status":"busy","cwd":"C:/path/to/home/orchestrator/workdir","startedAt":4}]"#,
                )
                .unwrap(),
            ),
            officina_home: Some(home.to_owned()),
            ..Default::default()
        };
        let board = build_board(&input, NOW);
        assert!(board.orchestrator.is_some());
        let view = build(
            &ViewInput::of(Some(&board), None, None, &[], &[], &[]),
            Cursor::None,
            NOW,
        );
        assert_eq!(view.presence, board.orchestrator);
    }
}
