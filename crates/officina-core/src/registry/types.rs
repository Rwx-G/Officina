//! The registry's vocabulary (AD-38, AD-40, D-45): every enum a column of
//! schema 1 stores, each declared with `closed_enum!` so its `ALL` cannot
//! miss a variant and its text form (`code()`, `snake_case`) is the one the
//! `CHECK` constraints list; the records rows are read into; and
//! [`Sourced`], a value with its provenance.

use serde::{Deserialize, Serialize};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a stored fact came from (the PRD glossary's provenance).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Source {
        /// `claude agents --json`.
        Agents,
        /// A hook event line.
        Hook,
        /// Officina's own deterministic rules.
        Rules,
        /// The triage model's answer (FR-10).
        Model,
        /// The author, in the window.
        Author,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a verdict proposal came from (FR-8).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProposalSource {
        /// Claude Code's background `state`.
        State,
        /// Officina's rules, the default included.
        Rules,
        /// The triage model.
        Model,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A verdict the author confirms (FR-9).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Verdict {
        Finished,
        ToResume,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A verdict a proposal may carry (glossary, Proposal), also the enum of
    /// the triage model's answer schema (AD-51).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ModelVerdict {
        Finished,
        ToResume,
        /// Never correct (D-59): the author decides.
        Unsure,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a proposal says what it says (AD-40): never prose.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ReasonCode {
        /// A background session's `state` is `done`.
        StateDone,
        /// A background session's `state` is `failed`.
        StateFailed,
        /// A background session's `state` is `stopped`.
        StateStopped,
        /// The last `UserPromptSubmit` came after the last `Stop`.
        CutMidTurn,
        /// No earlier rule matched: `to_resume`.
        DefaultToResume,
        /// None of the subject's sessions saw a prompt (D-76).
        EmptySubject,
        /// The triage model answered.
        Model,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The kinds of triage row (AD-49, D-76).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RowKind {
        /// A subject whose sessions all ended.
        Subject,
        /// An ended, unfiled, interactive session that saw a prompt.
        UnfiledSession,
        /// A subject none of whose sessions saw a prompt.
        EmptySubject,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A subject's status (PRD glossary).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum SubjectStatus {
        Active,
        ToResume,
        Archived,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The keys of the `setting` table (D-88).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum SettingKey {
        /// The copy a damaged registry was restored from, and when (D-93).
        RestoreNotice,
        /// Whether the triage model is enabled, and why it changed (D-69).
        TriageModel,
        /// Why the triage model is paused, with the `claude` binary's stamp
        /// (D-66).
        TriageModelPause,
        /// Whether auto-archive is on, and why it changed (D-69).
        AutoArchive,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a triage call was for (AD-52).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum TriagePurpose {
        Proposal,
        DryRun,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a triage call ended (AD-51): an answer, or why there is none.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum TriageOutcome {
        Answer,
        /// Refused before spawning; spends no budget (D-65).
        Refused,
        Failed,
        Timeout,
        /// An answer that fails validation.
        Invalid,
    }
}

/// A value with where it came from and when (AD-40).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Sourced<T> {
    pub value: T,
    pub src: Source,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
}

/// One `event_file` row: how far an event file was ingested (AD-43).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventFileRecord {
    /// `YYYY-MM-DD.jsonl`.
    pub name: String,
    /// Bytes read, always up to a line end.
    pub ingested_bytes: u64,
    /// When the file was found complete: dated before today, fully read,
    /// past the grace.
    pub complete_at_ms: Option<i64>,
    /// Lines that could not be read, since the last time the file was read
    /// from its start.
    pub corrupt_lines: u32,
    /// How many times the file was found shorter than its offset, and read
    /// again from its start (D-84).
    pub rewrites: u32,
}

/// The value of the [`SettingKey::RestoreNotice`] setting (D-93): which
/// copy replaced a damaged registry, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RestoreNotice {
    /// The copy's file name under `registry/backup/`.
    pub copy: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub restored_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::*;

    /// Runs `check` once per enum of this file, giving it the enum's `ALL`,
    /// `code` and `from_code`; returns how many enums it ran on.
    fn each_enum(check: &dyn Fn(&dyn EnumUnderTest)) -> usize {
        let enums: [&dyn EnumUnderTest; 10] = [
            &Closed(Source::ALL, Source::code, Source::from_code),
            &Closed(
                ProposalSource::ALL,
                ProposalSource::code,
                ProposalSource::from_code,
            ),
            &Closed(Verdict::ALL, Verdict::code, Verdict::from_code),
            &Closed(
                ModelVerdict::ALL,
                ModelVerdict::code,
                ModelVerdict::from_code,
            ),
            &Closed(ReasonCode::ALL, ReasonCode::code, ReasonCode::from_code),
            &Closed(RowKind::ALL, RowKind::code, RowKind::from_code),
            &Closed(
                SubjectStatus::ALL,
                SubjectStatus::code,
                SubjectStatus::from_code,
            ),
            &Closed(SettingKey::ALL, SettingKey::code, SettingKey::from_code),
            &Closed(
                TriagePurpose::ALL,
                TriagePurpose::code,
                TriagePurpose::from_code,
            ),
            &Closed(
                TriageOutcome::ALL,
                TriageOutcome::code,
                TriageOutcome::from_code,
            ),
        ];
        for each in enums {
            check(each);
        }
        enums.len()
    }

    trait EnumUnderTest {
        fn round_trips(&self);
        fn refuses(&self, text: &str);
        fn codes(&self) -> Vec<&'static str>;
    }

    struct Closed<T: 'static>(&'static [T], fn(T) -> &'static str, fn(&str) -> Option<T>);

    impl<T> EnumUnderTest for Closed<T>
    where
        T: Copy + PartialEq + Debug + Serialize + DeserializeOwned + 'static,
    {
        fn round_trips(&self) {
            let Closed(all, code, from_code) = self;
            for &variant in *all {
                let text = code(variant);
                assert_eq!(from_code(text), Some(variant), "{variant:?}");
                assert_eq!(
                    serde_json::to_value(variant).unwrap().as_str(),
                    Some(text),
                    "{variant:?}"
                );
                let back: T = serde_json::from_value(text.into()).unwrap();
                assert_eq!(back, variant);
                assert!(
                    !text.is_empty() && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                    "{text} is not snake_case"
                );
            }
        }

        fn refuses(&self, text: &str) {
            let Closed(_, _, from_code) = self;
            assert_eq!(from_code(text), None, "{text:?}");
            assert!(
                serde_json::from_value::<T>(text.into()).is_err(),
                "{text:?}"
            );
        }

        fn codes(&self) -> Vec<&'static str> {
            let Closed(all, code, _) = self;
            all.iter().map(|v| code(*v)).collect()
        }
    }

    /// The count of enums checked must match the `closed_enum!` blocks of
    /// this file, so a new enum fails here until it is listed above.
    fn declared_enums() -> usize {
        include_str!("types.rs")
            .lines()
            .filter(|line| line.trim() == "closed_enum! {")
            .count()
    }

    #[test]
    fn every_variant_round_trips_its_text() {
        let checked = each_enum(&|e| e.round_trips());
        assert_eq!(checked, declared_enums());
    }

    #[test]
    fn unknown_text_is_refused() {
        each_enum(&|e| {
            e.refuses("");
            e.refuses("not_a_variant");
            for code in e.codes() {
                e.refuses(&code.to_ascii_uppercase());
                e.refuses(&format!(" {code}"));
                if code.contains('_') {
                    e.refuses(&code.replace('_', "-"));
                    e.refuses(&code.replace('_', ""));
                }
            }
        });
    }
}
