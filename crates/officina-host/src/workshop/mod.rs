//! The isolated workshop on the host (AD-97): what the window, doctor and
//! the CLI read and run for it. Every rule lives in `officina_core::workshop`.

/// The folder of the Linux programs, for development and tests (AD-100);
/// the window's own is `<exe dir>/linux/`.
pub const ENV_LINUX_BIN_DIR: &str = "OFFICINA_LINUX_BIN_DIR";

use std::path::{Path, PathBuf};

use officina_core::workshop::WORKSHOP_DIR;

/// `<home>/workshop/` (AD-124).
pub fn workshop_dir(home: &Path) -> PathBuf {
    home.join(WORKSHOP_DIR)
}

/// `<home>/workshop/runs/`: one folder per run, named by its id.
pub fn runs_dir(home: &Path) -> PathBuf {
    workshop_dir(home).join("runs")
}

/// `<home>/workshop/build/`: the build contexts of Officina's images.
pub fn build_dir(home: &Path) -> PathBuf {
    workshop_dir(home).join("build")
}

/// `<home>/workshop/battery/`: the battery's read-only mount.
pub fn battery_dir(home: &Path) -> PathBuf {
    workshop_dir(home).join("battery")
}

/// `<home>/workshop/stopped`: the workshop's stop flag (AD-116).
pub fn stopped_file(home: &Path) -> PathBuf {
    workshop_dir(home).join("stopped")
}

/// True when the workshop is stopped (AD-116): its flag exists, or cannot
/// be looked at, AD-34's predicate on the workshop's own file (P-50).
pub fn workshop_stopped(home: &Path) -> bool {
    crate::relay::home::is_stopped(
        std::fs::metadata(stopped_file(home))
            .map(|_| ())
            .map_err(|e| e.kind()),
    )
}

/// Creates the workshop's folders on first use (AD-124), the window's
/// alone; the empty hooks folder is refused unless empty (AD-111).
pub fn ensure_home(home: &Path) -> Result<PathBuf, String> {
    for dir in [runs_dir(home), build_dir(home), battery_dir(home)] {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    git::ensure_empty_hooks(home)?;
    Ok(workshop_dir(home))
}

pub mod build;
pub mod doctor;
pub mod engine;
pub mod files;
pub mod git;
pub mod inventory;
pub mod proxy;
#[cfg(feature = "window-authority")]
pub mod threads;
