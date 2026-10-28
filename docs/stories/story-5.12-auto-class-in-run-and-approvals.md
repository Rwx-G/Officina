# Story 5.12: The `auto` class in `officina run` and on the approval card

**Status:** Approved

## Story

As the author classing some guarded actions `auto`,
I want an `auto` action requested and waited on exactly like a `human` one, its approval file saying who approved it, and its card asking why when I reject it,
so that the review of story 5.13 plugs into a path that already works, and every author decision on an `auto` request is evidence the gate can count (J3).

## Acceptance Criteria

1. **`officina run`** (AD-86, AD-33 as amended, FR-7). `classes::is_available(Auto)` becomes true; `auto` takes `human`'s path unchanged: request file, wait (`--wait`, default 110 s), single-use consumption by rename, the 15-minute block after a rejection; D-20's `class_not_available` goes for `auto`; when an `auto` action names a `check`, the `after-check` rule applies first, and again just before an approval of it is consumed, whoever gave it: `run` evaluates `check_satisfied` with a tree fingerprint computed at that moment and, when it fails, refuses `check_not_satisfied`, leaves the approval unconsumed and journals the refusal, so an edit made during the wait cannot run unchecked (D-151, review F6); `run --list` shows each action's class. Proof: CLI `crates/officina/tests/run.rs::{auto_creates_a_request_and_waits_like_human, auto_with_a_check_is_refused_until_the_check_passed, an_auto_approval_is_refused_after_a_tree_change, the_refused_approval_stays_unconsumed, an_auto_rejection_blocks_for_15_minutes, run_list_shows_the_class}`; `crates/officina/tests/approvals.rs::an_auto_approval_while_waiting_runs_once`; the `relay/classes.rs` doctest now asserts `is_available(ActionClass::Auto)`.
2. **The approval file** (AD-86, AD-25 as amended). `<id>.approved.json` gains an optional `by` (`author`, `review`, `rule`) and `review_id`, parsed with `serde(default)` (absent reads as `author`); `officina run` consumes a valid approval whatever its `by`; the journal's `approval_given` gains `by`. Proof: core `crates/officina-core/src/relay/approvals.rs::tests::{an_approval_without_by_reads_as_author, by_and_review_id_round_trip}`; core `crates/officina-core/src/relay/journal.rs::tests::approval_given_carries_by`.
3. **`ApprovalView`** (AD-92, AD-35 as amended, P-26). `ApprovalView` gains `class` (`human`, `auto`) and `state` (`pending`, `reviewing`); `waiting_count` counts `pending` requests only; `BOARD_SCHEMA` goes up by one relative to `main` at merge. Until story 5.14 turns live mode on, every request is `pending`. Proof: core `crates/officina-core/src/board.rs::tests::{approvals_carry_their_class_and_state, reviewing_requests_are_not_counted_as_waiting}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; CLI `crates/officina/tests/board.rs::board_classifies_the_fake_sessions_and_joins_hook_events` (compares `schema` with the constant).
4. **Decisions with a reason, one per request** (AD-86, AD-92, FR-9, D-157; review F26). `decide_approval(id, approve, reason?)`: rejecting an `auto` request requires `should_not_run` or `not_now` and is refused without one; a `human` rejection takes none; an approval by the author writes `by = "author"`; the reason is written to the `.rejected.json` file and the `approval_rejected` journal line. `decide_approval` refuses a request in state `reviewing`; every decision first creates `relay/approvals/<id>.decided` with `create_new`, so the first writer wins and a second is refused `already_decided` before it writes anything (story 5.14's review approval uses the same marker); the marker is deleted with the request's other files. Proof: host `crates/officina-host/tests/authority.rs::{rejecting_auto_requires_a_reason, a_human_rejection_takes_no_reason, an_author_approval_says_by_author, a_second_decision_is_refused, two_concurrent_decisions_write_one, a_reviewing_request_cannot_be_decided}`; core `journal.rs::tests::approval_rejected_carries_the_reason`.
5. **The card** (AD-35 as amended, AD-93, D-28). An `auto` request is rebuilt from the approved catalogue (D-27) and shown at the head of Needs you like a `human` one, with its class; Reject on an `auto` card opens two pointer-only buttons, "should not run" and "not now"; Enter and Space on them do nothing. Proof: Playwright `app/e2e/approvals.spec.ts > an auto request shows its class`, `> rejecting an auto request asks should not run or not now`, `> the reason buttons ignore the keyboard`; Vitest `app/src/components/components.test.ts > the reason buttons ignore a click with detail 0`.
6. **Doctor** (AD-94 part B, its action part). Doctor lists the `auto` actions with their check and `auto_max_per_hour`. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::auto_actions_are_listed_with_check_and_cap`.
7. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]`: Added, the `auto` class, waiting for the author until the review ships; Changed, the board schema. README "The orchestrator (v0.5, in progress)": `class = "auto"`, the two rejection reasons and what they mean for the gate. Proof: review.

## Tasks

- [ ] `relay::classes`, `officina run`'s `auto` path with the check again at consumption, `run --list` class (AC 1)
- [ ] Approval file `by` and `review_id`; journal `by` and the rejection reason (AC 2, 4)
- [ ] `ApprovalView.class` and `state`, `waiting_count`, `BOARD_SCHEMA`, bindings (AC 3)
- [ ] `authority::decide_approval` with the reason, the `reviewing` refusal and the `.decided` marker; command signature, `ipc.ts`, mock (AC 4)
- [ ] Approval card class and reason buttons (AC 5)
- [ ] Doctor line (AC 6)
- [ ] Update `CHANGELOG.md` and the README (AC 7)

## Dev Notes

- Requirements: FR-7's first paragraph, FR-9's reasons, J3. Decisions: AD-25, AD-33 and AD-35 as amended, AD-86 (the `run` and file parts), AD-92. Epic decisions: D-17, D-20, D-22, D-27, D-28, D-29 (Epic 2); P-13 (Epic 3); P-24, P-26, D-124, D-132, D-149, D-151, D-157.
- Seams (D-124): `relay::classes` and `officina run` (2.5, 2.8), `relay::approvals`, `ApprovalView` and the approval card (2.8), `authority::decide_approval` (2.8), `BOARD_SCHEMA` (bumped by 2.8, 3.2 and 4.1 before).
- With no review yet, an `auto` request behaves as a `human` one: the author decides. The review worker (5.13) runs beside this path; it never changes `officina run`, which owns the consumption-time check of D-151 whoever approved. That check departs from AD-86's "`human`'s path unchanged" (P-24): the request file holds no tree, and the wait can reach 540 s with a reuse window of 60 minutes.
- This story is the first Epic 5 change to `Board` (P-26): story 5.16 adds `Board.orchestrator` under the same number.
- The author's decisions and reasons recorded here become the cases of the gate in story 5.14; 5.13 copies them onto the review row when one exists.

## Testing

- Core unit: `crates/officina-core/src/{relay/approvals,relay/journal,relay/classes,board}.rs`; core integration: `typescript_bindings.rs`.
- Host: `crates/officina-host/tests/authority.rs`, `src/relay/doctor.rs`. CLI: `crates/officina/tests/{run,approvals,board}.rs`.
- Vitest: `components.test.ts`. Playwright: `app/e2e/approvals.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the after-check rule applies again at consumption with a fresh tree (F6); one decision per request through a `.decided` marker, none while `reviewing` (F26)
