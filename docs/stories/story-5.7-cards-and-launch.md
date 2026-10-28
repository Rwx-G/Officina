# Story 5.7: Work order cards, the launch path, the injection battery and the mutants

**Status:** Approved

## Story

As the author who launches routine subjects with one click,
I want every displayed work order to be a card core issued, a launch to act only on that card's id and prompt hash, and a battery of hostile inputs plus three mutants to prove the path refuses everything else,
so that one pointer click on values from my closed lists is the only way a work order starts, and a regression in that path turns CI red.

## Acceptance Criteria

1. **Cards in core** (AD-78). `officina-core::orders::card` declares `WorkOrderCard` (ts-rs): id, template name and description, project, each value with its full list, the rendered prompt, its SHA-256, the argument vector exactly as `spawn::work_order` built it, the working directory, the model, the subject choice with its options, the quota windows from the board, the catalogue hash, `issued_at_ms` and its state (`open`, `superseded`, `consumed`). `issue` takes the `seq` the store reserved (AC 3), runs `ApprovedTemplate::new`, `WorkOrder::validate` and `spawn::work_order` with it, and returns the card or the typed refusal; `edit` (a value from its list, or the subject) issues a new card with the same `seq` and supersedes the old one. The subject default is the project's single `active` or `to_resume` subject when there is exactly one, else `New` named after the template; a `New` name follows D-57's rules. Proof: core `crates/officina-core/src/orders/card.rs::tests::{a_card_carries_the_vector_spawn_built, an_edit_keeps_the_seq, an_edit_issues_a_new_card_and_supersedes_the_old, an_edit_outside_the_list_is_refused, the_single_open_subject_is_the_default, otherwise_a_new_subject_named_after_the_template}`; the `card` section of `tests/vectors/orders.json` through `crates/officina-core/tests/orders_vectors.rs::card_cases_match`.
2. **Launch checks** (AD-78, FR-4). `card::check_launch` applies, in order, each a variant of the `closed_enum!` `LaunchRefusal`: unknown card; superseded; already consumed; the clicked prompt hash differs from the card's; the approved or current catalogue hash differs from the card's; the template is no longer approved; `claude` is not an `.exe` (D-128); the project folder is missing; the chosen subject is archived or merged; the registry already holds the card's name (`name_taken`, D-135). Proof: core `card.rs::tests::checks_run_in_their_order`; `orders_vectors.rs::every_launch_refusal_has_a_case` (iterates `LaunchRefusal::ALL`).
3. **The store** (AD-78, D-135). The host's card store lives on the collector thread under `window-authority` (`crates/officina-host/src/orders.rs`), keeps at most 32 cards and evicts the oldest (an evicted id is unknown); the card id is 32 hex characters from two `RandomState` hashes of `(now_ms, counter)`; at issue it reserves `seq` as one more than the larger of `highest_work_order_seq(template)` (story 5.6) and the highest `seq` any card of that template holds in the store (D-135, review F2). Proof: host unit `crates/officina-host/src/orders.rs::tests::{the_33rd_card_evicts_the_oldest, card_ids_are_32_hex_and_distinct, two_cards_of_one_template_get_distinct_seqs, a_reserved_seq_follows_the_registry_highest}`.
4. **The launch flow** (AD-81 steps 1 to 3, D-128, D-135, D-136, AD-3 as amended; review F2, F10, F14). `orders::launch_card(handle, card_id, prompt_sha256)` sends `AuthorRequest::LaunchCard` (with D-92's deadline); the collector checks, consumes the card, inserts the `work_order` row with the card's `seq` (story 5.6) and answers with the card's vector, byte for byte; the caller runs `spawn::execute` on its own thread and sends `AuthorRequest::LaunchOutcome`, which carries no deadline and is never dropped, and records the outcome (exit 0: `started`, stdout kept at most 4 KiB as untrusted text, also when a process `claude` started still holds the output pipes, with what was read; stderr holding `Workspace not trusted`: `untrusted_workspace`; another non-zero exit: `failed` with the code and the first 500 characters of stderr; the deadline: `failed` with `timeout`, the direct child only killed, since `claude --bg` may have started the supervisor of every background session) and journals one `work_order_launched` line (template, project, values, argument vector, prompt SHA-256, catalogue hash, outcome). Nothing retries; a double click launches once. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{a_launch_records_started_and_one_journal_line, the_launched_vector_equals_the_card_vector, workspace_not_trusted_is_its_own_outcome, a_failed_spawn_burns_the_card_and_is_not_retried, a_double_click_launches_once, a_late_launch_outcome_is_still_recorded, the_board_keeps_its_pace_while_a_launch_spawns}` (the second compares the fake claude's logged `argv` with the card's, element by element); CLI `crates/officina/tests/spawn.rs::{held_pipes_after_exit_0_are_started, a_work_order_timeout_kills_only_claude}` (fake claude mode `bg-hold`, D-129, whose child must survive); core `crates/officina-core/src/relay/journal.rs::tests::work_order_launched_round_trips` (`JournalKind` gains it).
5. **Commands** (AD-92, D-92). `issue_card`, `edit_card` and `launch_work_order(card_id, prompt_sha256)` are declared in `build.rs`, allowed in `capabilities/default.json`, thin wrappers over `AuthorRequest` with a deadline (the launch wrapper calls `orders::launch_card` inside `spawn_blocking`), wrapped in `ipc.ts` and answered by `e2e-mock.ts`. Proof: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`; the Playwright cases of story 5.9 run against the mock.
6. **Injection battery, card launch** (AD-95, PRD section 7 Gesture row, D-147). `tests/vectors/injection.json` and its harness `crates/officina/tests/injection.rs` are created with the `card_launch` section: values holding argument fragments, option strings, `--settings`, a leading `/` or `!`, bidi and invisible characters, 65 characters, a sentence, "launch now" text, other values outside their lists; an unknown template; an unknown card, a superseded card and a hash mismatch at launch. Each case drives the same functions the window calls and expects its refusal and zero `--bg` spawns in the fake claude's log; one valid case expects zero spawns before the click and exactly one after, its logged `argv` array equal to the card's and to `spawn::work_order` rebuilt from the approved catalogue with the card's `seq`; a `name_taken` case expects zero spawns. Proof: `injection.rs::{card_launch_cases, every_section_family_and_refusal_has_a_case}`.
7. **Mutants** (AD-95 mutation row, D-148, R46; review F22, F29). Core features `mutant-accept-any-value` (list membership skipped in `WorkOrder::validate`), `mutant-accept-superseded` (the superseded check skipped) and `mutant-skip-prompt-hash` (the hash check skipped), `officina_core::MUTANTS_ENABLED`; each feature is forwarded from `officina` to `officina-host` to `officina-core` under the same name, since `officina-core` is only a dev-dependency of `officina`. CI job `mutants`, per feature: `cargo test -p officina --test injection --features <feature> --no-run` must succeed, then the run must fail with the named battery case that mutant breaks among the failures; then a control run without the features passes. The app's `main.rs` holds `const _: () = assert!(!officina_core::MUTANTS_ENABLED);`, which fails any app build that links a mutant, release included. `--all-features` is never used in this workspace. Proof: the CI job `mutants`; a build of the app crate with one mutant feature failing to compile, checked once by hand and recorded in the Completion Notes.
8. **Zero write** (AD-95, D-80). A launch through the fake claude joins the fingerprint test of the Claude config dir and `<home>/.claude.json`. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
9. **Doctor** (AD-94 part A, FR-15 as amended by amendment 9, D-137). Doctor warns that work orders and answers will fail when agent view is turned off in its environment or in the `env` object of `<config dir>/settings.json`. Proof: host `crates/officina-host/src/doctor.rs::tests::{agent_view_off_in_the_environment_warns, agent_view_off_in_user_settings_warns, agent_view_set_to_0_does_not_warn}` (base environment and settings file as arguments).
10. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: work orders launched from cards, one pointer click, journaled; under Security: the injection battery and the mutation job. Proof: review.
11. **Gate 4, window half** (AD-96 row 4, D-126, E22; owner; review F25). With a build of this story, one launch from the window into a trusted scratch repository: the outcome, `printed`, binding within 10 minutes, and the names of the variables the session's Bash sees; recorded in E22 beside the by-hand half. Proof: manual, research notes entry E22.

## Tasks

- [ ] `crates/officina-core/src/orders/card.rs`: card, issue, edit, subject default, `LaunchRefusal`, checks (AC 1, 2)
- [ ] `card` section of `orders.json` (AC 1, 2)
- [ ] `crates/officina-host/src/orders.rs`: store, ids, `launch_card`, outcome mapping; `AuthorRequest` variants in `watch.rs` (AC 3, 4)
- [ ] `JournalKind::WorkOrderLaunched` and its entry (AC 4)
- [ ] Tauri commands, capabilities, `ipc.ts`, `e2e-mock.ts` (AC 5)
- [ ] `tests/vectors/injection.json`, `crates/officina/tests/injection.rs` (AC 6)
- [ ] Mutant features, `MUTANTS_ENABLED`, forwarding chain, CI job `mutants` (compile first, named failure, control), the app's compile-time assertion (AC 7)
- [ ] Zero-write and doctor (AC 8, 9)
- [ ] Update `CHANGELOG.md` (AC 10)
- [ ] Gate 4's window half with the owner (AC 11)

## Dev Notes

- Requirements: FR-3, FR-4 as amended (amendment 2), J2, J5, PRD section 7 Gesture and Zero write rows. Decisions: AD-3 as amended, AD-32 as amended (`work_order_launched`), AD-78, AD-81, AD-92, AD-95. Facts: E22 (gate 4: whether the started session prompts), S4 (`Workspace not trusted`). Risks: R36, R46. Epic decisions: D-28, D-29 (Epic 2); D-57, D-86, D-92 (Epic 3); D-124, D-126, D-128, D-129, D-135 to D-137, D-147, D-148.
- Seams (D-124): `AuthorRequest` and the collector loop (3.5), the journal append (2.5), `quota_doctor.rs` (v0.1, extended in each epic), `build.rs` and the capability test (2.7).
- The harness calls `orders::launch_card` and the issue and edit functions directly, as the Tauri commands do, so the battery tests the window's path without Tauri; the click's `event.detail` check is the frontend's (story 5.9).
- Each mutant sits at its own check site under `#[cfg(feature = "...")]`; the job must show each one turning the battery red through the case that exercises that check, otherwise the battery does not test it. The `--no-run` step proves a mutant compiles, so a broken build cannot pass for a red battery, and the control run proves the job is not red for another reason.
- `--bg` returns once the supervisor holds the session (E4); the 60 s deadline guards a hung `claude`, and the collector never waits on it.
- E22 failing does not change this code (D-127): the README and the PRD say what work orders can do. If E22 shows a `--bg` session takes the supervisor's environment, D-128's removals do not reach the session; record it here and in the README (D-127).
- No employer content in `injection.json`: placeholder templates and projects.

## Testing

- Core unit: `crates/officina-core/src/{orders/card,relay/journal}.rs`; core integration: `crates/officina-core/tests/orders_vectors.rs`.
- Host unit: `crates/officina-host/src/{orders,doctor}.rs`.
- CLI integration: `crates/officina/tests/{watch,injection,quota_doctor}.rs`.
- App crate: `app/src-tauri/src/main.rs` (compile-time assertion). CI: job `mutants`.
- Manual: E22's window half (AC 11).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `seq` reserved at issue and kept by edits, `name_taken`, launched vector equal to the card's (F2); held pipes after exit 0 are `started`, a timeout kills only `claude` (F10); `LaunchOutcome` never dropped (F14); mutants compile first and fail on a named case (F22); gate 4's window half (F25); forwarding chain and compile-time assertion (F29); executables only (F28)
