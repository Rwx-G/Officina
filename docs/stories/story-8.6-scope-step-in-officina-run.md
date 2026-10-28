# Story 8.6: The scope step in `officina run` (Slice A)

**Status:** Approved

## Story

As the author who wants a write I forgot to class `human` to stop short of production,
I want every write in a guarded project judged on its target against my declared scope before its class applies, an out-of-scope write sent to story 2.8's approval with its dry run whatever its class, and the hosts it reaches resolved and bound to my approval, then resolved again just before it is consumed,
so that autonomy is judged on the target, not on the repository, and an undeclared write fails closed (FR-3).

## Acceptance Criteria

1. **Entry condition** (FR-0, P-72, D-404). 8.2 has recorded a Slice A trigger; the owner has recorded question 0 (b) and confirmed question 10; story 2.8 and stories 8.3 to 8.5 have merged. Until then this story stays approved and unimplemented. Proof: 8.2's entry and `epic-8-decisions.md`.
2. **Step 6** (AD-154, AD-29 as amended, FR-3, D-411). In `officina run`, after steps 1 to 5 and before the class branch: 6a `scope::judge` with `Enforcement::Session`; 6b for `needs_targets`, `resolve_targets` inside the session's call through `run_child` with `JobMode::ChildOnly`, after which the call goes on (D-409), then `after_targets`; 6c `read`, `unenforced` and `in` take the class exactly as in v0.2 and v0.5; `out`, whatever the class, takes story 2.8's `human` path with the action's `check` (after-check rule, kept output, request bound to the check run and the tree, fingerprint recheck); a write naming no `check` is refused `no_dry_run`. Proof: CLI `crates/officina/tests/scope.rs::{a_read_runs_under_its_class, an_in_scope_write_runs_under_its_class, a_free_project_write_is_unenforced, each_out_of_scope_cause_takes_the_human_path_with_its_check, an_out_of_scope_free_write_needs_the_check_first, a_write_without_check_out_of_scope_is_refused_no_dry_run}` (the fourth iterates `OutOfScopeCause::ALL` per D-403's outcome).
3. **Every out-of-scope write with a targets action resolves** (D-411, review F7). When 6a gives `out` (`target_not_a_name`, `target_not_in_scope`, `kept_on_order`) for a write that declares a targets action, 6b still runs the resolution: the request records the names hash, or the `TargetsFailure` when it failed, and the card shows the hosts or `names_missing`. Proof: CLI `scope.rs::{an_out_of_scope_target_still_resolves_and_binds_its_names, a_failed_resolution_of_an_out_of_scope_write_is_recorded}`.
4. **The session's line** (AD-154 6b, D-411). The session reads exactly one stderr line from the resolution: `officina: targets: <N> hosts, in scope`, `officina: targets: <name> not in scope`, or `officina: targets: failed (<TargetsFailure>)`; a failed resolution counts as out of scope. Proof: CLI `scope.rs::{the_session_reads_one_targets_line, a_failed_resolution_takes_the_human_path}`.
5. **`check` on writes, read only where it applies** (AD-151, FR-3 as amended by amendment 4, D-411, review F11). With Slice A, phase 2 lets a write of any class name a `check`, used only out of scope; a read naming a check under a class that cannot use it stays an issue. The class branch reads a write's `check` for `after-check`, `human`, `auto` and every `out` write, never for an in-scope `free` write. Proof: core `crates/officina-core/src/relay/catalogue.rs::tests::{a_free_write_may_name_a_check, a_free_read_naming_a_check_is_an_issue}`; fixtures through `every_issue_kind_has_a_case`; CLI `scope.rs::an_in_scope_free_write_naming_a_check_runs_without_it`.
6. **The request** (AD-154, AD-33 as amended, D-411, review F10). `KeyInputs` and `ApprovalRequest` gain `targets_sha256` (or `targets_failure`) and `scope_cause`, optional and skipped when absent; `ApprovalRequest` also gains `targets_run_id`, outside the key since the hash binds; `approval_requested` gains `scope_cause`. The key of a request without them is unchanged. Proof: core `crates/officina-core/src/relay/approvals.rs::tests::{a_key_without_scope_fields_is_unchanged, targets_hash_and_cause_change_the_key, a_request_names_its_targets_run}` (the first against a key pinned before this story); `journal.rs::tests::approval_requested_with_scope_cause_round_trips`.
7. **The rebuild re-judges** (D-27, D-411, review F6). `approvals::rebuild` accepts a request whose action is not `human` only when it re-judges the rebuilt values (`judge` on the approved catalogue, `Enforcement::Session`) to `out`, or to `needs_targets` with the kept names of `targets_run_id` hashing to the request's `targets_sha256` and judged `out` by `after_targets`, or with a recorded `targets_failure`; otherwise the request is invalid with the reason "in scope: it runs under its class". The request's `scope_cause` is never trusted; `decidable` and `pending` follow the rebuild. Proof: core `approvals.rs::tests::{an_out_of_scope_free_request_rebuilds, a_free_request_judged_in_scope_is_invalid, a_forged_scope_cause_is_not_trusted}`; CLI `scope.rs::an_out_of_scope_free_write_is_approved_and_runs_once`.
8. **Consumption** (AD-154, D-170's pattern, D-411). Just before consuming an approval whose request has `targets_sha256` or `targets_failure`, after the fingerprint recheck, `run` resolves the targets again; a different names hash, or a different outcome, refuses `targets_changed`, leaves the approval unconsumed and journals the refusal. Proof: CLI `scope.rs::{a_changed_host_list_refuses_targets_changed_unconsumed, a_failed_resolution_at_consumption_refuses_unconsumed, an_unchanged_list_consumes_and_runs}`.
9. **`auto` out of scope** (AD-154 6c, AD-87 as amended, FR-3 as amended, D-412). `EscalationCause` gains `out_of_scope`, checked first. The window's precheck rebuilds the request (D-27), judges it again from the rebuilt values and the kept names (re-parsed from the kept stdout of the request's `targets_run_id`, used only when their hash equals the request's, otherwise `out_of_scope`), and escalates with no review call; the `auto_review` row records outcome `no_call`. An `auto` write in scope is reviewed as in v0.5. Proof: section `precheck` of `tests/vectors/auto.json` through `crates/officina-core/tests/auto_vectors.rs::{every_cause_has_a_case, causes_are_checked_in_their_order}`; CLI `crates/officina/tests/auto.rs::{an_out_of_scope_auto_write_escalates_without_a_review_call, an_in_scope_auto_write_is_reviewed}` (the fake claude's log counts zero review calls).
10. **Refusals and journal** (AD-154, AD-160, P-73). `RefusalReason` gains `no_dry_run` and, per D-403, `targets_changed`; `run_started` gains `scope` (`ScopeStanding`) and `targets_sha256`. The TypeScript bindings are regenerated. Proof: CLI `crates/officina/tests/run.rs::every_refusal_code_exits_126_with_one_journal_line` (iterates the reasons); core `journal.rs::tests::run_started_with_scope_round_trips`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types` (review F29).
11. **The second format bump** (AD-152, D-405, P-73). `CANONICAL_FORMAT` goes up by one relative to `main` at this merge; `catalogue::SCOPE_ENFORCED_FORMAT` names it; `autonomy_labels` adds `approval_required` to every write of every guarded project when the approved snapshot's format is below it and the current one is not, so the banner lists the writes FR-3 sends to story 2.8's path. Proof: `crates/officina-core/tests/catalogue_hash.rs::{golden_hash_is_pinned, a_previous_format_approval_is_void}`; section `labels` of `scope.json` through `scope_vectors.rs::the_enforcement_bump_requires_approval_for_every_guarded_write`.
12. **Mutants** (AD-165, D-148, D-422). Core features `mutant-scope-skip` (6a skipped) and `mutant-targets-skip` (6b skipped) join the `mutants` job: each compiles with `--no-run` and fails on a named case of `scope.json` and of `scope.rs`. Proof: the CI `mutants` job.
13. **Doctor's time warning** (P-6, D-411, review F30). Relay doctor's warning on the Bash tool's 10-minute cap adds a write's targets action `timeout_s` to its own and the wait, twice for a write that can reach an approval (the resolution at the request and again at consumption). Proof: host `crates/officina-host/src/relay/doctor.rs::tests::{the_time_warning_counts_the_targets_action, a_write_that_can_reach_approval_counts_it_twice}`.
14. **Zero write** (D-80, D-422). `quota_doctor.rs` gains a Slice A refusal and an approval with targets: the Claude config dir unchanged. Proof: CLI `crates/officina/tests/quota_doctor.rs::{a_scope_refusal_writes_nothing_under_the_config_dir, an_approval_with_targets_writes_nothing_under_the_config_dir}`.
15. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: scope enforcement in guarded projects; under Changed: the catalogue format goes up again and the first approval lists every write that now needs a click (R87, R93). README "Relay scope and workers (v0.8, in progress)": the decision for a write (described, `OutOfScopeCause` named), `no_dry_run`, `targets_changed`, the fail-closed defaults, free projects unenforced, and that for a host session a dry run can still write (PRD section 1). Proof: review.

## Tasks

- [ ] Step 6 in `officina run` with `run_child` for the resolution; the session's line (AC 2 to 4)
- [ ] Phase 2's `check` on writes; the class branch's check rule (AC 5)
- [ ] `KeyInputs`, `ApprovalRequest` with `targets_run_id`, `approval_requested` (AC 6)
- [ ] `approvals::rebuild`, `decidable`, `pending` (AC 7)
- [ ] Re-resolution at consumption (AC 8)
- [ ] `EscalationCause::OutOfScope`, the window's precheck (AC 9)
- [ ] Refusal reasons, `run_started` fields, bindings (AC 10)
- [ ] Format bump, `SCOPE_ENFORCED_FORMAT`, labels case (AC 11)
- [ ] Mutants (AC 12); doctor warning (AC 13); zero write (AC 14); `CHANGELOG.md`, README (AC 15)

## Dev Notes

- Requirements: FR-3 as amended (amendment 4), PRD question 10, section 7's Slice A table. Decisions: AD-29, AD-33, AD-87, AD-151, AD-152, AD-153, AD-154, AD-160, AD-165. Risks: R86, R87, R89, R93. Epic decisions: P-6, D-27, D-29, D-80, D-148, D-151, D-162, D-170; P-72, P-73, D-400, D-403 to D-405, D-408 to D-412, D-422, D-428.
- Seams (D-400): `officina run`'s class branches and its `check_step` call (2.5, 2.6, 2.8, 5.12), `approvals::KeyInputs`, `ApprovalRequest`, `approvals::rebuild`, `decidable` and `pending` (2.8 AC 3, 5.12), the consumption-time recheck (2.8 AC 19, AC 21, D-151), `RefusalReason` (2.5, 2.8), `run_started` (2.5), `relay::auto` and `EscalationCause` (5.13, 5.14, D-140), the `mutants` job (5.7, D-148), relay doctor's P-6 warning (2.10), `quota_doctor.rs` (D-80).
- Waits for: the entry condition (AC 1).
- The `auto` escalation lands here rather than with the card (8.7): between two merges an `auto` write out of scope must never reach its model.
- Why the rebuild re-judges (review F6): story 2.8's `rebuild` refuses every request whose action is not `human`, so without this the window could approve no out-of-scope `free` or `after-check` write; and trusting the request's own `scope_cause` would let a file any process can write (D-27) make an in-scope write look approvable. A recorded `targets_failure` is accepted because it can only send a write to an approval, never let one run.
- Two Ansible starts before each guarded write that declares a targets action, three when it goes to an approval (AD-154's stated cost); the journal holds their times for section 7's Friction row (8.16).
- No employer content in fixtures: `lab`, `prod-web`, `firewall.yml` as a placeholder playbook name taken from the PRD's journeys, `example.org`.

## Testing

- Core unit: `crates/officina-core/src/relay/{catalogue,approvals,journal}.rs`; core integration: `crates/officina-core/tests/{scope_vectors,auto_vectors,catalogue_hash,typescript_bindings}.rs`.
- Host unit: `crates/officina-host/src/relay/doctor.rs`.
- CLI integration: `crates/officina/tests/{scope,run,auto,quota_doctor}.rs`.
- CI: `mutants`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `approvals::rebuild` re-judges non-`human` requests and never trusts `scope_cause` (F6, D-411); every out-of-scope write with a targets action resolves and binds its names (F7); the request names its targets run (F10); the class branch reads a check only where it applies (F11); the resolution runs through `run_child` and the call goes on (F5, D-409); `target_not_a_name` among the causes (F3, D-428); bindings named (F29); the P-6 warning counts the targets timeout twice (F30)
