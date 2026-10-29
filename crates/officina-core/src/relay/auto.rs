//! The `auto` class (AD-86 to AD-88): what a review of an `auto` request
//! decides. Story 5.6 creates this module with the decision alone, since
//! migration 2's `auto_review.decision` column lists its values (D-133);
//! the review call, the rule baseline and the counting join it with their
//! stories (5.13, 5.14).

use serde::{Deserialize, Serialize};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a review, or the rule, decided about an `auto` request (AD-86).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ReviewDecision {
        Approve,
        Reject,
        /// Left to the author.
        Escalate,
    }
}
