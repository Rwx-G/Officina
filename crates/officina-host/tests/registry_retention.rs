//! The hourly pass (story 3.2 AC 6 and 7; AD-44, D-48, D-50, D-84, D-85,
//! D-91, D-94): ingestion first, then the daily copy, then retention, the
//! WAL truncated; nothing deleted while the registry is unavailable or
//! read-only. Every pass runs at an explicit `now_ms`.

mod common;
#[path = "../../../tests/support/spawn_cases.rs"]
mod spawn_cases;

use std::path::Path;

use common::registry::{DAY, Home, NOW, damage_page, day_file, file_contains, line, prompt};
use officina_core::dates::{day_start_ms, utc_date};
use officina_core::registry::ingestion::COMPLETE_GRACE_MS;
use officina_core::registry::retention::{COPIES_KEPT, EXCERPT_TTL_MS};
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::schema::SCHEMA_VERSION;
use officina_host::registry::store::{daily_copies, open_reader, quarantined};
use officina_host::registry::window::WindowRegistry;

const CWD: &str = "C:/work/api";
const HOUR: i64 = 3_600_000;

/// Noon on the UTC day `days` before `NOW`'s.
fn noon(days_before: i64) -> i64 {
    NOW - days_before * DAY
}

/// One event file per day, `days` days back to today, each with a session
/// of its own named after its day.
fn days_of_events(home: &Home, days: i64) -> Vec<String> {
    (0..=days)
        .rev()
        .map(|back| {
            let name = day_file(noon(back));
            home.append(
                &name,
                &[line("Stop", &format!("day-{back}"), noon(back), CWD)],
            );
            name
        })
        .collect()
}

fn on_disk(home: &Home) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&home.events)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn session_ids(home: &Home) -> Vec<String> {
    open_reader(&home.home, &[])
        .unwrap()
        .sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.session_id)
        .collect()
}

#[test]
fn four_utc_days_are_ingested_before_any_deletion() {
    let home = Home::new();
    let files = days_of_events(&home, 3);
    let mut writer = home.writer(NOW);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.ingested.lines, 4);
    assert_eq!(session_ids(&home), ["day-0", "day-1", "day-2", "day-3"]);
    assert!(
        report.deleted_event_files.is_empty(),
        "one copy holds them, not two"
    );
    assert_eq!(on_disk(&home), files);
}

/// D-48: a file goes once both kept copies hold it complete; a copy taken
/// before the file was complete does not count.
#[test]
fn deletion_waits_for_both_copies() {
    let home = Home::new();
    let files = days_of_events(&home, 3);
    let mut writer = home.writer(NOW);
    // Two days ago, a minute past midnight: the file of three days ago is
    // complete, and one copy holds it, which is not enough.
    let start_of_two_back = day_start_ms(utc_date(noon(2)));
    let report = writer
        .hourly_pass(&home.ctx(), start_of_two_back + COMPLETE_GRACE_MS)
        .unwrap();
    assert!(report.copy.is_some());
    assert!(report.deleted_event_files.is_empty(), "one copy is kept");
    // Yesterday, within the grace: the copy taken now is the second to hold
    // the file of three days ago complete, which goes; the file of two days
    // ago is not complete yet, so this copy does not hold it complete.
    let start_of_yesterday = day_start_ms(utc_date(noon(1)));
    let report = writer
        .hourly_pass(&home.ctx(), start_of_yesterday + 1_000)
        .unwrap();
    assert_eq!(report.deleted_event_files, [files[0].clone()]);
    // Today: two days ago's file is before yesterday and complete, but the
    // copy of yesterday does not hold it complete.
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert!(report.deleted_event_files.is_empty(), "{report:?}");
    assert!(on_disk(&home).contains(&files[1]));
    // Tomorrow, the copies of today and tomorrow both hold it, and
    // yesterday's file too.
    let report = writer.hourly_pass(&home.ctx(), NOW + DAY).unwrap();
    assert_eq!(report.deleted_event_files, files[1..3]);
    assert_eq!(
        session_ids(&home),
        ["day-0", "day-1", "day-2", "day-3"],
        "deletion loses no session"
    );
}

