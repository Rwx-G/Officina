//! Closed parameters (FR-3, AD-30): the rules every value meets, whatever its
//! parameter accepts, then the parameter's own `values` or `pattern`.
//!
//! The value rules take the runner (D-5): one function serves values passed
//! to `officina run` and the `values` entries checked at validation, so the
//! two can never disagree.

use std::ops::RangeInclusive;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use super::catalogue::Runner;

/// Length of a value, in characters (P-2).
pub const VALUE_LENGTH: RangeInclusive<usize> = 1..=256;

/// Longest `pattern` source, in characters (AD-23).
pub const MAX_PATTERN_CHARS: usize = 256;

/// Compiled size limit of a `pattern` (AD-23).
pub const PATTERN_SIZE_LIMIT: usize = 1 << 20;

/// Invisible and bidirectional formatting characters a value may not hold:
/// they make the text the author reads in an approval differ from the bytes
/// a program receives. This is wider than AD-30's examples: it adds the
/// Arabic letter mark (a bidi control), the invisible operators, the
/// deprecated format controls, the soft hyphen, the Mongolian vowel
/// separator, the interlinear annotation controls and the tag characters,
/// which can carry hidden ASCII.
pub const FORBIDDEN_FORMAT_RANGES: [RangeInclusive<char>; 10] = [
    '\u{00AD}'..='\u{00AD}',
    '\u{061C}'..='\u{061C}',
    '\u{180E}'..='\u{180E}',
    '\u{200B}'..='\u{200F}',
    '\u{202A}'..='\u{202E}',
    '\u{2060}'..='\u{2064}',
    '\u{2066}'..='\u{206F}',
    '\u{FEFF}'..='\u{FEFF}',
    '\u{FFF9}'..='\u{FFFB}',
    '\u{E0000}'..='\u{E007F}',
];

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a value was refused. Each variant is a rule of AD-30 or D-5, or a
    /// parameter's own constraint.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ValueRefusal {
        /// No character at all.
        Empty,
        /// Over [`VALUE_LENGTH`] characters.
        TooLong,
        /// Starts with `-`: it would become an option.
        LeadingDash,
        /// Starts with `/` or `@` on the `native` runner: a Windows switch or
        /// a response file (D-5).
        NativeSwitch,
        /// Holds a control character.
        ControlCharacter,
        /// Holds a character of [`FORBIDDEN_FORMAT_RANGES`].
        FormatCharacter,
        /// Has a `.` or `..` segment when split on `/` and `\`.
        DotSegment,
        /// Not one of the parameter's `values`.
        NotInValues,
        /// Does not match the parameter's `pattern`.
        PatternMismatch,
    }
}

impl std::fmt::Display for ValueRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Empty => "the value is empty",
            Self::TooLong => {
                return write!(
                    f,
                    "the value is longer than {} characters",
                    VALUE_LENGTH.end()
                );
            }
            Self::LeadingDash => "the value starts with `-`",
            Self::NativeSwitch => "the value starts with `/` or `@`",
            Self::ControlCharacter => "the value holds a control character",
            Self::FormatCharacter => {
                "the value holds an invisible or bidirectional formatting character"
            }
            Self::DotSegment => "the value has a `.` or `..` path segment",
            Self::NotInValues => "the value is not one of the declared values",
            Self::PatternMismatch => "the value does not match the declared pattern",
        };
        f.write_str(text)
    }
}

/// What a parameter accepts once its value passed the rules.
#[derive(Debug, Clone)]
pub enum ParamRule {
    /// One of these, sorted.
    Values(Vec<String>),
    /// A match of the whole value.
    Pattern(Pattern),
}

/// One declared parameter of a validated action.
#[derive(Debug, Clone)]
pub struct Param {
    pub rule: ParamRule,
}

/// A compiled, anchored `pattern` with the source the author wrote.
#[derive(Debug, Clone)]
pub struct Pattern {
    source: String,
    regex: Regex,
}

impl Pattern {
    /// The pattern as declared, before anchoring.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// True when the whole value matches.
    pub fn matches(&self, value: &str) -> bool {
        self.regex.is_match(value)
    }
}

