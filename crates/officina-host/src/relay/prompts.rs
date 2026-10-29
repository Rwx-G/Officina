//! The `PermissionRequest` router's files (AD-181, D-579): the window's
//! liveness marker, the hook's request and its wait by stat for a decision
//! only the window's authority writes (AD-33's pattern), and the window's
//! cleanup. The decision's writer lives in [`super::authority`], under
//! `window-authority`.
//!
//! The hook's path gives Claude Code a decision only when one bound to the
//! request was written and claimed; every other end, a fault included, is
//! no output, so Claude Code's own prompt takes over.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use officina_core::relay::prompts::{
    MARKER_RECHECK_MS, MARKER_REFRESH_MS, PROMPT_FILE_TTL_MS, PromptBehavior, PromptDecision,
    PromptFileKind, PromptOutput, PromptRequest, WindowMarker, is_prompt_id,
    parse_prompt_file_name, prompt_file_name, window_routes,
};

use super::home::RelayHome;

use crate::files::{atomic_write, now_ms};
/// What `officina hook` branches on and polls with, which does not link
/// `officina-core` itself.
pub use officina_core::relay::prompts::{PROMPT_EVENT, PROMPT_POLL_MS};

/// Most bytes of the marker the hook reads: a marker is a few dozen.
const MARKER_READ_LIMIT: u64 = 4 * 1024;

/// What the tests of the `officina` crate set to shorten the hook's wait
/// for a decision (feature `test-seams` only, never in a shipped binary).
#[cfg(feature = "test-seams")]
pub const ENV_TEST_PROMPT_WAIT_MS: &str = "OFFICINA_TEST_PROMPT_WAIT_MS";

/// How long the hook waits for a decision:
/// [`officina_core::relay::prompts::PROMPT_WAIT_MS`], or with `test-seams`
/// [`ENV_TEST_PROMPT_WAIT_MS`] when set.
pub fn prompt_wait_ms() -> i64 {
    #[cfg(feature = "test-seams")]
    if let Some(ms) = std::env::var(ENV_TEST_PROMPT_WAIT_MS)
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return ms;
    }
    officina_core::relay::prompts::PROMPT_WAIT_MS
}

