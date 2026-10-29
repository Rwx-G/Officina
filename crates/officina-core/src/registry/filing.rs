//! Filing (FR-5, AD-48, D-50, D-53, D-75): the subject a new session joins
//! at first sight, and which excerpts a subject keeps. The host creates the
//! session and runs these in the same transaction (AD-43, D-79).

use std::collections::BTreeMap;

use crate::board::NEVER_FILED_PROJECTS;

use super::confirm::checked_name;
use super::types::{
    AuthorRefusal, ExcerptRecord, SessionRecord, Source, Sourced, SubjectRecord, SubjectStatus,
};

/// True for the project names that never file (AD-48, D-53).
pub fn never_files(project: &str) -> bool {
    NEVER_FILED_PROJECTS.contains(&project)
}

/// The subject a new session of `project` joins at first sight (AD-48,
/// D-75): the single `active` subject of its project, whatever the
/// session's `SessionStart.source`; with none or several, it stays unfiled.
/// No rule ever creates a subject.
///
/// ```
/// use officina_core::registry::filing::filing_at_creation;
/// assert_eq!(filing_at_creation("api", &[]), None);
/// ```
pub fn filing_at_creation(project: &str, subjects: &[SubjectRecord]) -> Option<i64> {
    if never_files(project) {
        return None;
    }
    let mut active = subjects
        .iter()
        .filter(|s| s.project == project && s.status.value == SubjectStatus::Active);
    match (active.next(), active.next()) {
        (Some(only), None) => Some(only.id),
        _ => None,
    }
}

/// The subject a new session joins at creation, and who filed it (D-79 as
/// AD-81 step 5 amends it): the subject of the work order bound to it
/// first, source `author`, whatever its project; else
/// [`filing_at_creation`]'s rule, source `rules`.
///
/// ```
/// use officina_core::registry::filing::filing_for_new_session;
/// use officina_core::registry::types::Source;
/// assert_eq!(filing_for_new_session(Some(4), "api", &[]), Some((4, Source::Author)));
/// assert_eq!(filing_for_new_session(None, "api", &[]), None);
/// ```
pub fn filing_for_new_session(
    bound_subject: Option<i64>,
    project: &str,
    subjects: &[SubjectRecord],
) -> Option<(i64, Source)> {
    match bound_subject {
        Some(subject) => Some((subject, Source::Author)),
        None => filing_at_creation(project, subjects).map(|subject| (subject, Source::Rules)),
    }
}

/// True when binding a work order files its session, filed as `current`,
/// under `order_subject` (AD-81 step 5): `author` beats `rules`, so a
/// session the rules filed elsewhere, or did not file, moves; one the
/// author filed stays where he put it.
pub fn refiles_on_binding(current: Option<&Sourced<Option<i64>>>, order_subject: i64) -> bool {
    match current {
        None => true,
        Some(filing) => filing.src != Source::Author && filing.value != Some(order_subject),
    }
}

/// Where a work order's `New` subject goes once its launch started (AD-81
/// step 5, D-57, D-158).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewSubjectFiling {
    /// A subject of the project that is not archived has the name,
    /// ignoring case: the session goes there, none is created.
    Existing(i64),
    /// Created with this name, as D-57 stores it.
    Create(String),
    /// The name breaks another of D-57's rules: no subject, and the
    /// session is filed by the rules.
    Unfiled(AuthorRefusal),
}

/// What a `New(name)` choice of an order of `project` becomes at `started`
/// (D-158): the name checked by D-57's rules, a taken name filing into the
/// subject that holds it.
pub fn subject_for_new(name: &str, project: &str, subjects: &[SubjectRecord]) -> NewSubjectFiling {
    match checked_name(name, project, None, subjects) {
        Ok(name) => NewSubjectFiling::Create(name),
        Err(AuthorRefusal::NameTaken) => {
            let lower = name.trim().to_lowercase();
            subjects
                .iter()
                .find(|s| {
                    s.project == project
                        && s.status.value != SubjectStatus::Archived
                        && s.name.to_lowercase() == lower
                })
                .map_or(NewSubjectFiling::Unfiled(AuthorRefusal::NameTaken), |s| {
                    NewSubjectFiling::Existing(s.id)
                })
        }
        Err(refusal) => NewSubjectFiling::Unfiled(refusal),
    }
}

/// When a session's excerpts were recorded: its latest prompt or answer.
pub fn excerpt_time(excerpt: &ExcerptRecord) -> Option<i64> {
    excerpt.prompt_at_ms.max(excerpt.answer_at_ms)
}

