//! A package as the author reviews it (AD-121, FR-10) and the verdict on
//! it (AD-114, FR-12). Story 6.16 adds Import and Discard; the verdict's
//! values are frozen by the workshop migration (D-212).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::package::QuarantineCause;
use super::run::{CollectFailure, FinishCause, RunRefusal, RunState, WorkshopOutcome};
use super::sensitive::{MAX_GROUP_HUNK_BYTES, PackageGroup, SensitiveGroup};
use super::stream::HookCounts;
use crate::meeting::DiffView;
use crate::untrusted::Untrusted;

/// What the review view shows of how a run went (AD-121, FR-10): counts
/// and measures from the registry, the hosts and refusals as replaced
/// worker text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewFacts {
    pub duration_ms: Option<i64>,
    pub cost_usd_micros: Option<u64>,
    pub turns: Option<u64>,
    pub exit_code: Option<i64>,
    /// Hooks run and failed, per event.
    pub hooks: BTreeMap<String, HookCounts>,
    /// Allowed connections per host.
    pub allowed: Vec<(Untrusted<String>, u32)>,
    pub refused: u32,
    pub refusals: Vec<RunRefusal>,
    /// The engine's clock against the window's at launch (D-242).
    pub clock_skew_ms: Option<i64>,
    pub commits_past_base: Option<u32>,
    pub files_changed: Option<u32>,
    pub insertions: Option<u32>,
    pub deletions: Option<u32>,
    pub collect_failed: Option<CollectFailure>,
    /// How many places the credential scan found the value in (AD-112):
    /// from the facts, so the revoke advice outlives the texts (D-212).
    pub credential_locations: Option<u32>,
    /// Where it found it, while the texts are kept: never the value.
    pub credential_places: Vec<Untrusted<String>>,
}

/// One commit of a package, for display (AD-111 step 8): worker text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewCommit {
    pub hash: String,
    pub subject: Untrusted<String>,
    pub author: Untrusted<String>,
    pub at_ms: i64,
}

/// One changed file as host git reported it (`diff --raw`), in the
/// package's sorted order: its status letter and its path, both sides of a
/// rename or copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownFile {
    pub status: String,
    pub paths: Vec<Untrusted<String>>,
}

/// One group's hunks as host git computed them (AD-113): the text shown,
/// none past [`MAX_GROUP_HUNK_BYTES`], and the size and SHA-256 of the raw
/// bytes, which an approval binds to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownHunks {
    pub text: Option<Untrusted<String>>,
    pub bytes: u64,
    pub sha256: String,
}

impl ShownHunks {
    /// True when the group can be approved in the panel (AD-113).
    pub fn approvable(&self) -> bool {
        self.text.is_some() && self.bytes <= MAX_GROUP_HUNK_BYTES as u64
    }
}

/// What host git showed of a package (AD-111 step 8): its commits, the
/// diff capped as AD-84, the file list whole (D-220 bounds it) and each
/// group's hunks by key (D-239's cache).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDisplay {
    pub commits: Vec<ReviewCommit>,
    pub files: Vec<ShownFile>,
    pub diff: DiffView,
    pub hunks: BTreeMap<String, ShownHunks>,
}

/// A changed file in the review (AD-121): its index in the package's
/// sorted list, its status, its paths and the keys of its groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewFile {
    pub index: u32,
    pub status: String,
    pub paths: Vec<Untrusted<String>>,
    pub groups: Vec<String>,
}

/// A sensitive group in the review (AD-113, AD-121): its hunks inline,
/// their SHA-256, and whether it can be and was approved. An approval
/// counts only for the hash shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewGroup {
    pub key: String,
    pub group: SensitiveGroup,
    pub files: Vec<u32>,
    pub hunks: Option<Untrusted<String>>,
    pub hunks_bytes: Option<u64>,
    pub hunks_sha256: Option<String>,
    pub approvable: bool,
    pub approved: bool,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why Import is not enabled (AD-113, AD-114).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum NotReadyCause {
        /// The run is not in state `review`.
        NotInReview,
        /// A group waits for its approval.
        Unapproved,
        /// A group's hunks pass 1 MiB, or could not be shown: the package
        /// can only be discarded.
        TooLarge,
    }
}

