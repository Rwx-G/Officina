# Story 5.22: The permission router's core and hook path (Slice D, v0.5.x)

**Status:** Review

## Story

As the author whose background sessions sometimes stop on a Claude Code permission prompt that only an attach can answer,
I want `officina hook` able to hold such a prompt while a decision only my window can write is awaited, and to give Claude Code back nothing at all whenever the window is absent, slow or silent,
so that the routing story can put the prompt on a card later, and until then, and whenever anything fails, Claude Code's own prompt appears exactly as it does today (AD-181).

## Acceptance Criteria

1. **Core types and the output** (AD-181, D-491, D-580; hooks reference, E18). `officina-core::relay::prompts` declares: `PromptRequest` (id, session id, `CLAUDE_PID` when the environment has it, tool name, SHA-256 of the canonical `tool_input` (sorted keys, no whitespace), `tool_input` capped at 4 KiB as `Untrusted<String>`, `permission_mode`, `requested_at_ms`, `deadline_ms`); `PromptDecision` (id, session id, input hash, `behavior` `allow` or `deny`, `decided_at_ms`); `PromptOutput`, which serializes only the documented `PermissionRequest` output: `hookSpecificOutput.hookEventName` and `decision.behavior` (`allow`, or `deny` with one constant message). `PromptOutput` has no field for `updatedPermissions`, `updatedInput` or `interrupt`, so Officina can never ask Claude Code to write its settings. Proof: core `crates/officina-core/src/relay/prompts.rs::tests::{the_output_holds_only_behavior_and_the_constant_message, the_input_hash_is_canonical, the_input_is_capped_on_a_character_boundary}`; the vector set `tests/vectors/prompts.json` (formatter-exempt; payloads with extra, missing and oversized fields) through `crates/officina-core/tests/prompts_vectors.rs::{payload_cases_match, the_output_schema_is_pinned}`, the second comparing the serialized schema's property names, at every depth, with the pinned set.
2. **The liveness marker** (D-579, D-511). The collector writes `<home>/window/alive.json` (`pid`, `started_at_ms`, `routes_prompts`) at start, refreshes its modification time every 5 s from its tick, and removes it on a clean exit; `routes_prompts` is `false` in this story (the routing story turns it on). The hook reads it with one `stat` and one read. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{the_window_writes_its_marker_and_refreshes_it, a_clean_exit_removes_the_marker}`; core `prompts.rs::tests::{a_missing_stale_or_non_routing_marker_is_no_window, a_fresh_routing_marker_is_a_window}` (staleness from explicit times).
3. **The hook, no window** (AD-181, D-511, D-579; AD-16, AD-28). `officina hook` on a `PermissionRequest` payload, when the marker says "no window", exits 0 at once with no output and writes nothing. A payload that does not parse, or has no session id, does the same. It never exits 2 (Claude Code ignores exit 2 here, and the rule of `officina-rust.md` stands). Proof: CLI `crates/officina/tests/hook.rs::{permission_request_without_a_window_prints_nothing_and_writes_nothing, a_malformed_permission_request_prints_nothing, the_hook_never_exits_2_on_permission_request}`; `crates/officina/tests/perf.rs::permission_request_without_a_window_is_within_the_hook_budget` (release, ignored run).
4. **The hook, with a window** (AD-181, AD-33's pattern). With a fresh routing marker, the hook writes `relay/prompts/<id>.json` (AD-10's atomic write) and waits by `stat` for `<id>.decided.json` or `<id>.declined`, reading the marker again each second: a decision bound to its session id and input hash, unexpired and unconsumed, is consumed by rename (single use) and printed as AC 1's output; a declined request, a stale marker, a decision for another session or hash, or no answer before the deadline (580 s, inside the registration's 590 s timeout) ends with exit 0 and no output, so Claude Code's own prompt takes over; the request file is removed on every exit. The wait is a host function taking its deadline, poll interval and clock as arguments. Proof: host `crates/officina-host/src/relay/prompts.rs::tests::{a_matching_decision_is_consumed_once, a_decision_for_another_session_or_hash_is_ignored, a_stale_marker_ends_the_wait, a_declined_request_ends_the_wait, the_deadline_ends_the_wait_with_no_decision}` (the last with a 200 ms deadline); CLI `hook.rs::{an_allow_written_before_the_wait_is_printed, a_deny_prints_the_constant_message, the_request_file_is_removed_on_every_exit}`.
5. **The authority writer** (AD-181, AD-21, D-157's pattern). Under `window-authority`, `authority::decide_prompt(home, id, Allow | Deny | Decline, now_ms)`: refuses an unknown or expired request; writes `<id>.decided.json` or `<id>.declined` with `create_new`, so the first writer wins and a second is refused `already_decided`; journals `prompt_decided` (session, tool, input hash, decision), never the input. The collector journals `prompt_routed` (session, tool, input hash) the first time it sees a request, and deletes request and decision files older than 24 h (AD-33's cleanup). `officina.exe` holds no such writer (P-5). Proof: host `crates/officina-host/tests/authority.rs::{a_prompt_decision_is_written_once, an_expired_prompt_cannot_be_decided, decline_writes_no_decision}`; core `crates/officina-core/src/relay/journal.rs::tests::{prompt_routed_and_decided_never_carry_the_input}`; the CI step "officina holds no authority"; CLI-crate collector test `watch.rs::prompt_files_older_than_24_h_are_removed`.
6. **Nothing registered yet, nothing written to Claude Code** (D-580, D-530, D-80). Doctor's hooks block does not register `PermissionRequest` in this story (the routing story does, with `routes_prompts`); the hook through no window, allow, deny, decline and the deadline joins the fingerprint test. Proof: host `crates/officina-host/src/doctor.rs::tests::permission_request_is_not_registered_yet`; `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
7. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added (internal, nothing user-visible): the permission router's hook path, inert until the routing story. Proof: review.

