//! A workshop launch (AD-102, AD-107, D-243): the approved workshop
//! template, the base the author chose, the facts read at launch, and the
//! one validated value the worker's plan and vector are built from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::floor::{
    CanonicalProjectWorkshop, CanonicalTemplateWorkshop, CanonicalWorkshop, CredentialKind,
};
use super::gateway::{CLAUDE_CODE_HOSTS, GatewayConfig};
use super::names::{ObjectName, RunNames};
use super::vector::{self, ContainerVector, CredentialSource, GitIdentity, Limits, WorkerShape};
use crate::orders::work_order::WorkOrder;
use crate::relay::catalogue::{ApprovedCatalogue, RelayFloor};
use crate::relay::template::{ApprovedTemplate, ProjectName, TemplateName, TemplateUnavailable};

/// The tools a worker never runs (FR-7): `WebSearch`, whose pages do not
/// say whether its requests leave through the worker's proxy. The vector
/// reads this one copy, and a `system/init` listing one of them stops the
/// run `unexpected_tool` (story 6.12).
pub const WORKER_DISALLOWED_TOOLS: &[&str] = &["WebSearch"];

/// The hosts a `login` worker adds to the allow-list (D-606): token
/// refresh for claude.ai accounts goes to `platform.claude.com`
/// (`network-config`). Story 6.26 adds what the login entry records.
pub const LOGIN_HOSTS: &[&str] = &["platform.claude.com"];

/// The files a `login` run copies from the floor's login into its own
/// config dir (D-606). Story 6.26 adds `.claude.json` if the login entry
/// requires it.
pub const LOGIN_FILES: &[&str] = &[".credentials.json"];

/// The version of [`RunPlan`]'s JSON; the entry point refuses another.
pub const RUN_PLAN_SCHEMA: u32 = 1;

/// Where the plan, the base bundle and the prompt are in the worker's
/// volume (AD-107).
pub const IN_DIR: &str = "/work/.officina/in";

/// The allow-list of a worker of credential `kind` (D-606, D-611):
/// [`CLAUDE_CODE_HOSTS`], then for the login kind [`LOGIN_HOSTS`], then
/// the project's hosts, each once, in that order.
pub fn allow_list(kind: CredentialKind, hosts: &[String]) -> Vec<String> {
    let mut allow: Vec<String> = Vec::new();
    let mut add = |host: &str| {
        if !allow.iter().any(|h| h == host) {
            allow.push(host.to_owned());
        }
    };
    CLAUDE_CODE_HOSTS.iter().for_each(|h| add(h));
    if kind == CredentialKind::Login {
        LOGIN_HOSTS.iter().for_each(|h| add(h));
    }
    hosts.iter().for_each(|h| add(h));
    allow
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What is known of a floor's credential (D-607). This story reads it
    /// through the host's `CredentialStateSource`, which answers
    /// `unchecked`; story 6.26 folds the journal into it. `missing` is
    /// computed, never journaled.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum CredentialState {
        #[default]
        Unchecked,
        Ok,
        Missing,
        /// The login kind's login expired or was revoked.
        NeedsLogin,
        /// A token kind's credential expired or was revoked.
        NeedsNewCredential,
    }
}

impl CredentialState {
    /// True for the states that refuse a launch `credential_expired`
    /// (D-607).
    pub const fn needs_new(self) -> bool {
        matches!(
            self,
            CredentialState::NeedsLogin | CredentialState::NeedsNewCredential
        )
    }
}

/// Why a workshop template cannot be launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkshopTemplateUnavailable {
    /// v0.5's refusals: the catalogue changed, or the template is not
    /// approved.
    Unavailable(TemplateUnavailable),
    /// The template has no `[template.workshop]`: it runs on the host.
    NotWorkshop,
    /// The approved catalogue holds no valid `[workshop]` table.
    WorkshopOff,
}

/// A workshop template as the author approved it (AD-107, 5.5's pattern):
/// present and equal in the approved and the current catalogue, with the
/// floor's `[workshop]` and each project's `[project.workshop]` from the
/// approved snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedWorkshopTemplate {
    template: ApprovedTemplate,
    workshop: CanonicalTemplateWorkshop,
    floor: CanonicalWorkshop,
    projects: BTreeMap<String, CanonicalProjectWorkshop>,
}

