# Story 6.7: The entry point and the packer

**Status:** Approved

## Story

As the author whose worker must start from a clean clone and whose result must be packaged however the worker ended,
I want the container's first program to be fixed Officina code that clones, branches, links the mounts and runs `claude` under a timeout, and a separate packer that writes the branch's tip and bundle from a container with no network and no credential,
so that Officina adds nothing to the workspace but its flags, and a killed, stopped or out-of-memory worker is packaged exactly like one that exited.

## Acceptance Criteria

1. **`officina-entry run`** (AD-108, FR-6 step 3, FR-9). Reads `/work/.officina/in/plan.json` with core's `RunPlan` (an unknown schema exits non-zero) and prints one `EntryLine` JSON object per step on stderr, each at most 4 KiB (D-216): (1) `git clone --branch <base> /work/.officina/in/base.bundle /work/<project>`, `git checkout -b officina/<name> <base commit>`, `HEAD` equal to the base commit or exit non-zero; (2) each mount path appended to the clone's `.git/info/exclude`, then a symbolic link at its place in the clone, parents created, pointing at `/work/.officina/mounts/<i>`; (3) Slice B's memory copy when the plan holds one (story 6.21); (3a) for a `login` plan, each login file the plan names copied from `/work/.officina/login/` into `$CLAUDE_CONFIG_DIR` with mode `0600`, a missing file a non-zero exit with step `login_missing`, the content never printed (D-606); (4) `timeout --kill-after=30 <deadline_s> /opt/officina/bin/claude <argv>` in `/work/<project>`, stdin the prompt file, stdout and stderr inherited, the exit code recorded on stderr and returned. It adds no setting file, hook, `CLAUDE.md` or MCP server. Proof: `crates/officina-container/tests/entry_linux.rs::{clone_branch_and_head_equal_the_base, mounts_are_excluded_and_linked, login_files_are_copied_with_mode_0600_and_never_printed, a_missing_login_file_exits_non_zero, the_fake_claude_gets_the_plan_vector_and_the_prompt_on_stdin, a_timeout_exits_124, the_exit_code_is_passed_through, an_unknown_plan_schema_exits_non_zero, nothing_is_added_to_the_clone_but_links_and_excludes}` (real git in a temp folder, a fake `claude` script on `PATH` logging its argv and stdin; `/work` replaced by a temp root through a test-only argument).
2. **`officina-entry pack`** (AD-110, FR-10 as amended by amendment 8, D-219, D-248). Takes the branch and the base commit from its own arguments (`--branch`, `--base`, story 6.6's `Packer` vector), never from the volume's worker-writable `plan.json`; removes `/work/.officina/out` without following a symlink (a symlink or any other entry there is removed, not traversed) and recreates it as a folder (review F48); then writes `tip` (the commit `refs/heads/officina/<name>` points at, or nothing when the branch is gone), `work.bundle` only when `rev-list --count <base>..<tip>` is above 0 (`git bundle create` of the branch excluding the base commit; a bundle over 64 MiB deleted and reported `too_large`), and `manifest.json` (the entry point's recorded steps and exit code, the count of uncommitted files left in the clone), all untrusted. Git runs with `-c core.hooksPath=/dev/null -c core.fsmonitor=false`, `GIT_CONFIG_NOSYSTEM=1` and `GIT_CONFIG_GLOBAL=/dev/null`. Proof: `entry_linux.rs::{out_is_emptied_first, an_out_symlink_is_replaced, a_forged_plan_does_not_change_what_is_packed, a_commit_past_the_base_gives_tip_and_bundle, no_commit_gives_tip_and_no_bundle, a_branch_reset_behind_the_base_gives_its_tip_and_no_bundle, a_deleted_branch_gives_no_tip, an_oversized_bundle_is_deleted_and_reported, a_planted_hook_in_the_clone_never_runs}`.
3. **Status lines** (AD-108, AD-109). `EntryLine` is a core type (`officina-core::workshop::run`), parsed by the window with bounds; a line that does not parse is counted and skipped. Proof: core `crates/officina-core/src/workshop/run.rs::tests::{entry_lines_round_trip, an_oversized_entry_line_is_counted_not_kept}`.
4. **The Linux-only target runs** (D-224). `core-linux` runs `cargo test -p officina-container --test entry_linux` as a named step that fails when it reports zero tests run, so a `cfg` mistake that compiles the tests out turns the job red. The `run` and `pack` modules are `#[cfg(unix)]` (symlinks, `/proc`, `timeout`); on Windows `officina-entry` exits 64, as story 6.5 set up (review F23). Proof: the CI step; `crates/officina-container/tests/entry_windows.rs::officina_entry_exits_64_on_windows`.
5. **`auth-status` and `scan`** (D-608, D-609). `officina-entry auth-status` copies `/work/.officina/login/.credentials.json` into a config dir under `/tmp`, runs `/opt/officina/bin/claude auth status` with `CLAUDE_CONFIG_DIR` there, and prints exactly one line, `{"logged_in": <exit code 0>, "auth_method": <the JSON's authMethod, or null>}`; every other field of the JSON stays in the container. `officina-entry scan` builds core's `scan::needles` from every JSON string value of at least 16 characters in the stored credentials file (no field name assumed), reads framed records on stdin (a length-prefixed header naming the record's index, then its bytes, bounded per record and in total as AD-112's scope is), and prints one line per record with hits, `{"record": <index>, "ranges": [[start, end], ...]}`, never a value; no credentials file means no needles and a single `{"needles": 0}` line. Proof: `entry_linux.rs::{auth_status_prints_one_line_without_email_or_organisation, auth_status_reports_a_missing_login, scan_reports_ranges_never_values, scan_finds_every_form_of_a_seeded_value, scan_with_no_login_reports_zero_needles, an_oversized_record_is_refused}` (a fake `claude` on `PATH` printing a JSON with an email and an organisation; seeded values derived programmatically, D-237's forms); core `crates/officina-core/src/workshop/scan.rs::tests::{every_form_of_a_seeded_value_is_found, a_value_embedded_at_each_alignment_is_found_in_base64, a_wrapped_base64_form_is_found, replace_leaves_no_form, an_unrelated_text_is_not_found}` (moved from story 6.11).
6. **Documentation** (D-226). README: Officina's only additions inside a run are its flags and variables (and, for a `login` floor, the login's copy in the run's own config dir); what the workshop leaves out (FR-9: instructions above the project folder, the author's user-level configuration, `officina hook` and the relay gate, anything neither tracked nor mounted, Windows behaviour). Proof: review.