/// Why a `pattern` cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("the pattern is longer than {MAX_PATTERN_CHARS} characters")]
    TooLong,
    #[error("the pattern compiles past the size limit of {PATTERN_SIZE_LIMIT} bytes")]
    TooComplex,
    #[error("the pattern does not compile: {0}")]
    Invalid(String),
}

/// Compiles a `pattern` as `^(?:...)$` within the size limit (AD-23). Only
/// phase 2 calls this: the gate never compiles a regex (AD-28).
///
/// ```
/// let p = officina_core::relay::params::compile_pattern("lab-[a-z]+").unwrap();
/// assert!(p.matches("lab-web") && !p.matches("xlab-web") && !p.matches("lab-web;"));
/// ```
pub fn compile_pattern(source: &str) -> Result<Pattern, PatternError> {
    if source.chars().count() > MAX_PATTERN_CHARS {
        return Err(PatternError::TooLong);
    }
    let regex = RegexBuilder::new(&format!("^(?:{source})$"))
        .size_limit(PATTERN_SIZE_LIMIT)
        .build()
        .map_err(|e| match e {
            regex::Error::CompiledTooBig(_) => PatternError::TooComplex,
            other => PatternError::Invalid(other.to_string()),
        })?;
    Ok(Pattern {
        source: source.to_owned(),
        regex,
    })
}

/// True when `c` is in [`FORBIDDEN_FORMAT_RANGES`].
pub fn is_forbidden_format(c: char) -> bool {
    FORBIDDEN_FORMAT_RANGES
        .iter()
        .any(|range| range.contains(&c))
}

/// The value rules of AD-30 and D-5, which hold whatever the parameter
/// accepts.
///
/// ```
/// use officina_core::relay::catalogue::Runner;
/// use officina_core::relay::params::{check_value, ValueRefusal};
/// assert_eq!(check_value(Runner::Native, "lab-web"), Ok(()));
/// assert_eq!(check_value(Runner::Native, "--force"), Err(ValueRefusal::LeadingDash));
/// assert_eq!(check_value(Runner::Native, "/S"), Err(ValueRefusal::NativeSwitch));
/// assert_eq!(check_value(Runner::Wsl, "/srv/data"), Ok(()));
/// ```
pub fn check_value(runner: Runner, value: &str) -> Result<(), ValueRefusal> {
    check_value_for(ValueTarget::from(runner), value)
}

/// Where a value goes, which decides whether D-5's rule applies: only a
/// `native` program reads a leading `/` or `@` as a switch. An HTTP action's
/// values are percent-encoded into its URL (AD-36), so they meet the AD-30
/// rules alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueTarget {
    Native,
    Wsl,
    Http,
}

impl From<Runner> for ValueTarget {
    fn from(runner: Runner) -> ValueTarget {
        match runner {
            Runner::Native => ValueTarget::Native,
            Runner::Wsl => ValueTarget::Wsl,
        }
    }
}

/// [`check_value`] for any [`ValueTarget`].
///
/// ```
/// use officina_core::relay::params::{check_value_for, ValueRefusal, ValueTarget};
/// assert_eq!(check_value_for(ValueTarget::Http, "/srv"), Ok(()));
/// assert_eq!(check_value_for(ValueTarget::Http, ".."), Err(ValueRefusal::DotSegment));
/// ```
pub fn check_value_for(target: ValueTarget, value: &str) -> Result<(), ValueRefusal> {
    check_shape(value)?;
    if target == ValueTarget::Native && value.starts_with(['/', '@']) {
        return Err(ValueRefusal::NativeSwitch);
    }
    check_characters(value)
}

/// AD-30's rules, which hold whatever reads the value: [`check_value_for`]
/// without the native runner's refusal of a leading `/` or `@` (D-5).
///
/// ```
/// use officina_core::relay::params::{check_runner_independent, ValueRefusal};
/// assert_eq!(check_runner_independent("@scope/pkg"), Ok(()));
/// assert_eq!(check_runner_independent("-x"), Err(ValueRefusal::LeadingDash));
/// ```
pub fn check_runner_independent(value: &str) -> Result<(), ValueRefusal> {
    check_shape(value)?;
    check_characters(value)
}

