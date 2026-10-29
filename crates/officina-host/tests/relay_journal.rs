//! The journal's locked append, its spill (D-760) and two-day reader
//! (AD-32), and the meeting's read back to a session's start (D-169).
//! Every test works in its own temp home.

use std::fs::{self, OpenOptions};
use std::time::{Duration, Instant};

use officina_core::dates::event_file_name;
use officina_core::relay::journal::{
    CatalogueApproved, JOURNAL_RETENTION_DAYS, JournalEntry, JournalKind, JournalLine,
    spill_file_name, spilled_day_file,
};
use officina_host::relay::home::RelayHome;
use officina_host::relay::journal::{
    JournalError, JournalLock, LOCK_TIMEOUT, append, prune, read_recent, record,
};

const DAY: i64 = 86_400_000;
const NOW: i64 = 1_793_102_400_000;

fn approved(hash: &str) -> JournalEntry {
    JournalEntry::CatalogueApproved(CatalogueApproved {
        hash: hash.to_owned(),
        previous_hash: None,
    })
}

/// Holds `journal/.lock` from outside the journal module, as another
/// process would.
fn hold_lock(home: &RelayHome) -> fs::File {
    fs::create_dir_all(home.journal_dir()).unwrap();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(home.journal_lock_file())
        .unwrap();
    file.try_lock().unwrap();
    file
}

fn day_file(home: &RelayHome, ts: i64) -> std::path::PathBuf {
    home.journal_dir().join(event_file_name(ts))
}

#[test]
fn append_writes_one_line_under_the_lock_file() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let held = hold_lock(&home);

    let writer_home = home.clone();
    let writer = std::thread::spawn(move || append(&writer_home, &[approved("01")], NOW));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !day_file(&home, NOW).exists(),
        "nothing is written while another writer holds the lock"
    );
    held.unlock().unwrap();
    writer.join().unwrap().unwrap();

    let content = fs::read_to_string(day_file(&home, NOW)).unwrap();
    assert_eq!(content.matches('\n').count(), 1, "{content}");
    let line: JournalLine = serde_json::from_str(content.trim_end()).unwrap();
    assert_eq!(line, JournalLine::new(NOW, approved("01")));
    assert!(home.journal_lock_file().exists());

    // The lock is released after the write: the next one goes through.
    append(&home, &[approved("02")], NOW).unwrap();
    let (lines, corrupt) = read_recent(&home, NOW);
    assert_eq!((lines.len(), corrupt), (2, 0));
}

#[test]
fn several_entries_go_in_one_append() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let lock = JournalLock::acquire(&home).unwrap();
    lock.append(&[approved("01"), JournalEntry::RelayStopped], NOW)
        .unwrap();
    lock.append(&[], NOW).unwrap();
    drop(lock);
    let kinds: Vec<JournalKind> = read_recent(&home, NOW)
        .0
        .iter()
        .map(|l| l.entry.kind())
        .collect();
    assert_eq!(
        kinds,
        [JournalKind::CatalogueApproved, JournalKind::RelayStopped]
    );
}

#[test]
fn lock_held_past_2_s_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let _held = hold_lock(&home);
    let started = Instant::now();
    let result = append(&home, &[approved("01")], NOW);
    let waited = started.elapsed();
    assert!(
        matches!(result, Err(JournalError::LockTimeout)),
        "{result:?}"
    );
    assert!(waited >= LOCK_TIMEOUT, "{waited:?}");
    assert!(waited < LOCK_TIMEOUT + Duration::from_secs(1), "{waited:?}");
    assert!(!day_file(&home, NOW).exists());
}

/// The spill files in `home`'s journal, by name.
fn spill_files(home: &RelayHome) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(home.journal_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| spilled_day_file(name).is_some())
        .collect();
    names.sort();
    names
}

