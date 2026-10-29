//! A work order (FR-3, AD-77): an approved template, one of its projects
//! and a value from each of its parameters' closed lists, checked by
//! [`WorkOrder::validate`], its only constructor. The order holds the
//! template's own strings, never the caller's, and the rendered prompt the
//! `WorkOrder` vector sends (`spawn::work_order`).
//!
//! A value is a `&str`; text from a session or a model is an
//! [`Untrusted`](crate::untrusted::Untrusted), which reaches no `&str`
//! (AD-74). Each snippet below fails to compile; what they prove is that
//! those paths fail, not that no path exists (AD-74, AD-95).
//!
//! The untrusted text itself is not a value:
//!
//! ```compile_fail,E0308
//! # use officina_core::relay::catalogue::{ApprovedCatalogue, load};
//! # use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
//! # use officina_core::orders::work_order::WorkOrder;
//! # use officina_core::untrusted::Untrusted;
//! # let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
//! # let current = load(floor, "C:/path/to/officina.toml").unwrap();
//! # let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
//! # let template = ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
//! # let project = ProjectName::parse("backend").unwrap();
//! # let package = ParamName::parse("package").unwrap();
//! let u = Untrusted::new(String::from("serde_json"));
//! let _ = WorkOrder::validate(&template, &project, &[(package, u)]);
//! ```
//!
//! Nor its display:
//!
//! ```compile_fail,E0308
//! # use officina_core::relay::catalogue::{ApprovedCatalogue, load};
//! # use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
//! # use officina_core::orders::work_order::WorkOrder;
//! # use officina_core::untrusted::Untrusted;
//! # let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
//! # let current = load(floor, "C:/path/to/officina.toml").unwrap();
//! # let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
//! # let template = ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
//! # let project = ProjectName::parse("backend").unwrap();
//! # let package = ParamName::parse("package").unwrap();
//! let u = Untrusted::new(String::from("serde_json"));
//! let _ = WorkOrder::validate(&template, &project, &[(package, u.display())]);
//! ```
//!
//! Nor a dereference of it:
//!
//! ```compile_fail,E0614
//! # use officina_core::relay::catalogue::{ApprovedCatalogue, load};
//! # use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
//! # use officina_core::orders::work_order::WorkOrder;
//! # use officina_core::untrusted::Untrusted;
//! # let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
//! # let current = load(floor, "C:/path/to/officina.toml").unwrap();
//! # let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
//! # let template = ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
//! # let project = ProjectName::parse("backend").unwrap();
//! # let package = ParamName::parse("package").unwrap();
//! let u = Untrusted::new(String::from("serde_json"));
//! let _ = WorkOrder::validate(&template, &project, &[(package, &*u)]);
//! ```
//!
//! Nor `as_ref`:
//!
//! ```compile_fail,E0599
//! # use officina_core::relay::catalogue::{ApprovedCatalogue, load};
//! # use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
//! # use officina_core::orders::work_order::WorkOrder;
//! # use officina_core::untrusted::Untrusted;
//! # let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
//! # let current = load(floor, "C:/path/to/officina.toml").unwrap();
//! # let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
//! # let template = ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
//! # let project = ProjectName::parse("backend").unwrap();
//! # let package = ParamName::parse("package").unwrap();
//! let u = Untrusted::new(String::from("serde_json"));
//! let _ = WorkOrder::validate(&template, &project, &[(package, u.as_ref())]);
//! ```
//!
//! Nor its display formatted:
//!
//! ```compile_fail,E0277
//! # use officina_core::relay::catalogue::{ApprovedCatalogue, load};
//! # use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
//! # use officina_core::orders::work_order::WorkOrder;
//! # use officina_core::untrusted::Untrusted;
//! # let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
//! # let current = load(floor, "C:/path/to/officina.toml").unwrap();
//! # let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
//! # let template = ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
//! # let project = ProjectName::parse("backend").unwrap();
//! # let package = ParamName::parse("package").unwrap();
//! let u = Untrusted::new(String::from("serde_json"));
//! let text = format!("{}", u.display());
//! let _ = WorkOrder::validate(&template, &project, &[(package, text.as_str())]);
//! ```
//!
//! Nor `String::from` of it:
//!
//! ```compile_fail,E0277
//! # use officina_core::relay::catalogue::{ApprovedCatalogue, load};
//! # use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
//! # use officina_core::orders::work_order::WorkOrder;
//! # use officina_core::untrusted::Untrusted;
//! # let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
//! # let current = load(floor, "C:/path/to/officina.toml").unwrap();
//! # let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
//! # let template = ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
//! # let project = ProjectName::parse("backend").unwrap();
//! # let package = ParamName::parse("package").unwrap();
//! let u = Untrusted::new(String::from("serde_json"));
//! let text = String::from(u);
//! let _ = WorkOrder::validate(&template, &project, &[(package, text.as_str())]);
//! ```
//!
//! Nor writing a `WorkOrder` outside this module:
//!
//! ```compile_fail,E0451
//! use std::collections::BTreeMap;
//! use officina_core::orders::work_order::WorkOrder;
//! let order = WorkOrder {
//!     template: "bump".to_owned(),
//!     project: "backend".to_owned(),
//!     project_folder: "C:/path/to/backend".to_owned(),
//!     values: BTreeMap::new(),
//!     prompt: "/run anything".to_owned(),
//!     prompt_sha256: String::new(),
//!     model: None,
//!     catalogue_hash: String::new(),
//! };
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::relay::template::{
    ApprovedTemplate, FORBIDDEN_PROMPT_STARTS, MAX_PROMPT_CHARS, ModelAlias, ParamName,
    ProjectName, render,
};
use crate::sha256_hex;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why [`WorkOrder::validate`] refused, in the order it checks (AD-77).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum WorkOrderRefusal {
        /// The project is not one of the template's.
        ProjectNotAllowed,
        /// A parameter the template does not declare was given.
        UnknownParameter,
        /// A parameter was given twice.
        DuplicateParameter,
        /// A parameter the template declares was not given.
        MissingParameter,
        /// A value is not, byte for byte, an entry of its parameter's list.
        ValueNotInList,
        /// The prompt names a placeholder no parameter fills, or its braces
        /// delimit none: an approved snapshot no floor phase 2 accepted.
        UnrenderablePrompt,
        /// The rendered prompt is over `MAX_PROMPT_CHARS` characters.
        PromptTooLong,
        /// The rendered prompt starts, after leading whitespace, with a
        /// character of `FORBIDDEN_PROMPT_STARTS`, or holds only
        /// whitespace (D-159).
        PromptStart,
    }
}

