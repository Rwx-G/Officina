//! The schema and the migration runner (story 3.1 AC 4 and 5, story 5.6
//! AC 1 to 3; AD-40, AD-41, AD-82, D-88, D-133): STRICT tables, `CHECK`
//! lists equal to the core enums, pinned migrations, one transaction per
//! migration, a copy before migrating data, a fixture per past version, and
//! migration 2's shape.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::registry::{Home, NOW, writable};
use officina_core::registry::types::{
    AuthorDecision, AuthorReason, LaunchOutcome, ModelVerdict, OrchestratorStartOutcome,
    ProposalKind, ProposalSource, ProposalStatus, ReasonCode, ReviewMode, ReviewOutcome,
    SettingKey, Source, SubjectStatus, TriageOutcome, TriagePurpose, TriageRefusal, Verdict,
    WorkOrderSource,
};
use officina_core::relay::auto::ReviewDecision;
use officina_core::workshop::battery::BatteryOutcome;
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::{BuildOutcome, ImageKind};
use officina_core::workshop::review::{DiscardReason, PackageVerdict};
use officina_core::workshop::run::{RunState, Runner, WorkshopOutcome};
use officina_host::registry::schema::{MIGRATIONS, SCHEMA_VERSION, migrate};
use officina_host::registry::store::{
    Integrity, WriterOpen, open_reader_expecting, open_writer, open_writer_with,
    read_only_connection,
};
use rusqlite::Connection;
use rusqlite::types::Value;

/// The SHA-256 of each shipped migration, in order (AD-41). A shipped
/// entry never changes; a new entry fails here until its hash is pinned.
const PINNED: [&str; 3] = [
    "bd13f2dd8c256dad763a965aa411f772077a3b50cbd0aacfbeb1fbe418eefe7b",
    "fdf160193383b12b1d78d2e540bf509d5c801d7cad83521d882c3a6693fc16e1",
    "23bd1f196d088f571497a0b1da0fc26cf9e62a5b6cda9969f1eb61ea0959f6ad",
];

/// The tables the workshop migration creates (AD-119, D-212), and those of
/// them keyed WITHOUT ROWID.
const WORKSHOP_TABLES: [&str; 5] = [
    "workshop_run",
    "workshop_approval",
    "workshop_image",
    "workshop_inventory",
    "workshop_battery",
];
const WORKSHOP_WITHOUT_ROWID: [&str; 2] = ["workshop_approval", "workshop_inventory"];

/// The tables migration 2 creates (AD-82, D-133).
const V2_TABLES: [&str; 4] = [
    "work_order",
    "proposal",
    "orchestrator_start",
    "auto_review",
];

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

