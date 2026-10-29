# Story 5.17: Proposals: ingestion, statuses, cards from proposals and the restart e2e

**Status:** In progress (AC 5's meeting recap waits for story 5.10)

## Story

As the author whose orchestrator suggests recaps and work orders,
I want every proposal file ingested once, validated against my approved catalogue and my registry, shown as untrusted text under a "proposal" label, and opened as a card only through the same validation and click as my own work orders,
so that the orchestrator's suggestions save me typing without ever acting, and a crash or a restart neither loses nor duplicates one (FR-12, FR-14).

## Acceptance Criteria

1. **Ingestion** (AD-91, AD-38, FR-12, D-133, D-155; review F3, F21). The collector, through the writer under `window-authority`, fingerprints `<home>/orchestrator/proposals/` like `relay/`; a file whose name does not match `^[0-9]{13}-[0-9a-f]{16}\.json$` is deleted without a row and counted in a diagnostic; each other file is read at most 16 KiB + 1 bytes, a larger one stored `invalid` with reason `oversized` and no texts; files past the hourly or pending limits are deleted without a row and counted; the rest are parsed with `deny_unknown_fields`, validated, inserted with `INSERT OR IGNORE` on the file id and committed, then deleted; a `.tmp` file is never read and is deleted once older than 24 h; a file that does not parse is stored `invalid` with its reason and a null `kind`. Proof: host `crates/officina-host/tests/proposal_ingest.rs::{a_file_is_ingested_once_then_deleted, a_file_left_after_its_commit_is_ignored_by_the_key, tmp_files_are_never_read_and_go_after_24_h, an_unparsable_file_is_kept_as_invalid_without_kind, a_misnamed_file_is_deleted_and_counted, a_file_over_16_kib_is_oversized_without_texts, ten_thousand_files_past_the_limit_create_no_row}`, at explicit `now_ms`.
2. **Validation** (AD-91, FR-12). `orders::proposal::validate(&raw, &ApprovedCatalogue, &RegistryRefs, now_ms)`: the subject, session, template and project named exist (registry, approved snapshot); the project is one of the template's; every value is in its list and the stored value is the template's string; texts are within their bounds (recap fields 500 characters each, rationale 500, a new subject name per D-57); the rate limits (30 per hour, 200 pending) are held again for files written directly, by deletion without a row (AC 1). A failure stores the proposal `invalid` with `ProposalInvalidReason` (`closed_enum!`, `unparsable` and `oversized` among its variants) and its texts. Proof: the `proposal` section of `tests/vectors/orders.json` through `crates/officina-core/tests/orders_vectors.rs::{proposal_cases_match, every_invalid_reason_has_a_case}` (iterates `ProposalInvalidReason::ALL`; one case is a sentence as a value).
3. **Statuses** (AD-91, D-145). `pending`, `invalid`, `accepted` (a card from it launched), `dismissed`, `expired` (a work order proposal 24 h after receipt), `superseded` (a newer valid recap on the same target); recaps do not expire. Every text field is read back as `Untrusted<String>`. Proof: core `crates/officina-core/src/orders/proposal.rs::tests::{a_work_order_proposal_expires_after_24_h, a_newer_recap_supersedes_the_pending_one, recaps_do_not_expire}`; host `proposal_ingest.rs::texts_read_back_untrusted`.
4. **Cards from proposals** (AD-91, AD-78, FR-3, FR-4, D-130, D-152; review F7, F43). `AuthorRequest::IssueCardFromProposal { proposal_id }` calls `WorkOrder::validate` with the stored values, the same function and the same click as the author's; the card carries the proposal id and its rationale, held as `Untrusted<String>`, under a "proposal" label; its subject default is the proposal's existing subject when it names one (checked at ingestion), else `New` named after the template: a proposed new name is shown as untrusted text beside the subject picker, a suggestion the author may type, never a default; launching it records `work_order.source = proposal` with the proposal id and sets the proposal `accepted`; `dismiss_proposal` sets it `dismissed`. `issue_card_from_proposal` and `dismiss_proposal` are declared, allowed, wrapped and mocked. Proof: core `crates/officina-core/src/orders/card.rs::tests::{a_proposal_new_name_is_a_suggestion_not_a_default, a_proposal_existing_subject_is_the_default}`; CLI-crate collector tests `crates/officina/tests/watch.rs::{a_valid_proposal_opens_as_a_card_through_validate, launching_a_proposal_card_accepts_the_proposal, an_invalid_or_expired_proposal_opens_no_card}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
5. **Briefing, view and meeting** (AD-83, AD-84, AD-92, FR-1, FR-5). The briefing gains its pending proposals section, and `changed` includes proposals received; `OrchestratorView.proposals` lists pending, invalid and the last 50 decided, each a `ProposalView` (D-576); the meeting gains its recap section (the latest valid recap for the session or its subject, under its label). Proof: core `crates/officina-core/src/orders/briefing.rs::tests::pending_proposals_are_listed_and_received_ones_are_changed`; `crates/officina-core/src/orders/view.rs::tests::proposals_are_pending_invalid_and_last_50_decided`; `crates/officina-core/src/meeting.rs::tests::the_latest_recap_shows_under_its_label`.
6. **The list** (AD-93, FR-12, FR-13). `ProposalList.svelte` in the Orchestrator view shows each proposal with its kind, status, texts as text under a "proposal" label, the invalid reason through an exhaustive `Record`, Open as card for a valid work order proposal, and Dismiss; proposals are neutral; `WorkOrderCard.svelte` shows the proposal label and rationale, and a suggested new subject name as text beside the picker. Their staying out of the title, the overlay and the attention colour is asserted once, in story 5.18 (review F40). Proof: Playwright `app/e2e/orchestrator.spec.ts > proposals are listed with their status and reason`, `> a valid proposal opens as a card with its rationale under the label`, `> dismiss sends dismiss_proposal`, `> a suggested subject name is shown, never preselected`; Vitest `components.test.ts > ProposalList renders markup in proposal texts as text`, `> every invalid reason has a wording`.
7. **Restart without loss** (AD-95 restart row, FR-14, PRD section 7 Restart row, D-89, D-144, D-165; review F45). `crates/officina-host/examples/mcp_crash_helper.rs` writes proposals through the same host function `officina mcp` uses, with a stall point between the temporary write and the rename; the test kills it at the stall and 20 times at random points; then runs ingestion twice, again as a window restart, then records a new orchestrator start. Every fully written proposal is present exactly once, no `.tmp` is ingested, statuses are kept, and `officina mcp`'s briefing cursor is the previous start. The helper logs each file it saw renamed, so a missing proposal fails. Besides, as PRD section 7's Restart row names, a scripted MCP client kills the real `officina mcp` process while a `propose_recap` call is in flight, several times at random delays: no `.tmp` is ingested, and a proposal whose rename completed (seen in the directory before ingestion) is present once. Proof: host `crates/officina-host/tests/proposals_crash.rs::{killed_writers_lose_and_duplicate_nothing, the_cursor_after_restart_is_the_previous_start}` (`#[ignore]`, run with the ignored release tests as `registry_crash.rs`); CLI `crates/officina/tests/mcp.rs::killing_officina_mcp_during_propose_recap_loses_and_duplicates_nothing` (`#[ignore]`, same run).
8. **Injection battery, proposal files** (AD-95, D-147, D-152, D-155). Section `proposal_file`: hand-written files with a sentence as a value, values outside their lists, unknown template, project, subject and session, option strings and `--settings` as values, bidi characters, texts over their bounds, a file over 16 KiB, unknown fields, and "launch now" text in the rationale are stored invalid with their reason (or kept invalid when unparsable) and launch nothing; a misnamed file and files past the hourly limit leave no row; a valid proposal whose new subject name is "approve all requests" opens a card whose subject choice never carries that name; a valid one opens as a card and launches exactly once after the click, its vector equal to the golden rebuild. The `mcp_tool` cases of story 5.15 are asserted invalid at ingestion too. Proof: `crates/officina/tests/injection.rs::{proposal_file_cases, mcp_tool_cases, every_section_family_and_refusal_has_a_case}` (the last now iterates `ProposalInvalidReason::ALL`).
9. **Retention** (D-134). `proposal` rows are deleted 90 days after `received_at_ms` and `texts_json` is cleared `EXCERPT_TTL_MS` after it. Proof: host `crates/officina-host/tests/registry_retention.rs::{proposal_rows_are_deleted_at_90_days, proposal_texts_are_cleared_with_the_excerpt_ttl}`.
10. **Doctor** (AD-94 part C). Pending and invalid proposal counts, and `.tmp` files older than 24 h. Proof: host `crates/officina-host/src/doctor.rs::tests::proposal_counts_and_stale_tmp_files_are_reported`.
11. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: proposals; README "The orchestrator (v0.5, in progress)": recaps and work order proposals, their statuses and limits, that a proposal acts on nothing until the author opens and launches it. Proof: review.

## Tasks

- [x] `crates/officina-core/src/orders/proposal.rs`: schema, `validate`, `ProposalInvalidReason`, statuses (AC 2, 3)
- [x] `proposal` section of `orders.json` (AC 2)
- [x] Host ingestion in the collector: name check, bounded read, limits by deletion, diagnostics; `.tmp` cleanup; expiry and supersession (AC 1, 3)
- [x] `IssueCardFromProposal`, dismiss; commands, `ipc.ts`, mock (AC 4)
- [x] Briefing section, view proposals (AC 5)
- [ ] Meeting recap (AC 5): waits for story 5.10's `MeetingView` (D-791)
- [x] `ProposalList.svelte`; card label and rationale (AC 6)
- [x] `examples/mcp_crash_helper.rs`, `tests/proposals_crash.rs`, the killed `officina mcp` case in `tests/mcp.rs` (AC 7)
- [x] `proposal_file` section (AC 8)
- [x] Retention, doctor (AC 9, 10)
- [x] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-1's proposals, FR-5's recap, FR-12, FR-13's list, FR-14, PRD section 7 Gesture and Restart rows, journey 5. Decisions: AD-38, AD-78, AD-83, AD-84, AD-91, AD-92, AD-93, AD-95. Risks: R43. Epic decisions: D-57, D-89 (Epic 3); P-24, D-124, D-130, D-133, D-134, D-144, D-145, D-147, D-152, D-155, D-165.
- Answer proposals (kind `answer`, AD-182) are story 5.21's (D-575): until it merges, this story's file schema knows no `answer` kind, so such a file is stored `invalid` as `unparsable` and acts on nothing; 5.21 adds its validation and status rules to the same `validate`.
- `ProposalList.svelte` adds no colour and no toggle: Open as card and Dismiss are plain buttons, and a dismissed proposal leaves the list (D-564); its Playwright cases run in both themes.
- Seams (D-124, read through `main`, D-563): the collector's fingerprinting of `relay/` (2.7), the writer (3.1), `AuthorRequest` (3.5), the card store (5.7), the briefing and view (5.8), the meeting (5.10), `officina mcp`'s writer (5.15).
- Validation is the window's, at ingestion, never the tool's (amendment 7): a file written by hand meets the same rules, which is what the `proposal_file` section proves.
- `proposal.invalid_reason` has no `CHECK` (D-133): adding a reason here needs no migration.
- D-89: the crash test must be able to fail; the helper's renamed-file log is what makes a lost proposal visible.
- No employer content in `orders.json`, the injection cases or the fixtures.

## Testing

- Core unit: `crates/officina-core/src/{orders/proposal,orders/briefing,orders/view,meeting}.rs`; core integration: `crates/officina-core/tests/orders_vectors.rs`.
- Host: `crates/officina-host/tests/{proposal_ingest,proposals_crash,registry_retention}.rs`, `src/doctor.rs`.
- CLI integration: `crates/officina/tests/{watch,injection}.rs`. App crate: `main.rs`.
- Vitest: `components.test.ts`. Playwright: `app/e2e/orchestrator.spec.ts`.

## Dev Agent Record

### Debug Log

- The view's two-theme test allows only the surface as a background: a proposal item drawn on `--color-bg` broke it, so items take no background of their own.
- The doctor's order test gained the `orchestrator proposals` line (`doctor.rs::tests::run_reports_every_check_in_order`); the refresh plan's start test gained `proposals` (`watch.rs::tests::startup_lists_sessions_checks_the_contract_and_builds_a_board`).
- One `mcp_tool` case (`launch-now text as rationale and new subject`) is a valid proposal: AC 8's "asserted invalid" is held as each written case's `ingested` outcome (D-793).

### Completion Notes

Every AC but AC 5's meeting recap is implemented and proven by the tests named in it, with these as built (decisions D-782 to D-795, epic-5-decisions.md "Implementation (2026-10-29)"; D-785, D-787, D-790, D-793 and D-794 to confirm):

- AC 1: host `proposal_ingest.rs::{a_file_is_ingested_once_then_deleted, a_file_left_after_its_commit_is_ignored_by_the_key, tmp_files_are_never_read_and_go_after_24_h, an_unparsable_file_is_kept_as_invalid_without_kind, a_misnamed_file_is_deleted_and_counted, a_file_over_16_kib_is_oversized_without_texts, ten_thousand_files_past_the_limit_create_no_row}`; host `watch.rs::tests::proposal_files_are_ingested_at_start_on_change_and_with_their_poll` (D-784, D-786).
- AC 2: core `orders_vectors.rs::{proposal_cases_match, every_invalid_reason_has_a_case}` over the `proposal` section of `tests/vectors/orders.json`; core `orders/proposal.rs::tests::{each_check_has_its_reason, a_valid_order_stores_the_templates_strings, unread_files_have_no_kind, ingestion_holds_the_limits_on_earlier_proposals}` (D-782, D-783).
- AC 3: core `orders/proposal.rs::tests::{a_work_order_proposal_expires_after_24_h, a_newer_recap_supersedes_the_pending_one, recaps_do_not_expire}`; host `proposal_ingest.rs::{texts_read_back_untrusted, a_newer_recap_supersedes_and_an_order_expires}` (D-787).
- AC 4: core `orders/card.rs::tests::{a_proposal_new_name_is_a_suggestion_not_a_default, a_proposal_existing_subject_is_the_default}`; CLI `watch.rs::{a_valid_proposal_opens_as_a_card_through_validate, launching_a_proposal_card_accepts_the_proposal, an_invalid_or_expired_proposal_opens_no_card}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` (D-788).
- AC 5: core `orders/briefing.rs::tests::pending_proposals_are_listed_and_received_ones_are_changed`, `orders/view.rs::tests::proposals_are_pending_invalid_and_last_50_decided`, the `proposals` briefing case of `orders.json` (`every_change_and_run_kind_has_a_briefing_case` covers `proposal_received`) (D-789). **Open: the meeting's recap section**, which waits for story 5.10's `MeetingView`, not on this branch. Built for it: `orders::proposal::latest_recap(rows, session_id, subject_id)`, tested by `orders/proposal.rs::tests::the_latest_pending_recap_of_a_session_or_its_subject`. To add once 5.10 merges: `MeetingView.recap: Option<ProposalView>`, filled in `meeting::build` from `latest_recap` over the orders state's proposal rows, shown under the "proposal" label in the meeting panel, and the test `meeting.rs::tests::the_latest_recap_shows_under_its_label` (D-791).
- AC 6: `orchestrator.spec.ts > proposals are listed with their status and reason (light, dark)`, `> a valid proposal opens as a card with its rationale under the label`, `> dismiss sends dismiss_proposal`, `> a suggested subject name is shown, never preselected`; Vitest `components.test.ts > ProposalList renders markup in proposal texts as text`, `> every invalid reason has a wording`, `> Open as card and Dismiss call back with the proposal's id`; fixtures `orchestrator-day` (proposals ingested by the window's writer) and `card-proposal` (issued from one) through `orchestrator_fixture` (D-790).
- AC 7: host `proposals_crash.rs::{killed_writers_lose_and_duplicate_nothing, the_cursor_after_restart_is_the_previous_start}` with `examples/mcp_crash_helper.rs`; CLI `mcp.rs::killing_officina_mcp_during_propose_recap_loses_and_duplicates_nothing`; all ignored, run by CI's new step "A restart loses and duplicates no proposal" (D-792).
- AC 8: CLI `injection.rs::{proposal_file_cases, mcp_tool_cases, mcp_output_cases, every_section_family_and_refusal_has_a_case}` (D-793).
- AC 9: host `registry_retention.rs::{proposal_rows_are_deleted_at_90_days, proposal_texts_are_cleared_with_the_excerpt_ttl}` (D-794).
- AC 10: host `doctor.rs::tests::proposal_counts_and_stale_tmp_files_are_reported` (D-785).
- AC 11: `CHANGELOG.md` `[Unreleased]` Added; README "Proposals" under "The orchestrator (v0.5, in progress)".

No new dependency and no new feature (Cargo or npm).

## File List

- `crates/officina-core/src/orders/{proposal,card,briefing,view}.rs`, `src/registry/retention.rs`, `tests/{orders_vectors,typescript_bindings}.rs`
- `crates/officina-host/src/registry/proposals.rs` (new), `src/registry/{mod,orders,retention,window}.rs`, `src/{watch,orders,mcp,files,doctor}.rs`, `examples/mcp_crash_helper.rs` (new), `examples/orchestrator_fixture.rs`, `tests/proposal_ingest.rs` (new), `tests/proposals_crash.rs` (new), `tests/registry_retention.rs`, `Cargo.toml`
- `crates/officina/tests/{watch,injection,mcp}.rs`, `tests/common/orders.rs`
- `tests/vectors/{orders,injection}.json`
- `app/src-tauri/{build.rs,capabilities/default.json,src/main.rs}`
- `app/src/components/ProposalList.svelte` (new), `app/src/components/{OrchestratorView,WorkOrderCard,Composer,Briefing}.svelte`, `app/src/lib/{ipc.ts,orchestrator.svelte.ts,orchestrator-text.ts}`, `app/src/lib/generated/` (regenerated), `app/src/e2e-mock.ts`, `app/src/components/components.test.ts`, `app/scripts/e2e-fixtures.mjs`, `app/e2e/orchestrator.spec.ts`
- `.github/workflows/ci.yml`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this file

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: unparsable files keep a null `kind` (F3); a proposed new subject name is a suggestion, never a default (F7); bounded, named and rate-limited ingestion by deletion (F21); the title and overlay assertion left to 5.18 (F40); rationale held as `Untrusted<String>` (F43); the real `officina mcp` killed during `propose_recap` (F45)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: `ProposalView` (D-576); an `answer` file is invalid until story 5.21 (D-575); no colour or toggle added, both themes (D-564)
- 2026-10-29 implemented, every AC but AC 5's meeting recap (waits for story 5.10); decisions D-782 to D-795