#[test]
fn a_damaged_copy_does_not_count() {
    let home = Home::new();
    let files = days_of_events(&home, 3);
    let mut writer = home.writer(NOW);
    writer.hourly_pass(&home.ctx(), noon(1)).unwrap();
    let yesterday_copy = daily_copies(&home.paths())[0].path.clone();
    damage_page(&yesterday_copy);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert!(report.deleted_event_files.is_empty(), "{report:?}");
    assert_eq!(on_disk(&home), files);
    assert!(!yesterday_copy.exists(), "a damaged copy is not kept");
    assert_eq!(daily_copies(&home.paths()).len(), 1);
}

/// A registry whose two copies hold the two oldest files complete, ready
/// for a pass that would delete them.
fn ready_to_delete(home: &Home) -> Vec<String> {
    let files = days_of_events(home, 3);
    let mut writer = home.writer(noon(1));
    writer.ingest(&home.ctx(), noon(1)).unwrap();
    writer.copy_if_due(noon(1)).unwrap();
    writer.copy_if_due(NOW).unwrap();
    files
}

#[test]
fn unavailable_registry_deletes_nothing() {
    let home = Home::new();
    let files = days_of_events(&home, 5);
    std::fs::create_dir_all(&home.home).unwrap();
    std::fs::write(home.paths().dir(), b"a file where the folder goes").unwrap();
    let mut registry = WindowRegistry::open(&home.home, &[], NOW);
    assert!(registry.hourly_pass(&home.ctx(), NOW).is_none());
    assert!(registry.ingest(&home.ctx(), NOW).is_none());
    assert_eq!(on_disk(&home), files);
}

#[test]
fn read_only_registry_deletes_nothing() {
    let home = Home::new();
    let files = ready_to_delete(&home);
    rusqlite::Connection::open(home.paths().database())
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    let mut registry = WindowRegistry::open(&home.home, &[], NOW);
    assert!(registry.writer().is_none());
    assert!(registry.hourly_pass(&home.ctx(), NOW).is_none());
    assert_eq!(on_disk(&home), files);
    // The same registry at this build's version deletes them.
    rusqlite::Connection::open(home.paths().database())
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .unwrap();
    let mut registry = WindowRegistry::open(&home.home, &[], NOW);
    let report = registry.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.deleted_event_files, files[..2]);
}

fn prompt_of(home: &Home, session: &str) -> Option<String> {
    open_reader(&home.home, &[])
        .unwrap()
        .excerpts()
        .unwrap()
        .into_iter()
        .find(|e| e.session_id == session)
        .and_then(|e| e.prompt)
}

/// D-50: an excerpt's lifetime counts from its event, never from its
/// ingestion, so a window closed for weeks cannot lengthen it.
#[test]
fn excerpts_expire_from_their_event_time() {
    let home = Home::new();
    let event = NOW - EXCERPT_TTL_MS + HOUR;
    home.append(
        &day_file(event),
        &[prompt("s1", event, CWD, "kept until its time")],
    );
    let mut writer = home.writer(NOW);
    writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(
        prompt_of(&home, "s1").as_deref(),
        Some("kept until its time")
    );
    writer
        .hourly_pass(&home.ctx(), event + EXCERPT_TTL_MS - 1)
        .unwrap();
    assert!(prompt_of(&home, "s1").is_some());
    let report = writer
        .hourly_pass(&home.ctx(), event + EXCERPT_TTL_MS)
        .unwrap();
    assert_eq!(report.cleared_excerpts, 1);
    assert_eq!(prompt_of(&home, "s1"), None);

    // Ingested after its expiry, an excerpt is never stored.
    let late = Home::new();
    let old = NOW - EXCERPT_TTL_MS - DAY;
    late.append(&day_file(old), &[prompt("s2", old, CWD, "already expired")]);
    late.writer(NOW).hourly_pass(&late.ctx(), NOW).unwrap();
    assert_eq!(session_ids(&late), ["s2"]);
    assert_eq!(prompt_of(&late, "s2"), None);
}

