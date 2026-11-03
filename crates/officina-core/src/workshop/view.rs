//! What the workshop view reads from the journal (AD-121): FR-11's group
//! notice and the per-template table (FR-12); story 6.18 builds
//! `WorkshopView` here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::package::PackageState;
use super::review::DiscardReason;
use super::run::WorkshopOutcome;
use super::sensitive::SensitiveGroup;
use crate::relay::journal::JournalEntry;

/// Verdicts a template needs before its notice can show (FR-11).
pub const NOTICE_MIN_VERDICTS: usize = 10;

/// One package of a template, as the journal tells it: the families of
/// its groups (`review_package.sensitive`) and whether the author decided
/// it (Import, or Discard with one of his reasons, D-222).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePackage {
    pub template: String,
    pub sensitive: Vec<SensitiveGroup>,
    pub decided: bool,
}

/// FR-11's notice (AD-113): `group` appeared in more than half of the
/// `verdicts` packages the author decided on `template`, so he may keep
/// the group or narrow the template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroupNotice {
    pub template: String,
    pub group: SensitiveGroup,
    pub packages: u32,
    pub verdicts: u32,
}

/// The notices of `packages` (AD-113, FR-11): a template with at least
/// [`NOTICE_MIN_VERDICTS`] decided packages gets one per family present in
/// more than half of them. `file` is left out: `review = "all"` puts it in
/// every package by the author's own choice, so it says nothing about the
/// template.
pub fn group_notices(packages: &[TemplatePackage]) -> Vec<GroupNotice> {
    let mut by_template: std::collections::BTreeMap<&str, Vec<&TemplatePackage>> =
        std::collections::BTreeMap::new();
    for package in packages.iter().filter(|p| p.decided) {
        by_template
            .entry(package.template.as_str())
            .or_default()
            .push(package);
    }
    let mut notices = Vec::new();
    for (template, decided) in by_template {
        if decided.len() < NOTICE_MIN_VERDICTS {
            continue;
        }
        let verdicts = u32::try_from(decided.len()).unwrap_or(u32::MAX);
        for group in SensitiveGroup::ALL
            .iter()
            .filter(|g| **g != SensitiveGroup::File)
        {
            let count = decided
                .iter()
                .filter(|p| p.sensitive.contains(group))
                .count();
            if count * 2 > decided.len() {
                notices.push(GroupNotice {
                    template: template.to_owned(),
                    group: *group,
                    packages: u32::try_from(count).unwrap_or(u32::MAX),
                    verdicts,
                });
            }
        }
    }
    notices
}

/// How many of a template's packages were discarded with one reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiscardCount {
    pub reason: DiscardReason,
    pub count: u32,
}

/// One template's row of the per-template table (AD-114, FR-12 as
/// amended, D-222): its runs and what became of them. Acceptance is
/// `imported` over `decided`, the author's verdicts only: a `no_change` or
/// failed run, and a package `expired` or `not_collected`, is no verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TemplateRow {
    pub template: String,
    /// Runs whose outcome is known (`workshop_finished`).
    pub runs: u32,
    pub no_change: u32,
    pub failed: u32,
    /// Packages collection quarantined (`review_package`).
    pub quarantined: u32,
    pub imported: u32,
    /// Discarded packages per reason, every reason listed.
    pub discarded: Vec<DiscardCount>,
    /// Import and the author's three reasons: Acceptance's denominator.
    pub decided: u32,
    /// FR-11's notices of this template.
    pub notices: Vec<GroupNotice>,
}

/// What the journal says of one run, joined by its id.
#[derive(Default)]
struct RunSeen {
    template: Option<String>,
    outcome: Option<WorkshopOutcome>,
    package: Option<PackageState>,
    sensitive: Vec<SensitiveGroup>,
    imported: bool,
    discarded: Option<DiscardReason>,
}

