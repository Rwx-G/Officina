# Story 2.11: HTTP read actions and credentials

**Status:** Approved

## Story

As the author whose routine checks include a few HTTP reads (a health endpoint, a monitoring API),
I want catalogue actions that run a GET or HEAD on a templated URL, with a credential from the Windows Credential Manager sent only to the hosts it is bound to,
so that sessions can read those endpoints through the relay, journaled, without ever seeing or carrying the secret.

## Acceptance Criteria

1. Schema (FR-2, FR-5, FR-12, AD-36): an `[[action]]` with `kind = "http"` declares `method` (`GET` or `HEAD`), `url` and optionally `credential`, and no `runner`, `distro`, `program`, `env` or `args`; a top-level `[[credential]]` declares `name`, `hosts`, `header` and `scheme`, validated per D-40. Phase 2 validates them; the "not in this build" issue of AD-23 is removed. Proof: core unit tests `crates/officina-core/src/relay/catalogue.rs::tests::{http_action_parses, http_action_with_program_fields_is_an_issue, credential_entry_parses, unknown_credential_is_an_issue, credential_host_not_bound_is_an_issue, credential_over_plain_http_is_an_issue, forbidden_or_malformed_header_is_an_issue, malformed_scheme_is_an_issue}` and `every_issue_kind_has_a_case` (story 2.1) with the new variants.
2. URL templates (AD-36, FR-12), core `relay::url`: scheme `https`, or `http` only without a credential; scheme, host and port literal; no userinfo (D-40); a placeholder fills exactly one whole path segment or one whole query value, never a query key; values pass the AD-30 rules, then are percent-encoded outside `A-Za-z0-9-._~`, so a value can neither add a segment nor a parameter. Proof: vector set `tests/vectors/url.json` (template, params, expected URL or issue or refusal) replayed by `crates/officina-core/tests/url_vectors.rs::every_case_matches`, and `url_vectors.rs::every_rule_has_a_case`, which iterates the URL issue-kind enum and every byte class of the encoder (unreserved, reserved, non-ASCII) and fails when one has no case.
3. The canonical catalogue covers credentials (name, sorted `hosts`, `header`, `scheme`) and the HTTP fields of actions; `format` is bumped, which invalidates existing approvals on purpose (D-39); the diff reports credentials added, removed and changed per field. Proof: `crates/officina-core/tests/catalogue_hash.rs::{golden_fixture_hash_is_pinned, every_canonical_field_changes_the_hash}` on a golden floor that now declares a credential and an HTTP action, and `catalogue.rs::tests::diff_reports_credential_changes`.
4. Credential store seam: a host trait `CredentialStore` with a Windows implementation that reads `officina:<name>` (`CRED_TYPE_GENERIC`) with `CredReadW` and frees it with `CredFree`, unsafe confined to `platform/windows.rs` with `// SAFETY:` comments; non-Windows builds report "unsupported". The blob is decoded as UTF-16LE (what `cmdkey /generic` stores) by a pure core function; an odd length, a lone surrogate or a CR or LF in the secret is refused. The host `Secret` type prints `<redacted>` under `Debug` and implements no `Serialize`. Proof: `crates/officina-core/src/relay/credential.rs::tests::{decodes_a_cmdkey_blob, odd_length_is_refused, lone_surrogate_is_refused, line_breaks_are_refused}` and `crates/officina-host/src/relay/credential.rs::tests::secret_debug_is_redacted`. Automated tests use a fake store only; the real `CredReadW` path is AC 18.
5. Request execution (AD-36) in host `relay::http` with `ureq` over rustls and the platform verifier: no redirect followed, timeout `min(timeout_s, 60)`, no proxy (environment proxy variables ignored), body read up to `max_output_kib` then the connection dropped, `HEAD` reads no body. Proof: CLI tests `crates/officina/tests/http.rs::{get_prints_status_and_body, head_sends_head_and_reads_no_body, redirect_is_not_followed, proxy_environment_is_ignored, body_is_capped_and_the_connection_dropped, timeout_exits_124, no_listener_exits_127}`, each against the local stub server described in Dev Notes.
6. A credential reaches only its bound hosts (D-37): core plan building attaches `<header>: <scheme> <secret>` only when the URL host is in the credential's `hosts`; the secret is read after every AD-29 check, just before the request, and before a `human` approval is consumed (D-29); a missing or unreadable credential refuses with exit 126 before any connection and leaves the approval unconsumed. Proof: `crates/officina-core/src/relay/http.rs::tests::{header_only_for_a_bound_host, host_match_is_exact_and_case_insensitive}` and host tests `crates/officina-host/tests/relay_http.rs::{credential_header_reaches_the_bound_host, missing_credential_refuses_before_any_connection, unreadable_credential_refuses_before_any_connection, missing_credential_does_not_burn_the_approval}` with the fake store and the stub server.
7. The new refusal codes join the exhaustive scenario builder of story 2.5 AC 3; the codes that need a credential store are proved at host level and the builder names the host test for them, so the `match` stays exhaustive without the CLI touching the real store. Proof: `crates/officina/tests/run.rs::every_refusal_code_exits_126_with_one_journal_line` and the host tests of AC 6.
8. Exit codes per D-35: 0 for a 2xx status, 1 for any other status with one stderr line `officina: http <status>`, 124 on timeout, 127 when no response arrives; never 2. Proof: `http.rs::non_2xx_status_exits_1` plus the tests of AC 5.
9. Journal (FR-13, AD-32, D-36): `run_started` carries `method`, the filled `url` and the `credential` name instead of `argv`; `run_finished` carries outcome `http` with `status`, `body_bytes`, `body_sha256` and `duration_ms`, or `timeout`, or `connect_failed`. Proof: `http.rs::http_run_journals_method_url_and_status` and core unit test `crates/officina-core/src/relay/journal.rs::tests::http_lines_round_trip`.
10. The secret never leaves the request header: not in stdout, stderr, the journal, approval request files or `RelayView`. Proof: `relay_http.rs::secret_never_leaves_the_header`, which scans every file under the temp home and every captured output for the fake store's marker secret.
11. A `human` HTTP action's approval card shows method and URL rebuilt by core from the approved catalogue and the request's parameters, never read from the request file; a forged request is invalid (D-27, story 2.8 AC 3). Proof: `crates/officina/tests/approvals.rs::{http_request_shows_rebuilt_method_and_url, forged_http_request_is_not_approvable}`.
12. `RelayView` shows HTTP actions with method and URL template, and a `credentials` list (name, hosts, actions using it, `present`). `officina relay --json` never reads the store and reports `present: null`; the window's collector fills it through the store seam on relay recomputes for a floor or relay change and at most once a minute otherwise (D-41). Proof: core `crates/officina-core/src/relay/view.rs::tests::credentials_list_their_actions`, CLI `crates/officina/tests/relay.rs::relay_json_never_reads_credentials`, CLI-crate collector tests `crates/officina/tests/watch.rs::{relay_view_carries_credential_presence, credential_presence_is_polled_at_most_once_a_minute}` (fake store counting reads), `typescript_bindings.rs::committed_bindings_match_the_rust_types`, and Playwright `app/e2e/relay-view.spec.ts > credential status shows present, missing and unchecked`, `> credential and URL text renders as text`.
13. Doctor's Credential Manager check (FR-14, D-31, D-38): for each declared credential, present or missing in the store; missing while an approved action uses it is a failure (exit 1), otherwise a warning; a missing credential prints `cmdkey /generic:officina:<name> /user:<user> /pass` with placeholders, so `cmdkey` prompts and the secret never lands in shell history. Doctor never prints a secret. Proof: host unit tests `crates/officina-host/src/relay/doctor.rs::tests::{present_credential_is_ok, missing_used_credential_fails_with_the_cmdkey_line, unused_missing_credential_warns, doctor_output_holds_no_secret}` with the fake store.
14. Dependencies (addendum section 2): `ureq` with rustls and the platform verifier and `windows-sys` feature `Win32_Security_Credentials` in `officina-host`; no `webpki-roots`, `reqwest`, `tokio` or `url` in the graph; `cargo deny check` clean. Proof: CI `audit` job and a CI `windows` step "no bundled roots" that fails when `cargo tree -p officina -i webpki-roots` finds the crate and treats its "nothing to print" exit as success.
15. Linking HTTP and TLS into `officina.exe` keeps the hook and gate budgets: AD-17 hook latency and D-19 gate overhead still pass. Proof: `#[ignore]` tests `crates/officina/tests/perf.rs::{hook_latency_p95, gate_overhead_over_empty_process}` in the CI release step; `dumpbin /imports` of the release `officina.exe` recorded in Completion Notes.
16. Zero write: an HTTP run joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
17. Documentation (D-34): the README relay section gains HTTP actions and credentials (setup through doctor's `cmdkey` line, the host binding), and `CHANGELOG.md` `[Unreleased]` gains the entries. Proof: `crates/officina/tests/readme.rs` (story 2.10) stays green; prose reviewed by hand.
18. Manual, recorded in `docs/research-notes.md` with Windows, `ureq` and endpoint details redacted: store a credential with doctor's `cmdkey` line (it prompts for the secret), then run an HTTP action with that credential against a real internal `https` endpoint whose CA is trusted only through the Windows store; the request succeeds, the platform verifier accepts the enterprise CA, and the journal shows the run with no secret. Proof: manual, per AD-37.

## Tasks

- [ ] Verify the exact `ureq` version, feature names and crypto provider against the crate's documentation before writing `Cargo.toml` (see Dev Notes) (AC 5, 14)
- [ ] Core: HTTP action and `[[credential]]` schema and issues, `relay::url` (template parse, encode), `relay::http` (request plan, header binding), `relay::credential` (blob decode), journal HTTP fields, canonical and diff, approval rebuild for HTTP, view (AC 1-4, 6, 9, 11, 12)
- [ ] `tests/vectors/url.json` and `crates/officina-core/tests/url_vectors.rs`; re-pin the golden hash (AC 2, 3)
- [ ] Host: `CredentialStore` trait, Windows `CredReadW` implementation in `platform/windows.rs`, `Secret`; `relay::http` executor on `ureq`; `run` dispatch for `kind = "http"` with the credential read before the approval is consumed (AC 4-10)
- [ ] `RelayView.credentials`, collector presence check with its rate limit, window rendering, fixture scenario through the `approve_fixture` example (D-25); regenerate TypeScript (AC 12)
- [ ] Doctor credential check and `cmdkey` line (AC 13)
- [ ] Dependencies, `cargo deny`, CI "no bundled roots" step (AC 14)
- [ ] Stub server test support, `crates/officina/tests/http.rs`, `crates/officina-host/tests/relay_http.rs`; extend refusal, zero-write and perf runs (AC 5-10, 15, 16)
- [ ] README relay section and `CHANGELOG.md` `[Unreleased]` (AC 17)
- [ ] Manual check (AC 18)

## Dev Notes

- Requirements: FR-2 (`kind = "http"`), FR-5, FR-12, FR-13, FR-14 (credential check). Decisions: AD-36, with AD-23 (issues), AD-24 (canonical form), AD-29 (order of checks), AD-32 (journal), AD-33 (approvals), AD-35 (view). Epic decisions: P-7, D-25, D-27, D-29, D-31, D-34 to D-41.
- **Why this story exists inside Epic 2 (P-7).** The owner's rule is that what a PRD defers is done at the end of that PRD, before the next one. That rule supersedes amendment 1's "FR-5 and FR-12 follow as v0.2.x" and AD-36's "build it after a week of use, or drop it". AD-36's design is unchanged and this story does not edit the addendum.
- **`ureq` features must be verified against the crate documentation (docs.rs, the crate's `Cargo.toml`, a web search if needed) before coding.** The intent is `default-features = false` plus rustls and the platform verifier (believed to be named `rustls` and `platform-verifier` in ureq 3; unverified). Check on the chosen version: whether the rustls feature pulls `webpki-roots` (excluded by addendum section 2; if it cannot be avoided, stop and raise it to the owner); whether proxy settings are read from the environment by default (AD-36 says no proxy: disable it explicitly); the default redirect limit (set it to zero); and which crypto provider rustls uses, since `aws-lc-rs` may need CMake or NASM on the Windows runner and `ring` has its own licence terms. A licence that `deny.toml` does not allow goes to the owner before any exception is added.
- Stub server: a test helper built on `std::net::TcpListener` bound to `127.0.0.1:0`, a thread that answers scripted responses (status, headers, body size, delay, hang) and records each request line and headers. One helper file, included by `#[path]` from the host and CLI test crates. No external network in any automated test.
- Credentials and `https` cannot be exercised on the wire without a TLS server, which would need a new dependency. So: header binding is proved in core; the host executor test sends a hand-built plan (credential header over `http` to the stub, bypassing catalogue validation on purpose) to prove the header reaches the wire and nothing else carries the secret; TLS with the Windows store and a real credential is AC 18.
- The CLI never runs a credential lookup in automated tests: CLI tests use credential-less HTTP actions, and `relay --json` reports `present: null`. Only `run` (for an action with a credential), doctor and the window read the store.
- The secret lives in memory only between the store read and the request; no `zeroize` crate (not approved). Never log the request headers.
- Gate: nothing changes. HTTP actions go through `officina run` like the others, and the gate's deny reason lists them like the others.

## Testing

- Core unit: `crates/officina-core/src/relay/{catalogue,url,http,credential,journal,view}.rs`.
- Core integration: `crates/officina-core/tests/{url_vectors,catalogue_hash,typescript_bindings}.rs`.
- Host unit and integration: `crates/officina-host/src/relay/{credential,doctor}.rs`, `crates/officina-host/tests/relay_http.rs` (fake store, stub server).
- CLI integration: `crates/officina/tests/{http,run,relay,approvals,watch,quota_doctor,readme}.rs`; ignored release tests in `perf.rs`.
- Playwright: `app/e2e/relay-view.spec.ts`.
- Manual: AC 18, research notes.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: P-7 (owner's rule supersedes amendment 1), D-27 (method and URL rebuilt for approvals), D-29 (credential read before consume), D-40 (header, scheme, userinfo), D-41 (presence polling), crypto provider and `cargo tree` exit checks
