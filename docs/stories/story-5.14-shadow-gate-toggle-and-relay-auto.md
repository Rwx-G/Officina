# Story 5.14: Shadow statistics, the toggle, live and rule modes, and the Relay view's `auto` section

**Status:** Approved

## Story

As the author who will let reviews approve only once my own decisions say they may,
I want the shadow results counted against my decisions and a plain rule, a toggle offered only when the evidence holds, a way to take it back with one mark, and the option to replace the model by the rule when it adds nothing,
so that `auto` approves what I would have approved, I can see how weak that evidence still is, and one wrong run turns it off (J3, FR-9).

## Acceptance Criteria

1. **Statistics** (AD-88, FR-9, D-143; review F16). `auto::shadow_stats(rows, now_ms)` covers the last 30 days: cases are rows of mode `shadow` with outcome `answered` that met an author decision (approve, or reject with a reason; expiry is none); live rows never count, since a live escalation showed its cause before the author decided; a disagreement is a review `approve` against a reject `should_not_run`, or against a "should not have run" mark; `rule_diff` counts cases where "review approved" differs from the baseline; the approval gate holds with at least 20 cases and no disagreement; the rule offer holds with the gate and `rule_diff` under 5 % of cases; the bound 3/n is reported. Proof: the `stats` section of `tests/vectors/auto.json` through `crates/officina-core/tests/auto_vectors.rs::stats_cases_match` (19 and 20 cases, one disagreement, "not now" not counting, expiry not counting, live rows not counting, a mark counting, 4 % and 5 % rule difference, a case older than 30 days).
2. **The toggles** (AD-88, D-69's pattern). `set_auto_approval(true)` is refused by the writer unless the gate holds when it writes; off is always allowed; the `setting` row `auto_approval` keeps source and reason; `relay::auto::AutoMode` (`review`, `rule`) is stored as text in `setting.auto_mode` (D-162); `set_auto_mode(rule)` is refused without the rule offer, `set_auto_mode(review)` always allowed; each change journals `auto_approval_set`. Proof: host `crates/officina-host/tests/registry_review.rs::{turning_on_is_refused_without_the_gate, turning_on_is_refused_when_the_gate_broke_since_display, off_is_always_allowed, rule_mode_needs_the_rule_offer}`; core `crates/officina-core/src/relay/journal.rs::tests::auto_approval_set_round_trips`.
3. **Live mode** (AD-86, FR-7 as amended by amendment 4, D-142, D-143, D-157; review F15, F26). With auto approval on and mode `review`, a new `auto` request is `reviewing` (not counted in `waiting_count`) until its review answers; `approve` makes `authority::record_review_approval`, after checking `hourly_cap` once more, create the request's `.decided` marker with `create_new` (D-157) and write `<id>.approved.json` with `by = "review"` and `review_id` and journal `approval_given` with `by`; an author decision already made wins and the review approval is refused `already_decided`; `officina run` consumes it and runs; any other outcome makes the request a Needs you card with its cause; a request decided or expired before its turn is skipped. Proof: CLI `crates/officina/tests/auto.rs::{a_live_approve_runs_once_through_officina_run, a_live_reject_or_escalate_becomes_a_needs_you_card_with_its_cause, a_reviewing_request_is_not_counted_as_waiting, a_request_decided_before_its_turn_is_skipped}`; host `crates/officina-host/tests/authority.rs::{a_review_approval_says_by_review_with_its_id, a_review_approval_loses_to_an_author_decision, a_burst_cannot_approve_past_the_hourly_cap}`; CLI `auto.rs::run_output_never_holds_the_review_reason` (a marker reason, in shadow and live modes, searched in `officina run`'s stdout and stderr; FR-8, review F36).
4. **Rule mode** (AD-88, D-141). With auto approval on and mode `rule`, no model call runs; the precheck applies `no_session_id` and `hourly_cap` only; a true baseline approves with `by = "rule"`, a false one escalates with `rule_baseline_false` (added to `EscalationCause`); each request writes one row with mode `rule`, outcome `no_call` and decision `approve` for a true baseline or `escalate` with `rule_baseline_false` for a false one (D-162), so the cap reads review and rule rows the same way; story 5.12's consumption-time check still guards the tree (D-151). Proof: `auto.json` cases for `rule_baseline_false` through `auto_vectors.rs::every_cause_has_a_case`; CLI `auto.rs::{rule_mode_makes_no_call, rule_mode_approves_by_rule_and_escalates_a_false_baseline, rule_approvals_count_toward_the_hourly_cap}`.
5. **Taking it back** (AD-88, FR-9, D-141). The auto-approved runs of the last 7 days (approval `by` review or rule, linked to their run through `run_started.approval_id`) each offer "should not have run"; `mark_should_not_have_run(run_id)` stamps `marked_at_ms`, turns auto approval off with source `author` and the reason, sets the mode back to `review`, and counts as a disagreement, so turning on again needs the gate to hold again. Proof: host `registry_review.rs::{a_mark_turns_approval_off_and_resets_the_mode, a_mark_is_a_disagreement_for_30_days, runs_link_to_their_review_through_the_approval_id}`.
6. **Injection battery, auto-approved runs** (AD-95, PRD section 7 Auto fail-closed row, D-147). Section `auto_approved_run`: in live and rule modes, every approved run's argument vector in the journal equals the catalogue rebuild (D-27); a request file with a forged `argv` is invalid and never reviewed or approved; a catalogue change during a review makes its approval refused; a review answering approve with extra fields is `invalid_answer`; an approval given while the tree matched the check, then a tree change before consumption, runs nothing and leaves the approval unconsumed (D-151, review F6). Proof: `crates/officina/tests/injection.rs::{auto_approved_run_cases, every_section_family_and_refusal_has_a_case}`.
7. **`RelayView.auto`** (AD-92, P-26, D-142, D-143). `RelayView` gains `auto`: approval on or off with source and reason, mode, gate counts (cases, disagreements, `rule_diff`), the 3/n bound, the rule offer, reviews and calls used today against the budget, the review model, the auto-approved runs of the last 7 days, and the author's `human` plus `auto` decisions per UTC day over the last 14 days; each recent entry shows its review (decision, reason held as `Untrusted<String>`, baseline) once the author decided, and not before (D-130, review F43). `officina relay --json` prints the same section, reading the registry read-only (D-82), with decisions and baselines but never a review's reason, since every session can run it (D-143, review F17); the e2e scenario `relay-auto` is written by `examples/orchestrator_fixture.rs` (review rows and author decisions at explicit `now_ms`), then printed by `relay --json`. Proof: CLI `crates/officina/tests/relay.rs::{prints_the_auto_section_with_the_window_closed, relay_json_never_prints_a_review_reason}` (the second with a marker reason); core `crates/officina-core/src/relay/view.rs::tests::{auto_section_reports_the_gate_and_its_bound, author_decisions_per_day_count_human_and_auto, a_review_shows_only_after_the_author_decided}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
8. **The Relay view** (AD-93, FR-9, D-28). The Relay view gains an `auto` section: the counts and the sentence that 20 clean cases bound the false-approve rate only to about 15 % (3/n from the payload, never typed in the frontend); the toggle, shown only while the gate holds; the rule offer, shown only while it holds; the auto-approved runs with "should not have run"; the decisions per day. Turning approval or rule mode on, and "should not have run", act on a pointer click only; turning off is reachable by keyboard. The two toggles keep their box in every state (`aria-pressed` with `StableLabel`), and the toggle and the rule offer appear in slots the section reserves (`data-stable-slot`), so their coming and going moves nothing below (D-564); no colour is added. The approval card (`ApprovalCard.svelte`) shows `reviewing` without buttons while a live review runs, in the slot its buttons take, so the card keeps its height. Proof: Playwright `app/e2e/relay-view.spec.ts > the auto section shows the counts and the weakness of the evidence`, `> the toggle is offered only while the gate holds`, `> turning auto approval on needs a pointer click`, `> should not have run sends the mark`, `> the rule offer appears only with the gate and under 5 %`; `app/e2e/approvals.spec.ts > a reviewing request shows no buttons`; `app/e2e/stable-size.spec.ts`, `CASES` entries "auto approval toggle", "auto rule mode toggle" and "approval reviewing slot" on a relay scene with the gate holding (a `SCENES` entry fed by the `relay-auto` fixture); the Relay view cases run in the light and the dark theme.
9. **Commands** (AD-92). `set_auto_approval`, `set_auto_mode` and `mark_should_not_have_run(run_id)` are declared, allowed, thin wrappers over `AuthorRequest` with a deadline, wrapped in `ipc.ts` and mocked. Proof: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
10. **Doctor** (AD-94 part B). Auto approval and mode, the gate counts and the rule difference join the review lines of story 5.13. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::auto_state_and_gate_are_reported`.
11. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: the gated toggle, live and rule modes, "should not have run"; README "The orchestrator (v0.5, in progress)": the gate, its stated weakness, the rule offer, how to take approval back. Proof: review.

