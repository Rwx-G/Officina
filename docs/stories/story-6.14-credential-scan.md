# Story 6.14: The credential scan and the credential end to end

**Status:** Review

## Story

As the author who lends a worker a model token,
I want every package and every text a run hands back searched for that token, raw and encoded, before I see it, and the token itself proven absent from everything Officina writes or shows, live runs included,
so that a careless leak is quarantined and replaced before it reaches my repository or my screen, and Officina itself never becomes the leak.

## Acceptance Criteria

1. **Scope** (AD-112, FR-10 step 3, D-237). At collection, after story 6.13's step 7, using story 6.7's `scan` with the run's needles (or needles built from a fresh credential read when the window restarted). The pass takes its hits as byte ranges per record from a `ScanSource`: host needles for the token kinds; for a `login` run, story 6.26's `Scanner` container, fed the same records (D-609), so placeholders, quarantine and locations are this story's code for both. The records: every blob added or changed between base and tip, every commit message and path, every commit's author and committer names and emails (review F15), the report tail and every stream text kept, the gateway's refused and allowed host names, the manifest, and in Slice B the memory proposals (story 6.21). The texts are scanned for every outcome, `no_change` and `failed` included. Then the run's needles are dropped (D-230). Proof: CLI `crates/officina/tests/workshop_collect.rs::{a_token_in_a_blob_quarantines, a_token_in_a_commit_message_quarantines, a_token_in_a_path_quarantines, a_token_in_an_author_email_quarantines, a_wrapped_base64_token_in_a_blob_quarantines, a_token_in_the_report_tail_is_found, a_token_in_a_refused_host_is_found, a_token_in_a_no_change_run_is_replaced, needles_are_dropped_after_collection, ranges_from_any_source_are_applied_alike}` (each with the raw, base64 and hex forms derived from the seeded value; the last feeds the pass a fixed range list as a second source).
2. **When found** (AD-112, FR-10, D-212). Quarantine `credential_found`; its locations recorded (commit and path, "author", "report", "commit message", "gateway line") in `texts_json` and their count in `facts_json`, never the value; the value replaced by `[credential]` in every text Officina stores; `out/` and `verify.git` deleted at once; the card and the review panel tell the author to revoke the token (the question 10 entry's place, README). For `no_change` and `failed` runs, the texts are replaced the same way and the run's card carries the revoke advice, with no package to quarantine. `review_package` counts locations. Proof: CLI `workshop_collect.rs::{found_deletes_out_and_verify_at_once, stored_texts_hold_the_placeholder_not_the_value, the_revoke_advice_outlives_the_texts}`.
3. **The credential end to end** (AD-125 credential row, PRD section 7 Credential, D-230, review F2, F14). After a full run through the fake engine with a seeded credential (launch, follow, collection, quarantine, review view, journal, a window restart with reconciliation), whose fixture stream carries the value in a forged refused host, in a wildcard-allowed host and in a mid-run report tail, and whose fake `inspect` state holds the value in `Config.Env`: the value, raw and in every needle form, appears in no file under the test home, no journal line, no registry row, no log, no IPC payload (every serialized `WorkshopView`, `Board`, `ReviewView` and command answer, taken while the run is live and after) and no fake engine log line, whose lines show the credential's name on the worker's `create` only (D-223). Proof: host integration `crates/officina/tests/credential.rs::{the_value_is_nowhere_after_a_full_run, the_value_is_nowhere_after_a_quarantine, the_value_is_nowhere_while_the_run_is_live, the_value_is_nowhere_after_a_restart}`.
4. **Documentation** (D-226, D-237). README: what the scan catches (careless leaks only; a steered worker can send the value through `api.anthropic.com`, PRD section 1); the revoke advice beside the quarantine; that a run kept over a credential rotation is scanned with the new value only (review F15). Proof: review.

## Tasks

- [x] The collection scan pass: the package's records in `crates/officina-host/src/workshop/verify.rs` (`package_records`), the pass in `crates/officina-host/src/workshop/collect.rs` (`TextRecords`, `scan_pass`), `ScanSource`, `Place`, `apply` and `locations` in core's `scan`; scope with author and committer fields; placeholders; deletion; needles dropped (AC 1, 2)
- [x] `crates/officina/tests/credential.rs` with the hostile fixture stream and `inspect` state (AC 3); README (AC 4)

## Dev Notes

- Requirements: FR-5, FR-10 step 3, PRD section 7 Credential. Decisions: AD-105, AD-112, AD-125. Facts: the question 10 entry (where to revoke). Risks: R49, R58. Epic decisions: P-50, D-200, D-212, D-223, D-226, D-230, D-237.
- Seams (D-200): story 6.13's verification pass; the review view; story 6.11's `scan` and the run's needles; story 6.12's replacement at the source.
- Waits for: story 6.13 merged.
- The scan is honest about its reach: it catches what a worker wrote carelessly into what it hands back; a worker steered to leak sends anything through `api.anthropic.com` with any account's key (R58), which no scan sees.
- AC 3's live and restart cases are what make D-230 executable: without a forged host or a mid-run tail carrying the value, the end-to-end search would pass while the journal held the value (review F2).
- The extra-file route (a file in `out/` carrying the value, never scanned) is closed by story 6.13's allow-list (D-229): such a file is `bundle_invalid` and never written.

## Testing

- CI, no Docker: CLI with the fake engine and real git `crates/officina/tests/{workshop_collect,credential}.rs`.

## Dev Agent Record

### Debug Log

- The lead's guess for the perf false failure (perf tests loading each other in parallel) did not hold: every perf test holds `SERIAL`. The hook's own cost compared two blocks timed one after the other; it is now timed in pairs (D-1177).

### Completion Notes

- AC 1: `workshop_collect.rs::{a_token_in_a_blob_quarantines, a_token_in_a_commit_message_quarantines, a_token_in_a_path_quarantines, a_token_in_an_author_email_quarantines, a_wrapped_base64_token_in_a_blob_quarantines, a_token_in_the_report_tail_is_found, a_token_in_a_refused_host_is_found, a_token_in_a_no_change_run_is_replaced, needles_are_dropped_after_collection, ranges_from_any_source_are_applied_alike}`, each over the raw, base64 and hex forms derived from the seeded value (a refused host skips the base64 form, whose `+`, `/` and `=` a host name cannot hold); `each_quarantine_cause_from_a_crafted_package` covers `credential_found`; core `scan::tests::{apply_cuts_and_merges_foreign_ranges, locations_never_carry_the_value}`.
- AC 2: `workshop_collect.rs::{found_deletes_out_and_verify_at_once, stored_texts_hold_the_placeholder_not_the_value, the_revoke_advice_outlives_the_texts}`; `review_package` carries the count. A blob's location is its path, not its commit and path (D-1173). The card's and the review panel's revoke sentence are 6.18 and 6.19's surfaces (D-1176); the view carries the count and the places.
- AC 3: `credential.rs::{the_value_is_nowhere_after_a_full_run, the_value_is_nowhere_after_a_quarantine, the_value_is_nowhere_while_the_run_is_live, the_value_is_nowhere_after_a_restart}`: every file under the test home (registry, WAL, journal, logs, run folders), the run's row, the review view, the card and the launch answer, and the fake engine's call log, searched with core's `find` over every needle form; the credential's name on the worker's `create` only. The Workshop view does not exist yet (6.18), and the `Board` is not read by this test.
- AC 4: README, the workshop's collection section: what the scan catches, careless leaks only, the revoke advice with E98's unknown place, the rotation limit, a `login` run until its scanner.
- A `login` run has no scan source until 6.26 (D-1175).

## File List

- `crates/officina-core/src/workshop/scan.rs`
- `crates/officina-core/src/workshop/package.rs`
- `crates/officina-core/src/workshop/review.rs`
- `crates/officina-core/src/untrusted.rs`
- `crates/officina-core/tests/bundle_vectors.rs`
- `crates/officina-host/src/workshop/verify.rs`
- `crates/officina-host/src/workshop/collect.rs`
- `crates/officina-host/src/workshop/follow.rs`
- `crates/officina-host/src/workshop/review.rs`
- `crates/officina/tests/workshop_collect.rs`
- `crates/officina/tests/credential.rs`
- `app/src/lib/generated/QuarantineCause.ts`, `app/src/lib/generated/ReviewFacts.ts`, `app/src/e2e-mock.ts`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-6-decisions.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `scan` itself moved to 6.11, where launch and following need it first; the end-to-end search covers a forged refused host, a wildcard-allowed host, a mid-run tail and a restart (F2) and a seeded `Config.Env` (F14); author and committer fields, wrapped base64, the rotation limit (F15); the extra-file route closed by 6.13's allow-list (F1)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
- 2026-10-29 the owner's answers: the pass applies byte ranges from a `ScanSource`, host needles or story 6.26's `Scanner` for a `login` run; `scan` is story 6.7's (D-609)
- 2026-11-03 implemented (D-1173 to D-1177); status review
