//! The meeting on Receive (AD-84, D-138, D-168, D-169): the collector
//! answers what it holds (the card, the folder `claude agents` reported
//! for its session, its pending approvals, where the files are), then the
//! window command's blocking thread reads the event lines, the journal back
//! to the session's start, `git` and `claude logs`, and builds the view in
//! core. Neither the journal, `git` nor `claude logs` runs on the collector
//! thread (AD-3).

/// What marks a line of `claude logs` as an error, as the meeting finds it;
/// the fake claude builds its logs fixture from it (D-129).
pub use officina_core::meeting::LOG_ERROR_MARKERS;

#[cfg(feature = "window-authority")]
pub use window::*;

#[cfg(feature = "window-authority")]
mod window {
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;
    use std::time::{Duration, Instant};

    use officina_core::agents::LiveStatus;
    use officina_core::meeting::{
        Answer, AnswerRefused, AnswerResult, CONFIRM_WINDOW_MS, Delivery,
    };
    use officina_core::relay::journal::{AnswerOutcome, JournalEntry, MeetingAnswer};
    use officina_core::spawn::{ClaudeVector, SessionId};

    use officina_core::board::{Board, group_events};
    use officina_core::ipc::{IpcError, IpcErrorCode};
    use officina_core::meeting::{
        GitRead, LogsRead, MeetingCard, MeetingInput, MeetingTarget, MeetingView, agents_folder_of,
        build, delivery, latest_prompted, since_iso,
    };
    use officina_core::relay::approvals::ApprovalView;
    use officina_core::spawn::BackgroundId;

