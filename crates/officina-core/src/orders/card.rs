//! Work order cards (FR-4, AD-78, D-135): what the window shows before a
//! launch and the only thing a launch acts on. Core issues a card from an
//! approved template, a project, values and a subject: it runs
//! `ApprovedTemplate::new`, [`WorkOrder::validate`] and
//! [`spawn::work_order`] with the `seq` the host's store reserved, so the
//! argument vector the card shows is the one that runs, byte for byte.
//! An edit issues a new card with the same `seq`. [`check_launch`] is the
//! launch's checks, in their order; the host gathers the facts they read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::QuotaView;
use crate::orders::permission::{WorkOrderPermission, WorkOrderPermissions};
use crate::orders::proposal::ProposedSubjectView;
use crate::orders::work_order::{SubjectChoice, WorkOrder, WorkOrderRefusal, order_name};
use crate::registry::confirm::checked_name;
use crate::registry::types::{AuthorRefusal, LaunchOutcome, SubjectRecord, SubjectStatus};
use crate::relay::catalogue::{ApprovedCatalogue, RelayFloor};
use crate::relay::template::{
    ApprovedTemplate, ModelAlias, ParamName, ProjectName, TemplateName, TemplateUnavailable,
};
use crate::spawn::{self, ClaudeVector};
use crate::untrusted::Untrusted;

/// How many hex characters a card id has (AD-78).
pub const CARD_ID_HEX: usize = 32;

/// What `claude` writes on stderr when it refuses a folder its trust
/// dialog never accepted (S4, E19): the launch's outcome is then
/// `untrusted_workspace`, not `failed`.
pub const WORKSPACE_NOT_TRUSTED: &str = "Workspace not trusted";

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a card stands (AD-78).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum CardState {
        /// Shown, and the one a launch may act on.
        Open,
        /// An edit issued a newer card in its place.
        Superseded,
        /// A launch acted on it: it never launches again (FR-4).
        Consumed,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the subject a card names cannot be its subject.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum SubjectRefusal {
        /// No subject has that id.
        UnknownSubject,
        /// The subject is archived, merged ones included (D-55).
        SubjectClosed,
        /// The subject belongs to another project than the order's.
        OtherProject,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a launch was refused, in the order [`check_launch`] checks
    /// (AD-78, D-135).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum LaunchRefusal {
        /// No card of the store has that id, or it was evicted.
        UnknownCard,
        /// An edit issued a newer card in its place.
        Superseded,
        /// A launch already acted on it.
        Consumed,
        /// The prompt hash the click sent is not the card's: the window
        /// showed another prompt than the one that would run.
        PromptHashMismatch,
        /// The approved catalogue, or the floor's, no longer hashes to the
        /// one the card was issued under.
        CatalogueChanged,
        /// The project's work order permission changed since the card was
        /// issued (D-985): the window showed another mode than the one
        /// that would run.
        PermissionChanged,
        /// The approved snapshot no longer holds the card's template.
        TemplateNotApproved,
        /// No `claude` was found, or it is not a native executable (D-128,
        /// D-647).
        ClaudeNotExecutable,
        /// The project's folder is not a folder any more.
        ProjectFolderMissing,
        /// The subject the card files under is archived or merged.
        SubjectClosed,
        /// The registry already holds the card's name (D-135).
        NameTaken,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why no card opens from a proposal (AD-91, D-152): only a pending,
    /// valid work order proposal opens one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProposalRefusal {
        /// The registry holds no proposal of that id.
        UnknownProposal,
        /// It is a recap: there is nothing to launch.
        NotAWorkOrder,
        /// It is invalid, or the author decided it already, or a newer one
        /// superseded it.
        NotPending,
        /// Its 24 hours passed (D-145).
        Expired,
    }
}

/// Why no card was issued, or an edit was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum IssueRefusal {
    /// The template is not in the approved snapshot, or the floor is not
    /// the one it describes; a name that cannot be a template's is not
    /// approved either.
    Template(TemplateUnavailable),
    /// [`WorkOrder::validate`] refused; a project or parameter name that
    /// cannot be one is the refusal of a name it does not know.
    Order(WorkOrderRefusal),
    Subject(SubjectRefusal),
    /// A new subject's name breaks one of D-57's rules other than being
    /// taken (a taken name files into the subject that holds it, D-158).
    SubjectName(AuthorRefusal),
    /// The edited card is unknown to the store.
    UnknownCard,
    /// The edited card is no longer open.
    CardNotOpen(CardState),
    /// The proposal a card was asked from opens none.
    Proposal(ProposalRefusal),
}

/// What the window asks a card for (AD-78): names and values as the
/// composer sends them, checked here, never trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CardRequest {
    pub template: String,
    pub project: String,
    /// One `(parameter, value)` pair each, in any order.
    pub values: Vec<(String, String)>,
    /// `None` takes the default (AD-78).
    pub subject: Option<SubjectChoice>,
}

/// What an edit changes (AD-78): a value, from its list, or the subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CardChange {
    Value { name: String, value: String },
    Subject { subject: SubjectChoice },
}

/// One parameter of a card: its value and its whole list, the composer's
/// choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ParamChoice {
    pub name: String,
    pub value: String,
    pub options: Vec<String>,
}

/// A subject a card may file under: one of the project's that is not
/// archived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SubjectOption {
    pub id: i64,
    pub name: String,
    pub status: SubjectStatus,
}

