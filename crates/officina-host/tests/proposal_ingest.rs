//! Proposal ingestion (story 5.17 AC 1 and 3; AD-91, D-133, D-145,
//! D-155): the window's writer reads `orchestrator/proposals/`, stores each
//! file once by its id and deletes it; misnamed, oversized, unparsable,
//! temporary and rate-limited files each meet their rule. Every pass runs
//! at an explicit `now_ms`.

mod common;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use common::registry::{Home, NOW, day_file, line};
use officina_core::orders::proposal::{
    PROPOSAL_FILE_MAX_BYTES, PROPOSAL_HOUR_MS, PROPOSALS_PER_HOUR, ProposalFile,
    ProposalInvalidReason, ProposalTarget, ProposalView, ProposedSubject, RecapProposal,
    TMP_FILE_TTL_MS, WORK_ORDER_PROPOSAL_TTL_MS, WorkOrderProposal, file_name, proposal_id,
};
use officina_core::registry::confirm::SubjectOrigin;
use officina_core::registry::types::{ProposalKind, ProposalStatus};
use officina_core::relay::catalogue::{ApprovedCatalogue, CanonicalCatalogue, load};
use officina_host::mcp::write_proposal;
use officina_host::registry::store::{Writer, open_reader};

/// The session the test's subject is created from, and its folder.
const SESSION: &str = "s-api";
const CWD: &str = "C:/work/api";

const FLOOR: &str = r#"schema = 2
[[project]]
name = "api"
path = "C:/work/api"
[[template]]
name = "bump"
description = "Bump one dependency"
projects = ["api"]
prompt = "Bump {package}."
[template.params.package]
values = ["serde_json", "tokio"]
"#;

/// The approved snapshot of [`FLOOR`].
fn catalogue() -> CanonicalCatalogue {
    let current = load(FLOOR, "C:/path/to/floor/officina.toml").unwrap();
    ApprovedCatalogue::approving(&current, "C:/path/to/floor/officina.toml", 0).canonical
}

fn proposals_dir(home: &Home) -> PathBuf {
    home.home.join("orchestrator").join("proposals")
}

