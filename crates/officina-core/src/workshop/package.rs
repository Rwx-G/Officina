//! What host git found in a run's package, and what the run produced
//! (AD-110, AD-111, D-219, D-220, D-231): the host runs AD-111's steps in
//! order and hands their outputs here after each one; [`assess`] says
//! whether to run the next step or what the package is. No stream line,
//! report or manifest text enters the decision.

use serde::{Deserialize, Serialize};

use super::bundle::{BundleError, BundleHeader};
use super::run::{FinishCause, WorkshopOutcome};

/// The largest `work.bundle` collection keeps (AD-110).
pub const MAX_PACKAGE_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;

/// What the copy of `out/` may stream before it is stopped: the bundle's
/// bound plus room for the tip, the manifest and the tar headers.
pub const MAX_PACKAGE_STREAM_BYTES: u64 = MAX_PACKAGE_BUNDLE_BYTES + 1024 * 1024;

/// The most commits past the base a package may hold (AD-111 step 5).
pub const MAX_PACKAGE_COMMITS: u32 = 500;

/// The most paths a package may change before it is quarantined (D-220):
/// the file list shown is then never capped.
pub const MAX_PACKAGE_FILES: u32 = 10_000;

/// The most bytes of objects a package may inflate to (AD-111 step 7).
pub const MAX_INFLATED_BYTES: u64 = 512 * 1024 * 1024;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a package is quarantined (AD-111); story 6.14 adds the scan's.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum QuarantineCause {
        BranchMissing,
        BundleInvalid,
        TooLarge,
        WrongBase,
        ExtraRef,
        MountCommitted,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a package is for the author (AD-120 `review_package`): one to
    /// review, or one quarantined with its cause.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum PackageState {
        Review,
        Quarantined,
    }
}

/// What the run's card fixed: the base, the work branch, the mounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    pub base_commit: String,
    /// `refs/heads/officina/<name>`.
    pub branch_ref: String,
    pub mounts: Vec<String>,
}

/// Host git's outputs so far, each `None` until its step ran (AD-111).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outputs {
    /// `out/tip` as the stream held it, if at all.
    pub tip: Option<Vec<u8>>,
    /// `work.bundle`'s size, when the stream held one.
    pub bundle_bytes: Option<u64>,
    /// Without a bundle: `cat-file -e <tip>^{commit}` succeeded.
    pub tip_held: Option<bool>,
    /// The bundle header's parse (step 2).
    #[serde(skip)]
    pub header: Option<Result<BundleHeader, BundleError>>,
    /// `git bundle verify` (step 3).
    pub verified: Option<bool>,
    /// The fetch into the scratch repository with fsck (step 4).
    pub fetched: Option<bool>,
    /// `merge-base --is-ancestor <base> <tip>` (step 5).
    pub base_is_ancestor: Option<bool>,
    /// `rev-list --count <base>..<tip>` (steps 1 and 5).
    pub commits: Option<u32>,
    /// Every path the diff names, both sides of a rename (step 6).
    pub paths: Option<Vec<String>>,
    /// The objects' inflated bytes (step 7).
    pub inflated_bytes: Option<u64>,
}

/// What a package is (AD-110's table, the host's half).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackageAssessment {
    /// No commit past the base on objects the host holds.
    NoCommit { tip: String },
    /// At least one commit, or a quarantine: what the author reviews.
    Package {
        tip: Option<String>,
        commits_past_base: u32,
        cause: Option<QuarantineCause>,
    },
}

/// The next thing the host runs, or the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assessed {
    /// Without a bundle: check that the host holds the tip, and count.
    CheckTipHeld,
    ParseHeader,
    Verify,
    Fetch,
    AncestryAndCount,
    Diff,
    Inflate,
    Done(PackageAssessment),
}

