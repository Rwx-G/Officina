# Story 8.7: Scope on the approval card and Slice A's doctor (Slice A)

**Status:** Approved

## Story

As the author clicking Approve on a write my scope sent to me,
I want the card to say why it came to me, which hosts the inventory listed and what Ansible printed beside them, and doctor to name every write my catalogue leaves without a dry run, a targets action or a scope,
so that I approve with the target in front of me, and I see where my declarations still leave gaps before a session meets them.

## Acceptance Criteria

1. **Entry condition** (FR-0, P-72, D-404). 8.6 has merged; 8.2's Decision time row does not hold this story, or the card has been revisited as 8.2 recorded. Proof: 8.2's entry and `epic-8-decisions.md`.
2. **`ApprovalView.scope`** (AD-162, FR-9, P-73, D-1306). Core `relay::approvals`, where `ApprovalView` and `DryRunView` are built today, builds `scope: Option<ScopeView { cause, names: Option<Vec<Untrusted<String>>>, names_missing: Option<String>, stderr: Option<Untrusted<String>>, resolved_at_ms }>` for a request with a `scope_cause`, whatever the cause, whenever its write declares a targets action (D-411, review F7): the names re-parsed from the kept stdout of the request's `targets_run_id` (review F10) and shown only when their `names_sha256` equals the request's `targets_sha256`, otherwise `names_missing` with the reason (resolution failed, pruned, hash differs, unreadable); the stderr from the kept file, inert. The kept files sit inside the gate's `PROTECTED_DIRS` (D-1308); the hash check stays the guarantee. `BOARD_SCHEMA` (5 at this rework) goes up by one relative to `main` at this merge unless 8.8 already moved it for v0.8 with no release in between (P-73; from whatever `main` holds, v0.7's bump included if it merged first, D-1301). The TypeScript bindings are regenerated. Proof: core `crates/officina-core/src/relay/approvals.rs::tests::{scope_view_shows_names_whose_hash_matches, a_target_not_in_scope_shows_its_resolved_names, scope_view_names_missing_when_the_resolution_failed, scope_view_names_missing_when_the_hash_differs, scope_view_names_missing_when_pruned, stderr_is_untrusted}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; the classification vector replay.
3. **The card** (AD-163, FR-9, amendment 10, D-1309). `ApprovalCard.svelte` (shown in Board mode's Needs you column, `BoardColumn.svelte`) gains `ScopeBlock.svelte`: the cause in words, the names as one chip per name inside a box of fixed maximum height that scrolls inside, so hundreds of hosts keep the column's layout; the stderr in a `<pre>` under the same bound; every text rendered with `{text}`; the sentence that the names were read from the inventory and are read again just before the run. Existing tokens only; nothing joins the header. The cause's words come from `relay-text.ts`'s `Record<OutOfScopeCause, string>`, which 8.10 owns (D-433, review F21); if this story merges before 8.10, it adds that record and `Record<ScopeStanding, string>` with 8.10's Vitest case, and 8.10 keeps them. Proof: Vitest `app/src/components/components.test.ts > ScopeBlock renders one chip per name`, `> markup and bidi characters in names and stderr stay inert text`; `app/src/lib/relay-text.test.ts > every out-of-scope cause and standing has a text`; Playwright `app/e2e/relay-scope.spec.ts > an out-of-scope approval shows its cause, names and stderr ({light,dark})`, `> a request whose names hash differs shows names missing`, `> two hundred names scroll inside the scope block`; `app/e2e/stable-size.spec.ts`'s `CASES` gains "scope block" (the card's buttons keep their place whatever the names' count).
4. **Fixtures** (D-25, AD-162, D-1306). `crates/officina-host/examples/approve_fixture.rs` gains an out-of-scope request with a kept targets output, one whose kept output was edited after the request, and one with two hundred names; `app/scripts/e2e-fixtures.mjs` produces an `approvals-scope` scenario under the existing `board` kind of `app/e2e/fixture-index.ts`, from the real CLI. Proof: the local gate's fixture step (D-1303) and the Playwright cases above.
5. **Doctor, Slice A** (AD-164, FR-10, D-421, D-433, D-1314). Each line is a `ScopeCheck` variant (8.4) with its level. The session wording, in guarded projects: a write without a `check` ("an out-of-scope call is refused `no_dry_run`"); a write with a target and no targets action ("scope judged on names only" for sessions, and every write when D-403 left the resolver unbuilt); a guarded project without a scope ("every write takes story 2.8's path"); a free project with a scope ("scope not enforced for sessions; binds worker dry runs", review F33). Each is a warning naming the project and action. The lines for projects with dry runs listed for workers are 8.15's (review F21). Proof: CLI `crates/officina/tests/doctor_relay.rs::{a_write_without_check_is_named, a_write_judged_on_names_only_is_named, a_guarded_project_without_scope_is_named, a_free_project_with_scope_is_named}`.
6. **CLI** (AD-162). `officina board --json` carries the scope block. Proof: CLI `crates/officina/tests/board.rs::an_out_of_scope_request_carries_its_scope_view`.
7. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: the scope block on the approval card. README "Relay scope and workers (v0.8, in progress)": what the card shows for an out-of-scope write and what it does not bind (PRD section 4.1, R86). Proof: review.

## Tasks

- [ ] `ScopeView` in core, `BOARD_SCHEMA`, bindings (AC 2)
- [ ] `ScopeBlock.svelte`, `relay-text.ts` records (AC 3)
- [ ] Fixtures (AC 4)
- [ ] Doctor items (AC 5); `board --json` (AC 6)
- [ ] `CHANGELOG.md`, README (AC 7)

## Dev Notes

- Requirements: FR-9 (approval card), FR-10 (Slice A items), section 8 question 1 as amended (amendment 10). Decisions: AD-35, AD-162, AD-163, AD-164. Risks: R86. Epic decisions: D-25, D-27, D-130, D-163, D-564, D-620; P-71, P-72, P-73, D-400, D-403, D-404, D-410, D-411, D-421, D-423, D-433, D-1301, D-1303, D-1306, D-1308, D-1309, D-1314.
- Seams (D-400, read through `main` as built, D-1306): `ApprovalView` and `DryRunView` in core `relay/approvals.rs`, `ApprovalCard.svelte` in `BoardColumn.svelte` (2.8, 5.12), `approve_fixture` and the approvals scenario (2.8 AC 17), the board columns and Needs you (1.6, 2.8), relay doctor (2.10), `relay-text.ts`'s cause records (8.10), `app/e2e/stable-size.spec.ts`.
- Waits for: 8.6; 8.2's Decision time row (P-71).
- The names on the card come from the kept stdout, re-parsed with `parse_targets`; they are untrusted text (an inventory a session can edit) and render as chips, never as markup. The hash check keeps a file edited after the request from showing other names than those the request bound.
- No employer content in fixtures: `host01`, `web01`, `example.org`.

## Testing

- Core unit: `crates/officina-core/src/relay/approvals.rs`; core integration: `crates/officina-core/tests/{typescript_bindings,classification_vectors}.rs`.
- CLI integration: `crates/officina/tests/{doctor_relay,board}.rs`.
- Vitest: `app/src/components/components.test.ts`, `app/src/lib/relay-text.test.ts`. Playwright: `app/e2e/relay-scope.spec.ts`, `app/e2e/stable-size.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the card shows resolved names for every out-of-scope write that declares a targets action, read through the request's `targets_run_id` (F7, F10, D-411); the cause texts are 8.10's records (F21, D-433); doctor keeps the session wording, the free-project line says it binds worker dry runs (F21, F33)
- 2026-11-02 reworked against v0.4 to v0.6 as built: `ScopeView` built in core `relay/approvals.rs` beside `ApprovalView` (D-1306); the names box of fixed maximum height with a stable-size case, the colour case in both themes, no header element (D-1309); kept files inside `PROTECTED_DIRS` (D-1308); doctor lines as `ScopeCheck` variants (D-1314); fixture script, kind and local gate (D-1303, D-1306)
