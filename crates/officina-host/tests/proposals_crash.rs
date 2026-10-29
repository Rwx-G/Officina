//! Restart without loss (story 5.17 AC 7; AD-95's restart row, PRD
//! section 7 Restart row, FR-14, D-89, D-144, D-165): `officina mcp`'s
//! writer killed between a proposal's temporary file and its rename, then
//! at random points, and the window's ingestion run twice and again after
//! a restart: every proposal fully written is stored exactly once, no
//! temporary file is read, the statuses stay as they were, and the cursor
//! `officina mcp` reads after a new orchestrator start is the previous one.
//!
//! Ignored by default: CI runs it in release with the other crash tests,
//! after building the helper (`cargo build --release -p officina-host
//! --example mcp_crash_helper`). The random kill points come from a seed
//! printed first; `OFFICINA_CRASH_SEED` replays one.

#![expect(
    clippy::disallowed_methods,
    reason = "the tests start the crash helper example and kill it"
)]

mod common;

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::registry::{DAY, Home, example};
use officina_core::orders::proposal::{PROPOSAL_HOUR_MS, id_of_file, is_temporary_file};
use officina_core::registry::types::{OrchestratorStartOutcome, ProposalStatus};
use officina_host::registry::orders::OrchestratorStartRecord;
use officina_host::registry::store::{Writer, open_reader, open_writer};