/// The proposal a card came from (AD-91, D-130, D-152): its id, its
/// rationale and the new subject's name it suggests, all the
/// orchestrator's text, shown under a "proposal" label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CardProposal {
    pub id: String,
    pub rationale: Untrusted<String>,
    /// A suggestion shown beside the subject picker, which the author may
    /// type; never the card's subject (D-152).
    pub suggested_subject: Option<Untrusted<String>>,
}

/// A pending work order proposal as a card is issued from it: the names
/// and values ingestion validated and stored as the approved template's
/// own strings (AD-91), its subject and its rationale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalOrder {
    pub id: String,
    pub template: String,
    pub project: String,
    pub values: BTreeMap<String, String>,
    pub subject: ProposedSubjectView,
    pub rationale: Untrusted<String>,
}

/// A card as the window shows it (AD-78): the rendered prompt, its
/// SHA-256 and the argument vector exactly as `spawn::work_order` built
/// it, with what the author decides from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkOrderCard {
    /// [`CARD_ID_HEX`] hex characters, drawn by the host.
    pub id: String,
    pub template: String,
    pub description: String,
    pub project: String,
    /// In the template's parameter order.
    pub values: Vec<ParamChoice>,
    pub prompt: String,
    pub prompt_sha256: String,
    /// The arguments after the binary.
    pub argv: Vec<String>,
    /// The project's folder, where the session starts.
    pub cwd: String,
    pub model: Option<ModelAlias>,
    /// `<template>-<seq>`, the session's name.
    pub name: String,
    pub seq: u32,
    pub subject: SubjectChoice,
    pub subject_options: Vec<SubjectOption>,
    /// The quota windows of the board the card was issued with.
    pub quota: Option<QuotaView>,
    pub catalogue_hash: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub issued_at_ms: i64,
    pub state: CardState,
    /// The proposal it was opened from, when it was (AD-91).
    pub proposal: Option<CardProposal>,
    /// Where its permission mode comes from (D-980): `argv` carries
    /// `--permission-mode` only for [`WorkOrderPermission::Officina`].
    pub permission: WorkOrderPermission,
}

/// What the window gets back from an issue or an edit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CardAnswer {
    Issued(Box<WorkOrderCard>),
    Refused(IssueRefusal),
}

/// What the window gets back from a launch: how it ended, or why it was
/// refused before anything started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum LaunchAnswer {
    Launched(LaunchOutcome),
    Refused(LaunchRefusal),
}

/// A card and what it stands for: the validated order and the vector that
/// runs, which nothing but [`issue`] builds.
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedCard {
    card: WorkOrderCard,
    order: WorkOrder,
    vector: ClaudeVector,
}

impl IssuedCard {
    pub fn card(&self) -> &WorkOrderCard {
        &self.card
    }

    pub fn order(&self) -> &WorkOrder {
        &self.order
    }

    /// The vector a launch runs, byte for byte the card's `argv`.
    pub fn vector(&self) -> &ClaudeVector {
        &self.vector
    }

    pub fn id(&self) -> &str {
        &self.card.id
    }

    pub fn seq(&self) -> u32 {
        self.card.seq
    }

    pub fn state(&self) -> CardState {
        self.card.state
    }

    /// Records that an edit superseded the card or a launch consumed it.
    pub fn mark(&mut self, state: CardState) {
        self.card.state = state;
    }

    /// The request this card answers, its values as the order keeps them.
    fn request(&self) -> CardRequest {
        CardRequest {
            template: self.card.template.clone(),
            project: self.card.project.clone(),
            values: self
                .order
                .values()
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            subject: Some(self.card.subject.clone()),
        }
    }
}

/// What issuing reads besides the request: the approved catalogue and the
/// current floor, the registry's subjects, the board's quota windows, the
/// floor's config dir, the window's record of the projects whose work
/// orders take Officina's default (D-980) and the time.
#[derive(Debug, Clone, Copy)]
pub struct CardInputs<'a> {
    pub approved: &'a ApprovedCatalogue,
    pub current: &'a RelayFloor,
    pub subjects: &'a [SubjectRecord],
    pub quota: Option<&'a QuotaView>,
    pub config_dir: Option<&'a str>,
    pub permissions: &'a WorkOrderPermissions,
    pub now_ms: i64,
}

