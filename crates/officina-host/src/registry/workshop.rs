//! The workshop's records (AD-119, D-212): runs with their `work_order`
//! row, approvals, verdicts, images, inventories and batteries. Readers
//! take a connection, so the window and the read-only command line share
//! them; writers compile only with the `window-authority` feature.

use officina_core::untrusted::Untrusted;
use officina_core::workshop::battery::BatteryOutcome;
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::{BuildOutcome, ImageKind};
use officina_core::workshop::review::{DiscardReason, PackageVerdict};
use officina_core::workshop::run::{
    FinishCause, Recorded, RunAge, RunFacts, RunState, RunTexts, WorkshopOutcome,
};
use rusqlite::{Connection, OptionalExtension, params};

/// One run as the registry holds it, its texts untrusted.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRecord {
    pub id: String,
    pub template: String,
    pub project: String,
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub gateway_image_id: String,
    pub base_branch: String,
    pub base_commit: String,
    pub parent_run_id: Option<String>,
    pub branch: String,
    pub uncommitted: u32,
    pub plan_sha256: String,
    pub credential_kind: CredentialKind,
    pub deadline_ms: i64,
    pub battery_id: String,
    pub state: RunState,
    pub state_at_ms: i64,
    /// How the run ended; a cause this build does not know reads
    /// [`Recorded::Unknown`], never dropped (D-133).
    pub cause: Option<Recorded<FinishCause>>,
    pub outcome: Option<WorkshopOutcome>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub exit_code: Option<i64>,
    pub facts: RunFacts,
    /// `None` once cleared (AD-124) or never written.
    pub texts: Option<RunTexts>,
    pub package_sha256: Option<String>,
    pub tip_commit: Option<String>,
    pub commits_past_base: Option<u32>,
    pub files_changed: Option<u32>,
    pub groups_json: Option<String>,
    pub quarantine_cause: Option<String>,
    pub opened_at_ms: Option<i64>,
    pub verdict: Option<PackageVerdict>,
    pub discard_reason: Option<DiscardReason>,
    pub decided_at_ms: Option<i64>,
    pub stopped_at_ms: Option<i64>,
    pub removed_at_ms: Option<i64>,
    /// The project's folder the launch ran from (its `work_order` row).
    pub project_dir: String,
    /// The run plan the launch wrote (AD-102), Officina's own JSON.
    pub plan_json: String,
}

const RUN_COLUMNS: &str = "r.id, w.template, w.project, w.name, r.image, r.image_id,
    r.gateway_image_id, r.base_branch, r.base_commit, r.parent_run_id, r.branch, r.uncommitted,
    r.plan_sha256, r.credential_kind, r.deadline_ms, r.battery_id, r.state, r.state_at_ms,
    r.cause, r.outcome, r.started_at_ms, r.finished_at_ms, r.exit_code, r.facts_json,
    r.texts_json, r.package_sha256, r.tip_commit, r.commits_past_base, r.files_changed,
    r.groups_json, r.quarantine_cause, r.opened_at_ms, r.verdict, r.discard_reason,
    r.decided_at_ms, r.stopped_at_ms, r.removed_at_ms, w.cwd, r.plan_json";

const RUN_FROM: &str = "FROM workshop_run r JOIN work_order w ON w.id = r.id";

/// A column holding an enum this build does not parse: the row is
/// unreadable, as a `CHECK` makes it impossible.
fn bad_enum(index: usize, code: String) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(index, code, rusqlite::types::Type::Text)
}

fn required<T>(index: usize, code: String, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    parse(&code).ok_or_else(|| bad_enum(index, code))
}

fn optional<T>(
    row: &rusqlite::Row<'_>,
    index: usize,
    parse: fn(&str) -> Option<T>,
) -> rusqlite::Result<Option<T>> {
    row.get::<_, Option<String>>(index)?
        .map(|code| required(index, code, parse))
        .transpose()
}

fn count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u32>> {
    Ok(row
        .get::<_, Option<i64>>(index)?
        .and_then(|n| u32::try_from(n).ok()))
}

