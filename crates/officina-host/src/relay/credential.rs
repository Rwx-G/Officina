//! The credential store seam (AD-36, D-29, D-41): where the secret of an
//! HTTP action's credential comes from. The Windows implementation reads
//! `officina:<name>` from the Credential Manager; elsewhere the store is
//! unsupported. Tests use a fake store, never the real one.
//!
//! A [`Secret`] is only read by `officina run` just before the request, by
//! doctor and by the window (presence only). It prints `<redacted>` under
//! `Debug`, has no `Display` and no `Serialize`, so it cannot reach a log,
//! the journal or a payload by accident.

use std::fmt;

use officina_core::relay::credential::BlobError;
#[cfg(windows)]
use officina_core::relay::credential::{decode_blob, target};

/// A credential's secret. Read it with [`Secret::expose`], at the one place
/// that builds the request header.
pub struct Secret(String);

impl Secret {
    /// Wraps a decoded secret.
    pub fn new(secret: String) -> Secret {
        Secret(secret)
    }

    /// The secret itself, for the request header only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Why a credential could not be read. Never carries the secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("the Credential Manager is not available on this platform")]
    Unsupported,
    #[error("the Credential Manager cannot be read: {0}")]
    Io(String),
    #[error("{0}")]
    Undecodable(BlobError),
}

/// Where credentials come from.
pub trait CredentialStore {
    /// The secret of credential `name` (stored as `officina:<name>`), or
    /// `None` when the store has no such entry.
    fn read(&self, name: &str) -> Result<Option<Secret>, StoreError>;

    /// Whether credential `name` is stored; the default reads it and drops
    /// the secret at once.
    fn present(&self, name: &str) -> Result<bool, StoreError> {
        self.read(name).map(|secret| secret.is_some())
    }
}

/// A store the window's collector owns and moves to its thread.
pub struct OwnedStore(pub Box<dyn CredentialStore + Send>);

impl OwnedStore {
    /// The system's store.
    pub fn system() -> OwnedStore {
        OwnedStore(Box::new(SystemCredentialStore))
    }
}

impl fmt::Debug for OwnedStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OwnedStore")
    }
}

/// The Windows Credential Manager (`CredReadW`); unsupported elsewhere.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCredentialStore;

impl CredentialStore for SystemCredentialStore {
    fn read(&self, name: &str) -> Result<Option<Secret>, StoreError> {
        #[cfg(windows)]
        {
            let blob = crate::platform::windows::read_generic_credential(&target(name))
                .map_err(|e| StoreError::Io(e.to_string()))?;
            blob.map(|blob| {
                decode_blob(&blob)
                    .map(Secret::new)
                    .map_err(StoreError::Undecodable)
            })
            .transpose()
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Err(StoreError::Unsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_is_redacted() {
        let secret = Secret::new("marker-s3cret".to_owned());
        assert_eq!(format!("{secret:?}"), "<redacted>");
        assert_eq!(format!("{:?}", Some(&secret)), "Some(<redacted>)");
        assert_eq!(secret.expose(), "marker-s3cret");
        let error = StoreError::Undecodable(BlobError::LineBreak);
        assert!(!format!("{error} {error:?}").contains("marker"));
    }

    /// The real store, on a name no workstation holds: present nowhere,
    /// unsupported off Windows. Never writes anything.
    #[test]
    fn system_store_reports_a_missing_entry() {
        let found = SystemCredentialStore.read("officina-test-never-stored-entry");
        if cfg!(windows) {
            assert!(matches!(found, Ok(None)), "{found:?}");
        } else {
            assert_eq!(found.unwrap_err(), StoreError::Unsupported);
        }
    }
}
