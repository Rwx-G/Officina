//! Opening the registry (AD-38, AD-39, AD-41, AD-42): where it lives and
//! whether it may be opened there, the connection settings, the version
//! rule, the integrity check at open, quarantine, restore and the daily
//! copies.
//!
//! The reading half (location, readers, listing the copies) compiles
//! everywhere. The writing half (create, migrate, quarantine, restore,
//! copies) compiles only with the `window-authority` feature (D-83).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use officina_core::dates::{parse_utc_day, utc_day_text};
use officina_core::quota::normalise_path;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use super::RegistryPaths;
use super::read::{self, Reader};
use super::schema::{SCHEMA_VERSION, VersionState};

/// Entries whose presence in an ancestor folder marks a sync client's root
/// (AD-39). The only list of them: tests and doctor iterate it.
pub const SYNC_ROOT_MARKERS: &[&str] = &[".SynologyWorkingDirectory", ".dropbox"];

/// The environment variables OneDrive sets to its folders (AD-39).
pub const ONEDRIVE_VARIABLES: &[&str] = &["OneDrive", "OneDriveCommercial", "OneDriveConsumer"];

/// The OneDrive folders the environment names, read once by the caller and
/// passed to [`classify`], so tests set nothing process-wide.
pub fn onedrive_roots_from_env() -> Vec<PathBuf> {
    ONEDRIVE_VARIABLES
        .iter()
        .filter_map(|name| crate::paths::env_path(name))
        .collect()
}

/// Where the registry's folder lies, for the decision to open it (AD-39).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Local,
    /// Managed by a sync client, which may write an older copy back: opened,
    /// and `exclude` is the folder to keep out of the client.
    Synced {
        exclude: PathBuf,
    },
    /// On a network filesystem, where SQLite documents that WAL does not
    /// work: never opened.
    Network,
}

/// What the location check asks the system; injected so each answer is
/// tested without a network drive or a sync client.
pub trait LocationProbe {
    /// The drive whose root is `root` (`C:\`) is a network drive.
    fn is_remote_drive(&self, root: &str) -> bool;
    /// An entry exists at `path`.
    fn exists(&self, path: &Path) -> bool;
    /// `path` carries a Cloud Files placeholder attribute.
    fn is_cloud_placeholder(&self, path: &Path) -> bool;
}

/// The probe of the running system.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProbe;

impl LocationProbe for SystemProbe {
    fn is_remote_drive(&self, root: &str) -> bool {
        #[cfg(windows)]
        {
            crate::platform::windows::is_remote_drive(root)
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            false
        }
    }

    fn exists(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }

    fn is_cloud_placeholder(&self, path: &Path) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_PINNED, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
                FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_UNPINNED,
            };
            const CLOUD: u32 = FILE_ATTRIBUTE_PINNED
                | FILE_ATTRIBUTE_UNPINNED
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
            fs::metadata(path).is_ok_and(|m| m.file_attributes() & CLOUD != 0)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            false
        }
    }
}

/// The volume a path names, read from its text alone.
enum Volume {
    /// A UNC path (`\\server\share`), its verbatim form, or a device path.
    Network,
    /// A drive, by its root (`C:\`).
    Drive(String),
    /// Anything else: a relative path, or a path off Windows.
    Other,
}

fn drive_root(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        .then(|| format!("{}:\\", char::from(bytes[0].to_ascii_uppercase())))
}

fn volume_of(path: &Path) -> Volume {
    let text = path.to_string_lossy().replace('\\', "/");
    let Some(rest) = text.strip_prefix("//") else {
        return drive_root(&text).map_or(Volume::Other, Volume::Drive);
    };
    match rest.strip_prefix("?/") {
        Some(verbatim)
            if !verbatim
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case("UNC/")) =>
        {
            drive_root(verbatim).map_or(Volume::Network, Volume::Drive)
        }
        _ => Volume::Network,
    }
}