fn run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let facts: Option<String> = row.get(23)?;
    let texts: Option<String> = row.get(24)?;
    Ok(RunRecord {
        id: row.get(0)?,
        template: row.get(1)?,
        project: row.get(2)?,
        name: row.get(3)?,
        image: row.get(4)?,
        image_id: row.get(5)?,
        gateway_image_id: row.get(6)?,
        base_branch: row.get(7)?,
        base_commit: row.get(8)?,
        parent_run_id: row.get(9)?,
        branch: row.get(10)?,
        uncommitted: count(row, 11)?.unwrap_or(0),
        plan_sha256: row.get(12)?,
        credential_kind: required(13, row.get(13)?, CredentialKind::from_code)?,
        deadline_ms: row.get(14)?,
        battery_id: row.get(15)?,
        state: required(16, row.get(16)?, RunState::from_code)?,
        state_at_ms: row.get(17)?,
        cause: row
            .get::<_, Option<String>>(18)?
            .map(|code| Recorded::parse(&code, FinishCause::from_code)),
        outcome: optional(row, 19, WorkshopOutcome::from_code)?,
        started_at_ms: row.get(20)?,
        finished_at_ms: row.get(21)?,
        exit_code: row.get(22)?,
        facts: facts
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default(),
        texts: texts
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok()),
        package_sha256: row.get(25)?,
        tip_commit: row.get(26)?,
        commits_past_base: count(row, 27)?,
        files_changed: count(row, 28)?,
        groups_json: row.get(29)?,
        quarantine_cause: row.get(30)?,
        opened_at_ms: row.get(31)?,
        verdict: optional(row, 32, PackageVerdict::from_code)?,
        discard_reason: optional(row, 33, DiscardReason::from_code)?,
        decided_at_ms: row.get(34)?,
        stopped_at_ms: row.get(35)?,
        removed_at_ms: row.get(36)?,
        project_dir: row.get(37)?,
        plan_json: row.get(38)?,
    })
}

/// The run `id`, if the registry holds it.
pub fn run(conn: &Connection, id: &str) -> rusqlite::Result<Option<RunRecord>> {
    conn.prepare_cached(&format!("SELECT {RUN_COLUMNS} {RUN_FROM} WHERE r.id = ?1"))?
        .query_row([id], run_row)
        .optional()
}

/// The runs in any of `states`, oldest launch first.
pub fn runs_in(conn: &Connection, states: &[RunState]) -> rusqlite::Result<Vec<RunRecord>> {
    let codes: Vec<String> = states.iter().map(|s| format!("'{}'", s.code())).collect();
    conn.prepare(&format!(
        "SELECT {RUN_COLUMNS} {RUN_FROM} WHERE r.state IN ({}) ORDER BY r.started_at_ms, r.id",
        codes.join(", ")
    ))?
    .query_map([], run_row)?
    .collect()
}

/// How many runs may hold a container now (AD-107's `workshop_full`).
pub fn live_count(conn: &Connection) -> rusqlite::Result<u32> {
    let live: Vec<RunState> = RunState::ALL
        .iter()
        .copied()
        .filter(|s| s.is_live())
        .collect();
    Ok(u32::try_from(runs_in(conn, &live)?.len()).unwrap_or(u32::MAX))
}

/// The packages awaiting a verdict, `review` and `quarantined` (AD-107's
/// `reviews_pending`).
pub fn awaiting_verdict(conn: &Connection) -> rusqlite::Result<Vec<RunRecord>> {
    let awaiting: Vec<RunState> = RunState::ALL
        .iter()
        .copied()
        .filter(|s| s.awaits_verdict())
        .collect();
    runs_in(conn, &awaiting)
}

/// The runs of `template` imported at or after `since_ms`, newest decision
/// first: the follow-up bases a card offers (AD-107).
pub fn imported_since(
    conn: &Connection,
    template: &str,
    since_ms: i64,
) -> rusqlite::Result<Vec<RunRecord>> {
    conn.prepare_cached(&format!(
        "SELECT {RUN_COLUMNS} {RUN_FROM}
         WHERE w.template = ?1 AND r.verdict = ?2 AND r.decided_at_ms >= ?3
         ORDER BY r.decided_at_ms DESC, r.id"
    ))?
    .query_map(
        params![template, PackageVerdict::Imported.code(), since_ms],
        run_row,
    )?
    .collect()
}

