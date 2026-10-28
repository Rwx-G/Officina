//! The registry (PRD v0.3, AD-38 to AD-42): one local SQLite file holding
//! the sessions, subjects and verdicts that outlive the board's 24 hours.
//!
//! **Only the window writes**, through the one connection of its collector
//! thread. [`write`] and the writing half of [`store`] (create, migrate,
//! quarantine, restore, copies) compile only with the `window-authority`
//! feature (D-43, D-83), so `officina.exe` built alone cannot write; every
//! command line opens a [`read::Reader`], read-only.

use std::path::{Path, PathBuf};

#[cfg(feature = "window-authority")]
pub mod ingest;
pub mod read;
pub mod schema;
pub mod store;
#[cfg(feature = "window-authority")]
pub mod write;

/// The registry's folder under the Officina home: the database, its `-wal`
/// and `-shm`, `backup/` and `quarantine/` sit together, so one sync
/// exclusion covers them (AD-39).
pub const REGISTRY_DIR: &str = "registry";

/// Where the registry's files are, for one Officina home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPaths {
    dir: PathBuf,
}

impl RegistryPaths {
    pub fn new(officina_home: &Path) -> RegistryPaths {
        RegistryPaths {
            dir: officina_home.join(REGISTRY_DIR),
        }
    }

    /// `<officina home>/registry`, the folder to exclude from a sync client.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `registry/registry.db`.
    pub fn database(&self) -> PathBuf {
        self.dir.join("registry.db")
    }

    /// The database's write-ahead log, next to it.
    pub fn wal(&self) -> PathBuf {
        self.dir.join("registry.db-wal")
    }

    /// The database's shared-memory index, next to it.
    pub fn shm(&self) -> PathBuf {
        self.dir.join("registry.db-shm")
    }

    /// `registry/backup/`: the daily copies and the pre-migration copies.
    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join("backup")
    }

    /// `registry/quarantine/`: one folder per database set aside.
    pub fn quarantine_dir(&self) -> PathBuf {
        self.dir.join("quarantine")
    }
}