/// One reason Import is not enabled, with the group it concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NotReady {
    pub cause: NotReadyCause,
    pub group: Option<String>,
}

/// Whether Import is enabled (AD-113): the run in state `review` and every
/// group approved; otherwise each reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImportReadiness {
    pub ready: bool,
    pub reasons: Vec<NotReady>,
}

impl ImportReadiness {
    /// The readiness of a run in `state` whose groups are `groups`.
    pub fn of(state: RunState, groups: &[ReviewGroup]) -> ImportReadiness {
        let mut reasons = Vec::new();
        if state != RunState::Review {
            reasons.push(NotReady {
                cause: NotReadyCause::NotInReview,
                group: None,
            });
        }
        for group in groups.iter().filter(|g| !g.approved) {
            reasons.push(NotReady {
                cause: if group.approvable {
                    NotReadyCause::Unapproved
                } else {
                    NotReadyCause::TooLarge
                },
                group: Some(group.key.clone()),
            });
        }
        ImportReadiness {
            ready: reasons.is_empty(),
            reasons,
        }
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why `approve_sensitive` refused (AD-113).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ApprovalRefusal {
        /// The registry holds no such run.
        UnknownRun,
        /// The run is not in state `review`.
        NotInReview,
        /// The package has no group of that key.
        UnknownGroup,
        /// The hunks computed now differ from the ones shown.
        HunksChanged,
        /// The group's hunks pass 1 MiB: the package can only be discarded.
        TooLarge,
        /// Host git could not compute the group's hunks.
        Unavailable,
    }
}

/// How `approve_sensitive(run_id, group_key, hunks_sha256)` was answered:
/// the approval stored and journaled, or refused with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum ApprovalAnswer {
    Approved,
    Refused(ApprovalRefusal),
}

/// What an approval of `group_key` for `hunks_sha256` meets, decided from
/// the group's hunks as host git computes them now (AD-113, AD-24's
/// pattern): `None` when it holds.
pub fn approval_refusal(shown: Option<&ShownHunks>, hunks_sha256: &str) -> Option<ApprovalRefusal> {
    match shown {
        None => Some(ApprovalRefusal::Unavailable),
        Some(hunks) if !hunks.approvable() => Some(ApprovalRefusal::TooLarge),
        Some(hunks) if hunks.sha256 != hunks_sha256 => Some(ApprovalRefusal::HunksChanged),
        Some(_) => None,
    }
}

/// What the registry holds of a run, as the review view reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewInputs {
    pub run_id: String,
    pub template: String,
    pub project: String,
    pub base_branch: String,
    pub base_commit: String,
    pub tip: Option<String>,
    pub state: RunState,
    pub outcome: Option<WorkshopOutcome>,
    pub cause: Option<FinishCause>,
    pub quarantine_cause: Option<QuarantineCause>,
    pub report_tail: Option<Untrusted<String>>,
    pub facts: ReviewFacts,
    /// The package's groups as collection recorded them (AD-113).
    pub groups: Vec<PackageGroup>,
    /// Each approved group's key and the SHA-256 it was approved for.
    pub approvals: Vec<(String, String)>,
}

/// A run's review (AD-121, first form; stories 6.14 to 6.16 extend it):
/// every worker text [`Untrusted`]. A `no_change` run carries its facts,
/// its report tail, the base and the tip, and `nothing_to_review`, with no
/// commit, file or diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewView {
    pub run_id: String,
    pub template: String,
    pub project: String,
    pub base_branch: String,
    pub base_commit: String,
    pub tip: Option<String>,
    pub state: RunState,
    pub outcome: Option<WorkshopOutcome>,
    pub cause: Option<FinishCause>,
    pub quarantine_cause: Option<QuarantineCause>,
    pub nothing_to_review: bool,
    pub report_tail: Option<Untrusted<String>>,
    pub facts: ReviewFacts,
    pub commits: Vec<ReviewCommit>,
    pub files: Vec<ReviewFile>,
    pub diff: Option<DiffView>,
    pub groups: Vec<ReviewGroup>,
    pub readiness: ImportReadiness,
}

