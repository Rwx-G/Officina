//! The focus action (FR-10, AD-15) and its outcome log (AD-8).

use std::path::{Path, PathBuf};

use officina_core::board::{Card, SessionKindView};
use officina_core::focus::{FocusOutcome, candidate_names, is_valid_attach_id};
use officina_core::quota::fnv1a;

use crate::files::append_line;
use crate::paths::{Paths, user_home};

/// What the focus action needs beyond the card.
#[derive(Debug, Clone)]
pub struct FocusContext {
    pub claude: Option<PathBuf>,
    pub config_override: Option<PathBuf>,
    pub state_dir: PathBuf,
}

impl FocusContext {
    pub fn new(paths: &Paths, claude: Option<PathBuf>) -> FocusContext {
        FocusContext {
            claude,
            config_override: paths.claude_config_override.clone(),
            state_dir: paths.state_dir(),
        }
    }
}

/// Focuses the window of a live interactive session, or opens an attach
/// console for a background one.
pub fn focus_card(card: &Card, ctx: &FocusContext) -> FocusOutcome {
    if card.kind == Some(SessionKindView::Background) {
        return open_attach(card, ctx);
    }
    let (Some(pid), Some(cwd)) = (card.pid, card.cwd.as_deref()) else {
        return FocusOutcome::NoWindow;
    };
    let home = user_home().map(|h| h.to_string_lossy().into_owned());
    let names = candidate_names(cwd, home.as_deref());
    focus_window(pid, &names)
}

#[cfg(windows)]
fn focus_window(pid: u32, names: &[String]) -> FocusOutcome {
    crate::platform::windows::focus_session_window(pid, names)
}

#[cfg(not(windows))]
fn focus_window(_pid: u32, _names: &[String]) -> FocusOutcome {
    FocusOutcome::Unsupported
}

fn open_attach(card: &Card, ctx: &FocusContext) -> FocusOutcome {
    let Some(id) = card
        .background_id
        .as_deref()
        .filter(|id| is_valid_attach_id(id))
    else {
        return FocusOutcome::Failed("invalid or missing background session id".into());
    };
    let Some(claude) = &ctx.claude else {
        return FocusOutcome::Failed("claude not found".into());
    };
    spawn_attach(claude, id, ctx.config_override.as_deref())
}

#[cfg(windows)]
fn spawn_attach(claude: &Path, id: &str, config: Option<&Path>) -> FocusOutcome {
    match crate::platform::windows::open_attach_console(claude, id, config) {
        Ok(()) => FocusOutcome::AttachOpened,
        Err(e) => FocusOutcome::Failed(e.to_string()),
    }
}

#[cfg(not(windows))]
fn spawn_attach(_claude: &Path, _id: &str, _config: Option<&Path>) -> FocusOutcome {
    FocusOutcome::Unsupported
}

/// Characters an interactive bash still interprets inside double quotes:
/// the closing quote, expansion, command substitution and history expansion.
/// A pasted command must not run anything its text does not show.
pub const BASH_DOUBLE_QUOTE_SPECIALS: [char; 4] = ['"', '$', '`', '!'];

/// The attach command line shown for copying, in bash syntax (the author
/// pastes it into Git Bash). Built from validated parts only. When the floor
/// names a config dir, the command sets `CLAUDE_CONFIG_DIR` for itself, with
/// forward slashes as in the hooks block. A dir holding one of
/// [`BASH_DOUBLE_QUOTE_SPECIALS`] or a control character is left out: the
/// command then attaches through the default config dir.
pub fn attach_command(card: &Card, ctx: &FocusContext) -> Option<String> {
    let id = card
        .background_id
        .as_deref()
        .filter(|id| is_valid_attach_id(id))?;
    let config_prefix = ctx
        .config_override
        .as_deref()
        .map(|dir| dir.to_string_lossy().replace('\\', "/"))
        .filter(|dir| {
            !dir.chars()
                .any(|c| BASH_DOUBLE_QUOTE_SPECIALS.contains(&c) || c.is_control())
        })
        .map(|dir| format!("{}=\"{dir}\" ", crate::paths::ENV_CLAUDE_CONFIG_DIR))
        .unwrap_or_default();
    Some(format!("{config_prefix}claude attach {id}"))
}