/// The length and the leading `-`, checked before D-5's rule so a value
/// that breaks both is refused as it always was.
fn check_shape(value: &str) -> Result<(), ValueRefusal> {
    let length = value.chars().count();
    if length < *VALUE_LENGTH.start() {
        return Err(ValueRefusal::Empty);
    }
    if length > *VALUE_LENGTH.end() {
        return Err(ValueRefusal::TooLong);
    }
    if value.starts_with('-') {
        return Err(ValueRefusal::LeadingDash);
    }
    Ok(())
}

/// Control and formatting characters, and `.` or `..` path segments.
fn check_characters(value: &str) -> Result<(), ValueRefusal> {
    if value.chars().any(char::is_control) {
        return Err(ValueRefusal::ControlCharacter);
    }
    if value.chars().any(is_forbidden_format) {
        return Err(ValueRefusal::FormatCharacter);
    }
    if value.split(['/', '\\']).any(|s| s == "." || s == "..") {
        return Err(ValueRefusal::DotSegment);
    }
    Ok(())
}

/// The value rules, then the parameter's `values` or `pattern`.
pub fn accept(param: &Param, runner: Runner, value: &str) -> Result<(), ValueRefusal> {
    accept_for(param, ValueTarget::from(runner), value)
}

/// [`accept`] for any [`ValueTarget`].
pub fn accept_for(param: &Param, target: ValueTarget, value: &str) -> Result<(), ValueRefusal> {
    check_value_for(target, value)?;
    match &param.rule {
        ParamRule::Values(values) if values.iter().any(|v| v == value) => Ok(()),
        ParamRule::Values(_) => Err(ValueRefusal::NotInValues),
        ParamRule::Pattern(pattern) if pattern.matches(value) => Ok(()),
        ParamRule::Pattern(_) => Err(ValueRefusal::PatternMismatch),
    }
}

/// Longest template value, in characters (FR-2).
pub const TEMPLATE_VALUE_MAX_CHARS: usize = 64;

/// What a template value may hold besides ASCII letters and digits (FR-2):
/// no space, nothing a prompt or a shell reads as structure.
pub const TEMPLATE_VALUE_PUNCTUATION: &str = "._:/@+-";

/// Why a template value was refused (FR-2, AD-75).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TemplateValueRefusal {
    #[error("the value is longer than {TEMPLATE_VALUE_MAX_CHARS} characters")]
    TooLong,
    #[error(
        "the value holds {0:?}: a template value holds ASCII letters, digits and `{TEMPLATE_VALUE_PUNCTUATION}` only"
    )]
    Character(char),
    #[error("{0}")]
    Rule(ValueRefusal),
}

