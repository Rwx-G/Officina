//! Schema version 1 and the migration runner (story 3.1 AC 4 and 5; AD-40,
//! AD-41, D-88, P-14): STRICT tables, `CHECK` lists equal to the core
//! enums, pinned migrations, one transaction per migration, a copy before
//! migrating data, a fixture per past version.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::registry::{Home, NOW, writable};
use officina_core::registry::types::{
    ModelVerdict, ProposalSource, ReasonCode, SettingKey, Source, SubjectStatus, TriageOutcome,
    TriagePurpose, Verdict,
};
use officina_host::registry::schema::{MIGRATIONS, SCHEMA_VERSION, migrate};
use officina_host::registry::store::{
    Integrity, WriterOpen, open_reader_expecting, open_writer, open_writer_with,
    read_only_connection,
};
use rusqlite::Connection;

/// The SHA-256 of each shipped migration, in order (AD-41). A shipped
/// entry never changes; a new entry fails here until its hash is pinned.
const PINNED: [&str; 1] = ["5dbc947a1e68c76dce049a886eb1a80b65d7fa509f1ea6bc019eb707669387a7"];

/// A database at the current version, schema only.
fn fresh() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&mut conn, MIGRATIONS).unwrap();
    conn
}

fn tables(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// One column as the schema declares it.
#[derive(Debug)]
struct Column {
    declared_type: String,
    /// The values its `CHECK (<column> IN (...))` allows, when it has one.
    check: Option<BTreeSet<String>>,
}

/// Every column of every table, by `table.column`, with the `CHECK` list
/// read from the table's SQL in `sqlite_master`.
fn columns(conn: &Connection) -> BTreeMap<String, Column> {
    let mut found = BTreeMap::new();
    for table in tables(conn) {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [&table],
                |row| row.get(0),
            )
            .unwrap();
        let declared: Vec<(String, String)> = conn
            .prepare(&format!(
                "SELECT name, type FROM pragma_table_info('{table}')"
            ))
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for (name, declared_type) in declared {
            let definition = sql
                .lines()
                .map(str::trim)
                .find(|line| line.starts_with(&format!("{name} ")))
                .unwrap_or_else(|| panic!("{table}.{name} has no line of its own"));
            let check = definition.split_once("CHECK (").map(|(_, rest)| {
                let list = rest
                    .split_once(" IN (")
                    .and_then(|(column, list)| {
                        assert_eq!(column, name, "{table}: a CHECK names another column");
                        list.split_once(')')
                    })
                    .unwrap_or_else(|| panic!("{table}.{name}: not a CHECK ... IN list"))
                    .0;
                list.split(',')
                    .map(|value| value.trim().trim_matches('\'').to_owned())
                    .collect()
            });
            found.insert(
                format!("{table}.{name}"),
                Column {
                    declared_type,
                    check,
                },
            );
        }
    }
    found
}

fn codes<T: Copy>(all: &[T], code: fn(T) -> &'static str) -> BTreeSet<String> {
    all.iter().map(|v| code(*v).to_owned()).collect()
}

/// D-88: which enum each enum-valued column takes, and the 0-or-1 columns.
fn expected_checks() -> BTreeMap<&'static str, BTreeSet<String>> {
    let source = codes(Source::ALL, Source::code);
    let model_verdict = codes(ModelVerdict::ALL, ModelVerdict::code);
    let flag: BTreeSet<String> = ["0", "1"].map(str::to_owned).into();
    BTreeMap::from([
        ("session_fact.src", source.clone()),
        ("session.subject_src", source.clone()),
        ("subject.status_src", source.clone()),
        ("setting.src", source),
        (
            "subject.status",
            codes(SubjectStatus::ALL, SubjectStatus::code),
        ),
        ("confirmation.proposed_verdict", model_verdict.clone()),
        ("model_answer.verdict", model_verdict),
        ("confirmation.verdict", codes(Verdict::ALL, Verdict::code)),
        (
            "confirmation.proposed_source",
            codes(ProposalSource::ALL, ProposalSource::code),
        ),
        (
            "confirmation.proposed_reason",
            codes(ReasonCode::ALL, ReasonCode::code),
        ),
        (
            "triage_call.purpose",
            codes(TriagePurpose::ALL, TriagePurpose::code),
        ),
        (
            "triage_call.outcome",
            codes(TriageOutcome::ALL, TriageOutcome::code),
        ),
        ("setting.key", codes(SettingKey::ALL, SettingKey::code)),
        ("subject.routine", flag.clone()),
        ("confirmation.auto", flag.clone()),
        ("confirmation_input.cut_mid_turn", flag),
    ])
}

