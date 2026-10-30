# Story 5.24: Work orders and a project's own permission mode (follow-up after v0.5)

**Status:** In progress (the owner's check in the installed window is open)

## Story

As the author who keeps some repositories in a permission mode that asks (`acceptEdits`) and runs the rest under his bypass default,
I want a work order to respect its project's own mode unless I choose Officina's default for that project, from the card itself,
so that I keep the granularity my project settings give, and can still let a project's work orders run unattended in auto mode with one deliberate click (D-980).

## Acceptance Criteria

1. **The project's own mode by default** (D-980, E88). A work order's vector carries no `--permission-mode` unless its project takes Officina's default; the card names where its mode comes from (`WorkOrderCard.permission`). A background session stopped at a permission prompt is a Needs you card like any session. Proof: core `spawn.rs::tests::a_work_order_vector_carries_no_settings_and_no_permission_mode`, `tests/vectors/spawn.json`'s first work order case, `tests/vectors/classification.json`'s "a work order blocked on a permission prompt" case; CLI `crates/officina/tests/watch.rs::a_work_order_takes_the_projects_mode_until_officinas_default_is_chosen` (the fake's `bg` mode lists the session blocked in a project whose settings set `acceptEdits`, and the board shows it in Needs you).
2. **Officina's default is auto mode** (D-982). `OFFICINA_WORK_ORDER_MODE` is `auto`; Officina's default adds exactly `--permission-mode auto` before the separator, nothing else of the vector moves, and no mode that lets every call through is ever written. Proof: core `orders/permission.rs::tests::officinas_default_is_auto_never_bypass`, `spawn.rs::tests::officinas_default_adds_the_auto_mode_and_nothing_else`, `tests/vectors/spawn.json`'s auto mode case through `spawn_vectors.rs`.
3. **Officina's own state, journaled, revertible** (D-983, D-984). The choice is written only by the window's authority to `relay/work-order-permissions.json`, per project and folder, with one `work_order_permission_set` journal line per change; the same choice twice writes nothing; going back removes it; a record that cannot be read gives every project its own mode; nothing is written under the project's `.claude` or the Claude configuration. Proof: core `orders/permission.rs::tests`, `relay/journal.rs::tests::work_order_permission_set_round_trips` and the every-kind sample; CLI `watch.rs::{a_work_order_takes_the_projects_mode_until_officinas_default_is_chosen, an_unreadable_permission_record_gives_every_project_its_own_mode}`.
4. **The card follows the switch** (D-985). A switch issues the card again under its `seq` with the new mode and supersedes the old one; a card issued before the switch is refused `permission_changed` at Launch; the launched argument vector, the journal line and the registry row are the card's. Proof: core `card.rs::tests::{a_switched_project_issues_its_cards_with_officinas_default, checks_run_in_their_order}`, `tests/vectors/orders.json`'s `permission_changed` case; CLI `watch.rs::a_work_order_takes_the_projects_mode_until_officinas_default_is_chosen`; the e2e fixture `card-bump-officina` issued by core through `orchestrator_fixture`.
5. **The control** (D-986). The card's Permissions line shows a tooltip on hover and when Tab reaches its button; "Use Officina's default for this project" acts on a pointer click only (Enter and Space do nothing); "Back to the project's mode" takes the keyboard too; the tooltip is laid over the card, so nothing moves; both themes; no attention colour. Proof: Vitest `components.test.ts > WorkOrderCard` (the line, pointer only, back by keyboard, disabled while busy or launched); Playwright `orchestrator.spec.ts > the permission tooltip shows on hover and moves nothing ({light,dark})`, `> the permission switch is reached with Tab, which shows its tooltip, and no key widens`, `> a click switches the project to Officina's default; going back takes the keyboard`, `> the card core issued for Officina's default carries auto mode before the prompt`; `stable-size.spec.ts > work order permission switch` and `> work order permission line and its tooltip's text`; the existing attention-colour case covers the tooltip's elements.
6. **Doctor shows the choice** (D-983, D-984). The Orchestrator section's `work order permissions` line names each project taking Officina's default with its folder, sets aside a choice the relay floor no longer matches, and warns on a record it cannot read. Proof: host `doctor.rs::tests::{the_work_order_permissions_line_names_each_switched_project, run_reports_every_check_in_order}`.
7. **Gate 4's owner half and the supervisor's environment** (D-987). The research notes record the owner half (E88, redacted) and point E22 at it; the environment a service started by a work order hands to the author's later background sessions is assessed and stated in the README. Proof: review.
8. **Documentation**. README (Work orders: permission mode and environment; Permission modes; What v0.5 stores; Checking the setup), `CHANGELOG.md` `[Unreleased]` Added, decisions D-980 and D-982 to D-987, `sprint-status.yaml`. Proof: review; `crates/officina/tests/readme.rs`.
9. **Owner's check** (open). In the installed window, with a template approved on a project whose settings set `acceptEdits`: launch a card in the project's mode and see it wait in Needs you; hover the Permissions line, switch with the mouse, launch, and see the session's footer in `claude logs` name auto mode; go back.

## Tasks

- [x] `WorkOrderPermission`, `WorkOrderPermissions` and the vector's mode (AC 1, 2)
- [x] The window's record, its authority writer and the journal kind (AC 3)
- [x] The card's permission, `reissue`, `PermissionChanged`, the collector command and the Tauri command (AC 4)
- [x] The card's Permissions line and tooltip, the store, the mock (AC 5)
- [x] Doctor's line (AC 6)
- [x] The fake's `bg` mode reads the session's mode; the e2e fixture card (AC 1, 4)
- [x] Research notes E88 and E22, decisions, README, CHANGELOG (AC 7, 8)
- [ ] The owner's check (AC 9)

## Dev Notes

- Requirements: the owner's decision D-980 after gate 4's owner half (E88). Decisions: D-127 as D-980 amends it, D-128, D-490, D-573, D-671, D-173, D-961, D-28; this story's D-982 to D-987.
- Officina still never writes Claude Code configuration: a `--permission-mode` on its own vector is no setting, and the project's `.claude` is only read by Claude Code.
- `relay/` is chosen over the registry for the record: the gate protects it from a guarded session's shell, and no schema migration is needed (D-983).
- No employer content: the fixtures use `backend` and placeholder folders; E88 names the projects "a free project" and "a guarded project".

## Testing

- Core unit: `orders/permission.rs`, `orders/card.rs`, `spawn.rs`, `relay/journal.rs`, `relay/view.rs`; core integration: `spawn_vectors.rs`, `orders_vectors.rs`, `classification_vectors.rs`, `typescript_bindings.rs`.
- Host unit: `doctor.rs`.
- CLI integration: `crates/officina/tests/watch.rs`, `injection.rs`, `readme.rs`.
- Vitest: `app/src/components/components.test.ts`; Playwright: `app/e2e/{orchestrator,stable-size}.spec.ts`.

## Dev Agent Record

### Debug Log

- The tooltip's text first changed height with the mode, which moved the switch inside it: both texts are now stable labels (`stable-size.spec.ts` caught it).
- A hover can scroll the line into view, so the "moves nothing" case compares page coordinates, not viewport ones.
- Gate, Windows, on this story's commits over the doctor fix (`533dd67`), every job of `.github/workflows/ci.yml` replayed: fmt; frontend install and build; clippy; `cargo test --workspace --features officina-core/ts` 1,722 passed, 0 failed, 16 ignored (TypeScript drift green); the spawn tests alone; `perf.rs` in release with CI's budgets (hook own cost 2.1 ms, gate 3.0 ms, PermissionDenied 3.2 ms, PermissionRequest 2.2 ms, in-process p95 4.1 ms); the lexer differential; `approvals_stress`; `registry_crash`; `proposals_crash` and the `officina mcp` kill; officina holds no authority; registry dependencies; no bundled roots; e2e fixtures from the real CLI (`card-bump-officina` among them); `pnpm check`, `lint`, Vitest 356 passed; Playwright `--repeat-each=3` 1,365 passed, 6 skipped; the mutants job (each mutant caught, control green); `cargo deny`; `pnpm audit` (one low, ignored as listed). An earlier injection run, right after a rebuild, timed out waiting for four homes' first board within 10 s; it passed on the next run and in the gate. The `core-linux` job in a `rust:1-bookworm` container (toolchain from `rust-toolchain.toml`, source read-only): fmt and clippy clean, 1,540 passed, 0 failed, 14 ignored, the spawn tests alone passed.

### Completion Notes

Decisions D-980 and D-982 to D-987; D-980's interpretation, D-982, D-983, D-986 and D-987 to confirm. AC 9 is the owner's.

## File List

- `crates/officina-core/src/orders/permission.rs` (new), `orders/mod.rs`, `orders/card.rs`, `spawn.rs`, `relay/journal.rs`, `relay/view.rs`; `crates/officina-core/tests/orders_vectors.rs`
- `crates/officina-host/src/relay/home.rs`, `relay/authority.rs`, `orders.rs`, `watch.rs`, `doctor.rs`, `registry/orders.rs`, `spawn.rs`; `crates/officina-host/examples/orchestrator_fixture.rs`; `crates/officina-host/tests/registry_orders.rs`, `registry_retention.rs`
- `crates/officina/src/bin/officina-fake-claude.rs`; `crates/officina/tests/watch.rs`, `injection.rs`, `common/orders.rs`
- `tests/vectors/spawn.json`, `orders.json`, `classification.json`; `tests/support/spawn_cases.rs`
- `app/src-tauri/src/main.rs`, `build.rs`, `capabilities/default.json`
- `app/src/lib/generated/WorkOrderPermission.ts` (new), `WorkOrderCard.ts`, `LaunchRefusal.ts`; `app/src/lib/ipc.ts`, `orchestrator.svelte.ts`, `orchestrator-text.ts`; `app/src/components/WorkOrderCard.svelte`, `OrchestratorView.svelte`, `components.test.ts`; `app/src/e2e-mock.ts`; `app/scripts/e2e-fixtures.mjs`; `app/e2e/orchestrator.spec.ts`, `stable-size.spec.ts`
- `README.md`, `CHANGELOG.md`, `docs/research-notes.md`, `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this story

## Change Log

- 2026-10-30 created and implemented after gate 4's owner half and the owner's decision D-980; decisions D-980, D-982 to D-987; status in progress (AC 9, the owner's check)