/// An object id: 40 or 64 lowercase hex characters.
pub fn object_id(text: &str) -> Option<String> {
    let id = text.trim_end_matches('\n');
    (matches!(id.len(), 40 | 64)
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then(|| id.to_owned())
}

fn quarantined(tip: Option<&str>, commits: u32, cause: QuarantineCause) -> Assessed {
    Assessed::Done(PackageAssessment::Package {
        tip: tip.map(str::to_owned),
        commits_past_base: commits,
        cause: Some(cause),
    })
}

/// A path as compared with a mount (D-231): per segment, case folded,
/// trailing dots and spaces stripped, empty and `.` segments dropped.
pub fn comparable(path: &str) -> Vec<String> {
    path.split(['/', '\\'])
        .map(|segment| segment.trim_end_matches(['.', ' ']).to_lowercase())
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// True when `path` is `mount` or under it (D-231).
pub fn under_mount(path: &str, mount: &str) -> bool {
    let mount = comparable(mount);
    !mount.is_empty() && comparable(path).starts_with(&mount)
}

/// The next step or the answer from `outputs` (AD-111's order).
pub fn assess(outputs: &Outputs, expected: &Expected) -> Assessed {
    let Some(tip) = outputs
        .tip
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(object_id)
    else {
        return quarantined(None, 0, QuarantineCause::BranchMissing);
    };
    let tip_id = tip;
    let tip = Some(tip_id.as_str());
    let Some(bundle_bytes) = outputs.bundle_bytes else {
        // D-219: without a bundle only a held tip with no commit past the
        // base is a run's answer; anything else is `bundle_invalid`.
        return match (outputs.tip_held, outputs.commits) {
            (None, _) => Assessed::CheckTipHeld,
            (Some(true), None) => Assessed::AncestryAndCount,
            (Some(true), Some(0)) => Assessed::Done(PackageAssessment::NoCommit {
                tip: tip_id.clone(),
            }),
            (Some(_), _) => quarantined(
                tip,
                outputs.commits.unwrap_or(0),
                QuarantineCause::BundleInvalid,
            ),
        };
    };
    if bundle_bytes > MAX_PACKAGE_BUNDLE_BYTES {
        return quarantined(tip, 0, QuarantineCause::TooLarge);
    }
    let header = match &outputs.header {
        None => return Assessed::ParseHeader,
        Some(Err(_)) => return quarantined(tip, 0, QuarantineCause::BundleInvalid),
        Some(Ok(header)) => header,
    };
    if header.prerequisites.len() != 1 || header.prerequisites[0] != expected.base_commit {
        return quarantined(tip, 0, QuarantineCause::WrongBase);
    }
    if header.references.len() != 1 || header.references[0].name != expected.branch_ref {
        return quarantined(tip, 0, QuarantineCause::ExtraRef);
    }
    if tip != Some(header.references[0].oid.as_str()) {
        return quarantined(tip, 0, QuarantineCause::BundleInvalid);
    }
    match outputs.verified {
        None => return Assessed::Verify,
        Some(false) => return quarantined(tip, 0, QuarantineCause::BundleInvalid),
        Some(true) => {}
    }
    match outputs.fetched {
        None => return Assessed::Fetch,
        Some(false) => return quarantined(tip, 0, QuarantineCause::BundleInvalid),
        Some(true) => {}
    }
    let (Some(ancestor), Some(commits)) = (outputs.base_is_ancestor, outputs.commits) else {
        return Assessed::AncestryAndCount;
    };
    if !ancestor {
        return quarantined(tip, commits, QuarantineCause::WrongBase);
    }
    if commits == 0 {
        return Assessed::Done(PackageAssessment::NoCommit {
            tip: tip_id.clone(),
        });
    }
    if commits > MAX_PACKAGE_COMMITS {
        return quarantined(tip, commits, QuarantineCause::TooLarge);
    }
    let Some(paths) = &outputs.paths else {
        return Assessed::Diff;
    };
    if paths
        .iter()
        .any(|path| expected.mounts.iter().any(|mount| under_mount(path, mount)))
    {
        return quarantined(tip, commits, QuarantineCause::MountCommitted);
    }
    if paths.len() > MAX_PACKAGE_FILES as usize {
        return quarantined(tip, commits, QuarantineCause::TooLarge);
    }
    match outputs.inflated_bytes {
        None => Assessed::Inflate,
        Some(bytes) if bytes > MAX_INFLATED_BYTES => {
            quarantined(tip, commits, QuarantineCause::TooLarge)
        }
        Some(_) => Assessed::Done(PackageAssessment::Package {
            tip: tip.map(str::to_owned),
            commits_past_base: commits,
            cause: None,
        }),
    }
}

/// What a run produced (AD-110's table): `None` for a collection that
/// failed.
pub fn outcome(cause: FinishCause, assessment: Option<&PackageAssessment>) -> WorkshopOutcome {
    match assessment {
        None => WorkshopOutcome::Failed,
        Some(PackageAssessment::NoCommit { .. }) if cause == FinishCause::Exited => {
            WorkshopOutcome::NoChange
        }
        Some(PackageAssessment::NoCommit { .. }) => WorkshopOutcome::Failed,
        Some(PackageAssessment::Package { .. }) => WorkshopOutcome::Package,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D-231: case, trailing dots and spaces, separators, the mount itself.
    #[test]
    fn mount_paths_compare_by_segment() {
        for path in [
            ".claude",
            ".Claude/x",
            ".CLAUDE./x",
            ".claude /y",
            ".claude\\z",
        ] {
            assert!(under_mount(path, ".claude"), "{path}");
        }
        assert!(!under_mount(".claudex/a", ".claude"));
        assert!(!under_mount("src/.claude", ".claude"));
    }

    #[test]
    fn an_object_id_is_forty_or_sixty_four_hex() {
        assert!(object_id(&"a".repeat(40)).is_some());
        assert!(object_id(&format!("{}\n", "b".repeat(64))).is_some());
        assert!(object_id("--upload-pack=x").is_none());
        assert!(object_id(&"A".repeat(40)).is_none());
    }
}