fn card_hash(key: &str) -> String {
    format!("{:016x}", fnv1a(key))
}

/// Appends a focus outcome to `state/focus.jsonl`: outcome code and a hash of
/// the card key, never a title or a path (AD-8).
pub fn log_outcome(state_dir: &Path, card: &Card, outcome: &FocusOutcome, now_ms: i64) {
    let line = serde_json::json!({
        "v": 1,
        "ts": now_ms,
        "card": card_hash(&card.key),
        "kind": card.kind,
        "outcome": outcome.code(),
    });
    let _ = append_line(&state_dir.join("focus.jsonl"), &format!("{line}\n"));
}

/// Appends a "wrong window" report for a card.
pub fn log_wrong_window(state_dir: &Path, key: &str, now_ms: i64) {
    let line = serde_json::json!({
        "v": 1,
        "ts": now_ms,
        "card": card_hash(key),
        "outcome": "wrong_window",
    });
    let _ = append_line(&state_dir.join("focus.jsonl"), &format!("{line}\n"));
}

/// Focus outcomes are kept this long (AD-8): enough for the 10-working-day
/// success rate, not a history of the author's clicks.
pub const FOCUS_LOG_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Drops focus log lines older than [`FOCUS_LOG_RETENTION_MS`]. Unparsable
/// lines are dropped too: nothing reads them. Returns how many were removed.
pub fn prune_focus_log(state_dir: &Path, now_ms: i64) -> usize {
    let path = state_dir.join("focus.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return 0;
    };
    let total = content.lines().filter(|l| !l.trim().is_empty()).count();
    let kept: String = content
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| v["ts"].as_i64())
                .is_some_and(|ts| now_ms - ts <= FOCUS_LOG_RETENTION_MS)
        })
        .map(|line| {
            format!(
                "{line}
"
            )
        })
        .collect();
    let removed = total - kept.lines().count();
    if removed > 0 {
        let _ = crate::files::atomic_write(&path, kept.as_bytes());
    }
    removed
}