const KILLS: usize = 20;
/// Proposals per run of the helper.
const PER_RUN: i64 = 20;
/// Between two proposals of a run: well under the hourly limit.
const STEP_MS: i64 = PROPOSAL_HOUR_MS / 20;

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

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// A run of the helper writing [`PER_RUN`] proposals into `dir` from
/// `first`, logging each renamed id into `log`, stalling at `stall`.
fn helper(
    dir: &Path,
    log: &Path,
    first: i64,
    stall: Option<i64>,
) -> (Child, BufReader<std::process::ChildStdout>) {
    let mut cmd = Command::new(example("mcp_crash_helper"));
    cmd.arg(dir)
        .arg(log)
        .args([PER_RUN.to_string(), first.to_string(), STEP_MS.to_string()]);
    if let Some(stall) = stall {
        cmd.arg(stall.to_string());
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

/// The ids the helper logged as renamed.
fn logged(log: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The proposal files of `dir` by id, and the ids its temporary files
/// would have had.
fn on_disk(dir: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut complete = BTreeSet::new();
    let mut temporary = BTreeSet::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_temporary_file(&name) {
            let id = name.split(".json").next().unwrap().to_owned();
            temporary.insert(id);
        } else if let Some(id) = id_of_file(&name) {
            complete.insert(id.to_owned());
        }
    }
    (complete, temporary)
}

/// Every stored proposal, by id, with its status.
fn stored(home: &Home) -> Vec<(String, ProposalStatus)> {
    open_reader(&home.home, &[])
        .unwrap()
        .proposals()
        .unwrap()
        .into_iter()
        .map(|p| (p.id, p.status))
        .collect()
}

fn ingest(writer: &mut Writer, dir: &Path, now: i64) {
    writer.ingest_proposals(dir, None, now).unwrap();
}

#[test]
#[ignore = "release build, run alone by CI's crash step"]
fn killed_writers_lose_and_duplicate_nothing() {
    let seed = seed();
    println!("seed {seed} (replay with OFFICINA_CRASH_SEED={seed})");
    let mut random = XorShift(seed);
    let home = Home::new();
    let dir: PathBuf = home.home.join("orchestrator").join("proposals");
    std::fs::create_dir_all(&dir).unwrap();
    let log = home.root.join("renamed.log");
    // Every run writes in a stretch of its own, ten days back and on.
    let base = wall_ms() - 10 * DAY;
    let stretch = |run: i64| base + run * PER_RUN * STEP_MS;

    // An uninterrupted run, timed, in a folder of its own.
    let timing = home.root.join("timing");
    std::fs::create_dir_all(&timing).unwrap();
    let started = Instant::now();
    let (mut child, _) = helper(&timing, &home.root.join("timing.log"), base, None);
    assert!(child.wait().unwrap().success());
    let uninterrupted = started.elapsed();

    // Killed between a temporary file and its rename.
    let (mut child, mut out) = helper(&dir, &log, stretch(0), Some(3));
    let mut said = String::new();
    out.read_line(&mut said).unwrap();
    assert!(said.starts_with("stalled "), "the helper said {said:?}");
    child.kill().unwrap();
    child.wait().unwrap();
    let (complete, temporary) = on_disk(&dir);
    assert_eq!(complete.len(), 3, "three renamed before the stall");
    assert_eq!(temporary.len(), 1, "the stalled one's temporary file");

    // Killed at random points.
    let window = u64::try_from(uninterrupted.as_millis())
        .unwrap_or(100)
        .max(5);
    println!("uninterrupted run {uninterrupted:?}, kills within {window} ms of each start");
    for kill in 1..=KILLS as i64 {
        let (mut child, _) = helper(&dir, &log, stretch(kill), None);
        std::thread::sleep(Duration::from_millis(random.next() % window));
        let _ = child.kill();
        child.wait().unwrap();
    }

    let (complete, temporary) = on_disk(&dir);
    let renamed = logged(&log);
    println!(
        "{} renamed and logged, {} complete files, {} temporary files",
        renamed.len(),
        complete.len(),
        temporary.len()
    );
    assert!(
        renamed.is_subset(&complete),
        "a proposal logged as renamed is missing before ingestion"
    );
    let now = wall_ms();
    let mut writer = home.writer(now);
    ingest(&mut writer, &dir, now);
    let first = stored(&home);
    let ids: Vec<&String> = first.iter().map(|(id, _)| id).collect();
    let unique: BTreeSet<&String> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "a proposal stored twice");
    let stored_ids: BTreeSet<String> = unique.into_iter().cloned().collect();
    assert_eq!(
        stored_ids, complete,
        "every complete file, and only them, stored once"
    );
    assert!(stored_ids.is_disjoint(&temporary.difference(&complete).cloned().collect()));
    // The author decides one; the statuses must survive what follows.
    let decided = first[0].0.clone();
    assert!(writer.dismiss_proposal(&decided, now).unwrap());
    let decided_statuses = stored(&home);

    ingest(&mut writer, &dir, now + 1_000);
    assert_eq!(
        stored(&home),
        decided_statuses,
        "a second pass changes nothing"
    );
    drop(writer);
    // The window restarts.
    let mut restarted = open_writer(&home.home, &[], now + 2_000).writer().unwrap();
    ingest(&mut restarted, &dir, now + 2_000);
    assert_eq!(stored(&home), decided_statuses, "a restart changes nothing");
    let (left, _) = on_disk(&dir);
    assert!(
        left.is_empty(),
        "every complete file deleted after its commit"
    );
}

#[test]
#[ignore = "release build, run alone by CI's crash step"]
fn the_cursor_after_restart_is_the_previous_start() {
    let home = Home::new();
    let now = wall_ms();
    let start = |writer: &mut Writer, at: i64, outcome: OrchestratorStartOutcome| {
        let spawned = outcome == OrchestratorStartOutcome::Spawned;
        writer
            .record_orchestrator_start(&OrchestratorStartRecord {
                started_at_ms: at,
                outcome,
                pid: spawned.then_some(7),
                pid_created_at_ms: spawned.then_some(at),
                error: (!spawned).then_some("workdir_not_empty"),
            })
            .unwrap();
    };
    let mut writer = home.writer(now);
    start(&mut writer, now - 3_000, OrchestratorStartOutcome::Spawned);
    start(&mut writer, now - 2_000, OrchestratorStartOutcome::Refused);
    drop(writer);
    // The window restarts, and the author starts the orchestrator again.
    let mut restarted = open_writer(&home.home, &[], now).writer().unwrap();
    start(&mut restarted, now, OrchestratorStartOutcome::Spawned);
    drop(restarted);
    let cursor = open_reader(&home.home, &[])
        .unwrap()
        .previous_orchestrator_start()
        .unwrap();
    assert_eq!(cursor, Some(now - 3_000), "the previous spawned start");
}