impl ApprovedWorkshopTemplate {
    pub fn new(
        approved: &ApprovedCatalogue,
        current: &RelayFloor,
        name: &TemplateName,
    ) -> Result<ApprovedWorkshopTemplate, WorkshopTemplateUnavailable> {
        let template = ApprovedTemplate::new_for_workshop(approved, current, name)
            .map_err(WorkshopTemplateUnavailable::Unavailable)?;
        let canonical = approved.canonical.templates.get(name.as_str()).ok_or(
            WorkshopTemplateUnavailable::Unavailable(TemplateUnavailable::NotApproved),
        )?;
        let workshop = canonical
            .workshop
            .clone()
            .ok_or(WorkshopTemplateUnavailable::NotWorkshop)?;
        let floor = approved
            .canonical
            .workshop
            .clone()
            .ok_or(WorkshopTemplateUnavailable::WorkshopOff)?;
        let projects = canonical
            .projects
            .iter()
            .filter_map(|p| {
                approved
                    .canonical
                    .projects
                    .get(p)
                    .and_then(|project| project.workshop.clone())
                    .map(|w| (p.clone(), w))
            })
            .collect();
        Ok(ApprovedWorkshopTemplate {
            template,
            workshop,
            floor,
            projects,
        })
    }

    /// The v0.5 template the work order is validated against (AD-77).
    pub fn template(&self) -> &ApprovedTemplate {
        &self.template
    }

    pub fn workshop(&self) -> &CanonicalTemplateWorkshop {
        &self.workshop
    }

    pub fn floor(&self) -> &CanonicalWorkshop {
        &self.floor
    }

    /// The `[project.workshop]` of `project`, when it declares one.
    pub fn project(&self, project: &ProjectName) -> Option<&CanonicalProjectWorkshop> {
        self.projects.get(project.as_str())
    }
}

/// The base a run starts from (AD-107): a branch of the template's list,
/// or an imported `officina/` branch with the run it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BaseChoice {
    Listed {
        branch: String,
    },
    Imported {
        branch: String,
        parent_run_id: String,
    },
}

impl BaseChoice {
    pub fn branch(&self) -> &str {
        match self {
            BaseChoice::Listed { branch } | BaseChoice::Imported { branch, .. } => branch,
        }
    }

    pub fn parent_run_id(&self) -> Option<&str> {
        match self {
            BaseChoice::Listed { .. } => None,
            BaseChoice::Imported { parent_run_id, .. } => Some(parent_run_id),
        }
    }
}

/// What the host read for a launch (AD-107): the base's tip, the image ids,
/// the floor key, each mount's resolved source, the commit identity and
/// the gateway's routes for this network (D-611).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchFacts {
    pub base_commit: String,
    pub image: String,
    pub image_id: String,
    pub gateway_image_id: String,
    pub floor_key: String,
    pub officina_version: String,
    pub mount_sources: Vec<String>,
    pub git: GitIdentity,
    pub routes: BTreeMap<String, String>,
    /// The run's sequence number, reserved at issue (D-215).
    pub seq: u32,
    /// The run's id, the card's (D-215).
    pub run_id: String,
}

/// Why a launch cannot be built from what was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchInvalid {
    /// The order is not of this template.
    OtherTemplate,
    /// The order's project declares no `[project.workshop]`.
    ProjectWithoutWorkshop,
    /// The base is not in the template's list (a listed choice).
    BaseNotListed,
    /// The mounts' sources do not match the project's mounts.
    MountsMismatch,
    /// A run id or name that is not an object name.
    BadRunId,
}

/// A launch built from a validated work order and an approved workshop
/// template (AD-107): the only input the worker's plan and vector are
/// built from. Its fields are private; [`WorkshopLaunch::new`] is the only
/// constructor.
///
/// ```compile_fail,E0451
/// use officina_core::workshop::launch::WorkshopLaunch;
/// let launch = WorkshopLaunch { order: todo!(), template: todo!(), base: todo!(), facts: todo!() };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkshopLaunch {
    order: WorkOrder,
    template: ApprovedWorkshopTemplate,
    base: BaseChoice,
    facts: LaunchFacts,
    project: CanonicalProjectWorkshop,
    names: RunNames,
}

