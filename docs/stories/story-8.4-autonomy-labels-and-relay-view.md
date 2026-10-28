# Story 8.4: Autonomy labels, the Relay view's declarations and doctor's first v0.8 items

**Status:** Approved

## Story

As the author approving a catalogue change,
I want the banner to say which changes widen what sessions or workers may do alone and which writes now need my click, the Relay view to show each project's scope and each action's effect, and doctor to tell me where kept dry run outputs live,
so that I never approve a widening without seeing it named, and a kept `--diff` output does not sit in a synced folder unnoticed.

## Acceptance Criteria

1. **Labels** (AD-152, FR-9). Core `catalogue::autonomy_labels(approved, current) -> Vec<AutonomyLabel>` beside `diff`: `widens` with a `WidenCause` (`closed_enum!` seeded from FR-9's list: a scope `targets` or `hosts` value added, a pattern changed, an `on_order` value removed, an effect changed to `read`, a write's `target` or targets action removed, a bound path removed, an action added to `relay_actions`), and `approval_required` for each write whose scope inputs narrowed (a value removed, an `on_order` value added, an effect changed to `write`, a `target` added). A pattern change carries both labels: Officina does not compare patterns. From then on `WidenCause::ALL` is the list. Proof: section `labels` of `tests/vectors/scope.json` (formatter-exempt, D-408) through `crates/officina-core/tests/scope_vectors.rs::{labels_cases_match, every_widen_cause_has_a_case, a_pattern_change_carries_both_labels}` (the second iterates `WidenCause::ALL`).
2. **Relay view** (AD-162, FR-9, P-73). `RELAY_VIEW_SCHEMA` goes up by one relative to `main` at this merge. The pending change gains `labels`; `RelayView` gains per project its scope (targets, hosts, kept values) and bound paths; per action its effect, target, targets action, `dry_run_for_workers` (D-427: an action listed in some template's `relay_actions` that is the `check` of any write of the approved catalogue, whichever project lists that write, or an unscoped check-mode read; review F2) and, for a write, whether an out-of-scope call has a dry run (`no_dry_run` named when it has no `check`); v0.8's notice kinds reach the view through story 2.13's notice list, beside issues (D-406). The TypeScript bindings are regenerated. Proof: core `crates/officina-core/src/relay/view.rs::tests::{pending_change_carries_labels, scope_and_bound_paths_per_project, effect_and_dry_run_columns_per_action, a_check_of_a_write_another_project_lists_is_a_dry_run_for_workers, an_unscoped_check_mode_read_is_marked, v08_notices_are_listed_with_2_13s}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
3. **CLI** (AD-162). `officina relay --json` prints the new fields. Proof: CLI `crates/officina/tests/relay.rs::{relay_json_prints_labels_and_scope, relay_json_schema_matches_the_constant}`.
4. **Frontend** (AD-163, D-163). The catalogue banner renders "widens autonomy" and "approval now required" on neutral surfaces, never `--color-attention`; `ScopeTable.svelte` in the Relay view; the effect and dry-run columns. `app/src/lib/relay-text.ts` holds an exhaustive `Record<WidenCause, string>`, so a new Rust variant fails `pnpm check`; v0.8's notice kinds get their one-line explanations wherever story 2.13 keeps its own, exhaustively. No rule is computed in TypeScript. Proof: Vitest `app/src/lib/relay-text.test.ts > every widen cause has a text`, and story 2.13's notice-text case covering the v0.8 kinds; Playwright `app/e2e/relay-scope.spec.ts > the banner labels a widening and an approval now required`, `> labels never use the attention colour` (computed colours against `--color-attention` resolved on the root, D-163), `> the scope table shows each project's scope and kept values`.
5. **Fixtures** (D-25). `scripts/e2e-fixtures.mjs` gains a `relay-scope` scenario from the real CLI: a catalogue with a scope, kept values, a write with and without a `check`, approved through `approve_fixture`, then edited to widen one value and narrow another. Proof: the CI fixture step and the Playwright cases above.
6. **Doctor: kept outputs** (AD-164 first item, FR-10, D-410, D-421). Doctor reports the files under `relay/outputs/` (every suffix: story 2.8's `.txt` and the targets runs' `.stdout` and `.stderr`) by count, total size and oldest age, and runs AD-39's location check on that folder (`SYNC_ROOT_MARKERS`, a `OneDrive*` path, the Cloud Files attribute): a synced or backed-up folder is a warning naming the folder to exclude, since kept outputs may hold what `--diff` shows. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::{kept_outputs_are_counted_whatever_their_suffix, a_synced_outputs_folder_warns}`.
7. **Doctor: notices** (AD-164, D-406, D-421; review F1). The v0.8 notice kinds print through story 2.13's one warning line per notice, naming the action and its notice; notices never fail doctor. Proof: CLI `crates/officina/tests/doctor_relay.rs::{v08_notices_are_warnings, notices_alone_exit_0}` (the first iterates the notice kinds' `ALL`).
8. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: the labels, the scope table, doctor's kept-output check. README "Relay scope and workers (v0.8, in progress)": what each label means, `WidenCause` named and never listed; kept outputs, their 7 days and the synced-folder advice. Proof: review.

