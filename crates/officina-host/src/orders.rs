//! Work order cards and their launch (FR-4, AD-78, AD-81 steps 1 to 3,
//! D-135, D-136). The card store lives on the collector thread, beside the
//! registry's writer: it issues and edits cards through core, and at a
//! click runs AD-78's checks, consumes the card, records the `work_order`
//! row and hands the card's vector back. [`launch_card`], called by the
//! window's command on a blocking thread, runs that vector through
//! `spawn::execute` there, never on the collector thread, and sends the
//! outcome back, which the collector records and journals once. Nothing
//! retries: a failed spawn burns its card (FR-4, D-29). The orchestrator's
//! Start follows the same split ([`orchestrator`]). Compiled only with the
//! `window-authority` feature: only the window launches.

pub mod orchestrator;

use std::collections::VecDeque;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;
use std::sync::mpsc::Sender;

use officina_core::board::QuotaView;
use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::orders::card::{
    self, CardAnswer, CardChange, CardInputs, CardRequest, CardState, IssueRefusal, IssuedCard,
    LaunchAnswer, LaunchFacts, LaunchRefusal, WORKSPACE_NOT_TRUSTED, card_values, check_launch,
    reserved_seq,
};
use officina_core::orders::permission::WorkOrderPermission;
use officina_core::orders::work_order::{SubjectChoice, cut_error};
use officina_core::registry::types::{
    LaunchOutcome, SubjectRecord, SubjectStatus, WorkOrderSource,
};
use officina_core::relay::catalogue::{ApprovedCatalogue, CanonicalCatalogue, RelayFloor};
use officina_core::relay::journal::{JournalEntry, WorkOrderLaunched};
use officina_core::relay::template::TemplateUnavailable;
use officina_core::spawn::ClaudeVector;

use crate::collector::Collector;
use crate::registry::orders::LaunchRecord;
use crate::registry::window::WindowRegistry;
use crate::relay::home::{RelayHome, load_relay_floor};
use crate::spawn::{self, BaseEnv, ResolvedClaude, SpawnError, SpawnOutcome};
use crate::watch::{AUTHOR_WAIT, AuthorCommand, CollectorMsg, ask};

/// How many cards the store keeps (D-135); the oldest goes first, and an
/// id it evicted is unknown.
pub const CARD_CAPACITY: usize = 32;

/// The cards of one window, in the order they were issued (AD-78). In
/// memory only: a card is cheap to issue again.
#[derive(Debug, Default)]
pub struct CardStore {
    cards: VecDeque<IssuedCard>,
    counter: u64,
}

impl CardStore {
    /// A new card id: [`card::CARD_ID_HEX`] hex characters from two
    /// `RandomState` hashes of `(now_ms, counter)` (AD-78, AD-51's source
    /// of randomness). Uniqueness matters here, not secrecy.
    pub fn new_id(&mut self, now_ms: i64) -> String {
        self.counter += 1;
        let counter = self.counter;
        let half = || {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_i64(now_ms);
            hasher.write_u64(counter);
            hasher.finish()
        };
        format!("{:016x}{:016x}", half(), half())
    }

    /// The `seq` a new card of `template` takes (D-135, review F2): one
    /// more than the larger of the registry's highest and the highest a
    /// card of the store holds, so two cards never share a name.
    pub fn reserve_seq(&self, template: &str, registry_highest: Option<u32>) -> u32 {
        reserved_seq(
            registry_highest,
            self.cards
                .iter()
                .filter(|issued| issued.card().template == template)
                .map(IssuedCard::seq),
        )
    }

    /// Keeps `issued`, evicting the oldest card past [`CARD_CAPACITY`].
    pub fn insert(&mut self, issued: IssuedCard) {
        while self.cards.len() >= CARD_CAPACITY {
            self.cards.pop_front();
        }
        self.cards.push_back(issued);
    }

    pub fn get(&self, id: &str) -> Option<&IssuedCard> {
        self.cards.iter().find(|issued| issued.id() == id)
    }

