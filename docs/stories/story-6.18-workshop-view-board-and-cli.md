# Story 6.18: The workshop view, board cards, `officina workshop --json` and the briefing

**Status:** Review

## Story

As the author who reads the board and the orchestrator's briefing to know what waits on me,
I want workshop runs on the board as their own card kind, a Workshop section payload with images, batteries, runs, flags and per-template results, the same section printed by the CLI, and the briefing told about runs and packages,
so that a package waiting for review is a Needs you card like any other, and the orchestrator sees the workshop without any new tool that could act on it.

## Acceptance Criteria

1. **Board cards** (AD-121, FR-17, P-53). Core's `view::workshop_cards(runs, now_ms)` builds cards of kind `workshop`, key `ws:<run id>`: Working while `launching`, `running`, `stopping` or `collecting` (an "isolated" chip); Needs you while `review` or `quarantined` ("review", counted in `waiting_count`); To resume while `stopped` (not counted); Done for 24 h after a verdict, after `no_change` (badge "no change", never Needs you), `failed` or `unknown`. `Board.workshop_live: u32` feeds the live-run count on the Orchestrator view switch (D-618). Each `workshop` card's `scene` is `None` in this story, so Slice A draws no worker in Scene mode; story 6.23 gives them v0.4's zones in `officina-core::scene` (D-619). `BOARD_SCHEMA` goes up by one relative to `main`; TypeScript bindings regenerated. Proof: cases in `tests/vectors/classification.json` for the kind and each state through `crates/officina-core/tests/classification_vectors.rs` (its coverage test sees the new kind, states and badge); core `crates/officina-core/src/workshop/view.rs::tests::{no_change_is_done_never_needs_you, stopped_is_to_resume_not_counted, done_lasts_24_hours, workshop_cards_have_no_scene_yet}`; `typescript_bindings.rs` drift check.
2. **`WorkshopView`** (AD-121, amendment 13, D-217, D-222). `WORKSHOP_VIEW_SCHEMA = 1`, built in core, emitted on `workshop://updated` on byte change (AD-5), separate from `OrchestratorView`: the engine state and key; both flags; images (state `absent`, `building`, `ready`, `mismatch`, `failed`; labels compared; inventory per project; the battery's last outcome, age and validity); live, kept and orphan runs; story 6.16's per-template table and notices; this week's workshop share (D-214) and environment failures (D-217); the limits; the credential (its kind, its `CredentialState`, declared by story 6.11 and moved by story 6.26, and the action it calls for: for `login` the login command, for the token kinds the `setup-token` step and the floor's `cmdkey` line, D-607); the network (the proxy source and each allow-list entry's route, never a value of the credential, D-611); command `get_workshop` (read; this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer, review F33). Every run text in it has passed the run's needles (D-230). Proof: core `view.rs::tests::{the_view_is_stable_for_equal_inputs, workshop_share_counts_started_outcomes_of_both_kinds, environment_failures_exclude_no_change, every_credential_state_names_its_action}` (the last iterates `CredentialState::ALL` and `CredentialKind::ALL`); host `crates/officina-host/src/watch.rs::tests::the_workshop_view_is_emitted_on_change_only`; `crates/officina-core/tests/typescript_bindings.rs` (drift check over `WorkshopView`); `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
3. **`officina workshop --json`** (AD-121, D-47). Prints `WorkshopView` from the read-only registry and, when it answers, the engine's read half; exit 0, 3 when the registry cannot be read, 64 on usage, never 2; `readme.rs` and `quota_doctor.rs` cover it. Proof: CLI `crates/officina/tests/workshop_cli.rs::{workshop_json_prints_the_view, an_unreadable_registry_exits_3, the_engine_down_still_prints_the_registry_part}`; `readme.rs::readme_mentions_every_subcommand`; `quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
4. **History and briefing** (AD-121, AD-83, FR-17). `OrchestratorView` history rows show workshop runs with their state (under story 6.11's schema number); the briefing gains live runs, packages awaiting a verdict, and quarantined, failed and unknown runs of the last 24 h; `officina mcp` serves them through `briefing` and `board`, wrapped as untrusted where they hold run text (D-153); no tool is added, and none imports, discards or stops. Proof: core `crates/officina-core/src/orders/briefing.rs::tests::{the_briefing_lists_workshop_sections, no_change_runs_are_not_failures_in_the_briefing}`; CLI `crates/officina/tests/mcp.rs::{the_tool_list_is_unchanged, workshop_texts_are_wrapped_untrusted}`.
5. **Fixtures** (D-25, D-181, AD-125). `crates/officina-host/examples/workshop_fixture.rs` (`required-features = ["window-authority"]`) drives the fake engine and real git through launch, follow, collection, approval and import to produce homes in each run state; `app/scripts/e2e-fixtures.mjs` runs it and writes `officina workshop --json` and `board --json` outputs under kind `workshop` in D-181's index. Proof: `app/e2e/fixture-index.ts` loads the kind; the Playwright cases of stories 6.19 and 6.20 consume them.
6. **Documentation** (D-226). README: the board's workshop cards and `officina workshop --json`; `CHANGELOG.md`. Proof: review.

## Tasks

- [x] `view::workshop_cards`, `Board.workshop_live`, schema bump, classification cases, bindings (AC 1)
- [x] `WorkshopView`, `get_workshop`, emission (AC 2)
- [x] `officina workshop --json` (AC 3)
- [x] History rows, briefing sections, `officina mcp` wrapping (AC 4)
- [x] `examples/workshop_fixture.rs`, `e2e-fixtures.mjs`, index kind (AC 5); README, `CHANGELOG.md` (AC 6)

## Dev Notes

- Requirements: FR-17 as amended (amendment 13), PRD section 7 Workshop share, Environment failures, Acceptance. Decisions: AD-5, AD-13, AD-54, AD-83, AD-121. Epic decisions: D-25, D-47 (Epic 2, 3); D-153, D-181 (Epic 5, 4); P-53, D-200, D-214, D-217, D-222, D-230, D-249.
- Seams (D-200): `core/board.rs` and `classification.json` (1.4), the briefing (5.8), `officina mcp` (5.15), `OrchestratorView` (5.8), `officina/main.rs`, `readme.rs`, `quota_doctor.rs`, `app/scripts/e2e-fixtures.mjs` and D-181's index, as they stand on `main` (D-620, `epic-5-decisions.md` D-563: files win over story numbers); `core/scene.rs`'s rules are left as they are until story 6.23 (D-619).
- Waits for: stories 6.11 to 6.17, 6.25 and 6.26.
- The Workshop view is its own payload so that followers' updates do not churn the orchestrator view (AD-121).
- A `no_change` run counts as a run in Use, Workshop share and the environment failures' denominator; it is never a failure, has no verdict, and stays out of Review cost and Acceptance (amendment 15).

## Testing

- CI, no Docker: core `crates/officina-core/src/{workshop/view,orders/briefing}.rs`, `crates/officina-core/tests/{classification_vectors,typescript_bindings}.rs`; host `crates/officina-host/src/watch.rs`; CLI with the fake engine `crates/officina/tests/{workshop_cli,mcp,readme,quota_doctor}.rs`; the fixture example.

## Dev Agent Record

### Debug Log

- The first bindings export wrote the view's `RunView` and `CredentialView` over the relay's files of the same names, and the drift check stayed green, since it compares a fresh export with the folder; renamed `WorkshopRunView` and `WorkshopCredential` (D-1233).
- The first fixture run counted no launch in the week's share: the hand-written `workshop_started` lines spelled the credential kind `oauth_token`, its wire value being `oauth-token`, so the journal reader skipped them; the script now checks the share.
- `cargo test --all-features` turns the mutant features on and fails by design; the story's runs use `--features ts`, as the gate does.
- 6.26 has not merged: the view's credential state comes from `UncheckedStates`, with `missing` computed (D-1232); story 6.26 swaps the source.

### Completion Notes

- AC 1: core `workshop/view.rs::tests::{no_change_is_done_never_needs_you, stopped_is_to_resume_not_counted, done_lasts_24_hours, workshop_cards_have_no_scene_yet}`; `tests/vectors/classification.json` gains two workshop cases replayed by `classification_vectors.rs::every_case_classifies_as_expected`, whose coverage test now requires every `RunState` and, through `Badge::ALL`, the `no_change` badge; `typescript_bindings.rs` regenerated. `BOARD_SCHEMA` is 6. D-1230.
- AC 2: core `workshop/view.rs::tests::{the_view_is_stable_for_equal_inputs, workshop_share_counts_started_outcomes_of_both_kinds, environment_failures_exclude_no_change, every_credential_state_names_its_action}`, `dates.rs` doctests for the ISO week; host `watch.rs::tests::the_workshop_view_is_emitted_on_change_only`, `workshop/view.rs::tests::{orphans_are_unowned_run_containers_of_this_floor, a_missing_credential_is_computed}`; `typescript_bindings.rs` exports `WorkshopView`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` covers `get_workshop` (`build.rs`, capabilities, `ipc.ts` `getWorkshop`, `e2e-mock.ts`). D-1231 to D-1234.
- AC 3: CLI `workshop_cli.rs::{workshop_json_prints_the_view, an_unreadable_registry_exits_3, the_engine_down_still_prints_the_registry_part}` (the second also checks usage exit 64); `readme.rs::readme_mentions_every_subcommand`; `quota_doctor.rs::no_command_writes_under_the_claude_config_dir` runs `workshop --json` on a missing engine.
- AC 4: history rows already carried the runner and the run's state from story 6.11 (`orders/view.rs::tests::history_rows_name_their_runner`); core `orders/briefing.rs::tests::{the_briefing_lists_workshop_sections, no_change_runs_are_not_failures_in_the_briefing}`; CLI `mcp.rs::{the_tool_list_is_unchanged, workshop_texts_are_wrapped_untrusted}`. `officina board --json` and `officina mcp` read the runs through `read_registry`.
- AC 5: `crates/officina-host/examples/workshop_fixture.rs` and the `workshop-runs` scenario of `app/scripts/e2e-fixtures.mjs` write `workshop-runs.json` and `workshop-runs-board.json` under kind `workshop`; Vitest `e2e/fixture-index.test.ts > the workshop kind loads the view and its board`. Rows are seeded through the window's writer rather than the whole flow (D-1235).
- AC 6: README "Runs on the board and `officina workshop --json`" and the command table; `CHANGELOG.md`.

## File List

- `crates/officina-core/src/board.rs`, `crates/officina-core/src/dates.rs`, `crates/officina-core/src/workshop/view.rs`, `crates/officina-core/src/workshop/image.rs`, `crates/officina-core/src/workshop/battery.rs`, `crates/officina-core/src/workshop/proxy.rs`, `crates/officina-core/src/orders/briefing.rs`, `crates/officina-core/src/relay/auto.rs`, `crates/officina-core/tests/classification_vectors.rs`, `crates/officina-core/tests/typescript_bindings.rs`, `tests/vectors/classification.json`
- `crates/officina-host/src/workshop/view.rs` (new), `crates/officina-host/src/workshop/mod.rs`, `crates/officina-host/src/workshop/doctor.rs`, `crates/officina-host/src/collector.rs`, `crates/officina-host/src/watch.rs`, `crates/officina-host/src/focus.rs`, `crates/officina-host/src/registry/read.rs`, `crates/officina-host/src/registry/window.rs`, `crates/officina-host/src/registry/workshop.rs`, `crates/officina-host/src/registry/commands.rs`, `crates/officina-host/examples/workshop_fixture.rs` (new), `crates/officina-host/Cargo.toml`
- `crates/officina/src/main.rs`, `crates/officina/tests/workshop_cli.rs` (new), `crates/officina/tests/mcp.rs`, `crates/officina/tests/quota_doctor.rs`, `crates/officina/tests/common/mod.rs`
- `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/main.rs`, `app/src/lib/ipc.ts`, `app/src/e2e-mock.ts`, `app/src/e2e-globals.d.ts`, `app/src/lib/generated/` (`WorkshopView` and its parts; `Board`, `Card`, `Badge`, `SessionKindView`, `Briefing` changed), `app/src/components/SessionCard.svelte`, `app/src/lib/scene/text.ts`, `app/src/components/components.test.ts`, `app/src/lib/board.test.ts`, `app/src/lib/scene/testing.ts`, `app/scripts/e2e-fixtures.mjs`, `app/e2e/fixture-index.ts`, `app/e2e/fixture-index.test.ts`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-6-decisions.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `get_workshop` plumbing and the `WorkshopView` bindings drift check named (F33); run texts pass the needles (F2)
- 2026-10-29 the owner's answers: `WorkshopView` carries the credential state with its action and the network's routes; waits for 6.25 and 6.26 (D-607, D-611, D-621)
- 2026-10-29 aligned with v0.4 and v0.5 as built: `workshop_live` feeds the view switch's count, workshop cards carry no scene until 6.23, seams named by files on `main` (D-618, D-619, D-620)
- 2026-11-04 implemented (D-1230 to D-1235); status review