/// D-85: turning `no_excerpts` on clears what is stored at the next pass.
#[test]
fn no_excerpts_turned_on_clears_stored_excerpts() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[prompt("s1", NOW - 5, CWD, "stored before")],
    );
    let mut writer = home.writer(NOW);
    writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert!(prompt_of(&home, "s1").is_some());
    let report = writer
        .hourly_pass(
            &IngestContext {
                no_excerpts: true,
                ..home.ctx()
            },
            NOW + HOUR,
        )
        .unwrap();
    assert_eq!(report.cleared_excerpts, 1);
    assert!(
        open_reader(&home.home, &[])
            .unwrap()
            .excerpts()
            .unwrap()
            .is_empty()
    );
    assert_eq!(session_ids(&home), ["s1"], "the session stays");
}

#[test]
fn expired_text_leaves_the_wal() {
    const MARKER: &str = "MARKER-7f3e-expired-prompt";
    let home = Home::new();
    let event = NOW - EXCERPT_TTL_MS;
    home.append(
        &day_file(NOW),
        &[prompt(
            "s0",
            NOW - 3,
            CWD,
            "a later session keeps the page busy",
        )],
    );
    let mut writer = home.writer(event);
    writer
        .set_setting(
            officina_core::registry::types::SettingKey::AutoArchive,
            "false",
            officina_core::registry::types::Source::Author,
            event,
        )
        .unwrap();
    home.append(&day_file(event), &[prompt("s1", event, CWD, MARKER)]);
    writer.ingest(&home.ctx(), event).unwrap();
    assert!(
        file_contains(&home.paths().wal(), MARKER)
            || file_contains(&home.paths().database(), MARKER),
        "positive control"
    );
    writer.hourly_pass(&home.ctx(), NOW).unwrap();
    for file in [home.paths().database(), home.paths().wal()] {
        assert!(!file_contains(&file, MARKER), "{}", file.display());
    }
}

/// D-91: the hourly pass truncates the WAL whatever it did.
#[test]
fn hourly_pass_truncates_the_wal() {
    let home = Home::new();
    days_of_events(&home, 1);
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    assert!(std::fs::metadata(home.paths().wal()).unwrap().len() > 0);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert!(report.checkpointed);
    assert_eq!(std::fs::metadata(home.paths().wal()).unwrap().len(), 0);
    let idle = writer.hourly_pass(&home.ctx(), NOW + HOUR).unwrap();
    assert!(idle.checkpointed, "with nothing to do");
    assert_eq!(std::fs::metadata(home.paths().wal()).unwrap().len(), 0);
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// AD-44, D-94: copies, quarantined databases and pre-migration copies are
/// kept two, and for two days, by the date in their name.
#[test]
fn copies_quarantine_and_pre_migration_copies_kept_two_for_2_days() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    for back in [3, 1] {
        writer.copy_if_due(noon(back)).unwrap();
    }
    let backup = home.paths().backup_dir();
    for day in ["2026-10-27", "2026-10-26", "2026-10-25"] {
        std::fs::copy(
            backup.join("registry-2026-10-26.db"),
            backup.join(format!("pre-v2-{day}.db")),
        )
        .unwrap();
    }
    for back in [0, 1, 2] {
        let dir = home.paths().quarantine_dir().join((noon(back)).to_string());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("registry.db"), b"set aside").unwrap();
    }
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(
        report.copy.as_deref(),
        Some(backup.join("registry-2026-10-27.db").as_path())
    );
    assert_eq!(
        names_in(&backup),
        [
            "pre-v2-2026-10-26.db",
            "pre-v2-2026-10-27.db",
            "registry-2026-10-26.db",
            "registry-2026-10-27.db"
        ]
    );
    assert_eq!(
        quarantined(&home.paths()),
        [noon(1), noon(0)]
            .map(|ms| home.paths().quarantine_dir().join(ms.to_string()))
            .to_vec()
    );
    // Two days on, yesterday's copy is past its lifetime even though only
    // two are left.
    writer.hourly_pass(&home.ctx(), NOW + 2 * DAY).unwrap();
    assert!(names_in(&backup).iter().all(|n| !n.contains("2026-10-26")));
    assert!(quarantined(&home.paths()).is_empty());
    assert!(daily_copies(&home.paths()).len() <= COPIES_KEPT);
}