## Tasks

- [ ] `catalogue::autonomy_labels`, `WidenCause`; `scope.json` section `labels`; `scope_vectors.rs` (AC 1)
- [ ] `RelayView` fields, `RELAY_VIEW_SCHEMA`, bindings (AC 2)
- [ ] `relay --json` (AC 3)
- [ ] Banner labels, `ScopeTable.svelte`, columns, `relay-text.ts` (AC 4)
- [ ] Fixture scenario (AC 5)
- [ ] Doctor items (AC 6, 7); `CHANGELOG.md`, README (AC 8)

## Dev Notes

- Requirements: FR-9 (banner, Relay view), FR-10 (kept outputs, notices). Decisions: AD-39, AD-152, AD-162, AD-163, AD-164. Risks: R87, R92. Epic decisions: D-25, D-163, D-172; P-73, D-400, D-405, D-406, D-408, D-410, D-421, D-423, D-427.
- Seams (D-400): `RelayView` and `RELAY_VIEW_SCHEMA` (2.6, 2.9, 5.14), the banner and `RelayBanners.svelte` with story 2.13's notice lines (2.7, 2.9, 2.13), the catalogue table (2.9, 5.4), relay doctor (2.10, 2.13), story 3.1's location classification, story 2.8's kept outputs (2.8 AC 20), `e2e-fixtures.mjs` and `approve_fixture` (2.7, 2.8).
- Waits for: 8.3.
- Slice A's bump adds `approval_required` to every write of every guarded project; that rule needs `SCOPE_ENFORCED_FORMAT`, which 8.6 creates with its bump (D-405). This story's labels compare declarations only.
- `dry_run_for_workers` lets the Relay view and doctor (8.15) name the dry runs a worker may request, which the check-mode scan sits beside. It is catalogue-wide (D-427): a check listed by a project that does not list its write is still a dry run, judged by 8.10's step 9.
- No employer content in fixtures: `lab`, `prod-web`, `example.org`.

## Testing

- Core unit: `crates/officina-core/src/relay/{catalogue,view}.rs`; core integration: `crates/officina-core/tests/{scope_vectors,typescript_bindings}.rs`.
- Host unit: `crates/officina-host/src/relay/doctor.rs`.
- CLI integration: `crates/officina/tests/{relay,doctor_relay}.rs`.
- Vitest: `app/src/lib/relay-text.test.ts`. Playwright: `app/e2e/relay-scope.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `dry_run_for_workers` is catalogue-wide and includes unscoped check-mode reads (F2, D-427); notices ride story 2.13's notice list, texts and doctor line, so no issue level is built (F1, D-406, P-74)
