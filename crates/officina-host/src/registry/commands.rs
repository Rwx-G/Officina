//! The command lines' read-only use of the registry (AD-53, D-47, D-82):
//! `board --json` fills its cards' subjects, with the window open or
//! closed. It writes nothing.

use officina_core::registry::types::{RegistryReport, RegistryState};

use super::read::Reader;
use super::store::{ReaderFailure, onedrive_roots_from_env, open_reader, reader_report};
use crate::collector::Collector;

/// Opens the registry of `collector`'s home read-only and hands the
/// collector its state and the subject of each filed session: with the
/// registry in any other state than readable, cards show no subject and the
/// board says why (D-47).
pub fn read_registry(collector: &mut Collector) -> Result<Reader, ReaderFailure> {
    let opened = open_reader(&collector.paths().officina_home, &onedrive_roots_from_env());
    let mut report = reader_report(&opened);
    if let Ok(reader) = &opened {
        match reader.card_subjects() {
            Ok(subjects) => collector.set_subjects(subjects),
            Err(error) => {
                report = RegistryReport {
                    state: RegistryState::Unavailable,
                    detail: Some(error.to_string()),
                    exclude_from_sync: report.exclude_from_sync,
                }
            }
        }
    }
    collector.set_registry(report);
    opened
}