/// The card `id` for `request` with the `seq` the store reserved (D-135),
/// or the first refusal: the template, the project, the parameters' names,
/// then [`WorkOrder::validate`]'s checks, then the subject. Without a
/// subject in the request the card takes [`default_subject`].
pub fn issue(
    id: &str,
    seq: u32,
    request: &CardRequest,
    inputs: &CardInputs,
) -> Result<IssuedCard, IssueRefusal> {
    let name = TemplateName::parse(&request.template)
        .ok_or(IssueRefusal::Template(TemplateUnavailable::NotApproved))?;
    let template = ApprovedTemplate::new(inputs.approved, inputs.current, &name)
        .map_err(IssueRefusal::Template)?;
    let project = ProjectName::parse(&request.project)
        .filter(|project| template.project_folder(project).is_some())
        .ok_or(IssueRefusal::Order(WorkOrderRefusal::ProjectNotAllowed))?;
    let values = request
        .values
        .iter()
        .map(|(name, value)| {
            ParamName::parse(name)
                .map(|name| (name, value.as_str()))
                .ok_or(IssueRefusal::Order(WorkOrderRefusal::UnknownParameter))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let order = WorkOrder::validate(&template, &project, &values).map_err(IssueRefusal::Order)?;
    let wanted = request
        .subject
        .clone()
        .unwrap_or_else(|| default_subject(&order, inputs.subjects));
    let subject = checked_subject(&wanted, &order, inputs.subjects)?;
    let permission = inputs
        .permissions
        .of(order.project(), order.project_folder());
    let vector = spawn::work_order(&order, seq, inputs.config_dir, permission);
    let card = WorkOrderCard {
        id: id.to_owned(),
        template: order.template().to_owned(),
        description: template.description().to_owned(),
        project: order.project().to_owned(),
        values: template
            .params()
            .iter()
            .map(|(name, options)| ParamChoice {
                name: name.clone(),
                value: order.values().get(name).cloned().unwrap_or_default(),
                options: options.clone(),
            })
            .collect(),
        prompt: order.prompt().to_owned(),
        prompt_sha256: order.prompt_sha256().to_owned(),
        argv: vector.args().to_vec(),
        cwd: order.project_folder().to_owned(),
        model: order.model(),
        name: order_name(order.template(), seq),
        seq,
        subject,
        subject_options: open_subjects(order.project(), inputs.subjects)
            .map(|s| SubjectOption {
                id: s.id,
                name: s.name.clone(),
                status: s.status.value,
            })
            .collect(),
        quota: inputs.quota.cloned(),
        catalogue_hash: order.catalogue_hash().to_owned(),
        issued_at_ms: inputs.now_ms,
        state: CardState::Open,
        proposal: None,
        permission,
    };
    Ok(IssuedCard {
        card,
        order,
        vector,
    })
}

/// The card `id` that replaces `previous` with `change` applied, under the
/// same `seq` (D-135): a value goes through [`WorkOrder::validate`] again,
/// so one outside its list is refused. A card that is not open is refused;
/// superseding `previous` is the store's, once this succeeded.
pub fn edit(
    previous: &IssuedCard,
    change: &CardChange,
    id: &str,
    inputs: &CardInputs,
) -> Result<IssuedCard, IssueRefusal> {
    if previous.state() != CardState::Open {
        return Err(IssueRefusal::CardNotOpen(previous.state()));
    }
    let mut request = previous.request();
    match change {
        CardChange::Value { name, value } => {
            match request.values.iter_mut().find(|(given, _)| given == name) {
                Some(pair) => pair.1.clone_from(value),
                None => request.values.push((name.clone(), value.clone())),
            }
        }
        CardChange::Subject { subject } => request.subject = Some(subject.clone()),
    }
    issued_again(previous, &request, id, inputs)
}

/// The card `id` that replaces `previous` as it stands, under the same
/// `seq`, after its project's work order permission changed (D-985): the
/// same request issued again, so its vector carries the permission
/// `inputs` now hold. A card that is not open is refused.
pub fn reissue(
    previous: &IssuedCard,
    id: &str,
    inputs: &CardInputs,
) -> Result<IssuedCard, IssueRefusal> {
    if previous.state() != CardState::Open {
        return Err(IssueRefusal::CardNotOpen(previous.state()));
    }
    issued_again(previous, &previous.request(), id, inputs)
}

/// [`issue`] of `request` under `previous`'s `seq`, keeping the proposal
/// it came from.
fn issued_again(
    previous: &IssuedCard,
    request: &CardRequest,
    id: &str,
    inputs: &CardInputs,
) -> Result<IssuedCard, IssueRefusal> {
    let mut issued = issue(id, previous.seq(), request, inputs)?;
    issued.card.proposal.clone_from(&previous.card.proposal);
    Ok(issued)
}

/// The card `id` for the work order proposal `proposal`, with the `seq`
/// the store reserved (AD-91, D-152): [`issue`] with its stored values, so
/// [`WorkOrder::validate`] runs on them as on the author's. Its subject is
/// the proposal's existing subject when it names one (checked at
/// ingestion, checked again here), else a new subject named after the
/// template: a proposed new name is carried as a suggestion, never as the
/// default.
pub fn issue_from_proposal(
    id: &str,
    seq: u32,
    proposal: &ProposalOrder,
    inputs: &CardInputs,
) -> Result<IssuedCard, IssueRefusal> {
    let (subject, suggested_subject) = match &proposal.subject {
        ProposedSubjectView::Id(subject) => (SubjectChoice::Existing(*subject), None),
        ProposedSubjectView::New(name) => (
            SubjectChoice::New(proposal.template.clone()),
            Some(name.clone()),
        ),
    };
    let request = CardRequest {
        template: proposal.template.clone(),
        project: proposal.project.clone(),
        values: proposal
            .values
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        subject: Some(subject),
    };
    let mut issued = issue(id, seq, &request, inputs)?;
    issued.card.proposal = Some(CardProposal {
        id: proposal.id.clone(),
        rationale: proposal.rationale.clone(),
        suggested_subject,
    });
    Ok(issued)
}

/// The project's subjects that are not archived.
fn open_subjects<'a>(
    project: &'a str,
    subjects: &'a [SubjectRecord],
) -> impl Iterator<Item = &'a SubjectRecord> {
    subjects
        .iter()
        .filter(move |s| s.project == project && s.status.value != SubjectStatus::Archived)
}