    fn mark(&mut self, id: &str, state: CardState) {
        if let Some(issued) = self.cards.iter_mut().find(|issued| issued.id() == id) {
            issued.mark(state);
        }
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

/// The approved catalogue the window's record confirms (D-175), and the
/// floor the relay reads: what a card is issued from.
pub(crate) fn catalogue(
    relay: &RelayHome,
) -> Result<(ApprovedCatalogue, RelayFloor), TemplateUnavailable> {
    let state = relay.read_approved();
    let current = load_relay_floor(&relay.relay_floor(&state));
    let approved = match state.approved() {
        Some(approved) => approved.clone(),
        None if matches!(state, crate::relay::home::ApprovedState::Missing) => {
            return Err(TemplateUnavailable::NotApproved);
        }
        None => return Err(TemplateUnavailable::CatalogueChanged),
    };
    if !confirmed(relay, &approved.hash) {
        return Err(TemplateUnavailable::CatalogueChanged);
    }
    let current = current.map_err(|_| TemplateUnavailable::CatalogueChanged)?;
    Ok((approved, current))
}

/// True when the window's record of its decisions names `hash` as the
/// catalogue it approved (D-175): an `approved.json` another process
/// wrote issues no card and launches nothing.
fn confirmed(relay: &RelayHome, hash: &str) -> bool {
    relay
        .read_record()
        .valid()
        .is_some_and(|record| record.approved_hash.as_deref() == Some(hash))
}

/// The floor's config dir, as every vector sets it (D-646).
fn config_dir(collector: &Collector) -> Option<String> {
    collector
        .paths()
        .claude_config_override
        .as_deref()
        .map(|dir| dir.to_string_lossy().into_owned())
}

/// Issues a card for `request` (AD-78) on the collector thread: the
/// registry's highest `seq` and the store's reserve the card's (D-135).
pub(crate) fn issue(
    store: &mut CardStore,
    collector: &Collector,
    registry: &mut WindowRegistry,
    quota: Option<&QuotaView>,
    request: &CardRequest,
    now_ms: i64,
) -> Result<CardAnswer, IpcError> {
    let (approved, current) = match catalogue(collector.relay()) {
        Ok(found) => found,
        Err(unavailable) => return Ok(CardAnswer::Refused(IssueRefusal::Template(unavailable))),
    };
    let (subjects, highest) = registry.card_reads(&request.template)?;
    let seq = store.reserve_seq(&request.template, highest);
    let id = store.new_id(now_ms);
    let config = config_dir(collector);
    let permissions = collector.relay().read_work_order_permissions().effective();
    let inputs = CardInputs {
        approved: &approved,
        current: &current,
        subjects: &subjects,
        quota,
        config_dir: config.as_deref(),
        permissions: &permissions,
        now_ms,
    };
    Ok(match card::issue(&id, seq, request, &inputs) {
        Ok(issued) => {
            let shown = issued.card().clone();
            store.insert(issued);
            CardAnswer::Issued(Box::new(shown))
        }
        Err(refusal) => CardAnswer::Refused(refusal),
    })
}

/// The approved snapshot the window's record confirms (D-175), which
/// proposals are validated against at ingestion (AD-91); none when no
/// catalogue is approved or the record does not vouch for it.
pub(crate) fn approved_catalogue(relay: &RelayHome) -> Option<CanonicalCatalogue> {
    catalogue(relay)
        .ok()
        .map(|(approved, _)| *approved.canonical)
}

/// Issues a card from the pending work order proposal `proposal_id`
/// (AD-91, D-152) on the collector thread: its stored values through
/// core's issue, as the author's request would go, with the `seq` the
/// registry and the store reserve. A proposal that opens no card is
/// answered with why.
pub(crate) fn issue_from_proposal(
    store: &mut CardStore,
    collector: &Collector,
    registry: &mut WindowRegistry,
    quota: Option<&QuotaView>,
    proposal_id: &str,
    now_ms: i64,
) -> Result<CardAnswer, IpcError> {
    let proposal = match registry.proposal_order(proposal_id, now_ms)? {
        Ok(proposal) => proposal,
        Err(refusal) => return Ok(CardAnswer::Refused(IssueRefusal::Proposal(refusal))),
    };
    let (approved, current) = match catalogue(collector.relay()) {
        Ok(found) => found,
        Err(unavailable) => return Ok(CardAnswer::Refused(IssueRefusal::Template(unavailable))),
    };
    let (subjects, highest) = registry.card_reads(&proposal.template)?;
    let seq = store.reserve_seq(&proposal.template, highest);
    let id = store.new_id(now_ms);
    let config = config_dir(collector);
    let permissions = collector.relay().read_work_order_permissions().effective();
    let inputs = CardInputs {
        approved: &approved,
        current: &current,
        subjects: &subjects,
        quota,
        config_dir: config.as_deref(),
        permissions: &permissions,
        now_ms,
    };
    Ok(
        match card::issue_from_proposal(&id, seq, &proposal, &inputs) {
            Ok(issued) => {
                let shown = issued.card().clone();
                store.insert(issued);
                CardAnswer::Issued(Box::new(shown))
            }
            Err(refusal) => CardAnswer::Refused(refusal),
        },
    )
}

/// Edits card `card_id` (AD-78): a new card under the same `seq`, the old
/// one superseded once the new one exists.
pub(crate) fn edit(
    store: &mut CardStore,
    collector: &Collector,
    registry: &mut WindowRegistry,
    quota: Option<&QuotaView>,
    card_id: &str,
    change: &CardChange,
    now_ms: i64,
) -> Result<CardAnswer, IpcError> {
    let Some(previous) = store.get(card_id).cloned() else {
        return Ok(CardAnswer::Refused(IssueRefusal::UnknownCard));
    };
    if previous.state() != CardState::Open {
        return Ok(CardAnswer::Refused(IssueRefusal::CardNotOpen(
            previous.state(),
        )));
    }
    let (approved, current) = match catalogue(collector.relay()) {
        Ok(found) => found,
        Err(unavailable) => return Ok(CardAnswer::Refused(IssueRefusal::Template(unavailable))),
    };
    let (subjects, _) = registry.card_reads(&previous.card().template)?;
    let id = store.new_id(now_ms);
    let config = config_dir(collector);
    let permissions = collector.relay().read_work_order_permissions().effective();
    let inputs = CardInputs {
        approved: &approved,
        current: &current,
        subjects: &subjects,
        quota,
        config_dir: config.as_deref(),
        permissions: &permissions,
        now_ms,
    };
    Ok(match card::edit(&previous, change, &id, &inputs) {
        Ok(issued) => {
            let shown = issued.card().clone();
            store.mark(card_id, CardState::Superseded);
            store.insert(issued);
            CardAnswer::Issued(Box::new(shown))
        }
        Err(refusal) => CardAnswer::Refused(refusal),
    })
}

/// Switches the work orders of card `card_id`'s project to `permission`
/// (story 5.24, D-983, D-985): the window's authority writes the choice
/// for the project and its folder as the card names them, and journals
/// it; then the card is issued again under its `seq`, so the card the
/// author sees carries the mode that will run, and the old one is
/// superseded. A card unknown or not open, a catalogue that issues no
/// card, or one approved since the card was issued, writes nothing.
/// Compiled with the window's authority only.
pub(crate) fn set_permission(
    store: &mut CardStore,
    collector: &Collector,
    registry: &mut WindowRegistry,
    quota: Option<&QuotaView>,
    card_id: &str,
    permission: WorkOrderPermission,
    now_ms: i64,
) -> Result<CardAnswer, IpcError> {
    let Some(previous) = store.get(card_id).cloned() else {
        return Ok(CardAnswer::Refused(IssueRefusal::UnknownCard));
    };
    if previous.state() != CardState::Open {
        return Ok(CardAnswer::Refused(IssueRefusal::CardNotOpen(
            previous.state(),
        )));
    }
    let (approved, current) = match catalogue(collector.relay()) {
        Ok(found) => found,
        Err(unavailable) => return Ok(CardAnswer::Refused(IssueRefusal::Template(unavailable))),
    };
    // The folder recorded is the one the approved catalogue resolves now.
    if previous.card().catalogue_hash != approved.hash {
        return Ok(CardAnswer::Refused(IssueRefusal::Template(
            TemplateUnavailable::CatalogueChanged,
        )));
    }
    let permissions = crate::relay::authority::set_work_order_permission(
        collector.relay(),
        &previous.card().project,
        previous.order().project_folder(),
        permission,
        now_ms,
    )
    .map_err(|e| e.ipc())?;
    let (subjects, _) = registry.card_reads(&previous.card().template)?;
    let id = store.new_id(now_ms);
    let config = config_dir(collector);
    let inputs = CardInputs {
        approved: &approved,
        current: &current,
        subjects: &subjects,
        quota,
        config_dir: config.as_deref(),
        permissions: &permissions,
        now_ms,
    };
    Ok(match card::reissue(&previous, &id, &inputs) {
        Ok(issued) => {
            let shown = issued.card().clone();
            store.mark(card_id, CardState::Superseded);
            store.insert(issued);
            CardAnswer::Issued(Box::new(shown))
        }
        Err(refusal) => CardAnswer::Refused(refusal),
    })
}

/// What a granted launch hands the window's command: the card's vector,
/// byte for byte, the `claude` and base environment the collector holds,
/// and the journal line its outcome completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTicket {
    pub order_id: String,
    pub vector: ClaudeVector,
    pub claude: ResolvedClaude,
    pub base_env: BaseEnv,
    /// The `work_order_launched` line, its outcome still `launching`.
    pub line: WorkOrderLaunched,
}

/// The collector's answer to a click: the ticket, or the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchGrant {
    Granted(Box<LaunchTicket>),
    Refused(LaunchRefusal),
}

