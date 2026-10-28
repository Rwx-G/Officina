//! Subject transitions (AD-48, D-54 to D-58, D-77, D-95): what each author
//! action changes, or why it is refused, and when a session alive again
//! wakes its subject. Pure: the host loads the subjects and sessions an
//! action involves, applies the [`Plan`] in one transaction, and gives every
//! change the source `author` and the action's time (AD-39, AD-40).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::relay::params::is_forbidden_format;

use super::types::{AuthorRefusal, SessionRecord, SubjectRecord, SubjectStatus};

/// The longest subject name, in characters, once trimmed (D-57).
pub const NAME_MAX_CHARS: usize = 80;

/// Where a new subject's project comes from (D-95).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectOrigin {
    /// The selected sessions: their one project, and they are filed under
    /// the new subject in the same gesture.
    Sessions(Vec<String>),
    /// One of the projects the registry knows.
    Project(String),
}

/// An author action on subjects (AD-48, AD-54).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorAction {
    Create {
        name: String,
        origin: SubjectOrigin,
    },
    Rename {
        subject: i64,
        name: String,
    },
    /// `source`'s sessions move to `target`, and `source` is archived with
    /// `merged_into` (D-55).
    Merge {
        source: i64,
        target: i64,
    },
    /// One or several sessions, of any project (D-56).
    File {
        sessions: Vec<String>,
        subject: i64,
    },
    Unfile {
        sessions: Vec<String>,
    },
    /// With the "routine, could have run alone" box (FR-13, P-10).
    Archive {
        subject: i64,
        routine: bool,
    },
    Reopen {
        subject: i64,
    },
}

/// What an action reads: every subject, the sessions it names and those of
/// the subjects it touches, and the projects the registry knows.
#[derive(Debug, Clone, Copy)]
pub struct AuthorContext<'a> {
    pub subjects: &'a [SubjectRecord],
    pub sessions: &'a [SessionRecord],
    pub known_projects: &'a [String],
}

/// A subject a filing goes to: one that exists, or the one the plan
/// creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Existing(i64),
    Created,
}

/// A subject the plan creates, `active`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSubject {
    pub project: String,
    pub name: String,
}

/// What a plan changes on an existing subject; `None` leaves a column.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubjectChange {
    pub id: i64,
    pub name: Option<String>,
    pub status: Option<SubjectStatus>,
    pub routine: Option<bool>,
    pub merged_into: Option<i64>,
}

/// What an accepted action writes, in one transaction, with source
/// `author` at the action's time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub create: Option<NewSubject>,
    pub changes: Vec<SubjectChange>,
    /// Each session filed under a subject, or unfiled (`None`).
    pub filings: Vec<(String, Option<Target>)>,
    /// Sessions whose excerpts are cleared now (archive, FR-4).
    pub clear_excerpts: Vec<String>,
    /// The subject whose latest confirmation is stamped `reopened_at_ms`
    /// (D-58).
    pub stamp_reopen: Option<i64>,
    /// The subject whose sessions then keep the latest excerpts only
    /// (D-50).
    pub prune: Option<Target>,
    /// Truncate the WAL after the commit, so cleared text leaves the log
    /// (AD-44).
    pub checkpoint: bool,
}

/// `name` as a subject of `project` stores it (D-57): trimmed, 1 to
/// [`NAME_MAX_CHARS`] characters, no control or hidden formatting character
/// (AD-30's ranges), unique ignoring case among the project's subjects that
/// are not archived, `except` the subject being renamed.
///
/// ```
/// use officina_core::registry::confirm::checked_name;
/// use officina_core::registry::types::AuthorRefusal;
/// assert_eq!(checked_name("  rollout ", "api", None, &[]), Ok("rollout".to_owned()));
/// assert_eq!(checked_name(" ", "api", None, &[]), Err(AuthorRefusal::NameEmpty));
/// ```
pub fn checked_name(
    name: &str,
    project: &str,
    except: Option<i64>,
    subjects: &[SubjectRecord],
) -> Result<String, AuthorRefusal> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AuthorRefusal::NameEmpty);
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(AuthorRefusal::NameTooLong);
    }
    if name
        .chars()
        .any(|c| c.is_control() || is_forbidden_format(c))
    {
        return Err(AuthorRefusal::NameHasForbiddenCharacters);
    }
    let lower = name.to_lowercase();
    let taken = subjects.iter().any(|s| {
        s.project == project
            && s.status.value != SubjectStatus::Archived
            && Some(s.id) != except
            && s.name.to_lowercase() == lower
    });
    if taken {
        return Err(AuthorRefusal::NameTaken);
    }
    Ok(name.to_owned())
}