/// The subject a card files under when the request names none (AD-78):
/// the project's single `active` or `to_resume` subject when there is
/// exactly one, else a new subject named after the template.
pub fn default_subject(order: &WorkOrder, subjects: &[SubjectRecord]) -> SubjectChoice {
    let mut open = open_subjects(order.project(), subjects);
    match (open.next(), open.next()) {
        (Some(only), None) => SubjectChoice::Existing(only.id),
        _ => SubjectChoice::New(order.template().to_owned()),
    }
}

/// `wanted` when it can be the order's subject: an existing subject of the
/// order's project that is not archived, or a new name D-57's rules accept,
/// trimmed as they store it; a name another open subject holds is kept, and
/// files into that subject at `started` (D-158).
fn checked_subject(
    wanted: &SubjectChoice,
    order: &WorkOrder,
    subjects: &[SubjectRecord],
) -> Result<SubjectChoice, IssueRefusal> {
    match wanted {
        SubjectChoice::Existing(id) => {
            let subject = subjects
                .iter()
                .find(|s| s.id == *id)
                .ok_or(IssueRefusal::Subject(SubjectRefusal::UnknownSubject))?;
            if subject.project != order.project() {
                return Err(IssueRefusal::Subject(SubjectRefusal::OtherProject));
            }
            if subject.status.value == SubjectStatus::Archived {
                return Err(IssueRefusal::Subject(SubjectRefusal::SubjectClosed));
            }
            Ok(SubjectChoice::Existing(*id))
        }
        SubjectChoice::New(name) => match checked_name(name, order.project(), None, subjects) {
            Ok(name) => Ok(SubjectChoice::New(name)),
            Err(AuthorRefusal::NameTaken) => Ok(SubjectChoice::New(name.trim().to_owned())),
            Err(refusal) => Err(IssueRefusal::SubjectName(refusal)),
        },
    }
}

/// What a launch's checks read (AD-78): the store's card, the click's
/// hash, and what the host found when the click arrived.
#[derive(Debug, Clone, Copy)]
pub struct LaunchFacts<'a> {
    /// The card the store holds under the clicked id, if any.
    pub card: Option<&'a WorkOrderCard>,
    /// The prompt hash the click sent.
    pub clicked_sha256: &'a str,
    /// The approved catalogue's hash, when one is approved and the window's
    /// record confirms it (D-175).
    pub approved_hash: Option<&'a str>,
    /// The current floor's hash, when it reads.
    pub current_hash: Option<&'a str>,
    /// The permission the card's project takes now (D-985).
    pub permission: WorkOrderPermission,
    /// The approved snapshot still holds the card's template.
    pub template_approved: bool,
    /// `claude` resolves to a native executable.
    pub claude_native: bool,
    pub folder_exists: bool,
    /// The card's subject is new, or exists and is not archived.
    pub subject_open: bool,
    /// The registry holds the card's name, or its template's `seq`.
    pub name_taken: bool,
}

/// AD-78's launch checks, in their order; the first that fails is the
/// refusal. Each mutant of AD-95 (D-148) skips one at its own site.
pub fn check_launch(facts: &LaunchFacts) -> Result<(), LaunchRefusal> {
    let card = facts.card.ok_or(LaunchRefusal::UnknownCard)?;
    check_superseded(card)?;
    if card.state == CardState::Consumed {
        return Err(LaunchRefusal::Consumed);
    }
    check_prompt_hash(card, facts.clicked_sha256)?;
    let hash = Some(card.catalogue_hash.as_str());
    if facts.approved_hash != hash || facts.current_hash != hash {
        return Err(LaunchRefusal::CatalogueChanged);
    }
    if facts.permission != card.permission {
        return Err(LaunchRefusal::PermissionChanged);
    }
    if !facts.template_approved {
        return Err(LaunchRefusal::TemplateNotApproved);
    }
    if !facts.claude_native {
        return Err(LaunchRefusal::ClaudeNotExecutable);
    }
    if !facts.folder_exists {
        return Err(LaunchRefusal::ProjectFolderMissing);
    }
    if !facts.subject_open {
        return Err(LaunchRefusal::SubjectClosed);
    }
    if facts.name_taken {
        return Err(LaunchRefusal::NameTaken);
    }
    Ok(())
}

#[cfg(not(feature = "mutant-accept-superseded"))]
fn check_superseded(card: &WorkOrderCard) -> Result<(), LaunchRefusal> {
    if card.state == CardState::Superseded {
        return Err(LaunchRefusal::Superseded);
    }
    Ok(())
}

/// The mutant `mutant-accept-superseded` (AD-95, D-148): the check skipped.
#[cfg(feature = "mutant-accept-superseded")]
fn check_superseded(_card: &WorkOrderCard) -> Result<(), LaunchRefusal> {
    Ok(())
}

#[cfg(not(feature = "mutant-skip-prompt-hash"))]
fn check_prompt_hash(card: &WorkOrderCard, clicked: &str) -> Result<(), LaunchRefusal> {
    if card.prompt_sha256 != clicked {
        return Err(LaunchRefusal::PromptHashMismatch);
    }
    Ok(())
}

/// The mutant `mutant-skip-prompt-hash` (AD-95, D-148): the check skipped.
#[cfg(feature = "mutant-skip-prompt-hash")]
fn check_prompt_hash(_card: &WorkOrderCard, _clicked: &str) -> Result<(), LaunchRefusal> {
    Ok(())
}