/// AD-78's checks on card `card_id` against the click's `clicked_sha256`,
/// in their order; then the card is consumed and the `work_order` row
/// recorded `launching` with the card's `seq` (AD-81 step 1). A refusal
/// writes nothing and consumes nothing.
pub(crate) fn grant(
    store: &mut CardStore,
    collector: &Collector,
    registry: &mut WindowRegistry,
    card_id: &str,
    clicked_sha256: &str,
    now_ms: i64,
) -> Result<LaunchGrant, IpcError> {
    let Some(issued) = store.get(card_id).cloned() else {
        return Ok(LaunchGrant::Refused(LaunchRefusal::UnknownCard));
    };
    let shown = issued.card();
    let relay = collector.relay();
    let state = relay.read_approved();
    let approved = state.approved().filter(|a| confirmed(relay, &a.hash));
    let current = load_relay_floor(&relay.relay_floor(&state)).ok();
    let claude = collector.resolved_claude().filter(|c| c.is_native());
    let permission = relay
        .read_work_order_permissions()
        .effective()
        .of(&shown.project, issued.order().project_folder());
    // The registry's facts are the last two checks: they are read only
    // once every earlier check passed, so a registry that cannot be read
    // never hides an earlier refusal.
    let mut facts = LaunchFacts {
        card: Some(shown),
        clicked_sha256,
        approved_hash: approved.map(|a| a.hash.as_str()),
        current_hash: current.as_ref().map(|c| c.hash.as_str()),
        permission,
        template_approved: approved
            .is_some_and(|a| a.canonical.templates.contains_key(&shown.template)),
        claude_native: claude.is_some(),
        folder_exists: Path::new(issued.order().project_folder()).is_dir(),
        subject_open: true,
        name_taken: false,
    };
    if let Err(refusal) = check_launch(&facts) {
        return Ok(LaunchGrant::Refused(refusal));
    }
    let (subjects, name_taken) = registry.launch_reads(&shown.template, shown.seq)?;
    facts.subject_open = subject_open(&shown.subject, &subjects);
    facts.name_taken = name_taken;
    if let Err(refusal) = check_launch(&facts) {
        return Ok(LaunchGrant::Refused(refusal));
    }
    let Some(claude) = claude.cloned() else {
        return Ok(LaunchGrant::Refused(LaunchRefusal::ClaudeNotExecutable));
    };
    // A card opened from a proposal records it, and accepts it (AD-91).
    let proposal_id = shown.proposal.as_ref().map(|p| p.id.as_str());
    let record = LaunchRecord {
        order: issued.order(),
        card_id,
        seq: issued.seq(),
        source: if proposal_id.is_some() {
            WorkOrderSource::Proposal
        } else {
            WorkOrderSource::Composer
        },
        subject: &shown.subject,
        proposal_id,
        permission: shown.permission,
    };
    if registry.insert_work_order(&record, now_ms)?.is_err() {
        return Ok(LaunchGrant::Refused(LaunchRefusal::NameTaken));
    }
    store.mark(card_id, CardState::Consumed);
    Ok(LaunchGrant::Granted(Box::new(LaunchTicket {
        order_id: card_id.to_owned(),
        vector: issued.vector().clone(),
        claude,
        base_env: collector.base_env().clone(),
        line: WorkOrderLaunched {
            order_id: card_id.to_owned(),
            name: shown.name.clone(),
            template: shown.template.clone(),
            project: shown.project.clone(),
            values: card_values(shown),
            argv: issued.vector().args().to_vec(),
            prompt_sha256: shown.prompt_sha256.clone(),
            catalogue_hash: shown.catalogue_hash.clone(),
            outcome: LaunchOutcome::Launching,
            exit_code: None,
        },
    })))
}

