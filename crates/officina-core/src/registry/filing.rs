//! Filing (FR-5, AD-48, D-50, D-53, D-75): the subject a new session joins
//! at first sight, and which excerpts a subject keeps. The host creates the
//! session and runs these in the same transaction (AD-43, D-79).

use std::collections::BTreeMap;

use crate::board::NEVER_FILED_PROJECTS;

use super::types::{ExcerptRecord, SessionRecord, SubjectRecord, SubjectStatus};

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
    use crate::registry::types::{Source, Sourced};

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
}
