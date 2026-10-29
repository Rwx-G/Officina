//! Opening the registry (story 3.1 AC 6 to 11; AD-38 to AD-42, D-47, D-82,
//! D-93, D-94, D-97): location, connection settings, readers beside and
//! after the writer, the version rule, integrity at open, copies.

#![expect(
    clippy::disallowed_methods,
    reason = "a test starts the crash helper example and kills it"
)]

mod common;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::registry::{DAY, Home, NOW, damage_page, example, file_contains};
use officina_core::registry::types::{RegistryState, RestoreNotice, SettingKey, Source};
use officina_host::registry::schema::{MIGRATIONS, SCHEMA_VERSION};
use officina_host::registry::store::{
    Integrity, Location, READER_PRAGMAS, SYNC_ROOT_MARKERS, WRITER_PRAGMAS, WriterOpen,
    daily_copies, daily_copy_name, open_reader, open_reader_expecting, open_writer,
    open_writer_with, quarantined, read_only_connection,
};

/// Writes one value under every setting key, as the crash helper does.
fn fill(home: &Home) {
    let mut writer = home.writer(NOW);
    for key in SettingKey::ALL {
        writer
            .set_setting(*key, &format!("\"{}\"", key.code()), Source::Rules, NOW)
            .unwrap();
    }
}

fn setting_keys(home: &Home) -> Vec<SettingKey> {
    open_reader(&home.home, &[])
        .unwrap()
        .settings()
        .unwrap()
        .into_iter()
        .map(|row| row.key)
        .collect()
}

fn sorted(mut keys: Vec<SettingKey>) -> Vec<SettingKey> {
    keys.sort_by_key(|key| key.code());
    keys
}

fn every_key() -> Vec<SettingKey> {
    sorted(SettingKey::ALL.to_vec())
}

#[test]
fn planted_marker_opens_and_names_the_folder() {
    for marker in SYNC_ROOT_MARKERS {
        let home = Home::new();
        std::fs::write(home.root.join(marker), b"").unwrap();
        let writer = home.writer(NOW);
        let expected = Location::Synced {
            exclude: home.paths().dir().to_path_buf(),
        };
        assert_eq!(writer.location(), &expected, "{marker}");
        drop(writer);
        assert_eq!(open_reader(&home.home, &[]).unwrap().location(), &expected);
    }
}

#[test]
fn network_location_is_not_opened() {
    let unc = PathBuf::from(r"\\localhost\officina-missing-share\home");
    assert!(matches!(open_writer(&unc, &[], NOW), WriterOpen::Network));
    assert_eq!(
        open_reader(&unc, &[]).unwrap_err().state,
        RegistryState::Network
    );
}

#[test]
fn writer_sets_every_pragma() {
    let home = Home::new();
    let writer = home.writer(NOW);
    for p in WRITER_PRAGMAS {
        assert_eq!(
            writer.pragma(p.name).as_deref(),
            Some(p.reads_back),
            "{}",
            p.name
        );
    }
    drop(writer);
    let again = home.writer(NOW);
    for p in WRITER_PRAGMAS {
        assert_eq!(
            again.pragma(p.name).as_deref(),
            Some(p.reads_back),
            "{} at a second open",
            p.name
        );
    }
}

/// Quality audit L7: the values AD-39 names, pinned from the addendum
/// rather than from the table that sets them, so editing a value and its
/// read-back together still fails here.
#[test]
fn the_pragmas_are_the_ones_ad_39_names() {
    let home = Home::new();
    let writer = home.writer(NOW);
    for (name, value) in [
        ("journal_mode", "wal"),
        ("synchronous", "2"),
        ("secure_delete", "1"),
        ("foreign_keys", "1"),
        ("trusted_schema", "0"),
        ("busy_timeout", "5000"),
    ] {
        assert_eq!(writer.pragma(name).as_deref(), Some(value), "writer {name}");
    }
    drop(writer);
    let reader = open_reader(&home.home, &[]).unwrap();
    for (name, value) in [("query_only", "1"), ("busy_timeout", "2000")] {
        assert_eq!(reader.pragma(name).as_deref(), Some(value), "reader {name}");
    }
}

