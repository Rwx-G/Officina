//! The window's writes to the registry (AD-38): compiled only with the
//! `window-authority` feature (D-43), so `officina.exe` built alone holds
//! none of it.

use std::path::Path;

use officina_core::registry::fold::FactWrite;
use officina_core::registry::types::{
    EventFileRecord, ExcerptRecord, FactValue, SessionRecord, SessionRow, SettingKey, Source,
};
use rusqlite::config::DbConfig;
use rusqlite::types::Value;
use rusqlite::{Connection, ErrorCode, TransactionBehavior};

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

    /// Closes the connection without the checkpoint SQLite runs when the
    /// last connection closes, so a database found damaged is left as it
    /// was, its WAL beside it, for the next start to set aside (AD-42).
    pub fn abandon(self) {
        let _ = self
            .conn
            .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true);
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

    /// Every session with its facts, by session id (AD-40).
    pub fn session_records(&self) -> Result<Vec<SessionRecord>, RegistryError> {
        Ok(read::session_records(&self.conn)?)
    }

    /// Records `writes` in one transaction, each replacing the stored fact
    /// only when it is later (AD-43). Every session must exist.
    pub fn record_facts(&mut self, writes: &[FactWrite]) -> Result<(), RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for write in writes {
            write_fact(&tx, write)?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Writes one fact, replacing the stored row only when
/// [`officina_core::registry::fold::replaces`] says so: whatever the order
/// observations arrive in, the latest wins, and a replayed one changes
/// nothing (AD-43).
pub(crate) fn write_fact(conn: &Connection, write: &FactWrite) -> rusqlite::Result<()> {
    let value = match &write.value {
        FactValue::Integer(i) => Value::Integer(*i),
        FactValue::Text(text) => Value::Text(text.clone()),
    };
    // The `WHERE` is `replaces` in SQL: a strictly later observation only.
    // The fold vectors replayed through the writer hold the two together.
    conn.prepare_cached(
        "INSERT INTO session_fact (session_id, fact, value, src, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (session_id, fact) DO UPDATE SET value = excluded.value, src = excluded.src,
             at_ms = excluded.at_ms
         WHERE excluded.at_ms > session_fact.at_ms",
    )?
    .execute(rusqlite::params![
        write.session_id,
        write.fact.code(),
        value,
        write.src.code(),
        write.at_ms
    ])?;
    Ok(())
}