/// AD-44, D-84: within one pass, the copy holds what was just ingested,
/// and retention counts the copy just taken.
#[test]
fn hourly_pass_ingests_then_copies_then_retains() {
    let home = Home::new();
    let files = days_of_events(&home, 3);
    let mut writer = home.writer(noon(1));
    // Yesterday: every file but today's ingested, yesterday's copy taken.
    writer.ingest(&home.ctx(), noon(1)).unwrap();
    writer.copy_if_due(noon(1)).unwrap();
    // Today's line arrives while the window is busy elsewhere.
    home.append(&files[3], &[line("Stop", "late", NOW - 1, CWD)]);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    // Ingested before the copy: today's copy holds the late line's session.
    let copy = report.copy.unwrap();
    let in_copy: Vec<String> =
        rusqlite::Connection::open_with_flags(&copy, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .prepare("SELECT session_id FROM session ORDER BY session_id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
    assert!(in_copy.contains(&"late".to_owned()), "{in_copy:?}");
    // Copied before retention: only with today's copy are two copies kept
    // holding the two oldest files complete, and they go in this pass.
    assert_eq!(report.deleted_event_files, files[..2]);
}

/// Story 3.9 AC 10 (AD-44): a `triage_call` row is kept 90 days from its
/// start, then the hourly pass deletes it.
#[test]
fn triage_call_rows_are_kept_90_days() {
    use officina_core::registry::retention::TRIAGE_CALL_TTL_MS;
    use officina_core::registry::triage_call::{CallUsage, TriageCase};
    use officina_core::registry::types::{TriageOutcome, TriagePurpose};
    use officina_host::registry::triage::CallResult;

    let home = Home::new();
    let mut writer = home.writer(NOW);
    let call = |started_at_ms| CallResult {
        purpose: TriagePurpose::DryRun,
        case_key: "confirmation:1".into(),
        input_hash: TriageCase::default().input_hash(),
        started_at_ms,
        duration_ms: 1_000,
        outcome: TriageOutcome::Answer,
        refusal: None,
        answer: None,
        usage: CallUsage::default(),
        detail: None,
    };
    let kept = NOW - TRIAGE_CALL_TTL_MS + 1;
    let gone = NOW - TRIAGE_CALL_TTL_MS;
    writer.record_call(&call(kept)).unwrap();
    writer.record_call(&call(gone)).unwrap();
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.deleted_calls, 1);
    let left: Vec<i64> = writer
        .triage_calls_since(0)
        .unwrap()
        .into_iter()
        .map(|c| c.started_at_ms)
        .collect();
    assert_eq!(left, [kept]);
    writer
        .hourly_pass(&home.ctx(), kept + TRIAGE_CALL_TTL_MS)
        .unwrap();
    assert!(writer.triage_calls_since(0).unwrap().is_empty());
}

/// Records a launch of the golden `dependency-bump` order as card `card`,
/// with `seq`, at `at`, and its outcome with what it printed and erred.
fn launched(
    writer: &mut officina_host::registry::store::Writer,
    home: &Home,
    card: &str,
    seq: u32,
    at: i64,
) {
    use officina_core::orders::work_order::SubjectChoice;
    use officina_core::registry::types::{LaunchOutcome, WorkOrderSource};
    use officina_core::spawn::VectorKind;
    use officina_host::registry::orders::LaunchRecord;

    let order = spawn_cases::load()
        .cases
        .into_iter()
        .find(|case| case.kind() == VectorKind::WorkOrder)
        .unwrap()
        .work_order(Some(&home.root.to_string_lossy()));
    writer
        .insert_work_order(
            &LaunchRecord {
                order: &order,
                card_id: card,
                seq,
                source: WorkOrderSource::Composer,
                subject: &SubjectChoice::Existing(1),
                proposal_id: None,
                permission: officina_core::orders::permission::WorkOrderPermission::Project,
            },
            at,
        )
        .unwrap()
        .unwrap();
    writer
        .record_launch_outcome(
            card,
            LaunchOutcome::Failed,
            Some(1),
            Some(&format!("error of {card}")),
            Some(&format!("printed by {card}")),
            at,
        )
        .unwrap();
}

/// Story 5.6 AC 8 (D-134): a `work_order` row is deleted 90 days after its
/// launch by the hourly pass.
#[test]
fn work_order_rows_are_deleted_at_90_days() {
    use officina_core::registry::retention::WORK_ORDER_TTL_MS;

    let home = Home::new();
    let mut writer = home.writer(NOW);
    let kept = NOW - WORK_ORDER_TTL_MS + 1;
    let gone = NOW - WORK_ORDER_TTL_MS;
    launched(&mut writer, &home, "kept", 1, kept);
    launched(&mut writer, &home, "gone", 2, gone);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.deleted_work_orders, 1);
    let left: Vec<String> = writer
        .work_orders()
        .unwrap()
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert_eq!(left, ["kept"]);
    writer
        .hourly_pass(&home.ctx(), kept + WORK_ORDER_TTL_MS)
        .unwrap();
    assert!(writer.work_orders().unwrap().is_empty());
}

