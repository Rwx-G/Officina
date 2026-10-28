//! The error every window command returns over IPC (AD-20): a code the
//! window can act on and a message it shows, one shape for every command,
//! generated to TypeScript with the board's types.

use serde::{Deserialize, Serialize};

use crate::registry::types::AuthorRefusal;

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
        /// A rule refused an author action on subjects and nothing was
        /// written: `refusal` says which (D-57).
        AuthorRefused,
        /// The registry cannot be written now: read only, unavailable or on
        /// a network folder (AD-41, AD-42).
        RegistryUnavailable,
        /// The registry could not be read now, a busy file say, without
        /// being damaged: it stays usable, and a retry may succeed.
        ReadFailed,
        /// The collector did not answer in time; a request it had not
        /// started is dropped, never run later (D-92).
        Timeout,
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
    /// Which rule refused an author action, for
    /// [`IpcErrorCode::AuthorRefused`]: the window words it.
    pub refusal: Option<AuthorRefusal>,
}

impl IpcError {
    /// A failure of kind `code`, saying `message`.
    pub fn new(code: IpcErrorCode, message: impl Into<String>) -> IpcError {
        IpcError {
            code,
            message: message.into(),
            refusal: None,
        }
    }

    /// A failure of the window's own machinery.
    ///
    /// ```
    /// use officina_core::ipc::{IpcError, IpcErrorCode};
    /// assert_eq!(IpcError::internal("a task panicked").code, IpcErrorCode::Internal);
    /// ```
    pub fn internal(message: impl Into<String>) -> IpcError {
        IpcError::new(IpcErrorCode::Internal, message)
    }

    /// An author action a rule refused (D-57).
    ///
    /// ```
    /// use officina_core::ipc::{IpcError, IpcErrorCode};
    /// use officina_core::registry::types::AuthorRefusal;
    /// let refused = IpcError::refused_action(AuthorRefusal::NameTaken);
    /// assert_eq!((refused.code, refused.refusal), (IpcErrorCode::AuthorRefused, Some(AuthorRefusal::NameTaken)));
    /// ```
    pub fn refused_action(refusal: AuthorRefusal) -> IpcError {
        IpcError {
            code: IpcErrorCode::AuthorRefused,
            message: format!("refused: {}", refusal.code()),
            refusal: Some(refusal),
        }
    }
}
