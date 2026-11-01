//! The escape battery (AD-117). Story 6.10 adds the plan, the judgement
//! and the validity; the outcome's values are frozen by the workshop
//! migration (D-212).

use serde::{Deserialize, Serialize};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a battery ended (D-212).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BatteryOutcome {
        Passed,
        Failed,
        Stopped,
        Error,
    }
}