/// One review row of `id`, requested at `at`, with a reason naming `id`.
fn reviewed(writer: &mut officina_host::registry::store::Writer, id: &str, at: i64) {
    use officina_core::registry::types::{ReviewMode, ReviewOutcome};
    use officina_core::relay::auto::{EscalationCause, ReviewDecision};
    writer
        .insert_review(&officina_host::registry::review::NewReview {
            approval_id: id.into(),
            request_key: "ab".into(),
            project: "lab".into(),
            action: "apply".into(),
            requested_at_ms: at,
            mode: ReviewMode::Shadow,
            outcome: ReviewOutcome::Answered,
            cause: Some(EscalationCause::ReviewEscalated),
            decision: Some(ReviewDecision::Escalate),
            reason: Some(format!("reason of {id}")),
            rule_baseline: false,
            call_started_at_ms: Some(at),
            duration_ms: Some(1),
            input_tokens: None,
            output_tokens: None,
            cost_microusd: None,
        })
        .unwrap();
}

/// Story 5.13 AC 10 (D-134): `auto_review` rows go 90 days after their
/// request.
#[test]
fn auto_review_rows_are_deleted_at_90_days() {
    use officina_core::registry::retention::AUTO_REVIEW_TTL_MS;
    let home = Home::new();
    let mut writer = home.writer(NOW);
    reviewed(&mut writer, "kept", NOW - AUTO_REVIEW_TTL_MS + 1);
    reviewed(&mut writer, "gone", NOW - AUTO_REVIEW_TTL_MS);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.deleted_auto_reviews, 1);
    let left: Vec<String> = writer
        .review_rows(0)
        .unwrap()
        .into_iter()
        .map(|r| r.approval_id)
        .collect();
    assert_eq!(left, ["kept"]);
}