/// True when `subject` is new, or exists and is not archived.
fn subject_open(subject: &SubjectChoice, subjects: &[SubjectRecord]) -> bool {
    match subject {
        SubjectChoice::New(_) => true,
        SubjectChoice::Existing(id) => subjects
            .iter()
            .any(|s| s.id == *id && s.status.value != SubjectStatus::Archived),
    }
}

/// How a launch ended, as the registry records it (AD-81 step 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchResult {
    pub outcome: LaunchOutcome,
    pub exit_code: Option<i32>,
    /// The first characters of stderr, or why nothing ran to an exit.
    pub error: Option<String>,
    /// What `claude --bg` printed, display only (the registry keeps its
    /// first 4 KiB as untrusted text).
    pub printed: Option<String>,
}

/// The outcome of one run of a work order's vector (AD-81 step 3): exit 0
/// is `started` with stdout, also when a process `claude` started still
/// holds the output pipes (D-128); stderr holding
/// [`WORKSPACE_NOT_TRUSTED`] is `untrusted_workspace`; any other exit is
/// `failed` with the code and the start of stderr; the deadline is
/// `failed` with `timeout`.
pub fn launch_result(ran: Result<SpawnOutcome, SpawnError>) -> LaunchResult {
    let failed = |error: String| LaunchResult {
        outcome: LaunchOutcome::Failed,
        exit_code: None,
        error: Some(error),
        printed: None,
    };
    match ran {
        Ok(SpawnOutcome::Captured(ran)) if ran.status.success() => LaunchResult {
            outcome: LaunchOutcome::Started,
            exit_code: Some(0),
            error: None,
            printed: Some(String::from_utf8_lossy(&ran.stdout).into_owned()),
        },
        Ok(SpawnOutcome::Captured(ran)) => {
            let stderr = String::from_utf8_lossy(&ran.stderr);
            let outcome = if stderr.contains(WORKSPACE_NOT_TRUSTED) {
                LaunchOutcome::UntrustedWorkspace
            } else {
                LaunchOutcome::Failed
            };
            LaunchResult {
                outcome,
                exit_code: ran.status.code(),
                error: Some(cut_error(stderr.trim()).to_owned()),
                printed: None,
            }
        }
        Ok(SpawnOutcome::Started { .. }) => failed("claude was not captured".to_owned()),
        Err(error) => match error.cause() {
            SpawnError::Timeout(_) => failed("timeout".to_owned()),
            cause => failed(cause.to_string()),
        },
    }
}

