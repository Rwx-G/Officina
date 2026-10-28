//! The command lines' read-only use of the registry (AD-53, D-47, D-82,
//! D-97): `board --json` fills its cards' subjects, `subjects --json` prints
//! the Subjects view, with the window open or closed. Neither writes.

use officina_core::registry::dry_run::DryRunState;
use officina_core::registry::model::unavailable;
use officina_core::registry::types::{RegistryReport, RegistryState};
use officina_core::registry::view::{RegistryView, unread_view};

use super::read::{Reader, ViewContext};
use super::store::{ReaderFailure, onedrive_roots_from_env, open_reader, reader_report};
use crate::collector::Collector;

/// Opens the registry of `collector`'s home read-only and hands the
/// collector its state, the subject of each filed session and what settles
/// each ended one (story 4.15): with the
/// registry in any other state than readable, cards show no subject and the
/// board says why (D-47).
pub fn read_registry(collector: &mut Collector) -> Result<Reader, ReaderFailure> {
    let opened = open_reader(&collector.paths().officina_home, &onedrive_roots_from_env());
    let mut report = reader_report(&opened);
    if let Ok(reader) = &opened {
        match reader.card_subjects().and_then(|subjects| {
            reader
                .card_settlements()
                .map(|settlements| (subjects, settlements))
        }) {
            Ok((subjects, settlements)) => {
                collector.set_subjects(subjects);
                collector.set_settlements(settlements);
            }
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

/// Whether a view rests on the registry or says why nothing was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewRead {
    Read,
    NotRead,
}

/// What `officina subjects --json` prints, and whether the registry was
/// read: from the read-only registry and a fresh board, whose staleness
/// does not make the registry unread (D-97).
pub fn subjects_view(collector: &mut Collector, now_ms: i64) -> (RegistryView, ViewRead) {
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
            let claude = collector
                .claude()
                .map(|path| path.to_string_lossy().into_owned());
            let ctx = ViewContext {
                report: report.clone(),
                board: Some(&board),
                events_dir: &events,
                dry_run: DryRunState::default(),
                unavailable: unavailable(claude.as_deref(), collector.no_excerpts()),
                no_excerpts: collector.no_excerpts(),
            };
            match reader.view(ctx, now_ms) {
                Ok(view) => (view, ViewRead::Read),
                Err(error) => (
                    unread_view(
                        RegistryReport {
                            state: RegistryState::Unavailable,
                            detail: Some(error.to_string()),
                            exclude_from_sync: report.exclude_from_sync,
                        },
                        now_ms,
                    ),
                    ViewRead::NotRead,
                ),
            }
        }
        _ => (unread_view(report, now_ms), ViewRead::NotRead),
    }
}
