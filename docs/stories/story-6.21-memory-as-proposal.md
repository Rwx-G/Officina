# Story 6.21: Project memory in the workshop, returned as proposals

**Status:** Approved

## Story

As the author whose project auto memory shapes how sessions work,
I want a worker to start from a read-only copy of that memory and hand back what it changed as proposals I can paste,
so that the workshop works with what my sessions know, and Officina never writes a memory file itself.

## Acceptance Criteria

1. **The folder** (AD-115, FR-13, AD-7). With `[project.workshop] memory_name` (validated since story 6.2), the host folder is `<floor config dir>/projects/<memory_name>/memory`, the config dir being AD-7's resolution; no literal path is written in the floor. Missing, the launch is refused `memory_missing` (a new `LaunchRefusal` variant with its injection case, D-225); reached through a reparse point on its path from the config dir, or with a canonical path leaving it, `memory_missing` with detail `reparse_point` (D-236, review F11; defensive, since whether Docker Desktop follows a junction is not known), and doctor names it. Proof: core `crates/officina-core/src/workshop/memory.rs::tests::{the_folder_derives_from_the_config_dir, no_literal_path_is_accepted}`; host `crates/officina-host/src/workshop/launch.rs::tests::a_memory_folder_through_a_junction_is_refused`; `crates/officina/tests/injection.rs::every_section_family_and_refusal_has_a_case` (the new variant).
2. **In the run** (AD-115, AD-102, AD-108). The `Worker` vector mounts the folder read-only at `/work/.officina/memory` and sets `CLAUDE_CODE_PROJECT_DIR_NAME` to `memory_name`; the plan's memory copy makes `officina-entry run` copy it into `$CLAUDE_CONFIG_DIR/projects/<memory_name>/memory/` before `claude` starts; the battery's vector gains the same mount kind (D-210 keeps the comparison). Golden cases change once. Proof: the `Worker` case with memory in `tests/vectors/workshop.json`; `crates/officina-core/tests/workshop_vectors.rs::worker_and_battery_differ_only_where_they_must`; `crates/officina-container/tests/entry_linux.rs::memory_is_copied_before_claude_starts`.
3. **Out of the run** (AD-115, D-221). The packer copies the worker's memory folder into `out/memory/`: text files only, at most 64 files of 256 KiB each, the rest refused and counted. Story 6.13's `tar::read` admits `out/memory/` entries whose every segment passes D-229's rule (no backslash, drive prefix, colon, device name, trailing dot or space, case-insensitive duplicate) and keeps them in memory, never on disk (review F1). Core's `memory::proposals(host, worker)` lists added and changed files with a unified diff as `Untrusted<String>`, sorted by path; deletions are not proposed; a path that fails D-229's segment rule, is absolute, holds a `..` segment or is not UTF-8 is dropped (D-221). The proposals are scanned for the credential (story 6.14) and held in `texts_json`. Proof: `entry_linux.rs::{memory_out_is_bounded, binary_memory_files_are_refused}`; core `memory.rs::tests::{added_and_changed_files_are_proposed, deletions_are_not_proposed, a_dotdot_path_is_dropped}`; CLI `crates/officina/tests/workshop_collect.rs::a_token_in_a_memory_proposal_is_found`.
4. **The proposal** (AD-115, AD-121 as D-221 reads it, D-80). `copy_memory_proposal(run_id, index)` (this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer) returns the proposed text and the host path, built from the validated segments only, so no worker path can name a file outside the memory folder (`memory/..\..\settings.json` never displays `<config dir>/settings.json`, review F1); the frontend copies the text on the click (AD-85's pattern) and shows the path to paste it into; `MemoryProposals.svelte` in the review panel lists each proposal's path and diff as text. Officina never writes a memory file; proposals end with their package. Proof: CLI `crates/officina/tests/workshop_review.rs::{copy_takes_an_index_never_a_path, an_index_out_of_range_is_refused, copy_builds_the_host_path_from_validated_segments}` (one case per hostile segment form of `bundle.json`'s table); core `memory.rs::tests::every_hostile_segment_form_is_dropped`; Vitest `app/src/components/workshop.test.ts > memory proposals render as text and copy on click`; Playwright `app/e2e/review.spec.ts > a memory proposal is copied`.
5. **Zero write** (D-80, AD-125). A run with memory, its collection and a copied proposal leave the Claude config dir unchanged. Proof: `crates/officina/tests/quota_doctor.rs::a_memory_run_writes_nothing_under_the_config_dir`.
6. **Doctor** (AD-123 Slice B, D-228). Each declared memory folder present, and not reached through a reparse point. Proof: host `crates/officina-host/src/doctor.rs::tests::{a_missing_memory_folder_is_named, a_memory_folder_through_a_junction_is_named}`.
7. **Documentation** (D-226). README: `memory_name`, where `/memory` shows the name, proposals pasted by hand; `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/workshop/memory.rs`; `memory_missing` and its injection case (AC 1, 3)
- [ ] Vector mount, variable, plan copy, golden cases; entry point and packer (AC 2, 3)
- [ ] Proposals in collection and the scan (AC 3)
- [ ] `copy_memory_proposal`, `MemoryProposals.svelte` (AC 4)
- [ ] Zero-write case (AC 5); doctor (AC 6); README, `CHANGELOG.md` (AC 7)

## Dev Notes

- Requirements: FR-13, FR-2 (`memory_name`), FR-17 (memory proposals in the panel), FR-18 Slice B. Decisions: AD-7, AD-85, AD-102, AD-108, AD-115, AD-121, AD-123. Facts: S8 (`CLAUDE_CODE_PROJECT_DIR_NAME`, 2.1.234+). Epic decisions: D-80 (Epic 3); P-53, D-200, D-210, D-221, D-225, D-226, D-229, D-235, D-236.
- Seams (D-200): the review panel (story 6.19), the vectors (story 6.6), the entry point and packer (story 6.7), the scan (story 6.14); `quota_doctor.rs`.
- Waits for: Slice A (stories 6.2 to 6.20).
- The name is declared, not computed, because S8 says it is derived from the git repository without giving the rule (FR-13).
- Migration 3 holds proposals in `texts_json`; Slice B adds no column (AD-119).

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/memory.rs`, `crates/officina-core/tests/workshop_vectors.rs`; CLI with the fake engine `crates/officina/tests/{workshop_collect,workshop_review,injection,quota_doctor}.rs`; host `crates/officina-host/src/doctor.rs`; Vitest and Playwright.
- CI `core-linux`: `crates/officina-container/tests/entry_linux.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: memory names pass D-229's segment rule and the host path is built from validated segments (F1); a memory folder through a reparse point is refused and named (F11)