impl WorkshopLaunch {
    pub fn new(
        order: &WorkOrder,
        template: &ApprovedWorkshopTemplate,
        base: BaseChoice,
        facts: &LaunchFacts,
    ) -> Result<WorkshopLaunch, LaunchInvalid> {
        if order.template() != template.template().name() {
            return Err(LaunchInvalid::OtherTemplate);
        }
        let project = template
            .projects
            .get(order.project())
            .cloned()
            .ok_or(LaunchInvalid::ProjectWithoutWorkshop)?;
        if let BaseChoice::Listed { branch } = &base
            && !template.workshop.base.contains(branch)
        {
            return Err(LaunchInvalid::BaseNotListed);
        }
        if facts.mount_sources.len() != project.mounts.len() {
            return Err(LaunchInvalid::MountsMismatch);
        }
        let run =
            crate::workshop::names::RunId::parse(&facts.run_id).ok_or(LaunchInvalid::BadRunId)?;
        Ok(WorkshopLaunch {
            order: order.clone(),
            template: template.clone(),
            base,
            facts: facts.clone(),
            project,
            names: RunNames::of(&run),
        })
    }

    pub fn order(&self) -> &WorkOrder {
        &self.order
    }

    pub fn base(&self) -> &BaseChoice {
        &self.base
    }

    pub fn facts(&self) -> &LaunchFacts {
        &self.facts
    }

    pub fn names(&self) -> &RunNames {
        &self.names
    }

    pub fn credential_kind(&self) -> CredentialKind {
        self.template.floor.auth
    }

    /// The run's work branch, `officina/<template>-<seq>` (D-215).
    pub fn branch(&self) -> String {
        format!(
            "officina/{}",
            crate::orders::work_order::order_name(self.order.template(), self.facts.seq)
        )
    }

    /// The deadline the gateway and the entry point count, in seconds.
    pub fn deadline_s(&self) -> u64 {
        u64::from(self.template.workshop.max_minutes) * 60
    }

    /// The allow-list of this run's gateway and card (D-606).
    pub fn allow_list(&self) -> Vec<String> {
        allow_list(self.credential_kind(), &self.project.hosts)
    }

    /// The labels of the run's objects of `role` (D-215): its networks and
    /// volume take the role of the container they serve.
    pub fn labels(&self, role: vector::ContainerRole) -> BTreeMap<String, String> {
        let run = crate::workshop::names::RunId::parse(&self.facts.run_id);
        match run {
            Some(run) => crate::workshop::names::run_labels(
                &self.facts.floor_key,
                &self.facts.officina_version,
                &run,
                role,
                &crate::orders::work_order::order_name(self.order.template(), self.facts.seq),
            ),
            None => BTreeMap::new(),
        }
    }
}

/// One mount's place in the clone (AD-108 step 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountLink {
    pub index: usize,
    pub path: String,
}

/// The memory copy of Slice B (AD-115), set by story 6.21.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryCopy {
    pub name: String,
}

/// What the entry point runs (AD-102, AD-108): written by the window to
/// `/work/.officina/in/plan.json`, parsed by `officina-entry run` with this
/// same type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlan {
    pub schema: u32,
    pub project: String,
    pub clone_path: String,
    pub branch: String,
    pub base_branch: String,
    pub base_commit: String,
    pub mount_links: Vec<MountLink>,
    pub exclude: Vec<String>,
    pub claude_argv: Vec<String>,
    pub deadline_s: u64,
    pub prompt_file: String,
    #[serde(default)]
    pub login_files: Vec<String>,
    #[serde(default)]
    pub memory: Option<MemoryCopy>,
}

/// The worker's `claude` vector (AD-102, FR-7): fixed, then the budget and
/// the model when the template sets them.
pub fn claude_argv(max_usd_cents: Option<u32>, model: Option<&str>) -> Vec<String> {
    let mut argv: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-hook-events",
        "--permission-mode",
        "bypassPermissions",
        "--permission-prompts",
        "none",
        "--disallowedTools",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    argv.push(WORKER_DISALLOWED_TOOLS.join(","));
    if let Some(cents) = max_usd_cents {
        argv.push("--max-budget-usd".to_owned());
        argv.push(format!("{}.{:02}", cents / 100, cents % 100));
    }
    if let Some(model) = model {
        argv.push("--model".to_owned());
        argv.push(model.to_owned());
    }
    argv
}

