//! A fake credential store for the tests of HTTP actions (story 2.11):
//! secrets by name, a name that cannot be read, and a count of every read,
//! so a test can say how often the store was asked. Automated tests never
//! touch the real Credential Manager. Included by `#[path]` from the host
//! and CLI test crates.

#![allow(dead_code)] // each test crate uses a different subset

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use officina_host::relay::credential::{CredentialStore, Secret, StoreError};

/// A marker no real secret holds: a scan for it finds every leak.
pub const MARKER_SECRET: &str = "officina-test-marker-s3cret";

#[derive(Debug, Clone, Default)]
pub struct FakeStore {
    /// `Some(secret)` stored; `None` makes a read fail.
    entries: BTreeMap<String, Option<String>>,
    reads: Arc<AtomicUsize>,
}

impl FakeStore {
    pub fn empty() -> FakeStore {
        FakeStore::default()
    }

    /// A store holding `secret` as `name`.
    pub fn with(name: &str, secret: &str) -> FakeStore {
        let mut store = FakeStore::default();
        store
            .entries
            .insert(name.to_owned(), Some(secret.to_owned()));
        store
    }

    /// A store that fails to read `name`.
    pub fn unreadable(name: &str) -> FakeStore {
        let mut store = FakeStore::default();
        store.entries.insert(name.to_owned(), None);
        store
    }

    /// Reads so far, by this store and its clones.
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

impl CredentialStore for FakeStore {
    fn read(&self, name: &str) -> Result<Option<Secret>, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        match self.entries.get(name) {
            None => Ok(None),
            Some(None) => Err(StoreError::Io("access denied".to_owned())),
            Some(Some(secret)) => Ok(Some(Secret::new(secret.clone()))),
        }
    }
}
