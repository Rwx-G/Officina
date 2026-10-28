//! A writer killed while ingesting (story 3.2 AC 11; AD-58's crash row, PRD
//! section 7 Memory, D-89): killed inside a chunk's transaction at a chosen
//! line, then at random points, the registry ends equal to one an
//! uninterrupted ingestion of the same files built, passes `quick_check`,
//! and every offset stands at a line end.
//!
//! Ignored by default: CI runs it in release, alone, after building the
//! helper (`cargo build --release -p officina-host --features
//! window-authority --example registry_crash_helper`). The random kill
//! points come from a seed printed first; `OFFICINA_CRASH_SEED` replays one.

mod common;

use std::io::{BufRead, BufReader};

use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::registry::{DAY, Home, NOW, answer, day_file, example, line, prompt};
use officina_core::dates::{day_start_ms, utc_date};
use officina_core::registry::ingestion::CHUNK_MAX_LINES;
use officina_host::registry::store::{open_reader, quick_check, read_only_connection};

/// Lines per day's file: several chunks each, so a random kill lands in a
/// transaction more often than between two.
const LINES_PER_DAY: usize = 50_000;
const KILLS: usize = 20;

/// A small, seeded generator: the kill points only need to vary and be
/// replayable.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Four days of event files: sessions seen again and again, prompts and
/// answers out of time order, corrupt lines, an unfinished last line in the
/// oldest file (one corrupt line) and in yesterday's (left unread).
fn write_events(home: &Home) {
    for back in (0..4).rev() {
        let start = day_start_ms(utc_date(NOW - back * DAY));
        let lines: Vec<String> = (0..LINES_PER_DAY)
            .map(|i| {
                let session = format!("s{:03}", (i * 7 + back as usize * 13) % 400);
                let cwd = format!("C:/work/project-{}", i % 5);
                let ts = start + ((i * 7919) % LINES_PER_DAY) as i64 * 1_000;
                match i % 5 {
                    _ if i % 997 == 0 => "{\"v\":1,\"broken\n".to_owned(),
                    0 => prompt(&session, ts, &cwd, &format!("prompt {i} of day {back}")),
                    1 => answer(
                        &session,
                        ts,
                        &cwd,
                        &format!("answer {i}"),
                        Some(&format!("…end {i}")),
                    ),
                    2 => line("SessionStart", &session, ts, &cwd),
                    3 => line("Notification", &session, ts, &cwd),
                    _ => line("Stop", &session, ts, &cwd),
                }
            })
            .collect();
        let name = day_file(start);
        home.append(&name, &lines);
        if back == 3 || back == 1 {
            home.append(&name, &["{\"v\":1,\"ts\":5,\"ev".to_owned()]);
        }
    }
}

/// Copies `from`'s event files into `to`'s home.
fn copy_events(from: &Home, to: &Home) {
    std::fs::create_dir_all(&to.events).unwrap();
    for entry in std::fs::read_dir(&from.events).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), to.events.join(entry.file_name())).unwrap();
    }
}

fn helper(home: &Home, stall_at: Option<usize>) -> (Child, BufReader<std::process::ChildStdout>) {
    let mut cmd = Command::new(example("registry_crash_helper"));
    cmd.args(["ingest", &home.home.to_string_lossy(), &NOW.to_string()]);
    if let Some(line) = stall_at {
        cmd.arg(line.to_string());
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    (child, out)
}

fn expect_word(out: &mut BufReader<std::process::ChildStdout>, word: &str) {
    let mut said = String::new();
    out.read_line(&mut said).unwrap();
    assert_eq!(said.trim(), word, "the helper did not say {word}");
}

/// Everything ingestion writes, read back for comparison.
fn contents(home: &Home) -> String {
    let reader = open_reader(&home.home, &[]).unwrap();
    format!(
        "{:#?}\n{:#?}\n{:#?}",
        reader.event_files().unwrap(),
        reader.sessions().unwrap(),
        reader.excerpts().unwrap()
    )
}

/// Each offset stands at a line end, or at the end of a file whose
/// unfinished line was counted corrupt.
fn offsets_at_line_ends(home: &Home) {
    let reader = open_reader(&home.home, &[]).unwrap();
    for row in reader.event_files().unwrap() {
        let bytes = std::fs::read(home.events.join(&row.name)).unwrap();
        let at = usize::try_from(row.ingested_bytes).unwrap();
        assert!(at <= bytes.len(), "{}: offset past the end", row.name);
        assert!(
            at == 0 || bytes[at - 1] == b'\n' || at == bytes.len(),
            "{}: offset {at} inside a line",
            row.name
        );
    }
}

fn seed() -> u64 {
    std::env::var("OFFICINA_CRASH_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64)
                | 1
        })
}