/// What retention reads of every run (AD-124).
pub fn run_ages(conn: &Connection) -> rusqlite::Result<Vec<RunAge>> {
    conn.prepare_cached(
        "SELECT id, started_at_ms, texts_json IS NOT NULL, state FROM workshop_run",
    )?
    .query_map([], |row| {
        Ok(RunAge {
            id: row.get(0)?,
            started_at_ms: row.get(1)?,
            has_texts: row.get(2)?,
            awaits_verdict: required(3, row.get(3)?, RunState::from_code)?.awaits_verdict(),
        })
    })?
    .collect()
}

/// The approved groups of run `run_id`: each group's key and the hunks'
/// SHA-256 the author approved (AD-113).
pub fn approvals(conn: &Connection, run_id: &str) -> rusqlite::Result<Vec<(String, String)>> {
    conn.prepare_cached(
        "SELECT group_key, hunks_sha256 FROM workshop_approval WHERE run_id = ?1 ORDER BY group_key",
    )?
    .query_map([run_id], |row| Ok((row.get(0)?, row.get(1)?)))?
    .collect()
}

/// An image as its last build recorded it (AD-99).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRecord {
    pub name: String,
    pub kind: ImageKind,
    pub project: Option<String>,
    pub image_id: Option<String>,
    pub labels_json: Option<String>,
    pub env_names_json: Option<String>,
    pub programs_sha256: Option<String>,
    pub built_at_ms: i64,
    pub outcome: BuildOutcome,
    /// The build's last output, the engine's and a Dockerfile's text;
    /// cleared at the excerpt TTL (AD-124).
    pub build_tail: Option<Untrusted<String>>,
}

/// The image row `name`, if any.
pub fn image(conn: &Connection, name: &str) -> rusqlite::Result<Option<ImageRecord>> {
    conn.prepare_cached(
        "SELECT name, kind, project, image_id, labels_json, env_names_json, programs_sha256,
                built_at_ms, outcome, build_tail
         FROM workshop_image WHERE name = ?1",
    )?
    .query_row([name], |row| {
        Ok(ImageRecord {
            name: row.get(0)?,
            kind: required(1, row.get(1)?, ImageKind::from_code)?,
            project: row.get(2)?,
            image_id: row.get(3)?,
            labels_json: row.get(4)?,
            env_names_json: row.get(5)?,
            programs_sha256: row.get(6)?,
            built_at_ms: row.get(7)?,
            outcome: required(8, row.get(8)?, BuildOutcome::from_code)?,
            build_tail: row.get::<_, Option<String>>(9)?.map(Untrusted::new),
        })
    })
    .optional()
}

/// One project's inventory on one image (AD-118, D-212).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryRecord {
    pub image: String,
    pub project: String,
    pub image_id: String,
    pub inventory_key: String,
    pub inventory_json: String,
    pub inventory_at_ms: i64,
}

/// The inventory of `project` on `image`, if any.
pub fn inventory(
    conn: &Connection,
    image: &str,
    project: &str,
) -> rusqlite::Result<Option<InventoryRecord>> {
    conn.prepare_cached(
        "SELECT image, project, image_id, inventory_key, inventory_json, inventory_at_ms
         FROM workshop_inventory WHERE image = ?1 AND project = ?2",
    )?
    .query_row([image, project], |row| {
        Ok(InventoryRecord {
            image: row.get(0)?,
            project: row.get(1)?,
            image_id: row.get(2)?,
            inventory_key: row.get(3)?,
            inventory_json: row.get(4)?,
            inventory_at_ms: row.get(5)?,
        })
    })
    .optional()
}

/// What a battery's validity keys on (AD-117, D-232).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryKey {
    pub image_id: String,
    pub gateway_image_id: String,
    pub engine_key: String,
    pub officina_version: String,
    pub vector_sha256: String,
    pub programs_sha256: String,
}

/// A battery row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryRecord {
    pub id: String,
    pub image: String,
    pub key: BatteryKey,
    pub clock_skew_ms: Option<i64>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub outcome: Option<BatteryOutcome>,
    pub totals: BatteryTotals,
    pub cause: Option<String>,
}

