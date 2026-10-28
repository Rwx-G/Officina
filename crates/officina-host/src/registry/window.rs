//! The window's registry (AD-38, AD-41, AD-42, D-47, D-91): what the
//! collector thread holds, whatever the open found. Only a writable
//! registry ingests or deletes; one written by a newer Officina, one that
//! could not be opened and one found damaged while the window runs
//! (unavailable until the next start) do neither, and the board says why.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use officina_core::agents::AgentRecord;
use officina_core::board::ProjectRule;
use officina_core::registry::fold::{AgentsChanges, AgentsSnapshot};
use officina_core::registry::types::{RegistryReport, RegistryState, SubjectRef};

use super::ingest::{IngestContext, IngestReport};
use super::read::{self, Reader};
use super::retention::PassReport;
use super::store::{Integrity, Writer, WriterOpen, open_writer};
use super::write::RegistryError;

#[derive(Debug)]
enum Slot {
    Writable(Writer),
    /// Written by a newer Officina.
    ReadOnly(Reader),
    /// Why it is not usable, until the next start.
    Unavailable(String),
    Network,
}

/// The registry as the window's collector thread holds it.
#[derive(Debug)]
pub struct WindowRegistry {
    slot: Slot,
    /// How the database came to be readable at open, when it was opened
    /// writable.
    integrity: Option<Integrity>,
    /// The last error that did not make the registry unavailable (a disk
    /// full, a busy file), cleared by the next success.
    last_error: Option<String>,
    /// The previous `claude agents` snapshot, in memory only (AD-46);
    /// seeded from the registry at the first refresh (D-51).
    agents: Option<AgentsSnapshot>,
}

impl WindowRegistry {
    /// Opens the registry of `officina_home` for the window (story 3.1).
    pub fn open(officina_home: &Path, onedrive_roots: &[PathBuf], now_ms: i64) -> WindowRegistry {
        WindowRegistry::from_open(open_writer(officina_home, onedrive_roots, now_ms))
    }

    /// The window's registry from what an open found.
    pub fn from_open(opened: WriterOpen) -> WindowRegistry {
        let (slot, integrity) = match opened {
            WriterOpen::Writable { writer, integrity } => (Slot::Writable(writer), Some(integrity)),
            WriterOpen::ReadOnly { reader } => (Slot::ReadOnly(reader), None),
            WriterOpen::Network => (Slot::Network, None),
            WriterOpen::Unavailable { error } => (Slot::Unavailable(error.to_string()), None),
        };
        WindowRegistry {
            slot,
            integrity,
            last_error: None,
            agents: None,
        }
    }

    /// How the database came to be readable at open.
    pub fn integrity(&self) -> Option<&Integrity> {
        self.integrity.as_ref()
    }

    /// The writer, while the registry is writable.
    pub fn writer(&self) -> Option<&Writer> {
        match &self.slot {
            Slot::Writable(writer) => Some(writer),
            _ => None,
        }
    }

    /// The subject of each filed session, by session id, while the
    /// registry is writable; empty otherwise, and the board says why
    /// (AD-54, D-47).
    pub fn card_subjects(&self) -> BTreeMap<String, SubjectRef> {
        self.writer()
            .and_then(|writer| read::card_subjects(&writer.conn).ok())
            .unwrap_or_default()
    }

    /// What the board says of the registry (`Diagnostics.registry`).
    pub fn report(&self) -> RegistryReport {
        let (state, detail, location) = match &self.slot {
            Slot::Writable(writer) => (
                RegistryState::Ready,
                self.last_error.clone(),
                Some(writer.location()),
            ),
            Slot::ReadOnly(reader) => (RegistryState::Newer, None, Some(reader.location())),
            Slot::Unavailable(reason) => (RegistryState::Unavailable, Some(reason.clone()), None),
            Slot::Network => (RegistryState::Network, None, None),
        };
        RegistryReport {
            state,
            detail,
            exclude_from_sync: location
                .and_then(|l| l.exclude())
                .map(|p| p.display().to_string()),
        }
    }

    /// Runs `step` on the writer, when there is one: an error that says the
    /// database is damaged makes the registry unavailable until the next
    /// start (AD-42); any other is reported and the registry stays.
    fn on_writer<T>(
        &mut self,
        step: impl FnOnce(&mut Writer) -> Result<T, RegistryError>,
    ) -> Option<T> {
        let Slot::Writable(writer) = &mut self.slot else {
            return None;
        };
        match step(writer) {
            Ok(done) => {
                self.last_error = None;
                Some(done)
            }
            Err(error) if error.is_corruption() => {
                let found = std::mem::replace(&mut self.slot, Slot::Unavailable(error.to_string()));
                if let Slot::Writable(writer) = found {
                    writer.abandon();
                }
                None
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
                None
            }
        }
    }

    /// One ingestion pass, when the registry is writable.
    pub fn ingest(&mut self, ctx: &IngestContext, now_ms: i64) -> Option<IngestReport> {
        self.on_writer(|writer| writer.ingest(ctx, now_ms))
    }

    /// Folds a successful `claude agents` refresh into the registry, when
    /// it is writable (AD-46). Only call it after a refresh that succeeded:
    /// a failed one keeps the previous snapshot and writes nothing.
    pub fn apply_agents(
        &mut self,
        records: &[AgentRecord],
        projects: &[ProjectRule],
        now_ms: i64,
    ) -> Option<AgentsChanges> {
        let mut snapshot = self.agents.take();
        let applied =
            self.on_writer(|writer| writer.apply_agents(&mut snapshot, records, projects, now_ms));
        self.agents = snapshot;
        applied
    }

    /// The registry's hourly pass, when the registry is writable: read-only
    /// or unavailable, nothing is ingested and nothing deleted.
    pub fn hourly_pass(&mut self, ctx: &IngestContext, now_ms: i64) -> Option<PassReport> {
        self.on_writer(|writer| writer.hourly_pass(ctx, now_ms))
    }
}
