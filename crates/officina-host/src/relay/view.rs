//! Reads what the relay view shows (AD-35, D-24): the stop flag, the
//! approved snapshot, the relay floor and two days of journal, for core's
//! [`officina_core::relay::view::build`]. Read-only: `officina relay --json`
//! and the window's collector both call it.

use std::collections::BTreeMap;
use std::fs;
use std::io;

use officina_core::relay::StopFlag;
use officina_core::relay::record::{Anomaly, AnomalyInputs, anomalies};
use officina_core::relay::run::SnapshotState;
use officina_core::relay::view::{FloorRead, RelayView, StopState, ViewInputs, build};

use super::approvals::read_files;
use super::credential::CredentialStore;
use super::home::{ApprovedState, RelayHome, load_relay_floor};
use super::journal::read_recent;

/// Why no view could be read: the Officina home is not a folder that can be
/// listed. A home that does not exist yet is an empty relay, not an error.
#[derive(Debug, thiserror::Error)]
#[error("cannot read the Officina home {path}: {error}")]
pub struct HomeUnreadable {
    pub path: String,
    pub error: String,
}

fn check_home(home: &RelayHome) -> Result<(), HomeUnreadable> {
    let path = home.officina_home();
    let unreadable = |error: String| HomeUnreadable {
        path: path.display().to_string(),
        error,
    };
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(unreadable(e.to_string())),
        Ok(meta) if !meta.is_dir() => Err(unreadable("not a folder".to_owned())),
        Ok(_) => fs::read_dir(path)
            .map(|_| ())
            .map_err(|e| unreadable(e.to_string())),
    }
}

/// The stop flag (AD-34): absent means running; present means stopped, with
/// its time when it holds a well-formed flag. A flag that cannot be read
/// still stops the relay.
pub fn read_stop(home: &RelayHome) -> StopState {
    match fs::read(home.stopped_file()) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => StopState::Running,
        Ok(bytes) => StopState::Stopped {
            since_ms: StopFlag::parse(&bytes).map(|flag| flag.since_ms),
        },
        Err(_) => StopState::Stopped { since_ms: None },
    }
}

/// Reads the relay's state at `now_ms` and builds its view, every
/// credential unchecked: `officina relay --json` never reads the store
/// (D-41).
pub fn read_view(home: &RelayHome, now_ms: i64) -> Result<RelayView, HomeUnreadable> {
    read_view_with(home, now_ms, None)
}

/// [`read_view`] with what the window found in the credential store
/// ([`credential_presence`]), by credential name.
pub fn read_view_with(
    home: &RelayHome,
    now_ms: i64,
    credential_presence: Option<&BTreeMap<String, bool>>,
) -> Result<RelayView, HomeUnreadable> {
    check_home(home)?;
    let stop = read_stop(home);
    let approved = home.read_approved();
    let floor_path = home.relay_floor(&approved);
    let floor_text = floor_path.display().to_string();
    let missing = matches!(
        fs::metadata(&floor_path),
        Err(e) if e.kind() == io::ErrorKind::NotFound
    );
    let loaded = (!missing).then(|| load_relay_floor(&floor_path));
    let floor_error = match &loaded {
        Some(Err(e)) => e.to_string(),
        _ => String::new(),
    };
    let floor = match &loaded {
        None => FloorRead::Missing,
        Some(Ok(relay)) => FloorRead::Valid(relay),
        Some(Err(_)) => FloorRead::Unreadable(&floor_error),
    };
    // A snapshot gone while the known-projects record names an approval is
    // broken, not absent: the gate guards every project then (D-15), and the
    // author must be offered a new approval whatever the floor holds.
    let recorded = home
        .read_known_projects()
        .is_some_and(|known| known.approved_hash.is_some());
    let gone = format!(
        "{} is gone while {} records an approval",
        home.approved_file().display(),
        home.known_projects_file().display()
    );
    let snapshot = match &approved {
        ApprovedState::Missing if recorded => SnapshotState::Unreadable(&gone),
        ApprovedState::Missing => SnapshotState::Missing,
        ApprovedState::Unreadable(why) => SnapshotState::Unreadable(why),
        ApprovedState::Valid(approved) => SnapshotState::Valid(approved),
    };
    let (lines, journal_corrupt_lines) = read_recent(home, now_ms);
    let anomalies = read_anomalies(
        home,
        &lines,
        approved.approved().map(|a| a.hash.as_str()),
        !matches!(stop, StopState::Running),
    );
    Ok(build(&ViewInputs {
        stop,
        snapshot,
        floor_path: &floor_text,
        floor,
        lines: &lines,
        journal_corrupt_lines,
        now_ms,
        credential_presence,
        anomalies: &anomalies,
    }))
}

/// The decisions the window did not make (D-173), judged on the window's
/// record, the approval files, `lines` of journal, the approved hash and
/// whether the relay is stopped.
pub fn read_anomalies(
    home: &RelayHome,
    lines: &[officina_core::relay::journal::JournalLine],
    approved_hash: Option<&str>,
    stopped: bool,
) -> Vec<Anomaly> {
    let record = home.read_record();
    if record.is_none() {
        return Vec::new();
    }
    anomalies(&AnomalyInputs {
        record: record.as_ref(),
        files: &read_files(home),
        lines,
        approved_hash,
        stopped,
    })
}

/// Whether each credential of the approved catalogue is in `store`, by
/// name (D-41); a credential the store cannot answer for is left out, so
/// it shows unchecked. Empty when nothing is approved.
pub fn credential_presence(
    home: &RelayHome,
    store: &dyn CredentialStore,
) -> BTreeMap<String, bool> {
    let ApprovedState::Valid(approved) = home.read_approved() else {
        return BTreeMap::new();
    };
    approved
        .canonical
        .credentials
        .keys()
        .filter_map(|name| Some((name.clone(), store.present(name).ok()?)))
        .collect()
}