#[test]
#[ignore = "release build, run alone by CI's crash step"]
fn killed_mid_transaction_and_at_random_loses_no_event() {
    let seed = seed();
    println!("seed {seed} (replay with OFFICINA_CRASH_SEED={seed})");
    let mut random = XorShift(seed);

    let crashed = Home::new();
    write_events(&crashed);
    let reference = Home::new();
    copy_events(&crashed, &reference);
    let started = std::time::Instant::now();
    let mut writer = reference.writer(NOW);
    writer.ingest(&reference.ctx(), NOW).unwrap();
    drop(writer);
    let uninterrupted = started.elapsed();

    // Killed inside a chunk's transaction, a few lines into the second one.
    let (mut child, mut out) = helper(&crashed, Some(CHUNK_MAX_LINES + 37));
    expect_word(&mut out, "started");
    expect_word(&mut out, "stalled");
    child.kill().unwrap();
    child.wait().unwrap();
    let after_stall = open_reader(&crashed.home, &[])
        .unwrap()
        .event_files()
        .unwrap();
    assert_eq!(after_stall.len(), 1, "only the first chunk committed");
    assert!(after_stall[0].ingested_bytes > 0);

    // Killed at random points, each run resuming from the offsets.
    let total: u64 = std::fs::read_dir(&crashed.events)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    // Each run reads its files and fills a chunk before its first commit;
    // a window of an eighth of a pass lets most runs commit a few chunks,
    // and twenty of them end within the pass rather than after it.
    let window = (u64::try_from(uninterrupted.as_millis()).unwrap_or(1000) / 8).max(5);
    println!("uninterrupted pass {uninterrupted:?}, kills within {window} ms of each start");
    for kill in 1..=KILLS {
        let (mut child, mut out) = helper(&crashed, None);
        expect_word(&mut out, "started");
        std::thread::sleep(Duration::from_millis(random.next() % window));
        let _ = child.kill();
        child.wait().unwrap();
        let read: u64 = open_reader(&crashed.home, &[])
            .unwrap()
            .event_files()
            .unwrap()
            .iter()
            .map(|row| row.ingested_bytes)
            .sum();
        println!("kill {kill}: {read} of {total} bytes ingested");
    }

    // The window's next start finishes the pass.
    let mut writer = crashed.writer(NOW);
    writer.ingest(&crashed.ctx(), NOW).unwrap();
    drop(writer);

    let conn = read_only_connection(&crashed.paths().database()).unwrap();
    assert_eq!(quick_check(&conn).unwrap(), ["ok"]);
    drop(conn);
    offsets_at_line_ends(&crashed);
    assert_eq!(contents(&crashed), contents(&reference), "seed {seed}");
}

/// The comparison can fail (D-89): a registry whose offsets are right but
/// that lost what one line applied differs from the reference.
#[test]
#[ignore = "release build, run alone by CI's crash step"]
fn a_missing_event_is_seen_by_the_comparison() {
    let full = Home::new();
    write_events(&full);
    let lossy = Home::new();
    copy_events(&full, &lossy);
    for home in [&full, &lossy] {
        let mut writer = home.writer(NOW);
        writer.ingest(&home.ctx(), NOW).unwrap();
    }
    assert_eq!(contents(&full), contents(&lossy));
    let conn = rusqlite::Connection::open(lossy.paths().database()).unwrap();
    conn.execute(
        "UPDATE session_excerpt SET prompt = NULL, prompt_at_ms = NULL WHERE session_id = 's005'",
        [],
    )
    .unwrap();
    drop(conn);
    assert_ne!(contents(&full), contents(&lossy));
}