#[test]
fn reader_is_read_only_and_query_only() {
    let home = Home::new();
    fill(&home);
    let reader = open_reader(&home.home, &[]).unwrap();
    for p in READER_PRAGMAS {
        assert_eq!(
            reader.pragma(p.name).as_deref(),
            Some(p.reads_back),
            "{}",
            p.name
        );
    }
    let conn = read_only_connection(&home.paths().database()).unwrap();
    assert!(conn.is_readonly(rusqlite::MAIN_DB).unwrap());
    let refused = conn.execute(
        "UPDATE setting SET value = 'x' WHERE key = ?1",
        [SettingKey::AutoArchive.code()],
    );
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(sorted(setting_keys(&home)), every_key());
}

#[test]
fn reader_reads_beside_the_writer() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    writer
        .set_setting(SettingKey::AutoArchive, "1", Source::Author, NOW)
        .unwrap();
    let reader = open_reader(&home.home, &[]).unwrap();
    assert_eq!(reader.settings().unwrap().len(), 1);
    writer
        .set_setting(SettingKey::TriageModel, "0", Source::Author, NOW)
        .unwrap();
    assert_eq!(
        reader.settings().unwrap().len(),
        2,
        "the reader sees each commit"
    );
}

#[test]
fn reader_reads_a_cleanly_closed_registry() {
    let home = Home::new();
    fill(&home);
    assert!(
        !home.paths().wal().exists(),
        "a clean close removes the WAL"
    );
    assert_eq!(sorted(setting_keys(&home)), every_key());
}

