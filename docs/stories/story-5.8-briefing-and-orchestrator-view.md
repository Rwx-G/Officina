# Story 5.8: The briefing and the Orchestrator view

**Status:** Approved

## Story

As the author who starts the day juggling subjects, not terminals,
I want an Orchestrator view where Officina's own code tells me what is open, what waits for me and what changed since I last read it, beside the history of my work orders,
so that I get the day's picture in one place with no model involved (J1).

## Acceptance Criteria

1. **The briefing** (AD-83, FR-1). `officina-core::orders::briefing::build(&BriefingInput, cursor: Cursor, now_ms) -> Briefing` takes the board, the relay view, the registry view and the orders state, and lists: open subjects with their sessions' states; Needs you and Your turn sessions; pending approvals; triage rows; failed and refused runs; launched work orders with their sessions' states; and `changed`, the items whose `at_ms` follows the cursor (runs finished, sessions that started waiting, subjects archived or reopened, orders launched). Session names and every session text are `Untrusted`; subject and template names are plain. `Cursor` is `ReadAt(ms)`, `PreviousStart(ms)` or `None`. Proof: core `crates/officina-core/src/orders/briefing.rs::tests::{each_section_lists_its_items, changed_holds_only_items_after_the_cursor, none_cursor_marks_everything_new, session_texts_are_untrusted}`; the `briefing` section of `tests/vectors/orders.json` through `crates/officina-core/tests/orders_vectors.rs::briefing_cases_match`.
2. **The bound** (AD-83, FR-1). The serialized briefing is at most 32 KiB: while over, the globally oldest list item is dropped and counted in its section's `dropped`; counts are never dropped; the same input gives the same bytes. Proof: `briefing.rs::tests::{an_oversized_briefing_drops_the_oldest_items_first, counts_survive_every_drop, same_input_same_bytes}`; the vector cases.
3. **The window's cursor** (AD-83, FR-1). The writer stores `setting.briefing_read_at_ms` (source `author`) when the author leaves the Orchestrator view and when the window closes; the view's briefing uses it, `None` before the first reading. Proof: host `crates/officina-host/tests/registry_orders.rs::briefing_read_at_is_written_and_read_back`; CLI-crate collector test `crates/officina/tests/watch.rs::the_window_briefing_starts_at_the_last_reading`.
4. **`OrchestratorView`** (AD-92, P-26, D-136). `officina-core::orders::view` declares `OrchestratorView` (ts-rs, `ORCHESTRATOR_VIEW_SCHEMA = 1`): `presence` (null until story 5.16), the window's briefing, approved templates (for the composer, with their projects and lists), templates awaiting approval (from the pending diff), `proposals` (`ProposalView` items, D-576; empty until story 5.17), and the last 50 work orders with binding and session state, each with the derived `unbound` and "printed id differs" flags. Proof: core `crates/officina-core/src/orders/view.rs::tests::{templates_come_from_the_approved_snapshot, awaiting_templates_come_from_the_pending_diff, unbound_and_printed_id_differs_are_derived}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
5. **Emission** (AD-92, AD-5, D-44). The collector builds the view after each recompute and emits `orchestrator://updated` only when its bytes change; `get_orchestrator` returns the cached view; `Emit` gains `orchestrator`; `mark_briefing_read` is an author command with a deadline (D-92). Both commands are declared, allowed, wrapped in `ipc.ts` and answered by `e2e-mock.ts` (`window.__officinaPushOrchestrator` simulates the event). Proof: `watch.rs::the_orchestrator_view_is_emitted_only_on_change`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
6. **E2E fixtures** (AD-95 frontend row, D-72's pattern). `crates/officina-host/examples/orchestrator_fixture.rs` (`required-features = ["window-authority"]`) writes a floor with templates, approves it through `authority` (D-25), writes work orders and registry rows through the window's writer at explicit `now_ms`, and prints the `OrchestratorView` the window would build; `app/scripts/e2e-fixtures.mjs` runs it for the scenarios `orchestrator-empty` and `orchestrator-day` and records their outputs in D-181's `index.json` under the new kind `orchestrator`; specs load them through the shared `app/e2e/fixture-index.ts`, never by file name (review F30). Proof: CI step "e2e fixtures from the real CLI"; `fixture-index.ts` failing, naming `pnpm e2e:fixtures`, when the index lists no `orchestrator` output.
7. **The view** (AD-93, AD-66 as amended by section 3, D-113, D-517, D-526, D-532, D-564, D-565). The header's view switch (`Header.svelte`, `data-testid="view-switch"`) becomes Sessions, Subjects, Relay, Orchestrator, and `lib/view.svelte.ts`'s current view gains `orchestrator` (not persisted, Sessions at start, as today); `m`, `f` and the digits keep acting in the Sessions view only (D-113, D-532), so Focus over a base is untouched; the fourth button is a fixed control of the header in Board and Scene modes alike, the same element in both (D-526), and its presence mark and proposal count arrive with story 5.18 (D-565); `OrchestratorView.svelte` holds `Briefing.svelte` (sections, "since" the cursor's time or "first reading", dropped counts) and `WorkOrderHistory.svelte` (template, project, values, outcome, binding, unbound, printed id differs, the bound session's state); `lib/orchestrator.svelte.ts` holds the view; leaving the view calls `mark_briefing_read`. All text renders as text. Proof: `app/e2e/stable-size.spec.ts`'s existing "view switch" case, whose selector covers the fourth button, and `app/e2e/scene.spec.ts > eight projects fit a maximised Full HD window with the alert band`, both green with the fourth button (D-564); Vitest `app/src/lib/view.test.ts > the orchestrator view is not persisted and digits stay in the Sessions view`; Playwright `app/e2e/orchestrator.spec.ts > the header switches to the Orchestrator view`, `> the Orchestrator view reads the same in both themes but for its tokens` (light and dark), `> the briefing shows each section and since when`, `> dropped items are counted`, `> leaving the view marks the briefing read`, `> the history shows outcome, binding and unbound orders`; Vitest `app/src/components/components.test.ts > Briefing renders session names with markup as text`.
8. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: the Orchestrator view and its briefing; README "The orchestrator (v0.5, in progress)": what the briefing lists and since when. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/orders/briefing.rs`: input, sections, `changed`, `Cursor`, bound (AC 1, 2)
- [ ] `briefing` section of `orders.json` (AC 1, 2)
- [ ] Writer and reader for `briefing_read_at_ms`; window-close hook in the app (AC 3)
- [ ] `crates/officina-core/src/orders/view.rs`; bindings (AC 4)
- [ ] Collector build and emission, `Emit::orchestrator`, commands, `ipc.ts`, mock (AC 5)
- [ ] `examples/orchestrator_fixture.rs`, `e2e-fixtures.mjs` scenarios (AC 6)
- [ ] `Header.svelte` switch, `OrchestratorView.svelte`, `Briefing.svelte`, `WorkOrderHistory.svelte`, `lib/orchestrator.svelte.ts` (AC 7)
- [ ] `orchestrator.spec.ts`, Vitest case (AC 7)
- [ ] Update `CHANGELOG.md` and the README (AC 8)

## Dev Notes

- Requirements: FR-1, J1, journey 1 (its Slice A part). Decisions: AD-5, AD-83, AD-92, AD-93; AD-55 and AD-66 as amended. Epic decisions: D-25, D-44, D-72, D-92 (Epics 2, 3); D-113, D-181 (Epic 4); P-26, D-124, D-136, D-149.
- Seams (D-124, read through `main`, D-563): `app/src/lib/view.svelte.ts` (views `sessions`, `subjects`, `relay`; `ViewState`) and `app/src/components/Header.svelte` (the view switch; the one-line header of Scene mode, D-526, D-533), `Emit` and `AuthorRequest` (3.5), `RelayView` (2.6) and `RegistryView` (3.4) as briefing inputs.
- Header width (D-564, D-565): the one-line header fitted eight projects with the alert band on the owner's screen by a few pixels after D-533. If the fourth button turns `eight projects fit ...` red, the story does not shrink targets or gauges on its own: it records the overflow in the Completion Notes and asks the owner, as D-520 requires.
- New colours: none; the view reuses the Relay and Subjects views' tokens (D-564).
- The proposals section of the briefing and of the view is story 5.17's; the presence is 5.16's. Keep the fields' shapes stable now (`proposals: []`, `presence: null`) so those stories add data, not fields, except where they say so.
- `officina mcp` calls the same `build` with `Cursor::PreviousStart` in story 5.15 (AD-83: the author and the orchestrator read the same briefing).
- The 32 KiB bound and the drop order are core's: the frontend shows `dropped`, never truncates itself.
- No employer content in the fixture: placeholder projects, templates and session names.

## Testing

- Core unit: `crates/officina-core/src/orders/{briefing,view}.rs`; core integration: `crates/officina-core/tests/{orders_vectors,typescript_bindings}.rs`.
- Host integration: `crates/officina-host/tests/registry_orders.rs`. CLI integration: `crates/officina/tests/watch.rs`.
- App crate: `app/src-tauri/src/main.rs`. Vitest: `app/src/components/components.test.ts`. Playwright: `app/e2e/orchestrator.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: fixtures registered in D-181's index under the kind `orchestrator` (F30)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: the fourth view button under the header's stable-size and Full HD fit rules, keys unchanged, both themes (D-564, D-565); `ProposalView` (D-576); seams cited by file (D-563)