fn hashes(lines: &[JournalLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| match &l.entry {
            JournalEntry::CatalogueApproved(a) => a.hash.clone(),
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn record_appends_to_the_day_file_when_the_lock_is_free() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    record(&home, &[approved("01")], NOW).unwrap();
    record(&home, &[], NOW).unwrap();
    assert!(day_file(&home, NOW).exists());
    assert!(spill_files(&home).is_empty());
    assert_eq!(hashes(&read_recent(&home, NOW).0), ["01"]);
}

/// D-760: a line that records what already happened survives a lock held
/// past the timeout, in a spill file of its own that the reader reads with
/// its day, after the day file, in the order the spills were written.
#[test]
fn record_spills_when_the_lock_stays_held() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    append(&home, &[approved("locked")], NOW).unwrap();
    let held = hold_lock(&home);
    let started = Instant::now();
    record(
        &home,
        &[approved("spilled-1"), JournalEntry::RelayStopped],
        NOW,
    )
    .unwrap();
    assert!(
        started.elapsed() >= LOCK_TIMEOUT,
        "it waited for the lock first"
    );
    record(&home, &[approved("spilled-2")], NOW).unwrap();
    drop(held);

    let spills = spill_files(&home);
    assert_eq!(spills.len(), 2, "one file per write, same millisecond");
    let today = event_file_name(NOW);
    assert!(
        spills
            .iter()
            .all(|name| spilled_day_file(name).as_deref() == Some(today.as_str())),
        "{spills:?}"
    );
    let (lines, corrupt) = read_recent(&home, NOW);
    assert_eq!(corrupt, 0);
    let kinds: Vec<JournalKind> = lines.iter().map(|l| l.entry.kind()).collect();
    assert_eq!(
        kinds,
        [
            JournalKind::CatalogueApproved,
            JournalKind::CatalogueApproved,
            JournalKind::RelayStopped,
            JournalKind::CatalogueApproved,
        ]
    );
    assert!(lines.iter().all(|l| l.ts == NOW));
}

#[test]
fn reader_takes_each_days_spills_after_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let spill = |ts: i64, writer: &str, hash: &str| {
        fs::create_dir_all(home.journal_dir()).unwrap();
        fs::write(
            home.journal_dir().join(spill_file_name(ts, writer)),
            JournalLine::new(ts, approved(hash)).to_line(),
        )
        .unwrap();
    };
    spill(NOW - 2 * DAY, "1-1-0", "spill-two-days-ago");
    spill(NOW - DAY, "1-1-0", "spill-yesterday");
    append(&home, &[approved("yesterday")], NOW - DAY).unwrap();
    spill(NOW, "2-1-0", "spill-today-second");
    spill(NOW, "1-1-0", "spill-today-first");
    append(&home, &[approved("today")], NOW).unwrap();
    // A spill file cut by a killed writer: its partial line is skipped,
    // as in a day file.
    fs::write(
        home.journal_dir().join(spill_file_name(NOW, "3-1-0")),
        b"{\"v\":1,\"ts\":5,\"kind\":\"catalogue_appr",
    )
    .unwrap();

    let (lines, corrupt) = read_recent(&home, NOW);
    assert_eq!(
        hashes(&lines),
        [
            "yesterday",
            "spill-yesterday",
            "today",
            "spill-today-first",
            "spill-today-second"
        ]
    );
    assert_eq!(corrupt, 0);
}

#[test]
fn reader_spans_two_days_and_counts_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    append(&home, &[approved("two-days-ago")], NOW - 2 * DAY).unwrap();
    append(&home, &[approved("yesterday")], NOW - DAY).unwrap();
    append(&home, &[approved("today")], NOW).unwrap();
    let mut today = fs::read(day_file(&home, NOW)).unwrap();
    today.extend_from_slice(b"garbage\n{\"v\":1,\"ts\":1,\"kind\":\"refused\"}\n");
    today.extend_from_slice(b"{\"v\":1,\"bad\":\"\xff\"}\n");
    today.extend_from_slice(b"{\"v\":1,\"ts\":5,\"kind\":\"catalogue_appr");
    fs::write(day_file(&home, NOW), today).unwrap();

    let (lines, corrupt) = read_recent(&home, NOW);
    let hashes: Vec<String> = lines
        .iter()
        .map(|l| match &l.entry {
            JournalEntry::CatalogueApproved(a) => a.hash.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(hashes, ["yesterday", "today"]);
    assert_eq!(
        corrupt, 3,
        "garbage, a refused line without its fields, invalid UTF-8; the partial tail is not counted"
    );
    assert_eq!(
        read_recent(&RelayHome::new(dir.path().join("none"), None), NOW),
        (Vec::new(), 0)
    );
}

/// A day file of the journal dated `ts`, written directly.
fn seed_day(home: &RelayHome, ts: i64) -> std::path::PathBuf {
    fs::create_dir_all(home.journal_dir()).unwrap();
    let path = day_file(home, ts);
    fs::write(&path, JournalLine::new(ts, approved("seeded")).to_line()).unwrap();
    path
}

#[test]
fn retention_keeps_90_days() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let kept: Vec<_> = [0, 1, 89, JOURNAL_RETENTION_DAYS]
        .iter()
        .map(|days| seed_day(&home, NOW - days * DAY))
        .collect();
    let gone: Vec<_> = [JOURNAL_RETENTION_DAYS + 1, 400]
        .iter()
        .map(|days| seed_day(&home, NOW - days * DAY))
        .collect();

    let spill = |days: i64| {
        let ts = NOW - days * DAY;
        let path = home.journal_dir().join(spill_file_name(ts, "1-1-0"));
        fs::write(&path, JournalLine::new(ts, approved("spilled")).to_line()).unwrap();
        path
    };
    let kept: Vec<_> = kept
        .into_iter()
        .chain([spill(JOURNAL_RETENTION_DAYS)])
        .collect();
    let gone: Vec<_> = gone
        .into_iter()
        .chain([spill(JOURNAL_RETENTION_DAYS + 1)])
        .collect();

    assert_eq!(prune(&home, NOW), gone.len());
    for path in &kept {
        assert!(path.exists(), "{} must stay", path.display());
    }
    for path in &gone {
        assert!(!path.exists(), "{} must go", path.display());
    }
    assert_eq!(prune(&home, NOW), 0, "a second pass finds nothing");
    assert_eq!(
        prune(&RelayHome::new(dir.path().join("none"), None), NOW),
        0
    );
}