impl ReviewView {
    /// The view of `inputs`, with host git's `display` for a package; a
    /// `no_change` run drops whatever display it was given.
    pub fn build(inputs: ReviewInputs, display: Option<PackageDisplay>) -> ReviewView {
        let nothing_to_review = inputs.outcome == Some(WorkshopOutcome::NoChange);
        let display = display.filter(|_| !nothing_to_review);
        let groups: Vec<ReviewGroup> = if nothing_to_review {
            Vec::new()
        } else {
            inputs
                .groups
                .iter()
                .map(|group| review_group(group, display.as_ref(), &inputs.approvals))
                .collect()
        };
        let (commits, files, diff) = match display {
            Some(shown) => (
                shown.commits,
                review_files(shown.files, &groups),
                Some(shown.diff),
            ),
            None => (Vec::new(), Vec::new(), None),
        };
        let readiness = ImportReadiness::of(inputs.state, &groups);
        ReviewView {
            run_id: inputs.run_id,
            template: inputs.template,
            project: inputs.project,
            base_branch: inputs.base_branch,
            base_commit: inputs.base_commit,
            tip: inputs.tip,
            state: inputs.state,
            outcome: inputs.outcome,
            cause: inputs.cause,
            quarantine_cause: inputs.quarantine_cause,
            nothing_to_review,
            report_tail: inputs.report_tail,
            facts: inputs.facts,
            commits,
            files,
            diff,
            groups,
            readiness,
        }
    }
}

/// `group` with the hunks `display` computed for it and its approval,
/// which counts only when its hash equals the one shown.
fn review_group(
    group: &PackageGroup,
    display: Option<&PackageDisplay>,
    approvals: &[(String, String)],
) -> ReviewGroup {
    let shown = display.and_then(|d| d.hunks.get(&group.key));
    let sha256 = shown.map(|h| h.sha256.clone());
    let approved = sha256.as_ref().is_some_and(|hash| {
        approvals
            .iter()
            .any(|(key, approved)| *key == group.key && approved == hash)
    });
    ReviewGroup {
        key: group.key.clone(),
        group: group.group,
        files: group.files.clone(),
        hunks: shown.and_then(|h| h.text.clone()),
        hunks_bytes: shown.map(|h| h.bytes),
        hunks_sha256: sha256,
        approvable: shown.is_some_and(ShownHunks::approvable),
        approved,
    }
}

/// The files of a package with the keys of the groups holding each.
fn review_files(files: Vec<ShownFile>, groups: &[ReviewGroup]) -> Vec<ReviewFile> {
    files
        .into_iter()
        .enumerate()
        .map(|(index, file)| {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            ReviewFile {
                index,
                status: file.status,
                paths: file.paths,
                groups: groups
                    .iter()
                    .filter(|g| g.files.contains(&index))
                    .map(|g| g.key.clone())
                    .collect(),
            }
        })
        .collect()
}

/// The `log` format of AD-111 step 8, read by [`parse_log`]: hash,
/// subject, author name and time, NUL-separated; `-z` ends each commit
/// with one more NUL.
pub const REVIEW_LOG_FORMAT: &str = "--format=%H%x00%s%x00%an%x00%at";

/// The commits of `git log -z` run with [`REVIEW_LOG_FORMAT`].
pub fn parse_log(out: &[u8]) -> Vec<ReviewCommit> {
    let text = String::from_utf8_lossy(out);
    let fields: Vec<&str> = text.split('\0').collect();
    fields
        .chunks(4)
        .filter_map(|commit| {
            let [hash, subject, author, at] = commit else {
                return None;
            };
            let hash = super::package::object_id(hash.trim_start_matches('\n'))?;
            Some(ReviewCommit {
                hash,
                subject: Untrusted::new((*subject).to_owned()),
                author: Untrusted::new((*author).to_owned()),
                at_ms: at.trim().parse::<i64>().ok()?.saturating_mul(1000),
            })
        })
        .collect()
}