/// The writer of `home`, its registry holding session [`SESSION`] filed
/// under a subject of its project, whose id it returns.
fn window(home: &Home) -> (Writer, i64) {
    home.append(
        &day_file(NOW),
        &[line("SessionStart", SESSION, NOW - 60_000, CWD)],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    let subject = writer
        .create_subject(
            "upgrades",
            SubjectOrigin::Sessions(vec![SESSION.into()]),
            NOW,
        )
        .unwrap();
    (writer, subject)
}

fn recap(target: ProposalTarget, done: &str) -> ProposalFile {
    ProposalFile::recap(
        RecapProposal {
            target,
            done: done.into(),
            blocking: "nothing".into(),
            decision_expected: "merge it?".into(),
        },
        Some("s-orchestrator".into()),
        NOW,
    )
}

fn order(project: &str, package: &str, subject: ProposedSubject) -> ProposalFile {
    ProposalFile::work_order(
        WorkOrderProposal {
            template: "bump".into(),
            project: project.into(),
            values: [("package".to_owned(), package.to_owned())].into(),
            subject,
            rationale: "<b>tests</b> fail".into(),
        },
        None,
        NOW,
    )
}

fn rows(home: &Home) -> Vec<ProposalView> {
    open_reader(&home.home, &[]).unwrap().proposals().unwrap()
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// AD-91: a file is validated, stored once by its id and deleted; a second
/// pass finds nothing.
#[test]
fn a_file_is_ingested_once_then_deleted() {
    let home = Home::new();
    let (mut writer, subject) = window(&home);
    let dir = proposals_dir(&home);
    let id = write_proposal(
        &dir,
        &order("api", "tokio", ProposedSubject::Id(subject)),
        NOW,
    )
    .unwrap();
    let pass = writer
        .ingest_proposals(&dir, Some(&catalogue()), NOW + 1_000)
        .unwrap();
    assert_eq!(pass.ingested, 1);
    assert!(files(&dir).is_empty(), "deleted after its commit");
    let again = writer
        .ingest_proposals(&dir, Some(&catalogue()), NOW + 2_000)
        .unwrap();
    assert_eq!(again.ingested, 0);
    let stored = rows(&home);
    assert_eq!(stored.len(), 1);
    let row = &stored[0];
    assert_eq!(row.id, id);
    assert_eq!(
        (row.kind, row.status, row.received_at_ms),
        (
            Some(ProposalKind::WorkOrder),
            ProposalStatus::Pending,
            Some(NOW + 1_000)
        )
    );
    assert_eq!(row.written_at_ms, NOW);
}

/// D-155, AD-38: a file found again after its row was committed (a crash
/// between the commit and the deletion) is ignored by its key and deleted:
/// its row keeps what the first reading stored.
#[test]
fn a_file_left_after_its_commit_is_ignored_by_the_key() {
    let home = Home::new();
    let (mut writer, _) = window(&home);
    let dir = proposals_dir(&home);
    let id = write_proposal(
        &dir,
        &recap(ProposalTarget::SessionId(SESSION.into()), "first"),
        NOW,
    )
    .unwrap();
    let path = dir.join(file_name(&id));
    let kept = std::fs::read(&path).unwrap();
    writer.ingest_proposals(&dir, None, NOW + 1_000).unwrap();
    let before = rows(&home);
    // The same name back, another content: as if the deletion never ran.
    let other = String::from_utf8(kept).unwrap().replace("first", "second");
    std::fs::write(&path, other).unwrap();
    let pass = writer.ingest_proposals(&dir, None, NOW + 2_000).unwrap();
    assert_eq!(pass.ingested, 0);
    assert!(files(&dir).is_empty());
    assert_eq!(rows(&home), before);
}

/// D-155: a temporary file is never read; it goes once older than 24 h.
#[test]
fn tmp_files_are_never_read_and_go_after_24_h() {
    let home = Home::new();
    let (mut writer, _) = window(&home);
    let dir = proposals_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    let id = proposal_id(NOW, 7);
    let tmp = dir.join(format!("{}.4242.tmp", file_name(&id)));
    std::fs::write(&tmp, recap(ProposalTarget::SubjectId(1), "half").to_bytes()).unwrap();
    let written = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let pass = writer
        .ingest_proposals(&dir, None, written + TMP_FILE_TTL_MS - 60_000)
        .unwrap();
    assert_eq!((pass.ingested, pass.stale_tmp, pass.misnamed), (0, 0, 0));
    assert!(tmp.exists(), "younger than 24 h: kept, unread");
    let pass = writer
        .ingest_proposals(&dir, None, written + TMP_FILE_TTL_MS + 60_000)
        .unwrap();
    assert_eq!((pass.ingested, pass.stale_tmp), (0, 1));
    assert!(!tmp.exists());
    assert!(rows(&home).is_empty(), "never a row");
}

/// D-133 (review F3): a file that does not parse is stored invalid with
/// its reason and no kind, then deleted.
#[test]
fn an_unparsable_file_is_kept_as_invalid_without_kind() {
    let home = Home::new();
    let (mut writer, _) = window(&home);
    let dir = proposals_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    let id = proposal_id(NOW, 1);
    std::fs::write(
        dir.join(file_name(&id)),
        br#"{"kind":"answer","text":"yes"}"#,
    )
    .unwrap();
    writer.ingest_proposals(&dir, None, NOW + 1_000).unwrap();
    let stored = rows(&home);
    assert_eq!(stored.len(), 1);
    assert_eq!(
        (
            stored[0].kind,
            stored[0].status,
            stored[0].content.is_none()
        ),
        (None, ProposalStatus::Invalid, true)
    );
    let reason =
        officina_core::untrusted::wrapped(|| serde_json::to_value(&stored[0].invalid_reason))
            .unwrap();
    assert_eq!(
        reason,
        serde_json::json!({"untrusted": ProposalInvalidReason::Unparsable.code()})
    );
    assert!(files(&dir).is_empty());
}

/// D-155: a file whose name is no proposal's is deleted without a row and
/// counted; a folder is left alone.
#[test]
fn a_misnamed_file_is_deleted_and_counted() {
    let home = Home::new();
    let (mut writer, _) = window(&home);
    let dir = proposals_dir(&home);
    std::fs::create_dir_all(dir.join("a-folder")).unwrap();
    let valid = recap(ProposalTarget::SubjectId(1), "named badly").to_bytes();
    for name in [
        "notes.json",
        "1761700000000-00000000000000AB.json",
        "1761700000000-00000000000000ab.json.bak",
    ] {
        std::fs::write(dir.join(name), &valid).unwrap();
    }
    let pass = writer.ingest_proposals(&dir, None, NOW).unwrap();
    assert_eq!((pass.misnamed, pass.ingested), (3, 0));
    assert_eq!(files(&dir), ["a-folder"]);
    assert!(rows(&home).is_empty());
}

/// D-155: a file over 16 KiB is read no further than one byte past the
/// bound and stored invalid, `oversized`, with no kind and no texts.
#[test]
fn a_file_over_16_kib_is_oversized_without_texts() {
    let home = Home::new();
    let (mut writer, _) = window(&home);
    let dir = proposals_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    let id = proposal_id(NOW, 2);
    let mut bytes = recap(ProposalTarget::SubjectId(1), "big").to_bytes();
    bytes.resize(PROPOSAL_FILE_MAX_BYTES + 1, b' ');
    std::fs::write(dir.join(file_name(&id)), &bytes).unwrap();
    // At the bound exactly, a file is read whole.
    let at_bound = proposal_id(NOW, 3);
    let mut fits = recap(ProposalTarget::SubjectId(1), "fits").to_bytes();
    fits.resize(PROPOSAL_FILE_MAX_BYTES, b' ');
    std::fs::write(dir.join(file_name(&at_bound)), &fits).unwrap();
    writer.ingest_proposals(&dir, None, NOW + 1_000).unwrap();
    let stored = rows(&home);
    let big = stored.iter().find(|p| p.id == id).unwrap();
    assert_eq!(
        (big.kind, big.status, big.content.is_none()),
        (None, ProposalStatus::Invalid, true)
    );
    let reason =
        officina_core::untrusted::wrapped(|| serde_json::to_value(&big.invalid_reason)).unwrap();
    assert_eq!(reason["untrusted"], ProposalInvalidReason::Oversized.code());
    let fitting = stored.iter().find(|p| p.id == at_bound).unwrap();
    assert_eq!(fitting.kind, Some(ProposalKind::Recap));
    let db = home.paths().database();
    drop(writer);
    assert!(
        !common::registry::file_contains(&db, "\"big\""),
        "no texts stored"
    );
}

/// AD-90, D-155 (review F21): with the hour's proposals already stored,
/// ten thousand files written directly in the same hour are deleted
/// without a row, unread, and counted.
#[test]
fn ten_thousand_files_past_the_limit_create_no_row() {
    let home = Home::new();
    let (mut writer, _) = window(&home);
    let dir = proposals_dir(&home);
    let start = NOW - PROPOSAL_HOUR_MS / 2;
    for at in 0..PROPOSALS_PER_HOUR as i64 {
        write_proposal(
            &dir,
            &recap(ProposalTarget::SubjectId(1), "within"),
            start + at,
        )
        .unwrap();
    }
    let pass = writer.ingest_proposals(&dir, None, NOW).unwrap();
    assert_eq!(pass.ingested as usize, PROPOSALS_PER_HOUR);
    let flood = recap(ProposalTarget::SubjectId(1), "flood").to_bytes();
    for n in 0..10_000u64 {
        let id = proposal_id(NOW - 60_000 + (n as i64 % 60_000), n);
        std::fs::write(dir.join(file_name(&id)), &flood).unwrap();
    }
    let pass = writer.ingest_proposals(&dir, None, NOW + 1_000).unwrap();
    assert_eq!((pass.ingested, pass.over_limit), (0, 10_000));
    assert!(files(&dir).is_empty());
    assert_eq!(rows(&home).len(), PROPOSALS_PER_HOUR);
}

/// AD-91, D-130: every text, the invalid reason and the session label read
/// back untrusted; the author's own names stay what the row holds.
#[test]
fn texts_read_back_untrusted() {
    let home = Home::new();
    let (mut writer, subject) = window(&home);
    let dir = proposals_dir(&home);
    write_proposal(
        &dir,
        &recap(ProposalTarget::SubjectId(subject), "<script>x</script>"),
        NOW,
    )
    .unwrap();
    write_proposal(
        &dir,
        &order("api", "--settings", ProposedSubject::New("<i>n</i>".into())),
        NOW + 1,
    )
    .unwrap();
    writer
        .ingest_proposals(&dir, Some(&catalogue()), NOW + 1_000)
        .unwrap();
    let stored = rows(&home);
    let marked = officina_core::untrusted::wrapped(|| serde_json::to_value(&stored)).unwrap();
    let invalid = &marked[0];
    assert_eq!(invalid["status"], "invalid");
    assert_eq!(
        invalid["invalid_reason"],
        serde_json::json!({"untrusted": ProposalInvalidReason::ValueNotInList.code()})
    );
    let content = &invalid["content"]["work_order"];
    assert_eq!(
        content["rationale"],
        serde_json::json!({"untrusted": "<b>tests</b> fail"})
    );
    assert_eq!(
        content["values"],
        serde_json::json!({"untrusted": {"package": "--settings"}})
    );
    assert_eq!(
        content["subject"],
        serde_json::json!({"new": {"untrusted": "<i>n</i>"}})
    );
    let recap = &marked[1];
    assert_eq!(
        recap["content"]["recap"]["done"],
        serde_json::json!({"untrusted": "<script>x</script>"})
    );
    assert_eq!(
        recap["orchestrator_session_id"],
        serde_json::json!({"untrusted": "s-orchestrator"})
    );
}

/// D-145: a newer valid recap on the same target supersedes the pending
/// one at ingestion; a work order proposal expires 24 h after its receipt,
/// at the pass that follows.
#[test]
fn a_newer_recap_supersedes_and_an_order_expires() {
    let home = Home::new();
    let (mut writer, subject) = window(&home);
    let dir = proposals_dir(&home);
    let older =
        write_proposal(&dir, &recap(ProposalTarget::SubjectId(subject), "old"), NOW).unwrap();
    let order_id = write_proposal(
        &dir,
        &order("api", "tokio", ProposedSubject::Id(subject)),
        NOW + 1,
    )
    .unwrap();
    writer
        .ingest_proposals(&dir, Some(&catalogue()), NOW + 1_000)
        .unwrap();
    let newer = write_proposal(
        &dir,
        &recap(ProposalTarget::SubjectId(subject), "new"),
        NOW + 2_000,
    )
    .unwrap();
    let pass = writer
        .ingest_proposals(&dir, Some(&catalogue()), NOW + 3_000)
        .unwrap();
    assert_eq!(pass.superseded, 1);
    let status = |id: &str| rows(&home).into_iter().find(|p| p.id == id).unwrap().status;
    assert_eq!(status(&older), ProposalStatus::Superseded);
    assert_eq!(status(&newer), ProposalStatus::Pending);
    assert_eq!(status(&order_id), ProposalStatus::Pending);
    let pass = writer
        .ingest_proposals(&dir, None, NOW + 1_000 + WORK_ORDER_PROPOSAL_TTL_MS)
        .unwrap();
    assert_eq!(pass.expired, 1);
    assert_eq!(status(&order_id), ProposalStatus::Expired);
    assert_eq!(
        status(&newer),
        ProposalStatus::Pending,
        "a recap never expires"
    );
}