    use crate::collector::Collector;
    use crate::files::{now_ms, read_recent_events};
    use crate::registry::window::WindowRegistry;
    use crate::relay::home::RelayHome;
    use crate::spawn::{BaseEnv, ResolvedClaude};
    use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, AuthorReply, CollectorMsg, ask};

    /// The prefix of a key naming a subject to resume (D-138): its meeting
    /// is its latest session that saw a prompt.
    pub const SUBJECT_KEY_PREFIX: &str = "subject:";

    /// The prefix of a key naming a session by its id.
    pub const SESSION_KEY_PREFIX: &str = "sid:";

    /// What the collector answers to a meeting's request: everything the
    /// blocking thread needs, none of it read there.
    #[derive(Debug, Clone, PartialEq)]
    pub struct MeetingGrant {
        pub card: MeetingCard,
        /// The folder `claude agents` reported for the session (D-168).
        pub agents_folder: Option<String>,
        pub approvals: Vec<ApprovalView>,
        pub claude: Option<ResolvedClaude>,
        pub config_dir: Option<PathBuf>,
        pub base_env: BaseEnv,
        pub relay: RelayHome,
        pub events_dir: PathBuf,
        /// The `claude` version the collector last read (D-139).
        pub claude_version: Option<String>,
    }

    fn unknown(key: &str) -> IpcError {
        IpcError::new(
            IpcErrorCode::Refused,
            format!("{key} is no longer on the board, and the registry does not hold it"),
        )
    }

    /// The collector's half of `open_meeting(key)` (D-138): the card of
    /// `key` on `board`; for a `sid:` key absent from it, the registry's
    /// record of that session; for a `subject:` key, the subject's latest
    /// session that saw a prompt, as `sid:`. The folder comes from `claude
    /// agents` only: the listed record of the card's session, else the
    /// registry's agents-sourced `cwd` (D-168).
    pub fn grant(
        collector: &Collector,
        registry: &mut WindowRegistry,
        board: Option<&Board>,
        key: &str,
    ) -> Result<MeetingGrant, IpcError> {
        let mut records = None;
        let mut key = key.to_owned();
        if let Some(subject) = key.strip_prefix(SUBJECT_KEY_PREFIX) {
            let subject: i64 = subject.parse().map_err(|_| unknown(&key))?;
            let (found, excerpts) = registry.meeting_records()?;
            let session = latest_prompted(&found, subject)
                .ok_or_else(|| unknown(&key))?
                .to_owned();
            key = format!("{SESSION_KEY_PREFIX}{session}");
            records = Some((found, excerpts));
        }
        let session = key.strip_prefix(SESSION_KEY_PREFIX);
        let on_board = board.and_then(|board| {
            board.columns.all().find(|card| {
                card.key == key || session.is_some() && card.session_id.as_deref() == session
            })
        });
        let listed = |session_id: Option<&str>, card_key: &str| {
            collector.agents().and_then(|agents| {
                agents.iter().find(|record| {
                    officina_core::board::card_key(record).as_deref() == Some(card_key)
                        || session_id.is_some() && record.session_id.as_deref() == session_id
                })
            })
        };
        let (card, mut agents_folder) = match on_board {
            Some(card) => (
                MeetingCard::from(card),
                listed(card.session_id.as_deref(), &card.key).and_then(|r| r.cwd.clone()),
            ),
            None => {
                let session = session.ok_or_else(|| unknown(&key))?;
                let (found, excerpts) = match records.take() {
                    Some(read) => read,
                    None => registry.meeting_records()?,
                };
                let record = found
                    .iter()
                    .find(|r| r.session_id == session)
                    .ok_or_else(|| unknown(&key))?;
                let excerpt = excerpts.iter().find(|e| e.session_id == session);
                let card = MeetingCard::from_record(record, excerpt, collector.no_excerpts());
                let folder = agents_folder_of(record).map(str::to_owned);
                records = Some((found, excerpts));
                (card, folder)
            }
        };
        if agents_folder.is_none()
            && let Some(session) = card.session_id.as_deref()
        {
            let (found, _) = match records.take() {
                Some(read) => read,
                None => registry.meeting_records().unwrap_or_default(),
            };
            agents_folder = found
                .iter()
                .find(|r| r.session_id == session)
                .and_then(agents_folder_of)
                .map(str::to_owned);
        }
        let paths = collector.paths();
        Ok(MeetingGrant {
            approvals: board.map(|b| b.approvals.clone()).unwrap_or_default(),
            agents_folder,
            claude: collector.resolved_claude().cloned(),
            config_dir: paths.claude_config_override.clone(),
            base_env: collector.base_env().clone(),
            relay: collector.relay().clone(),
            events_dir: paths.events_dir(),
            claude_version: collector.claude_version().map(str::to_owned),
            card,
        })
    }

    /// The blocking thread's half (D-138): the latest `Notification`
    /// message from the event lines, the journal back to the session's
    /// start (D-169), `git` in the agents-reported folder (D-154, D-168)
    /// and `claude logs` for a background session, the two at once; then
    /// the view.
    pub fn gather(grant: &MeetingGrant, now_ms: i64) -> MeetingView {
        let card = &grant.card;
        let session = card.session_id.as_deref();
        let (events, _) = read_recent_events(&grant.events_dir, now_ms);
        let notification = session.and_then(|sid| {
            group_events(&events)
                .remove(sid)
                .and_then(|grouped| grouped.last_notification_message)
        });
        let days = officina_core::meeting::journal_days(card.started_at_ms, now_ms);
        let (journal, _) = crate::relay::journal::read_days(&grant.relay, now_ms, days);
        let started = Instant::now();
        let (git, logs) = std::thread::scope(|scope| {
            let git = scope.spawn(|| read_git(grant, started));
            let logs = read_logs(grant);
            (
                git.join()
                    .unwrap_or_else(|_| GitRead::Unavailable("git's reader stopped".to_owned())),
                logs,
            )
        });
        let target = MeetingTarget {
            card,
            agents_folder: grant.agents_folder.as_deref(),
            claude_version: grant.claude_version.as_deref(),
        };
        build(&MeetingInput {
            card,
            notification: notification.as_deref(),
            journal: &journal,
            approvals: &grant.approvals,
            git,
            logs,
            delivery: delivery(&target),
            now_ms,
        })
    }

    /// `git` in the folder `claude agents` reported, never another
    /// (D-168): without one, no git runs.
    fn read_git(grant: &MeetingGrant, started: Instant) -> GitRead {
        let Some(folder) = grant.agents_folder.as_deref() else {
            return GitRead::FolderUnknown;
        };
        let Some(git) = crate::relay::tree::resolve_git() else {
            return GitRead::Unavailable(
                "git is not installed where Officina looks for it".to_owned(),
            );
        };
        let since = since_iso(grant.card.started_at_ms.unwrap_or(0));
        crate::git::meeting_changes(
            &git,
            std::path::Path::new(folder),
            &since,
            &crate::relay::tree::git_env(),
            started,
        )
    }

    /// `claude logs` of a background session (AD-84): `None` for any other
    /// session, which shows no logs section.
    fn read_logs(grant: &MeetingGrant) -> Option<LogsRead> {
        let card = &grant.card;
        if card.kind != Some(officina_core::board::SessionKindView::Background) {
            return None;
        }
        let id = card
            .background_id
            .as_deref()
            .and_then(BackgroundId::parse)?;
        let Some(claude) = &grant.claude else {
            return Some(LogsRead::Failed(
                crate::spawn::SpawnError::NotFound.to_string(),
            ));
        };
        Some(
            match crate::spawn::logs(claude, &id, grant.config_dir.as_deref(), &grant.base_env) {
                Ok(stdout) => LogsRead::Output(stdout),
                Err(error) => LogsRead::Failed(error.to_string()),
            },
        )
    }

    /// `open_meeting(key)` (AD-84, D-138): the collector's answer, then the
    /// reads and the build on this, the command's blocking, thread.
    pub fn open_meeting(
        collector: &Sender<CollectorMsg>,
        key: &str,
    ) -> Result<MeetingView, IpcError> {
        let grant = ask(
            collector,
            AuthorCommand::OpenMeeting(key.to_owned()),
            AUTHOR_WAIT,
        )?
        .into_meeting()?;
        Ok(gather(&grant, now_ms()))
    }

    /// What the collector answers to an answer (AD-85): settled at once
    /// (refused, copied, already journaled), or a vector for the command's
    /// thread to start.
    #[derive(Debug, Clone, PartialEq)]
    pub enum AnswerGrant {
        Settled(AnswerResult),
        Start(Box<AnswerTicket>),
    }

    /// A granted answer's start: the vector, what it runs with, and its
    /// journal line, whose outcome the start decides.
    #[derive(Debug, Clone, PartialEq)]
    pub struct AnswerTicket {
        pub vector: ClaudeVector,
        pub claude: ResolvedClaude,
        pub base_env: BaseEnv,
        pub line: MeetingAnswer,
    }

    /// How a granted answer's start went, back to the collector: its line
    /// and, when the console could not start, why.
    #[derive(Debug, Clone, PartialEq)]
    pub struct AnswerReport {
        pub line: MeetingAnswer,
        pub failed: Option<String>,
    }

    /// A resumed answer the collector watches for `busy` (AD-85, D-160):
    /// its session, its line, until when, and where the window waits.
    #[derive(Debug)]
    pub struct PendingConfirmation {
        pub session_id: String,
        pub line: MeetingAnswer,
        pub until: Instant,
        pub reply: Sender<AuthorReply>,
    }

    fn journal_answer(collector: &Collector, line: &MeetingAnswer, now_ms: i64) {
        let _ = crate::relay::journal::record(
            collector.relay(),
            &[JournalEntry::MeetingAnswer(line.clone())],
            now_ms,
        );
    }

    /// The collector's half of `send_answer` (AD-85, D-139): the delivery
    /// computed again from the latest board, the claude version and the
    /// agents-reported folder; refused `state_changed` when it differs from
    /// the one the panel showed. Every refusal and every copy is journaled
    /// here, never the text.
    pub fn grant_answer(
        collector: &Collector,
        registry: &mut WindowRegistry,
        board: Option<&Board>,
        key: &str,
        expected: Delivery,
        text: &str,
        now_ms: i64,
    ) -> Result<AnswerGrant, IpcError> {
        let meeting = match grant(collector, registry, board, key) {
            Ok(meeting) => meeting,
            Err(_) => {
                return Ok(AnswerGrant::Settled(AnswerResult::Refused(
                    AnswerRefused::UnknownSession,
                )));
            }
        };
        let delivery = delivery(&MeetingTarget {
            card: &meeting.card,
            agents_folder: meeting.agents_folder.as_deref(),
            claude_version: collector.claude_version(),
        });
        let mut line = MeetingAnswer {
            session_id: meeting.card.session_id.clone(),
            delivery,
            answer_sha256: officina_core::sha256_hex(text.as_bytes()),
            outcome: AnswerOutcome::Refused,
        };
        let settle = |line: &MeetingAnswer, result: AnswerResult| {
            journal_answer(collector, line, now_ms);
            Ok(AnswerGrant::Settled(result))
        };
        let refused = |reason| AnswerResult::Refused(reason);
        if delivery != expected {
            return settle(&line, refused(AnswerRefused::StateChanged));
        }
        let answer = match Answer::parse(text) {
            Ok(answer) => answer,
            Err(refusal) => return settle(&line, refused(AnswerRefused::Answer(refusal))),
        };
        let config = meeting
            .config_dir
            .as_deref()
            .map(|dir| dir.to_string_lossy().into_owned());
        let vector = match delivery {
            Delivery::FocusOnly => return settle(&line, refused(AnswerRefused::NoComposer)),
            Delivery::CopyOnly => {
                line.outcome = AnswerOutcome::Copied;
                return settle(&line, AnswerResult::Copied);
            }
            Delivery::CopyAndAttach => {
                match meeting
                    .card
                    .background_id
                    .as_deref()
                    .and_then(BackgroundId::parse)
                {
                    Some(id) => officina_core::spawn::attach(&id, config.as_deref()),
                    // Nothing to attach to: the text is copied all the same.
                    None => {
                        line.outcome = AnswerOutcome::Copied;
                        return settle(&line, AnswerResult::Copied);
                    }
                }
            }
            Delivery::ResumeLive | Delivery::ResumeEnded => {
                let session = meeting
                    .card
                    .session_id
                    .as_deref()
                    .and_then(SessionId::parse);
                let (Some(session), Some(folder)) = (session, meeting.agents_folder.as_deref())
                else {
                    return settle(&line, refused(AnswerRefused::StateChanged));
                };
                officina_core::spawn::answer(&answer, &session, folder, config.as_deref())
            }
        };
        let Some(claude) = meeting.claude else {
            line.outcome = AnswerOutcome::SpawnFailed;
            return settle(
                &line,
                AnswerResult::SpawnFailed(crate::spawn::SpawnError::NotFound.to_string()),
            );
        };
        line.outcome = AnswerOutcome::Spawned;
        Ok(AnswerGrant::Start(Box::new(AnswerTicket {
            vector,
            claude,
            base_env: meeting.base_env,
            line,
        })))
    }

    /// The collector's record of a granted answer's start (AD-85): its
    /// line, then, for a resume that started, a watch for `busy` within
    /// `window`, answered once it settles; any other answer is settled now.
    pub fn record_answer(
        collector: &Collector,
        report: &AnswerReport,
        reply: Sender<AuthorReply>,
        window: Duration,
        now_ms: i64,
    ) -> Option<PendingConfirmation> {
        let mut line = report.line.clone();
        if report.failed.is_some() {
            line.outcome = AnswerOutcome::SpawnFailed;
        }
        journal_answer(collector, &line, now_ms);
        let result = match (&report.failed, line.delivery) {
            (Some(error), _) => AnswerResult::SpawnFailed(error.clone()),
            (None, Delivery::ResumeLive | Delivery::ResumeEnded) => {
                if let Some(session_id) = line.session_id.clone() {
                    return Some(PendingConfirmation {
                        session_id,
                        line,
                        until: Instant::now() + window,
                        reply,
                    });
                }
                AnswerResult::NotConfirmed
            }
            (None, _) => AnswerResult::Attached,
        };
        let _ = reply.send(Ok(AuthorAnswer::Answered(result)));
        None
    }

    /// Settles the watches whose session `claude agents` now lists `busy`,
    /// confirmed, and those past their window, not confirmed: the second
    /// line of each, then the window's answer (AD-85, D-139).
    pub fn settle_confirmations(
        collector: &Collector,
        pending: &mut Vec<PendingConfirmation>,
        now_ms: i64,
    ) {
        let busy = |session: &str| {
            collector.agents().is_some_and(|agents| {
                agents.iter().any(|record| {
                    record.session_id.as_deref() == Some(session)
                        && record.status == Some(LiveStatus::Busy)
                })
            })
        };
        let now = Instant::now();
        let mut kept = Vec::new();
        for watch in pending.drain(..) {
            let outcome = if busy(&watch.session_id) {
                AnswerOutcome::Confirmed
            } else if now >= watch.until {
                AnswerOutcome::NotConfirmed
            } else {
                kept.push(watch);
                continue;
            };
            let line = MeetingAnswer {
                outcome,
                ..watch.line
            };
            journal_answer(collector, &line, now_ms);
            let result = if outcome == AnswerOutcome::Confirmed {
                AnswerResult::Confirmed
            } else {
                AnswerResult::NotConfirmed
            };
            let _ = watch.reply.send(Ok(AuthorAnswer::Answered(result)));
        }
        *pending = kept;
    }

    /// `send_answer(key, expected, text)` (AD-85): the collector's grant,
    /// then the start on this, the command's blocking, thread, then its
    /// record, which for a resume answers once the session confirmed it or
    /// the window passed.
    pub fn send_answer(
        collector: &Sender<CollectorMsg>,
        key: &str,
        expected: Delivery,
        text: &str,
    ) -> Result<AnswerResult, IpcError> {
        let command = AuthorCommand::SendAnswer {
            key: key.to_owned(),
            expected,
            text: text.to_owned(),
        };
        let ticket = match ask(collector, command, AUTHOR_WAIT)?.into_answer_grant()? {
            AnswerGrant::Settled(result) => return Ok(result),
            AnswerGrant::Start(ticket) => ticket,
        };
        let failed = crate::spawn::execute(&ticket.claude, &ticket.vector, &ticket.base_env)
            .err()
            .map(|error| error.to_string());
        let report = AnswerReport {
            line: ticket.line,
            failed,
        };
        let wait = Duration::from_millis(CONFIRM_WINDOW_MS) + AUTHOR_WAIT;
        ask(
            collector,
            AuthorCommand::AnswerOutcome(Box::new(report)),
            wait,
        )?
        .into_answered()
    }
}