/// Classifies the registry folder `dir` (AD-39): network when its path is a
/// UNC path, without asking the system, or when its drive is remote; synced
/// when it lies under a OneDrive folder of `onedrive_roots`, when an
/// ancestor holds an entry of [`SYNC_ROOT_MARKERS`], or when it or an
/// ancestor carries a Cloud Files placeholder attribute; local otherwise.
pub fn classify(dir: &Path, onedrive_roots: &[PathBuf], probe: &dyn LocationProbe) -> Location {
    match volume_of(dir) {
        Volume::Network => return Location::Network,
        Volume::Drive(root) if probe.is_remote_drive(&root) => return Location::Network,
        Volume::Drive(_) | Volume::Other => {}
    }
    let synced = Location::Synced {
        exclude: dir.to_path_buf(),
    };
    let folder = normalise_path(&dir.to_string_lossy());
    let under_onedrive = onedrive_roots.iter().any(|root| {
        let root = normalise_path(&root.to_string_lossy());
        !root.is_empty()
            && (folder == root
                || folder
                    .strip_prefix(&root)
                    .is_some_and(|rest| rest.starts_with('/')))
    });
    if under_onedrive {
        return synced;
    }
    let marked = dir
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
        .any(|ancestor| {
            SYNC_ROOT_MARKERS
                .iter()
                .any(|marker| probe.exists(&ancestor.join(marker)))
                || probe.is_cloud_placeholder(ancestor)
        });
    if marked { synced } else { Location::Local }
}

/// The registry folder of `officina_home` classified on this system.
pub fn locate(officina_home: &Path, onedrive_roots: &[PathBuf]) -> Location {
    classify(
        RegistryPaths::new(officina_home).dir(),
        onedrive_roots,
        &SystemProbe,
    )
}

/// A reader's settings (AD-38, AD-39): name, value set, value read back.
pub const READER_PRAGMAS: &[(&str, &str, &str)] =
    &[("query_only", "ON", "1"), ("busy_timeout", "2000", "2000")];

/// Opens `db` the way every reader does: `SQLITE_OPEN_READ_ONLY`, then
/// [`READER_PRAGMAS`]. Never creates the file. On a database in WAL mode
/// whose window is closed, SQLite may create its `-wal` and `-shm`, which is
/// accepted (D-82): no row changes and no write lock is taken.
pub fn read_only_connection(db: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    for (name, value, _) in READER_PRAGMAS {
        conn.pragma_update(None, name, value)?;
    }
    Ok(conn)
}

officina_core::closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a reader found (AD-41's table, D-47). In every state but
    /// `ready`, nothing is read.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
    pub enum ReaderState {
        /// The version this build reads: read.
        Ready,
        /// No registry yet; a reader never creates one.
        Absent,
        /// Written by an older Officina: the window migrates it.
        Older,
        /// Written by a newer Officina.
        Newer,
        /// Cannot be opened or read.
        Unavailable,
        /// On a network folder: never opened.
        Network,
    }
}

/// Why a reader read nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderFailure {
    /// Never [`ReaderState::Ready`].
    pub state: ReaderState,
    /// The error met, when the state comes from one.
    pub detail: Option<String>,
}

impl ReaderFailure {
    fn new(state: ReaderState) -> ReaderFailure {
        ReaderFailure {
            state,
            detail: None,
        }
    }

    fn unavailable(error: impl ToString) -> ReaderFailure {
        ReaderFailure {
            state: ReaderState::Unavailable,
            detail: Some(error.to_string()),
        }
    }
}

/// Opens the registry of `officina_home` read-only, when it holds the
/// version this build reads.
pub fn open_reader(
    officina_home: &Path,
    onedrive_roots: &[PathBuf],
) -> Result<Reader, ReaderFailure> {
    open_reader_expecting(officina_home, onedrive_roots, SCHEMA_VERSION)
}

