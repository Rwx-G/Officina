//! Pure parts of the focus action (FR-10, AD-15): which window title belongs
//! to a session, and which arguments may reach `claude attach`.

use serde::{Deserialize, Serialize};

use crate::untrusted::Untrusted;

/// What a click on a card did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum FocusOutcome {
    Focused,
    AttachOpened,
    /// Several or no window matched; the titles are listed instead of guessing.
    Candidates(Vec<Untrusted<String>>),
    NoWindow,
    /// Windows refused the foreground change; the window was flashed.
    Refused,
    Unsupported,
    Failed(String),
}

impl FocusOutcome {
    /// Short name stored in the focus log (AD-8): no title, no path.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::AttachOpened => "attach_opened",
            Self::Candidates(_) => "candidates",
            Self::NoWindow => "no_window",
            Self::Refused => "refused",
            Self::Unsupported => "unsupported",
            Self::Failed(_) => "failed",
        }
    }
}

/// Folder names to look for in window titles, most specific first: the last
/// segment of `cwd`, then each parent, stopping before the user home and the
/// drive root. A session started in a subfolder of the opened workspace is
/// found through its parent.
///
/// ```
/// let names = officina_core::focus::candidate_names("C:/Users/me/work/api/src", Some("C:/Users/me"));
/// assert_eq!(names, ["src", "api", "work"]);
/// ```
pub fn candidate_names(cwd: &str, user_home: Option<&str>) -> Vec<String> {
    let normalised = cwd.replace('\\', "/");
    let home = user_home.map(crate::quota::normalise_path);
    let mut names = Vec::new();
    let mut current = normalised.trim_end_matches('/').to_owned();
    loop {
        if home.as_deref() == Some(current.to_lowercase().as_str()) {
            break;
        }
        let Some((parent, name)) = current.rsplit_once('/') else {
            break;
        };
        if name.is_empty() || name.ends_with(':') {
            break;
        }
        names.push(name.to_owned());
        current = parent.to_owned();
        if current.ends_with(':') || current.is_empty() {
            break;
        }
    }
    names
}

/// True when `name` appears in `title` as a whole name: bounded by the start,
/// the end, or a character that cannot belong to a folder name. `-`, `_` and
/// `.` belong to names, so `api` matches `api - Visual Studio Code` but
/// neither `rapid`, `billing-api` nor `api.ts`.
pub fn title_contains_word(title: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let title = title.to_lowercase();
    let name = name.to_lowercase();
    let in_name = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.');
    let boundary = |c: Option<char>| c.is_none_or(|c| !in_name(c));
    title.match_indices(&name).any(|(start, _)| {
        boundary(title[..start].chars().next_back())
            && boundary(title[start + name.len()..].chars().next())
    })
}

/// Which window to focus among `titles` (the visible top-level windows of the
/// nearest editor or terminal ancestor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowChoice {
    /// Index into `titles`.
    One(usize),
    /// Ambiguous or unmatched: show these titles.
    Candidates(Vec<String>),
    None,
}

/// A single window wins outright; otherwise the first candidate name that
/// matches exactly one title wins.
pub fn choose_window(titles: &[String], names: &[String]) -> WindowChoice {
    match titles.len() {
        0 => return WindowChoice::None,
        1 => return WindowChoice::One(0),
        _ => {}
    }
    for name in names {
        let matches: Vec<usize> = titles
            .iter()
            .enumerate()
            .filter(|(_, t)| title_contains_word(t, name))
            .map(|(i, _)| i)
            .collect();
        if let [only] = matches.as_slice() {
            return WindowChoice::One(*only);
        }
    }
    WindowChoice::Candidates(titles.to_vec())
}

/// A background session id may reach a command line only if it matches
/// `^[A-Za-z0-9_-]{1,64}$` (FR-10): nothing from prompts or messages, no
/// shell metacharacter, ever.
pub fn is_valid_attach_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn candidate_names_stop_at_home_and_drive() {
        assert_eq!(
            candidate_names(r"C:\Users\me\work\api", Some(r"C:\Users\me")),
            ["api", "work"]
        );
        assert_eq!(candidate_names("D:/repos/x/", None), ["x", "repos"]);
        assert_eq!(candidate_names("/home/me/proj", Some("/home/me")), ["proj"]);
        assert!(candidate_names("C:/", None).is_empty());
    }

    #[test]
    fn whole_word_matching() {
        assert!(title_contains_word("api - Visual Studio Code", "API"));
        assert!(title_contains_word(
            "main.rs - Officina - Visual Studio Code",
            "officina"
        ));
        assert!(!title_contains_word("rapid - Visual Studio Code", "api"));
        assert!(!title_contains_word("apis", "api"));
        assert!(!title_contains_word(
            "billing-api - Visual Studio Code",
            "api"
        ));
        assert!(!title_contains_word("api.ts - web - Code", "api"));
        assert!(title_contains_word(
            "billing-api - Visual Studio Code",
            "billing-api"
        ));
        assert!(title_contains_word("Écrit - Code", "écrit"));
        assert!(!title_contains_word("x", ""));
    }

    #[test]
    fn verified_vs_code_case_picks_the_session_window() {
        // E8: two windows of one Code.exe, sessions in Lorica and Officina.
        let titles = s(&[
            "Officina - Visual Studio Code",
            "Lorica - Visual Studio Code",
        ]);
        let names = candidate_names("C:/Users/me/Apps/Lorica", Some("C:/Users/me"));
        assert_eq!(choose_window(&titles, &names), WindowChoice::One(1));
    }

    #[test]
    fn subfolder_session_falls_back_to_parent_name() {
        let titles = s(&["api - Visual Studio Code", "web - Visual Studio Code"]);
        assert_eq!(
            choose_window(&titles, &s(&["src", "api"])),
            WindowChoice::One(0)
        );
    }

    #[test]
    fn ambiguity_lists_candidates_and_single_window_wins() {
        let titles = s(&["api - Code", "api - Code [2]"]);
        assert_eq!(
            choose_window(&titles, &s(&["api"])),
            WindowChoice::Candidates(titles.clone())
        );
        assert_eq!(
            choose_window(&s(&["anything"]), &s(&["api"])),
            WindowChoice::One(0)
        );
        assert_eq!(choose_window(&[], &s(&["api"])), WindowChoice::None);
    }

    /// AD-74: a window title is the terminal's, which Claude Code sets from
    /// the session, so the titles listed are untrusted; plain strings
    /// outside `untrusted::wrapped`.
    #[test]
    fn candidate_titles_are_untrusted() {
        let outcome = FocusOutcome::Candidates(vec![crate::untrusted::text("api - Code")]);
        assert_eq!(
            serde_json::to_string(&outcome).unwrap(),
            r#"{"outcome":"candidates","detail":["api - Code"]}"#
        );
        assert_eq!(
            crate::untrusted::wrapped(|| serde_json::to_value(&outcome)).unwrap(),
            serde_json::json!({"outcome": "candidates", "detail": [{"untrusted": "api - Code"}]})
        );
    }

    #[test]
    fn attach_ids_are_strictly_validated() {
        assert!(is_valid_attach_id("5e8062f4"));
        assert!(is_valid_attach_id("a_b-C9"));
        for bad in [
            "",
            "a;b",
            "a b",
            "a&b",
            "a|b",
            "../x",
            "\"x\"",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_attach_id(bad), "{bad}");
        }
    }
}