## Tasks

- [x] `officina-core::relay::prompts`: types, canonical hash, cap, output, marker reading; `tests/vectors/prompts.json` and its tests (AC 1, 2)
- [x] Collector: the marker's write, refresh and removal; `prompt_routed`; the 24 h cleanup (AC 2, 5)
- [x] `officina hook`'s `PermissionRequest` branch; the wait function in `officina-host::relay::prompts` (AC 3, 4)
- [x] `authority::decide_prompt`; journal kinds (AC 5)
- [x] Doctor's non-registration case; zero-write (AC 6)
- [x] Update `CHANGELOG.md` (AC 7)

## Dev Notes

- Requirements: PRD section 9 item 10 (AD-181), R108. Decisions: AD-181 as amended by epic-4-decisions.md D-511, AD-10, AD-16, AD-21, AD-25, AD-28, AD-33. Facts: E14 (`CLAUDE_PID` in hooks), E18 (a hook that fails or times out renders no decision). Epic decisions: D-80, D-157, D-491, D-511, D-530, D-578 to D-581.
- Entry (D-581): after story 5.19 closes v0.5. This story depends on none of SD-1 to SD-7: what it builds is either documented (the output fields, the timeouts, failure falling back to the native prompt) or decided (the marker, D-579). SD-5 may show that a payload lacks `CLAUDE_PID` or carries `cwd` in another form: the request records absent fields as absent, and binding uses the session id and the input hash only.
- Why the hook never decides alone: every path but a window-written decision ends in no output, so a fault anywhere is Claude Code's own prompt, never an approval (the opposite of the gate's `onFailure: "block"`, which is why the routing story registers this hook with `onFailure` unset).
- `Untrusted`: the capped input is shown on the routing story's card as text only and never parsed into anything that runs; the hook never echoes it.
- No employer content in `prompts.json`: placeholder tools, commands and paths.

## Testing

- Core unit: `crates/officina-core/src/relay/{prompts,journal}.rs`; core integration: `crates/officina-core/tests/prompts_vectors.rs`.
- Host: `crates/officina-host/src/relay/prompts.rs`, `src/doctor.rs`, `tests/authority.rs`.
- CLI integration: `crates/officina/tests/{hook,watch,quota_doctor,perf}.rs`.

## Dev Agent Record

### Debug Log

- Implemented on the lead's parallel track before 5.19 closes v0.5 (D-900, to confirm), rebased on story 5.21.
- `approvals::consume`'s claim (a new `<id>.consumed.json`) replaces AC 4's "rename", which is not single-winner on Windows (D-902); the authority writes a decision through a temporary file hard-linked to its name, `create_new` semantics without a half-written file.
- The hook's wait in the CLI tests is cut through `OFFICINA_TEST_PROMPT_WAIT_MS`, read only with `test-seams` (D-906); the local gate also checks the release `officina.exe` holds the name nowhere.
- `perf.rs` failed once in a first run on the shared machine for the existing `hook_latency_p95` and the new case alike (own cost about 40 ms each, other gates running); rerun in the gate below, both well inside the budget.
- Gate, Windows, on this story's code (rebased on `3c681b9`), every job of `.github/workflows/ci.yml` replayed: fmt; frontend install and build; clippy in both CI feature sets; `cargo test --workspace --features officina-core/ts` 1,672 passed, 0 failed, 15 ignored (TypeScript drift green); the spawn tests alone; `perf.rs` in release with CI's budgets, 6 passed (PermissionRequest without a window own cost 1.1 ms, hook own cost 8.9 ms, gate own cost 8.7 ms, in-process p95 3.2 ms); the lexer differential; `approvals_stress`; `registry_crash`; `proposals_crash` and the `officina mcp` kill; officina holds no authority (and no test seam) with its controls; registry dependencies; no bundled roots; e2e fixtures from the real CLI; `pnpm check`, `lint`, Vitest 350 passed; Playwright `--repeat-each=3` 1,326 passed, 6 skipped; the mutants job (each mutant caught, control green); `cargo deny`; `pnpm audit`. The `core-linux` job ran in a `rust:1-bookworm` container (toolchain from `rust-toolchain.toml`, source read-only): fmt and clippy clean; its first run failed `host::relay::prompts::tests::the_deadline_ends_the_wait_with_no_decision`, which measured the 200 ms deadline from after the request file was written (the container syncs slower), a test fault fixed to assert the wait never ends before `deadline_ms`; the rerun with `--no-fail-fast` passed 1,497 and failed once `watch.rs::a_re_instructed_auto_archived_session_counts_as_a_flip` (no event within 10 s, outside this story), which passed three reruns of `watch.rs` (67 each); the spawn tests alone passed.

