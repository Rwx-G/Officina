//! Workshop cards (AD-107, FR-6, D-239): what a card of a template with
//! `[template.workshop]` shows beside v0.5's, built from the facts the host
//! gathered on thread `workshop`, and the workshop's launch checks in
//! AD-107's reporting order (D-247). The card's run is built by
//! [`WorkshopLaunch::new`] from the same facts, so the plan hash the card
//! shows is the one a launch compares.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::card::{
    CardInputs, CardProposal, CardRequest, CardState, IssueRefusal, IssuedCard, LaunchRefusal,
    ParamChoice, SubjectOption, WorkOrderCard, checked_subject, default_subject, open_subjects,
};
use super::work_order::{WorkOrder, WorkOrderRefusal, order_name};
use crate::relay::template::{ParamName, ProjectName, TemplateName, TemplateUnavailable};
use crate::workshop::floor::CredentialKind;
use crate::workshop::launch::{
    ApprovedWorkshopTemplate, BaseChoice, CredentialState, LaunchFacts, WorkshopLaunch,
    WorkshopTemplateUnavailable, allow_list, claude_argv, plan_sha256,
};
use crate::workshop::proxy::{ProxyRefusal, Route};
use crate::workshop::vector::GitIdentity;

/// How long a branch imported from a run stays offered as a base
/// (AD-107).
pub const IMPORTED_BASE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// How many packages awaiting a verdict refuse a launch (AD-107).
pub const MAX_PENDING_REVIEWS: u32 = 10;

/// How many characters of an image id the card shows.
pub const SHORT_ID_CHARS: usize = 12;

/// The detail of a `base_moved` refusal whose branch does not resolve.
pub const UNRESOLVED_DETAIL: &str = "unresolved";

/// One base a card offers (AD-107): the branch, and its tip when it
/// resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BaseOption {
    pub choice: BaseChoice,
    pub tip: Option<String>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the host found of one mount (FR-2, D-231, D-236).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum MountState {
        /// A folder, git-ignored, holding no tracked file.
        Ready,
        /// No folder at the path.
        Missing,
        /// Not ignored by any rule.
        NotIgnored,
        /// Ignored, but the base tracks files under it (review F3).
        Tracked,
        /// A comma, a quote or a control character in its path.
        PathUnsupported,
        /// A reparse point on its path, or a canonical path leaving the
        /// project (review F11).
        ReparsePoint,
    }
}

impl MountState {
    /// The launch refusal this state gives, with its detail.
    pub fn refusal(self) -> Option<(LaunchRefusal, Option<&'static str>)> {
        match self {
            MountState::Ready => None,
            MountState::Missing => Some((LaunchRefusal::MountMissing, None)),
            MountState::NotIgnored => Some((LaunchRefusal::MountNotIgnored, None)),
            MountState::Tracked => Some((LaunchRefusal::MountNotIgnored, Some("tracked"))),
            MountState::PathUnsupported => Some((LaunchRefusal::MountPathUnsupported, None)),
            MountState::ReparsePoint => {
                Some((LaunchRefusal::MountPathUnsupported, Some("reparse_point")))
            }
        }
    }
}

/// One mount as the card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MountFact {
    /// Relative to the project (AD-101).
    pub path: String,
    pub state: MountState,
}

/// One allow-list entry and how it leaves (D-611): `route` is the
/// proxy's `host:port`, or none for direct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AllowEntry {
    pub host: String,
    pub route: Option<String>,
}

/// The image a run would use, as the card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageFact {
    pub tag: String,
    /// The first [`SHORT_ID_CHARS`] of its id, past `sha256:`, when it
    /// passed the image check.
    pub short_id: Option<String>,
    pub built_at_ms: Option<i64>,
}

/// One failing check, as the card's readiness shows it (AD-107).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Readiness {
    pub refusal: LaunchRefusal,
    pub detail: Option<String>,
}