/// The `seq` the store reserves for a new card of a template (D-135): one
/// more than the larger of the registry's highest for the template and
/// the highest a card of that template holds (`held`).
///
/// ```
/// use officina_core::orders::card::reserved_seq;
/// assert_eq!(reserved_seq(None, []), 1);
/// assert_eq!(reserved_seq(Some(4), [2, 7]), 8);
/// assert_eq!(reserved_seq(Some(9), [2]), 10);
/// ```
pub fn reserved_seq(registry_highest: Option<u32>, held: impl IntoIterator<Item = u32>) -> u32 {
    held.into_iter()
        .chain(registry_highest)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

/// The values of `card` by parameter, for the journal's line.
pub fn card_values(card: &WorkOrderCard) -> BTreeMap<String, String> {
    card.values
        .iter()
        .map(|choice| (choice.name.clone(), choice.value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::types::{Source, Sourced};
    use crate::relay::template::sample;

    const CONFIG: &str = "C:/path/to/claude-work";
    const NOW: i64 = 1_793_102_400_000;

    fn subject(id: i64, project: &str, name: &str, status: SubjectStatus) -> SubjectRecord {
        SubjectRecord {
            id,
            project: project.to_owned(),
            name: name.to_owned(),
            status: Sourced {
                value: status,
                src: Source::Author,
                at_ms: NOW,
            },
            created_at_ms: NOW,
            routine: false,
            merged_into: None,
        }
    }

    fn request(values: &[(&str, &str)], subject: Option<SubjectChoice>) -> CardRequest {
        CardRequest {
            template: sample::TEMPLATE.to_owned(),
            project: "backend".to_owned(),
            values: values
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
            subject,
        }
    }

    const VALID: &[(&str, &str)] = &[("package", "serde_json"), ("level", "minor")];

    /// Issues `request` with `subjects` on the sample catalogue.
    fn issue_with(
        id: &str,
        seq: u32,
        request: &CardRequest,
        subjects: &[SubjectRecord],
    ) -> Result<IssuedCard, IssueRefusal> {
        let approved = sample::approved();
        let current = sample::current();
        issue(
            id,
            seq,
            request,
            &CardInputs {
                approved: &approved,
                current: &current,
                subjects,
                quota: None,
                config_dir: Some(CONFIG),
                permissions: &WorkOrderPermissions::default(),
                now_ms: NOW,
            },
        )
    }

    fn edit_with(
        previous: &IssuedCard,
        change: &CardChange,
        id: &str,
    ) -> Result<IssuedCard, IssueRefusal> {
        let approved = sample::approved();
        let current = sample::current();
        edit(
            previous,
            change,
            id,
            &CardInputs {
                approved: &approved,
                current: &current,
                subjects: &[],
                quota: None,
                config_dir: Some(CONFIG),
                permissions: &WorkOrderPermissions::default(),
                now_ms: NOW + 1,
            },
        )
    }

    #[test]
    fn a_card_carries_the_vector_spawn_built() {
        let issued = issue_with("a", 7, &request(VALID, None), &[]).unwrap();
        let card = issued.card();
        let order = WorkOrder::validate(
            &sample::template(),
            &ProjectName::parse("backend").unwrap(),
            &[
                (ParamName::parse("package").unwrap(), "serde_json"),
                (ParamName::parse("level").unwrap(), "minor"),
            ],
        )
        .unwrap();
        let rebuilt = spawn::work_order(&order, 7, Some(CONFIG), WorkOrderPermission::Project);
        assert_eq!(card.argv, rebuilt.args());
        assert_eq!(issued.vector(), &rebuilt);
        assert_eq!(card.prompt, order.prompt());
        assert_eq!(card.prompt_sha256, order.prompt_sha256());
        assert_eq!(card.cwd, order.project_folder());
        assert_eq!(card.model, Some(ModelAlias::Sonnet));
        assert_eq!(card.name, order_name(sample::TEMPLATE, 7));
        assert_eq!(card.catalogue_hash, sample::approved().hash);
        assert_eq!(card.description, sample::template().description());
        assert_eq!((card.issued_at_ms, card.state), (NOW, CardState::Open));
        // Each value with its whole list.
        let lists = sample::template().params().clone();
        assert_eq!(card.values.len(), lists.len());
        for choice in &card.values {
            assert_eq!(choice.options, lists[&choice.name]);
            assert_eq!(Some(&choice.value), order.values().get(&choice.name));
        }
    }

    #[test]
    fn an_edit_keeps_the_seq() {
        let first = issue_with("a", 4, &request(VALID, None), &[]).unwrap();
        let change = CardChange::Value {
            name: "level".into(),
            value: "patch".into(),
        };
        let second = edit_with(&first, &change, "b").unwrap();
        assert_eq!(second.seq(), 4);
        assert_eq!(second.card().name, first.card().name);
        assert!(second.card().prompt.contains("patch"));
        assert_eq!(
            second.vector().args(),
            spawn::work_order(
                second.order(),
                4,
                Some(CONFIG),
                WorkOrderPermission::Project
            )
            .args()
        );
    }

    #[test]
    fn an_edit_issues_a_new_card_and_supersedes_the_old() {
        let mut first = issue_with("a", 1, &request(VALID, None), &[]).unwrap();
        let change = CardChange::Subject {
            subject: SubjectChoice::New("Rollout".into()),
        };
        let second = edit_with(&first, &change, "b").unwrap();
        assert_eq!(second.id(), "b");
        assert_eq!(second.state(), CardState::Open);
        assert_eq!(second.card().subject, SubjectChoice::New("Rollout".into()));
        assert_eq!(second.card().prompt, first.card().prompt);
        // Superseding is the store's, once the edit succeeded; a card no
        // longer open is never edited again.
        first.mark(CardState::Superseded);
        assert_eq!(
            edit_with(&first, &change, "c"),
            Err(IssueRefusal::CardNotOpen(CardState::Superseded))
        );
        let mut consumed = second;
        consumed.mark(CardState::Consumed);
        assert_eq!(
            edit_with(&consumed, &change, "d"),
            Err(IssueRefusal::CardNotOpen(CardState::Consumed))
        );
    }

    #[test]
    fn an_edit_outside_the_list_is_refused() {
        let first = issue_with("a", 1, &request(VALID, None), &[]).unwrap();
        for (name, value, refusal) in [
            ("level", "major", WorkOrderRefusal::ValueNotInList),
            (
                "package",
                "serde_json --settings x",
                WorkOrderRefusal::ValueNotInList,
            ),
            ("package", "minor", WorkOrderRefusal::ValueNotInList),
            ("force", "minor", WorkOrderRefusal::UnknownParameter),
            ("--model", "opus", WorkOrderRefusal::UnknownParameter),
        ] {
            let change = CardChange::Value {
                name: name.into(),
                value: value.into(),
            };
            assert_eq!(
                edit_with(&first, &change, "b"),
                Err(IssueRefusal::Order(refusal)),
                "{name}={value}"
            );
        }
    }

    #[test]
    fn the_single_open_subject_is_the_default() {
        let subjects = [
            subject(1, "backend", "Upgrades", SubjectStatus::ToResume),
            subject(2, "backend", "Old work", SubjectStatus::Archived),
            subject(3, "lab", "Elsewhere", SubjectStatus::Active),
        ];
        let issued = issue_with("a", 1, &request(VALID, None), &subjects).unwrap();
        assert_eq!(issued.card().subject, SubjectChoice::Existing(1));
        assert_eq!(
            issued
                .card()
                .subject_options
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [1]
        );
    }

    #[test]
    fn otherwise_a_new_subject_named_after_the_template() {
        let two = [
            subject(1, "backend", "Upgrades", SubjectStatus::Active),
            subject(2, "backend", "Docs", SubjectStatus::ToResume),
        ];
        for subjects in [&two[..], &[]] {
            let issued = issue_with("a", 1, &request(VALID, None), subjects).unwrap();
            assert_eq!(
                issued.card().subject,
                SubjectChoice::New(sample::TEMPLATE.into())
            );
        }
        // A new name follows D-57's rules: trimmed; one another subject
        // holds is kept (it files there, D-158); a broken one is refused.
        let named = |name: &str| request(VALID, Some(SubjectChoice::New(name.into())));
        let issued = issue_with("a", 1, &named("  Rollout "), &two).unwrap();
        assert_eq!(issued.card().subject, SubjectChoice::New("Rollout".into()));
        let issued = issue_with("a", 1, &named("upgrades"), &two).unwrap();
        assert_eq!(issued.card().subject, SubjectChoice::New("upgrades".into()));
        for (name, refusal) in [
            ("   ", AuthorRefusal::NameEmpty),
            ("a\u{202e}b", AuthorRefusal::NameHasForbiddenCharacters),
        ] {
            assert_eq!(
                issue_with("a", 1, &named(name), &two),
                Err(IssueRefusal::SubjectName(refusal)),
                "{name:?}"
            );
        }
        // An existing subject must be the project's and open.
        let existing = |id: i64| request(VALID, Some(SubjectChoice::Existing(id)));
        let subjects = [
            subject(1, "backend", "Upgrades", SubjectStatus::Active),
            subject(2, "backend", "Old work", SubjectStatus::Archived),
            subject(3, "lab", "Elsewhere", SubjectStatus::Active),
        ];
        assert!(issue_with("a", 1, &existing(1), &subjects).is_ok());
        for (id, refusal) in [
            (2, SubjectRefusal::SubjectClosed),
            (3, SubjectRefusal::OtherProject),
            (9, SubjectRefusal::UnknownSubject),
        ] {
            assert_eq!(
                issue_with("a", 1, &existing(id), &subjects),
                Err(IssueRefusal::Subject(refusal)),
                "{id}"
            );
        }
    }

    #[test]
    fn names_that_cannot_be_ones_are_refused_as_unknown() {
        let mut bad_template = request(VALID, None);
        bad_template.template = "--settings".into();
        assert_eq!(
            issue_with("a", 1, &bad_template, &[]),
            Err(IssueRefusal::Template(TemplateUnavailable::NotApproved))
        );
        let mut bad_project = request(VALID, None);
        bad_project.project = "back\u{200b}end".into();
        assert_eq!(
            issue_with("a", 1, &bad_project, &[]),
            Err(IssueRefusal::Order(WorkOrderRefusal::ProjectNotAllowed))
        );
        // The project is checked before the parameters' names, as
        // `validate` checks it first.
        bad_project.values.push(("-p".into(), "x".into()));
        assert_eq!(
            issue_with("a", 1, &bad_project, &[]),
            Err(IssueRefusal::Order(WorkOrderRefusal::ProjectNotAllowed))
        );
        let bad_param = request(
            &[("package", "serde_json"), ("level", "minor"), ("-p", "x")],
            None,
        );
        assert_eq!(
            issue_with("a", 1, &bad_param, &[]),
            Err(IssueRefusal::Order(WorkOrderRefusal::UnknownParameter))
        );
    }

    /// Facts under which every check passes for `card`.
    fn passing(card: &WorkOrderCard) -> LaunchFacts<'_> {
        LaunchFacts {
            card: Some(card),
            clicked_sha256: &card.prompt_sha256,
            approved_hash: Some(&card.catalogue_hash),
            current_hash: Some(&card.catalogue_hash),
            permission: card.permission,
            template_approved: true,
            claude_native: true,
            folder_exists: true,
            subject_open: true,
            name_taken: false,
        }
    }

    /// AD-78: each check is a `LaunchRefusal` variant, and with every
    /// later check failing too, the earliest is the refusal. Iterates
    /// `LaunchRefusal::ALL`, so a variant without its fact fails here.
    #[test]
    fn checks_run_in_their_order() {
        let issued = issue_with("a", 1, &request(VALID, None), &[]).unwrap();
        let open = issued.card().clone();
        let mut superseded = open.clone();
        superseded.state = CardState::Superseded;
        let mut consumed = open.clone();
        consumed.state = CardState::Consumed;
        assert_eq!(check_launch(&passing(&open)), Ok(()));
        // Breaks the fact of `refusal` in `facts`.
        fn broken<'a>(
            refusal: LaunchRefusal,
            facts: &mut LaunchFacts<'a>,
            cards: [&'a WorkOrderCard; 2],
        ) {
            match refusal {
                LaunchRefusal::UnknownCard => facts.card = None,
                LaunchRefusal::Superseded => facts.card = Some(cards[0]),
                LaunchRefusal::Consumed => facts.card = Some(cards[1]),
                LaunchRefusal::PromptHashMismatch => facts.clicked_sha256 = "0",
                LaunchRefusal::CatalogueChanged => facts.current_hash = Some("0"),
                LaunchRefusal::PermissionChanged => {
                    facts.permission = WorkOrderPermission::Officina
                }
                LaunchRefusal::TemplateNotApproved => facts.template_approved = false,
                LaunchRefusal::ClaudeNotExecutable => facts.claude_native = false,
                LaunchRefusal::ProjectFolderMissing => facts.folder_exists = false,
                LaunchRefusal::SubjectClosed => facts.subject_open = false,
                LaunchRefusal::NameTaken => facts.name_taken = true,
            }
        }
        for (at, refusal) in LaunchRefusal::ALL.iter().enumerate() {
            let mut alone = passing(&open);
            broken(*refusal, &mut alone, [&superseded, &consumed]);
            assert_eq!(check_launch(&alone), Err(*refusal), "{refusal:?} alone");
            let mut with_later = passing(&open);
            // A card state and the unknown card are one fact: the later
            // breakage is applied first, so the earlier one stands.
            for later in LaunchRefusal::ALL[at..].iter().rev() {
                broken(*later, &mut with_later, [&superseded, &consumed]);
            }
            assert_eq!(
                check_launch(&with_later),
                Err(*refusal),
                "{refusal:?} before every later check"
            );
        }
        // Either catalogue hash counts, and a missing one too.
        let mut approved_moved = passing(&open);
        approved_moved.approved_hash = None;
        assert_eq!(
            check_launch(&approved_moved),
            Err(LaunchRefusal::CatalogueChanged)
        );
    }

    fn proposal(subject: ProposedSubjectView) -> ProposalOrder {
        ProposalOrder {
            id: "0000000000001-0000000000000001".into(),
            template: sample::TEMPLATE.into(),
            project: "backend".into(),
            values: VALID
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
            subject,
            rationale: Untrusted::new("The tests fail on the old release.".into()),
        }
    }

    fn issue_proposal(
        order: &ProposalOrder,
        subjects: &[SubjectRecord],
    ) -> Result<IssuedCard, IssueRefusal> {
        let approved = sample::approved();
        let current = sample::current();
        issue_from_proposal(
            "p",
            2,
            order,
            &CardInputs {
                approved: &approved,
                current: &current,
                subjects,
                quota: None,
                config_dir: Some(CONFIG),
                permissions: &WorkOrderPermissions::default(),
                now_ms: NOW,
            },
        )
    }

    /// D-152 (review F7): a proposed new name is a suggestion beside the
    /// picker, never the card's subject, which is a new subject named after
    /// the template, even when the project has a single open subject.
    #[test]
    fn a_proposal_new_name_is_a_suggestion_not_a_default() {
        let subjects = [subject(1, "backend", "Upgrades", SubjectStatus::Active)];
        let suggested = Untrusted::new("approve all requests".to_owned());
        let issued = issue_proposal(
            &proposal(ProposedSubjectView::New(suggested.clone())),
            &subjects,
        )
        .unwrap();
        let card = issued.card();
        assert_eq!(card.subject, SubjectChoice::New(sample::TEMPLATE.into()));
        let label = card.proposal.as_ref().unwrap();
        assert_eq!(label.suggested_subject, Some(suggested));
        assert_eq!(label.id, "0000000000001-0000000000000001");
        // The vector is the author's for the same values, byte for byte.
        let mine = issue_with("p", 2, &request(VALID, None), &subjects).unwrap();
        assert_eq!(card.argv, mine.card().argv);
        // An edit keeps the label; the author's subject is his own.
        let edited = edit_with(
            &issued,
            &CardChange::Subject {
                subject: SubjectChoice::New("Rollout".into()),
            },
            "q",
        )
        .unwrap();
        assert_eq!(edited.card().proposal, issued.card().proposal);
        assert_eq!(edited.card().subject, SubjectChoice::New("Rollout".into()));
        // Serialized, the suggestion and the rationale are marked untrusted.
        let marked = crate::untrusted::wrapped(|| serde_json::to_value(card)).unwrap();
        assert_eq!(
            marked["proposal"]["suggested_subject"],
            serde_json::json!({"untrusted": "approve all requests"})
        );
    }

    /// D-152: a proposal's existing subject, checked at ingestion, is the
    /// card's default; one archived since refuses the card.
    #[test]
    fn a_proposal_existing_subject_is_the_default() {
        let subjects = [
            subject(1, "backend", "Upgrades", SubjectStatus::Active),
            subject(2, "backend", "Docs", SubjectStatus::Active),
            subject(3, "backend", "Old", SubjectStatus::Archived),
        ];
        let issued = issue_proposal(&proposal(ProposedSubjectView::Id(2)), &subjects).unwrap();
        assert_eq!(issued.card().subject, SubjectChoice::Existing(2));
        assert_eq!(
            issued.card().proposal.as_ref().unwrap().suggested_subject,
            None
        );
        assert_eq!(
            issue_proposal(&proposal(ProposedSubjectView::Id(3)), &subjects),
            Err(IssueRefusal::Subject(SubjectRefusal::SubjectClosed))
        );
        // `validate` runs on the stored values as on the author's.
        let mut changed = proposal(ProposedSubjectView::Id(2));
        changed.values.insert("level".into(), "major".into());
        assert_eq!(
            issue_proposal(&changed, &subjects),
            Err(IssueRefusal::Order(WorkOrderRefusal::ValueNotInList))
        );
    }

    #[test]
    fn a_card_serializes_for_the_window() {
        let issued = issue_with("a", 3, &request(VALID, None), &[]).unwrap();
        let answer =
            serde_json::to_value(CardAnswer::Issued(Box::new(issued.card().clone()))).unwrap();
        assert_eq!(answer["outcome"], "issued");
        assert_eq!(answer["value"]["state"], "open");
        assert_eq!(answer["value"]["subject"]["kind"], "new");
        let refused = serde_json::to_value(CardAnswer::Refused(IssueRefusal::Order(
            WorkOrderRefusal::ValueNotInList,
        )))
        .unwrap();
        assert_eq!(
            refused,
            serde_json::json!({"outcome": "refused", "value": {"kind": "order", "detail": "value_not_in_list"}})
        );
        let launched =
            serde_json::to_value(LaunchAnswer::Launched(LaunchOutcome::Started)).unwrap();
        assert_eq!(
            launched,
            serde_json::json!({"outcome": "launched", "value": "started"})
        );
        assert_eq!(card_values(issued.card()), issued.order().values().clone());
    }
    /// D-980, D-985: a project switched to Officina's default issues its
    /// cards with `--permission-mode auto` and the permission named; the
    /// card issued before refuses to launch until it is issued again,
    /// which keeps its `seq`, its values and its proposal; a project the
    /// record names for another folder keeps its own mode.
    #[test]
    fn a_switched_project_issues_its_cards_with_officinas_default() {
        let approved = sample::approved();
        let current = sample::current();
        let before = issue_with("a", 5, &request(VALID, None), &[]).unwrap();
        assert_eq!(before.card().permission, WorkOrderPermission::Project);
        assert!(!before.card().argv.iter().any(|a| a == "--permission-mode"));
        let folder = before.order().project_folder().to_owned();
        let switched = WorkOrderPermissions::default().with(
            "backend",
            &folder,
            WorkOrderPermission::Officina,
            NOW,
        );
        let inputs = CardInputs {
            approved: &approved,
            current: &current,
            subjects: &[],
            quota: None,
            config_dir: Some(CONFIG),
            permissions: &switched,
            now_ms: NOW + 1,
        };
        let mut facts = passing(before.card());
        facts.permission = switched.of("backend", &folder);
        assert_eq!(check_launch(&facts), Err(LaunchRefusal::PermissionChanged));
        let after = reissue(&before, "b", &inputs).unwrap();
        assert_eq!(
            (after.seq(), after.card().permission),
            (5, WorkOrderPermission::Officina)
        );
        assert_eq!(after.card().prompt, before.card().prompt);
        assert_eq!(
            after.vector(),
            &spawn::work_order(
                after.order(),
                5,
                Some(CONFIG),
                WorkOrderPermission::Officina
            )
        );
        assert_eq!(after.card().argv, after.vector().args());
        assert_eq!(
            check_launch(&facts_for(after.card(), WorkOrderPermission::Officina)),
            Ok(())
        );
        let elsewhere = WorkOrderPermissions::default().with(
            "backend",
            "C:/somewhere/else",
            WorkOrderPermission::Officina,
            NOW,
        );
        let inputs = CardInputs {
            permissions: &elsewhere,
            ..inputs
        };
        assert_eq!(
            reissue(&before, "c", &inputs).unwrap().card().permission,
            WorkOrderPermission::Project
        );
        let mut consumed = before;
        consumed.mark(CardState::Consumed);
        assert_eq!(
            reissue(&consumed, "d", &inputs),
            Err(IssueRefusal::CardNotOpen(CardState::Consumed))
        );
    }

    /// [`passing`] facts for `card` with `permission` as the project's now.
    fn facts_for(card: &WorkOrderCard, permission: WorkOrderPermission) -> LaunchFacts<'_> {
        LaunchFacts {
            permission,
            ..passing(card)
        }
    }
}
