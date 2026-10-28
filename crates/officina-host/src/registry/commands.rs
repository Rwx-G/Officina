//! The command lines' read-only use of the registry (AD-53, D-47, D-82,
//! D-97): `board --json` fills its cards' subjects, `subjects --json` prints
//! the Subjects view, with the window open or closed. Neither writes.

use officina_core::registry::types::{RegistryReport, RegistryState};
use officina_core::registry::view::{IngestionView, RegistryView, ViewInput, build_view};

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

/// The view with nothing read, and the reason.
fn unread(report: RegistryReport, now_ms: i64) -> RegistryView {
    build_view(
        &ViewInput {
            registry: report,
            ingestion: IngestionView::default(),
            subjects: &[],
            sessions: &[],
            excerpts: &[],
            board: None,
            restore_notice: None,
        },
        now_ms,
    )
}

/// What `officina subjects --json` prints, and whether the registry was
/// read: from the read-only registry and a fresh board, whose staleness
/// does not make the registry unread (D-97).
pub fn subjects_view(collector: &mut Collector, now_ms: i64) -> (RegistryView, bool) {
    let opened = read_registry(collector);
    collector.refresh_agents(now_ms);
    let board = collector.board(now_ms);
    let report = board
        .diagnostics
        .registry
        .clone()
        .unwrap_or_else(|| reader_report(&opened));
    match &opened {
        Ok(reader) if report.state == RegistryState::Ready => {
            let events = collector.paths().events_dir();
            match reader.view(report.clone(), Some(&board), &events, now_ms) {
                Ok(view) => (view, true),
                Err(error) => (
                    unread(
                        RegistryReport {
                            state: RegistryState::Unavailable,
                            detail: Some(error.to_string()),
                            exclude_from_sync: report.exclude_from_sync,
                        },
                        now_ms,
                    ),
                    false,
                ),
            }
        }
        _ => (unread(report, now_ms), false),
    }
}
