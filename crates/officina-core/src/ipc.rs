//! The error every window command returns over IPC (AD-20): a code the
//! window can act on and a message it shows, one shape for every command,
//! generated to TypeScript with the board's types.

use serde::{Deserialize, Serialize};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What kind of failure a window command met.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum IpcErrorCode {
        /// The catalogue changed since the window displayed it: read the
        /// new one before approving (AD-24).
        CatalogueChanged,
        /// The relay floor cannot be read or fails phase 1.
        FloorUnreadable,
        /// A file the decision writes could not be written.
        WriteFailed,
        /// The journal could not be locked or written: retry.
        JournalUnavailable,
        /// The request cannot be decided: unknown, decided, expired or
        /// invalid (D-27).
        Refused,
        /// The window's own machinery failed, such as a task that did not
        /// finish.
        Internal,
    }
}

/// A window command's failure: what kind, and the sentence the window
/// shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
}

impl IpcError {
    /// A failure of the window's own machinery.
    ///
    /// ```
    /// use officina_core::ipc::{IpcError, IpcErrorCode};
    /// assert_eq!(IpcError::internal("a task panicked").code, IpcErrorCode::Internal);
    /// ```
    pub fn internal(message: impl Into<String>) -> IpcError {
        IpcError {
            code: IpcErrorCode::Internal,
            message: message.into(),
        }
    }
}