/// A finished battery's counts (AD-117).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatteryTotals {
    pub attempts: u32,
    pub refused: u32,
    pub escaped: u32,
    pub inconclusive: u32,
    pub not_applicable: u32,
}

/// The columns [`battery_row`] reads, in its order.
const BATTERY_COLUMNS: &str = "id, image, image_id, gateway_image_id, engine_key, officina_version,
     vector_sha256, programs_sha256, clock_skew_ms, started_at_ms, finished_at_ms,
     outcome, attempts, refused, escaped, inconclusive, not_applicable, cause";

fn battery_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BatteryRecord> {
    Ok(BatteryRecord {
        id: row.get(0)?,
        image: row.get(1)?,
        key: BatteryKey {
            image_id: row.get(2)?,
            gateway_image_id: row.get(3)?,
            engine_key: row.get(4)?,
            officina_version: row.get(5)?,
            vector_sha256: row.get(6)?,
            programs_sha256: row.get(7)?,
        },
        clock_skew_ms: row.get(8)?,
        started_at_ms: row.get(9)?,
        finished_at_ms: row.get(10)?,
        outcome: optional(row, 11, BatteryOutcome::from_code)?,
        totals: BatteryTotals {
            attempts: count(row, 12)?.unwrap_or(0),
            refused: count(row, 13)?.unwrap_or(0),
            escaped: count(row, 14)?.unwrap_or(0),
            inconclusive: count(row, 15)?.unwrap_or(0),
            not_applicable: count(row, 16)?.unwrap_or(0),
        },
        cause: row.get(17)?,
    })
}

/// The newest battery that passed for exactly `key` (story 6.10's
/// validity reads it).
pub fn newest_passing_battery(
    conn: &Connection,
    key: &BatteryKey,
) -> rusqlite::Result<Option<BatteryRecord>> {
    conn.prepare_cached(&format!(
        "SELECT {BATTERY_COLUMNS} FROM workshop_battery
         WHERE image_id = ?1 AND gateway_image_id = ?2 AND engine_key = ?3
           AND officina_version = ?4 AND vector_sha256 = ?5 AND programs_sha256 = ?6
           AND outcome = ?7
         ORDER BY finished_at_ms DESC, id LIMIT 1"
    ))?
    .query_row(
        params![
            key.image_id,
            key.gateway_image_id,
            key.engine_key,
            key.officina_version,
            key.vector_sha256,
            key.programs_sha256,
            BatteryOutcome::Passed.code()
        ],
        battery_row,
    )
    .optional()
}

/// The most battery rows read per image: validity needs the newest
/// passing one, doctor the newest of all.
pub const BATTERIES_PER_IMAGE: u32 = 50;

/// The newest batteries of image tag `image`, newest first (AD-117,
/// AD-123): validity and doctor read them.
pub fn batteries_of(conn: &Connection, image: &str) -> rusqlite::Result<Vec<BatteryRecord>> {
    conn.prepare_cached(&format!(
        "SELECT {BATTERY_COLUMNS} FROM workshop_battery WHERE image = ?1
         ORDER BY started_at_ms DESC, id LIMIT ?2"
    ))?
    .query_map(params![image, BATTERIES_PER_IMAGE], battery_row)?
    .collect()
}

/// Every image row, by name (AD-99): the battery picks the stale ones.
pub fn images(conn: &Connection) -> rusqlite::Result<Vec<ImageRecord>> {
    let names: Vec<String> = conn
        .prepare_cached("SELECT name FROM workshop_image ORDER BY name")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    names
        .iter()
        .filter_map(|name| image(conn, name).transpose())
        .collect()
}

#[cfg(feature = "window-authority")]
pub use writers::{PackageFields, RunInsert};

/// The writing half: only the window's collector writes (AD-38).
#[cfg(feature = "window-authority")]
mod writers {
    use officina_core::workshop::run::{RetentionAction, Runner};
    use rusqlite::{TransactionBehavior, params};

    use super::*;
    use crate::registry::orders::{LaunchRecord, NameTaken, insert_order_row};
    use crate::registry::store::Writer;
    use crate::registry::write::RegistryError;