/// The worker's plan, built from a validated launch only (AD-102).
///
/// ```compile_fail
/// let plan = officina_core::workshop::launch::run_plan(&());
/// ```
pub fn run_plan(launch: &WorkshopLaunch) -> RunPlan {
    let project = launch.order.project().to_owned();
    let mounts = &launch.project.mounts;
    RunPlan {
        schema: RUN_PLAN_SCHEMA,
        clone_path: format!("{}/{project}", vector::WORK_DIR),
        project,
        branch: launch.branch(),
        base_branch: launch.base.branch().to_owned(),
        base_commit: launch.facts.base_commit.clone(),
        mount_links: mounts
            .iter()
            .enumerate()
            .map(|(index, path)| MountLink {
                index,
                path: path.clone(),
            })
            .collect(),
        exclude: mounts.clone(),
        claude_argv: claude_argv(
            launch.template.workshop.max_usd_cents,
            launch.order.model().map(|m| m.code()),
        ),
        deadline_s: launch.deadline_s(),
        prompt_file: format!("{IN_DIR}/prompt.txt"),
        login_files: if launch.credential_kind() == CredentialKind::Login {
            LOGIN_FILES.iter().map(|f| (*f).to_owned()).collect()
        } else {
            Vec::new()
        },
        memory: None,
    }
}

/// The run's gateway configuration (AD-103, D-611).
pub fn gateway_config(launch: &WorkshopLaunch) -> GatewayConfig {
    let allow = launch.allow_list();
    let mut config = GatewayConfig::new(
        &launch.facts.run_id,
        &[],
        launch.deadline_s(),
        BTreeMap::new(),
    );
    config.allow = allow;
    config.routes = launch
        .facts
        .routes
        .iter()
        .filter(|(entry, _)| config.allow.contains(entry))
        .map(|(e, u)| (e.clone(), u.clone()))
        .collect();
    config
}

/// The run's worker vector, built from a validated launch only (AD-102).
pub fn worker(launch: &WorkshopLaunch) -> ContainerVector {
    let floor = &launch.template.floor;
    let credential = CredentialSource::of(floor.auth, &launch.facts.floor_key);
    let labels = launch.labels(vector::ContainerRole::Worker);
    let names = &launch.names;
    vector::worker(&WorkerShape {
        name: &names.worker,
        labels: &labels,
        image_id: &launch.facts.image_id,
        network: &names.internal,
        volume: &names.volume,
        gateway: &names.gateway,
        limits: Limits::worker(floor.cpus, floor.memory_mib),
        mounts: &launch.facts.mount_sources,
        credential: &credential,
        git: &launch.facts.git,
        memory: None,
    })
}

/// The run's gateway vector (AD-103, AD-104).
pub fn gateway(launch: &WorkshopLaunch) -> ContainerVector {
    let labels = launch.labels(vector::ContainerRole::Gateway);
    vector::gateway(
        &launch.names.gateway,
        &labels,
        &launch.facts.gateway_image_id,
        &launch.names.egress,
        &gateway_config(launch),
    )
}

/// `plan_sha256` (AD-102, D-207, D-242): the SHA-256 of the canonical JSON
/// of the worker's and the gateway's vectors and the plan. The credential's
/// value is in none of them; the image ids, the relative deadline and the
/// routes are.
pub fn plan_sha256(launch: &WorkshopLaunch) -> String {
    let canonical = serde_json::json!({
        "worker": worker(launch),
        "gateway": gateway(launch),
        "plan": run_plan(launch),
    });
    crate::sha256_hex(canonical.to_string().as_bytes())
}

/// The run's object names, for the host's calls.
pub fn object_names(launch: &WorkshopLaunch) -> &RunNames {
    &launch.names
}

/// The worker's internal network and the gateway's egress network, by the
/// run's names, in the order the launch creates them (AD-104).
pub fn networks(launch: &WorkshopLaunch) -> [&ObjectName; 2] {
    [&launch.names.egress, &launch.names.internal]
}

/// The labels of the owner's login container (D-605): Officina never
/// creates it, and reconciliation never counts it as an orphan.
pub fn login_labels(floor_key: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            crate::workshop::names::MANAGED_LABEL.0.to_owned(),
            crate::workshop::names::MANAGED_LABEL.1.to_owned(),
        ),
        (
            crate::workshop::names::FLOOR_LABEL.to_owned(),
            floor_key.to_owned(),
        ),
        (
            crate::workshop::names::ROLE_LABEL.to_owned(),
            crate::workshop::names::LOGIN_ROLE.to_owned(),
        ),
    ])
}

