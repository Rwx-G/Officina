# Story 3.5: Subjects view and author actions in the window

**Status:** Approved

## Story

As the author picking up work on a Monday,
I want a Subjects view that lists my active and to-resume subjects with their sessions' live state and last excerpts, lets me create, rename, move, merge, archive and reopen in one gesture, and keeps the board's cards labelled with their subject,
so that I find where I left each subject without opening a single session.

## Acceptance Criteria

1. Author requests (AD-38, D-44, D-92): `CollectorMsg` loses `Copy` and gains `Author(AuthorRequest)`, carrying a reply `Sender` and its deadline; the collector thread answers on its own connection, after a running `claude agents` if one runs, and drops a request whose deadline passed without executing it; each Tauri command waits in `spawn_blocking` for at most 10 s and reports a timeout as an error. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{author_request_is_answered_by_the_collector_thread, author_request_waits_behind_a_running_refresh, expired_author_request_is_dropped_unexecuted}`; app-crate unit test `app/src-tauri/src/main.rs::tests::author_reply_times_out` (the waiting helper takes its deadline as an argument, the test passes 50 ms).
2. Emission (AD-5, AD-54, D-44, D-92): `watch::spawn`'s callbacks become one `Emit` trait (`board`, `registry`, and `relay` when v0.2 story 2.7 has merged); `RegistryView` is emitted on `registry://updated` only when its bytes change, and after every author action; `get_registry` returns the cached view for the first paint and never goes through the collector. Proof: `watch.rs::{registry_view_is_emitted_only_on_change, author_action_emits_a_new_view}`.
3. Commands (AD-54, AD-19): `get_registry` (cached), and `list_archived`, `create_subject`, `rename_subject`, `merge_subjects`, `file_sessions`, `archive_subject`, `reopen_subject` as thin wrappers sending an `AuthorRequest` (`list_archived` included, so the app holds one connection), declared in `build.rs` and allowed in `capabilities/default.json`. Proof: `main.rs::tests::every_declared_command_is_allowed` (story 2.7, or created here per D-44).
4. Frontend plumbing (AD-20, AD-55): `ipc.ts` gains one function per command plus `onRegistry`; `lib/registry.svelte.ts` holds the view and the requests in flight; `e2e-mock.ts` answers every new command, records calls, and `window.__officinaPushRegistry` simulates the event. Proof: the Playwright cases below run against the mock.
5. Header (AD-55, D-61): a Board/Subjects switch beside v0.2's Relay switch (local window state, not in any Rust contract); digit keys focus cards in the Board view only. Proof: Playwright `app/e2e/subjects.spec.ts > the header switches between Board and Subjects`, `> number keys do nothing in the Subjects view`.
6. Subjects view (FR-6, journey 1, D-96): active and `to_resume` subjects first, each with its sessions and their live state from the board, the last prompt and the end of the last answer; unfiled sessions under their project, with non-interactive runs counted and the most recent listed, collapsed; archived subjects on demand through `list_archived`. Proof: `subjects.spec.ts > active and to-resume subjects come first with their sessions' live state`, `> a subject without a live session shows its last prompt and the end of its last answer`, `> unfiled sessions are listed under their project`, `> non-interactive runs are counted and collapsed`, `> archived subjects appear on demand`.
7. Actions (FR-6, AD-48, D-54 to D-58, D-95, P-10, D-62): create (with the project of the selected sessions, or one chosen among known projects), rename, move (file or unfile one or several sessions), merge, archive with the "routine, could have run alone" box, reopen; each sends its command with the arguments shown; a refusal shows its reason and changes nothing on screen. Proof: `subjects.spec.ts > each action sends its command with the shown arguments`, `> creating from selected sessions takes their project`, `> archive offers the routine box`, `> a refused action shows the reason and changes nothing`; Vitest `app/src/components/components.test.ts > SubjectPicker lists the project's open subjects first and accepts a new name`.
8. Board cards show their subject (FR-6, AD-55): `SessionCard` renders `card.subject` as text. Proof: Playwright `app/e2e/board.spec.ts > cards show their subject`; Vitest `components.test.ts > SessionCard renders its subject as text`.
9. Registry notes (AD-39, AD-42, D-93): a synced registry shows a note naming the folder to exclude; a restored registry names the copy used, for 7 days; an unavailable registry says why and the view stays readable. Proof: `subjects.spec.ts > a synced registry names the folder to exclude`, `> a restored registry names the copy used`, `> an unavailable registry says why`.
10. Session, subject and project text renders as text (AD-55, frontend rule). Proof: `subjects.spec.ts > session and subject text renders as text, never as HTML` (an `<img onerror>` prompt and subject name) and the `svelte/no-at-html-tags` lint.
11. Fixtures from the real code, window closed (D-72, D-82): `crates/officina-host/examples/registry_fixture.rs` writes event files and runs ingestion and author actions at an explicit `now_ms`, then exits; `app/scripts/e2e-fixtures.mjs` builds the scenarios `subjects`, `subjects-synced` and `subjects-restored` with it, then saves real `officina subjects --json` and `officina board --json` output; `pnpm e2e:fixtures` and the CI step "e2e fixtures from the real CLI" build and run the example with `--features window-authority`. Proof: that CI step and the Playwright cases loading the scenarios.
12. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": the Subjects view and its actions. Proof: review.