/// [`open_reader`] against `expected` rather than this build's version, so
/// the older-version state is tested while one version exists.
pub fn open_reader_expecting(
    officina_home: &Path,
    onedrive_roots: &[PathBuf],
    expected: i64,
) -> Result<Reader, ReaderFailure> {
    let paths = RegistryPaths::new(officina_home);
    let location = classify(paths.dir(), onedrive_roots, &SystemProbe);
    if location == Location::Network {
        return Err(ReaderFailure::new(ReaderState::Network));
    }
    let db = paths.database();
    match fs::metadata(&db) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(ReaderFailure::new(ReaderState::Absent));
        }
        Err(e) => return Err(ReaderFailure::unavailable(e)),
        Ok(meta) if !meta.is_file() => {
            return Err(ReaderFailure::unavailable(format!(
                "{} is not a file",
                db.display()
            )));
        }
        Ok(_) => {}
    }
    let conn = read_only_connection(&db).map_err(ReaderFailure::unavailable)?;
    let version = read::user_version(&conn).map_err(ReaderFailure::unavailable)?;
    match VersionState::of(version, expected) {
        VersionState::Absent => Err(ReaderFailure::new(ReaderState::Absent)),
        VersionState::Older => Err(ReaderFailure::new(ReaderState::Older)),
        VersionState::Newer => Err(ReaderFailure::new(ReaderState::Newer)),
        VersionState::Equal => Ok(Reader::new(conn, location)),
    }
}

/// `PRAGMA quick_check`'s findings: `["ok"]` when the database passed.
pub fn quick_check(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = conn.prepare("PRAGMA quick_check")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// True when `findings` are those of a database that passed its check.
pub fn passed(findings: &[String]) -> bool {
    findings == ["ok"]
}

/// True when the copy at `path` opens read-only, passes `quick_check` and
/// holds a schema. Copies are written by `VACUUM INTO`, outside WAL, so
/// opening one creates no file beside it.
pub fn copy_is_valid(path: &Path) -> bool {
    read_only_connection(path).is_ok_and(|conn| {
        quick_check(&conn).is_ok_and(|findings| passed(&findings))
            && read::user_version(&conn).is_ok_and(|version| version > 0)
    })
}

/// A daily copy's file name prefix (D-94): `registry-<UTC date>.db`.
const DAILY_COPY_PREFIX: &str = "registry-";

/// The daily copy taken on the UTC day of `now_ms`.
pub fn daily_copy_name(now_ms: i64) -> String {
    format!("{DAILY_COPY_PREFIX}{}.db", utc_day_text(now_ms))
}

/// The UTC day a daily copy's name stands for; `None` for any other name.
pub fn daily_copy_day(name: &str) -> Option<(i64, u32, u32)> {
    parse_utc_day(name.strip_prefix(DAILY_COPY_PREFIX)?.strip_suffix(".db")?)
}

/// A daily copy under `registry/backup/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCopy {
    pub path: PathBuf,
    /// The UTC day in its name, which decides when the next one is due and
    /// how old it is (D-94).
    pub day: (i64, u32, u32),
}

/// The daily copies of `paths`, newest day first.
pub fn daily_copies(paths: &RegistryPaths) -> Vec<DailyCopy> {
    let mut copies: Vec<DailyCopy> = fs::read_dir(paths.backup_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let day = daily_copy_day(&entry.file_name().to_string_lossy())?;
            Some(DailyCopy {
                path: entry.path(),
                day,
            })
        })
        .collect();
    copies.sort_by(|a, b| b.day.cmp(&a.day).then_with(|| b.path.cmp(&a.path)));
    copies
}