fn subject<'a>(ctx: &AuthorContext<'a>, id: i64) -> Result<&'a SubjectRecord, AuthorRefusal> {
    ctx.subjects
        .iter()
        .find(|s| s.id == id)
        .ok_or(AuthorRefusal::UnknownSubject)
}

fn open_subject<'a>(ctx: &AuthorContext<'a>, id: i64) -> Result<&'a SubjectRecord, AuthorRefusal> {
    let found = subject(ctx, id)?;
    if found.status.value == SubjectStatus::Archived {
        return Err(AuthorRefusal::Archived);
    }
    Ok(found)
}

/// The named sessions, each once, in the order given; refused when there is
/// none or one the registry does not hold.
fn named<'a>(
    ctx: &AuthorContext<'a>,
    ids: &[String],
) -> Result<Vec<&'a SessionRecord>, AuthorRefusal> {
    if ids.is_empty() {
        return Err(AuthorRefusal::NoSession);
    }
    let mut seen = BTreeSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str()))
        .map(|id| {
            ctx.sessions
                .iter()
                .find(|s| &s.session_id == id)
                .ok_or(AuthorRefusal::UnknownSession)
        })
        .collect()
}

fn sessions_of<'a>(ctx: &AuthorContext<'a>, id: i64) -> Vec<&'a SessionRecord> {
    ctx.sessions
        .iter()
        .filter(|s| s.filed_subject() == Some(id))
        .collect()
}

/// What `action` changes, or why it is refused (AD-48, D-54 to D-58,
/// D-95). Nothing is decided by time: the host stamps the action's.
pub fn plan(action: &AuthorAction, ctx: &AuthorContext) -> Result<Plan, AuthorRefusal> {
    match action {
        AuthorAction::Create { name, origin } => {
            let (project, filed) = match origin {
                SubjectOrigin::Sessions(ids) => {
                    let sessions = named(ctx, ids)?;
                    let projects: Vec<&str> = sessions
                        .iter()
                        .map(|s| s.project.as_str())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    let [project] = projects.as_slice() else {
                        return Err(AuthorRefusal::SessionsOfSeveralProjects);
                    };
                    ((*project).to_owned(), sessions)
                }
                SubjectOrigin::Project(project) => {
                    if !ctx.known_projects.contains(project) {
                        return Err(AuthorRefusal::UnknownProject);
                    }
                    (project.clone(), Vec::new())
                }
            };
            let name = checked_name(name, &project, None, ctx.subjects)?;
            Ok(Plan {
                create: Some(NewSubject { project, name }),
                filings: filed
                    .iter()
                    .map(|s| (s.session_id.clone(), Some(Target::Created)))
                    .collect(),
                prune: Some(Target::Created),
                ..Plan::default()
            })
        }
        AuthorAction::Rename { subject, name } => {
            let found = open_subject(ctx, *subject)?;
            let name = checked_name(name, &found.project, Some(found.id), ctx.subjects)?;
            Ok(Plan {
                changes: vec![SubjectChange {
                    id: found.id,
                    name: Some(name),
                    ..SubjectChange::default()
                }],
                ..Plan::default()
            })
        }
        AuthorAction::Merge { source, target } => {
            subject(ctx, *source)?;
            subject(ctx, *target)?;
            if source == target {
                return Err(AuthorRefusal::SameSubject);
            }
            let from = open_subject(ctx, *source)?;
            let into = open_subject(ctx, *target)?;
            let either_active = [from, into]
                .iter()
                .any(|s| s.status.value == SubjectStatus::Active);
            Ok(Plan {
                changes: vec![
                    SubjectChange {
                        id: from.id,
                        status: Some(SubjectStatus::Archived),
                        merged_into: Some(into.id),
                        ..SubjectChange::default()
                    },
                    SubjectChange {
                        id: into.id,
                        status: (either_active && into.status.value != SubjectStatus::Active)
                            .then_some(SubjectStatus::Active),
                        ..SubjectChange::default()
                    },
                ]
                .into_iter()
                .filter(|change| change.status.is_some() || change.merged_into.is_some())
                .collect(),
                filings: sessions_of(ctx, from.id)
                    .iter()
                    .map(|s| (s.session_id.clone(), Some(Target::Existing(into.id))))
                    .collect(),
                prune: Some(Target::Existing(into.id)),
                ..Plan::default()
            })
        }
        AuthorAction::File { sessions, subject } => {
            if sessions.is_empty() {
                return Err(AuthorRefusal::NoSession);
            }
            let into = open_subject(ctx, *subject)?;
            let filed = named(ctx, sessions)?;
            let wakes =
                into.status.value == SubjectStatus::ToResume && filed.iter().any(|s| !s.is_ended());
            Ok(Plan {
                changes: wakes
                    .then(|| SubjectChange {
                        id: into.id,
                        status: Some(SubjectStatus::Active),
                        ..SubjectChange::default()
                    })
                    .into_iter()
                    .collect(),
                filings: filed
                    .iter()
                    .map(|s| (s.session_id.clone(), Some(Target::Existing(into.id))))
                    .collect(),
                prune: Some(Target::Existing(into.id)),
                ..Plan::default()
            })
        }
        AuthorAction::Unfile { sessions } => Ok(Plan {
            filings: named(ctx, sessions)?
                .iter()
                .map(|s| (s.session_id.clone(), None))
                .collect(),
            ..Plan::default()
        }),
        AuthorAction::Archive { subject, routine } => {
            let found = open_subject(ctx, *subject)?;
            Ok(Plan {
                changes: vec![SubjectChange {
                    id: found.id,
                    status: Some(SubjectStatus::Archived),
                    routine: Some(*routine),
                    ..SubjectChange::default()
                }],
                clear_excerpts: sessions_of(ctx, found.id)
                    .iter()
                    .map(|s| s.session_id.clone())
                    .collect(),
                checkpoint: true,
                ..Plan::default()
            })
        }
        AuthorAction::Reopen { subject: id } => {
            let found = subject(ctx, *id)?;
            if found.merged_into.is_some() {
                return Err(AuthorRefusal::Merged);
            }
            if found.status.value != SubjectStatus::Archived {
                return Err(AuthorRefusal::NotArchived);
            }
            Ok(Plan {
                changes: vec![SubjectChange {
                    id: found.id,
                    status: Some(SubjectStatus::ToResume),
                    ..SubjectChange::default()
                }],
                stamp_reopen: Some(found.id),
                ..Plan::default()
            })
        }
    }
}