/// Focus success rate over the log (AD-15 metric): first clicks that focused
/// or opened an attach and were not followed within 10 s by a wrong-window
/// report or a second click on the same card. `None` when nothing was logged.
pub fn success_rate(state_dir: &Path) -> Option<(usize, usize)> {
    let content = std::fs::read_to_string(state_dir.join("focus.jsonl")).ok()?;
    let lines: Vec<serde_json::Value> = content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let mut clicks = 0;
    let mut good = 0;
    for (i, line) in lines.iter().enumerate() {
        let outcome = line["outcome"].as_str().unwrap_or_default();
        if outcome == "wrong_window" {
            continue;
        }
        let (card, ts) = (&line["card"], line["ts"].as_i64().unwrap_or(0));
        let repeated_before = lines[..i]
            .iter()
            .any(|p| &p["card"] == card && ts - p["ts"].as_i64().unwrap_or(0) <= 10_000);
        if repeated_before {
            continue;
        }
        clicks += 1;
        let undone = lines[i + 1..]
            .iter()
            .any(|n| &n["card"] == card && n["ts"].as_i64().unwrap_or(0) - ts <= 10_000);
        if matches!(outcome, "focused" | "attach_opened") && !undone {
            good += 1;
        }
    }
    Some((good, clicks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::board::{Column, ProjectView};

    fn card(kind: SessionKindView, id: Option<&str>, cwd: Option<&str>) -> Card {
        Card {
            key: "bg:x".into(),
            column: Column::Working,
            badge: None,
            kind: Some(kind),
            name: "n".into(),
            project: ProjectView {
                name: "p".into(),
                color: None,
            },
            session_id: None,
            background_id: id.map(Into::into),
            pid: None,
            cwd: cwd.map(Into::into),
            waiting_for: None,
            last_prompt: None,
            last_message: None,
            last_event_at_ms: None,
            started_at_ms: None,
            attach_available: true,
            subject: None,
        }
    }

    fn ctx(dir: &Path) -> FocusContext {
        FocusContext {
            claude: None,
            config_override: None,
            state_dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn attach_refuses_bad_ids_and_a_missing_claude_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_string_lossy().into_owned();
        let bad_id = card(SessionKindView::Background, Some("a;calc"), Some(&cwd));
        assert!(matches!(
            focus_card(&bad_id, &ctx(dir.path())),
            FocusOutcome::Failed(_)
        ));
        let no_claude = card(SessionKindView::Background, Some("abc"), Some(&cwd));
        assert_eq!(
            focus_card(&no_claude, &ctx(dir.path())),
            FocusOutcome::Failed("claude not found".into())
        );
        let context = ctx(dir.path());
        assert_eq!(
            attach_command(&no_claude, &context).unwrap(),
            "claude attach abc"
        );
        assert_eq!(attach_command(&bad_id, &context), None);
    }

    #[test]
    fn attach_command_carries_the_floor_config_dir_when_bash_keeps_it_literal() {
        let dir = tempfile::tempdir().unwrap();
        let bg = card(SessionKindView::Background, Some("abc"), None);
        let with_config = |config: &str| FocusContext {
            config_override: Some(PathBuf::from(config)),
            ..ctx(dir.path())
        };
        assert_eq!(
            attach_command(&bg, &with_config(r"C:\Users\user01\.claude-work")).unwrap(),
            "CLAUDE_CONFIG_DIR=\"C:/Users/user01/.claude-work\" claude attach abc"
        );
        assert_eq!(
            attach_command(&bg, &with_config("D:/My Configs/claude (work)")).unwrap(),
            "CLAUDE_CONFIG_DIR=\"D:/My Configs/claude (work)\" claude attach abc"
        );
        let interpreted = BASH_DOUBLE_QUOTE_SPECIALS
            .iter()
            .chain(&['\n', '\t'])
            .map(|c| format!("C:/a{c}b"));
        for dir in interpreted {
            assert_eq!(
                attach_command(&bg, &with_config(&dir)).unwrap(),
                "claude attach abc",
                "{dir:?}"
            );
        }
    }

    #[test]
    fn interactive_without_pid_has_no_window() {
        let dir = tempfile::tempdir().unwrap();
        let c = card(SessionKindView::Interactive, None, Some("C:/x"));
        assert_eq!(focus_card(&c, &ctx(dir.path())), FocusOutcome::NoWindow);
    }

    #[test]
    fn focus_log_keeps_thirty_days() {
        let dir = tempfile::tempdir().unwrap();
        let c = card(SessionKindView::Interactive, None, None);
        let now = 100 * FOCUS_LOG_RETENTION_MS;
        log_outcome(
            dir.path(),
            &c,
            &FocusOutcome::Focused,
            now - FOCUS_LOG_RETENTION_MS - 1,
        );
        log_outcome(dir.path(), &c, &FocusOutcome::Focused, now - 1000);
        assert_eq!(prune_focus_log(dir.path(), now), 1);
        assert_eq!(prune_focus_log(dir.path(), now), 0);
        assert_eq!(success_rate(dir.path()), Some((1, 1)));
        assert_eq!(prune_focus_log(&dir.path().join("missing"), now), 0);
    }

    #[test]
    fn success_rate_discounts_wrong_windows_and_repeat_clicks() {
        let dir = tempfile::tempdir().unwrap();
        let a = card(SessionKindView::Interactive, None, None);
        let mut b = a.clone();
        b.key = "pid:2:2".into();
        let mut c = a.clone();
        c.key = "pid:3:3".into();
        log_outcome(dir.path(), &a, &FocusOutcome::Focused, 0);
        log_outcome(dir.path(), &b, &FocusOutcome::Focused, 100_000);
        log_wrong_window(dir.path(), &b.key, 103_000);
        log_outcome(dir.path(), &c, &FocusOutcome::NoWindow, 200_000);
        log_outcome(dir.path(), &c, &FocusOutcome::NoWindow, 201_000);
        assert_eq!(success_rate(dir.path()), Some((1, 3)));
        let content = std::fs::read_to_string(dir.path().join("focus.jsonl")).unwrap();
        assert!(!content.contains("bg:x") && !content.contains("C:/"));
    }
}