/// D-88 and D-133: which enum each enum-valued column takes, and the
/// 0-or-1 columns.
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
        (
            "triage_call.refusal",
            codes(TriageRefusal::ALL, TriageRefusal::code),
        ),
        ("setting.key", codes(SettingKey::ALL, SettingKey::code)),
        ("subject.routine", flag.clone()),
        ("confirmation.auto", flag.clone()),
        ("confirmation_input.cut_mid_turn", flag.clone()),
        (
            "work_order.source",
            codes(WorkOrderSource::ALL, WorkOrderSource::code),
        ),
        (
            "work_order.outcome",
            codes(LaunchOutcome::ALL, LaunchOutcome::code),
        ),
        (
            "proposal.kind",
            codes(ProposalKind::ALL, ProposalKind::code),
        ),
        (
            "proposal.status",
            codes(ProposalStatus::ALL, ProposalStatus::code),
        ),
        (
            "orchestrator_start.outcome",
            codes(
                OrchestratorStartOutcome::ALL,
                OrchestratorStartOutcome::code,
            ),
        ),
        ("auto_review.mode", codes(ReviewMode::ALL, ReviewMode::code)),
        (
            "auto_review.outcome",
            codes(ReviewOutcome::ALL, ReviewOutcome::code),
        ),
        (
            "auto_review.decision",
            codes(ReviewDecision::ALL, ReviewDecision::code),
        ),
        (
            "auto_review.author_decision",
            codes(AuthorDecision::ALL, AuthorDecision::code),
        ),
        (
            "auto_review.author_reason",
            codes(AuthorReason::ALL, AuthorReason::code),
        ),
        ("auto_review.rule_baseline", flag),
        ("work_order.runner", codes(Runner::ALL, Runner::code)),
        ("workshop_run.state", codes(RunState::ALL, RunState::code)),
        (
            "workshop_run.outcome",
            codes(WorkshopOutcome::ALL, WorkshopOutcome::code),
        ),
        (
            "workshop_run.credential_kind",
            codes(CredentialKind::ALL, CredentialKind::code),
        ),
        (
            "workshop_run.verdict",
            codes(PackageVerdict::ALL, PackageVerdict::code),
        ),
        (
            "workshop_run.discard_reason",
            codes(DiscardReason::ALL, DiscardReason::code),
        ),
        (
            "workshop_image.kind",
            codes(ImageKind::ALL, ImageKind::code),
        ),
        (
            "workshop_image.outcome",
            codes(BuildOutcome::ALL, BuildOutcome::code),
        ),
        (
            "workshop_battery.outcome",
            codes(BatteryOutcome::ALL, BatteryOutcome::code),
        ),
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
    for table in V2_TABLES {
        assert!(listed.iter().any(|(name, _, _)| name == table), "{table}");
    }
    for table in WORKSHOP_TABLES {
        assert!(listed.iter().any(|(name, _, _)| name == table), "{table}");
    }
    for (name, strict, without_rowid) in listed {
        assert!(strict, "{name} is not STRICT");
        assert_eq!(
            without_rowid,
            name == "session_fact" || WORKSHOP_WITHOUT_ROWID.contains(&name.as_str()),
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
    let ad_40_and_d_133: BTreeSet<String> = [
        "session(subject_id)",
        "session(project) partial",
        "subject(project, status)",
        "confirmation(proposed_source, confirmed_at_ms)",
        "confirmation(subject_id)",
        "work_order(launched_at_ms)",
        "proposal(status, received_at_ms)",
        "auto_review(project, action, requested_at_ms)",
        "auto_review(requested_at_ms)",
        "workshop_run(state)",
        "workshop_run(finished_at_ms)",
        "workshop_run(decided_at_ms)",
        "workshop_battery(image_id, finished_at_ms)",
    ]
    .map(str::to_owned)
    .into();
    assert_eq!(indexes, ad_40_and_d_133);
    let partial: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND sql LIKE '%WHERE%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(partial.ends_with("WHERE subject_id IS NULL"), "{partial}");
}

/// AD-40, AD-82: each `CHECK` list equals its enum's `ALL`, so a new
/// variant fails here until a migration widens the check.
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
                "{column} has a CHECK neither D-88 nor D-133 names"
            );
        }
    }
}

