//! Subject transitions (AD-48, AD-49, D-54 to D-63, D-77, D-95): what each
//! author action changes, or why it is refused; what confirming a triage
//! row writes; and when a session alive again wakes its subject. Pure: the
//! host loads what an action or a confirmation reads, applies the [`Plan`]
//! or [`ConfirmPlan`] in one transaction, and gives every change the source
//! `author` and the action's time (AD-39, AD-40).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::relay::params::is_forbidden_format;

use super::proposal::{ModelInput, Proposal, cut_mid_turn};
use super::retention::EXCERPT_TTL_MS;
use super::types::{
    AuthorRefusal, ConfirmRefusal, ConfirmationRecord, Ending, ExcerptRecord, NotNeeded, RowKind,
    SessionRecord, Settlement, SubjectRecord, SubjectStatus, Verdict,
};
use super::verdict::{Row, VerdictInput, anchor, rows};

/// The longest subject name, in characters, once trimmed (D-57).
pub const NAME_MAX_CHARS: usize = 80;

/// Where a new subject's project comes from (D-95); the window's
/// `create_subject` command takes it as sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    /// The author's verdict on one ended session from the board (story
    /// 4.15, D-536), or `None` to take it back. It changes no subject: a
    /// subject's own verdict stays its triage row's.
    Settle {
        session: String,
        settlement: Option<Settlement>,
    },
}

impl AuthorAction {
    /// The sessions and subjects the action reads beyond every subject:
    /// those it names, and the sessions of the subjects it touches. Here,
    /// beside the variants, so a new action cannot compile without saying
    /// what it reads (quality audit M1).
    ///
    /// ```
    /// use officina_core::registry::confirm::AuthorAction;
    /// let merge = AuthorAction::Merge { source: 1, target: 2 };
    /// assert_eq!(merge.involved(), (Vec::new(), vec![1, 2]));
    /// ```
    pub fn involved(&self) -> (Vec<String>, Vec<i64>) {
        match self {
            AuthorAction::Create {
                origin: SubjectOrigin::Sessions(ids),
                ..
            } => (ids.clone(), Vec::new()),
            AuthorAction::Create { .. } => (Vec::new(), Vec::new()),
            AuthorAction::Rename { subject, .. }
            | AuthorAction::Archive { subject, .. }
            | AuthorAction::Reopen { subject } => (Vec::new(), vec![*subject]),
            AuthorAction::Merge { source, target } => (Vec::new(), vec![*source, *target]),
            AuthorAction::File { sessions, subject } => (sessions.clone(), vec![*subject]),
            AuthorAction::Unfile { sessions } => (sessions.clone(), Vec::new()),
            AuthorAction::Settle { session, .. } => (vec![session.clone()], Vec::new()),
        }
    }
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
    /// Each session whose board verdict is recorded, or taken back
    /// (`None`).
    pub settle: Vec<(String, Option<Settlement>)>,
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
        AuthorAction::Settle {
            session,
            settlement,
        } => Ok(Plan {
            settle: named(ctx, std::slice::from_ref(session))?
                .iter()
                .map(|s| (s.session_id.clone(), *settlement))
                .collect(),
            ..Plan::default()
        }),
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

/// Where an unfiled row's session is filed before its verdict (AD-49).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum FileUnder {
    /// An open subject, by id: the one proposed, or another the author chose.
    Subject { id: i64 },
    /// A new subject of the session's project, named by the author.
    New { name: String },
}

/// One triage row the author confirms (D-63): the row's key and the
/// proposal the window showed, the verdict chosen, the routine box of a
/// `finished` verdict (D-62), and for an unfiled row the subject it goes
/// under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ConfirmItem {
    pub key: String,
    pub shown: Proposal,
    pub verdict: Verdict,
    /// "Routine, could have run alone", read on a `finished` verdict only.
    #[serde(default)]
    pub routine: bool,
    /// Read on an unfiled row only.
    #[serde(default)]
    pub file_under: Option<FileUnder>,
}

