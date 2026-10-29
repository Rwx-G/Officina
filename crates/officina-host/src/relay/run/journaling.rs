//! The journal lines of one run (AD-32): `run_started` before anything
//! starts, and the one ending every run goes through, whatever ran and
//! however it ended.

use std::collections::BTreeMap;
use std::io::Write;
use std::time::Instant;

use officina_core::relay::journal::{
    JournalEntry, RunFinished, RunOutcome, RunStarted, StreamSummary, TreeFingerprint,
};
use officina_core::relay::run::RunPlan;

use super::kept::keep;
use crate::files::now_ms;
use crate::relay::home::RelayHome;
use crate::relay::journal;

/// What the start line records besides the plan.
pub(super) struct Started<'a> {
    pub params: &'a BTreeMap<String, String>,
    pub session_id: Option<&'a str>,
    pub tree: Option<TreeFingerprint>,
    pub approval_id: Option<String>,
}

/// The `run_started` line of `plan`.
pub(super) fn start_line(run_id: &str, plan: &RunPlan, started: Started<'_>) -> JournalEntry {
    let http = plan.http();
    JournalEntry::RunStarted(RunStarted {
        run_id: run_id.to_owned(),
        project: plan.project.clone(),
        session_id: started.session_id.map(str::to_owned),
        action: plan.action.clone(),
        class: plan.class,
        params: started.params.clone(),
        argv: plan.argv(),
        method: http.map(|h| h.method),
        url: http.map(|h| h.url.clone()),
        credential: http.and_then(|h| h.credential.as_ref().map(|c| c.name.clone())),
        catalogue_hash: plan.catalogue_hash.clone(),
        approval_id: started.approval_id,
        tree: started.tree,
        timeout_s: Some(http.map_or(plan.timeout_s, |h| h.timeout_s)),
    })
}

pub(super) fn elapsed_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

/// The process half of a `run_finished` line: what each stream carried and
/// what the budget dropped.
pub(super) struct Streams {
    pub stdout: StreamSummary,
    pub stderr: StreamSummary,
    pub dropped: u64,
}

/// The `run_finished` line of a process run.
pub(super) fn process_finished(
    run_id: &str,
    outcome: RunOutcome,
    exit_code: Option<i64>,
    started: Instant,
    streams: Streams,
    detail: Option<String>,
) -> JournalEntry {
    JournalEntry::RunFinished(RunFinished {
        run_id: run_id.to_owned(),
        outcome,
        exit_code,
        duration_ms: elapsed_ms(started),
        stdout: Some(streams.stdout),
        stderr: Some(streams.stderr),
        dropped_bytes: streams.dropped,
        status: None,
        body_bytes: None,
        body_sha256: None,
        detail,
        body_broke_off: false,
    })
}

/// The response half of a `run_finished` line: status, body bytes and
/// hash, and whether the body broke off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Response {
    pub status: u16,
    pub body_bytes: u64,
    pub body_sha256: String,
    pub body_broke_off: bool,
}

/// The `run_finished` line of an HTTP run.
pub(super) fn http_finished(
    run_id: &str,
    outcome: RunOutcome,
    started: Instant,
    response: Option<Response>,
    detail: Option<String>,
) -> JournalEntry {
    JournalEntry::RunFinished(RunFinished {
        run_id: run_id.to_owned(),
        outcome,
        exit_code: None,
        duration_ms: elapsed_ms(started),
        stdout: None,
        stderr: None,
        dropped_bytes: 0,
        status: response.as_ref().map(|r| r.status),
        body_bytes: response.as_ref().map(|r| r.body_bytes),
        body_broke_off: response.as_ref().is_some_and(|r| r.body_broke_off),
        body_sha256: response.map(|r| r.body_sha256),
        detail,
    })
}

/// The one ending of every run, in this order: a check run's kept output,
/// then the `run_finished` line, so a reader that sees the end finds the
/// output it pairs with (D-170). A journal that stays locked gets the line
/// in a spill file (D-760); a write that fails even so is said on `err`, and
/// the run stands either way.
pub(super) fn end_run(
    home: &RelayHome,
    err: &mut dyn Write,
    run_id: &str,
    line: JournalEntry,
    kept: Option<Vec<u8>>,
) {
    if let Some(bytes) = kept {
        keep(home, err, run_id, &bytes);
    }
    if let Err(e) = journal::record(home, &[line], now_ms()) {
        let _ = writeln!(
            err,
            "officina: cannot write the journal ({e}): this run's end is not recorded"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::approvals::kept_output_name;

    fn ended(home: &RelayHome) -> Vec<JournalEntry> {
        let (lines, _) = journal::read_recent(home, now_ms());
        lines.into_iter().map(|line| line.entry).collect()
    }

    #[test]
    fn the_end_keeps_the_output_then_journals() {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path(), None);
        let line = http_finished("r1", RunOutcome::Http, Instant::now(), None, None);
        let mut err = Vec::new();
        end_run(&home, &mut err, "r1", line.clone(), Some(b"kept".to_vec()));
        assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
        assert!(home.outputs_dir().join(kept_output_name("r1")).is_file());
        assert_eq!(ended(&home), [line]);
    }

    #[test]
    fn an_output_that_cannot_be_kept_is_said_and_the_end_still_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path(), None);
        std::fs::create_dir_all(home.relay_dir()).unwrap();
        std::fs::write(home.outputs_dir(), b"a file where the folder goes").unwrap();
        let line = http_finished("r2", RunOutcome::Timeout, Instant::now(), None, None);
        let mut err = Vec::new();
        end_run(&home, &mut err, "r2", line.clone(), Some(b"kept".to_vec()));
        let said = String::from_utf8_lossy(&err);
        assert!(
            said.starts_with("officina: this check's output could not be kept"),
            "{said}"
        );
        assert_eq!(ended(&home), [line]);
    }
}
