//! Templates (FR-2, AD-75, AD-77's types): the names a template, its
//! parameters and its projects carry, the model alias, and a prompt's
//! placeholders, rendering and longest rendering. Phase 2 validates a
//! template with these (`relay::catalogue`); [`ApprovedTemplate`] reads one
//! from the approved snapshot, and `orders::work_order::WorkOrder` builds on
//! it.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use super::catalogue::{
    ApprovedCatalogue, RelayFloor, is_valid_action_name, is_valid_param_name, project_dirs,
};
use super::params::is_forbidden_format;

/// Longest rendered prompt, in characters (FR-2): a template is valid only
/// when its longest rendering fits.
pub const MAX_PROMPT_CHARS: usize = 8_000;

/// Length of a template's `description`, in characters (AD-75).
pub const DESCRIPTION_CHARS: RangeInclusive<usize> = 1..=200;

/// What a prompt's first character after leading whitespace may not be
/// (D-159): Claude Code reads a leading `/` as a command, `!` as a shell
/// escape, and `-` would read as an option on a command line.
pub const FORBIDDEN_PROMPT_STARTS: &[char] = &['/', '!', '-'];

/// The characters a prompt may hold although they are control characters.
pub const PROMPT_CONTROL_ALLOWED: &[char] = &['\n', '\t'];

closed_enum! {
    #[serde(rename_all = "lowercase")]
    /// A model a template, the orchestrator or the review names (AD-75,
    /// D-132), passed to `claude --model` as written.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ModelAlias {
        Haiku,
        Sonnet,
        Opus,
    }
}

/// The review's model when `[auto_review]` names none (D-132).
pub const DEFAULT_REVIEW_MODEL: ModelAlias = ModelAlias::Haiku;

/// A template's name: [`is_valid_action_name`]'s rule (AD-77). Templates and
/// actions are separate namespaces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemplateName(String);