/// Story 5.16 AC 8 (D-134): `orchestrator_start` rows go 90 days after
/// their start, whatever their outcome.
#[test]
fn orchestrator_start_rows_are_deleted_at_90_days() {
    use officina_core::registry::retention::ORCHESTRATOR_START_TTL_MS;
    use officina_core::registry::types::OrchestratorStartOutcome;
    use officina_host::registry::orders::OrchestratorStartRecord;
    let home = Home::new();
    let mut writer = home.writer(NOW);
    for (at, outcome) in [
        (
            NOW - ORCHESTRATOR_START_TTL_MS,
            OrchestratorStartOutcome::Spawned,
        ),
        (
            NOW - ORCHESTRATOR_START_TTL_MS,
            OrchestratorStartOutcome::Refused,
        ),
        (
            NOW - ORCHESTRATOR_START_TTL_MS + 1,
            OrchestratorStartOutcome::Spawned,
        ),
    ] {
        let spawned = outcome == OrchestratorStartOutcome::Spawned;
        writer
            .record_orchestrator_start(&OrchestratorStartRecord {
                started_at_ms: at,
                outcome,
                pid: spawned.then_some(7),
                pid_created_at_ms: spawned.then_some(at - 5),
                error: (!spawned).then_some("workdir_not_empty"),
            })
            .unwrap();
    }
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.deleted_orchestrator_starts, 2);
    let left: Vec<i64> = writer
        .orchestrator_starts()
        .unwrap()
        .into_iter()
        .map(|row| row.started_at_ms)
        .collect();
    assert_eq!(left, [NOW - ORCHESTRATOR_START_TTL_MS + 1]);
}

/// A recap whose texts carry `marker`, written and ingested at `at` by the
/// window: its `received_at_ms` is `at`.
fn proposal_received(
    writer: &mut officina_host::registry::store::Writer,
    home: &Home,
    marker: &str,
    at: i64,
) -> String {
    use officina_core::orders::proposal::{ProposalFile, ProposalTarget, RecapProposal};
    let dir = home.home.join("orchestrator").join("proposals");
    let file = ProposalFile::recap(
        RecapProposal {
            target: ProposalTarget::SubjectId(1),
            done: format!("done by {marker}"),
            blocking: "nothing".into(),
            decision_expected: "none".into(),
        },
        None,
        at,
    );
    let id = officina_host::mcp::write_proposal(&dir, &file, at).unwrap();
    let pass = writer.ingest_proposals(&dir, None, at).unwrap();
    assert_eq!(pass.ingested, 1);
    id
}

/// Story 5.17 AC 9 (D-134): `proposal` rows go 90 days after the window
/// received them, whatever their status: a recap pending forever leaves.
#[test]
fn proposal_rows_are_deleted_at_90_days() {
    use officina_core::registry::retention::PROPOSAL_TTL_MS;
    let home = Home::new();
    let mut writer = home.writer(NOW);
    proposal_received(&mut writer, &home, "gone", NOW - PROPOSAL_TTL_MS);
    let kept = proposal_received(&mut writer, &home, "kept", NOW - PROPOSAL_TTL_MS + 1);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.deleted_proposals, 1);
    let reader = open_reader(&home.home, &[]).unwrap();
    let left: Vec<String> = reader
        .proposals()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(left, [kept]);
}

/// Story 5.17 AC 9 (D-134): a proposal's texts, the orchestrator's, are
/// cleared `EXCERPT_TTL_MS` after its receipt, the row kept, and leave the
/// database and its WAL.
#[test]
fn proposal_texts_are_cleared_with_the_excerpt_ttl() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let kept = proposal_received(&mut writer, &home, "kept", NOW - EXCERPT_TTL_MS + 1);
    let cleared = proposal_received(&mut writer, &home, "cleared", NOW - EXCERPT_TTL_MS);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.cleared_proposal_texts, 1);
    let again = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(again.cleared_proposal_texts, 0, "counted once");
    drop(writer);
    let rows = open_reader(&home.home, &[]).unwrap().proposals().unwrap();
    let content = |id: &str| rows.iter().find(|p| p.id == id).unwrap().content.is_some();
    assert!(content(&kept));
    assert!(!content(&cleared), "the row stays, its texts gone");
    let db = home.paths().database();
    assert!(file_contains(&db, "done by kept"), "positive control");
    for leftover in [&db, &home.paths().wal()] {
        assert!(
            !file_contains(leftover, "done by cleared"),
            "{}",
            leftover.display()
        );
    }
}

