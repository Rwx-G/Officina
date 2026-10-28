# Story 2.9: Relay view and journal retention

**Status:** review

## Story

As the author who wants to know what was done on my behalf,
I want a Relay view with the catalogue, the pending change, issues and recent activity, next to the stop toggle,
so that I can audit every run and refusal from the window, and the journal keeps 90 days of history.

## Acceptance Criteria

1. The header gains a Board/Relay toggle; the status chips and the stop toggle of story 2.7 stay visible in both views (AD-35 layout, FR-10). Proof: Playwright `app/e2e/relay-view.spec.ts > the toggle switches between Board and Relay`, `> chips and stop toggle stay visible in the Relay view`.
2. The Relay view shows, from `RelayView`: the stop toggle; the pending change with its diff per action and Approve; issues with their lines; the catalogue table of the approved snapshot (actions, class, projects, D-24); recent activity (time, project, action, params, outcome, duration, "abandoned" when so); the unreadable journal line count and the gate errors of the last 24 h (AD-35, AD-28, FR-10). Proof: `relay-view.spec.ts > every section renders from the relay fixture`, `> recent activity shows outcomes and abandoned runs`, `> journal and gate diagnostics are surfaced`.
3. All text in the Relay view is rendered as text (AD-35). Proof: `relay-view.spec.ts > journal text renders as text, never as HTML` and the `svelte/no-at-html-tags` lint.
4. Journal retention: the window deletes journal files older than 90 days at startup and hourly; `run` and the gate never delete (AD-32, PRD amendment 10). Proof: host tests `crates/officina-host/tests/relay_journal.rs::{retention_keeps_90_days, retention_leaves_the_lock_and_other_files}` and `crates/officina/tests/run.rs::run_never_deletes_journal_files`.
5. Fixtures (D-25): `scripts/e2e-fixtures.mjs` adds a `relay-activity` scenario from the real CLI, approved through the `approve_fixture` example: real runs and refusals through `officina run` with the fake tool, a gate deny through `officina hook`, and an abandoned run from a seeded journal (a `run_started` dated past its timeout plus 60 s with no `run_finished`). Proof: CI fixture step and the Playwright cases.

## Tasks

- [x] Journal retention in the collector's retention pass (AC 4)
- [x] Header Board/Relay toggle; `RelayView` component with its sections (AC 1-3)
- [x] `relay-activity` scenario; `app/e2e/relay-view.spec.ts` (AC 1-3, 5)
- [x] Host and CLI tests (AC 4)
- [x] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (Relay view, retention) (D-34)

## Dev Notes

- Requirements: FR-10, FR-13 retention (amendment 10). Decisions: AD-28 (gate errors shown), AD-32, AD-35. Epic decisions: D-24, D-25, D-26 (stop moved to story 2.7).
- The view toggle is local window state, not part of `RelayView`; keep it out of the Rust contract.
- `RelayView.recent` holds the last 50 entries (AD-35); the view does not page further. Older history stays in the journal files for 90 days.
- Retention runs in the same hourly pass as the event buffer retention (AD-9) and the approval file cleanup (story 2.8).
- Stop (writer, refusal, header toggle, banner) shipped in story 2.7; this story only places the same toggle in the Relay view.

## Testing

- Host integration: `crates/officina-host/tests/relay_journal.rs`.
- CLI integration: `crates/officina/tests/run.rs`.
- Playwright: `app/e2e/relay-view.spec.ts`.

## Dev Agent Record

### Debug Log