/// What became of one item (AD-49: answered per item).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemOutcome {
    /// The verdict applied to `subject`.
    Confirmed { subject: i64, verdict: Verdict },
    /// The session was filed under `subject`, and the verdict was not
    /// applied, for `cause` (D-60).
    VerdictNotNeeded { subject: i64, cause: NotNeeded },
    /// Nothing was written for this item.
    Refused { refusal: ConfirmRefusal },
    /// A subject rule refused the filing; nothing was written for this
    /// item (D-54, D-57).
    FilingRefused { refusal: AuthorRefusal },
}

impl ItemOutcome {
    /// True when the item wrote to the registry: a later item of the same
    /// batch must then be planned against the registry as it is now.
    ///
    /// ```
    /// use officina_core::registry::confirm::ItemOutcome;
    /// use officina_core::registry::types::ConfirmRefusal;
    /// use officina_core::registry::types::Verdict;
    /// assert!(ItemOutcome::Confirmed { subject: 1, verdict: Verdict::Finished }.wrote());
    /// assert!(!ItemOutcome::Refused { refusal: ConfirmRefusal::ALL[0] }.wrote());
    /// ```
    pub fn wrote(&self) -> bool {
        match self {
            ItemOutcome::Confirmed { .. } | ItemOutcome::VerdictNotNeeded { .. } => true,
            ItemOutcome::Refused { .. } | ItemOutcome::FilingRefused { .. } => false,
        }
    }
}

/// One item's key and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ItemAnswer {
    pub key: String,
    pub outcome: ItemOutcome,
}

/// What confirming a row reads: the registry as the view reads it, and the
/// projects a new subject may take.
#[derive(Debug, Clone, Copy)]
pub struct TriageContext<'a> {
    pub subjects: &'a [SubjectRecord],
    pub sessions: &'a [SessionRecord],
    pub confirmations: &'a [ConfirmationRecord],
    pub excerpts: &'a [ExcerptRecord],
    pub model: &'a ModelInput,
    pub known_projects: &'a [String],
}

/// What a confirmation keeps of what the decision rested on (AD-49, FR-4
/// amendment 4): the excerpts it copies and the facts beside them, until
/// those excerpts were due to expire (D-85). It is the dry run's evaluation
/// set (story 3.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationInput {
    pub prompt: Option<String>,
    pub answer_head: Option<String>,
    pub answer_tail: Option<String>,
    /// How the session ended: the `SessionEnd` reason as Claude Code sent
    /// it, `interrupted`, or a background session's state.
    pub end_reason: Option<String>,
    pub cut_mid_turn: bool,
    pub expires_at_ms: i64,
}

/// The verdict a confirmation applies to its subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// The row's session: the anchor, or the unfiled session; `None` for an
    /// empty subject (D-76, D-88).
    pub session_id: Option<String>,
    /// The proposal answered, copied as it was shown (P-11): verdict,
    /// source, reason code and call id, never prose.
    pub proposal: Proposal,
    pub verdict: Verdict,
    /// `archived` for `finished`, `to_resume` otherwise.
    pub status: SubjectStatus,
    /// The routine box, set on `finished` only (D-62).
    pub routine: Option<bool>,
    /// The subject's sessions whose excerpts go: all of them on `finished`.
    pub clear_excerpts: Vec<String>,
    /// `None` for a row with no excerpt to copy, an empty subject's
    /// included.
    pub input: Option<ConfirmationInput>,
}

/// What confirming one row writes, in the order the host applies it: the
/// filing of an unfiled row (an author [`Plan`]), then the verdict, unless
/// the filed session no longer decides its subject's (D-60).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmPlan {
    pub filing: Option<Plan>,
    /// The subject the verdict is about, or the session was filed under.
    pub subject: Target,
    pub decision: Result<Decision, NotNeeded>,
}

impl ConfirmPlan {
    /// The item's outcome, once the host knows the subject's id.
    pub fn outcome(&self, subject: i64) -> ItemOutcome {
        match &self.decision {
            Ok(decision) => ItemOutcome::Confirmed {
                subject,
                verdict: decision.verdict,
            },
            Err(cause) => ItemOutcome::VerdictNotNeeded {
                subject,
                cause: *cause,
            },
        }
    }

    /// True when the plan clears excerpts: the WAL is then truncated after
    /// the commit, so the cleared text leaves the log (AD-44).
    pub fn checkpoint(&self) -> bool {
        self.decision
            .as_ref()
            .is_ok_and(|decision| !decision.clear_excerpts.is_empty())
    }
}