## Tasks

- [ ] `crates/officina-container/src/entry/{mod,run,pack}.rs`, `src/bin/officina-entry.rs` (AC 1, 2)
- [ ] `EntryLine` in core (AC 3)
- [ ] `crates/officina-container/tests/entry_linux.rs` with the fake `claude` script (AC 1, 2)
- [ ] The login copy step; `crates/officina-container/src/entry/{auth_status,scan}.rs`; `crates/officina-core/src/workshop/scan.rs` with its core tests (AC 1, 5)
- [ ] The named CI step (AC 4); README (AC 6)

## Dev Notes

- Requirements: FR-6 step 3, FR-7 (the vector runs as the plan says), FR-8 (`timeout`), FR-9, FR-10 as amended (amendment 8). Decisions: AD-108, AD-110 (the packer's half). Facts: the gate 1 entry (copy into the volume worked, or `in/` is a read-only bind mount, D-203). Risks: R50, R60. Epic decisions: D-200, D-216, D-219, D-224, D-229, D-248, D-606, D-608, D-609, D-610.
- Seams (D-200): none; `RunPlan` is story 6.6's. Core's `officina-core::workshop::scan` (`needles`, `find`, `replace`, with AD-112's forms and D-237's wrapped base64) is declared here with its core tests, since the `scan` subcommand is its first user and story 6.7 merges before 6.11 (amends D-237's "declared in 6.11", D-609); story 6.11 builds the run's needles with it.
- Waits for: the gate 1 and 3 entries. The login files to copy come from the plan (story 6.6's `LOGIN_FILES`, seeded with `.credentials.json`; story 6.26 adds `.claude.json` if the login entry requires it, D-610), so this story does not wait for the login entry.
- Why the scanner and the probe live in the container (D-608, D-609): the host never reads the login, so the needles and `auth status` can only be computed where the file is; what comes out is a boolean, an auth method and byte ranges.
- Why a packer instead of the entry point packaging at the end (AD-110): a stopped, killed or out-of-memory worker never reaches its last step, and std cannot catch the engine's stop signal.
- The clone's own configuration still applies in the packer: whatever it makes git run, it runs with no network and no credential, and the host verifies the output (story 6.13, D-218, D-219). The test with a planted hook shows only that the hooks path is neutralised; it does not claim more.
- `pack` with no commit past the base writes no bundle on purpose: `git bundle create` refuses an empty range, and AD-110's outcome table reads that case as `no_change` or `failed` on the host, never from the packer's word.
- The entry point and the packer run as uid 1000, the worker's user: nothing they write is believed on the host, and the host reads only D-229's allow-listed names from `out` (story 6.13). The packer writes only `tip`, `manifest.json` and `work.bundle` (and Slice B's `memory/`); anything else found in `out` after it ran is the clone's configuration at work, which the host refuses.

## Testing

- CI `core-linux`, no Docker: `crates/officina-container/tests/entry_linux.rs` (real git, fake `claude` script). Core unit: `crates/officina-core/src/workshop/run.rs`.
- Workstation: exercised for real by gate 3's window half (story 6.19).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the packer's branch and base from its arguments, `out` replaced without following a symlink (F48); `run` and `pack` are unix-only, Windows exits 64 (F23); the host's allow-list noted (F1)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
- 2026-10-29 the owner's answers: the login copy step, `auth-status` and `scan` (D-606, D-608, D-609)