/// What a workshop card shows beside v0.5's (AD-107, AD-121).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkshopCardFacts {
    /// The template's bases, then the branches imported in the last 7
    /// days; the base `<select>` offers these only.
    pub bases: Vec<BaseOption>,
    /// The base this card runs from; the first listed one by default.
    pub base: BaseChoice,
    /// The project's uncommitted changes, which a run never carries.
    pub uncommitted: u32,
    pub mounts: Vec<MountFact>,
    pub allow: Vec<AllowEntry>,
    pub image: ImageFact,
    pub max_minutes: u32,
    /// Checked after each turn, never a hard limit (D-1134).
    pub max_usd_cents: Option<u32>,
    pub credential_kind: CredentialKind,
    /// Every check without a side effect that fails now, in AD-107's
    /// order; empty when the card may launch.
    pub readiness: Vec<Readiness>,
    /// AD-102's hash of what would run, when the base, the image and the
    /// gateway image resolve.
    pub plan_sha256: Option<String>,
}

/// What the workshop's launch checks read (AD-107, D-247): the collector
/// fills its stored facts, thread `workshop` the rest. The inventory starts
/// a container, so it runs last and only when every other check passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkshopChecks {
    pub workshop_on: bool,
    pub stopped: bool,
    pub engine_ready: bool,
    pub image_matches: bool,
    pub battery_valid: bool,
    pub mounts: Vec<MountState>,
    pub live_runs: u32,
    pub max_workers: u32,
    pub reviews_pending: u32,
    pub credential_kind: CredentialKind,
    /// Whether the floor's credential is stored: its Credential Manager
    /// entry for a token kind, its login volume for `login` (story 6.26).
    pub credential_present: bool,
    pub credential_state: CredentialState,
    pub proxy: Option<ProxyRefusal>,
    pub project_exists: bool,
    /// `None` when the base does not resolve.
    pub base_matches: Option<bool>,
    pub plan_matches: bool,
    /// `None` until the inventory ran.
    pub hooks_found: Option<bool>,
}

/// Every failing check of `checks`, in AD-107's order (D-247): the order
/// [`LaunchRefusal`] declares them.
pub fn readiness(checks: &WorkshopChecks) -> Vec<Readiness> {
    let mut found: Vec<Readiness> = Vec::new();
    let mut fail = |refusal: LaunchRefusal, detail: Option<&str>| {
        found.push(Readiness {
            refusal,
            detail: detail.map(str::to_owned),
        });
    };
    if !checks.workshop_on {
        fail(LaunchRefusal::WorkshopOff, None);
    }
    if checks.stopped {
        fail(LaunchRefusal::WorkshopStopped, None);
    }
    if !checks.engine_ready {
        fail(LaunchRefusal::EngineUnavailable, None);
    }
    if !checks.image_matches {
        fail(LaunchRefusal::ImageMismatch, None);
    }
    if !checks.battery_valid {
        fail(LaunchRefusal::BatteryStale, None);
    }
    for state in &checks.mounts {
        if let Some((refusal, detail)) = state.refusal() {
            fail(refusal, detail);
        }
    }
    if checks.hooks_found == Some(false) {
        fail(LaunchRefusal::HookProgramMissing, None);
    }
    if checks.live_runs >= checks.max_workers {
        fail(LaunchRefusal::WorkshopFull, None);
    }
    if checks.reviews_pending >= MAX_PENDING_REVIEWS {
        fail(LaunchRefusal::ReviewsPending, None);
    }
    if !checks.credential_present || checks.credential_state == CredentialState::Missing {
        fail(LaunchRefusal::CredentialMissing, None);
    }
    if checks.credential_state.needs_new() {
        fail(LaunchRefusal::CredentialExpired, None);
    }
    match checks.proxy {
        Some(ProxyRefusal::ProxyUnresolved) => fail(LaunchRefusal::ProxyUnresolved, None),
        Some(ProxyRefusal::ProxyUnsupported) => fail(LaunchRefusal::ProxyUnsupported, None),
        None => {}
    }
    if !checks.project_exists {
        fail(LaunchRefusal::ProjectMissing, None);
    }
    match checks.base_matches {
        None => fail(LaunchRefusal::BaseMoved, Some(UNRESOLVED_DETAIL)),
        Some(false) => fail(LaunchRefusal::BaseMoved, None),
        Some(true) => {}
    }
    if !checks.plan_matches {
        fail(LaunchRefusal::PlanChanged, None);
    }
    found.sort_by_key(|r| order_of(r.refusal));
    found
}

/// The place of `refusal` in AD-107's order.
fn order_of(refusal: LaunchRefusal) -> usize {
    LaunchRefusal::ALL
        .iter()
        .position(|r| *r == refusal)
        .unwrap_or(usize::MAX)
}