/// A subject a session alive again wakes (D-77): it becomes `active` with
/// source `rules`; when it was archived, its latest confirmation is stamped
/// `reopened_at_ms` (D-58).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wake {
    pub subject: i64,
    pub stamp_reopen: bool,
}

/// What a session alive again wakes (D-77): `before` was ended, `after`
/// no longer is, and the subject it counts under was `to_resume` or
/// `archived`. A merged subject holds no session.
pub fn wake(
    before: &SessionRecord,
    after: &SessionRecord,
    subjects: &[SubjectRecord],
) -> Option<Wake> {
    if !before.is_ended() || after.is_ended() {
        return None;
    }
    let id = after.filed_subject()?;
    let found = subjects.iter().find(|s| s.id == id)?;
    match found.status.value {
        SubjectStatus::Active => None,
        SubjectStatus::ToResume => Some(Wake {
            subject: id,
            stamp_reopen: false,
        }),
        SubjectStatus::Archived => Some(Wake {
            subject: id,
            stamp_reopen: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::types::{Source, Sourced};

    fn subject(id: i64, name: &str, status: SubjectStatus) -> SubjectRecord {
        SubjectRecord {
            id,
            project: "api".to_owned(),
            name: name.to_owned(),
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
    fn names_are_trimmed_bounded_clean_and_unique_ignoring_case() {
        let subjects = [
            subject(1, "Rollout", SubjectStatus::Active),
            subject(2, "Old work", SubjectStatus::Archived),
        ];
        let check = |name: &str| checked_name(name, "api", None, &subjects);
        assert_eq!(check(" Renewal\t"), Ok("Renewal".to_owned()));
        assert_eq!(
            check(&"é".repeat(NAME_MAX_CHARS)).map(|n| n.chars().count()),
            Ok(80)
        );
        assert_eq!(
            check(&"é".repeat(NAME_MAX_CHARS + 1)),
            Err(AuthorRefusal::NameTooLong)
        );
        assert_eq!(check("\u{3000}"), Err(AuthorRefusal::NameEmpty));
        for bad in ["a\u{7}b", "a\nb", "a\u{202E}b", "a\u{200B}b", "a\u{E0041}b"] {
            assert_eq!(
                check(bad),
                Err(AuthorRefusal::NameHasForbiddenCharacters),
                "{bad:?}"
            );
        }
        assert_eq!(check("ROLLOUT"), Err(AuthorRefusal::NameTaken));
        assert_eq!(
            check("old WORK"),
            Ok("old WORK".to_owned()),
            "archived names are free"
        );
        assert_eq!(
            checked_name("rollout", "api", Some(1), &subjects),
            Ok("rollout".to_owned())
        );
        assert_eq!(
            checked_name("rollout", "web", None, &subjects),
            Ok("rollout".to_owned())
        );
    }
}
