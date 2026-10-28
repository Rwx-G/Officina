//! Reading the registry (AD-38): the [`Reader`] every command line opens,
//! and the queries it shares with the window's writer, each turning rows
//! into core records.

use officina_core::registry::types::{RestoreNotice, SettingKey, Source};
use rusqlite::Connection;
use rusqlite::types::ValueRef;

use super::store::{self, Location};

/// One `setting` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    pub key: SettingKey,
    /// The value as text: JSON for structured settings.
    pub value: String,
    pub src: Source,
    pub at_ms: i64,
}

/// A read-only connection to the registry (AD-38, AD-39), opened by
/// [`store::open_reader`]: every command line's, and the window's when the
/// database was written by a newer Officina. It has no writing method;
/// calling one does not compile:
///
/// ```compile_fail
/// fn write_through(reader: &mut officina_host::registry::read::Reader) {
///     let _ = reader.copy_if_due(0);
/// }
/// ```
///
/// while the same function reading compiles:
///
/// ```
/// fn read_through(reader: &mut officina_host::registry::read::Reader) {
///     let _ = reader.settings();
/// }
/// ```
#[derive(Debug)]
pub struct Reader {
    conn: Connection,
    location: Location,
}

impl Reader {
    pub(crate) fn new(conn: Connection, location: Location) -> Reader {
        Reader { conn, location }
    }

    /// Where the registry lies; a synced folder names what to exclude.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// The schema version the database is at.
    pub fn user_version(&self) -> rusqlite::Result<i64> {
        user_version(&self.conn)
    }

    /// The value a setting of this connection reads back, by its name.
    pub fn pragma(&self, name: &str) -> Option<String> {
        pragma_value(&self.conn, name)
    }

    /// `PRAGMA quick_check`'s findings, which only doctor asks a reader for
    /// (D-47).
    pub fn quick_check(&self) -> rusqlite::Result<Vec<String>> {
        store::quick_check(&self.conn)
    }

    /// Every `setting` row.
    pub fn settings(&self) -> rusqlite::Result<Vec<SettingRow>> {
        settings(&self.conn)
    }

    /// Which copy replaced a damaged registry, and when (D-93).
    pub fn restore_notice(&self) -> Option<RestoreNotice> {
        restore_notice(&self.conn)
    }
}

/// `PRAGMA user_version`.
pub(crate) fn user_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

/// What `PRAGMA <name>` reads back, as text; `None` for a name that is not
/// a plain pragma name, or one SQLite does not answer.
pub(crate) fn pragma_value(conn: &Connection, name: &str) -> Option<String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return None;
    }
    conn.query_row(&format!("PRAGMA {name}"), [], |row| {
        Ok(text_of(row.get_ref(0)?))
    })
    .ok()
}

fn text_of(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(r) => r.to_string(),
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
    }
}

/// Every `setting` row whose key and source this build knows.
pub(crate) fn settings(conn: &Connection) -> rusqlite::Result<Vec<SettingRow>> {
    let mut statement =
        conn.prepare_cached("SELECT key, value, src, at_ms FROM setting ORDER BY key")?;
    let rows = statement.query_map([], |row| {
        let key: String = row.get(0)?;
        let src: String = row.get(2)?;
        Ok((key, text_of(row.get_ref(1)?), src, row.get::<_, i64>(3)?))
    })?;
    let mut found = Vec::new();
    for row in rows {
        let (key, value, src, at_ms) = row?;
        if let (Some(key), Some(src)) = (SettingKey::from_code(&key), Source::from_code(&src)) {
            found.push(SettingRow {
                key,
                value,
                src,
                at_ms,
            });
        }
    }
    Ok(found)
}

/// The restore notice, when one is recorded and readable.
pub(crate) fn restore_notice(conn: &Connection) -> Option<RestoreNotice> {
    settings(conn)
        .ok()?
        .into_iter()
        .find(|row| row.key == SettingKey::RestoreNotice)
        .and_then(|row| serde_json::from_str(&row.value).ok())
}