impl TemplateName {
    /// The name, when it follows the rule.
    ///
    /// ```
    /// use officina_core::relay::template::TemplateName;
    /// assert!(TemplateName::parse("dependency-bump").is_some());
    /// assert!(TemplateName::parse("Bump").is_none());
    /// ```
    pub fn parse(name: &str) -> Option<TemplateName> {
        is_valid_action_name(name).then(|| TemplateName(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A template parameter's name: [`is_valid_param_name`]'s rule (AD-77).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamName(String);

impl ParamName {
    /// The name, when it follows the rule.
    pub fn parse(name: &str) -> Option<ParamName> {
        is_valid_param_name(name).then(|| ParamName(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A catalogue project's name (AD-77). The floor sets no character rule on
/// project names, so this one refuses only what no name the author reads
/// may hold: nothing at all, a control character or a character of
/// `FORBIDDEN_FORMAT_RANGES`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectName(String);

impl ProjectName {
    /// The name, when it is one.
    pub fn parse(name: &str) -> Option<ProjectName> {
        let readable = !name.is_empty()
            && !name
                .chars()
                .any(|c| c.is_control() || is_forbidden_format(c));
        readable.then(|| ProjectName(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One piece of a prompt: author text, or a placeholder naming a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece<'p> {
    Text(&'p str),
    Placeholder(&'p str),
}

/// Why a prompt's braces do not delimit placeholders (D-2), at a character
/// offset of the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PromptBraceError {
    #[error("`}}` at character {0} closes no placeholder")]
    StrayClose(usize),
    #[error("`{{` at character {0} is never closed")]
    Unclosed(usize),
    #[error("`{{` at character {0} opens inside a placeholder")]
    Nested(usize),
}

/// The pieces of `prompt`. Braces delimit placeholders only (D-2): every
/// `{` closes with a `}` before any other brace, and no `}` stands alone.
///
/// ```
/// use officina_core::relay::template::{Piece, pieces};
/// assert_eq!(
///     pieces("Bump {package}.").unwrap(),
///     [Piece::Text("Bump "), Piece::Placeholder("package"), Piece::Text(".")],
/// );
/// assert!(pieces("a } b").is_err());
/// ```
pub fn pieces(prompt: &str) -> Result<Vec<Piece<'_>>, PromptBraceError> {
    let mut found = Vec::new();
    let mut text_start = 0;
    let mut open: Option<(usize, usize)> = None;
    for (index, (byte, c)) in prompt.char_indices().enumerate() {
        match (c, open) {
            ('{', None) => {
                if byte > text_start {
                    found.push(Piece::Text(&prompt[text_start..byte]));
                }
                open = Some((index, byte));
            }
            ('{', Some(_)) => return Err(PromptBraceError::Nested(index)),
            ('}', None) => return Err(PromptBraceError::StrayClose(index)),
            ('}', Some((_, start))) => {
                found.push(Piece::Placeholder(&prompt[start + 1..byte]));
                open = None;
                text_start = byte + 1;
            }
            _ => {}
        }
    }
    if let Some((index, _)) = open {
        return Err(PromptBraceError::Unclosed(index));
    }
    if text_start < prompt.len() {
        found.push(Piece::Text(&prompt[text_start..]));
    }
    Ok(found)
}

/// `prompt` with each placeholder replaced by its value, in one pass, so a
/// value is never read as a placeholder in turn. `None` when the braces do
/// not delimit placeholders or a placeholder has no value.
///
/// ```
/// use std::collections::BTreeMap;
/// use officina_core::relay::template::render;
/// let values = BTreeMap::from([("package".to_owned(), "serde_json".to_owned())]);
/// assert_eq!(render("Bump {package}.", &values).as_deref(), Some("Bump serde_json."));
/// ```
pub fn render(prompt: &str, values: &BTreeMap<String, String>) -> Option<String> {
    let mut rendered = String::with_capacity(prompt.len());
    for piece in pieces(prompt).ok()? {
        match piece {
            Piece::Text(text) => rendered.push_str(text),
            Piece::Placeholder(name) => rendered.push_str(values.get(name)?),
        }
    }
    Some(rendered)
}

/// The length in characters of `prompt`'s longest rendering: each
/// placeholder replaced by its parameter's longest value. `None` when the
/// braces do not delimit placeholders or a placeholder names no parameter.
///
/// ```
/// use std::collections::BTreeMap;
/// use officina_core::relay::template::worst_case_chars;
/// let params = BTreeMap::from([("level".to_owned(), vec!["patch".to_owned(), "minor".to_owned()])]);
/// assert_eq!(worst_case_chars("a {level}", &params), Some(7));
/// ```
pub fn worst_case_chars(prompt: &str, params: &BTreeMap<String, Vec<String>>) -> Option<usize> {
    pieces(prompt)
        .ok()?
        .into_iter()
        .try_fold(0, |total, piece| {
            let length = match piece {
                Piece::Text(text) => text.chars().count(),
                Piece::Placeholder(name) => params
                    .get(name)?
                    .iter()
                    .map(|value| value.chars().count())
                    .max()
                    .unwrap_or(0),
            };
            Some(total + length)
        })
}

/// A template phase 2 accepted, every list sorted (FR-2, AD-75). Approval and
/// launch read it from the approved snapshot ([`ApprovedTemplate`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub description: String,
    /// The catalogue projects it may run in: sorted, deduplicated, never
    /// empty.
    pub projects: Vec<String>,
    /// As written.
    pub prompt: String,
    /// Each parameter's closed list: sorted, deduplicated, never empty.
    pub params: BTreeMap<String, Vec<String>>,
    pub model: Option<ModelAlias>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why no [`ApprovedTemplate`] can be had (AD-77).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum TemplateUnavailable {
        /// The floor's catalogue is not the approved one: edited since the
        /// approval, or a project the template names is not on it.
        CatalogueChanged,
        /// The approved snapshot holds no template of that name.
        NotApproved,
    }
}

/// A template as the author approved it, with each of its projects' folders
/// on the current floor (AD-77). Its fields are private and
/// [`ApprovedTemplate::new`] is its only constructor, so a work order is
/// only ever built from the approved snapshot.
///
/// ```compile_fail,E0451
/// use std::collections::BTreeMap;
/// use officina_core::relay::template::ApprovedTemplate;
/// let template = ApprovedTemplate {
///     name: "bump".to_owned(),
///     description: "Bump".to_owned(),
///     prompt: "Run anything".to_owned(),
///     projects: BTreeMap::new(),
///     params: BTreeMap::new(),
///     model: None,
///     catalogue_hash: String::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedTemplate {
    name: String,
    description: String,
    prompt: String,
    /// Each allowed project's folder, by project name.
    projects: BTreeMap<String, String>,
    params: BTreeMap<String, Vec<String>>,
    model: Option<ModelAlias>,
    catalogue_hash: String,
}

impl ApprovedTemplate {
    /// The template `name` of `approved`, when `current` is the floor that
    /// approval read: its hash equal to the approved one, else
    /// [`TemplateUnavailable::CatalogueChanged`]. The template is read from
    /// the approved snapshot, never from `current`; each project's folder
    /// comes from `current` in its declared spelling, resolved against the
    /// floor (D-6), not the lowercased form the canonical catalogue
    /// compares.
    pub fn new(
        approved: &ApprovedCatalogue,
        current: &RelayFloor,
        name: &TemplateName,
    ) -> Result<ApprovedTemplate, TemplateUnavailable> {
        if current.hash != approved.hash {
            return Err(TemplateUnavailable::CatalogueChanged);
        }
        let template = approved
            .canonical
            .templates
            .get(name.as_str())
            .ok_or(TemplateUnavailable::NotApproved)?;
        let folders = project_dirs(&current.floor, &current.floor_path);
        let projects = template
            .projects
            .iter()
            .map(|project| {
                folders
                    .get(project)
                    .map(|folder| (project.clone(), folder.clone()))
                    .ok_or(TemplateUnavailable::CatalogueChanged)
            })
            .collect::<Result<_, _>>()?;
        Ok(ApprovedTemplate {
            name: name.as_str().to_owned(),
            description: template.description.clone(),
            prompt: template.prompt.clone(),
            projects,
            params: template.params.clone(),
            model: template.model,
            catalogue_hash: approved.hash.clone(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// The prompt as the author wrote it, placeholders included.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The folder of `project` when the template may run there.
    pub fn project_folder(&self, project: &ProjectName) -> Option<&str> {
        self.projects.get(project.as_str()).map(String::as_str)
    }

    /// The projects it may run in, sorted.
    pub fn projects(&self) -> impl Iterator<Item = &str> {
        self.projects.keys().map(String::as_str)
    }

    /// Each parameter's closed list, sorted.
    pub fn params(&self) -> &BTreeMap<String, Vec<String>> {
        &self.params
    }

    pub fn model(&self) -> Option<ModelAlias> {
        self.model
    }

    /// The approved catalogue's hash, which the current floor's equalled.
    pub fn catalogue_hash(&self) -> &str {
        &self.catalogue_hash
    }
}

/// An approved floor with one template, for the tests of this module, of
/// `orders` and of `spawn`.
#[cfg(test)]
pub(crate) mod sample {
    use super::{ApprovedTemplate, TemplateName};
    use crate::relay::catalogue::{ApprovedCatalogue, RelayFloor, load};

    pub const FLOOR_PATH: &str = "C:/Path/To/Floor/officina.toml";

    pub const TEMPLATE: &str = "dependency-bump";

    /// Three projects, two of them the template's: one relative to the
    /// floor and one absolute, both in mixed case.
    pub const FLOOR: &str = r#"schema = 2
[[project]]
name = "backend"
path = "Work/Backend"
[[project]]
name = "Docs Site"
path = "D:/Repos/Docs-Site"
[[project]]
name = "lab"
path = "Lab"
[[template]]
name = "dependency-bump"
description = "Bump one dependency"
projects = ["backend", "Docs Site"]
prompt = "Bump {package} to its latest {level} release."
model = "sonnet"
[template.params.package]
values = ["serde_json", "@scope/pkg"]
[template.params.level]
values = ["patch", "minor"]
"#;

    pub fn current() -> RelayFloor {
        let relay = load(FLOOR, FLOOR_PATH).unwrap();
        assert!(
            relay.catalogue.issues.is_empty(),
            "{:?}",
            relay.catalogue.issues
        );
        relay
    }

    pub fn approved() -> ApprovedCatalogue {
        ApprovedCatalogue::approving(&current(), FLOOR_PATH, 0)
    }

    pub fn name() -> TemplateName {
        TemplateName::parse(TEMPLATE).unwrap()
    }

    pub fn template() -> ApprovedTemplate {
        ApprovedTemplate::new(&approved(), &current(), &name()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::catalogue::{MAX_NAME_CHARS, load};

    #[test]
    fn a_floor_edited_since_approval_is_catalogue_changed() {
        let approved = sample::approved();
        let edited = sample::FLOOR.replace("its latest", "the latest");
        let current = load(&edited, sample::FLOOR_PATH).unwrap();
        assert_eq!(
            ApprovedTemplate::new(&approved, &current, &sample::name()),
            Err(TemplateUnavailable::CatalogueChanged)
        );
        // A comment changes no meaning, so the approval holds.
        let commented = format!("# the author's note\n{}", sample::FLOOR);
        let current = load(&commented, sample::FLOOR_PATH).unwrap();
        assert!(ApprovedTemplate::new(&approved, &current, &sample::name()).is_ok());
        // A snapshot naming a project the floor does not hold is not what
        // the floor approved, whatever hash it states.
        let mut tampered = sample::approved();
        tampered
            .canonical
            .templates
            .get_mut(sample::TEMPLATE)
            .unwrap()
            .projects
            .push("elsewhere".to_owned());
        assert_eq!(
            ApprovedTemplate::new(&tampered, &sample::current(), &sample::name()),
            Err(TemplateUnavailable::CatalogueChanged)
        );
    }

    #[test]
    fn a_template_absent_from_the_snapshot_is_not_approved() {
        let other = TemplateName::parse("other-template").unwrap();
        assert_eq!(
            ApprovedTemplate::new(&sample::approved(), &sample::current(), &other),
            Err(TemplateUnavailable::NotApproved)
        );
        // Read from the snapshot, never from the floor: the floor holds the
        // template, the snapshot (its hash unchanged) does not.
        let mut approved = sample::approved();
        approved.canonical.templates.clear();
        assert_eq!(
            ApprovedTemplate::new(&approved, &sample::current(), &sample::name()),
            Err(TemplateUnavailable::NotApproved)
        );
        // What the snapshot says is what the template holds.
        let mut approved = sample::approved();
        approved
            .canonical
            .templates
            .get_mut(sample::TEMPLATE)
            .unwrap()
            .prompt = "Approved {package} {level}.".to_owned();
        let template = ApprovedTemplate::new(&approved, &sample::current(), &sample::name());
        assert_eq!(template.unwrap().prompt(), "Approved {package} {level}.");
    }

    #[test]
    fn project_folders_keep_their_declared_spelling() {
        let template = sample::template();
        let folder = |name: &str| {
            template
                .project_folder(&ProjectName::parse(name).unwrap())
                .map(str::to_owned)
        };
        assert_eq!(
            folder("backend").as_deref(),
            Some("C:/Path/To/Floor/Work/Backend")
        );
        assert_eq!(folder("Docs Site").as_deref(), Some("D:/Repos/Docs-Site"));
        assert_eq!(folder("lab"), None, "not one of the template's projects");
        // The canonical form compares a lowercased path; the folder is not
        // it.
        let canonical = sample::current().canonical.projects["backend"].path.clone();
        assert_eq!(canonical, "c:/path/to/floor/work/backend");
        assert_ne!(folder("backend"), Some(canonical));
        assert_eq!(
            template.projects().collect::<Vec<_>>(),
            ["Docs Site", "backend"]
        );
        assert_eq!(template.catalogue_hash(), sample::approved().hash);
        assert_eq!(template.model(), Some(ModelAlias::Sonnet));
        assert_eq!(template.description(), "Bump one dependency");
    }

    #[test]
    fn every_unavailable_reason_has_a_case() {
        for reason in TemplateUnavailable::ALL {
            let (approved, current, name) = match reason {
                TemplateUnavailable::CatalogueChanged => (
                    sample::approved(),
                    load(&sample::FLOOR.replace("patch", "major"), sample::FLOOR_PATH).unwrap(),
                    sample::name(),
                ),
                TemplateUnavailable::NotApproved => (
                    sample::approved(),
                    sample::current(),
                    TemplateName::parse("missing").unwrap(),
                ),
            };
            assert_eq!(
                ApprovedTemplate::new(&approved, &current, &name),
                Err(*reason),
                "{reason:?}"
            );
        }
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn names_follow_the_catalogue_rules() {
        for valid in ["bump", "dependency-bump", &"a".repeat(MAX_NAME_CHARS)] {
            assert_eq!(
                TemplateName::parse(valid).map(|n| n.as_str().to_owned()),
                Some(valid.to_owned())
            );
        }
        for invalid in ["", "Bump", "2a", "a_b", &"a".repeat(MAX_NAME_CHARS + 1)] {
            assert!(TemplateName::parse(invalid).is_none(), "{invalid}");
            assert_eq!(
                TemplateName::parse(invalid).is_some(),
                is_valid_action_name(invalid)
            );
        }
        for (name, valid) in [
            ("package", true),
            ("play_book-2", true),
            ("Pkg", false),
            ("", false),
        ] {
            assert_eq!(ParamName::parse(name).is_some(), valid, "{name}");
            assert_eq!(ParamName::parse(name).is_some(), is_valid_param_name(name));
        }
        for (name, valid) in [
            ("backend", true),
            ("Docs Site", true),
            ("", false),
            ("a\nb", false),
            ("a\u{202E}b", false),
        ] {
            assert_eq!(ProjectName::parse(name).is_some(), valid, "{name:?}");
        }
    }

    #[test]
    fn render_replaces_each_placeholder_once() {
        let rendered = render(
            "Bump {package} to its latest {level} release; {package} again.",
            &values(&[("package", "serde_json"), ("level", "minor")]),
        );
        assert_eq!(
            rendered.as_deref(),
            Some("Bump serde_json to its latest minor release; serde_json again.")
        );
        // A value that looks like a placeholder stays as it is.
        assert_eq!(
            render("x {a} y", &values(&[("a", "{b}"), ("b", "no")])).as_deref(),
            Some("x {b} y")
        );
        assert_eq!(render("x {a}", &values(&[])), None, "a missing value");
        assert_eq!(render("x {a", &values(&[("a", "1")])), None);
        assert_eq!(
            render("no placeholder", &values(&[])).as_deref(),
            Some("no placeholder")
        );
    }

    #[test]
    fn braces_delimit_placeholders_only() {
        assert_eq!(pieces("a}"), Err(PromptBraceError::StrayClose(1)));
        assert_eq!(pieces("é{a"), Err(PromptBraceError::Unclosed(1)));
        assert_eq!(pieces("{a{b}}"), Err(PromptBraceError::Nested(2)));
        assert_eq!(
            pieces("{a}{b}").unwrap(),
            [Piece::Placeholder("a"), Piece::Placeholder("b")]
        );
        assert_eq!(pieces("{}").unwrap(), [Piece::Placeholder("")]);
        assert!(pieces("").unwrap().is_empty());
    }

    #[test]
    fn worst_case_uses_each_longest_value() {
        let params = BTreeMap::from([
            (
                "package".to_owned(),
                vec!["a".to_owned(), "serde_json".to_owned()],
            ),
            ("level".to_owned(), vec!["minor".to_owned()]),
        ]);
        let prompt = "Bump {package} ({level}), then {package}.";
        let longest = render(
            prompt,
            &values(&[("package", "serde_json"), ("level", "minor")]),
        )
        .unwrap();
        assert_eq!(
            worst_case_chars(prompt, &params),
            Some(longest.chars().count())
        );
        assert_eq!(worst_case_chars("é {level}", &params), Some(7));
        assert_eq!(worst_case_chars("{other}", &params), None);
        assert_eq!(worst_case_chars("{level", &params), None);
    }

    #[test]
    fn model_alias_round_trips_its_text() {
        for alias in ModelAlias::ALL {
            let text = serde_json::to_string(alias).unwrap();
            assert_eq!(text, format!("\"{}\"", alias.code()));
            assert_eq!(serde_json::from_str::<ModelAlias>(&text).unwrap(), *alias);
            assert_eq!(ModelAlias::from_code(alias.code()), Some(*alias));
            assert_eq!(ModelAlias::from_code(&alias.code().to_uppercase()), None);
        }
        assert!(ModelAlias::ALL.contains(&DEFAULT_REVIEW_MODEL));
    }
}