#[test]
fn every_table_is_strict() {
    let conn = fresh();
    let listed: Vec<(String, bool, bool)> = conn
        .prepare("SELECT name, strict, wr FROM pragma_table_list WHERE schema = 'main' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(listed.len(), tables(&conn).len());
    for (name, strict, without_rowid) in listed {
        assert!(strict, "{name} is not STRICT");
        assert_eq!(
            without_rowid,
            name == "session_fact",
            "{name} WITHOUT ROWID"
        );
    }
    let indexes: BTreeSet<String> = conn
        .prepare(
            "SELECT m.tbl_name || '(' || group_concat(i.name, ', ') || ')' || (CASE l.partial WHEN 1 THEN ' partial' ELSE '' END)
             FROM sqlite_master m
             JOIN pragma_index_list(m.tbl_name) l ON l.name = m.name
             JOIN pragma_index_info(m.name) i
             WHERE m.type = 'index' AND m.sql IS NOT NULL
             GROUP BY m.name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let ad_40: BTreeSet<String> = [
        "session(subject_id)",
        "session(project) partial",
        "subject(project, status)",
        "confirmation(proposed_source, confirmed_at_ms)",
        "confirmation(subject_id)",
    ]
    .map(str::to_owned)
    .into();
    assert_eq!(indexes, ad_40);
    let partial: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND sql LIKE '%WHERE%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(partial.ends_with("WHERE subject_id IS NULL"), "{partial}");
}

/// AD-40: each `CHECK` list equals its enum's `ALL`, so a new variant fails
/// here until a migration widens the check.
#[test]
fn check_lists_match_the_core_enums() {
    let found = columns(&fresh());
    let expected = expected_checks();
    for (column, values) in &expected {
        let declared = found
            .get(*column)
            .unwrap_or_else(|| panic!("{column} is missing"));
        assert_eq!(declared.check.as_ref(), Some(values), "{column}");
    }
    for (column, declared) in &found {
        if declared.check.is_some() {
            assert!(
                expected.contains_key(column.as_str()),
                "{column} has a CHECK D-88 does not name"
            );
        }
    }
}

/// D-88: without `CHECK` are the fact name (a new fact needs no
/// migration), Claude Code's end reason (its values may grow), and the
/// identifier, project, name and text columns; every other text column
/// holds an enum and has its list.
#[test]
fn columns_without_check_are_the_ones_d_88_names() {
    let identifiers = [
        "event_file.name",
        "session.session_id",
        "session_fact.session_id",
        "session_excerpt.session_id",
        "confirmation.session_id",
        "model_answer.session_id",
    ];
    let named_by_d_88 = [
        "session_fact.fact",
        "confirmation_input.end_reason",
        "session.project",
        "subject.project",
        "subject.name",
    ];
    let text = [
        "session_fact.value",
        "session_excerpt.prompt",
        "session_excerpt.answer_head",
        "session_excerpt.answer_tail",
        "confirmation_input.prompt",
        "confirmation_input.answer_head",
        "confirmation_input.answer_tail",
        "model_answer.reason",
        "setting.value",
    ];
    let allowed: BTreeSet<&str> = identifiers
        .into_iter()
        .chain(named_by_d_88)
        .chain(text)
        .collect();
    let found = columns(&fresh());
    let unchecked_text: BTreeSet<&str> = found
        .iter()
        .filter(|(_, c)| c.check.is_none() && c.declared_type != "INTEGER")
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(unchecked_text, allowed);
    let flags_or_enums = expected_checks();
    for (name, column) in &found {
        if column.check.is_none() {
            assert!(!flags_or_enums.contains_key(name.as_str()), "{name}");
        }
    }
}

#[test]
fn migration_hashes_are_pinned() {
    let hashes: Vec<String> = MIGRATIONS
        .iter()
        .map(|sql| officina_core::sha256_hex(sql.as_bytes()))
        .collect();
    assert_eq!(
        hashes, PINNED,
        "a shipped migration changed, or a new one is not pinned"
    );
    assert_eq!(SCHEMA_VERSION, PINNED.len() as i64);
}

fn user_version(db: &Path) -> i64 {
    read_only_connection(db)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// AD-41: the failed entry's transaction rolls back its tables and
/// `user_version` together.
#[test]
fn failed_migration_leaves_the_version_unchanged() {
    let home = Home::new();
    drop(home.writer(NOW));
    let broken = [
        MIGRATIONS[0],
        "CREATE TABLE added (x INTEGER) STRICT; INSERT INTO no_such_table VALUES (1);",
    ];
    let opened = open_writer_with(&home.home, &[], &broken, NOW);
    assert!(
        matches!(opened, WriterOpen::Unavailable { .. }),
        "{opened:?}"
    );
    drop(opened);
    let db = home.paths().database();
    assert_eq!(user_version(&db), 1);
    let conn = read_only_connection(&db).unwrap();
    assert!(!tables(&conn).contains(&"added".to_owned()));

    let mut memory = fresh();
    assert!(migrate(&mut memory, &broken).is_err());
    assert_eq!(
        memory
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn migrating_a_database_with_data_copies_it_first() {
    let next = [MIGRATIONS[0], "CREATE TABLE added (x INTEGER) STRICT;"];

    let empty = Home::new();
    drop(empty.writer(NOW));
    match open_writer_with(&empty.home, &[], &next, NOW) {
        WriterOpen::Writable { integrity, .. } => {
            assert_eq!(
                integrity,
                Integrity::Migrated {
                    from: 1,
                    copy: None
                }
            );
        }
        other => panic!("{other:?}"),
    }

    let home = Home::new();
    let mut writer = home.writer(NOW);
    writer
        .set_setting(SettingKey::AutoArchive, "false", Source::Author, NOW)
        .unwrap();
    drop(writer);
    let copy = match open_writer_with(&home.home, &[], &next, NOW) {
        WriterOpen::Writable {
            writer,
            integrity:
                Integrity::Migrated {
                    from: 1,
                    copy: Some(copy),
                },
        } => {
            assert_eq!(writer.user_version().unwrap(), 2);
            copy
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(copy, home.paths().backup_dir().join("pre-v2-2026-10-27.db"));
    assert_eq!(user_version(&copy), 1, "the copy holds the version it left");
    let held: String = read_only_connection(&copy)
        .unwrap()
        .query_row("SELECT value FROM setting", [], |row| row.get(0))
        .unwrap();
    assert_eq!(held, "false");
    assert!(open_reader_expecting(&home.home, &[], 2).is_ok());
}

/// AD-58: from each past version, `tests/fixtures/registry/v<n>.sql` builds
/// a database the writer migrates to the current one. Holds trivially at
/// version 1; fails as soon as a migration lands without the fixture of
/// the version it leaves.
#[test]
fn every_past_version_has_a_fixture() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
    for version in (1..).take_while(|version| *version < SCHEMA_VERSION) {
        let fixture = fixtures.join(format!("v{version}.sql"));
        let sql = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("{}: {e}", fixture.display()));
        let home = Home::new();
        std::fs::create_dir_all(home.paths().dir()).unwrap();
        let conn = Connection::open(home.paths().database()).unwrap();
        conn.execute_batch(&sql).unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        drop(conn);
        let writer = writable(open_writer(&home.home, &[], NOW));
        assert_eq!(
            writer.user_version().unwrap(),
            SCHEMA_VERSION,
            "from v{version}"
        );
    }
}
