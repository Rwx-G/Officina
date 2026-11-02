# Story 8.14: Worker relay calls in the review, the views and the briefing (Slice B)

**Status:** Approved

## Story

As the author reviewing a worker's package or glancing at the workshop,
I want to see which reads each run asked for, which ran and which were refused and why, the relay actions a template allows on its card, and refusals in my briefing,
so that I judge a diagnosis with its evidence, notice a worker probing for more than it may have, and never see a worker's output or values presented as anything but untrusted text (FR-9).

## Acceptance Criteria

1. **Entry condition** (P-72, D-1313). 8.12 has merged; v0.6's collection, verdicts, workshop view and review panel (6.13, 6.16, 6.18, 6.19) are on `main`. Proof: the sprint status.
2. **Run facts** (AD-158, AD-161, D-212, D-419, D-1306, P-73). At collection the host folds the run's `worker_relay` lines into `facts_json`, that is core `workshop::run::RunFacts` as built (per call: seq, outcome, exit code, refusal and cause; its version constant up by one) and the action and values into `texts_json` as `Untrusted`, cleared with D-212's texts; never an output. A run without relay calls keeps its facts unchanged in content. Proof: core `crates/officina-core/src/workshop/run.rs::tests::{relay_calls_fold_from_lines, an_earlier_facts_version_still_reads, no_output_is_folded}`; CLI `crates/officina/tests/workshop_relay.rs::collection_records_relay_calls`.
3. **Review** (AD-158, FR-7, FR-9). `ReviewView.relay_calls` lists each call (action, values as `Untrusted`, outcome, exit code, refusal with its words); an `order_required` refusal reads "the worker asked for a dry run out of scope: run it from a session". `open_review` carries them. Proof: core `crates/officina-core/src/workshop/review.rs::tests::{relay_calls_are_listed_in_order, order_required_reads_run_it_from_a_session, values_are_untrusted}`.
4. **The card** (AD-156, AD-162, FR-5, P-73). `WorkshopCardFacts.relay_actions` lists the approved relay actions beside the allow-list; `ORCHESTRATOR_VIEW_SCHEMA` (1 at this rework, core `orders/view.rs`) goes up by one relative to `main` at this merge (D-1301). Proof: core `crates/officina-core/src/orders/view.rs::tests::the_workshop_card_lists_relay_actions`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
5. **Workshop view** (AD-162, P-73). `WorkshopView` gains, per live run, the mailbox's state (`attached`, `detached`) and its calls and refusals so far, and per template the calls and refusals of the table's period; `WORKSHOP_VIEW_SCHEMA` goes up by one. Proof: core `crates/officina-core/src/workshop/view.rs::tests::{live_runs_show_mailbox_state_and_counts, templates_show_calls_and_refusals_per_period}`.
6. **Relay view** (AD-162, P-73, D-419; review F23, F34). `RelayView.workers` holds the last 50 `worker_relay` lines with their run, battery lines grouped under their battery (the battery id 8.13 writes in `workshop_run`), their `params` as `Untrusted<String>` (D-130), under the `RELAY_VIEW_SCHEMA` 8.4 set unless a release has shipped since. Proof: core `crates/officina-core/src/relay/view.rs::tests::{workers_keeps_the_last_50, battery_lines_are_grouped_under_the_battery, worker_params_are_untrusted}`; Playwright `app/e2e/workshop-relay.spec.ts > battery relay lines are listed under their battery`.
7. **Briefing and MCP** (AD-162, AD-83, FR-9, D-153). The briefing lists worker relay refusals since the cursor; `officina mcp` serves them through `briefing` and `relay_activity`, wrapped as untrusted (D-153); no tool is added, and no kept output, resolved name or worker value is served. Proof: core `crates/officina-core/src/orders/briefing.rs::tests::worker_refusals_since_the_cursor`; CLI `crates/officina/tests/mcp.rs::{relay_activity_serves_worker_refusals_without_values, briefing_lists_worker_refusals}`; `tests/vectors/injection.json` section `mcp_output` gains a marker in a worker value that must not appear.
8. **CLI** (AD-162). `officina relay --json` and `officina workshop --json` print the new fields. Proof: CLI `crates/officina/tests/relay.rs::relay_json_prints_worker_calls` and `crates/officina/tests/workshop_cli.rs::workshop_json_prints_mailbox_state` (6.18's CLI test file, D-1306).
9. **Frontend** (AD-163, D-1309). `WorkOrderCard.svelte` shows the relay actions on a workshop card as one line of fixed height (the list cut with an ellipsis and whole in its tooltip), so the card keeps its box whatever the count; `RelayCalls.svelte` in `ReviewPanel.svelte` (6.19); `WorkerCalls.svelte` in `RelayView.svelte`; refusal and cause words from the `relay-text.ts` records 8.10 owns (D-433, review F21); every worker text rendered with `{text}`; existing tokens only, nothing in the header. No rule is computed in TypeScript. Proof: Vitest `app/src/components/components.test.ts > RelayCalls renders values as inert text`, `> WorkerCalls renders values as inert text`; Playwright `app/e2e/workshop-relay.spec.ts > the card lists relay actions`, `> the review panel lists relay calls with an order_required refusal`, `> the Relay view lists worker calls`, `> a refusal's words never use the attention colour ({light,dark})`; `app/e2e/stable-size.spec.ts`'s `CASES` gains "workshop card relay actions".
10. **Fixtures** (D-25, D-181, D-1306). `app/scripts/e2e-fixtures.mjs`'s workshop scenario gains a run with relay calls (ran, refused `worker_write`, refused `order_required`) from the real CLI with the fake engine, under the index kind `workshop` that story 6.18 adds to `app/e2e/fixture-index.ts`'s `FixtureKind`. Proof: the local gate's fixture step (D-1303) and the Playwright cases above.
11. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: relay calls in the review, the card, the views and the briefing. README "Relay scope and workers (v0.8, in progress)": where relay calls show, that outputs are never kept for review, and that `officina relay --json`, which host sessions can run, shows workers' values (review F34). Proof: review.

## Tasks

- [ ] Facts fold, typed version, texts (AC 2)
- [ ] `ReviewView.relay_calls` (AC 3); `WorkshopCardFacts.relay_actions`, schema (AC 4)
- [ ] `WorkshopView` fields, schema (AC 5); `RelayView.workers` (AC 6)
- [ ] Briefing and MCP (AC 7); CLI (AC 8)
- [ ] `WorkOrderCard.svelte`, `RelayCalls.svelte`, `WorkerCalls.svelte` (AC 9)
- [ ] Fixtures (AC 10); `CHANGELOG.md`, README (AC 11)

## Dev Notes

- Requirements: FR-5 (card), FR-7 (package), FR-9 (workshop, briefing, untrusted text). Decisions: AD-74, AD-83, AD-156, AD-158, AD-161, AD-162, AD-163. Epic decisions: D-25, D-130, D-153, D-181, D-212, D-564, D-620, D-962; P-73, D-400, D-419, D-420, D-423, D-433, D-1301, D-1303, D-1306, D-1309, D-1313.
- Seams (D-400, read through `main` as built, D-1306): `facts_json` as `workshop::run::RunFacts` and `texts_json` (6.4, 6.13, D-212), `ReviewView` and `open_review` in core `workshop/review.rs` (6.13, 6.16, 6.18), `WorkshopCardFacts` (6.11), `WorkshopView` and `workshop --json` (6.18), `WorkOrderCard.svelte` (5.9) and `ReviewPanel.svelte` (6.19), the briefing (core `orders/briefing.rs`, 5.8), `officina mcp`'s `RelayActivity` tool (5.15; it runs with the orchestrator's session variables and spawns nothing that could carry them, D-693), D-181's index (6.18), `RelayView` (8.4), the `relay-text.ts` records (8.10), the battery's journal lines (8.13), `injection.json`'s `mcp_output` source (D-962).
- Waits for: the entry condition (AC 1).
- The module paths of the v0.5 views (`orders::view`, `orders::briefing`) and v0.6's run facts (`workshop::run`) are on `main`; v0.6's `workshop::view` and `workshop::review` views are those stories 6.13 and 6.18 create; if a merged story named them otherwise, the tests live beside the merged code.
- No employer content in fixtures: placeholder templates, `lab`, `example.org`.

## Testing

- Core unit: `crates/officina-core/src/{workshop/run,workshop/review,workshop/view,orders/view,orders/briefing,relay/view}.rs`; core integration: `crates/officina-core/tests/typescript_bindings.rs`.
- CLI integration: `crates/officina/tests/{workshop_relay,mcp,relay,workshop_cli,injection}.rs`.
- Vitest: `app/src/components/components.test.ts`. Playwright: `app/e2e/workshop-relay.spec.ts`, `app/e2e/stable-size.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `Record<WorkerRefusal, string>` moves to 8.10 (F21, D-433); the Relay view's battery grouping lives here only (F23, D-420); worker values in `RelayView.workers` are `Untrusted` and the README says `relay --json` shows them to host sessions (F34, D-419)
- 2026-11-02 reworked against v0.4 to v0.6 as built: entry names v0.6's 6.13, 6.16, 6.18 and 6.19 (D-1313); facts in `workshop::run::RunFacts`, CLI test `workshop_cli.rs`, fixture script and kind as on `main` (D-1306); the card's relay actions on one fixed line with a stable-size case, the colour case in both themes, no header element (D-1309); schemas from `main` in either order (D-1301); the fixture step replayed locally (D-1303)