/// The sessions whose excerpts go so that each subject keeps the latest
/// only (FR-4, P-9, D-50): within each subject, the established session
/// with the latest excerpts keeps them, and every other established one of
/// that subject loses its own. A session not yet known to be one the author
/// drove keeps its excerpts and clears no other, until it is (see
/// [`SessionRecord::is_established`]). Unfiled sessions keep theirs.
pub fn excerpts_to_clear(sessions: &[SessionRecord], excerpts: &[ExcerptRecord]) -> Vec<String> {
    let held: BTreeMap<&str, i64> = excerpts
        .iter()
        .filter_map(|e| Some((e.session_id.as_str(), excerpt_time(e)?)))
        .collect();
    let mut by_subject: BTreeMap<i64, Vec<(i64, &str)>> = BTreeMap::new();
    for session in sessions.iter().filter(|s| s.is_established()) {
        if let (Some(subject), Some(&at)) = (
            session.filed_subject(),
            held.get(session.session_id.as_str()),
        ) {
            by_subject
                .entry(subject)
                .or_default()
                .push((at, session.session_id.as_str()));
        }
    }
    let mut cleared: Vec<String> = by_subject
        .into_values()
        .flat_map(|mut holders| {
            holders.sort();
            holders.pop();
            holders.into_iter().map(|(_, id)| id.to_owned())
        })
        .collect();
    cleared.sort();
    cleared
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(id: i64, project: &str, status: SubjectStatus) -> SubjectRecord {
        SubjectRecord {
            id,
            project: project.to_owned(),
            name: format!("subject {id}"),
            status: Sourced {
                value: status,
                src: Source::Author,
                at_ms: 1,
            },
            created_at_ms: 1,
            routine: false,
            merged_into: None,
        }
    }

    #[test]
    fn a_new_session_joins_the_single_active_subject_of_its_project() {
        let one = [
            subject(1, "api", SubjectStatus::Active),
            subject(2, "api", SubjectStatus::ToResume),
            subject(3, "web", SubjectStatus::Active),
            subject(4, "api", SubjectStatus::Archived),
        ];
        assert_eq!(filing_at_creation("api", &one), Some(1));
        let two = [
            subject(1, "api", SubjectStatus::Active),
            subject(2, "api", SubjectStatus::Active),
        ];
        assert_eq!(filing_at_creation("api", &two), None, "several");
        assert_eq!(filing_at_creation("docs", &one), None, "none");
        for name in NEVER_FILED_PROJECTS {
            let only = [subject(1, name, SubjectStatus::Active)];
            assert_eq!(filing_at_creation(name, &only), None, "{name}");
        }
    }

    #[test]
    fn a_bound_order_files_its_session_first() {
        let subjects = [
            subject(1, "api", SubjectStatus::Active),
            subject(2, "web", SubjectStatus::ToResume),
        ];
        // The order's subject wins over the rule, whatever its project.
        assert_eq!(
            filing_for_new_session(Some(2), "api", &subjects),
            Some((2, Source::Author))
        );
        assert_eq!(
            filing_for_new_session(None, "api", &subjects),
            Some((1, Source::Rules))
        );
        // A project that never files by rule still takes the author's.
        for name in NEVER_FILED_PROJECTS {
            let only = [subject(1, name, SubjectStatus::Active)];
            assert_eq!(filing_for_new_session(None, name, &only), None, "{name}");
            assert_eq!(
                filing_for_new_session(Some(1), name, &only),
                Some((1, Source::Author)),
                "{name}"
            );
        }
    }

    fn filed(subject: Option<i64>, src: Source) -> Sourced<Option<i64>> {
        Sourced {
            value: subject,
            src,
            at_ms: 1,
        }
    }

    #[test]
    fn author_filing_beats_rule_filing() {
        assert!(refiles_on_binding(None, 7), "never filed");
        assert!(refiles_on_binding(Some(&filed(Some(1), Source::Rules)), 7));
        assert!(refiles_on_binding(Some(&filed(None, Source::Rules)), 7));
        assert!(
            !refiles_on_binding(Some(&filed(Some(7), Source::Rules)), 7),
            "already there"
        );
        // The author's own filing, or unfiling, stays.
        assert!(!refiles_on_binding(
            Some(&filed(Some(1), Source::Author)),
            7
        ));
        assert!(!refiles_on_binding(Some(&filed(None, Source::Author)), 7));
        for src in Source::ALL.iter().copied() {
            let moves = refiles_on_binding(Some(&filed(Some(1), src)), 7);
            assert_eq!(moves, src != Source::Author, "{src:?}");
        }
    }

    #[test]
    fn a_new_subject_name_follows_d_57_and_d_158() {
        let subjects = [
            subject(1, "api", SubjectStatus::Active),
            subject(2, "api", SubjectStatus::Archived),
            subject(3, "web", SubjectStatus::Active),
        ];
        assert_eq!(
            subject_for_new("  Rollout ", "api", &subjects),
            NewSubjectFiling::Create("Rollout".to_owned())
        );
        // `subject 1` is taken in `api`, ignoring case: filed there.
        assert_eq!(
            subject_for_new("SUBJECT 1", "api", &subjects),
            NewSubjectFiling::Existing(1)
        );
        // Archived, or of another project, the name is free.
        assert_eq!(
            subject_for_new("subject 2", "api", &subjects),
            NewSubjectFiling::Create("subject 2".to_owned())
        );
        assert_eq!(
            subject_for_new("subject 3", "api", &subjects),
            NewSubjectFiling::Create("subject 3".to_owned())
        );
        assert_eq!(
            subject_for_new(" ", "api", &subjects),
            NewSubjectFiling::Unfiled(AuthorRefusal::NameEmpty)
        );
    }
}
