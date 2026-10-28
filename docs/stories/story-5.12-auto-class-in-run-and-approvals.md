# Story 5.12: The `auto` class in `officina run` and on the approval card

**Status:** Approved

## Story

As the author classing some guarded actions `auto`,
I want an `auto` action requested and waited on exactly like a `human` one, its approval file saying who approved it, and its card asking why when I reject it,
so that the review of story 5.13 plugs into a path that already works, and every author decision on an `auto` request is evidence the gate can count (J3).

## Acceptance Criteria

1. **`officina run`** (AD-86, AD-33 as amended, FR-7). `classes::is_available(Auto)` becomes true; `auto` takes `human`'s path unchanged: request file, wait (`--wait`, default 110 s), single-use consumption by rename, the 15-minute block after a rejection; D-20's `class_not_available` goes for `auto`; every approved `auto` action names its `check` (story 5.4's `AutoWithoutCheck`, D-570), so the `after-check` rule applies first on every `auto` request, and again just before an approval of it is consumed, whoever gave it: `run` evaluates `check_satisfied` with a tree fingerprint computed at that moment and, when it fails, refuses `check_not_satisfied`, leaves the approval unconsumed and journals the refusal, so an edit made during the wait cannot run unchecked (D-151, review F6); `run --list` shows each action's class. Proof: CLI `crates/officina/tests/run.rs::{auto_creates_a_request_and_waits_like_human, auto_with_a_check_is_refused_until_the_check_passed, an_auto_approval_is_refused_after_a_tree_change, the_refused_approval_stays_unconsumed, an_auto_rejection_blocks_for_15_minutes, run_list_shows_the_class}`; `crates/officina/tests/approvals.rs::an_auto_approval_while_waiting_runs_once`; the `relay/classes.rs` doctest now asserts `is_available(ActionClass::Auto)`.
2. **The approval file** (AD-86, AD-25 as amended). `<id>.approved.json` gains an optional `by` (`author`, `review`, `rule`) and `review_id`, parsed with `serde(default)` (absent reads as `author`); `officina run` consumes a valid approval whatever its `by`; the journal's `approval_given` gains `by`. Proof: core `crates/officina-core/src/relay/approvals.rs::tests::{an_approval_without_by_reads_as_author, by_and_review_id_round_trip}`; core `crates/officina-core/src/relay/journal.rs::tests::approval_given_carries_by`.
3. **`ApprovalView`** (AD-92, AD-35 as amended, P-26). `ApprovalView` gains `class` (`human`, `auto`) and `state` (`pending`, `reviewing`); `waiting_count` counts `pending` requests only; `BOARD_SCHEMA` goes up by one relative to `main` at merge (4 on `main` today). Until story 5.14 turns live mode on, every request is `pending`. Proof: core `crates/officina-core/src/board.rs::tests::{approvals_carry_their_class_and_state, reviewing_requests_are_not_counted_as_waiting}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; CLI `crates/officina/tests/board.rs::board_classifies_the_fake_sessions_and_joins_hook_events` (compares `schema` with the constant).
4. **A reviewing request is no wait in the scene** (v0.4 AD-167 rule 3 and D-496 as D-571 reads them). `officina-core::scene` reads `state == pending` only wherever it reads an approval: rule 3's approval-only wait, `alert_count`, `next_escalation_at_ms` and the approval ticket in the decision office's queue (D-464); a request in `reviewing` waits on the review, not on the author, so its session stays in its office and no alert, title count or taskbar badge follows from it; a request that turns `pending` (an escalation) becomes an approval-only wait from then on. Proof: new cases in `tests/vectors/scene.json` ("a reviewing approval leaves the session in its office", "an escalated approval queues it", "a reviewing approval never alerts") through `crates/officina-core/tests/scene_vectors.rs`, whose coverage test stays green.
5. **Decisions with a reason, one per request** (AD-86, AD-92, FR-9, D-157; review F26). `decide_approval(id, approve, reason?)`: rejecting an `auto` request requires `should_not_run` or `not_now` and is refused without one; a `human` rejection takes none; an approval by the author writes `by = "author"`; the reason is written to the `.rejected.json` file and the `approval_rejected` journal line. `decide_approval` refuses a request in state `reviewing`; every decision first creates `relay/approvals/<id>.decided` with `create_new`, so the first writer wins and a second is refused `already_decided` before it writes anything (story 5.14's review approval uses the same marker); the marker is deleted with the request's other files. Proof: host `crates/officina-host/tests/authority.rs::{rejecting_auto_requires_a_reason, a_human_rejection_takes_no_reason, an_author_approval_says_by_author, a_second_decision_is_refused, two_concurrent_decisions_write_one, a_reviewing_request_cannot_be_decided}`; core `journal.rs::tests::approval_rejected_carries_the_reason`.
6. **The card** (AD-35 as amended, AD-93, D-28, D-564). An `auto` request is rebuilt from the approved catalogue (D-27) and shown by `ApprovalCard.svelte` at the head of Needs you like a `human` one, with its class as text and its check named; the session file's approval line (D-464) shows the class too. Reject on an `auto` card reveals two pointer-only buttons, "should not run" and "not now", in a slot the card reserves (`data-stable-slot`), so revealing them moves neither the card's other buttons nor the cards below; Enter and Space on them do nothing. No colour is added: the class and the buttons use the approval card's tokens in both themes. Proof: Playwright `app/e2e/approvals.spec.ts > an auto request shows its class and its check`, `> rejecting an auto request asks should not run or not now`, `> the reason buttons ignore the keyboard`, `> the approval card is the same in both themes but for its tokens` (light and dark); `app/e2e/stable-size.spec.ts`, a `CASES` entry "auto rejection reasons" on a scene showing an `auto` request (a `SCENES` entry fed by a relay fixture with one); `app/e2e/scene.spec.ts > the file of a session with an auto request names its class`; Vitest `app/src/components/components.test.ts > the reason buttons ignore a click with detail 0`.
7. **Doctor** (AD-94 part B, its action part). Doctor lists the `auto` actions with their check and `auto_max_per_hour`. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::auto_actions_are_listed_with_check_and_cap`.
8. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]`: Added, the `auto` class, waiting for the author until the review ships; Changed, the board schema. README "The orchestrator (v0.5, in progress)": `class = "auto"`, the two rejection reasons and what they mean for the gate. Proof: review.

## Tasks

- [ ] `relay::classes`, `officina run`'s `auto` path with the check again at consumption, `run --list` class (AC 1)
- [ ] Approval file `by` and `review_id`; journal `by` and the rejection reason (AC 2, 5)
- [ ] `ApprovalView.class` and `state`, `waiting_count`, `BOARD_SCHEMA`, bindings (AC 3)
- [ ] `officina-core::scene` reads pending approvals only; `scene.json` cases (AC 4)
- [ ] `authority::decide_approval` with the reason, the `reviewing` refusal and the `.decided` marker; command signature, `ipc.ts`, mock (AC 5)
- [ ] `ApprovalCard.svelte` class and reason slot, the file's approval line; stable-size case and scene (AC 6)
- [ ] Doctor line (AC 7)
- [ ] Update `CHANGELOG.md` and the README (AC 8)

## Dev Notes

- Requirements: FR-7's first paragraph, FR-9's reasons, J3, PRD section 9 item 5 (read, not owned: D-570). Decisions: AD-25, AD-33 and AD-35 as amended, AD-86 (the `run` and file parts), AD-92; v0.4 AD-167 rule 3. Epic decisions: D-17, D-20, D-22, D-27, D-28, D-29 (Epic 2); P-13 (Epic 3); D-464, D-496 (Epic 4); P-24, P-26, D-124, D-132, D-149, D-151, D-157, D-563, D-564, D-570, D-571.
- Seams (D-124, read through `main`, D-563): `officina-core::relay::classes`, `relay::approvals` and `relay::run`; `crates/officina-host/src/relay/authority.rs` (`decide_approval(home, id, approve)` today) and `officina run`'s `human` path; `ApprovalView` and `BOARD_SCHEMA` in `officina-core::board`; `officina-core::scene` and `tests/vectors/scene.json` (v0.4); `app/src/components/ApprovalCard.svelte`, `SessionFile.svelte`, `app/e2e/approvals.spec.ts` and `stable-size.spec.ts`.
- AC 4 lands here, with the field, because the field's meaning and its only reader must not disagree for a single merge: from 5.14 on, a live review would otherwise send an employee to the decision office's queue for a request the author cannot decide (D-157 refuses a decision while `reviewing`).
- With no review yet, an `auto` request behaves as a `human` one: the author decides. The review worker (5.13) runs beside this path; it never changes `officina run`, which owns the consumption-time check of D-151 whoever approved. That check departs from AD-86's "`human`'s path unchanged" (P-24): the request file holds no tree, and the wait can reach 540 s with a reuse window of 60 minutes.
- This story is the first Epic 5 change to `Board` (P-26): story 5.16 adds `Board.orchestrator` under the same number.
- The author's decisions and reasons recorded here become the cases of the gate in story 5.14; 5.13 copies them onto the review row when one exists.

## Testing

- Core unit: `crates/officina-core/src/{relay/approvals,relay/journal,relay/classes,board,scene}.rs`; core integration: `typescript_bindings.rs`, `scene_vectors.rs` with `tests/vectors/scene.json`.
- Host: `crates/officina-host/tests/authority.rs`, `src/relay/doctor.rs`. CLI: `crates/officina/tests/{run,approvals,board}.rs`.
- Vitest: `components.test.ts`. Playwright: `app/e2e/{approvals,stable-size,scene}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the after-check rule applies again at consumption with a fresh tree (F6); one decision per request through a `.decided` marker, none while `reviewing` (F26)
- 2026-10-28 reworked against the addendum and v0.4 as built: every `auto` request takes the after-check rule, the check being mandatory (D-570); a reviewing request is no wait in v0.4's scene (AC 4, D-571); the card and the file as built, the reason buttons in a reserved slot with a stable-size case, both themes (D-563, D-564)