/// How `ending` reads in [`ConfirmationInput::end_reason`].
fn end_reason(ending: Ending) -> Option<String> {
    match ending {
        Ending::SessionEnd { reason } => reason,
        Ending::Interrupted => Some("interrupted".to_owned()),
        Ending::State { state } => serde_json::to_value(state)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned)),
    }
}

/// The input a confirmation of `session`'s row keeps (AD-49, D-85): its
/// excerpts and the facts beside them, expiring when the earliest excerpt
/// it copies was due to; `None` when the session holds no excerpt.
///
/// ```
/// use officina_core::registry::confirm::confirmation_input;
/// use officina_core::registry::retention::EXCERPT_TTL_MS;
/// use officina_core::registry::types::{ExcerptRecord, SessionRecord};
/// let session = SessionRecord { session_id: "s".into(), ..SessionRecord::default() };
/// let excerpt = ExcerptRecord { session_id: "s".into(), prompt: Some("p".into()), prompt_at_ms: Some(10), ..ExcerptRecord::default() };
/// assert_eq!(confirmation_input(&session, Some(&excerpt)).unwrap().expires_at_ms, 10 + EXCERPT_TTL_MS);
/// assert!(confirmation_input(&session, None).is_none());
/// ```
pub fn confirmation_input(
    session: &SessionRecord,
    excerpt: Option<&ExcerptRecord>,
) -> Option<ConfirmationInput> {
    let excerpt = excerpt?;
    let earliest = [excerpt.prompt_at_ms, excerpt.answer_at_ms]
        .into_iter()
        .flatten()
        .min()?;
    Some(ConfirmationInput {
        prompt: excerpt.prompt.clone(),
        answer_head: excerpt.answer_head.clone(),
        answer_tail: excerpt.answer_tail.clone(),
        end_reason: session.ending().and_then(|e| end_reason(e.value)),
        cut_mid_turn: cut_mid_turn(session),
        expires_at_ms: earliest.saturating_add(EXCERPT_TTL_MS),
    })
}

/// The verdict of `item` on `row`, whose subject counts `members`.
fn decide(
    item: &ConfirmItem,
    row: &Row,
    members: &[&SessionRecord],
    input: Option<ConfirmationInput>,
) -> Decision {
    let finished = item.verdict == Verdict::Finished;
    Decision {
        session_id: row.session_id.clone(),
        proposal: row.proposal,
        verdict: item.verdict,
        status: if finished {
            SubjectStatus::Archived
        } else {
            SubjectStatus::ToResume
        },
        routine: finished.then_some(item.routine),
        clear_excerpts: if finished {
            members.iter().map(|s| s.session_id.clone()).collect()
        } else {
            Vec::new()
        },
        input,
    }
}