#[test]
fn retention_leaves_the_lock_and_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let held = hold_lock(&home);
    let old = NOW - 2 * JOURNAL_RETENTION_DAYS * DAY;
    let old_name = event_file_name(old);
    let others = [
        "notes.txt".to_owned(),
        "2026-13-01.jsonl".to_owned(),
        format!("{old_name}.bak"),
        old_name.replace(".jsonl", ".json"),
    ];
    for name in &others {
        fs::write(home.journal_dir().join(name), b"x").unwrap();
    }
    // A folder named like an old day file is not a day file.
    let folder = home.journal_dir().join(&old_name);
    fs::create_dir_all(&folder).unwrap();

    assert_eq!(prune(&home, NOW), 0);
    assert!(home.journal_lock_file().exists());
    for name in &others {
        assert!(home.journal_dir().join(name).exists(), "{name}");
    }
    assert!(folder.is_dir());
    // The lock still works after the pass.
    held.unlock().unwrap();
    append(&home, &[approved("after")], NOW).unwrap();
}

#[test]
fn unknown_kind_is_not_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    append(&home, &[approved("01")], NOW).unwrap();
    let mut today = fs::read_to_string(day_file(&home, NOW)).unwrap();
    today.push_str("{\"v\":1,\"ts\":2,\"kind\":\"relay_paused\",\"by\":\"window\"}\n");
    today.push_str("{\"v\":2,\"ts\":3,\"kind\":\"run_started\",\"shape\":\"new\"}\n");
    fs::write(day_file(&home, NOW), today).unwrap();
    let (lines, corrupt) = read_recent(&home, NOW);
    assert_eq!((lines.len(), corrupt), (1, 0));
}

/// One `refused` line of `session`, for the meeting's read-back.
#[cfg(feature = "window-authority")]
fn refused_by(session: &str, action: &str) -> JournalEntry {
    JournalEntry::Refused(
        serde_json::from_value(serde_json::json!({
            "session_id": session, "action": action,
            "reason": officina_core::relay::journal::RefusalReason::ALL[0], "detail": "no",
        }))
        .unwrap(),
    )
}

/// D-169: the meeting reads the journal back to the session's start, past
/// the readers' two days, and no further than the journal's 90 days: a
/// session started ten days ago shows its refusal of nine days ago, which
/// `read_recent` does not reach, and one started 120 days ago shows
/// nothing older than 90 days.
#[cfg(feature = "window-authority")]
#[test]
fn a_meeting_reads_runs_back_to_the_session_start() {
    use officina_core::board::{BoardInput, build_board};
    use officina_core::meeting::MeetingCard;
    use officina_host::meeting::{MeetingGrant, gather};

    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let session = "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0";
    for (back, action) in [(9, "nine-days-ago"), (95, "past-retention"), (0, "today")] {
        let ts = NOW - back * DAY;
        fs::create_dir_all(home.journal_dir()).unwrap();
        fs::write(
            day_file(&home, ts),
            JournalLine::new(ts, refused_by(session, action)).to_line(),
        )
        .unwrap();
    }
    let (recent, _) = read_recent(&home, NOW);
    assert_eq!(recent.len(), 1, "the readers' two days hold today's only");
    let actions = |started_days_ago: i64| -> Vec<String> {
        let agents = format!(
            r#"[{{"pid":7,"sessionId":"{session}","kind":"interactive","status":"idle","startedAt":{}}}]"#,
            NOW - started_days_ago * DAY
        );
        let board = build_board(
            &BoardInput {
                agents: Some(officina_core::agents::parse_agents(&agents).unwrap()),
                ..Default::default()
            },
            NOW,
        );
        let grant = MeetingGrant {
            card: MeetingCard::from(&board.columns.your_turn[0]),
            agents_folder: None,
            approvals: Vec::new(),
            claude: None,
            config_dir: None,
            base_env: officina_host::spawn::BaseEnv::default(),
            relay: home.clone(),
            events_dir: dir.path().join("events"),
        };
        gather(&grant, NOW)
            .runs
            .iter()
            .map(|run| {
                serde_json::to_value(&run.action)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    };
    assert_eq!(actions(10), ["today", "nine-days-ago"]);
    assert_eq!(
        actions(120),
        ["today", "nine-days-ago"],
        "never past 90 days"
    );
    assert_eq!(actions(0), ["today"], "nothing before the session's start");
}