fn system_ms(at: SystemTime) -> Option<i64> {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

/// True when a window that routes prompts runs now (D-579): the marker
/// opened once, its modification time from that handle (one stat) and its
/// content read once. Any failure is no window.
pub fn window_routes_now(home: &RelayHome, now_ms: i64) -> bool {
    let Ok(file) = File::open(home.window_marker_file()) else {
        return false;
    };
    let modified = file
        .metadata()
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(system_ms);
    let mut content = Vec::new();
    if file
        .take(MARKER_READ_LIMIT)
        .read_to_end(&mut content)
        .is_err()
    {
        return false;
    }
    window_routes(Some(&content), modified, now_ms)
}

/// The window's marker while its collector runs (D-579): written at start,
/// its modification time refreshed every [`MARKER_REFRESH_MS`], removed
/// when dropped, so a collector that returns, or unwinds, leaves none.
#[derive(Debug)]
pub struct MarkerFile {
    path: PathBuf,
    refreshed: Instant,
}

impl MarkerFile {
    /// Writes `marker` to `window/alive.json` atomically.
    pub fn write(home: &RelayHome, marker: &WindowMarker) -> io::Result<MarkerFile> {
        let path = home.window_marker_file();
        let bytes = serde_json::to_vec(marker).map_err(io::Error::other)?;
        atomic_write(&path, &bytes)?;
        Ok(MarkerFile {
            path,
            refreshed: Instant::now(),
        })
    }

    /// Sets the marker's modification time to now when
    /// [`MARKER_REFRESH_MS`] passed since the last time. A marker someone
    /// removed is not written again: the window then reads as absent,
    /// which sends prompts to Claude Code's own dialog, the safe side.
    pub fn refresh_if_due(&mut self, now: Instant) {
        let due = Duration::from_millis(MARKER_REFRESH_MS.unsigned_abs());
        if now.duration_since(self.refreshed) < due {
            return;
        }
        self.refreshed = now;
        if let Ok(file) = OpenOptions::new().write(true).open(&self.path) {
            let _ = file.set_modified(SystemTime::now());
        }
    }
}

impl Drop for MarkerFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Removes the window's marker: what the window does as it exits, before
/// its collector thread is stopped with the process.
pub fn remove_marker(home: &RelayHome) {
    let _ = fs::remove_file(home.window_marker_file());
}

fn prompt_path(home: &RelayHome, id: &str, kind: PromptFileKind) -> PathBuf {
    home.prompts_dir().join(prompt_file_name(id, kind))
}

/// Request `id` as its file holds it; `None` for an id of another shape,
/// a missing file or one that does not parse.
pub fn read_request(home: &RelayHome, id: &str) -> Option<PromptRequest> {
    if !is_prompt_id(id) {
        return None;
    }
    let bytes = fs::read(prompt_path(home, id, PromptFileKind::Request)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Every request waiting in `relay/prompts/`, in name order.
pub fn read_requests(home: &RelayHome) -> Vec<PromptRequest> {
    let Ok(entries) = fs::read_dir(home.prompts_dir()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            match parse_prompt_file_name(&name) {
                Some((id, PromptFileKind::Request)) => Some(id.to_owned()),
                _ => None,
            }
        })
        .collect();
    ids.sort();
    ids.iter().filter_map(|id| read_request(home, id)).collect()
}

/// True when request `id` already has an answer: a decision, a decline or
/// a claimed decision.
pub fn answered(home: &RelayHome, id: &str) -> bool {
    [
        PromptFileKind::Decided,
        PromptFileKind::Declined,
        PromptFileKind::Consumed,
    ]
    .into_iter()
    .any(|kind| fs::symlink_metadata(prompt_path(home, id, kind)).is_ok())
}

/// The hook's request file, removed when dropped, so it goes on every
/// exit of the wait, an unwinding one included (AD-181).
struct RequestFile(PathBuf);

impl Drop for RequestFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// How a wait for a decision ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitEnd {
    /// A decision bound to the request, claimed by this wait.
    Decided(PromptBehavior),
    /// The author declined: Claude Code's own prompt, at once.
    Declined,
    /// A decision that does not answer this request (another session or
    /// input, past the deadline, unreadable), or one already claimed.
    Unbound,
    /// The window's marker went missing or stale (D-579).
    NoWindow,
    /// No answer by the deadline.
    Deadline,
}

impl WaitEnd {
    /// What the hook prints: a behaviour only for a claimed decision.
    pub fn behavior(self) -> Option<PromptBehavior> {
        match self {
            WaitEnd::Decided(behavior) => Some(behavior),
            WaitEnd::Declined | WaitEnd::Unbound | WaitEnd::NoWindow | WaitEnd::Deadline => None,
        }
    }
}

/// Claims `id`'s decision once (single use), as `approvals::consume` does:
/// `<id>.consumed.json` is created new with the decision's bytes, which
/// only one caller can do, then the decision file goes.
fn consume(home: &RelayHome, id: &str, decision: &[u8]) -> bool {
    let claimed = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(prompt_path(home, id, PromptFileKind::Consumed))
        .and_then(|mut file| file.write_all(decision));
    if claimed.is_err() {
        return false;
    }
    let _ = fs::remove_file(prompt_path(home, id, PromptFileKind::Decided));
    true
}

/// Waits for request's answer by stat (AD-33's pattern), a look every
/// `poll`, the window's marker read again every [`MARKER_RECHECK_MS`] by
/// `clock`, until `deadline_ms` by the same clock. A decision file is read
/// once: bound to the request and claimed, it is the answer; otherwise the
/// wait ends with none.
pub fn wait_for_decision(
    home: &RelayHome,
    request: &PromptRequest,
    deadline_ms: i64,
    poll: Duration,
    clock: &dyn Fn() -> i64,
) -> WaitEnd {
    let decided = prompt_path(home, &request.id, PromptFileKind::Decided);
    let declined = prompt_path(home, &request.id, PromptFileKind::Declined);
    let mut marker_read_at = clock();
    loop {
        if fs::symlink_metadata(&declined).is_ok() {
            return WaitEnd::Declined;
        }
        // A decision file absent, or held open by its writer for a moment,
        // is looked at again at the next poll.
        if let Ok(bytes) = fs::read(&decided) {
            let bound = serde_json::from_slice::<PromptDecision>(&bytes)
                .ok()
                .filter(|decision| decision.answers(request, clock()));
            return match bound {
                Some(decision) if consume(home, &request.id, &bytes) => {
                    WaitEnd::Decided(decision.behavior)
                }
                _ => WaitEnd::Unbound,
            };
        }
        let now = clock();
        if now >= deadline_ms {
            return WaitEnd::Deadline;
        }
        if now - marker_read_at >= MARKER_RECHECK_MS {
            marker_read_at = now;
            if !window_routes_now(home, now) {
                return WaitEnd::NoWindow;
            }
        }
        let left = Duration::from_millis((deadline_ms - now).unsigned_abs());
        std::thread::sleep(poll.min(left));
    }
}

/// `officina hook`'s `PermissionRequest` path (AD-181, D-511, D-579): with
/// no window that routes prompts, nothing at all; with no session id,
/// nothing either; else the request is written, waited for up to
/// `wait_ms`, and removed whatever ends the wait. The output only for a
/// decision the window wrote, bound to the request and claimed here.
pub fn route(
    home: &RelayHome,
    payload: &serde_json::Value,
    claude_pid: Option<u32>,
    wait_ms: i64,
    poll: Duration,
) -> Option<PromptOutput> {
    let now = now_ms();
    if !window_routes_now(home, now) {
        return None;
    }
    let request =
        PromptRequest::from_payload(payload, claude_pid, std::process::id(), now, wait_ms)?;
    let path = prompt_path(home, &request.id, PromptFileKind::Request);
    let bytes = serde_json::to_vec(&request).ok()?;
    atomic_write(&path, &bytes).ok()?;
    let _request_file = RequestFile(path);
    wait_for_decision(home, &request, request.deadline_ms, poll, &now_ms)
        .behavior()
        .map(PromptOutput::of)
}

/// The window's cleanup (AD-33's): the router's files older than
/// [`PROMPT_FILE_TTL_MS`] by modification time, a writer's leftover
/// temporary file included. Only names of the router's shapes are
/// touched. Returns how many went.
pub fn prune(home: &RelayHome, now: SystemTime) -> usize {
    let ttl = Duration::from_millis(PROMPT_FILE_TTL_MS.unsigned_abs());
    super::approvals::prune_dir(&home.prompts_dir(), now, ttl, |name| {
        parse_prompt_file_name(name).is_some()
            || (name.ends_with(".tmp") && name.split('.').next().is_some_and(is_prompt_id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::prompts::PROMPT_WAIT_MS;

    fn home() -> (tempfile::TempDir, RelayHome) {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path(), None);
        (dir, home)
    }

    fn routing_marker(home: &RelayHome) -> MarkerFile {
        MarkerFile::write(
            home,
            &WindowMarker {
                pid: std::process::id(),
                started_at_ms: now_ms(),
                routes_prompts: true,
            },
        )
        .unwrap()
    }

    fn request(home: &RelayHome, deadline_in_ms: i64) -> PromptRequest {
        let request = PromptRequest::from_payload(
            &serde_json::json!({
                "session_id": "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0",
                "tool_name": "Bash",
                "tool_input": {"command": "rm -rf build"},
            }),
            None,
            std::process::id(),
            now_ms(),
            deadline_in_ms,
        )
        .unwrap();
        atomic_write(
            &prompt_path(home, &request.id, PromptFileKind::Request),
            &serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        request
    }

    fn decide(home: &RelayHome, decision: &PromptDecision) {
        fs::create_dir_all(home.prompts_dir()).unwrap();
        fs::write(
            prompt_path(home, &decision.id, PromptFileKind::Decided),
            serde_json::to_vec(decision).unwrap(),
        )
        .unwrap();
    }

    fn wait(home: &RelayHome, request: &PromptRequest) -> WaitEnd {
        wait_for_decision(
            home,
            request,
            request.deadline_ms,
            Duration::from_millis(10),
            &now_ms,
        )
    }

    /// Story 5.22 AC 4: a decision bound to the request is claimed and
    /// given once; a second wait on it finds nothing to claim.
    #[test]
    fn a_matching_decision_is_consumed_once() {
        let (_dir, home) = home();
        let _marker = routing_marker(&home);
        let request = request(&home, 5_000);
        let decision = PromptDecision::on(&request, PromptBehavior::Allow, now_ms());
        decide(&home, &decision);
        assert_eq!(
            wait(&home, &request),
            WaitEnd::Decided(PromptBehavior::Allow)
        );
        assert!(!prompt_path(&home, &request.id, PromptFileKind::Decided).exists());
        assert!(prompt_path(&home, &request.id, PromptFileKind::Consumed).exists());
        // The same decision written again cannot be claimed a second time.
        decide(&home, &decision);
        assert_eq!(wait(&home, &request), WaitEnd::Unbound);
    }

    /// Story 5.22 AC 4: a decision for another session or another input
    /// ends the wait with nothing, and is never claimed.
    #[test]
    fn a_decision_for_another_session_or_hash_is_ignored() {
        let (_dir, home) = home();
        let _marker = routing_marker(&home);
        let request = request(&home, 5_000);
        let mut other_session = PromptDecision::on(&request, PromptBehavior::Allow, now_ms());
        other_session.session_id = "another-session".into();
        let mut other_hash = PromptDecision::on(&request, PromptBehavior::Allow, now_ms());
        other_hash.input_sha256 = "00".repeat(32);
        for decision in [other_session, other_hash] {
            decide(&home, &decision);
            let started = Instant::now();
            assert_eq!(wait(&home, &request), WaitEnd::Unbound);
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(!prompt_path(&home, &request.id, PromptFileKind::Consumed).exists());
        }
    }

    /// Story 5.22 AC 4 (D-579): a marker that turns stale while the hook
    /// waits ends the wait at the next reading, long before the deadline.
    #[test]
    fn a_stale_marker_ends_the_wait() {
        let (_dir, home) = home();
        let _marker = routing_marker(&home);
        let request = request(&home, 30_000);
        let old = SystemTime::now() - Duration::from_secs(60);
        OpenOptions::new()
            .write(true)
            .open(home.window_marker_file())
            .unwrap()
            .set_modified(old)
            .unwrap();
        let started = Instant::now();
        assert_eq!(wait(&home, &request), WaitEnd::NoWindow);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// Story 5.22 AC 4: a declined request ends the wait with nothing.
    #[test]
    fn a_declined_request_ends_the_wait() {
        let (_dir, home) = home();
        let _marker = routing_marker(&home);
        let request = request(&home, 5_000);
        fs::write(
            prompt_path(&home, &request.id, PromptFileKind::Declined),
            b"",
        )
        .unwrap();
        assert_eq!(wait(&home, &request), WaitEnd::Declined);
        assert_eq!(WaitEnd::Declined.behavior(), None);
    }

    /// Story 5.22 AC 4: with no answer, the wait ends at its deadline (a
    /// 200 ms one here) with no decision: not before the deadline, which
    /// counts from the request's time, before its file was written.
    #[test]
    fn the_deadline_ends_the_wait_with_no_decision() {
        let (_dir, home) = home();
        let _marker = routing_marker(&home);
        let request = request(&home, 200);
        let started = Instant::now();
        assert_eq!(wait(&home, &request), WaitEnd::Deadline);
        assert!(now_ms() >= request.deadline_ms);
        let took = started.elapsed();
        assert!(took < Duration::from_secs(5), "{took:?}");
    }

    /// Story 5.22 AC 2 and 3: the marker is removed when its holder goes;
    /// without it, `route` writes nothing and gives nothing.
    #[test]
    fn without_a_window_route_writes_nothing() {
        let (dir, home) = home();
        drop(routing_marker(&home));
        assert!(!home.window_marker_file().exists());
        let payload = serde_json::json!({"session_id": "s", "tool_name": "Bash"});
        assert!(
            route(
                &home,
                &payload,
                None,
                PROMPT_WAIT_MS,
                Duration::from_millis(10)
            )
            .is_none()
        );
        assert!(!home.prompts_dir().exists());
        let left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, [std::ffi::OsString::from("window")]);
    }

    /// AD-33's cleanup: router files past a day go, newer ones and other
    /// names stay.
    #[test]
    fn prune_takes_old_router_files_only() {
        let (_dir, home) = home();
        let dir = home.prompts_dir();
        fs::create_dir_all(&dir).unwrap();
        let old =
            SystemTime::now() - Duration::from_millis(PROMPT_FILE_TTL_MS.unsigned_abs() + 60_000);
        for name in [
            "0a-1.json",
            "0a-1.decided.json",
            "0a-1.json.7.tmp",
            "notes.txt",
        ] {
            let file = File::create(dir.join(name)).unwrap();
            file.set_modified(old).unwrap();
        }
        fs::write(dir.join("0b-2.json"), b"{}").unwrap();
        assert_eq!(prune(&home, SystemTime::now()), 3);
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["0b-2.json", "notes.txt"]);
    }
}