/// The host whose route the login command follows (D-605): token
/// exchange for claude.ai accounts.
pub const LOGIN_ROUTE_HOST: &str = "platform.claude.com";

/// The owner's login command (D-605): `docker run --rm -it` of the base
/// image as uid 1000 with the fixed options, the floor's login volume at
/// the login folder with `CLAUDE_CONFIG_DIR` there, `HOME` on a tmpfs, no
/// credential name, the proxy variables when a route exists for
/// [`LOGIN_ROUTE_HOST`], and `claude auth login`. The engine's default
/// network, as a build uses: it is attended.
pub fn login_command(
    floor_key: &str,
    base_image: &str,
    routes: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut argv: Vec<String> = ["docker", "run", "--rm", "-it"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let limits = Limits {
        cpu_tenths: 10,
        memory_mib: 1024,
        pids: 256,
    };
    argv.extend(vector::fixed_options(
        vector::WORKER_USER,
        vector::CLAUDE_PROGRAM,
        limits,
        &login_labels(floor_key),
    ));
    let push = |argv: &mut Vec<String>, flag: &str, value: String| {
        argv.push(flag.to_owned());
        argv.push(value);
    };
    push(
        &mut argv,
        "--tmpfs",
        "/tmp:rw,nosuid,nodev,size=64m".to_owned(),
    );
    push(
        &mut argv,
        "--tmpfs",
        "/home/login:rw,nosuid,nodev,size=16m".to_owned(),
    );
    push(
        &mut argv,
        "--mount",
        format!(
            "type=volume,source={},target={}",
            crate::workshop::names::login_volume(floor_key).as_str(),
            vector::LOGIN_DIR
        ),
    );
    let mut env = vec![
        ("CLAUDE_CONFIG_DIR".to_owned(), vector::LOGIN_DIR.to_owned()),
        ("HOME".to_owned(), "/home/login".to_owned()),
        ("DISABLE_AUTOUPDATER".to_owned(), "1".to_owned()),
        (
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned(),
            "1".to_owned(),
        ),
    ];
    if let Some(proxy) = routes.get(LOGIN_ROUTE_HOST) {
        let url = format!("http://{proxy}");
        for name in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            env.push((name.to_owned(), url.clone()));
        }
        env.push(("NO_PROXY".to_owned(), vector::NO_PROXY.to_owned()));
    }
    for (name, value) in env {
        push(&mut argv, "--env", format!("{name}={value}"));
    }
    argv.push(base_image.to_owned());
    argv.extend(["auth".to_owned(), "login".to_owned()]);
    argv
}