/// Story 5.13 AC 10 (D-134): a review's reason, model text, is cleared
/// `EXCERPT_TTL_MS` after the request, the row kept, and leaves the
/// database and its WAL.
#[test]
fn review_reasons_are_cleared_with_the_excerpt_ttl() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    reviewed(&mut writer, "kept", NOW - EXCERPT_TTL_MS + 1);
    reviewed(&mut writer, "cleared", NOW - EXCERPT_TTL_MS);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.cleared_review_reasons, 1);
    let rows = writer.review_rows(0).unwrap();
    let reason = |id: &str| {
        rows.iter()
            .find(|r| r.approval_id == id)
            .unwrap()
            .reason
            .is_some()
    };
    assert!(reason("kept"));
    assert!(!reason("cleared"), "the row stays, its reason gone");
    drop(writer);
    let db = home.paths().database();
    assert!(file_contains(&db, "reason of kept"), "positive control");
    for leftover in [&db, &home.paths().wal()] {
        assert!(
            !file_contains(leftover, "reason of cleared"),
            "{}",
            leftover.display()
        );
    }
}

/// Story 5.6 AC 8 (D-134): `printed` and `error` are cleared
/// `EXCERPT_TTL_MS` after the launch, the row kept, so D-85's 30-day bound
/// covers them; the cleared text leaves the database and its WAL.
#[test]
fn work_order_texts_are_cleared_with_the_excerpt_ttl() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let kept = NOW - EXCERPT_TTL_MS + 1;
    let cleared = NOW - EXCERPT_TTL_MS;
    launched(&mut writer, &home, "kept", 1, kept);
    launched(&mut writer, &home, "cleared", 2, cleared);
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(report.cleared_work_order_texts, 1);
    let orders = writer.work_orders().unwrap();
    let texts = |id: &str| {
        let order = orders.iter().find(|o| o.id == id).unwrap();
        (order.printed.is_some(), order.error.is_some())
    };
    assert_eq!(texts("kept"), (true, true));
    assert_eq!(texts("cleared"), (false, false), "the row stays");
    let again = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert_eq!(again.cleared_work_order_texts, 0, "counted once");
    drop(writer);
    let db = home.paths().database();
    assert!(file_contains(&db, "error of kept"), "positive control");
    for leftover in [&db, &home.paths().wal()] {
        assert!(
            !file_contains(leftover, "error of cleared"),
            "{}",
            leftover.display()
        );
        assert!(
            !file_contains(leftover, "printed by cleared"),
            "{}",
            leftover.display()
        );
    }
}

/// Records a workshop run of the golden order as `id`, launched at `at`,
/// left in `state`.
fn workshop_run(
    writer: &mut officina_host::registry::store::Writer,
    home: &Home,
    id: &str,
    seq: u32,
    at: i64,
    state: officina_core::workshop::run::RunState,
) {
    use officina_core::orders::work_order::SubjectChoice;
    use officina_core::registry::types::WorkOrderSource;
    use officina_core::spawn::VectorKind;
    use officina_host::registry::orders::LaunchRecord;
    use officina_host::registry::workshop::RunInsert;

    let order = spawn_cases::load()
        .cases
        .into_iter()
        .find(|case| case.kind() == VectorKind::WorkOrder)
        .unwrap()
        .work_order(Some(&home.root.to_string_lossy()));
    let argv = vec!["-p".to_owned()];
    let branch = format!("officina/bump-{seq}");
    writer
        .insert_workshop_run(
            &LaunchRecord {
                order: &order,
                card_id: id,
                seq,
                source: WorkOrderSource::Composer,
                subject: &SubjectChoice::New("bump".to_owned()),
                proposal_id: None,
                permission: officina_core::orders::permission::WorkOrderPermission::Project,
            },
            &RunInsert {
                image: "officina-workshop:local",
                image_id: "sha256:aa",
                gateway_image_id: "sha256:bb",
                base_branch: "main",
                base_commit: "cc",
                parent_run_id: None,
                branch: &branch,
                uncommitted: 0,
                plan_json: "{}",
                plan_sha256: "dd",
                credential_kind: officina_core::workshop::floor::CredentialKind::OauthToken,
                deadline_ms: 3_600_000,
                battery_id: "ee",
                argv: &argv,
            },
            at,
        )
        .unwrap()
        .unwrap();
    writer.set_run_state(id, state, at).unwrap();
}

