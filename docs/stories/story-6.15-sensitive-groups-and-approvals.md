# Story 6.15: Sensitive groups and their approvals

**Status:** Approved

## Story

As the author who will run an imported branch with my own identity,
I want every path that configures a tool, a build, CI, my editor or Claude Code itself grouped, its hunks shown inline, and Import held until I approve each group against the exact hunks I saw,
so that the changes that run on my machine get a second look, and an approval can never cover hunks I did not see.

## Acceptance Criteria

1. **Groups and patterns** (AD-113, FR-11, D-246). `sensitive::SensitiveGroup` (a `closed_enum!`, with a `binary` group besides FR-11's) and `SENSITIVE_PATTERNS`, seeded once from FR-11's list extended by D-246 (review F27: Rust toolchain files, pnpm and yarn hooks, dev containers, workspace and IDE folders, each in its family) and from then on the only copy; patterns compile to anchored expressions with core's `regex`: a pattern without `/` matches the last segment, a pattern with `/` any trailing run of segments, `**` spans segments, `*` stays within one, case-insensitively; every added, changed and deleted path and both sides of a rename are matched. Modes: a new symbolic link (120000) or an executable bit added gives `file_modes`, and so does a path segment shaped as an 8.3 short name (`~` followed by a digit), defensive, since git's refusal to write through such an alias over an ignored file is believed, not verified (review F50); a blob git reports as binary (`diff --numstat` shows `-`) gives `binary` (review F27); a gitlink (160000) `git`; `officina` is the floor file's path relative to the project when it lies inside; `project` is the project's `sensitive_paths`; with `review = "all"` every changed path is its own `file` group, keyed by its index in the package's sorted list. Proof: `tests/vectors/sensitive.json` (formatter-exempt) through `crates/officina-core/tests/sensitive_vectors.rs::{every_pattern_has_a_matching_and_a_non_matching_case, every_group_has_a_case, renames_match_both_sides, modes_give_their_groups, review_all_makes_one_group_per_file}` (the first iterates `SENSITIVE_PATTERNS`, deriving case variants and depth programmatically); `sensitive_vectors.rs::{an_8_3_alias_segment_gives_file_modes, a_binary_blob_gives_binary}`.
2. **Hunks** (AD-113, D-219, D-239). A group's hunks are computed on thread `workshop`, per file, from the old and new blob ids `diff --raw -z` reported (`git diff --no-ext-diff --no-textconv <old> <new>` under `GitRule::Workshop`), never from a path argument; `open_review` (story 6.13) caches them per run; a group whose hunks exceed 1 MiB cannot be approved in the panel (its package can only be discarded); `hunks_sha256` is the SHA-256 of the raw bytes, never of their rendering (story 6.19 escapes format characters for display, D-245). Proof: CLI `crates/officina/tests/workshop_review.rs::{hunks_come_from_blob_ids, a_path_starting_with_a_dash_never_reaches_git, an_oversized_group_cannot_be_approved}` (crafted packages from real git, D-23; the second points `OFFICINA_GIT_BIN` at the logging git and asserts no argument holds the path, review F38).
3. **Approval** (AD-113, FR-11 as amended by amendment 9, D-28, D-239, D-249). `approve_sensitive(run_id, group_key, hunks_sha256)` (pointer only; this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer): the collector supplies the stored state, the comparison runs against the hunks `open_review` cached for the run, recomputed on thread `workshop` when the cache is absent (a window restart), and the answer returns as an `AuthorRequest` without deadline (review F18); a differing SHA-256 is refused `hunks_changed`, otherwise the approval is stored in `workshop_approval` and `sensitive_approved` journaled. An approval holds for one package. Import readiness is state `review` and every group approved. Proof: CLI `workshop_review.rs::{an_approval_binds_to_the_hunks_displayed, a_stale_hash_is_refused, readiness_needs_every_group, an_approval_after_a_restart_recomputes_the_hunks}`; `crates/officina/tests/watch.rs::the_collector_never_spawns_docker` extended: the logging git sees no call from the collector thread while a group is approved; core `crates/officina-core/src/relay/journal.rs::tests::sensitive_approved_round_trips`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
4. **The review view gains groups** (AD-121). `ReviewView` lists files with their groups and each group with its hunks, `hunks_sha256` and approval state, and the import readiness with its reasons. Proof: core `crates/officina-core/src/workshop/review.rs::tests::{groups_carry_their_hash_and_state, readiness_names_unapproved_groups}`.
5. **The notice** (AD-113, FR-11). From the journal, a template with 10 or more verdicts where one group appears in more than half of its packages gets a notice (shown by story 6.18's per-template table). Proof: core `crates/officina-core/src/workshop/view.rs::tests::{the_notice_needs_10_verdicts, the_notice_needs_more_than_half}`.
6. **Documentation** (D-226). README: sensitive groups direct attention and are not the boundary; review an imported branch as a pull request from a stranger; the families described and `SENSITIVE_PATTERNS` named, never listed; `review = "all"` for infrastructure repositories; `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/workshop/sensitive.rs`; `tests/vectors/sensitive.json`; `sensitive_vectors.rs` (AC 1)
- [ ] Hunks from blob ids in `crates/officina-host/src/workshop/verify.rs` (AC 2)
- [ ] `approve_sensitive`, readiness, journal (AC 3); `ReviewView` groups (AC 4)
- [ ] The notice (AC 5); README, `CHANGELOG.md` (AC 6)

## Dev Notes

- Requirements: FR-11 as amended (amendment 9), PRD section 7 Review cost. Decisions: AD-24's pattern (hash of what was shown), AD-113, AD-121. Risks: R59. Epic decisions: D-28 (Epic 2); P-50, D-200, D-219, D-226.
- Seams (D-200): the collector's `AuthorRequest` (3.5); `regex` already in core; `JournalKind` (2.5).
- Waits for: story 6.13 merged.
- A `file` group's key is an index, so no worker text becomes a key; worker paths are matched and displayed, never passed to git (D-219).
- The constant is the list; this story and the README describe the families and never restate the patterns (derived, not transcribed).
- The seed's extension (D-246) is an owner decision taken as the review recommends and listed for his confirmation (P-56); it departs from FR-11's list (P-51). The reason: each added file runs code at the author's next ordinary command in his Rust and Svelte projects (a `path` toolchain makes `cargo` run binaries from the repository; `.pnpmfile.cjs` runs on `pnpm install`), and no FR-11 group covered a committed binary.
- Further epic decisions: D-239, D-245, D-246, D-249; P-56.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/{sensitive,review,view}.rs`, `relay/journal.rs`, `crates/officina-core/tests/sensitive_vectors.rs`; CLI with the fake engine and real git `crates/officina/tests/workshop_review.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: hunks on thread `workshop`, approvals compared against `open_review`'s cache (F18); the hash over raw bytes, rendering escaped in 6.19 (F26); the seed extended and a `binary` group (F27); the dash-path test through the logging git (F38); an 8.3 alias trigger in `file_modes` (F50); command plumbing (F33)
