//! The registry's vocabulary (AD-38, AD-40, AD-82, D-45, D-133): every
//! enum a column stores, each declared with `closed_enum!` so its `ALL` cannot
//! miss a variant and its text form (`code()`, `snake_case`) is the one the
//! `CHECK` constraints list; the records rows are read into; and
//! [`Sourced`], a value with its provenance.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agents::{LifecycleState, LiveStatus, SessionKind};
use crate::untrusted::Untrusted;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a stored fact came from (the PRD glossary's provenance).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Source {
        /// `claude agents --json`.
        Agents,
        /// A hook event line.
        Hook,
        /// Officina's own deterministic rules.
        Rules,
        /// The triage model's answer (FR-10). Reserved: no fact, filing,
        /// status or setting is written with it today, since a model answer
        /// lives in `model_answer` and a confirmation keeps
        /// `ProposalSource::Model`; the schema's `CHECK` lists accept it so
        /// a later use needs no migration (P-14).
        Model,
        /// The author, in the window.
        Author,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a verdict proposal came from (FR-8).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProposalSource {
        /// Claude Code's background `state`.
        State,
        /// Officina's rules, the default included.
        Rules,
        /// The triage model.
        Model,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A verdict the author confirms (FR-9).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Verdict {
        Finished,
        ToResume,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A verdict a proposal may carry (glossary, Proposal), also the enum of
    /// the triage model's answer schema (AD-51).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ModelVerdict {
        Finished,
        ToResume,
        /// Never correct (D-59): the author decides.
        Unsure,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a proposal says what it says (AD-40): never prose.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ReasonCode {
        /// A background session's `state` is `done`.
        StateDone,
        /// A background session's `state` is `failed`.
        StateFailed,
        /// A background session's `state` is `stopped`.
        StateStopped,
        /// The last `UserPromptSubmit` came after the last `Stop`.
        CutMidTurn,
        /// No earlier rule matched: `to_resume`.
        DefaultToResume,
        /// None of the subject's sessions saw a prompt (D-76).
        EmptySubject,
        /// The triage model answered.
        Model,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The kinds of triage row (AD-49, D-76).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RowKind {
        /// A subject whose sessions all ended.
        Subject,
        /// An ended, unfiled, interactive session that saw a prompt.
        UnfiledSession,
        /// A subject none of whose sessions saw a prompt.
        EmptySubject,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A subject's status (PRD glossary).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum SubjectStatus {
        Active,
        ToResume,
        Archived,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The keys of the `setting` table (D-88).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum SettingKey {
        /// The copy a damaged registry was restored from, and when (D-93).
        RestoreNotice,
        /// Whether the triage model is enabled, and why it changed (D-69).
        TriageModel,
        /// Why the triage model is paused, with the `claude` binary's stamp
        /// (D-66).
        TriageModelPause,
        /// Whether auto-archive is on, and why it changed (D-69).
        AutoArchive,
        /// Whether `auto` actions may be approved by their review (AD-88).
        AutoApproval,
        /// Whether `auto` requests are decided by the review or the rule
        /// (D-162).
        AutoMode,
        /// When the author last read the orchestrator's briefing (AD-83).
        BriefingReadAtMs,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a work order came from (AD-82).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum WorkOrderSource {
        /// The author's composer.
        Composer,
        /// A proposal of the orchestrator the author accepted.
        Proposal,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a work order's launch ended (AD-81 step 3).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum LaunchOutcome {
        /// Recorded, its spawn not answered yet.
        Launching,
        /// `claude --bg` exited 0.
        Started,
        /// `claude` refused the folder: the workspace is not trusted.
        UntrustedWorkspace,
        /// Any other non-zero exit, or the deadline.
        Failed,
        /// Its outcome never came back (D-136): the window stopped while it
        /// was launching.
        Unknown,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What an orchestrator proposal file proposes (AD-82, D-575).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProposalKind {
        Recap,
        WorkOrder,
        /// Used from story 5.21, declared here so migration 2's `CHECK`
        /// list holds it (D-575).
        Answer,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where an orchestrator proposal stands (AD-82).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProposalStatus {
        Pending,
        Invalid,
        Accepted,
        Dismissed,
        Expired,
        Superseded,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a Start of the orchestrator ended (AD-82, D-144).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum OrchestratorStartOutcome {
        Spawned,
        Refused,
        Failed,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How an `auto` request was reviewed (AD-82, AD-88).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ReviewMode {
        /// Blind: the author decides, the review is evidence.
        Shadow,
        /// The review's approval runs the action.
        Live,
        /// The rule baseline decides, without a call (D-162).
        Rule,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a review call ended (AD-82, AD-87).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ReviewOutcome {
        /// Escalated before any call.
        NoCall,
        Answered,
        /// An answer that fails validation.
        Invalid,
        Failed,
        Timeout,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The author's own decision on a reviewed request (AD-82).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum AuthorDecision {
        Approved,
        Rejected,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the author rejected an `auto` request (AD-82, AD-86): on the
    /// rejection file, the journal line and the review row.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum AuthorReason {
        /// It should not have run: turns `auto` off for the action.
        ShouldNotRun,
        NotNow,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a triage call was for (AD-52).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum TriagePurpose {
        Proposal,
        DryRun,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a triage call ended (AD-51): an answer, or why there is none.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum TriageOutcome {
        Answer,
        /// Refused before spawning; spends no budget (D-65).
        Refused,
        Failed,
        Timeout,
        /// An answer that fails validation.
        Invalid,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a triage call was refused before anything was started (AD-51,
    /// D-65, D-87): stored with its `refused` row, counted and shown.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum TriageRefusal {
        /// No `claude` binary was found.
        ClaudeNotFound,
        /// `claude` resolves to a `.cmd` or `.bat` shim, which only
        /// `cmd.exe` runs, and `cmd.exe` reparses its arguments.
        CmdShim,
        /// `no_excerpts` is set (environment or the window's floor): there
        /// is nothing the call may be sent.
        NoExcerpts,
        /// The empty folder the call runs in holds something.
        WorkdirNotEmpty,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What opening the registry found (AD-41's table, D-47): in every
    /// state but `ready`, nothing is read.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RegistryState {
        /// The version this build reads: read, and written by the window.
        Ready,
        /// No registry yet; only the window creates one.
        Absent,
        /// Written by an older Officina: the window migrates it.
        Older,
        /// Written by a newer Officina: the window reads it and writes
        /// nothing, a command line does not read it.
        Newer,
        /// Cannot be opened or read, or found damaged while the window runs.
        Unavailable,
        /// On a network folder: never opened.
        Network,
    }
}

/// The registry's state as the board reports it (`Diagnostics.registry`,
/// AD-54, D-47).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RegistryReport {
    pub state: RegistryState,
    /// The error behind the state, when one is.
    pub detail: Option<String>,
    /// The folder to exclude from a sync client, when the registry sits in
    /// one (AD-39).
    pub exclude_from_sync: Option<String>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A fact the registry keeps about a session, one `session_fact` row
    /// each (AD-40): its latest value, where it came from and when. Stored
    /// as text without a `CHECK`, so a new fact needs no migration (D-88).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
    pub enum SessionFact {
        /// `interactive` or `background`, from `claude agents`.
        Kind,
        /// The session's name, from `claude agents`.
        Name,
        /// The working directory, from `claude agents` or a hook line.
        Cwd,
        /// The `claude` process id: `pid` in `claude agents`, `CLAUDE_PID`
        /// in a hook line (E14).
        ClaudePid,
        /// The background session id, the argument of `claude attach`.
        BackgroundId,
        /// The live status `claude agents` last reported.
        Status,
        /// The background lifecycle state `claude agents` last reported.
        State,
        /// When `claude agents` last began to list the session: it was seen
        /// live (AD-47).
        Listed,
        /// The last `SessionStart`: its source and process ([`HookStart`]).
        Started,
        /// The time of the last `UserPromptSubmit`.
        LastPromptAt,
        /// The time of the last `Stop`.
        LastStopAt,
        /// The hook's end: the last `SessionEnd`, its reason and process
        /// ([`HookEnd`]).
        Ended,
        /// The session left `claude agents` without a `SessionEnd`, as
        /// Officina's rules inferred it (D-78): a fact apart from the hook's
        /// end.
        Interrupted,
        /// The session that replaced this one in the same process after
        /// `/clear` or `/resume` (D-52).
        Successor,
        /// The author's own verdict on the ended session, given from the
        /// board ([`Settlement`], story 4.15): an author action, never
        /// inferred, and it covers only what happened before it.
        Settled,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the author settled about an ended session from the board
    /// (story 4.15, D-536): the value of [`SessionFact::Settled`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Settlement {
        /// The work is done: the card leaves To resume for Done.
        Done,
        /// It was never real work (a test, a mistake): the card leaves the
        /// board and the scene, and the archived list can restore it.
        Discarded,
    }
}

/// A `session_fact` value as SQLite stores it (`value ANY`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactValue {
    Integer(i64),
    Text(String),
}

/// One `session_fact` row. `fact` is the stored text, so a row naming a
/// fact this build does not know is still read, and counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactRow {
    pub session_id: String,
    pub fact: String,
    pub value: FactValue,
    pub src: Source,
    pub at_ms: i64,
}

/// The value of the [`SessionFact::Started`] fact: what the last
/// `SessionStart` said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookStart {
    /// `SessionStart.source` as Claude Code sent it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The line's `claude_pid`, absent from a line written by a v0.1 hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_pid: Option<u32>,
}

/// The value of the [`SessionFact::Ended`] fact: what the last `SessionEnd`
/// said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEnd {
    /// `SessionEnd.reason` as Claude Code sent it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The line's `claude_pid`, absent from a line written by a v0.1 hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_pid: Option<u32>,
}

/// How a session ended (AD-47, D-78), when it is ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ending {
    /// An interactive session's `SessionEnd`, with its reason.
    SessionEnd { reason: Option<String> },
    /// An interactive session that left `claude agents` without a
    /// `SessionEnd`.
    Interrupted,
    /// A background session whose state is `done`, `failed` or `stopped`.
    State { state: LifecycleState },
}

/// A session as the registry holds it (FR-3, AD-40): its row, and each
/// fact pivoted into its field with its source and time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub project: String,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    /// The subject the session is filed under (`None` inside: unfiled
    /// since that time), with who filed it and when; `None` when it was
    /// never filed.
    pub subject: Option<Sourced<Option<i64>>>,
    pub kind: Option<Sourced<SessionKind>>,
    pub name: Option<Sourced<String>>,
    pub cwd: Option<Sourced<String>>,
    pub claude_pid: Option<Sourced<u32>>,
    pub background_id: Option<Sourced<String>>,
    pub status: Option<Sourced<LiveStatus>>,
    pub state: Option<Sourced<LifecycleState>>,
    /// The fact's value is its time, like `at_ms`.
    pub listed: Option<Sourced<i64>>,
    pub started: Option<Sourced<HookStart>>,
    pub last_prompt_at: Option<Sourced<i64>>,
    pub last_stop_at: Option<Sourced<i64>>,
    pub ended: Option<Sourced<HookEnd>>,
    /// The fact's value is its time, like `at_ms`.
    pub interrupted: Option<Sourced<i64>>,
    pub successor: Option<Sourced<String>>,
    /// The author's verdict from the board, and when it was given.
    pub settled: Option<Sourced<Settlement>>,
    /// Rows this build could not read: a fact name it does not know (a
    /// newer Officina wrote it), or a value of another type. Counted so a
    /// downgrade shows them instead of dropping them.
    pub unknown_facts: u32,
}

/// The documented `SessionEnd` reason of a session that never ran in a
/// terminal the author drove (E2: `other` for `-p`).
pub const NON_INTERACTIVE_END_REASON: &str = "other";

impl SessionRecord {
    /// The latest sign of life (D-77): a `UserPromptSubmit`, or the time
    /// `claude agents` began to list the session again.
    pub fn sign_of_life_ms(&self) -> Option<i64> {
        let prompt = self.last_prompt_at.as_ref().map(|p| p.value);
        let listed = self.listed.as_ref().map(|l| l.at_ms);
        prompt.max(listed)
    }

    /// True when `claude agents` reports the session as a background one.
    pub fn is_background(&self) -> bool {
        self.kind
            .as_ref()
            .is_some_and(|k| k.value == SessionKind::Background)
    }

    /// The hook's end, unless a later sign of life reopened it (D-77).
    fn hook_end(&self) -> Option<&Sourced<HookEnd>> {
        let life = self.sign_of_life_ms();
        self.ended
            .as_ref()
            .filter(|end| life.is_none_or(|life| life <= end.at_ms))
    }

    /// How the session ended, when it did (AD-47, D-78): a background
    /// session by its state; any other by its `SessionEnd`, which always
    /// beats `interrupted` whatever the times, else by `interrupted`; in
    /// both cases only until a later sign of life (D-77).
    pub fn ending(&self) -> Option<Sourced<Ending>> {
        if self.is_background() {
            return self.state.as_ref().and_then(|state| {
                matches!(
                    state.value,
                    LifecycleState::Done | LifecycleState::Failed | LifecycleState::Stopped
                )
                .then(|| Sourced {
                    value: Ending::State { state: state.value },
                    src: state.src,
                    at_ms: state.at_ms,
                })
            });
        }
        if let Some(end) = self.hook_end() {
            return Some(Sourced {
                value: Ending::SessionEnd {
                    reason: end.value.reason.clone(),
                },
                src: end.src,
                at_ms: end.at_ms,
            });
        }
        let life = self.sign_of_life_ms();
        self.interrupted
            .as_ref()
            .filter(|cut| life.is_none_or(|life| life <= cut.at_ms))
            .map(|cut| Sourced {
                value: Ending::Interrupted,
                src: cut.src,
                at_ms: cut.at_ms,
            })
    }

    /// True when the session is ended (AD-47).
    pub fn is_ended(&self) -> bool {
        self.ending().is_some()
    }

    /// The session that replaced this one in its process (D-52), unless a
    /// later sign of life shows this one continued since: a conversation
    /// resumed later keeps its id (D-75), and is itself again (D-77).
    pub fn current_successor(&self) -> Option<&Sourced<String>> {
        let life = self.sign_of_life_ms();
        self.successor
            .as_ref()
            .filter(|next| life.is_none_or(|life| life <= next.at_ms))
    }

    /// True when another session replaced this one in its process (AD-47):
    /// it gives no triage row of its own.
    pub fn is_superseded(&self) -> bool {
        self.current_successor().is_some()
    }

    /// True when a `UserPromptSubmit` was seen.
    pub fn saw_prompt(&self) -> bool {
        self.last_prompt_at.is_some()
    }

    /// True for a run that was never seen in `claude agents` and ended with
    /// [`NON_INTERACTIVE_END_REASON`] (FR-3 as amended, AD-47): recorded,
    /// never filed by rule or triaged. A session seen live never is.
    pub fn is_non_interactive(&self) -> bool {
        self.listed.is_none()
            && matches!(
                self.ending().map(|e| e.value),
                Some(Ending::SessionEnd { reason: Some(reason) }) if reason == NON_INTERACTIVE_END_REASON
            )
    }

    /// The subject the session counts under. Rules file every new session
    /// at first sight (AD-48), before anyone can tell a non-interactive run
    /// (AD-47); once the session reads non-interactive, a filing by rule no
    /// longer counts, so rules never file one, while the author still may
    /// (D-56).
    pub fn filed_subject(&self) -> Option<i64> {
        let filing = self.subject.as_ref()?;
        let by_rule_on_a_run = filing.src == Source::Rules && self.is_non_interactive();
        filing.value.filter(|_| !by_rule_on_a_run)
    }

    /// True once the session is known to be one the author drove: listed by
    /// `claude agents`, or ended otherwise than a non-interactive run does.
    /// Only such a session's excerpts replace its subject's older ones
    /// (D-50), so a `claude -p` run filed at first sight never clears them.
    pub fn is_established(&self) -> bool {
        self.listed.is_some() || (self.is_ended() && !self.is_non_interactive())
    }
}

/// One `subject` row (AD-40, AD-48).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectRecord {
    pub id: i64,
    /// Which new sessions may join it at first sight (AD-48).
    pub project: String,
    pub name: String,
    pub status: Sourced<SubjectStatus>,
    pub created_at_ms: i64,
    /// "Routine, could have run alone", set when archived (FR-13, P-10).
    pub routine: bool,
    /// The subject a merge moved its sessions to (D-55): archived, and left
    /// out of the archived list.
    pub merged_into: Option<i64>,
}

/// One `confirmation` row, as far as deciding what still needs a verdict
/// reads it (AD-49, D-58).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationRecord {
    pub subject_id: i64,
    /// The session the row was anchored on; `None` for an empty subject
    /// (D-76).
    pub session_id: Option<String>,
    pub confirmed_at_ms: i64,
    /// Set when the subject was reopened after it (D-58): the confirmation
    /// no longer answers anything.
    pub reopened_at_ms: Option<i64>,
}

/// A subject as a board card names it (AD-54).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SubjectRef {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: i64,
    pub name: String,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the registry refused an author action (D-54 to D-58, D-95):
    /// nothing was written. Distinct from the writer's error, which says
    /// the registry itself failed (D-57).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum AuthorRefusal {
        /// The name is empty once trimmed.
        NameEmpty,
        /// The name is longer than the bound, once trimmed.
        NameTooLong,
        /// The name holds a control character or a hidden formatting one.
        NameHasForbiddenCharacters,
        /// A subject of the project that is not archived already has the
        /// name, ignoring case.
        NameTaken,
        /// No subject has this id.
        UnknownSubject,
        /// The registry holds no session with this id.
        UnknownSession,
        /// No session or subject of the registry has this project.
        UnknownProject,
        /// The action names no session.
        NoSession,
        /// A subject is created from sessions of more than one project.
        SessionsOfSeveralProjects,
        /// The subject is archived: reopen it first (D-54).
        Archived,
        /// Only an archived subject is reopened (D-58).
        NotArchived,
        /// The subject was merged into another: its sessions are there
        /// (D-55).
        Merged,
        /// A subject is merged into itself.
        SameSubject,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the registry refused one item of a triage confirmation (AD-49,
    /// D-63): nothing was written for it, and the row shows why.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ConfirmRefusal {
        /// No row with this key needs a verdict now: it was confirmed, a
        /// session of it came alive again, or it was filed elsewhere.
        RowGone,
        /// The proposal computed now differs from the one the window showed:
        /// the author decides on what was shown, never on what changed since.
        ProposalChanged,
        /// An unfiled row came without the subject to file it under.
        NoSubject,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why an unfiled row's session was filed but its verdict not applied
    /// (D-60): the subject's own row now decides.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum NotNeeded {
        /// Another session of the subject saw a later prompt.
        NotLatest,
        /// A session of the subject is live.
        LiveSession,
    }
}

/// One `session` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRow {
    pub session_id: String,
    /// Recorded at first sight, never rewritten by a floor change (D-53).
    pub project: String,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    /// `subject_id`, `subject_src` and `subject_at_ms`: a column, not a
    /// fact, since queries filter on it (AD-40).
    pub subject: Option<Sourced<Option<i64>>>,
}

/// One `session_excerpt` row (AD-40, FR-4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcerptRecord {
    pub session_id: String,
    pub prompt: Option<String>,
    pub prompt_at_ms: Option<i64>,
    pub answer_head: Option<String>,
    pub answer_tail: Option<String>,
    pub answer_at_ms: Option<i64>,
}

/// A value with where it came from and when (AD-40).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Sourced<T> {
    pub value: T,
    pub src: Source,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
}

/// One `event_file` row: how far an event file was ingested (AD-43).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventFileRecord {
    /// `YYYY-MM-DD.jsonl`.
    pub name: String,
    /// Bytes read, always up to a line end.
    pub ingested_bytes: u64,
    /// When the file was found complete: dated before today, fully read,
    /// past the grace.
    pub complete_at_ms: Option<i64>,
    /// Lines that could not be read, since the last time the file was read
    /// from its start.
    pub corrupt_lines: u32,
    /// How many times the file was found shorter than its offset, and read
    /// again from its start (D-84).
    pub rewrites: u32,
}