/// The folders under `registry/quarantine/`, each one database set aside.
pub fn quarantined(paths: &RegistryPaths) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(paths.quarantine_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

#[cfg(feature = "window-authority")]
pub use writing::{Integrity, WRITER_PRAGMAS, Writer, WriterOpen, open_writer, open_writer_with};

/// The writing half (D-83): create, migrate, quarantine, restore, copies.
#[cfg(feature = "window-authority")]
mod writing {
    use std::fs;
    use std::path::{Path, PathBuf};

    use officina_core::dates::utc_day_text;
    use officina_core::registry::retention::COPIES_KEPT;
    use officina_core::registry::types::{RestoreNotice, SettingKey, Source};
    use rusqlite::{Connection, OpenFlags};

    use super::super::RegistryPaths;
    use super::super::read::{self, Reader};
    use super::super::schema::{MIGRATIONS, VersionState, migrate};
    use super::super::write::RegistryError;
    use super::{
        Location, SystemProbe, classify, copy_is_valid, daily_copies, daily_copy_name, passed,
        quick_check, read_only_connection,
    };

    /// The writer's settings, applied at every open (AD-39): name, value
    /// set, value read back.
    pub const WRITER_PRAGMAS: &[(&str, &str, &str)] = &[
        ("journal_mode", "WAL", "wal"),
        ("synchronous", "FULL", "2"),
        ("secure_delete", "ON", "1"),
        ("foreign_keys", "ON", "1"),
        ("trusted_schema", "OFF", "0"),
        ("busy_timeout", "5000", "5000"),
    ];

    /// The window's one connection to the registry (AD-38): the collector
    /// thread holds it, and only it writes.
    #[derive(Debug)]
    pub struct Writer {
        pub(in crate::registry) conn: Connection,
        paths: RegistryPaths,
        location: Location,
    }

    /// What the window's open found (AD-41, AD-42).
    #[derive(Debug)]
    pub enum WriterOpen {
        /// Read and written.
        Writable {
            writer: Writer,
            integrity: Integrity,
        },
        /// Written by a newer Officina: read only, so nothing is ingested
        /// and nothing deleted.
        ReadOnly { reader: Reader },
        /// On a network folder: not opened, nothing created (AD-39).
        Network,
        /// Not opened: why. Nothing was deleted.
        Unavailable { error: RegistryError },
    }

    /// How the database came to be readable at open.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Integrity {
        /// There was none: created at the current version.
        Created,
        /// It passed its check at the current version.
        Sound,
        /// It passed its check at an older version and was migrated, after
        /// a copy when it held data.
        Migrated { from: i64, copy: Option<PathBuf> },
        /// It failed its check: set aside in `quarantined`, and replaced by
        /// the newest copy that passes (D-93).
        Restored {
            quarantined: PathBuf,
            notice: RestoreNotice,
        },
        /// It failed its check and no copy passes: set aside, and a new
        /// database started, to be fed from the event files on disk.
        StartedEmpty { quarantined: PathBuf },
    }

    /// Opens the registry of `officina_home` for the window at this build's
    /// version.
    pub fn open_writer(
        officina_home: &Path,
        onedrive_roots: &[PathBuf],
        now_ms: i64,
    ) -> WriterOpen {
        open_writer_with(officina_home, onedrive_roots, MIGRATIONS, now_ms)
    }

    /// [`open_writer`] with `migrations` in place of this build's, so the
    /// version rule is tested with a second migration (AD-41).
    pub fn open_writer_with(
        officina_home: &Path,
        onedrive_roots: &[PathBuf],
        migrations: &[&str],
        now_ms: i64,
    ) -> WriterOpen {
        let paths = RegistryPaths::new(officina_home);
        let location = classify(paths.dir(), onedrive_roots, &SystemProbe);
        if location == Location::Network {
            return WriterOpen::Network;
        }
        if let Err(error) = fs::create_dir_all(paths.dir()) {
            return WriterOpen::Unavailable {
                error: RegistryError::io(paths.dir(), error),
            };
        }
        let opening = Opening {
            paths,
            location,
            migrations,
            now_ms,
        };
        if !opening.paths.database().exists() {
            return opening.open_missing();
        }
        match open_connection(&opening.paths.database()) {
            Ok(conn) => opening.settle(conn, None),
            Err(error) if error.is_corruption() => opening.recover(),
            Err(error) => WriterOpen::Unavailable { error },
        }
    }

    /// Opens `db` read-write, creating it when absent, applies
    /// [`WRITER_PRAGMAS`] and runs `quick_check` (AD-42).
    fn open_connection(db: &Path) -> Result<Connection, RegistryError> {
        let conn = Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        for (name, value, _) in WRITER_PRAGMAS {
            conn.pragma_update(None, name, value)?;
        }
        let findings = quick_check(&conn)?;
        if !passed(&findings) {
            return Err(RegistryError::Damaged(findings.join("; ")));
        }
        Ok(conn)
    }

    /// True when a user table of `conn` holds a row.
    fn has_data(conn: &Connection) -> Result<bool, RegistryError> {
        let tables: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for table in tables {
            let quoted = table.replace('"', "\"\"");
            let any: bool = conn.query_row(
                &format!("SELECT EXISTS (SELECT 1 FROM \"{quoted}\")"),
                [],
                |row| row.get(0),
            )?;
            if any {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Writes a copy of `conn`'s database at `dest` (AD-42, D-94): the WAL
    /// is truncated first so no cleared page lingers in it, `VACUUM INTO`
    /// writes a temporary name, renamed once complete, so an interrupted
    /// copy never leaves a file with the copy's name.
    pub(in crate::registry) fn copy_to(
        conn: &Connection,
        dest: &Path,
    ) -> Result<(), RegistryError> {
        let dir = dest
            .parent()
            .ok_or_else(|| RegistryError::io(dest, std::io::ErrorKind::InvalidInput.into()))?;
        fs::create_dir_all(dir).map_err(|e| RegistryError::io(dir, e))?;
        let tmp = dest.with_extension("db.tmp");
        // The temporary name of an earlier copy that did not finish.
        if tmp.is_file() {
            fs::remove_file(&tmp).map_err(|e| RegistryError::io(&tmp, e))?;
        }
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        conn.execute("VACUUM INTO ?1", [tmp.to_string_lossy()])?;
        fs::rename(&tmp, dest).map_err(|e| RegistryError::io(dest, e))
    }

    struct Opening<'a> {
        paths: RegistryPaths,
        location: Location,
        migrations: &'a [&'a str],
        now_ms: i64,
    }

    impl Opening<'_> {
        /// Applies the version rule to a database that passed its check;
        /// `recovered` is how it was recovered, when it was.
        fn settle(self, mut conn: Connection, recovered: Option<Integrity>) -> WriterOpen {
            let target = i64::try_from(self.migrations.len()).unwrap_or(i64::MAX);
            let settled = (|| -> Result<Option<Integrity>, RegistryError> {
                let version = read::user_version(&conn)?;
                let integrity = match VersionState::of(version, target) {
                    VersionState::Newer => return Ok(None),
                    VersionState::Equal => Integrity::Sound,
                    VersionState::Absent => {
                        migrate(&mut conn, self.migrations)?;
                        Integrity::Created
                    }
                    VersionState::Older => {
                        let copy = if has_data(&conn)? {
                            let dest = self
                                .paths
                                .backup_dir()
                                .join(format!("pre-v{target}-{}.db", utc_day_text(self.now_ms)));
                            copy_to(&conn, &dest)?;
                            Some(dest)
                        } else {
                            None
                        };
                        migrate(&mut conn, self.migrations)?;
                        Integrity::Migrated {
                            from: version,
                            copy,
                        }
                    }
                };
                Ok(Some(recovered.clone().unwrap_or(integrity)))
            })();
            match settled {
                Ok(Some(integrity)) => {
                    let mut writer = Writer {
                        conn,
                        paths: self.paths,
                        location: self.location,
                    };
                    if let Integrity::Restored { notice, .. } = &integrity
                        && let Err(error) = writer.record_restore(notice, self.now_ms)
                    {
                        return WriterOpen::Unavailable { error };
                    }
                    WriterOpen::Writable { writer, integrity }
                }
                Ok(None) => {
                    drop(conn);
                    match read_only_connection(&self.paths.database()) {
                        Ok(conn) => WriterOpen::ReadOnly {
                            reader: Reader::new(conn, self.location),
                        },
                        Err(error) => WriterOpen::Unavailable {
                            error: error.into(),
                        },
                    }
                }
                Err(error) => WriterOpen::Unavailable { error },
            }
        }

        /// The database failed its check, or SQLite called it corrupt or not
        /// a database: it is set aside, then replaced by the newest copy
        /// that passes, else by a new database (AD-42).
        fn recover(self) -> WriterOpen {
            match self.quarantine() {
                Ok(Some(quarantined)) => self.restore(quarantined),
                Ok(None) => self.open_missing(),
                Err(error) => WriterOpen::Unavailable { error },
            }
        }

        /// No database file. A log left without it (a kill between the
        /// quarantine's moves) is set aside first, since SQLite would replay
        /// it into a new database. After a quarantine, the database is
        /// missing because a restore did not finish: it is finished.
        /// Otherwise this is a first start, and the database is created.
        fn open_missing(self) -> WriterOpen {
            let unfinished = self.paths.database().with_extension("db.restore");
            if unfinished.exists()
                && let Err(error) = fs::remove_file(&unfinished)
            {
                return WriterOpen::Unavailable {
                    error: RegistryError::io(&unfinished, error),
                };
            }
            if let Err(error) = self.quarantine() {
                return WriterOpen::Unavailable { error };
            }
            match super::quarantined(&self.paths).pop() {
                Some(newest) => self.restore(newest),
                None => match open_connection(&self.paths.database()) {
                    Ok(conn) => self.settle(conn, None),
                    Err(error) => WriterOpen::Unavailable { error },
                },
            }
        }

        /// Puts the newest copy that passes its check in place of the
        /// database set aside in `quarantined`, else starts a new one.
        fn restore(self, quarantined: PathBuf) -> WriterOpen {
            let db = self.paths.database();
            for candidate in restore_candidates(&self.paths) {
                if !copy_is_valid(&candidate) {
                    continue;
                }
                if let Err(error) = place(&candidate, &db) {
                    return WriterOpen::Unavailable { error };
                }
                let notice = RestoreNotice {
                    copy: candidate
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    restored_at_ms: self.now_ms,
                };
                return match open_connection(&db) {
                    Ok(conn) => self.settle(
                        conn,
                        Some(Integrity::Restored {
                            quarantined,
                            notice,
                        }),
                    ),
                    Err(error) => WriterOpen::Unavailable { error },
                };
            }
            match open_connection(&db) {
                Ok(conn) => self.settle(conn, Some(Integrity::StartedEmpty { quarantined })),
                Err(error) => WriterOpen::Unavailable { error },
            }
        }

        /// Moves the database, its `-wal` and its `-shm`, those that exist,
        /// to `registry/quarantine/<now_ms>/`; `None` when none does. A move
        /// that fails puts back what was moved and leaves the registry
        /// unavailable until the next start: nothing is deleted (D-97).
        fn quarantine(&self) -> Result<Option<PathBuf>, RegistryError> {
            let files: Vec<PathBuf> = [self.paths.database(), self.paths.wal(), self.paths.shm()]
                .into_iter()
                .filter(|f| f.exists())
                .collect();
            if files.is_empty() {
                return Ok(None);
            }
            let dir = self.paths.quarantine_dir().join(self.now_ms.to_string());
            fs::create_dir_all(&dir).map_err(|e| RegistryError::io(&dir, e))?;
            let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
            for file in files {
                let Some(name) = file.file_name() else {
                    continue;
                };
                let to = dir.join(name);
                if let Err(error) = fs::rename(&file, &to) {
                    for (from, to) in moved.into_iter().rev() {
                        let _ = fs::rename(&to, &from);
                    }
                    // Only an empty folder goes, so doctor does not report
                    // a quarantine that holds nothing.
                    let _ = fs::remove_dir(&dir);
                    return Err(RegistryError::QuarantineFailed {
                        path: file.display().to_string(),
                        error,
                    });
                }
                moved.push((file, to));
            }
            Ok(Some(dir))
        }
    }

    /// Every copy under `registry/backup/`, daily or pre-migration, newest
    /// modification first: the order restore tries them in (D-94).
    fn restore_candidates(paths: &RegistryPaths) -> Vec<PathBuf> {
        let mut found: Vec<(Option<std::time::SystemTime>, PathBuf)> =
            fs::read_dir(paths.backup_dir())
                .into_iter()
                .flatten()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|x| x == "db"))
                .map(|entry| {
                    let modified = entry.metadata().and_then(|m| m.modified()).ok();
                    (modified, entry.path())
                })
                .collect();
        found.sort_by(|a, b| b.cmp(a));
        found.into_iter().map(|(_, path)| path).collect()
    }

    /// Copies `copy` to `db` through a temporary name renamed once complete.
    fn place(copy: &Path, db: &Path) -> Result<(), RegistryError> {
        let tmp = db.with_extension("db.restore");
        fs::copy(copy, &tmp).map_err(|e| RegistryError::io(&tmp, e))?;
        fs::rename(&tmp, db).map_err(|e| RegistryError::io(db, e))
    }

    impl WriterOpen {
        /// The writer, when the registry is writable.
        pub fn writer(self) -> Option<Writer> {
            match self {
                WriterOpen::Writable { writer, .. } => Some(writer),
                _ => None,
            }
        }
    }

    impl Writer {
        /// Where the registry lies; a synced folder names what to exclude.
        pub fn location(&self) -> &Location {
            &self.location
        }

        /// The registry's files.
        pub fn paths(&self) -> &RegistryPaths {
            &self.paths
        }

        fn record_restore(
            &mut self,
            notice: &RestoreNotice,
            now_ms: i64,
        ) -> Result<(), RegistryError> {
            let value =
                serde_json::to_string(notice).map_err(|e| RegistryError::Damaged(e.to_string()))?;
            self.set_setting(SettingKey::RestoreNotice, &value, Source::Rules, now_ms)
        }

        /// Takes today's daily copy when the newest one's UTC date is
        /// earlier than today's (D-94), then keeps the [`COPIES_KEPT`]
        /// newest valid copies and deletes the other daily copies, so a
        /// damaged copy never pushes out a valid one. Returns the copy
        /// written, if one was due.
        pub fn copy_if_due(&mut self, now_ms: i64) -> Result<Option<PathBuf>, RegistryError> {
            let today = officina_core::dates::utc_date(now_ms);
            if daily_copies(&self.paths)
                .first()
                .is_some_and(|newest| newest.day >= today)
            {
                return Ok(None);
            }
            let dest = self.paths.backup_dir().join(daily_copy_name(now_ms));
            copy_to(&self.conn, &dest)?;
            let mut valid = 0;
            for copy in daily_copies(&self.paths) {
                if valid < COPIES_KEPT && copy_is_valid(&copy.path) {
                    valid += 1;
                } else {
                    fs::remove_file(&copy.path).map_err(|e| RegistryError::io(&copy.path, e))?;
                }
            }
            Ok(Some(dest))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// A probe answering from lists, recording every question.
    #[derive(Default)]
    struct FakeProbe {
        remote: Vec<&'static str>,
        existing: Vec<String>,
        placeholders: Vec<String>,
        asked: RefCell<Vec<String>>,
    }

    impl LocationProbe for FakeProbe {
        fn is_remote_drive(&self, root: &str) -> bool {
            self.asked.borrow_mut().push(format!("drive {root}"));
            self.remote.contains(&root)
        }

        fn exists(&self, path: &Path) -> bool {
            let path = normalise_path(&path.to_string_lossy());
            self.asked.borrow_mut().push(format!("exists {path}"));
            self.existing.contains(&path)
        }

        fn is_cloud_placeholder(&self, path: &Path) -> bool {
            let path = normalise_path(&path.to_string_lossy());
            self.asked.borrow_mut().push(format!("placeholder {path}"));
            self.placeholders.contains(&path)
        }
    }

    const DIR: &str = "C:/sync/home/registry";

    fn synced(dir: &str) -> Location {
        Location::Synced {
            exclude: PathBuf::from(dir),
        }
    }

    #[test]
    fn unc_path_is_network() {
        for unc in [
            r"\\server01\share\officina\registry",
            "//server01/share/officina/registry",
            r"\\?\UNC\server01\share\registry",
            r"\\localhost\officina-missing-share\registry",
        ] {
            let probe = FakeProbe::default();
            assert_eq!(
                classify(Path::new(unc), &[], &probe),
                Location::Network,
                "{unc}"
            );
            assert!(probe.asked.borrow().is_empty(), "{unc}: no system call");
        }
        let verbatim_drive = FakeProbe::default();
        assert_eq!(
            classify(Path::new(r"\\?\C:\home\registry"), &[], &verbatim_drive),
            Location::Local
        );
        assert_eq!(verbatim_drive.asked.borrow()[0], r"drive C:\");
    }

    #[test]
    fn remote_drive_type_is_network() {
        let probe = FakeProbe {
            remote: vec![r"Z:\"],
            ..Default::default()
        };
        assert_eq!(
            classify(Path::new("Z:/officina/registry"), &[], &probe),
            Location::Network
        );
        assert_eq!(
            classify(Path::new("z:/officina/registry"), &[], &probe),
            Location::Network,
            "the drive letter's case"
        );
        assert_eq!(classify(Path::new(DIR), &[], &probe), Location::Local);
    }

    #[test]
    fn each_sync_marker_in_an_ancestor_is_synced() {
        for marker in SYNC_ROOT_MARKERS {
            for ancestor in ["C:/sync", "C:/sync/home", "C:/sync/home/registry"] {
                let probe = FakeProbe {
                    existing: vec![normalise_path(&format!("{ancestor}/{marker}"))],
                    ..Default::default()
                };
                assert_eq!(
                    classify(Path::new(DIR), &[], &probe),
                    synced(DIR),
                    "{marker} in {ancestor}"
                );
            }
        }
    }

    #[test]
    fn onedrive_environment_path_is_synced() {
        let roots = [PathBuf::from(r"C:\Users\user01\OneDrive - Example")];
        let probe = FakeProbe::default();
        let inside = "c:/users/USER01/onedrive - example/officina/registry";
        assert_eq!(classify(Path::new(inside), &roots, &probe), synced(inside));
        let beside = "C:/Users/user01/OneDrive - Example2/registry";
        assert_eq!(
            classify(Path::new(beside), &roots, &probe),
            Location::Local,
            "a sibling folder sharing the prefix"
        );
        assert_eq!(
            classify(Path::new(DIR), &[PathBuf::new()], &probe),
            Location::Local,
            "an empty variable names no folder"
        );
    }

    #[test]
    fn cloud_files_attribute_is_synced() {
        for placeholder in ["C:/sync", "C:/sync/home/registry"] {
            let probe = FakeProbe {
                placeholders: vec![normalise_path(placeholder)],
                ..Default::default()
            };
            assert_eq!(classify(Path::new(DIR), &[], &probe), synced(DIR));
        }
    }

    #[test]
    fn local_folder_is_local() {
        let probe = FakeProbe::default();
        assert_eq!(classify(Path::new(DIR), &[], &probe), Location::Local);
        let asked = probe.asked.borrow();
        assert_eq!(asked[0], r"drive C:\");
        for marker in SYNC_ROOT_MARKERS {
            let expected = format!("exists {}", normalise_path(&format!("C:/sync/{marker}")));
            assert!(asked.contains(&expected), "{expected}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn temp_dir_drive_is_not_remote() {
        let temp = std::env::temp_dir();
        let Volume::Drive(root) = volume_of(&temp) else {
            panic!("{} has no drive", temp.display());
        };
        assert!(!SystemProbe.is_remote_drive(&root), "{root}");
    }

    #[test]
    fn daily_copy_names_carry_the_utc_day() {
        let now = 1_793_102_400_000;
        let name = daily_copy_name(now);
        assert_eq!(name, "registry-2026-10-27.db");
        assert_eq!(daily_copy_day(&name), Some((2026, 10, 27)));
        for other in [
            "registry-2026-10-27.db.tmp",
            "pre-v2-2026-10-27.db",
            "registry-26-10-27.db",
            "registry.db",
        ] {
            assert_eq!(daily_copy_day(other), None, "{other}");
        }
    }
}