/// The launch's workshop checks (AD-107, D-247): every check without a
/// side effect first; when none fails, `inventory` runs (it starts a
/// container) and says whether every hook's program was found. The
/// refusal reported is the first failing one in AD-107's order.
pub fn check_workshop_launch(
    checks: &WorkshopChecks,
    inventory: impl FnOnce() -> bool,
) -> Result<(), LaunchRefusal> {
    if let Some(first) = readiness(checks).first() {
        return Err(first.refusal);
    }
    let mut checked = checks.clone();
    checked.hooks_found = Some(inventory());
    match readiness(&checked).first() {
        Some(first) => Err(first.refusal),
        None => Ok(()),
    }
}

/// A branch imported from one of the template's runs (AD-107).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedBase {
    pub branch: String,
    pub parent_run_id: String,
    pub tip: Option<String>,
}

/// The image the run would use, once it passed the image check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltImage {
    pub image_id: String,
    pub built_at_ms: Option<i64>,
}

/// What the host gathered for a workshop card (AD-107): the git, engine,
/// proxy and credential-presence facts thread `workshop` read, and the
/// stored facts the collector gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatheredFacts {
    /// Each base of the template's list and its tip.
    pub listed_tips: BTreeMap<String, Option<String>>,
    pub imported: Vec<ImportedBase>,
    pub uncommitted: u32,
    pub mounts: Vec<MountFact>,
    /// Each mount's source on the host, in the project's order.
    pub mount_sources: Vec<String>,
    pub image_tag: String,
    pub image: Option<BuiltImage>,
    pub gateway_image_id: Option<String>,
    pub floor_key: String,
    pub officina_version: String,
    pub git: GitIdentity,
    /// Every allow-list entry's route, or why they could not be decided.
    pub routes: Result<BTreeMap<String, Route>, ProxyRefusal>,
    /// Every other check's facts; `base_matches` and `plan_matches` are
    /// decided from the card.
    pub checks: WorkshopChecks,
}

/// What a workshop card keeps besides its v0.5 fields: the template and
/// facts it was issued from, and the run they build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkshopIssue {
    pub template: ApprovedWorkshopTemplate,
    pub gathered: GatheredFacts,
    pub base: BaseChoice,
    /// `None` when the base, the image or the gateway image does not
    /// resolve.
    pub launch: Option<WorkshopLaunch>,
}

/// The bases `gathered` offers for `template`: its list, then the
/// imported branches.
fn bases(template: &ApprovedWorkshopTemplate, gathered: &GatheredFacts) -> Vec<BaseOption> {
    let listed = template.workshop().base.iter().map(|branch| BaseOption {
        choice: BaseChoice::Listed {
            branch: branch.clone(),
        },
        tip: gathered.listed_tips.get(branch).cloned().flatten(),
    });
    let imported = gathered.imported.iter().map(|base| BaseOption {
        choice: BaseChoice::Imported {
            branch: base.branch.clone(),
            parent_run_id: base.parent_run_id.clone(),
        },
        tip: base.tip.clone(),
    });
    listed.chain(imported).collect()
}

fn short_id(image_id: &str) -> String {
    image_id
        .strip_prefix("sha256:")
        .unwrap_or(image_id)
        .chars()
        .take(SHORT_ID_CHARS)
        .collect()
}

/// The run of `order` from `base` at `tip`, when the images resolve.
fn launch_of(
    order: &WorkOrder,
    template: &ApprovedWorkshopTemplate,
    base: &BaseChoice,
    tip: Option<&str>,
    gathered: &GatheredFacts,
    id: &str,
    seq: u32,
) -> Option<WorkshopLaunch> {
    let facts = LaunchFacts {
        base_commit: tip?.to_owned(),
        image: gathered.image_tag.clone(),
        image_id: gathered.image.as_ref()?.image_id.clone(),
        gateway_image_id: gathered.gateway_image_id.clone()?,
        floor_key: gathered.floor_key.clone(),
        officina_version: gathered.officina_version.clone(),
        mount_sources: gathered.mount_sources.clone(),
        git: gathered.git.clone(),
        routes: gathered
            .routes
            .as_ref()
            .map(gateway_routes)
            .unwrap_or_default(),
        seq,
        run_id: id.to_owned(),
    };
    WorkshopLaunch::new(order, template, base.clone(), &facts).ok()
}