    /// What a workshop launch records beside its `work_order` row (AD-107
    /// step 1).
    #[derive(Debug, Clone)]
    pub struct RunInsert<'a> {
        pub image: &'a str,
        pub image_id: &'a str,
        pub gateway_image_id: &'a str,
        pub base_branch: &'a str,
        pub base_commit: &'a str,
        pub parent_run_id: Option<&'a str>,
        pub branch: &'a str,
        pub uncommitted: u32,
        pub plan_json: &'a str,
        pub plan_sha256: &'a str,
        pub credential_kind: CredentialKind,
        pub deadline_ms: i64,
        pub battery_id: &'a str,
        /// The worker's `claude` vector, the row's `argv_json` (AD-107).
        pub argv: &'a [String],
    }

    /// What collection records of a package (AD-110 to AD-113).
    #[derive(Debug, Clone, Default)]
    pub struct PackageFields<'a> {
        pub package_sha256: Option<&'a str>,
        pub tip_commit: Option<&'a str>,
        pub commits_past_base: Option<u32>,
        pub files_changed: Option<u32>,
        pub groups_json: Option<&'a str>,
        pub quarantine_cause: Option<&'a str>,
    }

    fn json<T: serde::Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap_or_default()
    }

    impl Writer {
        /// Inserts a workshop launch at `now_ms` (AD-107 step 1): its
        /// `work_order` row, runner `workshop` and outcome `launching`, and
        /// its `workshop_run` row, state `launching`, in one transaction. A
        /// name or `(template, seq)` already held is refused [`NameTaken`],
        /// nothing written.
        pub fn insert_workshop_run(
            &mut self,
            launch: &LaunchRecord,
            run: &RunInsert,
            now_ms: i64,
        ) -> Result<Result<(), NameTaken>, RegistryError> {
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Err(taken) = insert_order_row(&tx, launch, run.argv, Runner::Workshop, now_ms)? {
                return Ok(Err(taken));
            }
            tx.execute(
                "INSERT INTO workshop_run (id, image, image_id, gateway_image_id, base_branch,
                     base_commit, parent_run_id, branch, uncommitted, plan_json, plan_sha256,
                     credential_kind, deadline_ms, battery_id, state, state_at_ms, started_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
                params![
                    launch.card_id,
                    run.image,
                    run.image_id,
                    run.gateway_image_id,
                    run.base_branch,
                    run.base_commit,
                    run.parent_run_id,
                    run.branch,
                    run.uncommitted,
                    run.plan_json,
                    run.plan_sha256,
                    run.credential_kind.code(),
                    run.deadline_ms,
                    run.battery_id,
                    RunState::Launching.code(),
                    now_ms
                ],
            )?;
            tx.commit()?;
            Ok(Ok(()))
        }

        /// Moves run `id` to `state` at `now_ms`; false when no such run.
        pub fn set_run_state(
            &mut self,
            id: &str,
            state: RunState,
            now_ms: i64,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET state = ?2, state_at_ms = ?3 WHERE id = ?1",
                params![id, state.code(), now_ms],
            )? == 1)
        }

        /// Records when Stop all kept run `id`'s containers (AD-116).
        pub fn set_run_stopped_at(&mut self, id: &str, at_ms: i64) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET stopped_at_ms = ?2 WHERE id = ?1",
                params![id, at_ms],
            )? == 1)
        }

        /// Records a kept run removed without collection (AD-116): its
        /// removal time and the system's reason `not_collected`.
        pub fn set_run_not_collected(
            &mut self,
            id: &str,
            now_ms: i64,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET removed_at_ms = ?2, discard_reason = ?3, decided_at_ms = ?2
                 WHERE id = ?1",
                params![id, now_ms, DiscardReason::NotCollected.code()],
            )? == 1)
        }

        /// Records how run `id` ended (AD-109).
        pub fn set_run_finished(
            &mut self,
            id: &str,
            cause: FinishCause,
            outcome: WorkshopOutcome,
            exit_code: Option<i64>,
            now_ms: i64,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET cause = ?2, outcome = ?3, exit_code = ?4, finished_at_ms = ?5
                 WHERE id = ?1",
                params![id, cause.code(), outcome.code(), exit_code, now_ms],
            )? == 1)
        }

        /// Records what run `id` produced (AD-110) and when it finished.
        pub fn set_run_outcome(
            &mut self,
            id: &str,
            outcome: WorkshopOutcome,
            now_ms: i64,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET outcome = ?2, finished_at_ms = ?3 WHERE id = ?1",
                params![id, outcome.code(), now_ms],
            )? == 1)
        }

        /// Records run `id`'s first Receive at `now_ms` (AD-120); false
        /// when it was opened before, or no such run.
        pub fn set_run_opened(&mut self, id: &str, now_ms: i64) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET opened_at_ms = ?2 WHERE id = ?1 AND opened_at_ms IS NULL",
                params![id, now_ms],
            )? == 1)
        }

        /// Records how run `id`'s worker ended (AD-109), before collection
        /// decides what it produced.
        pub fn set_run_cause(
            &mut self,
            id: &str,
            cause: FinishCause,
            exit_code: Option<i64>,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET cause = ?2, exit_code = ?3 WHERE id = ?1",
                params![id, cause.code(), exit_code],
            )? == 1)
        }

        /// Replaces run `id`'s facts.
        pub fn set_run_facts(&mut self, id: &str, facts: &RunFacts) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET facts_json = ?2 WHERE id = ?1",
                params![id, json(facts)],
            )? == 1)
        }

        /// Replaces run `id`'s texts.
        pub fn set_run_texts(&mut self, id: &str, texts: &RunTexts) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET texts_json = ?2 WHERE id = ?1",
                params![id, json(texts)],
            )? == 1)
        }

        /// Records run `id`'s package fields (AD-110).
        pub fn set_run_package(
            &mut self,
            id: &str,
            package: &PackageFields,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_run SET package_sha256 = ?2, tip_commit = ?3,
                     commits_past_base = ?4, files_changed = ?5, groups_json = ?6,
                     quarantine_cause = ?7
                 WHERE id = ?1",
                params![
                    id,
                    package.package_sha256,
                    package.tip_commit,
                    package.commits_past_base,
                    package.files_changed,
                    package.groups_json,
                    package.quarantine_cause
                ],
            )? == 1)
        }

        /// Records an approved group of run `run_id`, replacing an earlier
        /// approval of the same group (AD-113).
        pub fn approve_group(
            &mut self,
            run_id: &str,
            group_key: &str,
            hunks_sha256: &str,
            now_ms: i64,
        ) -> Result<(), RegistryError> {
            self.conn.execute(
                "INSERT OR REPLACE INTO workshop_approval (run_id, group_key, hunks_sha256, approved_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![run_id, group_key, hunks_sha256, now_ms],
            )?;
            Ok(())
        }

        /// Records the author's verdict on run `id` (AD-114): its state,
        /// verdict, reason and decision time in one statement.
        pub fn set_verdict(
            &mut self,
            id: &str,
            verdict: PackageVerdict,
            reason: Option<DiscardReason>,
            now_ms: i64,
        ) -> Result<bool, RegistryError> {
            let state = match verdict {
                PackageVerdict::Imported => RunState::Imported,
                PackageVerdict::Discarded => RunState::Discarded,
            };
            Ok(self.conn.execute(
                "UPDATE workshop_run SET state = ?2, state_at_ms = ?5, verdict = ?3,
                     discard_reason = ?4, decided_at_ms = ?5
                 WHERE id = ?1",
                params![
                    id,
                    state.code(),
                    verdict.code(),
                    reason.map(DiscardReason::code),
                    now_ms
                ],
            )? == 1)
        }

        /// Records an image's build, replacing its earlier row (AD-99);
        /// `image.build_tail` is ignored for `build_tail`, the build's own
        /// output as it came.
        pub fn record_image(
            &mut self,
            image: &ImageRecord,
            build_tail: Option<&str>,
        ) -> Result<(), RegistryError> {
            self.conn.execute(
                "INSERT OR REPLACE INTO workshop_image (name, kind, project, image_id, labels_json,
                     env_names_json, programs_sha256, built_at_ms, outcome, build_tail)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    image.name,
                    image.kind.code(),
                    image.project,
                    image.image_id,
                    image.labels_json,
                    image.env_names_json,
                    image.programs_sha256,
                    image.built_at_ms,
                    image.outcome.code(),
                    build_tail
                ],
            )?;
            Ok(())
        }

        /// Records a project's inventory on an image, replacing the earlier
        /// one (AD-118).
        pub fn record_inventory(
            &mut self,
            inventory: &InventoryRecord,
        ) -> Result<(), RegistryError> {
            self.conn.execute(
                "INSERT OR REPLACE INTO workshop_inventory (image, project, image_id, inventory_key,
                     inventory_json, inventory_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    inventory.image,
                    inventory.project,
                    inventory.image_id,
                    inventory.inventory_key,
                    inventory.inventory_json,
                    inventory.inventory_at_ms
                ],
            )?;
            Ok(())
        }

        /// Records a battery's start (AD-117).
        pub fn insert_battery(
            &mut self,
            id: &str,
            image: &str,
            key: &BatteryKey,
            clock_skew_ms: Option<i64>,
            now_ms: i64,
        ) -> Result<(), RegistryError> {
            self.conn.execute(
                "INSERT INTO workshop_battery (id, image, image_id, gateway_image_id, engine_key,
                     officina_version, vector_sha256, programs_sha256, clock_skew_ms, started_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    image,
                    key.image_id,
                    key.gateway_image_id,
                    key.engine_key,
                    key.officina_version,
                    key.vector_sha256,
                    key.programs_sha256,
                    clock_skew_ms,
                    now_ms
                ],
            )?;
            Ok(())
        }

        /// Records how battery `id` ended.
        pub fn finish_battery(
            &mut self,
            id: &str,
            outcome: BatteryOutcome,
            totals: BatteryTotals,
            cause: Option<&str>,
            now_ms: i64,
        ) -> Result<bool, RegistryError> {
            Ok(self.conn.execute(
                "UPDATE workshop_battery SET outcome = ?2, attempts = ?3, refused = ?4, escaped = ?5,
                     inconclusive = ?6, not_applicable = ?7, cause = ?8, finished_at_ms = ?9
                 WHERE id = ?1",
                params![
                    id,
                    outcome.code(),
                    totals.attempts,
                    totals.refused,
                    totals.escaped,
                    totals.inconclusive,
                    totals.not_applicable,
                    cause,
                    now_ms
                ],
            )? == 1)
        }

        /// The run `id`.
        pub fn workshop_run(&self, id: &str) -> Result<Option<RunRecord>, RegistryError> {
            Ok(run(&self.conn, id)?)
        }

        /// The runs in any of `states`.
        pub fn workshop_runs_in(
            &self,
            states: &[RunState],
        ) -> Result<Vec<RunRecord>, RegistryError> {
            Ok(runs_in(&self.conn, states)?)
        }

        /// How many runs may hold a container now.
        pub fn workshop_live_count(&self) -> Result<u32, RegistryError> {
            Ok(live_count(&self.conn)?)
        }

        /// The packages awaiting a verdict.
        pub fn workshop_awaiting_verdict(&self) -> Result<Vec<RunRecord>, RegistryError> {
            Ok(awaiting_verdict(&self.conn)?)
        }

        /// The runs of `template` imported since `since_ms`.
        pub fn workshop_imported_since(
            &self,
            template: &str,
            since_ms: i64,
        ) -> Result<Vec<RunRecord>, RegistryError> {
            Ok(imported_since(&self.conn, template, since_ms)?)
        }

        /// The approved groups of run `run_id`.
        pub fn workshop_approvals(
            &self,
            run_id: &str,
        ) -> Result<Vec<(String, String)>, RegistryError> {
            Ok(approvals(&self.conn, run_id)?)
        }

        /// The image row `name`.
        pub fn workshop_image(&self, name: &str) -> Result<Option<ImageRecord>, RegistryError> {
            Ok(image(&self.conn, name)?)
        }

        /// The inventory of `project` on `image`.
        pub fn workshop_inventory(
            &self,
            image_name: &str,
            project: &str,
        ) -> Result<Option<InventoryRecord>, RegistryError> {
            Ok(inventory(&self.conn, image_name, project)?)
        }

        /// Records a finished battery whole (AD-117): its start and its
        /// end in one transaction, so no row is ever left without an
        /// outcome by a window that closed mid-battery.
        pub fn record_battery(&mut self, record: &BatteryRecord) -> Result<(), RegistryError> {
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "INSERT INTO workshop_battery (id, image, image_id, gateway_image_id, engine_key,
                     officina_version, vector_sha256, programs_sha256, clock_skew_ms, started_at_ms,
                     finished_at_ms, outcome, attempts, refused, escaped, inconclusive,
                     not_applicable, cause)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18)",
                params![
                    record.id,
                    record.image,
                    record.key.image_id,
                    record.key.gateway_image_id,
                    record.key.engine_key,
                    record.key.officina_version,
                    record.key.vector_sha256,
                    record.key.programs_sha256,
                    record.clock_skew_ms,
                    record.started_at_ms,
                    record.finished_at_ms,
                    record.outcome.map(BatteryOutcome::code),
                    record.totals.attempts,
                    record.totals.refused,
                    record.totals.escaped,
                    record.totals.inconclusive,
                    record.totals.not_applicable,
                    record.cause
                ],
            )?;
            tx.commit()?;
            Ok(())
        }

        /// Every image row.
        pub fn workshop_images(&self) -> Result<Vec<ImageRecord>, RegistryError> {
            Ok(images(&self.conn)?)
        }

        /// The newest batteries of image tag `image`.
        pub fn workshop_batteries(&self, image: &str) -> Result<Vec<BatteryRecord>, RegistryError> {
            Ok(batteries_of(&self.conn, image)?)
        }

        /// The newest passing battery for `key`.
        pub fn workshop_passing_battery(
            &self,
            key: &BatteryKey,
        ) -> Result<Option<BatteryRecord>, RegistryError> {
            Ok(newest_passing_battery(&self.conn, key)?)
        }

        /// The runs, as retention reads them.
        pub fn workshop_run_ages(&self) -> Result<Vec<RunAge>, RegistryError> {
            Ok(run_ages(&self.conn)?)
        }

        /// Applies retention's registry actions (AD-124, D-213): texts
        /// cleared, each undecided package discarded `expired` at `now_ms`
        /// while it still awaits a verdict, and each run deleted with its
        /// `work_order` and approvals, in one transaction; folder actions
        /// are the caller's. Returns the texts cleared, the runs deleted and
        /// the ids of the packages expired, for their journal lines.
        pub fn apply_workshop_retention(
            &mut self,
            actions: &[RetentionAction],
            now_ms: i64,
        ) -> Result<(usize, usize, Vec<String>), RegistryError> {
            let tx = self.conn.transaction()?;
            let (mut cleared, mut deleted, mut expired) = (0, 0, Vec::new());
            for action in actions {
                match action {
                    RetentionAction::Expire(id) => {
                        let state: Option<String> = tx
                            .query_row(
                                "SELECT state FROM workshop_run WHERE id = ?1",
                                [id],
                                |row| row.get(0),
                            )
                            .optional()?;
                        let awaits = state
                            .as_deref()
                            .and_then(RunState::from_code)
                            .is_some_and(RunState::awaits_verdict);
                        if awaits {
                            tx.execute(
                                "UPDATE workshop_run SET state = ?2, state_at_ms = ?5, verdict = ?3,
                                     discard_reason = ?4, decided_at_ms = ?5
                                 WHERE id = ?1",
                                params![
                                    id,
                                    RunState::Discarded.code(),
                                    PackageVerdict::Discarded.code(),
                                    DiscardReason::Expired.code(),
                                    now_ms
                                ],
                            )?;
                            expired.push(id.clone());
                        }
                    }
                    RetentionAction::ClearTexts(id) => {
                        cleared += tx.execute(
                            "UPDATE workshop_run SET texts_json = NULL WHERE id = ?1",
                            [id],
                        )?;
                    }
                    RetentionAction::DeleteRun(id) => {
                        tx.execute("DELETE FROM workshop_approval WHERE run_id = ?1", [id])?;
                        deleted += tx.execute("DELETE FROM workshop_run WHERE id = ?1", [id])?;
                        tx.execute("DELETE FROM work_order WHERE id = ?1", [id])?;
                    }
                    RetentionAction::DeleteFolder(_) => {}
                }
            }
            tx.commit()?;
            Ok((cleared, deleted, expired))
        }
    }
}
