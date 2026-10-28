# Story 5.20: Permission modes in doctor, and the auto-mode block the author may paste

**Status:** Approved

## Story

As the author whose host sessions run in `bypassPermissions` today, and who may switch them to Claude Code's auto mode,
I want doctor to tell me which permission mode my sessions actually run in, and, only when I ask, to print the complete user-settings keys that would switch me to auto mode with what the relay needs to keep working,
so that the switch is my decision, made on facts, pasted in one replacement, and never written by Officina (PRD section 9 item 7).

## Acceptance Criteria

1. **The event line carries the mode** (AD-180's doctor, D-574; E2, E15). `officina hook` writes `permission_mode` on every event line whose payload carries it, as an optional field cut to 32 characters on a character boundary; lines without it, older ones included, parse unchanged; `core::contract`'s hook fields gain it, so the canary notices when a payload drops it. Proof: core `crates/officina-core/src/events.rs::tests::{permission_mode_round_trips_and_is_optional, permission_mode_is_cut_to_32_characters}`; `crates/officina-core/src/contract.rs::tests::the_hook_fields_name_permission_mode`; CLI `crates/officina/tests/hook.rs::the_hook_records_the_permission_mode`; `crates/officina/tests/perf.rs`'s hook own-cost case stays within its bound (release, ignored run).
2. **Doctor's permission line** (D-574, D-573). Doctor reads the event lines of the last 24 h read-only and prints one "permission mode" line: for each mode seen, the count of distinct sessions, split by kind (interactive or background, from the registry's session records read-only, "unknown" without one); a value outside the documented ones (one core constant, with the permission-modes page cited; the story does not copy it) is named as unknown; no line at all when no event line carries the field. The line is information and never fails doctor, except D-573's warning when gate 8 failed in every case. Proof: host `crates/officina-host/src/doctor.rs::tests::{the_permission_line_counts_sessions_per_mode_and_kind, an_unknown_mode_is_named, a_session_without_a_record_is_unknown, no_permission_mode_no_line}`, and, in the branch that ships, `a_guarded_project_in_auto_mode_warns`.
3. **The block, on request only** (D-574, D-530, D-534). `officina doctor --permissions` reads the user settings' `permissions` and `autoMode` keys read-only and prints, under a line saying the switch from `bypassPermissions` to auto mode is the author's decision:
   - the whole `permissions` key, every existing entry and order kept, `defaultMode` set to `"auto"`;
   - the whole `autoMode` key, every existing entry kept; when the file has none, `{"environment": ["$defaults", "<trusted repository>", "<lab host group>"]}` with the placeholders explained in the line above it;
   - gate 8's addition (AC 4);

   each block written whole to `<home>/doctor/user-permissions.json` and `<home>/doctor/user-autoMode.json` (D-534's naming, cleaned like the other paste files), its line naming the file; a key that is not the documented shape gets the reason and no block; a settings file that does not exist gets both keys as its whole content. Plain `officina doctor` prints one line saying the block exists and the flag that prints it, and reads neither key. No settings file is ever written. The usage text lists the flag. Proof: host `doctor.rs::tests::{the_permissions_block_keeps_every_entry_and_sets_auto, an_absent_auto_mode_gets_defaults_and_placeholders, an_existing_auto_mode_is_kept_whole, an_unparsable_key_gets_no_block, a_missing_settings_file_gets_both_keys, each_permissions_block_is_written_to_its_file, plain_doctor_reads_neither_key}`; CLI `crates/officina/tests/quota_doctor.rs::doctor_permissions_writes_no_settings_file` (the fingerprint of the config dir and `<home>/.claude.json` unchanged), `crates/officina/tests/readme.rs` (usage and README name the flag).
4. **Gate 8's branch** (D-572, D-573, D-127). The branch the gate 8 entry recorded ships, and only it: case a passed, nothing is added; case b passed, `permissions.allow` gains the recorded rule after the author's entries; case c passed, `autoMode.allow` gains the recorded sentence; all failed, AC 2's warning and the README's statement that the relay's `human` and `auto` paths stop in auto mode. The text added is one core constant with the entry cited, never typed twice. Proof: host `doctor.rs::tests::the_gate_8_addition_is_the_recorded_one` (or, in the failed branch, `a_guarded_project_in_auto_mode_warns`); the Completion Notes name the branch.
5. **Officina sets no mode** (FR-3, principle 2, D-490). Nothing in this story changes a vector: work orders still pass no permission mode, Officina's own sessions keep theirs (AD-51, AD-89), and no command writes `defaultMode` anywhere. Proof: the golden cases of `tests/vectors/spawn.json` unchanged by this story; `quota_doctor.rs::no_command_writes_under_the_claude_config_dir` covers `doctor --permissions`.
6. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: the permission line and `officina doctor --permissions`; README "The orchestrator (v0.5, in progress)": what the line shows, that the block is printed only on request and is the author's to paste, what it adds for the relay and why (gate 8), and that Officina never switches a mode. Story 5.19 folds it into the permission tiers. Proof: review; `readme.rs`.

## Tasks

- [ ] `events.rs` field, the hook's write, `contract.rs` (AC 1)
- [ ] Doctor's permission line over the event lines and the registry (AC 2)
- [ ] `--permissions`: read, merge, print, write the two paste files; the pointer line; usage (AC 3)
- [ ] Gate 8's recorded branch (AC 4)
- [ ] Zero-write and readme cases (AC 3, 5)
- [ ] Update `CHANGELOG.md` and the README (AC 6)

## Dev Notes

- Requirements: PRD section 9 items 7 and 8 (its outcome), FR-15. Decisions: AD-180 (doctor), AD-94 part B, AD-16 as refined by epic-4-decisions.md D-530 and D-534. Facts: E2 and E15 (`permission_mode` in hook payloads), the gate 8 entry (story 5.1). Risks: R107. Epic decisions: D-490, D-560, D-564, D-572 to D-574, D-577.
- Entry: the gate 8 entry is recorded (story 5.1 AC 8); story 5.13 has merged, so doctor's Slice B lines and this one land in the same section that 5.19 orders.
- Seams (D-563): `officina-core::events` (`EventLine`), `officina-core::contract` (hook fields), the hook in `crates/officina/src/main.rs`, `crates/officina-host/src/doctor.rs` (`missing_hook_events`, the merged blocks of D-530, the paste files of D-534), the registry reader.
- Why a flag (D-574): D-530 prints only the `hooks` and `statusLine` keys because another key can hold a token in a rule's text; reading `permissions` and `autoMode` is lifted only when the author asks, and the block goes to his terminal and his own Officina home, never further. Merged whole, so he replaces two keys rather than editing fragments (D-530's reason).
- The auto-mode configuration is read by Claude Code from user settings, managed settings and `--settings` only, never from project settings (research note, auto-mode-config page), which is why the block targets the user settings file.
- Switching modes changes how a work order behaves (the research note's Q7): D-573 has gate 4 run again after the owner switches; this story only says so in the README.
- No employer content: the placeholders are `<trusted repository>` and `<lab host group>`, never a real name; the author fills them in his own file.

## Testing

- Core unit: `crates/officina-core/src/{events,contract}.rs`.
- Host unit: `crates/officina-host/src/doctor.rs`.
- CLI integration: `crates/officina/tests/{hook,quota_doctor,readme,perf}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created against the addendum (PRD section 9 items 7 and 8; AD-180; D-572 to D-574)