/// What confirming `item` writes, or why nothing is written for it (AD-49,
/// D-60, D-63, D-76): the row is found again by its key among the rows
/// computed now, and refused when its proposal is not the one shown, as
/// `approve_catalogue` refuses a hash that moved (AD-24). An unfiled row is
/// filed first, under the subject the item names or a new one; the verdict
/// then applies only when the filed session is its subject's latest that
/// saw a prompt and no session of the subject is live.
pub fn confirm_item(item: &ConfirmItem, ctx: &TriageContext) -> Result<ConfirmPlan, ItemOutcome> {
    let refused = |refusal| ItemOutcome::Refused { refusal };
    let row = rows(&VerdictInput {
        subjects: ctx.subjects,
        sessions: ctx.sessions,
        confirmations: ctx.confirmations,
        model: ctx.model,
    })
    .into_iter()
    .find(|row| row.key == item.key)
    .ok_or(refused(ConfirmRefusal::RowGone))?;
    if row.proposal != item.shown {
        return Err(refused(ConfirmRefusal::ProposalChanged));
    }
    let session = row
        .session_id
        .as_deref()
        .and_then(|id| ctx.sessions.iter().find(|s| s.session_id == id));
    let input = session.and_then(|session| {
        let excerpt = ctx
            .excerpts
            .iter()
            .find(|e| e.session_id == session.session_id);
        confirmation_input(session, excerpt)
    });
    let members_of = |id: i64, also: Option<&str>| -> Vec<&SessionRecord> {
        ctx.sessions
            .iter()
            .filter(|s| s.filed_subject() == Some(id) || Some(s.session_id.as_str()) == also)
            .collect()
    };
    match (row.kind, row.subject, session) {
        (RowKind::UnfiledSession, _, Some(session)) => {
            let id = session.session_id.clone();
            let (action, target) = match &item.file_under {
                None => return Err(refused(ConfirmRefusal::NoSubject)),
                Some(FileUnder::Subject { id: subject }) => (
                    AuthorAction::File {
                        sessions: vec![id],
                        subject: *subject,
                    },
                    Target::Existing(*subject),
                ),
                Some(FileUnder::New { name }) => (
                    AuthorAction::Create {
                        name: name.clone(),
                        origin: SubjectOrigin::Sessions(vec![id]),
                    },
                    Target::Created,
                ),
            };
            let filing = plan(
                &action,
                &AuthorContext {
                    subjects: ctx.subjects,
                    sessions: ctx.sessions,
                    known_projects: ctx.known_projects,
                },
            )
            .map_err(|refusal| ItemOutcome::FilingRefused { refusal })?;
            let members = match target {
                Target::Existing(subject) => members_of(subject, Some(&session.session_id)),
                Target::Created => vec![session],
            };
            let decision = if members.iter().any(|s| !s.is_ended()) {
                Err(NotNeeded::LiveSession)
            } else if anchor(&members).map(|a| &a.session_id) != Some(&session.session_id) {
                Err(NotNeeded::NotLatest)
            } else {
                Ok(decide(item, &row, &members, input))
            };
            Ok(ConfirmPlan {
                filing: Some(filing),
                subject: target,
                decision,
            })
        }
        (RowKind::Subject | RowKind::EmptySubject, Some(subject), _) => {
            let members = members_of(subject, None);
            Ok(ConfirmPlan {
                filing: None,
                subject: Target::Existing(subject),
                decision: Ok(decide(item, &row, &members, input)),
            })
        }
        // A row always names what it is about: an unfiled row its session,
        // a subject row its subject; anything else is no row of this build.
        _ => Err(refused(ConfirmRefusal::RowGone)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::proposal::{ModelAnswer, empty_subject};
    use crate::registry::retention::{COPY_LIFETIME_MS, PASS_INTERVAL_MS, PRIVACY_BOUND_MS};
    use crate::registry::types::{
        HookEnd, ModelVerdict, ProposalSource, ReasonCode, Source, Sourced,
    };

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

    fn hook<T>(value: T, at_ms: i64) -> Option<Sourced<T>> {
        Some(Sourced {
            value,
            src: Source::Hook,
            at_ms,
        })
    }

    /// A session of `api` seen live, filed under `filed` by the author,
    /// prompted at `prompt`, its turn closed by a `Stop` one ms later, and
    /// ended at `end` when given.
    fn session(id: &str, filed: Option<i64>, prompt: i64, end: Option<i64>) -> SessionRecord {
        SessionRecord {
            session_id: id.to_owned(),
            project: "api".to_owned(),
            first_seen_ms: 1,
            last_seen_ms: end.unwrap_or(prompt),
            subject: filed.map(|id| Sourced {
                value: Some(id),
                src: Source::Author,
                at_ms: 1,
            }),
            listed: Some(Sourced {
                value: 1,
                src: Source::Agents,
                at_ms: 1,
            }),
            last_prompt_at: hook(prompt, prompt),
            last_stop_at: hook(prompt + 1, prompt + 1),
            ended: end.and_then(|at| {
                hook(
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

    /// Story 4.15: settling a session writes its board verdict, or takes
    /// it back, and nothing else; an unknown session is refused.
    #[test]
    fn settling_a_session_writes_only_its_verdict() {
        let subjects = [subject(1, "Rollout", SubjectStatus::Archived)];
        let sessions = [session("s", Some(1), 10, Some(20))];
        let ctx = AuthorContext {
            subjects: &subjects,
            sessions: &sessions,
            known_projects: PROJECTS,
        };
        for settlement in Settlement::ALL.iter().copied().map(Some).chain([None]) {
            let action = AuthorAction::Settle {
                session: "s".to_owned(),
                settlement,
            };
            assert_eq!(action.involved(), (vec!["s".to_owned()], Vec::new()));
            assert_eq!(
                plan(&action, &ctx),
                Ok(Plan {
                    settle: vec![("s".to_owned(), settlement)],
                    ..Plan::default()
                })
            );
        }
        let unknown = AuthorAction::Settle {
            session: "gone".to_owned(),
            settlement: Some(Settlement::Done),
        };
        assert_eq!(plan(&unknown, &ctx), Err(AuthorRefusal::UnknownSession));
    }

    fn excerpt(id: &str, at: i64) -> ExcerptRecord {
        ExcerptRecord {
            session_id: id.to_owned(),
            prompt: Some(format!("prompt of {id}")),
            prompt_at_ms: Some(at),
            answer_head: Some(format!("answer of {id}")),
            answer_tail: None,
            answer_at_ms: Some(at + 1),
        }
    }

    const PROJECTS: &[String] = &[];

    fn context<'a>(
        subjects: &'a [SubjectRecord],
        sessions: &'a [SessionRecord],
        excerpts: &'a [ExcerptRecord],
        model: &'a ModelInput,
    ) -> TriageContext<'a> {
        TriageContext {
            subjects,
            sessions,
            confirmations: &[],
            excerpts,
            model,
            known_projects: PROJECTS,
        }
    }

    fn default_to_resume() -> Proposal {
        Proposal {
            verdict: ModelVerdict::ToResume,
            source: ProposalSource::Rules,
            reason: ReasonCode::DefaultToResume,
            call_id: None,
        }
    }

    fn item(key: &str, shown: Proposal, verdict: Verdict) -> ConfirmItem {
        ConfirmItem {
            key: key.to_owned(),
            shown,
            verdict,
            routine: false,
            file_under: None,
        }
    }

    /// AD-49, D-63: the author decides on what was shown; a proposal that
    /// moved since is refused, and so is a row that no longer needs a
    /// verdict.
    #[test]
    fn changed_proposal_is_refused() {
        let subjects = [subject(1, "rollout", SubjectStatus::Active)];
        let sessions = [session("a", Some(1), 100, Some(200))];
        let none = ModelInput::default();
        let ctx = context(&subjects, &sessions, &[], &none);
        let refused = |refusal| Err(ItemOutcome::Refused { refusal });
        let shown_state = Proposal {
            verdict: ModelVerdict::Finished,
            source: ProposalSource::State,
            reason: ReasonCode::StateDone,
            call_id: None,
        };
        assert_eq!(
            confirm_item(&item("subject:1", shown_state, Verdict::Finished), &ctx),
            refused(ConfirmRefusal::ProposalChanged)
        );
        assert!(
            confirm_item(
                &item("subject:1", default_to_resume(), Verdict::Finished),
                &ctx
            )
            .is_ok(),
            "positive control: the proposal shown"
        );
        assert_eq!(
            confirm_item(
                &item("subject:9", default_to_resume(), Verdict::Finished),
                &ctx
            ),
            refused(ConfirmRefusal::RowGone)
        );
    }

    /// Story 3.11 AC 4 (D-63, D-88): an item answering a model proposal
    /// carries the call that answered; a newer answer for the same row
    /// makes it refused as changed, and the call id the author saw is the
    /// one the confirmation copies.
    #[test]
    fn model_item_is_refused_when_the_answer_changed() {
        let subjects = [subject(1, "rollout", SubjectStatus::Active)];
        let sessions = [session("a", Some(1), 100, Some(200))];
        let refused = |refusal| Err(ItemOutcome::Refused { refusal });
        let answered = ModelInput {
            enabled: true,
            answers: [(
                "a".to_owned(),
                ModelAnswer {
                    verdict: ModelVerdict::Finished,
                    call_id: Some(5),
                },
            )]
            .into(),
        };
        let ctx = context(&subjects, &sessions, &[], &answered);
        let model = |call_id| Proposal {
            verdict: ModelVerdict::Finished,
            source: ProposalSource::Model,
            reason: ReasonCode::Model,
            call_id,
        };
        assert_eq!(
            confirm_item(&item("subject:1", model(Some(4)), Verdict::Finished), &ctx),
            refused(ConfirmRefusal::ProposalChanged),
            "another call's answer"
        );
        let planned =
            confirm_item(&item("subject:1", model(Some(5)), Verdict::Finished), &ctx).unwrap();
        assert_eq!(
            planned.decision.unwrap().proposal.call_id,
            Some(5),
            "the call id is copied (D-88)"
        );
    }

    /// AD-49, D-60: an unfiled row is filed first, under the subject named
    /// or a new one, then decided when it is its subject's latest prompted
    /// session; without a subject, or into an archived one, nothing is.
    #[test]
    fn unfiled_row_is_filed_then_decided() {
        let subjects = [
            subject(1, "rollout", SubjectStatus::Active),
            subject(2, "old", SubjectStatus::Archived),
        ];
        let sessions = [
            session("older", Some(1), 100, Some(200)),
            session("loose", None, 300, Some(400)),
        ];
        let excerpts = [excerpt("loose", 300)];
        let none = ModelInput::default();
        let ctx = context(&subjects, &sessions, &excerpts, &none);
        let mut confirmed = ConfirmItem {
            routine: true,
            file_under: Some(FileUnder::Subject { id: 1 }),
            ..item("session:loose", default_to_resume(), Verdict::Finished)
        };
        let planned = confirm_item(&confirmed, &ctx).unwrap();
        assert_eq!(
            planned.filing.as_ref().map(|f| f.filings.clone()),
            Some(vec![("loose".to_owned(), Some(Target::Existing(1)))])
        );
        assert_eq!(planned.subject, Target::Existing(1));
        assert_eq!(
            planned.outcome(1),
            ItemOutcome::Confirmed {
                subject: 1,
                verdict: Verdict::Finished
            }
        );
        let decision = planned.decision.as_ref().unwrap();
        assert_eq!(
            (
                decision.session_id.as_deref(),
                decision.status,
                decision.routine,
                decision.clear_excerpts.clone()
            ),
            (
                Some("loose"),
                SubjectStatus::Archived,
                Some(true),
                vec!["older".to_owned(), "loose".to_owned()]
            )
        );
        assert!(planned.checkpoint());
        assert_eq!(
            decision.input.as_ref().and_then(|i| i.prompt.as_deref()),
            Some("prompt of loose")
        );

        confirmed.file_under = Some(FileUnder::New {
            name: "loose work".into(),
        });
        confirmed.verdict = Verdict::ToResume;
        let created = confirm_item(&confirmed, &ctx).unwrap();
        assert_eq!(created.subject, Target::Created);
        assert_eq!(
            created
                .filing
                .as_ref()
                .and_then(|f| f.create.as_ref())
                .map(|c| c.name.as_str()),
            Some("loose work")
        );
        let decision = created.decision.as_ref().unwrap();
        assert_eq!(
            (decision.status, decision.routine),
            (SubjectStatus::ToResume, None)
        );
        assert!(decision.clear_excerpts.is_empty() && !created.checkpoint());

        confirmed.file_under = None;
        assert_eq!(
            confirm_item(&confirmed, &ctx),
            Err(ItemOutcome::Refused {
                refusal: ConfirmRefusal::NoSubject
            })
        );
        confirmed.file_under = Some(FileUnder::Subject { id: 2 });
        assert_eq!(
            confirm_item(&confirmed, &ctx),
            Err(ItemOutcome::FilingRefused {
                refusal: AuthorRefusal::Archived
            })
        );
    }

    /// D-60: filed under a subject whose latest prompted session is
    /// another, or which holds a live session, the session is filed and
    /// the verdict waits for the subject's own row.
    #[test]
    fn verdict_not_needed_is_answered_with_its_cause() {
        let subjects = [
            subject(1, "rollout", SubjectStatus::Active),
            subject(2, "renewal", SubjectStatus::ToResume),
        ];
        let sessions = [
            session("newer", Some(1), 500, Some(600)),
            session("live", Some(2), 700, None),
            session("loose", None, 300, Some(400)),
        ];
        let none = ModelInput::default();
        let ctx = context(&subjects, &sessions, &[], &none);
        let under = |id| ConfirmItem {
            file_under: Some(FileUnder::Subject { id }),
            ..item("session:loose", default_to_resume(), Verdict::Finished)
        };
        let later = confirm_item(&under(1), &ctx).unwrap();
        assert!(later.filing.is_some(), "filed all the same");
        assert_eq!(
            later.outcome(1),
            ItemOutcome::VerdictNotNeeded {
                subject: 1,
                cause: NotNeeded::NotLatest
            }
        );
        assert!(!later.checkpoint());
        let live = confirm_item(&under(2), &ctx).unwrap();
        assert_eq!(
            live.outcome(2),
            ItemOutcome::VerdictNotNeeded {
                subject: 2,
                cause: NotNeeded::LiveSession
            }
        );
    }

    /// D-76, D-88: an empty subject's row is confirmed like any row, with
    /// no session and no input.
    #[test]
    fn empty_subject_row_archives_without_input() {
        let subjects = [subject(1, "nothing yet", SubjectStatus::Active)];
        let none = ModelInput::default();
        let ctx = context(&subjects, &[], &[], &none);
        let planned = confirm_item(
            &ConfirmItem {
                routine: true,
                ..item("subject:1", empty_subject(), Verdict::Finished)
            },
            &ctx,
        )
        .unwrap();
        assert_eq!(planned.filing, None);
        let decision = planned.decision.unwrap();
        assert_eq!(
            (
                decision.session_id,
                decision.status,
                decision.routine,
                decision.input
            ),
            (None, SubjectStatus::Archived, Some(true), None)
        );
        assert_eq!(decision.proposal, empty_subject());
    }

    /// D-85: the copy expires when the earliest excerpt it copies was due
    /// to, so it falls under the same 30-day bound; it carries the end and
    /// the cut-mid-turn fact the decision rested on.
    #[test]
    fn confirmation_input_expires_with_its_excerpts() {
        let mut cut = session("a", Some(1), 1_000, None);
        cut.last_stop_at = None;
        cut.interrupted = Some(Sourced {
            value: 1_500,
            src: Source::Rules,
            at_ms: 1_500,
        });
        let early_answer = ExcerptRecord {
            answer_at_ms: Some(900),
            ..excerpt("a", 1_000)
        };
        let input = confirmation_input(&cut, Some(&early_answer)).unwrap();
        assert_eq!(input.expires_at_ms, 900 + EXCERPT_TTL_MS, "the earliest");
        assert_eq!(
            (input.end_reason.as_deref(), input.cut_mid_turn),
            (Some("interrupted"), true)
        );
        assert!(
            input.expires_at_ms - 900 + COPY_LIFETIME_MS + PASS_INTERVAL_MS <= PRIVACY_BOUND_MS,
            "a copy of the input in a daily copy still leaves within 30 days"
        );
        let only_prompt = ExcerptRecord {
            answer_head: None,
            answer_at_ms: None,
            ..excerpt("a", 1_000)
        };
        assert_eq!(
            confirmation_input(&cut, Some(&only_prompt))
                .unwrap()
                .expires_at_ms,
            1_000 + EXCERPT_TTL_MS
        );
        let cleared = ExcerptRecord {
            prompt: None,
            prompt_at_ms: None,
            answer_head: None,
            answer_at_ms: None,
            ..excerpt("a", 1_000)
        };
        assert_eq!(confirmation_input(&cut, Some(&cleared)), None);

        // Through a confirmation: the anchor's excerpts, a normal exit.
        let subjects = [subject(1, "rollout", SubjectStatus::Active)];
        let sessions = [session("a", Some(1), 1_000, Some(2_000))];
        let excerpts = [excerpt("a", 1_000)];
        let none = ModelInput::default();
        let planned = confirm_item(
            &item("subject:1", default_to_resume(), Verdict::ToResume),
            &context(&subjects, &sessions, &excerpts, &none),
        )
        .unwrap();
        let input = planned.decision.unwrap().input.unwrap();
        assert_eq!(
            (
                input.prompt.as_deref(),
                input.end_reason.as_deref(),
                input.cut_mid_turn,
                input.expires_at_ms
            ),
            (
                Some("prompt of a"),
                Some("prompt_input_exit"),
                false,
                1_000 + EXCERPT_TTL_MS
            )
        );
    }
}
