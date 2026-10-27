# Story 2.9: Relay view and journal retention

**Status:** Approved

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

- [ ] Journal retention in the collector's retention pass (AC 4)
- [ ] Header Board/Relay toggle; `RelayView` component with its sections (AC 1-3)
- [ ] `relay-activity` scenario; `app/e2e/relay-view.spec.ts` (AC 1-3, 5)
- [ ] Host and CLI tests (AC 4)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (Relay view, retention) (D-34)

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

### Completion Notes

## File List

## Change Log

- 2026-10-27 created as "Relay view and stop"
- 2026-10-27 reworked after review: stop moved to story 2.7 (D-26), file renamed from `story-2.9-relay-view-and-stop.md`; fixtures approved through the example and abandoned run seeded (D-25)