/// `argv` on one line for PowerShell and `cmd` (D-605): each argument in
/// double quotes when it holds a space or a character either shell reads.
pub fn render_one_line(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if arg.is_empty()
                || arg
                    .chars()
                    .any(|c| c.is_whitespace() || "&|<>^%;,()'`$@{}".contains(c))
            {
                format!("\"{arg}\"")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::relay::catalogue::load;
    use crate::relay::template::ParamName;

    pub(crate) const FLOOR_PATH: &str = "C:/path/to/officina.toml";

    pub(crate) fn floor(extra_template: &str) -> String {
        format!(
            "schema = 2\n\n[workshop]\n\n[[project]]\nname = \"api\"\npath = \"Api\"\n\n[project.workshop]\nmounts = [\".claude\"]\nhosts = [\"index.crates.io\"]\n\n[[template]]\nname = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"api\"]\nprompt = \"Bump {{package}}.\"\n{extra_template}\n[template.params.package]\nvalues = [\"serde_json\"]\n\n[template.workshop]\nbase = [\"main\", \"develop\"]\nmax_minutes = 30\nmax_usd = 2.5\n"
        )
    }

    pub(crate) fn approved_template(text: &str) -> ApprovedWorkshopTemplate {
        let current = load(text, FLOOR_PATH).unwrap();
        let approved = ApprovedCatalogue::approving(&current, FLOOR_PATH, 0);
        ApprovedWorkshopTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap())
            .unwrap()
    }

    pub(crate) fn facts() -> LaunchFacts {
        LaunchFacts {
            base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
            image: "officina-workshop:local".into(),
            image_id: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                .into(),
            gateway_image_id:
                "sha256:2222222222222222222222222222222222222222222222222222222222222222".into(),
            floor_key: "0123456789abcdef".into(),
            officina_version: "0.1.0".into(),
            mount_sources: vec!["C:/path/to/Api/.claude".into()],
            git: GitIdentity {
                name: "User One".into(),
                email: "user01@example.com".into(),
            },
            routes: BTreeMap::new(),
            seq: 1,
            run_id: "0123456789abcdef0123456789abcdef".into(),
        }
    }

    pub(crate) fn launch_with(
        template: &ApprovedWorkshopTemplate,
        facts: &LaunchFacts,
    ) -> WorkshopLaunch {
        let order = WorkOrder::validate(
            template.template(),
            &ProjectName::parse("api").unwrap(),
            &[(ParamName::parse("package").unwrap(), "serde_json")],
        )
        .unwrap();
        WorkshopLaunch::new(
            &order,
            template,
            BaseChoice::Listed {
                branch: "main".into(),
            },
            facts,
        )
        .unwrap()
    }

    pub(crate) fn sample_launch() -> WorkshopLaunch {
        launch_with(&approved_template(&floor("")), &facts())
    }

    #[test]
    fn an_unapproved_template_is_refused() {
        let current = load(&floor(""), FLOOR_PATH).unwrap();
        let approved = ApprovedCatalogue::approving(&current, FLOOR_PATH, 0);
        assert_eq!(
            ApprovedWorkshopTemplate::new(
                &approved,
                &current,
                &TemplateName::parse("other").unwrap()
            ),
            Err(WorkshopTemplateUnavailable::Unavailable(
                TemplateUnavailable::NotApproved
            ))
        );
        let host = "schema = 2\n[[project]]\nname = \"api\"\npath = \"Api\"\n[[template]]\nname = \"bump\"\ndescription = \"d\"\nprojects = [\"api\"]\nprompt = \"Bump.\"\n";
        let current = load(host, FLOOR_PATH).unwrap();
        let approved = ApprovedCatalogue::approving(&current, FLOOR_PATH, 0);
        assert_eq!(
            ApprovedWorkshopTemplate::new(
                &approved,
                &current,
                &TemplateName::parse("bump").unwrap()
            ),
            Err(WorkshopTemplateUnavailable::NotWorkshop)
        );
    }

    #[test]
    fn a_template_changed_since_approval_is_refused() {
        let first = load(&floor(""), FLOOR_PATH).unwrap();
        let approved = ApprovedCatalogue::approving(&first, FLOOR_PATH, 0);
        let edited = load(&floor("model = \"sonnet\"\n"), FLOOR_PATH).unwrap();
        assert_eq!(
            ApprovedWorkshopTemplate::new(
                &approved,
                &edited,
                &TemplateName::parse("bump").unwrap()
            ),
            Err(WorkshopTemplateUnavailable::Unavailable(
                TemplateUnavailable::CatalogueChanged
            ))
        );
    }

    #[test]
    fn a_base_outside_the_list_is_refused() {
        let template = approved_template(&floor(""));
        let order = WorkOrder::validate(
            template.template(),
            &ProjectName::parse("api").unwrap(),
            &[(ParamName::parse("package").unwrap(), "serde_json")],
        )
        .unwrap();
        assert_eq!(
            WorkshopLaunch::new(
                &order,
                &template,
                BaseChoice::Listed {
                    branch: "release".into()
                },
                &facts()
            ),
            Err(LaunchInvalid::BaseNotListed)
        );
    }

    #[test]
    fn the_plan_round_trips() {
        let plan = run_plan(&sample_launch());
        let text = serde_json::to_string(&plan).unwrap();
        assert_eq!(serde_json::from_str::<RunPlan>(&text).unwrap(), plan);
        assert_eq!(plan.branch, "officina/bump-1");
        assert_eq!(plan.clone_path, "/work/api");
        assert_eq!(plan.deadline_s, 30 * 60);
        assert!(plan.login_files.is_empty());
    }

    #[test]
    fn the_argument_vector_is_fixed() {
        let argv = claude_argv(None, None);
        assert_eq!(
            argv,
            [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-hook-events",
                "--permission-mode",
                "bypassPermissions",
                "--permission-prompts",
                "none",
                "--disallowedTools",
                &WORKER_DISALLOWED_TOOLS.join(","),
            ]
        );
    }

    #[test]
    fn max_usd_and_model_follow_the_template() {
        let plan = run_plan(&sample_launch());
        let at = plan
            .claude_argv
            .iter()
            .position(|a| a == "--max-budget-usd")
            .unwrap();
        assert_eq!(plan.claude_argv[at + 1], "2.50");
        assert!(!plan.claude_argv.iter().any(|a| a == "--model"));
        let with_model = launch_with(&approved_template(&floor("model = \"sonnet\"\n")), &facts());
        let argv = run_plan(&with_model).claude_argv;
        let at = argv.iter().position(|a| a == "--model").unwrap();
        assert_eq!(argv[at + 1], "sonnet");
    }

    #[test]
    fn a_changed_deadline_changes_the_hash() {
        let base = plan_sha256(&sample_launch());
        let longer = floor("").replace("max_minutes = 30", "max_minutes = 31");
        assert_ne!(
            plan_sha256(&launch_with(&approved_template(&longer), &facts())),
            base
        );
    }

    #[test]
    fn a_changed_image_id_changes_the_hash() {
        let base = plan_sha256(&sample_launch());
        let mut facts = facts();
        facts.image_id = facts.image_id.replace('1', "3");
        assert_ne!(
            plan_sha256(&launch_with(&approved_template(&floor("")), &facts)),
            base
        );
    }

    #[test]
    fn a_changed_route_changes_the_hash() {
        let base = plan_sha256(&sample_launch());
        let mut facts = facts();
        facts
            .routes
            .insert("api.anthropic.com".into(), "proxy.example.org:3128".into());
        assert_ne!(
            plan_sha256(&launch_with(&approved_template(&floor("")), &facts)),
            base
        );
    }

    #[test]
    fn login_adds_every_login_host_once() {
        let hosts = vec!["index.crates.io".to_owned(), LOGIN_HOSTS[0].to_owned()];
        let allow = allow_list(CredentialKind::Login, &hosts);
        for host in LOGIN_HOSTS.iter().chain(CLAUDE_CODE_HOSTS) {
            assert_eq!(allow.iter().filter(|h| h == host).count(), 1, "{host}");
        }
        assert_eq!(&allow[..CLAUDE_CODE_HOSTS.len()], CLAUDE_CODE_HOSTS);
    }

    #[test]
    fn token_kinds_add_no_host() {
        for kind in [CredentialKind::OauthToken, CredentialKind::ApiKey] {
            let allow = allow_list(kind, &["index.crates.io".to_owned()]);
            assert!(LOGIN_HOSTS.iter().all(|h| !allow.iter().any(|a| a == h)));
            assert_eq!(allow.len(), CLAUDE_CODE_HOSTS.len() + 1);
        }
    }

    #[test]
    fn the_login_command_carries_no_credential_name() {
        let mut routes = BTreeMap::new();
        for with_route in [false, true] {
            if with_route {
                routes.insert(
                    LOGIN_ROUTE_HOST.to_owned(),
                    "proxy.example.org:3128".to_owned(),
                );
            }
            let argv = login_command("0123456789abcdef", "officina-workshop:local", &routes);
            for kind in CredentialKind::ALL {
                if let Some(name) = vector::credential_name(*kind) {
                    assert!(!argv.iter().any(|a| a.starts_with(name)), "{name}");
                }
            }
            assert_eq!(
                argv.iter().any(|a| a.starts_with("HTTPS_PROXY=")),
                with_route
            );
            assert!(argv.ends_with(&[
                "officina-workshop:local".to_owned(),
                "auth".into(),
                "login".into()
            ]));
            assert!(argv.iter().any(|a| a == "officina.role=login"));
        }
    }

    #[test]
    fn the_login_command_renders_on_one_line() {
        let line = render_one_line(&login_command(
            "0123456789abcdef",
            "officina-workshop:local",
            &BTreeMap::new(),
        ));
        assert!(!line.contains('\n'));
        assert!(line.starts_with("docker run --rm -it "));
        assert!(line.ends_with(" auth login"));
        assert_eq!(
            render_one_line(&["a b".to_owned(), "c".to_owned()]),
            "\"a b\" c"
        );
    }
}