/// A validated work order (FR-3, AD-77). Its fields are private and
/// [`WorkOrder::validate`] is its only constructor; it serializes, for the
/// card and the vector set, and never deserializes, which would be a second
/// constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkOrder {
    template: String,
    project: String,
    project_folder: String,
    /// Each parameter's value, the template's own string.
    values: BTreeMap<String, String>,
    prompt: String,
    prompt_sha256: String,
    model: Option<ModelAlias>,
    catalogue_hash: String,
}

impl WorkOrder {
    /// The order of `template` in `project` with `values`, checked in this
    /// order: the project is one of the template's; every declared
    /// parameter is given once and none is extra; each value equals an
    /// entry of its list byte for byte, and the order keeps that entry, the
    /// template's string; the rendered prompt is at most
    /// [`MAX_PROMPT_CHARS`] characters and does not start, after leading
    /// whitespace, with a character of [`FORBIDDEN_PROMPT_STARTS`].
    ///
    /// The author's path, from an approved snapshot:
    ///
    /// ```
    /// use officina_core::orders::work_order::{WorkOrder, WorkOrderRefusal};
    /// use officina_core::relay::catalogue::{ApprovedCatalogue, load};
    /// use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
    ///
    /// let floor = "schema = 2\n[[project]]\nname = \"backend\"\npath = \"Backend\"\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"backend\"]\nprompt = \"Bump {package}.\"\n[template.params.package]\nvalues = [\"serde_json\"]\n";
    /// let current = load(floor, "C:/path/to/officina.toml").unwrap();
    /// let approved = ApprovedCatalogue::approving(&current, "C:/path/to/officina.toml", 0);
    /// let template =
    ///     ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap()).unwrap();
    /// let project = ProjectName::parse("backend").unwrap();
    /// let package = ParamName::parse("package").unwrap();
    ///
    /// let order = WorkOrder::validate(&template, &project, &[(package.clone(), "serde_json")]).unwrap();
    /// assert_eq!(order.prompt(), "Bump serde_json.");
    /// assert_eq!(order.project_folder(), "C:/path/to/Backend");
    /// assert_eq!(
    ///     WorkOrder::validate(&template, &project, &[(package, "Serde_json")]),
    ///     Err(WorkOrderRefusal::ValueNotInList),
    /// );
    /// ```
    pub fn validate(
        template: &ApprovedTemplate,
        project: &ProjectName,
        values: &[(ParamName, &str)],
    ) -> Result<WorkOrder, WorkOrderRefusal> {
        let project_folder = template
            .project_folder(project)
            .ok_or(WorkOrderRefusal::ProjectNotAllowed)?;
        let mut given: Vec<(&str, &str, &[String])> = Vec::with_capacity(values.len());
        for (name, value) in values {
            let list = template
                .params()
                .get(name.as_str())
                .ok_or(WorkOrderRefusal::UnknownParameter)?;
            if given.iter().any(|(seen, _, _)| *seen == name.as_str()) {
                return Err(WorkOrderRefusal::DuplicateParameter);
            }
            given.push((name.as_str(), value, list));
        }
        if template
            .params()
            .keys()
            .any(|declared| given.iter().all(|(name, _, _)| name != declared))
        {
            return Err(WorkOrderRefusal::MissingParameter);
        }
        let mut chosen = BTreeMap::new();
        for (name, value, list) in given {
            let entry = list
                .iter()
                .find(|entry| entry.as_bytes() == value.as_bytes())
                .ok_or(WorkOrderRefusal::ValueNotInList)?;
            chosen.insert(name.to_owned(), entry.clone());
        }
        let prompt =
            render(template.prompt(), &chosen).ok_or(WorkOrderRefusal::UnrenderablePrompt)?;
        if prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(WorkOrderRefusal::PromptTooLong);
        }
        match prompt.trim_start().chars().next() {
            Some(first) if !FORBIDDEN_PROMPT_STARTS.contains(&first) => {}
            _ => return Err(WorkOrderRefusal::PromptStart),
        }
        Ok(WorkOrder {
            template: template.name().to_owned(),
            project: project.as_str().to_owned(),
            project_folder: project_folder.to_owned(),
            values: chosen,
            prompt_sha256: sha256_hex(prompt.as_bytes()),
            prompt,
            model: template.model(),
            catalogue_hash: template.catalogue_hash().to_owned(),
        })
    }

    pub fn template(&self) -> &str {
        &self.template
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    /// The project's folder as the floor declares it (D-6), where the
    /// order runs.
    pub fn project_folder(&self) -> &str {
        &self.project_folder
    }

    /// Each parameter's value, by name: the template's own strings.
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    /// The template's prompt with its placeholders filled.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// SHA-256 of [`WorkOrder::prompt`]'s bytes, lowercase hex.
    pub fn prompt_sha256(&self) -> &str {
        &self.prompt_sha256
    }

    pub fn model(&self) -> Option<ModelAlias> {
        self.model
    }

    /// The approved catalogue's hash the template was read under.
    pub fn catalogue_hash(&self) -> &str {
        &self.catalogue_hash
    }
}