/// The value of the [`SettingKey::RestoreNotice`] setting (D-93): which
/// copy replaced a damaged registry, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RestoreNotice {
    /// The copy's file name under `registry/backup/`.
    pub copy: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub restored_at_ms: i64,
}

/// One `work_order` row read back (AD-82, D-130), with the record of the
/// session it bound to, when the registry holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkOrderRecord {
    /// The id of the card it consumed (D-136).
    pub id: String,
    pub seq: u32,
    pub template: String,
    pub project: String,
    /// The project folder at launch, which binding compares (D-133).
    pub cwd: String,
    /// Each parameter's value, by name.
    pub values: BTreeMap<String, String>,
    /// The argument vector after the binary, as it ran.
    pub argv: Vec<String>,
    pub prompt_sha256: String,
    pub catalogue_hash: String,
    /// `<template>-<seq>`, unique in the registry.
    pub name: String,
    /// The subject chosen, or created at `started` for a new one.
    pub subject_id: Option<i64>,
    /// The new subject's name the author chose, when it was one.
    pub new_subject_name: Option<String>,
    pub source: WorkOrderSource,
    pub proposal_id: Option<String>,
    pub launched_at_ms: i64,
    pub outcome: LaunchOutcome,
    pub exit_code: Option<i64>,
    /// The first characters of `claude`'s stderr on a failure.
    pub error: Option<Untrusted<String>>,
    /// What `claude --bg` printed, display only.
    pub printed: Option<Untrusted<String>>,
    /// The binding (AD-81 step 4): the background id, the session id and
    /// when.
    pub bg_id: Option<String>,
    pub session_id: Option<String>,
    pub bound_at_ms: Option<i64>,
    /// The bound session as the registry holds it.
    pub session: Option<SessionRecord>,
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::*;

    /// Runs `check` once per enum of this file, giving it the enum's `ALL`,
    /// `code` and `from_code`; returns how many enums it ran on.
    fn each_enum(check: &dyn Fn(&dyn EnumUnderTest)) -> usize {
        let enums: [&dyn EnumUnderTest; 26] = [
            &Closed(
                WorkOrderSource::ALL,
                WorkOrderSource::code,
                WorkOrderSource::from_code,
            ),
            &Closed(
                LaunchOutcome::ALL,
                LaunchOutcome::code,
                LaunchOutcome::from_code,
            ),
            &Closed(
                ProposalKind::ALL,
                ProposalKind::code,
                ProposalKind::from_code,
            ),
            &Closed(
                ProposalStatus::ALL,
                ProposalStatus::code,
                ProposalStatus::from_code,
            ),
            &Closed(
                OrchestratorStartOutcome::ALL,
                OrchestratorStartOutcome::code,
                OrchestratorStartOutcome::from_code,
            ),
            &Closed(ReviewMode::ALL, ReviewMode::code, ReviewMode::from_code),
            &Closed(
                ReviewOutcome::ALL,
                ReviewOutcome::code,
                ReviewOutcome::from_code,
            ),
            &Closed(
                AuthorDecision::ALL,
                AuthorDecision::code,
                AuthorDecision::from_code,
            ),
            &Closed(
                AuthorReason::ALL,
                AuthorReason::code,
                AuthorReason::from_code,
            ),
            &Closed(
                TriageRefusal::ALL,
                TriageRefusal::code,
                TriageRefusal::from_code,
            ),
            &Closed(
                ConfirmRefusal::ALL,
                ConfirmRefusal::code,
                ConfirmRefusal::from_code,
            ),
            &Closed(NotNeeded::ALL, NotNeeded::code, NotNeeded::from_code),
            &Closed(
                RegistryState::ALL,
                RegistryState::code,
                RegistryState::from_code,
            ),
            &Closed(
                AuthorRefusal::ALL,
                AuthorRefusal::code,
                AuthorRefusal::from_code,
            ),
            &Closed(SessionFact::ALL, SessionFact::code, SessionFact::from_code),
            &Closed(Settlement::ALL, Settlement::code, Settlement::from_code),
            &Closed(Source::ALL, Source::code, Source::from_code),
            &Closed(
                ProposalSource::ALL,
                ProposalSource::code,
                ProposalSource::from_code,
            ),
            &Closed(Verdict::ALL, Verdict::code, Verdict::from_code),
            &Closed(
                ModelVerdict::ALL,
                ModelVerdict::code,
                ModelVerdict::from_code,
            ),
            &Closed(ReasonCode::ALL, ReasonCode::code, ReasonCode::from_code),
            &Closed(RowKind::ALL, RowKind::code, RowKind::from_code),
            &Closed(
                SubjectStatus::ALL,
                SubjectStatus::code,
                SubjectStatus::from_code,
            ),
            &Closed(SettingKey::ALL, SettingKey::code, SettingKey::from_code),
            &Closed(
                TriagePurpose::ALL,
                TriagePurpose::code,
                TriagePurpose::from_code,
            ),
            &Closed(
                TriageOutcome::ALL,
                TriageOutcome::code,
                TriageOutcome::from_code,
            ),
        ];
        for each in enums {
            check(each);
        }
        enums.len()
    }

    trait EnumUnderTest {
        fn round_trips(&self);
        fn refuses(&self, text: &str);
        fn codes(&self) -> Vec<&'static str>;
    }

    struct Closed<T: 'static>(&'static [T], fn(T) -> &'static str, fn(&str) -> Option<T>);

    impl<T> EnumUnderTest for Closed<T>
    where
        T: Copy + PartialEq + Debug + Serialize + DeserializeOwned + 'static,
    {
        fn round_trips(&self) {
            let Closed(all, code, from_code) = self;
            for &variant in *all {
                let text = code(variant);
                assert_eq!(from_code(text), Some(variant), "{variant:?}");
                assert_eq!(
                    serde_json::to_value(variant).unwrap().as_str(),
                    Some(text),
                    "{variant:?}"
                );
                let back: T = serde_json::from_value(text.into()).unwrap();
                assert_eq!(back, variant);
                assert!(
                    !text.is_empty() && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                    "{text} is not snake_case"
                );
            }
        }

        fn refuses(&self, text: &str) {
            let Closed(_, _, from_code) = self;
            assert_eq!(from_code(text), None, "{text:?}");
            assert!(
                serde_json::from_value::<T>(text.into()).is_err(),
                "{text:?}"
            );
        }

        fn codes(&self) -> Vec<&'static str> {
            let Closed(all, code, _) = self;
            all.iter().map(|v| code(*v)).collect()
        }
    }

    /// The count of enums checked must match the `closed_enum!` blocks of
    /// this file, so a new enum fails here until it is listed above.
    fn declared_enums() -> usize {
        include_str!("types.rs")
            .lines()
            .filter(|line| line.trim() == "closed_enum! {")
            .count()
    }

    #[test]
    fn every_variant_round_trips_its_text() {
        let checked = each_enum(&|e| e.round_trips());
        assert_eq!(checked, declared_enums());
    }

    #[test]
    fn unknown_text_is_refused() {
        each_enum(&|e| {
            e.refuses("");
            e.refuses("not_a_variant");
            for code in e.codes() {
                e.refuses(&code.to_ascii_uppercase());
                e.refuses(&format!(" {code}"));
                if code.contains('_') {
                    e.refuses(&code.replace('_', "-"));
                    e.refuses(&code.replace('_', ""));
                }
            }
        });
    }
}
