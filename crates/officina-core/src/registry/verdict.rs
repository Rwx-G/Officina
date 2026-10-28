//! What needs a verdict (FR-7 as amended, AD-47, AD-49, D-75, D-76): one
//! row per finished subject, never one per session, anchored on its latest
//! session that saw a prompt, superseded or not; one row per ended,
//! unfiled, interactive session that saw a prompt and was not replaced in
//! its process; and an "empty subject: archive?" row for an `active`
//! subject none of whose sessions saw a prompt, so no subject stays open
//! forever. Computed again on every view build; nothing here writes.

use std::collections::BTreeMap;

use super::proposal::{ModelInput, Proposal, empty_subject, propose};
use super::types::{ConfirmationRecord, RowKind, SessionRecord, SubjectRecord, SubjectStatus};

/// A row of the triage list, before the view joins its excerpts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// Stable while the row stands: `subject:<id>` or `session:<id>`.
    pub key: String,
    pub project: String,
    /// The subject, for a subject or empty-subject row.
    pub subject: Option<i64>,
    /// The anchor, for a subject row; the session, for an unfiled one.
    pub session_id: Option<String>,
    pub proposal: Proposal,
    /// When the row's reason to exist arose: the anchor's end, or the
    /// empty subject's last status change. Rows are listed latest first.
    pub at_ms: i64,
    /// For an unfiled row: the subject proposed, when its project has
    /// exactly one open subject (AD-49).
    pub proposed_subject: Option<i64>,
    /// For an unfiled row: the open subjects of its project, by id.
    pub choices: Vec<i64>,
}

/// What the rows are computed from.
#[derive(Debug, Clone, Copy)]
pub struct VerdictInput<'a> {
    pub subjects: &'a [SubjectRecord],
    pub sessions: &'a [SessionRecord],
    pub confirmations: &'a [ConfirmationRecord],
    pub model: &'a ModelInput,
}

/// The latest of `sessions` that saw a prompt: by its last prompt, then
/// last seen, then id.
pub(crate) fn anchor<'a>(sessions: &[&'a SessionRecord]) -> Option<&'a SessionRecord> {
    sessions
        .iter()
        .filter_map(|s| Some((s.last_prompt_at.as_ref()?.value, *s)))
        .max_by(|(a_at, a), (b_at, b)| {
            a_at.cmp(b_at)
                .then_with(|| a.last_seen_ms.cmp(&b.last_seen_ms))
                .then_with(|| a.session_id.cmp(&b.session_id))
        })
        .map(|(_, s)| s)
}

/// True when a confirmation still answers the row: not reopened (D-58),
/// on the same session (none for an empty subject) and made at or after
/// `since`, the end it decided on. A session that lived and ended again
/// after the verdict needs a new one.
fn answered(
    confirmations: &[ConfirmationRecord],
    subject: Option<i64>,
    session: Option<&str>,
    since: i64,
) -> bool {
    confirmations.iter().any(|c| {
        c.reopened_at_ms.is_none()
            && subject.is_none_or(|id| c.subject_id == id)
            && c.session_id.as_deref() == session
            && c.confirmed_at_ms >= since
    })
}

/// The subject to propose for an unfiled session of `project`, and the
/// choices (AD-49): the project's `active` and `to_resume` subjects, one of
/// which is proposed when it is the only one.
///
/// ```
/// use officina_core::registry::verdict::subject_for_unfiled;
/// assert_eq!(subject_for_unfiled("api", &[]), (None, vec![]));
/// ```
pub fn subject_for_unfiled(project: &str, subjects: &[SubjectRecord]) -> (Option<i64>, Vec<i64>) {
    let choices: Vec<i64> = subjects
        .iter()
        .filter(|s| s.project == project && s.status.value != SubjectStatus::Archived)
        .map(|s| s.id)
        .collect();
    let proposed = match choices.as_slice() {
        [only] => Some(*only),
        _ => None,
    };
    (proposed, choices)
}

/// The row of `subject`, when it needs a verdict: `active`, every session
/// counting under it ended (D-77's live session keeps it off), anchored on
/// its latest prompted session; with none prompted, an empty-subject row.
fn subject_row(
    subject: &SubjectRecord,
    sessions: &[&SessionRecord],
    input: &VerdictInput,
) -> Option<Row> {
    if subject.status.value != SubjectStatus::Active || !sessions.iter().all(|s| s.is_ended()) {
        return None;
    }
    let (kind, session, proposal, at_ms) = match anchor(sessions) {
        Some(anchor) => (
            RowKind::Subject,
            Some(anchor),
            propose(anchor, input.model),
            anchor.ending().map_or(anchor.last_seen_ms, |e| e.at_ms),
        ),
        None => (
            RowKind::EmptySubject,
            None,
            empty_subject(),
            subject.status.at_ms,
        ),
    };
    let session_id = session.map(|s| s.session_id.as_str());
    if answered(input.confirmations, Some(subject.id), session_id, at_ms) {
        return None;
    }
    Some(Row {
        kind,
        key: format!("subject:{}", subject.id),
        project: subject.project.clone(),
        subject: Some(subject.id),
        session_id: session_id.map(str::to_owned),
        proposal,
        at_ms,
        proposed_subject: None,
        choices: Vec::new(),
    })
}