## Tasks

- [ ] `auto::shadow_stats`, the `stats` section (AC 1)
- [ ] Writer toggles and the mark; `auto_approval_set` (AC 2, 5)
- [ ] Live mode in the review worker and `authority::record_review_approval` with the cap check and the `.decided` marker; `reviewing` state (AC 3)
- [ ] Rule mode and `rule_baseline_false` (AC 4)
- [ ] Run linking through `approval_id` (AC 5)
- [ ] `auto_approved_run` section (AC 6)
- [ ] `RelayView.auto`, bindings, the CLI without reasons; the Relay view section; the approval card's reviewing state (AC 7, 8)
- [ ] Commands, `ipc.ts`, mock (AC 9)
- [ ] Doctor; `CHANGELOG.md` and README (AC 10, 11)

## Dev Notes

- Requirements: FR-7 as amended (amendment 4), FR-9, J3, journey 3, PRD section 7 Auto fail-closed and Auto rows. Decisions: AD-21 as amended (`record_review_approval` is authority code), AD-86, AD-88, AD-92, AD-93. Risks: R40. Epic decisions: D-27, D-28 (Epic 2); D-69 (Epic 3); P-24, P-26, D-124, D-130, D-141 to D-143, D-147, D-151, D-157, D-162.
- Seams (D-124, read through `main`, D-563): `crates/officina-host/src/relay/authority.rs` (2.8), `RelayView.svelte` and `RelayView` (2.6, 2.9; schema per P-26), `ApprovalCard.svelte` (2.8, 5.12), the review worker (5.13).
- The scene reads `pending` approvals only (story 5.12 AC 4, D-571): a live review's `reviewing` request leaves its session in its office, so turning live mode on here changes nothing in the scene; `scene.json`'s cases from 5.12 already hold it.
- Turning approval on is the decision that lets a review run a playbook, so it follows D-28's pointer-only rule; Start in the orchestrator's room and Send in the meeting do not (they run nothing in a repository).
- The 3/n bound and the 5 % threshold travel in the payload (AD-5); the frontend computes nothing.
- The Auto metric's "week before Slice B ships" reads the journal lines v0.2 already writes (`by` absent means author, D-142); 5.19 records both weeks.

## Testing

- Core unit: `crates/officina-core/src/{relay/auto,relay/view,relay/journal}.rs`; core integration: `crates/officina-core/tests/{auto_vectors,typescript_bindings}.rs`.
- Host: `crates/officina-host/tests/{registry_review,authority}.rs`, `src/relay/doctor.rs`.
- CLI integration: `crates/officina/tests/{auto,injection}.rs`. App crate: `main.rs`.
- Playwright: `app/e2e/{relay-view,approvals}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: a tree change before consumption runs nothing (F6); the cap checked again before a review approval (F15); gate cases are shadow rows only (F16); `relay --json` never prints a reason (F17); one decision per request (F26); `officina run` never prints the reason (F36); `AutoMode` and the rule row's decision (F41); stored reason as `Untrusted<String>` (F43)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: the toggles, the rule offer and the reviewing card keep their boxes with stable-size cases, both themes (D-564); the scene's reading of `reviewing` is story 5.12's (D-571); seams cited by file (D-563)