/// A launch's outcome as the window's command sends it back: the order,
/// its line and how it ended. The collector records it whatever its
/// deadline (D-135, review F14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchReport {
    pub order_id: String,
    pub line: WorkOrderLaunched,
    pub result: LaunchResult,
}

/// Records `report` on the collector thread (AD-81 step 3): the row's
/// outcome, then one `work_order_launched` line.
pub(crate) fn record(
    collector: &Collector,
    registry: &mut WindowRegistry,
    report: &LaunchReport,
    now_ms: i64,
) -> Result<(), IpcError> {
    registry.record_launch_outcome(&report.order_id, &report.result, now_ms)?;
    let line = WorkOrderLaunched {
        outcome: report.result.outcome,
        exit_code: report.result.exit_code.map(i64::from),
        ..report.line.clone()
    };
    crate::relay::journal::record(
        collector.relay(),
        &[JournalEntry::WorkOrderLaunched(line)],
        now_ms,
    )
    .map_err(|e| IpcError::new(IpcErrorCode::JournalUnavailable, e.to_string()))
}

/// The launch of card `card_id` at a click that sent `prompt_sha256`
/// (AD-81 steps 1 to 3): the collector checks and records, this thread
/// runs the card's vector, and the outcome goes back to the collector,
/// which records it whenever it arrives. The window's command calls it in
/// `spawn_blocking`; the collector never waits on `claude`.
pub fn launch_card(
    collector: &Sender<CollectorMsg>,
    card_id: &str,
    prompt_sha256: &str,
) -> Result<LaunchAnswer, IpcError> {
    let command = AuthorCommand::LaunchCard {
        card_id: card_id.to_owned(),
        prompt_sha256: prompt_sha256.to_owned(),
    };
    let ticket = match ask(collector, command, AUTHOR_WAIT)?.into_launch()? {
        LaunchGrant::Refused(refusal) => return Ok(LaunchAnswer::Refused(refusal)),
        LaunchGrant::Granted(ticket) => ticket,
    };
    let result = launch_result(spawn::execute(
        &ticket.claude,
        &ticket.vector,
        &ticket.base_env,
    ));
    let outcome = result.outcome;
    let report = LaunchReport {
        order_id: ticket.order_id,
        line: ticket.line,
        result,
    };
    // Recorded whenever the collector reaches it; a window that stops first
    // leaves the row `launching`, which reads `unknown` later (D-136).
    let _ = ask(
        collector,
        AuthorCommand::LaunchOutcome(Box::new(report)),
        AUTHOR_WAIT,
    );
    Ok(LaunchAnswer::Launched(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::catalogue::load;

    const FLOOR_PATH: &str = "C:/path/to/floor/officina.toml";
    const FLOOR: &str = "schema = 2
[[project]]
name = \"backend\"
path = \"Backend\"
[[template]]
name = \"bump\"
description = \"Bump one dependency\"
projects = [\"backend\"]
prompt = \"Bump {package}.\"
[template.params.package]
values = [\"serde_json\", \"tokio\"]
[[template]]
name = \"notes\"
description = \"Draft the notes\"
projects = [\"backend\"]
prompt = \"Draft the notes.\"
";

    /// A card of `template` with `seq`, issued as the store would.
    fn issued(id: &str, template: &str, seq: u32) -> IssuedCard {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let approved = ApprovedCatalogue::approving(&current, FLOOR_PATH, 0);
        let values = if template == "bump" {
            vec![("package".to_owned(), "serde_json".to_owned())]
        } else {
            Vec::new()
        };
        card::issue(
            id,
            seq,
            &CardRequest {
                template: template.to_owned(),
                project: "backend".to_owned(),
                values,
                subject: None,
            },
            &CardInputs {
                approved: &approved,
                current: &current,
                subjects: &[],
                quota: None,
                config_dir: None,
                permissions: &officina_core::orders::permission::WorkOrderPermissions::default(),
                now_ms: 0,
            },
        )
        .unwrap()
    }

    #[test]
    fn the_33rd_card_evicts_the_oldest() {
        let mut store = CardStore::default();
        let ids: Vec<String> = (0..=CARD_CAPACITY)
            .map(|at| {
                let id = store.new_id(at as i64);
                store.insert(issued(&id, "bump", at as u32 + 1));
                id
            })
            .collect();
        assert_eq!(store.len(), CARD_CAPACITY);
        assert!(store.get(&ids[0]).is_none(), "the oldest went");
        for id in &ids[1..] {
            assert!(store.get(id).is_some(), "{id}");
        }
    }

    #[test]
    fn card_ids_are_32_hex_and_distinct() {
        let mut store = CardStore::default();
        let ids: std::collections::BTreeSet<String> = (0..1_000)
            .map(|_| store.new_id(1_793_102_400_000))
            .collect();
        assert_eq!(ids.len(), 1_000, "distinct at the same millisecond");
        for id in &ids {
            assert_eq!(id.len(), card::CARD_ID_HEX, "{id}");
            assert!(
                id.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            );
        }
    }

    #[test]
    fn two_cards_of_one_template_get_distinct_seqs() {
        let mut store = CardStore::default();
        let first = store.reserve_seq("bump", None);
        store.insert(issued("a", "bump", first));
        let second = store.reserve_seq("bump", None);
        assert_eq!((first, second), (1, 2));
        store.insert(issued("b", "bump", second));
        // Another template counts from its own.
        assert_eq!(store.reserve_seq("notes", None), 1);
        // An edit keeps its card's seq, so the store's highest stays.
        assert_eq!(store.reserve_seq("bump", None), 3);
    }

    #[test]
    fn a_reserved_seq_follows_the_registry_highest() {
        let mut store = CardStore::default();
        assert_eq!(store.reserve_seq("bump", Some(41)), 42);
        store.insert(issued("a", "bump", 42));
        assert_eq!(store.reserve_seq("bump", Some(41)), 43, "the store's card");
        assert_eq!(
            store.reserve_seq("bump", Some(50)),
            51,
            "the registry's row"
        );
    }

    #[test]
    fn an_exit_maps_to_its_outcome() {
        let timeout = launch_result(Err(SpawnError::Timeout(60)));
        assert_eq!(
            (timeout.outcome, timeout.error.as_deref()),
            (LaunchOutcome::Failed, Some("timeout"))
        );
        let refused = launch_result(Err(SpawnError::NotNative("C:/x/claude.cmd".into())));
        assert_eq!(refused.outcome, LaunchOutcome::Failed);
    }
}