## Tasks

- [ ] `watch.rs`: `CollectorMsg::Author`, `AuthorRequest` with reply and deadline, `Emit` trait, registry emit, view cache (AC 1, 2)
- [ ] `app/src-tauri/src/main.rs`: commands, waiting helper, `registry://updated`; `build.rs`, `capabilities/default.json` (AC 1, 3)
- [ ] `ipc.ts`, `lib/registry.svelte.ts`, `e2e-mock.ts`, `e2e-globals.d.ts` (AC 4)
- [ ] `Header.svelte` switch; `App.svelte` key rule per view (AC 5)
- [ ] Components `SubjectsView`, `SubjectCard`, `UnfiledSessions`, `ArchivedList`, `SubjectPicker`; `SessionCard` subject chip (AC 6-10)
- [ ] `examples/registry_fixture.rs` (`required-features = ["window-authority"]`); scenarios in `app/scripts/e2e-fixtures.mjs`; `pnpm e2e:fixtures` and the CI fixture step run the example (AC 11)
- [ ] `app/e2e/subjects.spec.ts`, board case, Vitest cases (AC 5-10)
- [ ] Update `CHANGELOG.md` and the README (AC 12)

## Dev Notes

- Requirements: FR-6 as amended (amendment 6), journey 1. Decisions: AD-3, AD-5, AD-19, AD-20, AD-38, AD-48, AD-54, AD-55. Epic decisions: P-10, D-44, D-54 to D-58, D-61, D-62, D-72, D-82, D-92, D-93, D-95, D-96.
- v0.2 seams (D-44): `watch::spawn` takes one `emit(&Board)` today and v0.2 story 2.7 adds a relay callback; this story turns them into one trait. Whichever lands second adds its method. The Relay switch of story 2.9 and the Subjects switch share the header; if 2.9 has not merged, the switch has two positions and 2.9 adds the third.
- AD-38's trade-off: an author action can wait behind a running `claude agents` (about 210 ms, 5 s at worst, E10); the 10 s deadline covers it with room. A request that expired on the window side is dropped by the collector (D-92), so the window never reports an error for an action that later happened.
- The Triage panel heads this view from story 3.7 (D-61); leave its place at the top.
- `ReasonCode` wording is not needed yet; `ProjectColor` mapping already exists for project chips (AD-13).
- Subject names are untrusted text like session text: a session can write event lines, and the author types names.
- Fixtures are read with the window closed (D-82): the example's writer exits before `subjects --json` and `board --json` run, which is the case story 3.1 tests.

## Testing

- CLI-crate collector: `crates/officina/tests/watch.rs`.
- App crate unit: `app/src-tauri/src/main.rs`.
- Vitest: `app/src/components/components.test.ts`.
- Playwright: `app/e2e/{subjects,board}.spec.ts` on fixtures from `registry_fixture`, `subjects --json` and `board --json`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: requests carry their deadline and expired ones are dropped, `get_registry` reads the cache (D-92); `create_subject` takes a project (D-95); non-interactive runs bounded (D-96); restore notice for 7 days (D-93); fixtures read with the window closed and the example run by the fixture script, `pnpm e2e:fixtures` and CI (D-72, D-82)