/// Insertions and deletions of `git diff --shortstat`, 0 when absent.
pub fn parse_shortstat(out: &[u8]) -> (u32, u32) {
    let text = String::from_utf8_lossy(out);
    let count = |word: &str| {
        text.split(',')
            .find(|part| part.contains(word))
            .and_then(|part| part.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    (count("insertion"), count("deletion"))
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What became of a package. Named `PackageVerdict`, not `Verdict`, so
    /// it never meets the registry's `Verdict` in the generated TypeScript
    /// (D-1107's lesson, D-1115).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum PackageVerdict {
        Imported,
        Discarded,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a package was discarded (AD-114): the author's three reasons,
    /// and the two the system gives.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum DiscardReason {
        ShouldNotHaveRun,
        PoorResult,
        NotNeeded,
        /// Still undecided when its texts reached D-85's bound (AD-124).
        Expired,
        /// A stopped run removed without collection (AD-116).
        NotCollected,
    }
}

impl DiscardReason {
    /// True for a reason the author gives; the others are the system's
    /// and never count in Acceptance's denominator (D-222).
    pub const fn is_authors(self) -> bool {
        match self {
            DiscardReason::ShouldNotHaveRun
            | DiscardReason::PoorResult
            | DiscardReason::NotNeeded => true,
            DiscardReason::Expired | DiscardReason::NotCollected => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(outcome: WorkshopOutcome) -> ReviewInputs {
        ReviewInputs {
            run_id: "0123456789abcdef0123456789abcdef".into(),
            template: "bump".into(),
            project: "api".into(),
            base_branch: "main".into(),
            base_commit: "a".repeat(40),
            tip: Some("a".repeat(40)),
            state: RunState::NoChange,
            outcome: Some(outcome),
            cause: Some(FinishCause::Exited),
            quarantine_cause: None,
            report_tail: Some(Untrusted::new("nothing to do".into())),
            facts: ReviewFacts::default(),
            groups: Vec::new(),
            approvals: Vec::new(),
        }
    }

    fn display(mark: &str) -> PackageDisplay {
        PackageDisplay {
            commits: parse_log(
                format!("{}\0{mark}\0{mark}\01700000000\0", "b".repeat(40)).as_bytes(),
            ),
            files: vec![ShownFile {
                status: "M".into(),
                paths: vec![Untrusted::new(mark.into())],
            }],
            diff: crate::meeting::shape_diff(
                format!("diff --git a/{mark} b/{mark}\n@@ -1 +1 @@\n-a\n+b\n").as_bytes(),
                false,
            ),
            hunks: BTreeMap::from([(
                "ci".to_owned(),
                ShownHunks {
                    text: Some(Untrusted::new(format!("+{mark}\n"))),
                    bytes: 3,
                    sha256: "c".repeat(64),
                },
            )]),
        }
    }

    fn ci_group() -> PackageGroup {
        PackageGroup {
            key: "ci".into(),
            group: SensitiveGroup::Ci,
            files: vec![0],
        }
    }

    /// A package in `review` with the `ci` group and a `binary` one whose
    /// hunks passed the bound.
    fn in_review(approvals: Vec<(String, String)>) -> ReviewView {
        let mut inputs = inputs(WorkshopOutcome::Package);
        inputs.state = RunState::Review;
        inputs.groups = vec![
            ci_group(),
            PackageGroup {
                key: "binary".into(),
                group: SensitiveGroup::Binary,
                files: vec![0],
            },
        ];
        inputs.approvals = approvals;
        let mut shown = display("x");
        shown.hunks.insert(
            "binary".into(),
            ShownHunks {
                text: None,
                bytes: MAX_GROUP_HUNK_BYTES as u64 + 1,
                sha256: "e".repeat(64),
            },
        );
        ReviewView::build(inputs, Some(shown))
    }

    /// AD-113, AD-121: each group carries its hunks, their hash and
    /// whether it can be and was approved; an approval of another hash is
    /// none; each file names its groups.
    #[test]
    fn groups_carry_their_hash_and_state() {
        let view = in_review(vec![("ci".into(), "c".repeat(64))]);
        let ci = &view.groups[0];
        assert_eq!(ci.hunks_sha256.as_deref(), Some("c".repeat(64).as_str()));
        assert!(ci.approvable && ci.approved);
        assert_eq!(ci.hunks.as_ref().map(Untrusted::char_count), Some(3));
        let binary = &view.groups[1];
        assert!(!binary.approvable && !binary.approved && binary.hunks.is_none());
        assert_eq!(view.files[0].groups, ["ci", "binary"]);
        let stale = in_review(vec![("ci".into(), "f".repeat(64))]);
        assert!(!stale.groups[0].approved);
    }

    /// AD-113: Import needs state `review` and every group approved; each
    /// unapproved group is named, a group past the bound as too large.
    #[test]
    fn readiness_names_unapproved_groups() {
        let view = in_review(Vec::new());
        assert!(!view.readiness.ready);
        assert_eq!(
            view.readiness.reasons,
            [
                NotReady {
                    cause: NotReadyCause::Unapproved,
                    group: Some("ci".into())
                },
                NotReady {
                    cause: NotReadyCause::TooLarge,
                    group: Some("binary".into())
                },
            ]
        );
        let mut inputs = inputs(WorkshopOutcome::Package);
        inputs.state = RunState::Review;
        inputs.groups = vec![ci_group()];
        inputs.approvals = vec![("ci".into(), "c".repeat(64))];
        let ready = ReviewView::build(inputs.clone(), Some(display("x")));
        assert!(ready.readiness.ready && ready.readiness.reasons.is_empty());
        inputs.state = RunState::Quarantined;
        let held = ReviewView::build(inputs, Some(display("x")));
        assert_eq!(held.readiness.reasons[0].cause, NotReadyCause::NotInReview);
    }

    /// AD-121: a `no_change` view has its facts, tail, base and tip, and
    /// no commit, file or diff, whatever display it was handed.
    #[test]
    fn a_no_change_view_has_nothing_to_review() {
        let view = ReviewView::build(inputs(WorkshopOutcome::NoChange), Some(display("x")));
        assert!(view.nothing_to_review);
        assert!(view.commits.is_empty() && view.files.is_empty() && view.diff.is_none());
        assert!(view.report_tail.is_some() && view.tip.is_some());
        let package = ReviewView::build(inputs(WorkshopOutcome::Package), Some(display("x")));
        assert!(!package.nothing_to_review);
        assert_eq!(package.commits.len(), 1);
        assert_eq!(package.commits[0].at_ms, 1_700_000_000_000);
    }

    /// AD-74, AD-121: every worker text of the view is held by a field
    /// typed `Untrusted`: a marker placed in each worker input comes out
    /// of the serialised view only as many times as those fields hold it.
    #[test]
    fn every_worker_text_is_untrusted() {
        const MARK: &str = "worker-text-marker";
        let mut inputs = inputs(WorkshopOutcome::Package);
        inputs.report_tail = Some(Untrusted::new(MARK.into()));
        inputs.facts.allowed = vec![(Untrusted::new(MARK.into()), 1)];
        inputs.facts.refusals = vec![RunRefusal {
            host: Untrusted::new(MARK.into()),
            port: 443,
            cause: crate::workshop::gateway::GatewayRefusal::NotAllowed,
        }];
        inputs.groups = vec![ci_group()];
        let view = ReviewView::build(inputs, Some(display(MARK)));
        let diff = view.diff.as_ref().unwrap();
        let held: Vec<&Untrusted<String>> = [
            view.report_tail.as_ref().unwrap(),
            &view.facts.allowed[0].0,
            &view.facts.refusals[0].host,
            &view.commits[0].subject,
            &view.commits[0].author,
            &view.files[0].paths[0],
            view.groups[0].hunks.as_ref().unwrap(),
            &diff.text,
            &diff.files[0].path,
        ]
        .into_iter()
        .collect();
        let in_untrusted: usize = held
            .iter()
            .map(|text| serde_json::to_string(text).unwrap().matches(MARK).count())
            .sum();
        let json = serde_json::to_string(&view).unwrap();
        assert_eq!(json.matches(MARK).count(), in_untrusted);
    }

    #[test]
    fn shortstat_reads_insertions_and_deletions() {
        assert_eq!(
            parse_shortstat(b" 3 files changed, 10 insertions(+), 4 deletions(-)\n"),
            (10, 4)
        );
        assert_eq!(parse_shortstat(b" 1 file changed, 1 deletion(-)\n"), (0, 1));
        assert_eq!(parse_shortstat(b""), (0, 0));
    }

    #[test]
    fn the_system_reasons_are_not_the_authors() {
        let authors: Vec<_> = DiscardReason::ALL
            .iter()
            .filter(|r| r.is_authors())
            .collect();
        assert_eq!(authors.len(), DiscardReason::ALL.len() - 2);
        assert!(!DiscardReason::Expired.is_authors());
        assert!(!DiscardReason::NotCollected.is_authors());
    }
}
