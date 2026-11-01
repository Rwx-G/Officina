//! The author's verdict on a package (AD-114, FR-12). Story 6.16 adds
//! Import and Discard; the values are frozen by the workshop migration
//! (D-212).

use serde::{Deserialize, Serialize};

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
