//! Which permission mode a project's work orders start in (story 5.24,
//! D-980, D-982 to D-986). By default a work order takes the project's own
//! mode, as a session the author starts there would: no
//! `--permission-mode` on its vector. Per project, the author may switch
//! its work orders to Officina's default, [`OFFICINA_WORK_ORDER_MODE`], and
//! back. The choice is Officina's own state, the window's record
//! `relay/work-order-permissions.json`, never Claude Code's configuration
//! nor the floor; this module is its format and its reading, the window's
//! authority writes it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The schema of the record this build writes and reads.
pub const WORK_ORDER_PERMISSIONS_SCHEMA: u32 = 1;

/// The permission mode a work order starts in when its project takes
/// Officina's default (D-982): Claude Code's auto mode, whose classifier
/// accepts `officina run` (research notes E85), never `bypassPermissions`.
pub const OFFICINA_WORK_ORDER_MODE: &str = "auto";

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a work order's permission mode comes from (D-980).
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum WorkOrderPermission {
        /// The project's own settings, else the author's default: no
        /// permission mode on the vector.
        #[default]
        Project,
        /// Officina's default, [`OFFICINA_WORK_ORDER_MODE`].
        Officina,
    }
}

impl WorkOrderPermission {
    /// The `--permission-mode` value a work order's vector carries, if any.
    ///
    /// ```
    /// use officina_core::orders::permission::WorkOrderPermission;
    /// assert_eq!(WorkOrderPermission::Project.mode(), None);
    /// assert_eq!(WorkOrderPermission::Officina.mode(), Some("auto"));
    /// ```
    pub fn mode(self) -> Option<&'static str> {
        match self {
            WorkOrderPermission::Project => None,
            WorkOrderPermission::Officina => Some(OFFICINA_WORK_ORDER_MODE),
        }
    }
}

/// One project whose work orders take Officina's default: the folder its
/// choice was made for, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficinaDefault {
    /// The project's folder as the approved catalogue resolved it when the
    /// author chose: a project whose folder changed since takes its own
    /// mode again (D-984).
    pub folder: String,
    pub set_at_ms: i64,
}

/// The window's record of the projects whose work orders take Officina's
/// default; every other project takes its own mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrderPermissions {
    pub schema: u32,
    /// By project name.
    pub officina_default: BTreeMap<String, OfficinaDefault>,
}

impl Default for WorkOrderPermissions {
    fn default() -> Self {
        WorkOrderPermissions {
            schema: WORK_ORDER_PERMISSIONS_SCHEMA,
            officina_default: BTreeMap::new(),
        }
    }
}

impl WorkOrderPermissions {
    /// The record in `text`, or why it is not one: a schema this build
    /// does not read is not one either.
    pub fn parse(text: &str) -> Result<WorkOrderPermissions, String> {
        let record: WorkOrderPermissions =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        if record.schema != WORK_ORDER_PERMISSIONS_SCHEMA {
            return Err(format!(
                "schema {} is not {WORK_ORDER_PERMISSIONS_SCHEMA}",
                record.schema
            ));
        }
        Ok(record)
    }

    /// Where the work orders of `project`, in `folder`, take their mode
    /// from: Officina's default only when the record names the project
    /// with that very folder.
    ///
    /// ```
    /// use officina_core::orders::permission::{WorkOrderPermission, WorkOrderPermissions};
    /// let record = WorkOrderPermissions::default().with("backend", "C:/work/Backend", WorkOrderPermission::Officina, 1);
    /// assert_eq!(record.of("backend", "C:/work/Backend"), WorkOrderPermission::Officina);
    /// assert_eq!(record.of("backend", "C:/work/Other"), WorkOrderPermission::Project);
    /// assert_eq!(record.of("lab", "C:/work/Backend"), WorkOrderPermission::Project);
    /// ```
    pub fn of(&self, project: &str, folder: &str) -> WorkOrderPermission {
        match self.officina_default.get(project) {
            Some(chosen) if chosen.folder == folder => WorkOrderPermission::Officina,
            _ => WorkOrderPermission::Project,
        }
    }

    /// The record once `project`, in `folder`, takes `permission` at
    /// `now_ms`: [`WorkOrderPermission::Officina`] names it, with its
    /// folder; [`WorkOrderPermission::Project`] removes it.
    #[must_use]
    pub fn with(
        &self,
        project: &str,
        folder: &str,
        permission: WorkOrderPermission,
        now_ms: i64,
    ) -> WorkOrderPermissions {
        let mut next = self.clone();
        next.schema = WORK_ORDER_PERMISSIONS_SCHEMA;
        match permission {
            WorkOrderPermission::Officina => {
                next.officina_default.insert(
                    project.to_owned(),
                    OfficinaDefault {
                        folder: folder.to_owned(),
                        set_at_ms: now_ms,
                    },
                );
            }
            WorkOrderPermission::Project => {
                next.officina_default.remove(project);
            }
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDER: &str = "C:/path/to/work/Backend";

    #[test]
    fn the_record_round_trips_and_reverts_to_the_project_mode() {
        let none = WorkOrderPermissions::default();
        assert_eq!(none.of("backend", FOLDER), WorkOrderPermission::Project);
        let chosen = none.with("backend", FOLDER, WorkOrderPermission::Officina, 7);
        let text = serde_json::to_string_pretty(&chosen).unwrap();
        let read = WorkOrderPermissions::parse(&text).unwrap();
        assert_eq!(read, chosen);
        assert_eq!(read.of("backend", FOLDER), WorkOrderPermission::Officina);
        let back = read.with("backend", FOLDER, WorkOrderPermission::Project, 8);
        assert_eq!(back, none);
    }

    #[test]
    fn another_schema_or_shape_is_refused() {
        assert!(WorkOrderPermissions::parse("{").is_err());
        assert!(WorkOrderPermissions::parse(r#"{"schema": 2, "officina_default": {}}"#).is_err());
        assert!(
            WorkOrderPermissions::parse(
                r#"{"schema": 1, "officina_default": {"backend": {"folder": 3, "set_at_ms": 1}}}"#
            )
            .is_err()
        );
        assert!(
            WorkOrderPermissions::parse("\u{feff}{\"schema\": 1, \"officina_default\": {}}")
                .is_ok()
        );
    }

    /// D-982: Officina's default is auto mode and never a mode that lets
    /// every call through.
    #[test]
    fn officinas_default_is_auto_never_bypass() {
        for permission in WorkOrderPermission::ALL {
            let mode = permission.mode();
            assert_ne!(mode, Some("bypassPermissions"));
            assert_ne!(mode, Some("dontAsk"));
        }
        assert_eq!(WorkOrderPermission::Officina.mode(), Some("auto"));
    }
}