/// The row of an unfiled `session`, when it needs a verdict and a subject:
/// ended, interactive, prompted and not replaced in its process (AD-47,
/// D-75).
fn unfiled_row(session: &SessionRecord, input: &VerdictInput) -> Option<Row> {
    let end = session.ending()?;
    if session.is_non_interactive() || !session.saw_prompt() || session.is_superseded() {
        return None;
    }
    if answered(
        input.confirmations,
        None,
        Some(&session.session_id),
        end.at_ms,
    ) {
        return None;
    }
    let (proposed_subject, choices) = subject_for_unfiled(&session.project, input.subjects);
    Some(Row {
        kind: RowKind::UnfiledSession,
        key: format!("session:{}", session.session_id),
        project: session.project.clone(),
        subject: None,
        session_id: Some(session.session_id.clone()),
        proposal: propose(session, input.model),
        at_ms: end.at_ms,
        proposed_subject,
        choices,
    })
}

/// Every row that needs a verdict, the latest first (FR-7, AD-49, D-76).
///
/// ```
/// use officina_core::registry::proposal::ModelInput;
/// use officina_core::registry::verdict::{VerdictInput, rows};
/// let none = rows(&VerdictInput { subjects: &[], sessions: &[], confirmations: &[], model: &ModelInput::default() });
/// assert!(none.is_empty());
/// ```
pub fn rows(input: &VerdictInput) -> Vec<Row> {
    let mut by_subject: BTreeMap<i64, Vec<&SessionRecord>> = BTreeMap::new();
    let mut found = Vec::new();
    for session in input.sessions {
        match session.filed_subject() {
            Some(subject) => by_subject.entry(subject).or_default().push(session),
            None => found.extend(unfiled_row(session, input)),
        }
    }
    for subject in input.subjects {
        let sessions = by_subject.remove(&subject.id).unwrap_or_default();
        found.extend(subject_row(subject, &sessions, input));
    }
    found.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.key.cmp(&b.key)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::types::{
        HookEnd, ModelVerdict, ProposalSource, ReasonCode, Source, Sourced,
    };

    fn sourced<T>(value: T, at_ms: i64) -> Option<Sourced<T>> {
        Some(Sourced {
            value,
            src: Source::Hook,
            at_ms,
        })
    }

    fn subject(id: i64, status: SubjectStatus) -> SubjectRecord {
        SubjectRecord {
            id,
            project: "api".to_owned(),
            name: format!("subject {id}"),
            status: Sourced {
                value: status,
                src: Source::Author,
                at_ms: 10,
            },
            created_at_ms: 10,
            routine: false,
            merged_into: None,
        }
    }

    /// A session of `api`, filed under `filed`, prompted at `prompt` (if
    /// any), ended at `end` (if any).
    fn session(
        id: &str,
        filed: Option<i64>,
        prompt: Option<i64>,
        end: Option<i64>,
    ) -> SessionRecord {
        SessionRecord {
            session_id: id.to_owned(),
            project: "api".to_owned(),
            first_seen_ms: 1,
            last_seen_ms: end.or(prompt).unwrap_or(1),
            subject: filed.map(|id| Sourced {
                value: Some(id),
                src: Source::Author,
                at_ms: 1,
            }),
            listed: sourced(1, 1),
            last_prompt_at: prompt.and_then(|at| sourced(at, at)),
            last_stop_at: prompt.and_then(|at| sourced(at + 1, at + 1)),
            ended: end.and_then(|at| {
                sourced(
                    HookEnd {
                        reason: Some("prompt_input_exit".to_owned()),
                        claude_pid: None,
                    },
                    at,
                )
            }),
            ..SessionRecord::default()
        }
    }

    fn of(
        subjects: &[SubjectRecord],
        sessions: &[SessionRecord],
        confirmations: &[ConfirmationRecord],
    ) -> Vec<Row> {
        rows(&VerdictInput {
            subjects,
            sessions,
            confirmations,
            model: &ModelInput::default(),
        })
    }

    #[test]
    fn one_row_per_subject_not_per_session() {
        let subjects = [subject(1, SubjectStatus::Active)];
        let sessions = [
            session("a", Some(1), Some(100), Some(200)),
            session("b", Some(1), Some(300), Some(400)),
            session("c", Some(1), Some(500), Some(600)),
        ];
        let found = of(&subjects, &sessions, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].kind, found[0].key.as_str(), found[0].subject),
            (RowKind::Subject, "subject:1", Some(1))
        );
    }

    /// D-76: the latest session that saw a prompt anchors the row, even
    /// when a later one ended without a prompt or was replaced.
    #[test]
    fn anchor_is_the_latest_session_that_saw_a_prompt() {
        let subjects = [subject(1, SubjectStatus::Active)];
        let mut cleared = session("prompted", Some(1), Some(300), Some(400));
        cleared.successor = sourced("silent".to_owned(), 400);
        let sessions = [
            session("older", Some(1), Some(100), Some(200)),
            cleared,
            session("silent", Some(1), None, Some(500)),
        ];
        let found = of(&subjects, &sessions, &[]);
        assert_eq!(found[0].session_id.as_deref(), Some("prompted"));
        assert_eq!(found[0].at_ms, 400);
    }

    #[test]
    fn a_live_session_keeps_the_subject_off_the_list() {
        let subjects = [subject(1, SubjectStatus::Active)];
        let sessions = [
            session("ended", Some(1), Some(100), Some(200)),
            session("live", Some(1), Some(300), None),
        ];
        assert!(of(&subjects, &sessions, &[]).is_empty());
        // Nor is a subject that is not `active` listed.
        let resting = [subject(1, SubjectStatus::ToResume)];
        assert!(of(&resting, &sessions[..1], &[]).is_empty());
    }

    /// D-58: a confirmation answers the row until it is reopened, and only
    /// for the end it was made after.
    #[test]
    fn a_reopened_confirmation_does_not_count() {
        let subjects = [subject(1, SubjectStatus::Active)];
        let sessions = [session("a", Some(1), Some(100), Some(200))];
        let confirmed = |reopened_at_ms, confirmed_at_ms| ConfirmationRecord {
            subject_id: 1,
            session_id: Some("a".to_owned()),
            confirmed_at_ms,
            reopened_at_ms,
        };
        assert!(of(&subjects, &sessions, &[confirmed(None, 250)]).is_empty());
        assert_eq!(
            of(&subjects, &sessions, &[confirmed(Some(260), 250)]).len(),
            1
        );
        assert_eq!(
            of(&subjects, &sessions, &[confirmed(None, 150)]).len(),
            1,
            "made before this end: the session lived again since"
        );
        let elsewhere = ConfirmationRecord {
            session_id: Some("other".to_owned()),
            ..confirmed(None, 250)
        };
        assert_eq!(of(&subjects, &sessions, &[elsewhere]).len(), 1);
    }

    #[test]
    fn empty_subject_gets_its_own_row() {
        let subjects = [
            subject(1, SubjectStatus::Active),
            subject(2, SubjectStatus::Active),
            subject(3, SubjectStatus::ToResume),
        ];
        let sessions = [session("silent", Some(1), None, Some(200))];
        let found = of(&subjects, &sessions, &[]);
        let kinds: Vec<(RowKind, Option<i64>, Option<&str>)> = found
            .iter()
            .map(|r| (r.kind, r.subject, r.session_id.as_deref()))
            .collect();
        assert_eq!(
            kinds,
            [
                (RowKind::EmptySubject, Some(1), None),
                (RowKind::EmptySubject, Some(2), None)
            ]
        );
        let proposal = found[0].proposal;
        assert_eq!(
            (proposal.verdict, proposal.source, proposal.reason),
            (
                ModelVerdict::Finished,
                ProposalSource::Rules,
                ReasonCode::EmptySubject
            )
        );
        let answered = ConfirmationRecord {
            subject_id: 2,
            session_id: None,
            confirmed_at_ms: 20,
            reopened_at_ms: None,
        };
        assert_eq!(of(&subjects, &sessions, &[answered]).len(), 1);
    }

    #[test]
    fn unfiled_rows_need_an_end_a_prompt_and_no_successor() {
        let subjects = [
            subject(1, SubjectStatus::Active),
            subject(2, SubjectStatus::Archived),
        ];
        let mut replaced = session("replaced", None, Some(100), Some(200));
        replaced.successor = sourced("next".to_owned(), 200);
        let sessions = [
            session("ended", None, Some(100), Some(300)),
            session("live", None, Some(100), None),
            session("silent", None, None, Some(300)),
            replaced,
        ];
        let found: Vec<Row> = of(&subjects, &sessions, &[])
            .into_iter()
            .filter(|r| r.kind == RowKind::UnfiledSession)
            .collect();
        assert_eq!(found.len(), 1, "{found:?}");
        let row = &found[0];
        assert_eq!(
            (
                row.kind,
                row.key.as_str(),
                row.proposed_subject,
                row.choices.as_slice()
            ),
            (RowKind::UnfiledSession, "session:ended", Some(1), &[1][..])
        );
    }
}
