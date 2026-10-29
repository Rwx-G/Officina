//! File primitives (AD-9, AD-10): single-write appends, atomic replaces, the
//! event buffer reader. Event files are deleted by the window's hourly pass
//! only, once the registry holds them (AD-44).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use officina_core::dates::{event_file_name, event_files_to_read};
use officina_core::events::{EventLine, ParsedLine, parse_line};

/// Appends `line` to `path` with one `write_all` on a file opened in append
/// mode. On Windows, Rust opens append-mode files with `FILE_APPEND_DATA`
/// only, so each write lands at the current end of file whatever other
/// writers do; one write per line is what keeps concurrent hooks from
/// interleaving (stress-tested, AD-10). Creates the directory on first use.
///
/// A writer killed mid-line leaves the file without a final newline; the next
/// line then starts with one, so it is not glued to the partial line and lost
/// with it. Two writers doing this at once only produce an empty line.
pub fn append_line(path: &Path, line: &str) -> io::Result<()> {
    let open = || OpenOptions::new().append(true).create(true).open(path);
    let mut file = match open() {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            open()?
        }
        Err(e) => return Err(e),
    };
    if ends_without_newline(path) {
        file.write_all(format!("\n{line}").as_bytes())
    } else {
        file.write_all(line.as_bytes())
    }
}

fn ends_without_newline(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut last = [0u8; 1];
    file.seek(SeekFrom::End(-1)).is_ok() && file.read_exact(&mut last).is_ok() && last[0] != b'\n'
}

/// Replaces `path` with `content` atomically: temp file in the same directory,
/// flushed, then renamed over the target. A third-party handle (antivirus,
/// indexer) can make the rename fail transiently, hence three retries.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(dir)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let tmp = dir.join(format!(
        "{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content)?;
        file.sync_all()?;
    }
    let mut last_error = None;
    for attempt in 0..4 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(e),
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(last_error.unwrap_or_else(|| io::Error::other("rename failed")))
}

/// True when `dir` holds anything; a folder that cannot be listed counts
/// as holding something, so nothing starts in it (AD-89's workdir).
pub fn holds_anything(dir: &Path) -> bool {
    fs::read_dir(dir).map_or(true, |mut entries| entries.next().is_some())
}

/// Path of the event file a hook running at `now_ms` appends to.
pub fn event_file_for(events_dir: &Path, now_ms: i64) -> std::path::PathBuf {
    events_dir.join(event_file_name(now_ms))
}

/// Events of the last two UTC days, plus the number of lines that could not
/// be parsed. A partial last line (hook killed mid-write) and lines of another
/// format version are not corrupt; anything else that fails to parse is,
/// including a line that is not valid UTF-8. Lines are decoded one by one, so
/// a bad byte costs one line, never the whole day.
pub fn read_recent_events(events_dir: &Path, now_ms: i64) -> (Vec<EventLine>, u32) {
    let mut events = Vec::new();
    let mut corrupt = 0u32;
    for name in event_files_to_read(now_ms) {
        let bytes = match fs::read(events_dir.join(&name)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                corrupt += 1;
                continue;
            }
        };
        let ends_with_newline = bytes.ends_with(b"\n");
        let raw_lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
        let last_index = raw_lines.len().saturating_sub(1);
        for (index, raw) in raw_lines.iter().enumerate() {
            let is_partial_tail = index == last_index && !ends_with_newline;
            match parse_line(raw) {
                ParsedLine::Event(event) => events.push(*event),
                ParsedLine::Corrupt if !is_partial_tail => corrupt += 1,
                ParsedLine::Corrupt | ParsedLine::Blank | ParsedLine::OtherVersion => {}
            }
        }
    }
    (events, corrupt)
}

/// Current time in Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = officina_core::dates::DAY_MS;
    const NOW: i64 = 1_793_102_400_000;

    fn line(ts: i64, event: &str) -> String {
        format!("{{\"v\":1,\"ts\":{ts},\"event\":\"{event}\",\"session_id\":\"s\"}}\n")
    }

    #[test]
    fn append_creates_directory_and_reader_spans_two_days() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events");
        append_line(
            &event_file_for(&events, NOW - DAY),
            &line(NOW - DAY, "SessionStart"),
        )
        .unwrap();
        append_line(&event_file_for(&events, NOW), &line(NOW, "Stop")).unwrap();
        append_line(&event_file_for(&events, NOW - 2 * DAY), &line(1, "Old")).unwrap();
        let (read, corrupt) = read_recent_events(&events, NOW);
        assert_eq!(
            read.iter().map(|e| e.event.as_str()).collect::<Vec<_>>(),
            ["SessionStart", "Stop"]
        );
        assert_eq!(corrupt, 0);
    }

    #[test]
    fn partial_tail_and_foreign_version_are_not_corrupt_garbage_is() {
        let dir = tempfile::tempdir().unwrap();
        let file = event_file_for(dir.path(), NOW);
        let content = format!(
            "{}garbage line\n{{\"v\":2,\"ts\":1,\"event\":\"X\",\"new\":true}}\n{}{{\"v\":1,\"ts\":5,\"ev",
            line(NOW, "Stop"),
            line(NOW, "Stop")
        );
        fs::write(&file, content).unwrap();
        let (read, corrupt) = read_recent_events(dir.path(), NOW);
        assert_eq!(read.len(), 2);
        assert_eq!(corrupt, 1);
    }

    #[test]
    fn invalid_utf8_costs_one_line_and_a_partial_line_does_not_swallow_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let file = event_file_for(dir.path(), NOW);
        let mut content = line(NOW, "Stop").into_bytes();
        content.extend_from_slice(b"{\"v\":1,\"bad\":\"\xff\xfe\"}\n");
        content.extend_from_slice(line(NOW, "Stop").as_bytes());
        content.extend_from_slice(b"{\"v\":1,\"ts\":5,\"ev");
        fs::write(&file, content).unwrap();

        append_line(&file, &line(NOW, "SessionEnd")).unwrap();

        let (read, corrupt) = read_recent_events(dir.path(), NOW);
        assert_eq!(
            read.iter().map(|e| e.event.as_str()).collect::<Vec<_>>(),
            ["Stop", "Stop", "SessionEnd"]
        );
        assert_eq!(
            corrupt, 2,
            "the invalid UTF-8 line and the now-closed partial line"
        );
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sub").join("q.json");
        atomic_write(&target, b"one").unwrap();
        atomic_write(&target, b"two").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "two");
        let names: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
    }
}
