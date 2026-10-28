# Story 6.9: The hook-program inventory

**Status:** Approved

## Story

As the author whose project guards are `PreToolUse` hooks that call `python` and scripts,
I want each image checked, with no network, for every program my mounted hooks and my project's `.mcp.json` name, and a launch refused when a `PreToolUse` guard's program is missing,
so that under bypass a guard that cannot start never silently becomes a guard that is gone.

## Acceptance Criteria

1. **Extraction** (AD-118, D-211). Core's `workshop::inventory::extract(settings_files, mcp_json, tracked)` lists each hook (event, matcher, command text) from the mounted `settings.json` and `settings.local.json` of a mounted `.claude`, and from the base commit's `.claude/settings.json` and `.claude/settings.local.json` when they are tracked (`git show <base>:...` under `GitRule::Workshop`, bounded as the mounted ones), since a project that shares its settings through git cannot mount `.claude` (review F12); and each stdio MCP server's command from the base commit's `.mcp.json`; a hook program that is a path under `$CLAUDE_PROJECT_DIR` outside every mount is marked tracked, and the host passes its presence at the base commit and its first line (at most 256 bytes). `inventory_key` is the SHA-256 of the list, the tracked files' blob ids and the image id. Every input is bounded (settings files at most 256 KiB, `.mcp.json` at most 256 KiB, at most 256 commands). Proof: section `inventory` of `tests/vectors/workshop.json` through `crates/officina-core/tests/workshop_vectors.rs::{inventory_extraction_cases_match, the_key_changes_with_a_tracked_script, tracked_settings_hooks_are_listed}`; core `crates/officina-core/src/workshop/inventory.rs::tests::{oversized_inputs_are_refused_not_truncated, a_tracked_script_absent_at_the_base_is_missing, a_tracked_pretooluse_hook_is_checked}`.
2. **Resolution in the image** (AD-118, D-211). `officina-entry inventory` reads the list on stdin, creates the mount links in its tmpfs `/work/<project>`, finds each command's program with core's `relay::shell` lexer (the gate's command-position rules), expands `$CLAUDE_PROJECT_DIR` and nothing else, resolves the program on `PATH` or as a path, and a script's interpreter from its first line read raw (`#!/usr/bin/env x` resolves `x`; a carriage return in the line gives `missing` with detail `crlf_shebang`, since `#!/usr/bin/env bash\r` exits 127 at run time, review F12; the host passes a tracked script's first line raw too); one result per command: `found` (with the path), `missing` (with its detail) or `dynamic`. Proof: `crates/officina-container/tests/entry_linux.rs::{a_python_hook_is_found_when_python_exists, a_missing_program_is_missing, an_env_shebang_resolves_its_interpreter, a_crlf_shebang_is_missing, a_variable_other_than_the_project_dir_is_dynamic, a_mounted_script_resolves_through_its_link}`.
3. **When and where** (AD-118, D-205, D-212). After each build, and at launch when the key changed, thread `workshop-build` (or `workshop` at launch) runs the `Inventory` vector (story 6.6), attached, no network, 60 s, refused while the stop flag is set; results go to `workshop_inventory` per image and project; the image's `ENV` names, from `image inspect`, were recorded at build (story 6.8) for the battery. Fake engine mode: attached `start` of role `Inventory` prints fixture results. Proof: CLI `crates/officina/tests/workshop_build.rs::{an_inventory_follows_each_build_per_project, a_changed_hook_reruns_the_inventory_at_launch, an_unchanged_key_does_not_rerun, no_inventory_runs_while_stopped}`.
4. **Consequences** (AD-118, FR-6, D-203). A `PreToolUse` hook whose program is `missing` makes `hook_program_missing` (story 6.11's refusal reads it); `dynamic` and a missing program of any other event or MCP server are warnings on the card; `hook_failed` still applies at run time (story 6.12) unless the gate 5 entry removed it. Proof: core `inventory.rs::tests::{only_a_missing_pretooluse_program_refuses, other_events_and_mcp_servers_warn}`.
5. **Doctor** (AD-123, FR-18, D-211). Per project: the programs of mounted and tracked hooks and of `.mcp.json` against the inventory, `PreToolUse` ones as failures; each mounted settings file holding an `env` block or `apiKeyHelper` named as a warning (names only: the worker can read them); a mounted hook script with CRLF line endings warned (review F12). Each check is a variant of the doctor's workshop check enum with its level (D-228). Proof: host `crates/officina-host/src/doctor.rs::tests::{a_missing_pretooluse_program_fails, an_env_block_in_a_mounted_settings_file_warns_by_name, a_crlf_hook_script_warns}`.
6. **Documentation** (D-226). README: the inventory, what `missing`, `dynamic` and a refused launch mean, and that a hook written on Windows runs on Linux in the workshop (FR-9). Proof: review.

## Tasks

- [ ] `crates/officina-core/src/workshop/inventory.rs`; `inventory` section of `workshop.json` (AC 1, 4)
- [ ] Host side: settings and `.mcp.json` reads, tracked scripts at the base commit (`GitRule::Workshop`) (AC 1)
- [ ] `crates/officina-container/src/entry/inventory.rs`; `entry_linux.rs` cases (AC 2)
- [ ] `crates/officina-host/src/workshop/inventory.rs`: runs, records; fake engine mode (AC 3)
- [ ] Doctor (AC 5); README (AC 6)

## Dev Notes

- Requirements: FR-1 (inventory), FR-6 (`hook_program_missing`), FR-9, FR-18. Decisions: AD-118, AD-123. Facts: E2 (a missing program is a non-blocking error), E12 (a `PreToolUse` deny blocks in every mode), the gate 4 entry (gate 4's first half), the gate 5 entry. Risks: R57. Epic decisions: P-50, D-200, D-203, D-205, D-211, D-212, D-223.
- Seams (D-200): `relay::shell` (2.3), compiled into the entry point for the musl target (story 6.5 proved it builds); `host::git` (D-124) for the base commit's `.mcp.json` and tracked scripts.
- Waits for: the gate 4 entry (first half).
- Why the host checks tracked scripts (D-211): the inventory container has no clone, so a hook such as `"$CLAUDE_PROJECT_DIR/scripts/guard.sh"` in a tracked folder would read `missing` and refuse every launch; the host reads the file at the base commit, which is what the clone will hold.
- The inventory runs the image's own `PATH` and resolution, which is the worker's; it runs no hook.
- The settings files are the author's own; `env` and `apiKeyHelper` are names to warn about, never values to print.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/inventory.rs`, `crates/officina-core/tests/workshop_vectors.rs`; CLI with the fake engine `crates/officina/tests/workshop_build.rs`; host `crates/officina-host/src/doctor.rs`.
- CI `core-linux`: `crates/officina-container/tests/entry_linux.rs` (inventory cases).
- Workstation: gate 4's second half (story 6.20) runs real hooks against real inventories.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: tracked `.claude` settings join the inventory's input; a CRLF shebang is `missing` (`crlf_shebang`) and doctor warns on CRLF hook scripts (F12)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
