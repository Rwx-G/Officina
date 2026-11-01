//! Images (AD-99): the Officina base images a project Dockerfile may
//! descend from, and the one Dockerfile parser, [`stages`], which phase 2
//! (story 6.2) and Build images (story 6.8) both use.

use serde::{Deserialize, Serialize};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What an image is (AD-99, D-212): the base, the browser base (Slice
    /// B), the gateway, or a project's.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ImageKind {
        Workshop,
        Browser,
        Gateway,
        Project,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a build ended (D-212): a project image that does not descend
    /// from a base, or replaces its programs or reserved variables, is
    /// `not_from_base` (D-233).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BuildOutcome {
        Built,
        Failed,
        NotFromBase,
        Stopped,
    }
}

/// The image every workshop project builds on (AD-99).
pub const WORKSHOP_BASE: &str = "officina-workshop:local";
/// The browser base, Slice B (AD-99, FR-16).
pub const BROWSER_BASE: &str = "officina-workshop-browser:local";
/// The images a project Dockerfile's final stage may descend from.
pub const PROJECT_BASES: &[&str] = &[WORKSHOP_BASE, BROWSER_BASE];

/// One `FROM` of a Dockerfile: the image it names, as written, and its
/// alias, lowercase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub image: String,
    pub alias: Option<String>,
}

/// Why a Dockerfile's stages cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StageError {
    #[error("the Dockerfile has no FROM line")]
    NoFrom,
    #[error("line {0}: a FROM line names no image")]
    EmptyFrom(usize),
    #[error("line {0}: an instruction appears before the first FROM")]
    BeforeFrom(usize),
}

/// The `FROM` lines of `dockerfile`, in order. Comments, blank lines,
/// backslash continuations and heredoc bodies (`<<WORD` to a line holding
/// `WORD`) are skipped, so a heredoc line spelled like a `FROM` is never
/// read as one; `ARG` may precede the first `FROM`, nothing else may.
///
/// ```
/// use officina_core::workshop::image::stages;
/// let found = stages("FROM rust:1 AS build\nRUN cargo build\nFROM officina-workshop:local\n").unwrap();
/// assert_eq!(found.len(), 2);
/// assert_eq!(found[0].alias.as_deref(), Some("build"));
/// ```
pub fn stages(dockerfile: &str) -> Result<Vec<Stage>, StageError> {
    let mut found = Vec::new();
    let mut heredoc: Option<String> = None;
    let mut instruction = String::new();
    let mut first_line = 0;
    for (index, raw) in dockerfile.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim_end_matches('\r');
        if let Some(end) = &heredoc {
            if line.trim() == end {
                heredoc = None;
            }
            continue;
        }
        let trimmed = line.trim_start();
        if instruction.is_empty() && (trimmed.is_empty() || trimmed.starts_with('#')) {
            continue;
        }
        if instruction.is_empty() {
            first_line = number;
        }
        if let Some(body) = trimmed.strip_suffix('\\') {
            instruction.push_str(body);
            instruction.push(' ');
            continue;
        }
        instruction.push_str(trimmed);
        let whole = std::mem::take(&mut instruction);
        heredoc = heredoc_end(&whole);
        let mut words = whole.split_whitespace();
        let keyword = words.next().unwrap_or_default().to_ascii_uppercase();
        if keyword == "FROM" {
            let rest: Vec<&str> = words.filter(|w| !w.starts_with("--")).collect();
            let image = rest
                .first()
                .ok_or(StageError::EmptyFrom(first_line))?
                .to_string();
            let alias = match rest.get(1) {
                Some(word) if word.eq_ignore_ascii_case("as") => {
                    rest.get(2).map(|a| a.to_ascii_lowercase())
                }
                _ => None,
            };
            found.push(Stage { image, alias });
        } else if found.is_empty() && keyword != "ARG" {
            return Err(StageError::BeforeFrom(first_line));
        }
    }
    if found.is_empty() {
        return Err(StageError::NoFrom);
    }
    Ok(found)
}

