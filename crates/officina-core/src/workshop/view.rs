//! What the workshop view reads from the journal (AD-121). Story 6.15
//! lands FR-11's group notice; story 6.18 builds `WorkshopView` here.

use serde::{Deserialize, Serialize};

use super::sensitive::SensitiveGroup;

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
}