fn gateway_routes(routes: &BTreeMap<String, Route>) -> BTreeMap<String, String> {
    routes
        .iter()
        .filter_map(|(entry, route)| match route {
            Route::Proxy(proxy) => Some((entry.clone(), proxy.clone())),
            Route::Direct => None,
        })
        .collect()
}

/// The workshop card `id` for `request` with the `seq` the store reserved
/// (AD-107): v0.5's validation of the order and the subject, then the
/// facts `gathered` holds, from `base` or the first listed base. A base
/// the card would not offer is refused.
pub fn issue_workshop(
    id: &str,
    seq: u32,
    request: &CardRequest,
    inputs: &CardInputs,
    gathered: &GatheredFacts,
    base: Option<&BaseChoice>,
) -> Result<IssuedCard, IssueRefusal> {
    let name = TemplateName::parse(&request.template)
        .ok_or(IssueRefusal::Template(TemplateUnavailable::NotApproved))?;
    let template =
        ApprovedWorkshopTemplate::new(inputs.approved, inputs.current, &name).map_err(|why| {
            match why {
                WorkshopTemplateUnavailable::Unavailable(u) => IssueRefusal::Template(u),
                WorkshopTemplateUnavailable::NotWorkshop => IssueRefusal::NotApplicable,
                WorkshopTemplateUnavailable::WorkshopOff => {
                    IssueRefusal::Workshop(LaunchRefusal::WorkshopOff)
                }
            }
        })?;
    let project = ProjectName::parse(&request.project)
        .filter(|project| template.template().project_folder(project).is_some())
        .filter(|project| template.project(project).is_some())
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
    let order =
        WorkOrder::validate(template.template(), &project, &values).map_err(IssueRefusal::Order)?;
    let wanted = request
        .subject
        .clone()
        .unwrap_or_else(|| default_subject(&order, inputs.subjects));
    let subject = checked_subject(&wanted, &order, inputs.subjects)?;
    let offered = bases(&template, gathered);
    let chosen = match base {
        Some(base) => offered
            .iter()
            .find(|option| option.choice == *base)
            .ok_or(IssueRefusal::BaseNotOffered)?,
        None => offered.first().ok_or(IssueRefusal::BaseNotOffered)?,
    }
    .clone();
    let launch = launch_of(
        &order,
        &template,
        &chosen.choice,
        chosen.tip.as_deref(),
        gathered,
        id,
        seq,
    );
    let mut checks = gathered.checks.clone();
    checks.base_matches = chosen.tip.is_some().then_some(true);
    checks.plan_matches = true;
    let workshop = template.workshop();
    let allow = allow_list(
        template.floor().auth,
        &template
            .project(&project)
            .map(|p| p.hosts.clone())
            .unwrap_or_default(),
    );
    let facts = WorkshopCardFacts {
        bases: offered,
        base: chosen.choice.clone(),
        uncommitted: gathered.uncommitted,
        mounts: gathered.mounts.clone(),
        allow: allow
            .into_iter()
            .map(|host| AllowEntry {
                route: match gathered.routes.as_ref().ok().and_then(|r| r.get(&host)) {
                    Some(Route::Proxy(proxy)) => Some(proxy.clone()),
                    _ => None,
                },
                host,
            })
            .collect(),
        image: ImageFact {
            tag: gathered.image_tag.clone(),
            short_id: gathered.image.as_ref().map(|i| short_id(&i.image_id)),
            built_at_ms: gathered.image.as_ref().and_then(|i| i.built_at_ms),
        },
        max_minutes: workshop.max_minutes,
        max_usd_cents: workshop.max_usd_cents,
        credential_kind: template.floor().auth,
        readiness: readiness(&checks),
        plan_sha256: launch.as_ref().map(plan_sha256),
    };
    let permission = inputs
        .permissions
        .of(order.project(), order.project_folder());
    let card = WorkOrderCard {
        id: id.to_owned(),
        template: order.template().to_owned(),
        description: template.template().description().to_owned(),
        project: order.project().to_owned(),
        values: template
            .template()
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
        argv: claude_argv(workshop.max_usd_cents, order.model().map(|m| m.code())),
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
        proposal: None::<CardProposal>,
        permission,
        workshop: Some(facts),
    };
    Ok(IssuedCard::workshop(
        card,
        order,
        WorkshopIssue {
            template,
            gathered: gathered.clone(),
            base: chosen.choice,
            launch,
        },
    ))
}