/// Starts the crash helper in `mode`, waits for its `ready` line, then
/// kills it.
fn killed_writer(home: &Home) {
    let mut child = Command::new(example("registry_crash_helper"))
        .args(["hold", &home.home.to_string_lossy(), &NOW.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "ready");
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn reader_reads_after_the_writer_was_killed() {
    let home = Home::new();
    killed_writer(&home);
    assert!(
        home.paths().wal().exists(),
        "the killed writer left its WAL"
    );
    let reader = open_reader(&home.home, &[]).unwrap();
    let rows = reader.settings().unwrap();
    assert_eq!(sorted(rows.iter().map(|r| r.key).collect()), every_key());
    for row in rows {
        assert!(row.value.contains(row.key.code()), "{row:?}");
    }
}

/// D-82: a reader may create `-wal` and `-shm`; the database file itself
/// is left byte for byte.
#[test]
fn reader_leaves_the_database_file_unchanged() {
    let closed = Home::new();
    fill(&closed);
    let killed = Home::new();
    killed_writer(&killed);
    for home in [closed, killed] {
        let db = home.paths().database();
        let before = std::fs::read(&db).unwrap();
        let reader = open_reader(&home.home, &[]).unwrap();
        assert_eq!(reader.settings().unwrap().len(), SettingKey::ALL.len());
        assert!(reader.quick_check().is_ok());
        drop(reader);
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
}

#[test]
fn reader_never_blocks_the_writer() {
    let home = Home::new();
    fill(&home);
    // A reader in the middle of a read transaction, the window closed.
    let reading = read_only_connection(&home.paths().database()).unwrap();
    reading
        .execute_batch("BEGIN; SELECT count(*) FROM setting;")
        .unwrap();
    let started = Instant::now();
    let mut writer = home.writer(NOW);
    writer
        .set_setting(SettingKey::AutoArchive, "2", Source::Author, NOW)
        .unwrap();
    // A writer blocked by the reader would wait its whole busy timeout
    // (AD-39) before failing; well under half of it, nothing waited.
    let busy_ms: u64 = WRITER_PRAGMAS
        .iter()
        .find(|p| p.name == "busy_timeout")
        .map(|p| p.set.parse().unwrap())
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(busy_ms / 2),
        "{:?}",
        started.elapsed()
    );
    drop(writer);
    reading.execute_batch("COMMIT;").unwrap();
}

#[test]
fn absent_registry_is_created_by_the_writer_only() {
    let home = Home::new();
    assert_eq!(
        open_reader(&home.home, &[]).unwrap_err().state,
        RegistryState::Absent
    );
    assert!(!home.paths().dir().exists(), "a reader creates nothing");
    match open_writer(&home.home, &[], NOW) {
        WriterOpen::Writable { writer, integrity } => {
            assert_eq!(integrity, Integrity::Created);
            assert_eq!(writer.user_version().unwrap(), SCHEMA_VERSION);
        }
        other => panic!("{other:?}"),
    }
    assert!(open_reader(&home.home, &[]).is_ok());
}

#[test]
fn older_version_is_copied_then_migrated() {
    let home = Home::new();
    fill(&home);
    let next: Vec<&str> = MIGRATIONS
        .iter()
        .copied()
        .chain(["CREATE TABLE added (x INTEGER) STRICT;"])
        .collect();
    let target = SCHEMA_VERSION + 1;
    assert_eq!(
        open_reader_expecting(&home.home, &[], target)
            .unwrap_err()
            .state,
        RegistryState::Older
    );
    match open_writer_with(&home.home, &[], &next, NOW) {
        WriterOpen::Writable {
            integrity:
                Integrity::Migrated {
                    from: SCHEMA_VERSION,
                    copy: Some(copy),
                },
            ..
        } => assert!(copy.is_file()),
        other => panic!("{other:?}"),
    }
    let reader = open_reader_expecting(&home.home, &[], target).unwrap();
    assert_eq!(reader.settings().unwrap().len(), SettingKey::ALL.len());
}

/// Sets the database's `user_version` past this build's.
fn written_by_a_newer_officina(home: &Home) {
    fill(home);
    let conn = rusqlite::Connection::open(home.paths().database()).unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
}

#[test]
fn newer_version_opens_read_only_for_the_writer() {
    let home = Home::new();
    written_by_a_newer_officina(&home);
    let db = home.paths().database();
    let before = std::fs::read(&db).unwrap();
    match open_writer(&home.home, &[], NOW) {
        WriterOpen::ReadOnly { reader } => {
            assert_eq!(reader.user_version().unwrap(), SCHEMA_VERSION + 1);
            assert_eq!(reader.settings().unwrap().len(), SettingKey::ALL.len());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert!(!home.paths().backup_dir().exists(), "no copy, no migration");
    assert!(quarantined(&home.paths()).is_empty());
}

/// D-47: each state a reader can find, built by an exhaustive match.
#[test]
fn reader_reports_each_state() {
    for state in RegistryState::ALL {
        let home = Home::new();
        let mut at = home.home.clone();
        let mut expected_version = SCHEMA_VERSION;
        match state {
            RegistryState::Ready => fill(&home),
            RegistryState::Absent => {}
            RegistryState::Older => {
                fill(&home);
                expected_version = SCHEMA_VERSION + 1;
            }
            RegistryState::Newer => written_by_a_newer_officina(&home),
            RegistryState::Unavailable => {
                std::fs::create_dir_all(home.paths().dir()).unwrap();
                std::fs::write(home.paths().database(), "not a database, ".repeat(400)).unwrap();
            }
            RegistryState::Network => {
                at = PathBuf::from(r"\\localhost\officina-missing-share\home")
            }
        }
        let found = match open_reader_expecting(&at, &[], expected_version) {
            Ok(_) => RegistryState::Ready,
            Err(failure) => {
                assert_ne!(failure.state, RegistryState::Ready);
                failure.state
            }
        };
        assert_eq!(found, *state);
    }
}

/// A registry holding every setting, with a valid daily copy of it taken
/// on `day_ms`'s day.
fn with_copy(home: &Home, day_ms: i64) -> PathBuf {
    fill(home);
    let mut writer = home.writer(day_ms);
    let copy = writer.copy_if_due(day_ms).unwrap().unwrap();
    drop(writer);
    copy
}

fn restored(opened: WriterOpen) -> (PathBuf, RestoreNotice) {
    match opened {
        WriterOpen::Writable {
            integrity:
                Integrity::Restored {
                    quarantined,
                    notice,
                },
            ..
        } => (quarantined, notice),
        other => panic!("{other:?}"),
    }
}

#[test]
fn damaged_page_is_quarantined_and_the_newest_valid_copy_restored() {
    let home = Home::new();
    let copy = with_copy(&home, NOW - DAY);
    let db = home.paths().database();
    damage_page(&db);
    let damaged = std::fs::read(&db).unwrap();
    let (quarantined_dir, notice) = restored(open_writer(&home.home, &[], NOW));
    assert_eq!(
        quarantined_dir,
        home.paths().quarantine_dir().join(NOW.to_string())
    );
    assert_eq!(
        std::fs::read(quarantined_dir.join("registry.db")).unwrap(),
        damaged,
        "set aside as it was"
    );
    assert_eq!(notice.copy, copy.file_name().unwrap().to_string_lossy());
    assert_eq!(sorted(setting_keys(&home)), every_key());
}

#[test]
fn damaged_newest_copy_falls_back_to_the_next() {
    let home = Home::new();
    let older = with_copy(&home, NOW - 2 * DAY);
    let mut writer = home.writer(NOW - DAY);
    let newer = writer.copy_if_due(NOW - DAY).unwrap().unwrap();
    drop(writer);
    damage_page(&newer);
    let at = |copy: &Path, ms: u64| {
        let time = std::time::UNIX_EPOCH + Duration::from_millis(ms);
        std::fs::File::options()
            .write(true)
            .open(copy)
            .unwrap()
            .set_modified(time)
            .unwrap();
    };
    at(&older, 1_000_000_000_000);
    at(&newer, 1_000_000_100_000);
    damage_page(&home.paths().database());
    let (_, notice) = restored(open_writer(&home.home, &[], NOW));
    assert_eq!(notice.copy, older.file_name().unwrap().to_string_lossy());
}

/// Penetration audit L5: restore follows the UTC day in the copies' names,
/// so an older copy whose modification time was moved ahead is not tried
/// first, and a file of another name is never restored.
#[test]
fn restore_follows_the_day_in_the_name_not_the_modification_time() {
    let home = Home::new();
    let older = with_copy(&home, NOW - 2 * DAY);
    let mut writer = home.writer(NOW - DAY);
    let newer = writer.copy_if_due(NOW - DAY).unwrap().unwrap();
    drop(writer);
    let planted = home.paths().backup_dir().join("planted.db");
    std::fs::copy(&older, &planted).unwrap();
    let ahead = std::time::SystemTime::now() + Duration::from_secs(365 * 86_400);
    for copy in [&older, &planted] {
        std::fs::File::options()
            .write(true)
            .open(copy)
            .unwrap()
            .set_modified(ahead)
            .unwrap();
    }
    damage_page(&home.paths().database());
    let (_, notice) = restored(open_writer(&home.home, &[], NOW));
    assert_eq!(notice.copy, newer.file_name().unwrap().to_string_lossy());
}

fn started_empty(opened: WriterOpen) -> PathBuf {
    match opened {
        WriterOpen::Writable {
            writer,
            integrity: Integrity::StartedEmpty { quarantined },
        } => {
            assert_eq!(writer.user_version().unwrap(), SCHEMA_VERSION);
            assert!(writer.settings().unwrap().is_empty());
            quarantined
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn not_a_database_is_quarantined() {
    let home = Home::new();
    std::fs::create_dir_all(home.paths().dir()).unwrap();
    let text = "this is no database at all\n".repeat(200);
    std::fs::write(home.paths().database(), &text).unwrap();
    let quarantined_dir = started_empty(open_writer(&home.home, &[], NOW));
    assert_eq!(
        std::fs::read_to_string(quarantined_dir.join("registry.db")).unwrap(),
        text
    );
}

#[test]
fn no_valid_copy_starts_empty_and_says_so() {
    let home = Home::new();
    let copy = with_copy(&home, NOW - DAY);
    damage_page(&copy);
    damage_page(&home.paths().database());
    started_empty(open_writer(&home.home, &[], NOW));
    assert!(copy.is_file(), "the damaged copy is left in place");
}

#[test]
fn restore_writes_the_notice() {
    let home = Home::new();
    let copy = with_copy(&home, NOW - DAY);
    damage_page(&home.paths().database());
    let (_, notice) = restored(open_writer(&home.home, &[], NOW));
    let expected = RestoreNotice {
        copy: copy.file_name().unwrap().to_string_lossy().into_owned(),
        restored_at_ms: NOW,
    };
    assert_eq!(notice, expected);
    let reader = open_reader(&home.home, &[]).unwrap();
    assert_eq!(reader.restore_notice(), Some(expected));
    let row = reader
        .settings()
        .unwrap()
        .into_iter()
        .find(|row| row.key == SettingKey::RestoreNotice)
        .unwrap();
    assert_eq!((row.src, row.at_ms), (Source::Rules, NOW));
}

/// D-97: a database another process holds with a share mode that refuses
/// the move cannot be set aside; nothing is deleted and the registry stays
/// unavailable.
#[cfg(windows)]
#[test]
fn held_file_leaves_the_registry_unavailable() {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_SHARE_READ | FILE_SHARE_WRITE, without FILE_SHARE_DELETE.
    const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    let home = Home::new();
    fill(&home);
    let db = home.paths().database();
    damage_page(&db);
    let damaged = std::fs::read(&db).unwrap();
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ_WRITE)
        .open(&db)
        .unwrap();
    match open_writer(&home.home, &[], NOW) {
        WriterOpen::Unavailable { error } => {
            assert!(!error.is_corruption(), "{error}");
            assert!(
                error.to_string().contains("could not be set aside"),
                "{error}"
            );
        }
        other => panic!("{other:?}"),
    }
    drop(holder);
    assert_eq!(std::fs::read(&db).unwrap(), damaged);
    assert!(quarantined(&home.paths()).is_empty(), "nothing set aside");
}

fn copy_names(home: &Home) -> Vec<String> {
    daily_copies(&home.paths())
        .into_iter()
        .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn daily_copy_is_due_by_the_date_in_its_name() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let morning = NOW - 11 * 3_600_000;
    let first = writer.copy_if_due(morning).unwrap().unwrap();
    assert_eq!(
        first.file_name().unwrap().to_string_lossy(),
        daily_copy_name(NOW)
    );
    // An old modification time does not make today's copy due again.
    std::fs::File::options()
        .write(true)
        .open(&first)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
    assert_eq!(writer.copy_if_due(NOW + 11 * 3_600_000).unwrap(), None);
    let next_day = writer.copy_if_due(NOW + 12 * 3_600_000).unwrap().unwrap();
    assert_eq!(
        next_day.file_name().unwrap().to_string_lossy(),
        daily_copy_name(NOW + DAY)
    );
    assert_eq!(
        copy_names(&home),
        [daily_copy_name(NOW + DAY), daily_copy_name(NOW)]
    );
}

#[test]
fn interrupted_copy_leaves_no_dated_file() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let backup = home.paths().backup_dir();
    let dated = backup.join(daily_copy_name(NOW));
    let tmp = backup.join(format!("{}.tmp", daily_copy_name(NOW)));

    // A copy that cannot complete: its temporary name is taken.
    std::fs::create_dir_all(&tmp).unwrap();
    assert!(writer.copy_if_due(NOW).is_err());
    assert!(!dated.exists());
    std::fs::remove_dir(&tmp).unwrap();

    // A copy killed before its rename left its temporary file behind.
    std::fs::write(&tmp, b"half a copy").unwrap();
    assert_eq!(writer.copy_if_due(NOW).unwrap(), Some(dated.clone()));
    assert!(!tmp.exists());
    assert!(officina_host::registry::store::copy_is_valid(&dated));
}

#[test]
fn a_damaged_copy_never_pushes_out_a_valid_one() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let oldest = writer.copy_if_due(NOW - 2 * DAY).unwrap().unwrap();
    let damaged = writer.copy_if_due(NOW - DAY).unwrap().unwrap();
    damage_page(&damaged);
    writer.copy_if_due(NOW).unwrap().unwrap();
    assert_eq!(
        copy_names(&home),
        [daily_copy_name(NOW), daily_copy_name(NOW - 2 * DAY)],
        "the two newest valid copies"
    );
    assert!(oldest.is_file() && !damaged.exists());
}

/// AD-39, AD-42: with `secure_delete`, a replaced text is zeroed in the
/// database, the truncated WAL holds none, and `VACUUM INTO` writes no free
/// page into the copy. The replacement is longer than the text it replaces,
/// so SQLite writes it elsewhere and only `secure_delete` clears the old
/// cell (checked: the test fails with it off).
#[test]
fn deleted_text_reaches_neither_the_file_nor_the_copy() {
    const MARKER: &str = "MARKER-a5c1-deleted-text";
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let paths = home.paths();
    writer
        .set_setting(
            SettingKey::TriageModelPause,
            &format!("\"{MARKER}\""),
            Source::Rules,
            NOW,
        )
        .unwrap();
    // Rows written after it keep its cell from being the page's last, which
    // SQLite would reuse whatever the setting.
    for key in SettingKey::ALL
        .iter()
        .filter(|key| **key != SettingKey::TriageModelPause)
    {
        writer
            .set_setting(*key, "\"kept\"", Source::Rules, NOW)
            .unwrap();
    }
    assert!(writer.checkpoint().unwrap());
    assert!(
        file_contains(&paths.database(), MARKER),
        "positive control: the text reached the file"
    );
    let longer = format!("\"{}\"", "x".repeat(4 * MARKER.len()));
    writer
        .set_setting(SettingKey::TriageModelPause, &longer, Source::Rules, NOW)
        .unwrap();
    assert!(writer.checkpoint().unwrap());
    let copy = writer.copy_if_due(NOW).unwrap().unwrap();
    for file in [paths.database(), paths.wal(), copy] {
        assert!(!file_contains(&file, MARKER), "{}", file.display());
    }
    assert_eq!(std::fs::metadata(paths.wal()).unwrap().len(), 0);
}

/// Adversarial pass: a kill between the quarantine's moves leaves the
/// database's log without it. A new database must not start beside that
/// log, which SQLite would replay into it: the log is set aside too.
#[test]
fn a_log_left_without_its_database_is_set_aside() {
    let home = Home::new();
    killed_writer(&home);
    let paths = home.paths();
    let interrupted = paths.quarantine_dir().join((NOW - 1).to_string());
    std::fs::create_dir_all(&interrupted).unwrap();
    std::fs::rename(paths.database(), interrupted.join("registry.db")).unwrap();
    assert!(paths.wal().exists());
    let quarantined_dir = started_empty(open_writer(&home.home, &[], NOW));
    assert!(
        quarantined_dir.join("registry.db-wal").is_file(),
        "the log is kept, set aside"
    );
    assert!(!paths.wal().exists() || std::fs::metadata(paths.wal()).unwrap().len() == 0);
    let reader = open_reader(&home.home, &[]).unwrap();
    assert!(
        reader.settings().unwrap().is_empty(),
        "nothing replayed from the old log"
    );
    assert_eq!(reader.quick_check().unwrap(), ["ok"]);
}

/// Adversarial pass: a kill after the damaged database was set aside and
/// before the copy took its place leaves no database; the next open
/// finishes the restore rather than starting empty.
#[test]
fn an_interrupted_restore_is_finished_at_the_next_open() {
    let home = Home::new();
    let copy = with_copy(&home, NOW - DAY);
    let paths = home.paths();
    let set_aside = paths.quarantine_dir().join((NOW - 1).to_string());
    std::fs::create_dir_all(&set_aside).unwrap();
    std::fs::rename(paths.database(), set_aside.join("registry.db")).unwrap();
    std::fs::write(
        paths.database().with_extension("db.restore"),
        b"half a copy",
    )
    .unwrap();
    let (quarantined_dir, notice) = restored(open_writer(&home.home, &[], NOW));
    assert_eq!(quarantined_dir, set_aside);
    assert_eq!(notice.copy, copy.file_name().unwrap().to_string_lossy());
    assert!(!paths.database().with_extension("db.restore").exists());
    assert_eq!(sorted(setting_keys(&home)), every_key());
}

/// A first start, with no quarantine, creates the registry and restores
/// nothing, whatever copies lie around.
#[test]
fn a_first_start_restores_nothing() {
    let home = Home::new();
    with_copy(&home, NOW - DAY);
    std::fs::remove_file(home.paths().database()).unwrap();
    match open_writer(&home.home, &[], NOW) {
        WriterOpen::Writable { integrity, .. } => assert_eq!(integrity, Integrity::Created),
        other => panic!("{other:?}"),
    }
}