/// D-88 and D-133: without `CHECK` are the fact name (a new fact needs no
/// migration), Claude Code's end reason (its values may grow), a review's
/// cause and a proposal's invalid reason (their enums grow with stories
/// 5.13 and 5.17), and the identifier, hash, JSON, project, name and text
/// columns; every other text column holds an enum and has its list.
#[test]
fn columns_without_check_are_the_ones_d_88_names() {
    let identifiers = [
        "event_file.name",
        "session.session_id",
        "session_fact.session_id",
        "session_excerpt.session_id",
        "confirmation.session_id",
        "model_answer.session_id",
        "triage_call.case_key",
        "triage_call.input_hash",
        "work_order.id",
        "work_order.proposal_id",
        "work_order.bg_id",
        "work_order.session_id",
        "work_order.prompt_sha256",
        "work_order.catalogue_hash",
        "proposal.id",
        "proposal.orchestrator_session_id",
        "proposal.target_session_id",
        "proposal.work_order_id",
        "auto_review.approval_id",
        "auto_review.request_key",
        "auto_review.run_id",
        "workshop_run.id",
        "workshop_run.image_id",
        "workshop_run.gateway_image_id",
        "workshop_run.base_commit",
        "workshop_run.parent_run_id",
        "workshop_run.plan_sha256",
        "workshop_run.battery_id",
        "workshop_run.package_sha256",
        "workshop_run.tip_commit",
        "workshop_approval.run_id",
        "workshop_approval.group_key",
        "workshop_approval.hunks_sha256",
        "workshop_image.name",
        "workshop_image.image_id",
        "workshop_image.programs_sha256",
        "workshop_inventory.image",
        "workshop_inventory.image_id",
        "workshop_inventory.inventory_key",
        "workshop_battery.id",
        "workshop_battery.image",
        "workshop_battery.image_id",
        "workshop_battery.gateway_image_id",
        "workshop_battery.engine_key",
        "workshop_battery.officina_version",
        "workshop_battery.vector_sha256",
        "workshop_battery.programs_sha256",
    ];
    let named_by_d_88_and_d_133 = [
        "session_fact.fact",
        "confirmation_input.end_reason",
        "session.project",
        "subject.project",
        "subject.name",
        "auto_review.cause",
        "proposal.invalid_reason",
        "work_order.template",
        "work_order.project",
        "work_order.cwd",
        "work_order.name",
        "work_order.new_subject_name",
        "proposal.template",
        "proposal.project",
        "auto_review.project",
        "auto_review.action",
        // The causes grow with their stories (D-133, D-212); branches,
        // images and projects are names.
        "workshop_run.cause",
        "workshop_run.quarantine_cause",
        "workshop_battery.cause",
        "workshop_run.image",
        "workshop_run.base_branch",
        "workshop_run.branch",
        "workshop_image.project",
        "workshop_inventory.project",
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
        "work_order.values_json",
        "work_order.argv_json",
        "work_order.error",
        "work_order.printed",
        "proposal.values_json",
        "proposal.subject_choice_json",
        "proposal.texts_json",
        "orchestrator_start.error",
        "auto_review.reason",
        "workshop_run.plan_json",
        "workshop_run.facts_json",
        "workshop_run.texts_json",
        "workshop_run.groups_json",
        "workshop_image.labels_json",
        "workshop_image.env_names_json",
        "workshop_image.build_tail",
        "workshop_inventory.inventory_json",
    ];
    let allowed: BTreeSet<&str> = identifiers
        .into_iter()
        .chain(named_by_d_88_and_d_133)
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

/// This build's migrations followed by `extra`, a version past them.
fn then(extra: &'static str) -> Vec<&'static str> {
    MIGRATIONS.iter().copied().chain([extra]).collect()
}

/// AD-41: the failed entry's transaction rolls back its tables and
/// `user_version` together.
#[test]
fn failed_migration_leaves_the_version_unchanged() {
    let home = Home::new();
    drop(home.writer(NOW));
    let broken =
        then("CREATE TABLE added (x INTEGER) STRICT; INSERT INTO no_such_table VALUES (1);");
    let opened = open_writer_with(&home.home, &[], &broken, NOW);
    assert!(
        matches!(opened, WriterOpen::Unavailable { .. }),
        "{opened:?}"
    );
    drop(opened);
    let db = home.paths().database();
    assert_eq!(user_version(&db), SCHEMA_VERSION);
    let conn = read_only_connection(&db).unwrap();
    assert!(!tables(&conn).contains(&"added".to_owned()));

    let mut memory = fresh();
    assert!(migrate(&mut memory, &broken).is_err());
    assert_eq!(
        memory
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
}

#[test]
fn migrating_a_database_with_data_copies_it_first() {
    let next = then("CREATE TABLE added (x INTEGER) STRICT;");
    let target = SCHEMA_VERSION + 1;

    let empty = Home::new();
    drop(empty.writer(NOW));
    match open_writer_with(&empty.home, &[], &next, NOW) {
        WriterOpen::Writable { integrity, .. } => {
            assert_eq!(
                integrity,
                Integrity::Migrated {
                    from: SCHEMA_VERSION,
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
                    from: SCHEMA_VERSION,
                    copy: Some(copy),
                },
        } => {
            assert_eq!(writer.user_version().unwrap(), target);
            copy
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(
        copy,
        home.paths()
            .backup_dir()
            .join(format!("pre-v{target}-2026-10-27.db"))
    );
    assert_eq!(
        user_version(&copy),
        SCHEMA_VERSION,
        "the copy holds the version it left"
    );
    let held: String = read_only_connection(&copy)
        .unwrap()
        .query_row("SELECT value FROM setting", [], |row| row.get(0))
        .unwrap();
    assert_eq!(held, "false");
    assert!(open_reader_expecting(&home.home, &[], target).is_ok());
}

/// The registry of a home at schema `version`, holding the rows of
/// `tests/fixtures/registry/v<version>.sql`: the migrations up to that
/// version run first, so the fixture holds rows only and never a second
/// copy of a migration.
fn home_at(version: i64) -> Home {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/registry")
        .join(format!("v{version}.sql"));
    let sql =
        std::fs::read_to_string(&fixture).unwrap_or_else(|e| panic!("{}: {e}", fixture.display()));
    let home = Home::new();
    std::fs::create_dir_all(home.paths().dir()).unwrap();
    let mut conn = Connection::open(home.paths().database()).unwrap();
    let up_to = usize::try_from(version).unwrap();
    assert_eq!(migrate(&mut conn, &MIGRATIONS[..up_to]).unwrap(), version);
    conn.execute_batch(&sql).unwrap();
    home
}

/// AD-58: from each past version, `tests/fixtures/registry/v<n>.sql` fills
/// a database the writer migrates to the current one. Fails as soon as a
/// migration lands without the fixture of the version it leaves.
#[test]
fn every_past_version_has_a_fixture() {
    for version in (1..).take_while(|version| *version < SCHEMA_VERSION) {
        let home = home_at(version);
        let writer = writable(open_writer(&home.home, &[], NOW));
        assert_eq!(
            writer.user_version().unwrap(),
            SCHEMA_VERSION,
            "from v{version}"
        );
    }
}

/// Every row of `table`, each column as SQLite stores it, ordered by every
/// column (a `WITHOUT ROWID` table has no rowid to order by).
fn rows_of(conn: &Connection, table: &str) -> Vec<Vec<Value>> {
    let width = conn
        .prepare(&format!("SELECT * FROM {table}"))
        .unwrap()
        .column_count();
    let order: Vec<String> = (1..=width).map(|at| at.to_string()).collect();
    let mut statement = conn
        .prepare(&format!(
            "SELECT * FROM {table} ORDER BY {}",
            order.join(", ")
        ))
        .unwrap();
    statement
        .query_map([], |row| {
            (0..width).map(|at| row.get::<_, Value>(at)).collect()
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// AD-82, D-133: migration 2 widens `setting.key` by rebuilding the table
/// in its transaction, and every row comes through as stored, its value's
/// type included; the new keys are then accepted.
#[test]
fn the_setting_rebuild_keeps_every_row() {
    let home = home_at(1);
    let before = rows_of(
        &read_only_connection(&home.paths().database()).unwrap(),
        "setting",
    );
    assert!(
        before.iter().any(|row| matches!(row[1], Value::Integer(_)))
            && before.iter().any(|row| matches!(row[1], Value::Text(_))),
        "positive control: an integer and a text value"
    );
    let v1_keys = before.len();
    drop(writable(open_writer(&home.home, &[], NOW)));
    let conn = read_only_connection(&home.paths().database()).unwrap();
    assert_eq!(rows_of(&conn, "setting"), before);
    drop(conn);
    let mut writer = home.writer(NOW);
    for key in SettingKey::ALL.iter().copied() {
        writer.set_setting(key, "1", Source::Author, NOW).unwrap();
    }
    assert_eq!(writer.settings().unwrap().len(), SettingKey::ALL.len());
    assert!(SettingKey::ALL.len() > v1_keys, "the check widened");
}

/// AD-41, AD-82, story 5.6 AC 1: a version 1 registry with rows migrates
/// to version 2 after a copy, and every row of schema 1 is still there.
#[test]
fn version_1_with_data_migrates_to_the_current_version() {
    let home = home_at(1);
    let v1_tables: Vec<(String, Vec<Vec<Value>>)> = {
        let conn = read_only_connection(&home.paths().database()).unwrap();
        tables(&conn)
            .into_iter()
            .map(|table| {
                let rows = rows_of(&conn, &table);
                assert!(!rows.is_empty(), "the fixture fills {table}");
                (table, rows)
            })
            .collect()
    };
    let copy = match open_writer(&home.home, &[], NOW) {
        WriterOpen::Writable {
            writer,
            integrity:
                Integrity::Migrated {
                    from: 1,
                    copy: Some(copy),
                },
        } => {
            assert_eq!(writer.user_version().unwrap(), SCHEMA_VERSION);
            assert!(writer.work_orders().unwrap().is_empty());
            copy
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(
        copy,
        home.paths()
            .backup_dir()
            .join(format!("pre-v{SCHEMA_VERSION}-2026-10-27.db"))
    );
    assert_eq!(user_version(&copy), 1, "the copy holds the version it left");
    let conn = read_only_connection(&home.paths().database()).unwrap();
    for (table, rows) in &v1_tables {
        assert_eq!(&rows_of(&conn, table), rows, "{table}");
    }
    let quick: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(quick, "ok");
    let violations: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}

/// The columns of `table` declared `NOT NULL`, and its primary key's.
fn shape(conn: &Connection, table: &str) -> (BTreeSet<String>, Vec<String>) {
    let columns: Vec<(String, bool, i64)> = conn
        .prepare(&format!(
            "SELECT name, \"notnull\", pk FROM pragma_table_info('{table}')"
        ))
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let not_null = columns
        .iter()
        .filter(|(_, not_null, _)| *not_null)
        .map(|(name, _, _)| name.clone())
        .collect();
    let key = columns
        .iter()
        .filter(|(_, _, pk)| *pk > 0)
        .map(|(name, _, _)| name.clone())
        .collect();
    (not_null, key)
}

/// Story 5.6 AC 3 (D-133, review F2, F3): each new table's key, its type,
/// and its `NOT NULL` columns; every other column is nullable.
#[test]
fn not_null_columns_are_the_ones_the_story_names() {
    let conn = fresh();
    let stated: [(&str, &str, &[&str]); 4] = [
        (
            "work_order",
            "TEXT",
            &[
                "seq",
                "template",
                "project",
                "cwd",
                "values_json",
                "argv_json",
                "prompt_sha256",
                "catalogue_hash",
                "name",
                "source",
                "launched_at_ms",
                "outcome",
                "runner",
            ],
        ),
        ("proposal", "TEXT", &["status", "received_at_ms"]),
        (
            "orchestrator_start",
            "INTEGER",
            &["started_at_ms", "outcome"],
        ),
        (
            "auto_review",
            "INTEGER",
            &[
                "approval_id",
                "request_key",
                "project",
                "action",
                "requested_at_ms",
                "mode",
                "outcome",
                "rule_baseline",
            ],
        ),
    ];
    assert_eq!(stated.map(|(table, _, _)| table), V2_TABLES);
    let found = columns(&conn);
    for (table, key_type, not_null) in stated {
        let (declared, key) = shape(&conn, table);
        assert_eq!(key, ["id"], "{table}");
        assert_eq!(
            found[&format!("{table}.id")].declared_type,
            key_type,
            "{table}"
        );
        // SQLite lets a key that is not an integer hold NULL unless it is
        // declared `NOT NULL`; an integer key is the rowid, never null.
        let text_key = (key_type == "TEXT").then_some("id");
        let expected: BTreeSet<String> = not_null
            .iter()
            .copied()
            .chain(text_key)
            .map(str::to_owned)
            .collect();
        assert_eq!(declared, expected, "{table}");
    }
}

/// D-133: retention deletes `session` and `subject` rows on their own
/// schedule under `foreign_keys = ON`, so no table of migration 2 points
/// at them, or at anything.
#[test]
fn no_foreign_key_points_at_session_or_subject() {
    let conn = fresh();
    for table in V2_TABLES {
        let keys: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(keys, 0, "{table}");
    }
    let pointing: Vec<String> = tables(&conn)
        .into_iter()
        .filter(|table| {
            conn.query_row(
                &format!(
                    "SELECT count(*) FROM pragma_foreign_key_list('{table}')
                     WHERE \"table\" IN ('session', 'subject')"
                ),
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
                > 0
        })
        .collect();
    assert!(
        pointing
            .iter()
            .all(|table| !V2_TABLES.contains(&table.as_str())),
        "{pointing:?}"
    );
}

/// D-133: the indexes the counting queries read, each by its columns in
/// order.
#[test]
fn the_counting_indexes_exist() {
    let conn = fresh();
    let indexed = |table: &str| -> Vec<Vec<String>> {
        let names: Vec<String> = conn
            .prepare(&format!(
                "SELECT name FROM pragma_index_list('{table}') WHERE origin = 'c'"
            ))
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        names
            .iter()
            .map(|index| {
                conn.prepare(&format!(
                    "SELECT name FROM pragma_index_info('{index}') ORDER BY seqno"
                ))
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
            })
            .collect()
    };
    for (table, columns) in [
        ("auto_review", &["project", "action", "requested_at_ms"][..]),
        ("auto_review", &["requested_at_ms"][..]),
        ("proposal", &["status", "received_at_ms"][..]),
        ("work_order", &["launched_at_ms"][..]),
    ] {
        assert!(
            indexed(table).iter().any(|index| index == columns),
            "{table}{columns:?}"
        );
    }
}

fn insert_order(
    conn: &Connection,
    id: &str,
    template: &str,
    seq: i64,
    name: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO work_order (id, seq, template, project, cwd, values_json, argv_json,
             prompt_sha256, catalogue_hash, name, source, launched_at_ms, outcome)
         VALUES (?1, ?2, ?3, 'api', 'C:/work/api', '{}', '[]', 'h', 'c', ?4, 'composer', 1, 'launching')",
        rusqlite::params![id, seq, template, name],
    )
}

fn is_unique_failure(result: rusqlite::Result<usize>) -> bool {
    matches!(
        result,
        Err(rusqlite::Error::SqliteFailure(failure, _))
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

/// D-133, review F2: `name` is unique.
#[test]
fn work_order_names_are_unique() {
    let conn = fresh();
    insert_order(&conn, "a", "bump", 1, "bump-1").unwrap();
    assert!(is_unique_failure(insert_order(
        &conn, "b", "other", 9, "bump-1"
    )));
    insert_order(&conn, "c", "bump", 2, "bump-2").unwrap();
}

/// D-133, review F2: `(template, seq)` is unique.
#[test]
fn work_order_template_and_seq_are_unique() {
    let conn = fresh();
    insert_order(&conn, "a", "bump", 1, "bump-1").unwrap();
    assert!(is_unique_failure(insert_order(
        &conn, "b", "bump", 1, "renamed"
    )));
    insert_order(&conn, "c", "other", 1, "other-1").unwrap();
}

/// D-133: a proposal file that does not parse has no kind and its row is
/// stored; every kind of `ProposalKind` is accepted, any other text not.
#[test]
fn a_proposal_without_kind_is_accepted() {
    let conn = fresh();
    conn.execute(
        "INSERT INTO proposal (id, kind, status, received_at_ms) VALUES ('p1', NULL, 'invalid', 1)",
        [],
    )
    .unwrap();
    for kind in ProposalKind::ALL {
        conn.execute(
            "INSERT INTO proposal (id, kind, status, received_at_ms) VALUES (?1, ?2, 'pending', 1)",
            [format!("p-{}", kind.code()), kind.code().to_owned()],
        )
        .unwrap();
    }
    assert!(
        conn.execute(
            "INSERT INTO proposal (id, kind, status, received_at_ms) VALUES ('p2', 'chat', 'pending', 1)",
            [],
        )
        .is_err()
    );
}

/// D-144, review F3: a start keeps its pid's creation time, read back as
/// written; a refused start has neither.
#[test]
fn orchestrator_start_keeps_the_pid_creation_time() {
    let conn = fresh();
    conn.execute(
        "INSERT INTO orchestrator_start (started_at_ms, pid, pid_created_at_ms, outcome)
         VALUES (?1, 4242, ?2, 'spawned')",
        [NOW, NOW - 150],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO orchestrator_start (started_at_ms, outcome, error) VALUES (?1, 'refused', 'busy')",
        [NOW + 1],
    )
    .unwrap();
    let kept: (i64, i64) = conn
        .query_row(
            "SELECT pid, pid_created_at_ms FROM orchestrator_start WHERE outcome = 'spawned'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kept, (4242, NOW - 150));
}

// ---- story 6.4: the workshop migration (AD-119, D-212) ----

/// The workshop migration's version: the entry of [`MIGRATIONS`] that
/// creates `workshop_run` (P-53), so its number and the backup's name
/// derive from the list, never from a literal.
fn workshop_version() -> i64 {
    let index = MIGRATIONS
        .iter()
        .position(|sql| sql.contains("CREATE TABLE workshop_run"))
        .expect("a migration creates workshop_run");
    i64::try_from(index).unwrap() + 1
}

#[test]
fn the_workshop_migration_is_pinned() {
    let index = usize::try_from(workshop_version() - 1).unwrap();
    assert_eq!(
        officina_core::sha256_hex(MIGRATIONS[index].as_bytes()),
        PINNED[index],
        "the workshop migration's text changed: a new variant or column is a later migration (D-133)"
    );
}

#[test]
fn the_workshop_migration_backs_up_first() {
    let version = workshop_version();
    let home = home_at(version - 1);
    let copy = match open_writer(&home.home, &[], NOW) {
        WriterOpen::Writable {
            integrity:
                Integrity::Migrated {
                    from,
                    copy: Some(copy),
                },
            ..
        } => {
            assert_eq!(from, version - 1);
            copy
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(
        copy,
        home.paths()
            .backup_dir()
            .join(format!("pre-v{version}-2026-10-27.db"))
    );
    assert_eq!(user_version(&copy), version - 1);
    let conn = read_only_connection(&copy).unwrap();
    assert!(!tables(&conn).contains(&"workshop_run".to_owned()));
}

#[test]
fn a_previous_version_registry_migrates_with_its_work_orders_as_host() {
    let version = workshop_version();
    let home = home_at(version - 1);
    let before: Vec<Vec<Value>> = {
        let conn = read_only_connection(&home.paths().database()).unwrap();
        rows_of(&conn, "work_order")
    };
    assert!(!before.is_empty(), "the fixture holds work orders");
    drop(writable(open_writer(&home.home, &[], NOW)));
    let conn = read_only_connection(&home.paths().database()).unwrap();
    let after = rows_of(&conn, "work_order");
    assert_eq!(after.len(), before.len());
    let runner = Value::Text(Runner::Host.code().to_owned());
    for (old, new) in before.iter().zip(&after) {
        // The rebuilt table puts `runner` after the columns it kept.
        assert_eq!(
            &new[..old.len()],
            &old[..],
            "every column of every row is kept"
        );
        assert_eq!(&new[old.len()..], std::slice::from_ref(&runner));
    }
    let index: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'work_order_by_launch'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index, 1, "the rebuilt table keeps its index");
}

/// A valid row of each workshop table holding an enum, every enum column at
/// its enum's first value.
fn valid_rows() -> Vec<(&'static str, Vec<(&'static str, Value)>)> {
    let text = |s: &str| Value::Text(s.to_owned());
    let int = Value::Integer;
    vec![
        (
            "workshop_run",
            vec![
                ("id", text("0123456789abcdef0123456789abcdef")),
                ("image", text("officina-workshop:local")),
                ("image_id", text("sha256:aa")),
                ("gateway_image_id", text("sha256:bb")),
                ("base_branch", text("main")),
                ("base_commit", text("cc")),
                ("branch", text("officina/bump-1")),
                ("uncommitted", int(0)),
                ("plan_json", text("{}")),
                ("plan_sha256", text("dd")),
                ("credential_kind", text(CredentialKind::ALL[0].code())),
                ("deadline_ms", int(NOW)),
                ("battery_id", text("ee")),
                ("state", text(RunState::ALL[0].code())),
                ("state_at_ms", int(NOW)),
                ("started_at_ms", int(NOW)),
                ("outcome", text(WorkshopOutcome::ALL[0].code())),
                ("verdict", text(PackageVerdict::ALL[0].code())),
                ("discard_reason", text(DiscardReason::ALL[0].code())),
                ("cause", text("exited")),
                ("quarantine_cause", text("credential_found")),
            ],
        ),
        (
            "workshop_image",
            vec![
                ("name", text("officina-workshop:local")),
                ("kind", text(ImageKind::ALL[0].code())),
                ("built_at_ms", int(NOW)),
                ("outcome", text(BuildOutcome::ALL[0].code())),
            ],
        ),
        (
            "workshop_battery",
            vec![
                ("id", text("ff")),
                ("image", text("officina-workshop:local")),
                ("image_id", text("sha256:aa")),
                ("gateway_image_id", text("sha256:bb")),
                ("engine_key", text("key")),
                ("officina_version", text("0.1.0")),
                ("vector_sha256", text("11")),
                ("programs_sha256", text("22")),
                ("started_at_ms", int(NOW)),
                ("outcome", text(BatteryOutcome::ALL[0].code())),
                ("cause", text("engine_lost")),
            ],
        ),
    ]
}

fn insert(conn: &Connection, table: &str, row: &[(&str, Value)]) -> rusqlite::Result<usize> {
    let names: Vec<&str> = row.iter().map(|(n, _)| *n).collect();
    let marks: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
    let values: Vec<&dyn rusqlite::ToSql> =
        row.iter().map(|(_, v)| v as &dyn rusqlite::ToSql).collect();
    conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            names.join(", "),
            marks.join(", ")
        ),
        values.as_slice(),
    )
}

/// `row` with `column` set to `value`.
fn with(row: &[(&'static str, Value)], column: &str, value: &str) -> Vec<(&'static str, Value)> {
    row.iter()
        .map(|(n, v)| {
            let v = if *n == column {
                Value::Text(value.to_owned())
            } else {
                v.clone()
            };
            (*n, v)
        })
        .collect()
}

#[test]
fn every_enum_column_refuses_an_unknown_value() {
    let checks = expected_checks();
    for (table, row) in valid_rows() {
        insert(&fresh(), table, &row).unwrap_or_else(|e| panic!("{table}: {e}"));
        for (column, _) in &row {
            if checks.contains_key(format!("{table}.{column}").as_str()) {
                let bad = with(&row, column, "not_a_variant");
                assert!(insert(&fresh(), table, &bad).is_err(), "{table}.{column}");
            }
        }
    }
    let rows = valid_rows();
    for name in checks.keys().filter(|n| n.starts_with("workshop_")) {
        let (table, column) = name.split_once('.').unwrap();
        assert!(
            rows.iter()
                .any(|(t, row)| *t == table && row.iter().any(|(c, _)| *c == column)),
            "{name} has no case"
        );
    }
}

#[test]
fn cause_columns_accept_any_text() {
    for (table, row) in valid_rows() {
        for cause in ["cause", "quarantine_cause"] {
            if row.iter().any(|(c, _)| *c == cause) {
                let later = with(&row, cause, "a_cause_of_a_later_build");
                insert(&fresh(), table, &later).unwrap_or_else(|e| panic!("{table}.{cause}: {e}"));
            }
        }
    }
}

#[test]
fn a_battery_row_needs_its_vector_and_programs_digests() {
    let (_, row) = valid_rows()
        .into_iter()
        .find(|(t, _)| *t == "workshop_battery")
        .unwrap();
    for needed in ["vector_sha256", "programs_sha256"] {
        let without: Vec<(&str, Value)> =
            row.iter().filter(|(n, _)| *n != needed).cloned().collect();
        assert!(
            insert(&fresh(), "workshop_battery", &without).is_err(),
            "{needed}"
        );
    }
}