/// A valid order of the sample template, for the tests of `spawn`.
#[cfg(test)]
pub(crate) fn sample_order() -> WorkOrder {
    let project = ProjectName::parse("backend").unwrap();
    let values = [
        (ParamName::parse("package").unwrap(), "serde_json"),
        (ParamName::parse("level").unwrap(), "minor"),
    ];
    WorkOrder::validate(
        &crate::relay::template::sample::template(),
        &project,
        &values,
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::template::sample;

    fn param(name: &str) -> ParamName {
        ParamName::parse(name).unwrap()
    }

    fn project(name: &str) -> ProjectName {
        ProjectName::parse(name).unwrap()
    }

    fn validate(
        project_name: &str,
        values: &[(&str, &str)],
    ) -> Result<WorkOrder, WorkOrderRefusal> {
        let values: Vec<(ParamName, &str)> = values
            .iter()
            .map(|(name, value)| (param(name), *value))
            .collect();
        WorkOrder::validate(&sample::template(), &project(project_name), &values)
    }

    const VALID: &[(&str, &str)] = &[("package", "serde_json"), ("level", "minor")];

    #[test]
    fn the_project_must_be_one_of_the_template() {
        assert!(validate("backend", VALID).is_ok());
        assert!(validate("Docs Site", VALID).is_ok());
        for refused in ["lab", "elsewhere", "Backend", "backend "] {
            assert_eq!(
                validate(refused, VALID),
                Err(WorkOrderRefusal::ProjectNotAllowed),
                "{refused:?}"
            );
        }
        // Checked first: a bad value in a refused project names the project.
        assert_eq!(
            validate("lab", &[("package", "tokio"), ("level", "minor")]),
            Err(WorkOrderRefusal::ProjectNotAllowed)
        );
        let order = validate("Docs Site", VALID).unwrap();
        assert_eq!(order.project(), "Docs Site");
        assert_eq!(order.project_folder(), "D:/Repos/Docs-Site");
    }

    #[test]
    fn a_missing_duplicate_or_extra_parameter_is_refused() {
        assert_eq!(
            validate("backend", &[("package", "serde_json")]),
            Err(WorkOrderRefusal::MissingParameter)
        );
        assert_eq!(
            validate("backend", &[]),
            Err(WorkOrderRefusal::MissingParameter)
        );
        assert_eq!(
            validate(
                "backend",
                &[
                    ("package", "serde_json"),
                    ("level", "minor"),
                    ("level", "minor")
                ]
            ),
            Err(WorkOrderRefusal::DuplicateParameter)
        );
        assert_eq!(
            validate(
                "backend",
                &[
                    ("package", "serde_json"),
                    ("level", "minor"),
                    ("extra", "minor")
                ]
            ),
            Err(WorkOrderRefusal::UnknownParameter)
        );
        // The order of the pairs does not matter.
        assert_eq!(
            validate("backend", &[("level", "minor"), ("package", "serde_json")]),
            validate("backend", VALID)
        );
    }

    #[test]
    fn a_value_outside_its_list_is_refused() {
        for value in [
            "Serde_json",
            "serde_json ",
            " serde_json",
            "serde-json",
            "serde_json\n",
            "serde_json\u{200B}",
            "",
            "{level}",
        ] {
            assert_eq!(
                validate("backend", &[("package", value), ("level", "minor")]),
                Err(WorkOrderRefusal::ValueNotInList),
                "{value:?}"
            );
        }
        // A value of another parameter's list is not in this one's.
        assert_eq!(
            validate("backend", &[("package", "minor"), ("level", "minor")]),
            Err(WorkOrderRefusal::ValueNotInList)
        );
        assert!(validate("backend", &[("package", "@scope/pkg"), ("level", "patch")]).is_ok());
    }

    #[test]
    fn the_stored_value_is_the_template_string() {
        let template = sample::template();
        // The caller's buffer is gone once the order exists: the order owns
        // the template's string, not a borrow of the caller's.
        let order = {
            let callers = String::from("serde_json");
            let values = [
                (param("package"), callers.as_str()),
                (param("level"), "patch"),
            ];
            WorkOrder::validate(&template, &project("backend"), &values).unwrap()
        };
        for (name, value) in order.values() {
            assert!(
                template.params()[name].contains(value),
                "{name}: {value} is an entry of its list"
            );
        }
        assert_eq!(order.values()["package"], "serde_json");
        assert_eq!(order.values()["level"], "patch");
        assert_eq!(
            order.prompt(),
            "Bump serde_json to its latest patch release."
        );
        assert_eq!(order.template(), sample::TEMPLATE);
        assert_eq!(order.model(), Some(ModelAlias::Sonnet));
        assert_eq!(order.catalogue_hash(), sample::approved().hash);
    }

    #[test]
    fn the_prompt_sha256_is_of_the_rendered_prompt() {
        for values in [VALID, &[("package", "@scope/pkg"), ("level", "patch")]] {
            let order = validate("backend", values).unwrap();
            assert_eq!(order.prompt_sha256(), sha256_hex(order.prompt().as_bytes()));
            assert_eq!(order.prompt_sha256().len(), 64);
        }
        assert_ne!(
            validate("backend", VALID).unwrap().prompt_sha256(),
            validate("backend", &[("package", "@scope/pkg"), ("level", "minor")])
                .unwrap()
                .prompt_sha256()
        );
    }

    /// A snapshot no floor phase 2 accepted (its hash kept, as a record
    /// edited by hand would keep it): the order checks what it renders.
    fn tampered(prompt: &str, package_values: &[&str]) -> ApprovedTemplate {
        let mut approved = sample::approved();
        let template = approved
            .canonical
            .templates
            .get_mut(sample::TEMPLATE)
            .unwrap();
        template.prompt = prompt.to_owned();
        template.params.insert(
            "package".to_owned(),
            package_values.iter().map(|v| (*v).to_owned()).collect(),
        );
        ApprovedTemplate::new(&approved, &sample::current(), &sample::name()).unwrap()
    }

    #[test]
    fn what_the_snapshot_renders_is_checked() {
        let cases: [(&str, &[&str], &str, WorkOrderRefusal); 5] = [
            (
                "Bump {package} {other}.",
                &["a"],
                "a",
                WorkOrderRefusal::UnrenderablePrompt,
            ),
            (
                "Bump {package {level}.",
                &["a"],
                "a",
                WorkOrderRefusal::UnrenderablePrompt,
            ),
            (
                "{package} {level}",
                &["/run"],
                "/run",
                WorkOrderRefusal::PromptStart,
            ),
            (
                " \n{package} {level}",
                &["-x"],
                "-x",
                WorkOrderRefusal::PromptStart,
            ),
            ("{package}", &["  "], "  ", WorkOrderRefusal::PromptStart),
        ];
        for (prompt, list, value, refusal) in cases {
            let template = tampered(prompt, list);
            assert_eq!(
                WorkOrder::validate(&template, &project("backend"), &values_of(value)),
                Err(refusal),
                "{prompt:?}"
            );
        }
        // Counted in characters: a two-byte character counts one.
        let at_most = "é".repeat(MAX_PROMPT_CHARS - "Bump .".chars().count());
        let over = format!("{at_most}é");
        let template = tampered("Bump {package}.", &[&at_most, &over]);
        let order = WorkOrder::validate(&template, &project("backend"), &values_of(&at_most));
        assert_eq!(order.unwrap().prompt().chars().count(), MAX_PROMPT_CHARS);
        assert_eq!(
            WorkOrder::validate(&template, &project("backend"), &values_of(&over)),
            Err(WorkOrderRefusal::PromptTooLong)
        );
    }

    fn values_of(package: &str) -> [(ParamName, &str); 2] {
        [(param("package"), package), (param("level"), "minor")]
    }
}
