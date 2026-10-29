//! The TypeScript types of the board, relay and registry contracts are
//! generated from the Rust types (AD-13) and committed under
//! `app/src/lib/generated/`. This test regenerates them in memory and fails
//! when the committed files differ, so a Rust change cannot leave the
//! frontend on a stale contract.
//!
//! Regenerate with:
//! `OFFICINA_WRITE_TS=1 cargo test -p officina-core --features ts --test typescript_bindings`

#![cfg(feature = "ts")]
// The purity ban of clippy.toml protects the library; this test exists to
// write and compare generated files, which is IO by definition.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use officina_core::board::Board;
use officina_core::focus::FocusOutcome;
use officina_core::ipc::IpcError;
use officina_core::orders::card::{CardAnswer, CardChange, CardRequest, LaunchAnswer};
use officina_core::registry::confirm::{ConfirmItem, ItemAnswer, SubjectOrigin};
use officina_core::registry::types::Settlement;
use officina_core::registry::view::{DiscardedSession, RegistryView};
use officina_core::relay::view::RelayView;
use ts_rs::{Config, TS};

fn committed_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/lib/generated")
}

fn files_in(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "ts"))
                .map(|e| {
                    let text = std::fs::read_to_string(e.path())
                        .unwrap()
                        .replace("\r\n", "\n");
                    (e.file_name().to_string_lossy().into_owned(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn export(dir: &Path) {
    let cfg = Config::new().with_large_int("number").with_out_dir(dir);
    Board::export_all(&cfg).unwrap();
    FocusOutcome::export_all(&cfg).unwrap();
    IpcError::export_all(&cfg).unwrap();
    RelayView::export_all(&cfg).unwrap();
    RegistryView::export_all(&cfg).unwrap();
    SubjectOrigin::export_all(&cfg).unwrap();
    ConfirmItem::export_all(&cfg).unwrap();
    ItemAnswer::export_all(&cfg).unwrap();
    Settlement::export_all(&cfg).unwrap();
    DiscardedSession::export_all(&cfg).unwrap();
    CardRequest::export_all(&cfg).unwrap();
    CardChange::export_all(&cfg).unwrap();
    CardAnswer::export_all(&cfg).unwrap();
    LaunchAnswer::export_all(&cfg).unwrap();
}

#[test]
fn committed_bindings_match_the_rust_types() {
    if std::env::var_os("OFFICINA_WRITE_TS").is_some() {
        let dir = committed_dir();
        let _ = std::fs::remove_dir_all(&dir);
        export(&dir);
        return;
    }
    let fresh = tempfile_dir();
    export(&fresh);
    let expected = files_in(&fresh);
    assert!(!expected.is_empty());
    assert_eq!(
        files_in(&committed_dir()),
        expected,
        "app/src/lib/generated is stale: rerun with OFFICINA_WRITE_TS=1"
    );
    let _ = std::fs::remove_dir_all(&fresh);
}

fn tempfile_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("officina-ts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