- **Decisions where the documents are silent**, each the reading that fails closed or matches the architecture:
  - Retention keeps every day file that can hold a line of the last 90 days: a file dated before the UTC day 90 days back goes (`relay::journal::JOURNAL_RETENTION_DAYS`, `is_expired_journal_file`). The event buffer's rule ("older than yesterday") and this one share `dates::is_dated_file_older_than`, so only `YYYY-MM-DD.jsonl` names are ever eligible; the lock file, other names and a folder named like a day file stay. The pass runs where the approval and output pruning runs, at startup and hourly (`watch::RETENTION_EVERY`).
  - The Board/Relay switch appears only while the relay is in use (story 2.7's rule, `relayInUse`), so a v0.1 workstation, the board e2e cases and the README assets see nothing new; when the relay leaves use the window shows the board. The switch is two `aria-pressed` buttons in a `group`, local state in `App.svelte` (`WindowView`), never part of `RelayView`; story 4.4 moves it into its view store.
  - The Relay view replaces the board area; the board's own banners (stale, floor, diagnostics, relay banners) stay with the board. The header's chips and stop toggle stay in both views, and the view has its own stop section (state, floor, the same `setStopped`), as the Dev Notes place "the same toggle in the Relay view".
  - The pending change is one component (`PendingCatalogue.svelte`) shared by the board's banner and the view, so the diff and the pointer-only Approve are written once; the view lists the issues in its own section with their lines, always, not only while a change is pending. `ActionRuns.svelte` renders what an action starts (runner, program, one chip per argument) for the banner and the catalogue table; story 2.11 extends it for HTTP.
  - The catalogue table is the approved snapshot (D-24): one row per action (class and its check, the projects that list it, what it starts, parameters, limits), then the projects and the guarded programs. With nothing approved it says `officina run` refuses every action.
  - Recent activity: one row per `RelayView.recent` entry, as Rust pairs and dates them; `lib/relay-text.ts` maps each Rust vocabulary (refusal reasons, deny kinds, gate faults, run outcomes and states) through an exhaustive `Record`, so a new variant fails `pnpm check`. A gate line's action column reads `gate`, an approval's `approval`, the stop and resume lines `relay`. A refusal's detail, a deny's command excerpt and a gate error's detail show under the outcome as text. Durations under a minute show in ms or s (`runDuration`).
  - The `relay-activity` fixture's gate error is real: the scenario writes a floor phase 1 refuses (an unknown top-level key), sends a `Bash` call through `officina hook` (the gate journals `floor_invalid` and falls back to the snapshot), then restores the floor. Only the abandoned run and the unreadable line are seeded. The scenario checks it produced every kind the spec relies on.
- **Adversarial pass.** Tried: markup in a refused value, a gate excerpt, an issue's name and a description (all text; `img`, `b`, `i` absent, no script ran); a relay that stops while the view is open (chip, header toggle and view toggle follow); a relay leaving use while the view is shown (falls back to the board); retention against the held lock file, a `.bak` and a `.json` copy of an old day, an invalid date, a folder named like an old day, a missing journal folder, a second pass; `officina run` (success, refusal, `--list`) beside a day file twice past the retention. No miss found beyond the AC tests. Keys 1 to 9 still focus Needs you cards from the Relay view; story 4.6 scopes keys to their views.
- **Not run here:** the window itself (Tauri); the view is proved through the e2e build's mock on real CLI output.

### Completion Notes

- AC 1: `app/e2e/relay-view.spec.ts > the toggle switches between Board and Relay`, `> chips and stop toggle stay visible in the Relay view`, plus `> a workstation without the relay has no view switch`.
- AC 2: `relay-view.spec.ts > every section renders from the relay fixture`, `> recent activity shows outcomes and abandoned runs`, `> journal and gate diagnostics are surfaced`, plus `> the Relay view's stop toggle calls set_relay_stopped`; Vitest `app/src/lib/relay-text.test.ts > activityRow > *`, `app/src/lib/time.test.ts > runDuration`.
- AC 3: `relay-view.spec.ts > journal text renders as text, never as HTML`; Vitest `relay-text.test.ts > RecentActivity > renders session text as text, never as markup`; the `svelte/no-at-html-tags` lint.
- AC 4: `crates/officina-host/tests/relay_journal.rs::{retention_keeps_90_days, retention_leaves_the_lock_and_other_files}`, `crates/officina/tests/run.rs::run_never_deletes_journal_files`; doctests of `dates::is_dated_file_older_than` and `relay::journal::is_expired_journal_file`.
- AC 5: `app/scripts/e2e-fixtures.mjs` scenario `relay-activity` (approved through `approve_fixture`; runs and refusals through `officina run` with the fake tool; a deny and a gate error through `officina hook`; a seeded abandoned run and unreadable line; a pending change with issues).
- Gates at the end of the story: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --features officina-core/ts -- -D warnings` and the Linux job's crate set; `cargo test --workspace --features officina-core/ts` 476 passed, 0 failed, 6 ignored; `pnpm check`, `lint`, `test` (31 passed); fixtures rebuilt from the real CLI and `pnpm e2e` 43 passed. No exported Rust type changed (no TypeScript regeneration); the hook path did not change (no perf run needed).

## File List

- `crates/officina-core/src/dates.rs`, `crates/officina-core/src/relay/journal.rs`
- `crates/officina-host/src/relay/journal.rs`, `crates/officina-host/src/watch.rs`
- `crates/officina-host/tests/relay_journal.rs`, `crates/officina/tests/run.rs`
- `app/src/App.svelte`, `app/src/components/{Header,RelayBanners,PendingCatalogue,ActionRuns,CatalogueTable,RecentActivity,RelayView}.svelte`
- `app/src/lib/{relay.svelte.ts,relay-text.ts,relay-text.test.ts,time.ts,time.test.ts}`
- `app/e2e/{fixtures.ts,relay-view.spec.ts}`, `app/scripts/e2e-fixtures.mjs`
- `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-27 created as "Relay view and stop"
- 2026-10-27 reworked after review: stop moved to story 2.7 (D-26), file renamed from `story-2.9-relay-view-and-stop.md`; fixtures approved through the example and abandoned run seeded (D-25)
- 2026-10-28 implemented: journal retention in the window's hourly pass, the Board/Relay switch and the Relay view (stop, pending change, issues, catalogue, recent activity, diagnostics), the `relay-activity` scenario from the real CLI; status review