### Completion Notes

Decisions D-900 to D-906 (D-900 and D-903 to confirm). The path is inert: no window routes prompts (`ROUTES_PROMPTS` false in the collector) and doctor registers no `PermissionRequest`, so Claude Code's own prompt appears as before. The tests proving each AC:

1. Core types and output: `crates/officina-core/src/relay/prompts.rs::tests::{the_output_holds_only_behavior_and_the_constant_message, the_input_hash_is_canonical, the_input_is_capped_on_a_character_boundary, a_decision_answers_its_own_request_by_its_deadline, answers_map_to_behaviours_and_ids_are_safe_names}` and the module's doctests; `tests/vectors/prompts.json` through `crates/officina-core/tests/prompts_vectors.rs::{payload_cases_match, the_output_schema_is_pinned}`.
2. The marker: `prompts.rs::tests::{a_missing_stale_or_non_routing_marker_is_no_window, a_fresh_routing_marker_is_a_window}`; `crates/officina/tests/watch.rs::{the_window_writes_its_marker_and_refreshes_it, a_clean_exit_removes_the_marker}`; the app removes it on `RunEvent::Exit` (D-904).
3. No window: `crates/officina/tests/hook.rs::{permission_request_without_a_window_prints_nothing_and_writes_nothing, a_malformed_permission_request_prints_nothing, the_hook_never_exits_2_on_permission_request}`; `crates/officina/tests/perf.rs::permission_request_without_a_window_is_within_the_hook_budget`; `crates/officina-host/src/relay/prompts.rs::tests::without_a_window_route_writes_nothing`.
4. With a window: `crates/officina-host/src/relay/prompts.rs::tests::{a_matching_decision_is_consumed_once, a_decision_for_another_session_or_hash_is_ignored, a_stale_marker_ends_the_wait, a_declined_request_ends_the_wait, the_deadline_ends_the_wait_with_no_decision}`; `hook.rs::{an_allow_written_before_the_wait_is_printed, a_deny_prints_the_constant_message, the_request_file_is_removed_on_every_exit}` (D-906 for the first's reading).
5. The authority: `crates/officina-host/tests/authority.rs::{a_prompt_decision_is_written_once, an_expired_prompt_cannot_be_decided, decline_writes_no_decision}`; `crates/officina-core/src/relay/journal.rs::tests::prompt_routed_and_decided_never_carry_the_input`; `watch.rs::{a_waiting_prompt_is_journaled_routed_once, prompt_files_older_than_24_h_are_removed}`; `crates/officina-host/src/relay/prompts.rs::tests::prune_takes_old_router_files_only`; CI's "officina holds no authority" step, replayed.
6. Nothing registered, nothing written: `crates/officina-host/src/doctor.rs::tests::permission_request_is_not_registered_yet`; `crates/officina/tests/doctor.rs::the_printed_hooks_block_registers_no_permission_request`; `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` runs the hook through no window, allow, deny, decline and the deadline.
7. Documentation: `CHANGELOG.md` `[Unreleased]` Added.

## File List

- `crates/officina-core/src/relay/prompts.rs` (new), `relay/mod.rs`, `relay/journal.rs`, `relay/view.rs`, `relay/approvals.rs`
- `crates/officina-core/tests/prompts_vectors.rs` (new), `tests/vectors/prompts.json` (new)
- `crates/officina-host/src/relay/prompts.rs` (new), `relay/mod.rs`, `relay/home.rs`, `relay/approvals.rs`, `relay/authority.rs`, `watch.rs`, `doctor.rs`
- `crates/officina-host/tests/authority.rs`
- `crates/officina/src/main.rs`; `crates/officina/tests/hook.rs`, `watch.rs`, `doctor.rs`, `quota_doctor.rs`, `perf.rs`
- `app/src-tauri/src/main.rs`
- `CHANGELOG.md`, `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this story

## Change Log

- 2026-10-28 created for Slice D (AD-181; D-578 to D-580): the parts whose entry questions are answered
- 2026-10-29 implemented, inert until the routing story; decisions D-900 to D-906; status review