/// The per-template table of `lines` (AD-114, D-222): a run belongs to the
/// template its `workshop_finished` line names, and a verdict line of a
/// run with no such line is not counted. Templates in name order.
pub fn template_table<'a>(lines: impl IntoIterator<Item = &'a JournalEntry>) -> Vec<TemplateRow> {
    let mut runs: BTreeMap<&str, RunSeen> = BTreeMap::new();
    for line in lines {
        match line {
            JournalEntry::WorkshopFinished(l) => {
                let seen = runs.entry(&l.run_id).or_default();
                seen.template = Some(l.template.clone());
                seen.outcome = Some(l.outcome);
            }
            JournalEntry::ReviewPackage(l) => {
                let seen = runs.entry(&l.run_id).or_default();
                seen.package = Some(l.outcome);
                seen.sensitive.clone_from(&l.sensitive);
            }
            JournalEntry::ReviewImported(l) => runs.entry(&l.run_id).or_default().imported = true,
            JournalEntry::ReviewDiscarded(l) => {
                runs.entry(&l.run_id).or_default().discarded = Some(l.reason);
            }
            _ => {}
        }
    }
    let mut rows: BTreeMap<String, TemplateRow> = BTreeMap::new();
    let mut packages = Vec::new();
    let count = |n: &mut u32| *n = n.saturating_add(1);
    for seen in runs.into_values() {
        let Some(template) = seen.template else {
            continue;
        };
        let row = rows.entry(template.clone()).or_insert_with(|| TemplateRow {
            template: template.clone(),
            runs: 0,
            no_change: 0,
            failed: 0,
            quarantined: 0,
            imported: 0,
            discarded: DiscardReason::ALL
                .iter()
                .map(|reason| DiscardCount {
                    reason: *reason,
                    count: 0,
                })
                .collect(),
            decided: 0,
            notices: Vec::new(),
        });
        count(&mut row.runs);
        match seen.outcome {
            Some(WorkshopOutcome::NoChange) => count(&mut row.no_change),
            Some(WorkshopOutcome::Failed) => count(&mut row.failed),
            _ => {}
        }
        if seen.package == Some(PackageState::Quarantined) {
            count(&mut row.quarantined);
        }
        let decided = if seen.imported {
            count(&mut row.imported);
            true
        } else if let Some(reason) = seen.discarded {
            if let Some(slot) = row.discarded.iter_mut().find(|d| d.reason == reason) {
                count(&mut slot.count);
            }
            reason.is_authors()
        } else {
            false
        };
        if decided {
            count(&mut row.decided);
        }
        if seen.package.is_some() {
            packages.push(TemplatePackage {
                template,
                sensitive: seen.sensitive,
                decided,
            });
        }
    }
    for notice in group_notices(&packages) {
        if let Some(row) = rows.get_mut(&notice.template) {
            row.notices.push(notice);
        }
    }
    rows.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `decided` packages of template `bump`, the first `with_ci` of them
    /// carrying `ci`, and one undecided package carrying it too.
    fn packages(decided: usize, with_ci: usize) -> Vec<TemplatePackage> {
        let mut all: Vec<TemplatePackage> = (0..decided)
            .map(|i| TemplatePackage {
                template: "bump".into(),
                sensitive: if i < with_ci {
                    vec![SensitiveGroup::Ci, SensitiveGroup::File]
                } else {
                    vec![SensitiveGroup::File]
                },
                decided: true,
            })
            .collect();
        all.push(TemplatePackage {
            template: "bump".into(),
            sensitive: vec![SensitiveGroup::Ci],
            decided: false,
        });
        all
    }

    /// FR-11: no notice before 10 verdicts, an undecided package not
    /// counting as one.
    #[test]
    fn the_notice_needs_10_verdicts() {
        let short = NOTICE_MIN_VERDICTS - 1;
        assert!(group_notices(&packages(short, short)).is_empty());
        let notices = group_notices(&packages(NOTICE_MIN_VERDICTS, NOTICE_MIN_VERDICTS));
        assert_eq!(
            notices,
            [GroupNotice {
                template: "bump".into(),
                group: SensitiveGroup::Ci,
                packages: 10,
                verdicts: 10,
            }]
        );
    }

    /// FR-11: a group in exactly half of the decided packages gives no
    /// notice, one more does; `file` never gives one.
    #[test]
    fn the_notice_needs_more_than_half() {
        let half = NOTICE_MIN_VERDICTS / 2;
        assert!(group_notices(&packages(NOTICE_MIN_VERDICTS, half)).is_empty());
        let notices = group_notices(&packages(NOTICE_MIN_VERDICTS, half + 1));
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].group, SensitiveGroup::Ci);
        assert_eq!(notices[0].packages, u32::try_from(half + 1).unwrap());
    }

    use crate::relay::journal::{ReviewDiscarded, ReviewImported, ReviewPackage, WorkshopFinished};
    use crate::workshop::run::FinishCause;

    fn id(n: usize) -> String {
        format!("{n:032x}")
    }

    fn finished(n: usize, template: &str, outcome: WorkshopOutcome) -> JournalEntry {
        JournalEntry::WorkshopFinished(Box::new(WorkshopFinished {
            run_id: id(n),
            template: template.into(),
            project: "api".into(),
            base_branch: "main".into(),
            base_commit: "a".repeat(40),
            cause: FinishCause::Exited,
            outcome,
            commits_past_base: None,
            tip: None,
            exit_code: Some(0),
            duration_ms: 1,
            cost_usd_micros: None,
            turns: None,
            result_subtype: None,
            hooks: BTreeMap::new(),
            refused: 0,
            stream_bytes: 0,
        }))
    }

    fn package(n: usize, state: PackageState, sensitive: &[SensitiveGroup]) -> JournalEntry {
        JournalEntry::ReviewPackage(ReviewPackage {
            run_id: id(n),
            bundle_sha256: None,
            commits: 1,
            files: 1,
            insertions: 1,
            deletions: 0,
            groups: u32::try_from(sensitive.len()).unwrap(),
            outcome: state,
            cause: None,
            credential_locations: None,
            sensitive: sensitive.to_vec(),
        })
    }

    fn imported(n: usize) -> JournalEntry {
        JournalEntry::ReviewImported(ReviewImported {
            run_id: id(n),
            branch: format!("officina/bump-{n}"),
            tip: "b".repeat(40),
        })
    }

    fn discarded(n: usize, reason: DiscardReason) -> JournalEntry {
        JournalEntry::ReviewDiscarded(ReviewDiscarded {
            run_id: id(n),
            reason,
        })
    }

    fn discards(row: &TemplateRow, reason: DiscardReason) -> u32 {
        row.discarded
            .iter()
            .find(|d| d.reason == reason)
            .map_or(0, |d| d.count)
    }

    /// D-222, AD-110: a `no_change` and a failed run count as runs and in
    /// their own columns, never in `decided`.
    #[test]
    fn no_change_and_failed_are_not_decided() {
        let lines = [
            finished(1, "bump", WorkshopOutcome::NoChange),
            finished(2, "bump", WorkshopOutcome::Failed),
            finished(3, "bump", WorkshopOutcome::Package),
            package(3, PackageState::Review, &[]),
            imported(3),
        ];
        let table = template_table(&lines);
        assert_eq!(table.len(), 1);
        let row = &table[0];
        assert_eq!((row.runs, row.no_change, row.failed), (3, 1, 1));
        assert_eq!((row.imported, row.decided), (1, 1));
    }

    /// D-222: `expired` and `not_collected` sit in the discard columns and
    /// never in `decided`; a quarantined package discarded by the author
    /// is decided.
    #[test]
    fn expired_and_not_collected_are_not_decided() {
        let lines = [
            finished(1, "bump", WorkshopOutcome::Package),
            package(1, PackageState::Review, &[]),
            discarded(1, DiscardReason::Expired),
            finished(2, "bump", WorkshopOutcome::NotCollected),
            discarded(2, DiscardReason::NotCollected),
            finished(3, "bump", WorkshopOutcome::Package),
            package(3, PackageState::Quarantined, &[]),
            discarded(3, DiscardReason::ShouldNotHaveRun),
        ];
        let row = &template_table(&lines)[0];
        assert_eq!(discards(row, DiscardReason::Expired), 1);
        assert_eq!(discards(row, DiscardReason::NotCollected), 1);
        assert_eq!(discards(row, DiscardReason::ShouldNotHaveRun), 1);
        assert_eq!(row.quarantined, 1);
        assert_eq!(row.decided, 1);
        assert_eq!(row.discarded.len(), DiscardReason::ALL.len());
    }

    /// D-222, FR-11: Acceptance is imported over the author's verdicts,
    /// per template; a verdict on a run the journal never finished is not
    /// counted; the notice counts decided packages only.
    #[test]
    fn imported_over_decided_counts_author_verdicts_only() {
        let mut lines = Vec::new();
        for n in 0..NOTICE_MIN_VERDICTS {
            lines.push(finished(n, "bump", WorkshopOutcome::Package));
            lines.push(package(n, PackageState::Review, &[SensitiveGroup::Ci]));
            lines.push(if n % 2 == 0 {
                imported(n)
            } else {
                discarded(n, DiscardReason::PoorResult)
            });
        }
        let other = NOTICE_MIN_VERDICTS + 1;
        lines.push(finished(other, "lint", WorkshopOutcome::Package));
        lines.push(package(other, PackageState::Review, &[]));
        lines.push(discarded(other, DiscardReason::Expired));
        lines.push(imported(other + 1));
        let table = template_table(&lines);
        let names: Vec<&str> = table.iter().map(|r| r.template.as_str()).collect();
        assert_eq!(names, ["bump", "lint"]);
        let bump = &table[0];
        let half = u32::try_from(NOTICE_MIN_VERDICTS / 2).unwrap();
        assert_eq!((bump.imported, bump.decided), (half, half * 2));
        assert_eq!(discards(bump, DiscardReason::PoorResult), half);
        assert_eq!(bump.notices.len(), 1);
        assert_eq!(bump.notices[0].group, SensitiveGroup::Ci);
        let lint = &table[1];
        assert_eq!((lint.runs, lint.imported, lint.decided), (1, 0, 0));
        assert!(lint.notices.is_empty());
    }
}
