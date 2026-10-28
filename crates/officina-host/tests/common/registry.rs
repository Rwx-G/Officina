//! What the registry tests share: a writer on a temporary home, a damaged
//! page, the helper processes, byte searches.

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use officina_host::registry::RegistryPaths;
use officina_host::registry::store::{Writer, WriterOpen, open_writer};

/// 2026-10-27T12:00:00Z.
pub const NOW: i64 = 1_793_102_400_000;
pub const DAY: i64 = 86_400_000;

/// A temporary folder holding an Officina home, kept while the value lives.
pub struct Home {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    /// The event files' folder.
    pub events: PathBuf,
}

impl Home {
    pub fn new() -> Home {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let home = root.join("officina-home");
        let events = home.join("events");
        Home {
            _dir: dir,
            root,
            home,
            events,
        }
    }

    pub fn paths(&self) -> RegistryPaths {
        RegistryPaths::new(&self.home)
    }

    /// The registry opened for writing at `now`; panics on any other outcome.
    pub fn writer(&self, now: i64) -> Writer {
        writable(open_writer(&self.home, &[], now))
    }
}

/// The writer of an outcome that must be writable.
pub fn writable(opened: WriterOpen) -> Writer {
    match opened {
        WriterOpen::Writable { writer, .. } => writer,
        other => panic!("the registry did not open writable: {other:?}"),
    }
}

/// The page size of the database file `db`, from its header.
fn page_size(bytes: &[u8]) -> u64 {
    match u16::from_be_bytes([bytes[16], bytes[17]]) {
        1 => 65_536,
        size => u64::from(size),
    }
}

/// Overwrites page 2 of `db`, a table's root, with bytes that are no
/// b-tree page: `quick_check` reports it (verified in the addendum).
pub fn damage_page(db: &Path) {
    let bytes = std::fs::read(db).unwrap();
    let size = page_size(&bytes);
    assert!(
        bytes.len() as u64 >= 2 * size,
        "{} has one page",
        db.display()
    );
    let mut file = std::fs::OpenOptions::new().write(true).open(db).unwrap();
    file.seek(SeekFrom::Start(size)).unwrap();
    file.write_all(&vec![0xFF; usize::try_from(size).unwrap()])
        .unwrap();
}

/// True when `needle` occurs in the file at `path`; false when it is absent.
pub fn file_contains(path: &Path, needle: &str) -> bool {
    std::fs::read(path)
        .unwrap_or_default()
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

impl Home {
    /// Appends `lines` to the event file `name`, creating it.
    pub fn append(&self, name: &str, lines: &[String]) {
        std::fs::create_dir_all(&self.events).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.events.join(name))
            .unwrap();
        for line in lines {
            file.write_all(line.as_bytes()).unwrap();
        }
    }

    /// An ingestion context on this home, no floor, excerpts kept.
    pub fn ctx(&self) -> officina_host::registry::ingest::IngestContext<'_> {
        officina_host::registry::ingest::IngestContext {
            events_dir: &self.events,
            officina_home: &self.home,
            projects: &[],
            no_excerpts: false,
        }
    }
}

/// One event line, newline included, as the hook writes it.
pub fn line(event: &str, session: &str, ts: i64, cwd: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({"v": 1, "ts": ts, "event": event, "session_id": session, "cwd": cwd})
    )
}

/// A `UserPromptSubmit` line carrying `prompt`.
pub fn prompt(session: &str, ts: i64, cwd: &str, prompt: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({"v": 1, "ts": ts, "event": "UserPromptSubmit", "session_id": session, "cwd": cwd, "prompt": prompt})
    )
}

/// A `Stop` line carrying an answer's head and, when given, its tail.
pub fn answer(session: &str, ts: i64, cwd: &str, head: &str, tail: Option<&str>) -> String {
    let mut value = serde_json::json!({"v": 1, "ts": ts, "event": "Stop", "session_id": session, "cwd": cwd, "last_message": head});
    if let Some(tail) = tail {
        value["last_message_tail"] = tail.into();
    }
    format!("{value}\n")
}

/// The event file name of the UTC day `ms` falls in.
pub fn day_file(ms: i64) -> String {
    officina_core::dates::event_file_name(ms)
}

/// The example `name`, which `cargo test` builds next to the test
/// executables: `target/<profile>/examples/`.
pub fn example(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile = exe.parent().and_then(Path::parent).unwrap();
    let path = profile
        .join("examples")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} is missing: build it with `cargo build --features window-authority --example {name}`",
        path.display()
    );
    path
}