/// A template value (FR-2, AD-75): at most [`TEMPLATE_VALUE_MAX_CHARS`]
/// characters of ASCII letters, digits and [`TEMPLATE_VALUE_PUNCTUATION`],
/// then [`check_runner_independent`]. It lands in a prompt, never in an
/// argument vector, so D-5's native refusal of a leading `/` or `@` does
/// not apply.
///
/// ```
/// use officina_core::relay::params::{template_value, TemplateValueRefusal};
/// assert_eq!(template_value("@scope/pkg"), Ok(()));
/// assert_eq!(template_value("a b"), Err(TemplateValueRefusal::Character(' ')));
/// ```
pub fn template_value(value: &str) -> Result<(), TemplateValueRefusal> {
    if value.chars().count() > TEMPLATE_VALUE_MAX_CHARS {
        return Err(TemplateValueRefusal::TooLong);
    }
    if let Some(c) = value
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && !TEMPLATE_VALUE_PUNCTUATION.contains(*c))
    {
        return Err(TemplateValueRefusal::Character(c));
    }
    check_runner_independent(value).map_err(TemplateValueRefusal::Rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(list: &[&str]) -> Param {
        Param {
            rule: ParamRule::Values(list.iter().map(|v| (*v).to_owned()).collect()),
        }
    }

    fn pattern(source: &str) -> Param {
        Param {
            rule: ParamRule::Pattern(compile_pattern(source).unwrap()),
        }
    }

    #[test]
    fn length_is_counted_in_characters() {
        assert_eq!(check_value(Runner::Native, ""), Err(ValueRefusal::Empty));
        let longest = "é".repeat(*VALUE_LENGTH.end());
        assert_eq!(check_value(Runner::Native, &longest), Ok(()));
        assert_eq!(
            check_value(Runner::Native, &format!("{longest}a")),
            Err(ValueRefusal::TooLong)
        );
    }

    #[test]
    fn leading_dash_is_refused_on_both_runners() {
        for runner in Runner::ALL {
            assert_eq!(
                check_value(*runner, "-i"),
                Err(ValueRefusal::LeadingDash),
                "{runner:?}"
            );
            assert_eq!(check_value(*runner, "a-i"), Ok(()), "{runner:?}");
        }
    }

    #[test]
    fn switches_and_response_files_are_refused_on_native_only() {
        for value in ["/S", "@args.txt"] {
            assert_eq!(
                check_value(Runner::Native, value),
                Err(ValueRefusal::NativeSwitch)
            );
            assert_eq!(check_value(Runner::Wsl, value), Ok(()));
        }
        assert_eq!(check_value(Runner::Native, "a/S"), Ok(()));
    }

    #[test]
    fn control_characters_are_refused() {
        for value in [
            "a\0b",
            "a\nb",
            "a\rb",
            "a\tb",
            "a\u{7f}",
            "a\u{85}b",
            "a\u{1b}[31m",
        ] {
            assert_eq!(
                check_value(Runner::Wsl, value),
                Err(ValueRefusal::ControlCharacter),
                "{value:?}"
            );
        }
    }

    #[test]
    fn every_forbidden_range_bound_is_refused() {
        for range in &FORBIDDEN_FORMAT_RANGES {
            for bound in [*range.start(), *range.end()] {
                assert_eq!(
                    check_value(Runner::Native, &format!("a{bound}b")),
                    Err(ValueRefusal::FormatCharacter),
                    "U+{:04X}",
                    u32::from(bound)
                );
            }
            let outside = [
                char::from_u32(u32::from(*range.start()) - 1),
                char::from_u32(u32::from(*range.end()) + 1),
            ];
            for c in outside.into_iter().flatten() {
                assert_eq!(
                    check_value(Runner::Native, &format!("a{c}b")),
                    Ok(()),
                    "U+{:04X}",
                    u32::from(c)
                );
            }
        }
    }

    #[test]
    fn dot_segments_are_refused_on_both_separators() {
        for value in [
            ".", "..", "a/../b", r"a\..\b", "a/.", r".\a", "../", r"x\..",
        ] {
            assert_eq!(
                check_value(Runner::Wsl, value),
                Err(ValueRefusal::DotSegment),
                "{value}"
            );
        }
        for value in ["...", "a..b", ".hidden", "a/.b", "v1.2"] {
            assert_eq!(check_value(Runner::Wsl, value), Ok(()), "{value}");
        }
    }

    #[test]
    fn values_membership_is_exact() {
        let param = values(&["alerting.yml", "node-exporter.yml"]);
        assert_eq!(accept(&param, Runner::Wsl, "alerting.yml"), Ok(()));
        for other in ["Alerting.yml", "alerting.yml ", "alerting"] {
            assert_eq!(
                accept(&param, Runner::Wsl, other),
                Err(ValueRefusal::NotInValues),
                "{other}"
            );
        }
    }

    #[test]
    fn rules_apply_before_membership() {
        let param = values(&["-x"]);
        assert_eq!(
            accept(&param, Runner::Native, "-x"),
            Err(ValueRefusal::LeadingDash)
        );
        let any = pattern(".*");
        assert_eq!(
            accept(&any, Runner::Native, "a/../b"),
            Err(ValueRefusal::DotSegment)
        );
    }

    #[test]
    fn pattern_is_anchored() {
        let param = pattern("lab-[a-z0-9-]{1,30}");
        assert_eq!(accept(&param, Runner::Wsl, "lab-web"), Ok(()));
        for unanchored in ["xlab-web", "lab-web;id", "lab-web\u{2028}", "lab-"] {
            assert_eq!(
                accept(&param, Runner::Wsl, unanchored),
                Err(ValueRefusal::PatternMismatch),
                "{unanchored}"
            );
        }
        let alternation = pattern("a|b");
        assert_eq!(
            accept(&alternation, Runner::Wsl, "ab"),
            Err(ValueRefusal::PatternMismatch),
            "the group keeps an alternation inside the anchors"
        );
        assert_eq!(alternation.rule_source(), Some("a|b"));
    }

    #[test]
    fn oversized_pattern_is_an_issue() {
        let too_long = "a".repeat(MAX_PATTERN_CHARS + 1);
        assert_eq!(
            compile_pattern(&too_long).unwrap_err(),
            PatternError::TooLong
        );
        assert!(compile_pattern(&"a".repeat(MAX_PATTERN_CHARS)).is_ok());
        let too_big = "(?:a{1000}){1000}";
        assert!(too_big.len() < MAX_PATTERN_CHARS);
        assert_eq!(
            compile_pattern(too_big).unwrap_err(),
            PatternError::TooComplex
        );
    }

    #[test]
    fn invalid_pattern_is_an_issue() {
        for broken in ["(", "[a-", "a{2,1}", r"\p{NoSuchClass}"] {
            assert!(
                matches!(compile_pattern(broken), Err(PatternError::Invalid(_))),
                "{broken}"
            );
        }
    }

    /// Story 5.4 AC 4: a template value lands in a prompt, so a scoped
    /// package name is one, every character of the set included.
    #[test]
    fn template_value_accepts_a_scoped_package() {
        for value in ["@scope/pkg", "serde_json", "tokio-util", "v1.2.3+build:4"] {
            assert_eq!(template_value(value), Ok(()), "{value}");
        }
        let every_character: String = TEMPLATE_VALUE_PUNCTUATION
            .chars()
            .filter(|c| *c != '-')
            .chain(['a', 'Z', '0'])
            .collect();
        assert_eq!(template_value(&every_character), Ok(()));
        assert_eq!(
            template_value(&"a".repeat(TEMPLATE_VALUE_MAX_CHARS)),
            Ok(())
        );
    }

    #[test]
    fn template_value_refuses_a_space_and_65_characters() {
        assert_eq!(
            template_value("two words"),
            Err(TemplateValueRefusal::Character(' '))
        );
        assert_eq!(
            template_value(&"a".repeat(TEMPLATE_VALUE_MAX_CHARS + 1)),
            Err(TemplateValueRefusal::TooLong)
        );
        for c in ['{', '}', ';', '$', '\'', '"', '\n', '\u{200B}', 'é'] {
            assert_eq!(
                template_value(&format!("a{c}b")),
                Err(TemplateValueRefusal::Character(c)),
                "{c:?}"
            );
        }
    }

    #[test]
    fn template_value_refuses_a_leading_dash_and_a_dot_segment() {
        assert_eq!(
            template_value("-x"),
            Err(TemplateValueRefusal::Rule(ValueRefusal::LeadingDash))
        );
        for value in ["..", "a/../b", "./a"] {
            assert_eq!(
                template_value(value),
                Err(TemplateValueRefusal::Rule(ValueRefusal::DotSegment)),
                "{value}"
            );
        }
        assert_eq!(
            template_value(""),
            Err(TemplateValueRefusal::Rule(ValueRefusal::Empty))
        );
    }

    /// The split keeps D-5 on argument vectors: a native argument still
    /// refuses what a template value accepts.
    #[test]
    fn native_argument_values_still_refuse_a_leading_slash_or_at() {
        for value in ["/S", "@scope/pkg"] {
            assert_eq!(template_value(value), Ok(()), "{value}");
            assert_eq!(check_runner_independent(value), Ok(()), "{value}");
            assert_eq!(
                check_value(Runner::Native, value),
                Err(ValueRefusal::NativeSwitch),
                "{value}"
            );
            assert_eq!(check_value(Runner::Wsl, value), Ok(()), "{value}");
        }
        assert_eq!(
            check_value(Runner::Native, "/a\u{7}"),
            Err(ValueRefusal::NativeSwitch),
            "D-5 still comes before the character rules"
        );
    }

    impl Param {
        fn rule_source(&self) -> Option<&str> {
            match &self.rule {
                ParamRule::Pattern(p) => Some(p.source()),
                ParamRule::Values(_) => None,
            }
        }
    }
}