/// Story 6.16 AC 4 (AD-124, D-213, review F39): the hourly pass discards
/// a package still undecided at `RUN_FOLDER_TTL_MS` with reason `expired`,
/// in the pass that deletes its folder, and reports it for its journal
/// line; a younger package and a decided one keep their state.
#[test]
fn an_expired_package_is_discarded_with_its_reason() {
    use officina_core::workshop::review::{DiscardReason, PackageVerdict};
    use officina_core::workshop::run::{RUN_FOLDER_TTL_MS, RunState};

    let home = Home::new();
    let mut writer = home.writer(NOW);
    let due = "11111111111111111111111111111111";
    let quarantined = "22222222222222222222222222222222";
    let young = "33333333333333333333333333333333";
    let decided = "44444444444444444444444444444444";
    let old = NOW - RUN_FOLDER_TTL_MS;
    workshop_run(&mut writer, &home, due, 1, old, RunState::Review);
    workshop_run(
        &mut writer,
        &home,
        quarantined,
        2,
        old,
        RunState::Quarantined,
    );
    workshop_run(&mut writer, &home, young, 3, old + 1, RunState::Review);
    workshop_run(&mut writer, &home, decided, 4, old, RunState::Imported);
    let runs = home.home.join("workshop").join("runs");
    for id in [due, quarantined, young, decided] {
        std::fs::create_dir_all(runs.join(id).join("verify.git")).unwrap();
    }
    let report = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    let mut expired = report.workshop.expired.clone();
    expired.sort();
    assert_eq!(expired, [due, quarantined]);
    for id in [due, quarantined] {
        let row = writer.workshop_run(id).unwrap().unwrap();
        assert_eq!(row.state, RunState::Discarded, "{id}");
        assert_eq!(row.verdict, Some(PackageVerdict::Discarded), "{id}");
        assert_eq!(row.discard_reason, Some(DiscardReason::Expired), "{id}");
        assert_eq!(row.decided_at_ms, Some(NOW), "{id}");
        assert!(
            !runs.join(id).exists(),
            "{id}'s folder went in the same pass"
        );
    }
    assert_eq!(
        writer.workshop_run(young).unwrap().unwrap().state,
        RunState::Review
    );
    assert!(runs.join(young).exists());
    assert_eq!(
        writer.workshop_run(decided).unwrap().unwrap().state,
        RunState::Imported
    );
    // The next pass finds nothing left to expire.
    let again = writer.hourly_pass(&home.ctx(), NOW).unwrap();
    assert!(again.workshop.expired.is_empty());
}

/// Quality audit M2: a read of the cards' subjects that fails is reported,
/// never shown as a ready registry with no subject.
#[test]
fn a_failed_read_of_the_cards_subjects_is_reported() {
    let home = Home::new();
    let mut registry = WindowRegistry::open(&home.home, &[], NOW);
    assert!(registry.card_subjects().is_empty());
    assert_eq!(registry.report().detail, None);
    rusqlite::Connection::open(home.paths().database())
        .unwrap()
        .execute_batch("ALTER TABLE session RENAME TO session_moved;")
        .unwrap();
    assert!(registry.card_subjects().is_empty());
    let report = registry.report();
    assert!(
        report
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("session")),
        "{report:?}"
    );
}