/// The word ending a heredoc an instruction opens (`<<EOF`, `<<-EOF`,
/// `<<"EOF"`), if it opens one.
fn heredoc_end(instruction: &str) -> Option<String> {
    let start = instruction.find("<<")?;
    let rest = instruction[start + 2..].trim_start_matches('-');
    let word: String = rest
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!word.is_empty()).then_some(word)
}

/// The Officina base the final stage descends from, following stage aliases
/// back to an image; `None` when it reaches any other image, or names a
/// variable (`FROM ${BASE}`), which phase 2 cannot resolve. Other stages may
/// name any image (a builder stage pulling a toolchain).
///
/// ```
/// use officina_core::workshop::image::{final_base, stages, WORKSHOP_BASE};
/// let found = stages("FROM officina-workshop:local AS base\nFROM base\n").unwrap();
/// assert_eq!(final_base(&found), Some(WORKSHOP_BASE));
/// ```
pub fn final_base(found: &[Stage]) -> Option<&'static str> {
    let mut index = found.len().checked_sub(1)?;
    loop {
        let image = found[index].image.as_str();
        if let Some(base) = PROJECT_BASES.iter().find(|b| **b == image) {
            return Some(base);
        }
        let alias = image.to_ascii_lowercase();
        index = found[..index]
            .iter()
            .rposition(|stage| stage.alias.as_deref() == Some(alias.as_str()))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_final_stage_must_descend_from_a_base() {
        for base in PROJECT_BASES {
            let found = stages(&format!("FROM {base}\nRUN true\n")).unwrap();
            assert_eq!(final_base(&found), Some(*base));
        }
        let other = stages("FROM officina-workshop:local\nFROM debian:trixie-slim\n").unwrap();
        assert_eq!(final_base(&other), None);
        let variable = stages("ARG BASE=officina-workshop:local\nFROM ${BASE}\n").unwrap();
        assert_eq!(final_base(&variable), None, "a variable is not resolved");
        let tagged = stages("FROM officina-workshop:latest\n").unwrap();
        assert_eq!(final_base(&tagged), None);
    }

    #[test]
    fn an_alias_chain_is_followed() {
        let text = "FROM officina-workshop:local AS One\nFROM one AS two\nRUN x\nFROM TWO\n";
        assert_eq!(final_base(&stages(text).unwrap()), Some(WORKSHOP_BASE));
        let unknown = "FROM officina-workshop:local AS one\nFROM three\n";
        assert_eq!(final_base(&stages(unknown).unwrap()), None);
        let later = "FROM two AS one\nFROM officina-workshop:local AS two\nFROM one\n";
        assert_eq!(
            final_base(&stages(later).unwrap()),
            None,
            "an alias names an earlier stage only"
        );
    }

    #[test]
    fn a_builder_stage_may_name_any_image() {
        let text = "# syntax\nARG V=1\nFROM --platform=linux/amd64 rust:1-bookworm AS build\nRUN cargo build \\\n  --release\nFROM officina-workshop:local\nCOPY --from=build /x /y\n";
        let found = stages(text).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].image, "rust:1-bookworm");
        assert_eq!(final_base(&found), Some(WORKSHOP_BASE));
    }

    #[test]
    fn a_heredoc_line_is_never_a_stage() {
        let text = "FROM debian:trixie-slim\nRUN <<EOF\nFROM officina-workshop:local\nEOF\n";
        let found = stages(text).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(final_base(&found), None);
    }

    #[test]
    fn malformed_files_are_refused() {
        assert_eq!(stages("# only a comment\n"), Err(StageError::NoFrom));
        assert_eq!(stages("RUN x\nFROM a\n"), Err(StageError::BeforeFrom(1)));
        assert_eq!(stages("FROM\n"), Err(StageError::EmptyFrom(1)));
        assert_eq!(stages("FROM --platform=x\n"), Err(StageError::EmptyFrom(1)));
    }
}