/// The checks of a launch of `issue` against the facts gathered at the
/// click (AD-107, D-207, D-611): the base's tip and the plan hash rebuilt
/// from `fresh`, compared with the card's and the click's.
pub fn launch_checks(
    issue: &WorkshopIssue,
    order: &WorkOrder,
    fresh: &GatheredFacts,
    card_id: &str,
    seq: u32,
    clicked_plan: Option<&str>,
) -> WorkshopChecks {
    let tip = match &issue.base {
        BaseChoice::Listed { branch } => fresh.listed_tips.get(branch).cloned().flatten(),
        BaseChoice::Imported { branch, .. } => fresh
            .imported
            .iter()
            .find(|b| &b.branch == branch)
            .and_then(|b| b.tip.clone()),
    };
    let card_commit = issue.launch.as_ref().map(|l| l.facts().base_commit.clone());
    let rebuilt = launch_of(
        order,
        &issue.template,
        &issue.base,
        tip.as_deref(),
        fresh,
        card_id,
        seq,
    );
    let card_plan = issue.launch.as_ref().map(plan_sha256);
    let mut checks = fresh.checks.clone();
    checks.base_matches = tip.as_ref().map(|tip| Some(tip) == card_commit.as_ref());
    checks.plan_matches = card_plan.is_some()
        && card_plan == rebuilt.as_ref().map(plan_sha256)
        && card_plan.as_deref() == clicked_plan;
    checks
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::orders::card::{CardChange, edit};
    use crate::orders::permission::WorkOrderPermissions;
    use crate::relay::catalogue::{ApprovedCatalogue, RelayFloor, load};
    use crate::workshop::launch::tests::{FLOOR_PATH, floor};

    pub(crate) const CARD: &str = "0123456789abcdef0123456789abcdef";
    pub(crate) const MAIN_TIP: &str = "0123456789abcdef0123456789abcdef01234567";
    pub(crate) const DEVELOP_TIP: &str = "89abcdef0123456789abcdef0123456789abcdef";
    pub(crate) const IMPORTED_TIP: &str = "fedcba9876543210fedcba9876543210fedcba98";
    pub(crate) const PARENT: &str = "fedcba9876543210fedcba9876543210";

    pub(crate) fn passing() -> WorkshopChecks {
        WorkshopChecks {
            workshop_on: true,
            stopped: false,
            engine_ready: true,
            image_matches: true,
            battery_valid: true,
            mounts: vec![MountState::Ready],
            live_runs: 0,
            max_workers: 2,
            reviews_pending: 0,
            credential_kind: CredentialKind::OauthToken,
            credential_present: true,
            credential_state: CredentialState::Unchecked,
            proxy: None,
            project_exists: true,
            base_matches: Some(true),
            plan_matches: true,
            hooks_found: None,
        }
    }

    pub(crate) fn gathered() -> GatheredFacts {
        GatheredFacts {
            listed_tips: BTreeMap::from([
                ("main".to_owned(), Some(MAIN_TIP.to_owned())),
                ("develop".to_owned(), Some(DEVELOP_TIP.to_owned())),
            ]),
            imported: vec![ImportedBase {
                branch: "officina/bump-3".to_owned(),
                parent_run_id: PARENT.to_owned(),
                tip: Some(IMPORTED_TIP.to_owned()),
            }],
            uncommitted: 2,
            mounts: vec![MountFact {
                path: ".claude".to_owned(),
                state: MountState::Ready,
            }],
            mount_sources: vec!["C:/path/to/Api/.claude".to_owned()],
            image_tag: "officina-workshop:local".to_owned(),
            image: Some(BuiltImage {
                image_id: format!("sha256:{}", "1".repeat(64)),
                built_at_ms: Some(1_793_000_000_000),
            }),
            gateway_image_id: Some(format!("sha256:{}", "2".repeat(64))),
            floor_key: "0123456789abcdef".to_owned(),
            officina_version: "0.1.0".to_owned(),
            git: GitIdentity {
                name: "User One".to_owned(),
                email: "user01@example.com".to_owned(),
            },
            routes: Ok(BTreeMap::from([(
                "index.crates.io".to_owned(),
                Route::Proxy("proxy.example.com:8080".to_owned()),
            )])),
            checks: passing(),
        }
    }

    pub(crate) fn catalogue() -> (ApprovedCatalogue, RelayFloor) {
        let current = load(&floor(""), FLOOR_PATH).unwrap();
        let approved = ApprovedCatalogue::approving(&current, FLOOR_PATH, 0);
        (approved, current)
    }

    pub(crate) fn request() -> CardRequest {
        CardRequest {
            template: "bump".to_owned(),
            project: "api".to_owned(),
            values: vec![("package".to_owned(), "serde_json".to_owned())],
            subject: None,
        }
    }

    pub(crate) fn issue_with(
        gathered: &GatheredFacts,
        base: Option<&BaseChoice>,
    ) -> Result<IssuedCard, IssueRefusal> {
        let (approved, current) = catalogue();
        let permissions = WorkOrderPermissions::default();
        let inputs = CardInputs {
            approved: &approved,
            current: &current,
            subjects: &[],
            quota: None,
            config_dir: None,
            permissions: &permissions,
            now_ms: 0,
        };
        issue_workshop(CARD, 4, &request(), &inputs, gathered, base)
    }

    #[test]
    fn a_workshop_card_carries_its_facts() {
        let issued = issue_with(&gathered(), None).unwrap();
        let facts = issued.card().workshop.clone().unwrap();
        assert_eq!(facts.uncommitted, 2);
        assert_eq!(facts.mounts[0].state, MountState::Ready);
        assert_eq!(facts.max_minutes, 30);
        assert_eq!(facts.max_usd_cents, Some(250));
        assert_eq!(facts.credential_kind, CredentialKind::OauthToken);
        assert_eq!(facts.image.short_id.as_deref(), Some("111111111111"));
        assert!(facts.readiness.is_empty(), "{:?}", facts.readiness);
        let routed = facts
            .allow
            .iter()
            .find(|e| e.host == "index.crates.io")
            .unwrap();
        assert_eq!(routed.route.as_deref(), Some("proxy.example.com:8080"));
        assert!(
            facts.allow.iter().any(|e| e.route.is_none()),
            "direct entries"
        );
        let launch = issued.workshop_issue().unwrap().launch.clone().unwrap();
        assert_eq!(facts.plan_sha256, Some(plan_sha256(&launch)));
        assert_eq!(launch.facts().base_commit, MAIN_TIP);
        assert_eq!(
            issued.card().argv,
            crate::workshop::launch::run_plan(&launch).claude_argv
        );
        assert!(
            issued.vector().is_none(),
            "a workshop card has no host vector"
        );
    }

    #[test]
    fn the_first_base_is_the_default() {
        let issued = issue_with(&gathered(), None).unwrap();
        let facts = issued.card().workshop.clone().unwrap();
        assert_eq!(
            facts.base,
            BaseChoice::Listed {
                branch: "main".to_owned()
            }
        );
        let order: Vec<&str> = facts.bases.iter().map(|b| b.choice.branch()).collect();
        assert_eq!(order, ["main", "develop", "officina/bump-3"]);
    }

    #[test]
    fn an_imported_branch_names_its_parent() {
        let base = BaseChoice::Imported {
            branch: "officina/bump-3".to_owned(),
            parent_run_id: PARENT.to_owned(),
        };
        let issued = issue_with(&gathered(), Some(&base)).unwrap();
        let facts = issued.card().workshop.clone().unwrap();
        assert_eq!(facts.base.parent_run_id(), Some(PARENT));
        let launch = issued.workshop_issue().unwrap().launch.clone().unwrap();
        assert_eq!(launch.facts().base_commit, IMPORTED_TIP);
    }

    #[test]
    fn a_base_outside_the_list_is_refused() {
        let base = BaseChoice::Listed {
            branch: "release".to_owned(),
        };
        assert_eq!(
            issue_with(&gathered(), Some(&base)).unwrap_err(),
            IssueRefusal::BaseNotOffered
        );
        let forged = BaseChoice::Imported {
            branch: "officina/bump-3".to_owned(),
            parent_run_id: CARD.to_owned(),
        };
        assert_eq!(
            issue_with(&gathered(), Some(&forged)).unwrap_err(),
            IssueRefusal::BaseNotOffered,
            "an imported branch under another parent"
        );
    }

    #[test]
    fn edit_base_keeps_the_seq() {
        let (approved, current) = catalogue();
        let permissions = WorkOrderPermissions::default();
        let inputs = CardInputs {
            approved: &approved,
            current: &current,
            subjects: &[],
            quota: None,
            config_dir: None,
            permissions: &permissions,
            now_ms: 0,
        };
        let first = issue_with(&gathered(), None).unwrap();
        let second_id = "fedcba9876543210fedcba9876543211";
        let develop = BaseChoice::Listed {
            branch: "develop".to_owned(),
        };
        let second = edit(
            &first,
            &CardChange::Base {
                base: develop.clone(),
            },
            second_id,
            &inputs,
        )
        .unwrap();
        assert_eq!(second.seq(), first.seq());
        assert_eq!(second.card().name, first.card().name);
        let facts = second.card().workshop.clone().unwrap();
        assert_eq!(facts.base, develop);
        assert_ne!(
            facts.plan_sha256,
            first.card().workshop.clone().unwrap().plan_sha256
        );
        let release = BaseChoice::Listed {
            branch: "release".to_owned(),
        };
        assert_eq!(
            edit(
                &first,
                &CardChange::Base { base: release },
                second_id,
                &inputs
            )
            .unwrap_err(),
            IssueRefusal::BaseNotOffered
        );
    }

    #[test]
    fn a_rebuilt_image_after_issue_is_plan_changed() {
        let issued = issue_with(&gathered(), None).unwrap();
        let issue = issued.workshop_issue().unwrap();
        let plan = issued.card().workshop.clone().unwrap().plan_sha256;
        let same = launch_checks(issue, issued.order(), &gathered(), CARD, 4, plan.as_deref());
        assert!(same.plan_matches && same.base_matches == Some(true));
        let mut rebuilt = gathered();
        rebuilt.image = Some(BuiltImage {
            image_id: format!("sha256:{}", "3".repeat(64)),
            built_at_ms: None,
        });
        let checks = launch_checks(issue, issued.order(), &rebuilt, CARD, 4, plan.as_deref());
        assert_eq!(
            check_workshop_launch(&checks, || true),
            Err(LaunchRefusal::PlanChanged)
        );
        let forged = launch_checks(issue, issued.order(), &gathered(), CARD, 4, Some("00"));
        assert!(!forged.plan_matches, "a click's other hash");
        let mut moved = gathered();
        moved
            .listed_tips
            .insert("main".to_owned(), Some(DEVELOP_TIP.to_owned()));
        let checks = launch_checks(issue, issued.order(), &moved, CARD, 4, plan.as_deref());
        assert_eq!(
            check_workshop_launch(&checks, || true),
            Err(LaunchRefusal::BaseMoved)
        );
    }

    #[test]
    fn the_inventory_runs_only_after_every_other_check_passed() {
        let mut ran = 0;
        let mut full = passing();
        full.live_runs = 2;
        assert_eq!(
            check_workshop_launch(&full, || {
                ran += 1;
                false
            }),
            Err(LaunchRefusal::WorkshopFull)
        );
        assert_eq!(ran, 0, "not run while another check fails");
        assert_eq!(
            check_workshop_launch(&passing(), || {
                ran += 1;
                false
            }),
            Err(LaunchRefusal::HookProgramMissing)
        );
        assert_eq!(ran, 1);
        assert_eq!(check_workshop_launch(&passing(), || true), Ok(()));
    }

    /// Story 6.26 AC 2: a `login` floor launches like a token floor, its
    /// login volume standing for the stored credential.
    #[test]
    fn a_login_floor_is_missing_only_without_its_volume() {
        let mut checks = passing();
        checks.credential_kind = CredentialKind::Login;
        assert_eq!(readiness(&checks), vec![]);
        checks.credential_present = false;
        assert_eq!(
            readiness(&checks),
            vec![Readiness {
                refusal: LaunchRefusal::CredentialMissing,
                detail: None,
            }]
        );
        checks.credential_present = true;
        checks.credential_state = CredentialState::NeedsLogin;
        assert_eq!(
            readiness(&checks),
            vec![Readiness {
                refusal: LaunchRefusal::CredentialExpired,
                detail: None,
            }]
        );
    }
}
