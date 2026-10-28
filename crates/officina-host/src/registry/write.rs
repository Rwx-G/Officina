//! The window's writes to the registry (AD-38): compiled only with the
//! `window-authority` feature (D-43), so `officina.exe` built alone holds
//! none of it.

use std::path::Path;

use officina_core::registry::types::{
    EventFileRecord, ExcerptRecord, SessionRow, SettingKey, Source,
};
use rusqlite::ErrorCode;

use super::read::{self, SettingRow};
use super::store::Writer;

/// Text every registry writer error carries. CI searches the release
/// binaries for it: absent from `officina.exe` (D-83). It lives in the
/// error type the writer returns, so `lto` and `strip` keep it in any
/// binary that calls the writer.
pub const REGISTRY_MARKER: &str = "officina-registry-writer";

/// Why the window's registry could not be opened or written.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{REGISTRY_MARKER}: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{REGISTRY_MARKER}: {path}: {error}")]
    Io { path: String, error: std::io::Error },
    /// `quick_check` found problems.
    #[error("{REGISTRY_MARKER}: the integrity check failed: {0}")]
    Damaged(String),
    /// The damaged database could not be set aside: nothing was deleted,
    /// and the registry stays unavailable until the next start (D-97).
    #[error(
        "{REGISTRY_MARKER}: the damaged registry could not be set aside ({path}: {error}); nothing was deleted"
    )]
    QuarantineFailed { path: String, error: std::io::Error },
}

impl RegistryError {
    pub(crate) fn io(path: &Path, error: std::io::Error) -> RegistryError {
        RegistryError::Io {
            path: path.display().to_string(),
            error,
        }
    }

    /// True when the database itself is damaged: `quick_check` failed, or
    /// SQLite reported `SQLITE_CORRUPT` or `SQLITE_NOTADB` (AD-42).
    pub fn is_corruption(&self) -> bool {
        match self {
            RegistryError::Damaged(_) => true,
            RegistryError::Sqlite(rusqlite::Error::SqliteFailure(error, _)) => matches!(
                error.code,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase
            ),
            _ => false,
        }
    }
}

impl Writer {
    /// Records `value` under `key`, replacing what was there.
    pub fn set_setting(
        &mut self,
        key: SettingKey,
        value: &str,
        src: Source,
        now_ms: i64,
    ) -> Result<(), RegistryError> {
        self.conn
            .prepare_cached(
                "INSERT INTO setting (key, value, src, at_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value, src = excluded.src, at_ms = excluded.at_ms",
            )?
            .execute(rusqlite::params![key.code(), value, src.code(), now_ms])?;
        Ok(())
    }

    /// Every `setting` row.
    pub fn settings(&self) -> Result<Vec<SettingRow>, RegistryError> {
        Ok(read::settings(&self.conn)?)
    }

    /// Copies every page of the WAL into the database and truncates the WAL
    /// to zero bytes, so a cleared text does not linger in an old frame
    /// (AD-39). Returns false when a reader kept it from completing.
    pub fn checkpoint(&self) -> Result<bool, RegistryError> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        Ok(busy == 0)
    }

    /// The value a setting of this connection reads back, by its name.
    pub fn pragma(&self, name: &str) -> Option<String> {
        read::pragma_value(&self.conn, name)
    }

    /// The schema version the database is at.
    pub fn user_version(&self) -> Result<i64, RegistryError> {
        Ok(read::user_version(&self.conn)?)
    }

    /// Every `event_file` row, by name.
    pub fn event_files(&self) -> Result<Vec<EventFileRecord>, RegistryError> {
        Ok(read::event_files(&self.conn)?)
    }

    /// Every `session` row, by session id.
    pub fn sessions(&self) -> Result<Vec<SessionRow>, RegistryError> {
        Ok(read::sessions(&self.conn)?)
    }

    /// Every `session_excerpt` row, by session id.
    pub fn excerpts(&self) -> Result<Vec<ExcerptRecord>, RegistryError> {
        Ok(read::excerpts(&self.conn)?)
    }
}
