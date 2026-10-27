# Story 1.6: Board window

**Status:** Approved

## Story

As the author,
I want an always-open window that shows the board in four columns with the final palette and type,
so that I see which sessions need me, which are working and which are finished in under two seconds.

## Acceptance Criteria

1. `officina-app` runs the collector in-process (AD-2), exposes the commands `get_board` and `refresh` and emits `board://updated` (AD-19); `focus_card` and `report_wrong_window` arrive with story 1.7. Proof: Playwright e2e `app/e2e/board.spec.ts > first paint uses get_board, then updates on board://updated` through the mocked IPC; Rust unit test `app/src-tauri/src/commands.rs::tests::get_board_returns_cached_board`.
2. The capability file grants only Officina's commands, the event permission and (story 1.12) the always-on-top permission; no permission from the shell, fs, http or opener plugin families exists (AD-19). Proof: Rust test `app/src-tauri/tests/capabilities.rs::no_forbidden_plugin_permission` parsing `capabilities/default.json` against a list of forbidden prefixes.
3. `lib/ipc.ts` is the only module importing `@tauri-apps/api`, apart from `src/e2e-mock.ts` (AD-20). Proof: ESLint `no-restricted-imports` with an override for those two files; CI step `pnpm lint`.
4. The board renders the four column groups of the glossary, with To resume and Done in the fourth group, and every card field of FR-7 that the board carries. Proof: Playwright `board.spec.ts > renders every column from a fixture with one card per column` and Vitest `src/lib/components/Card.test.ts > renders name, project chip, kind, waiting_for, excerpts, times`.
5. Text from events is rendered as text, never HTML. Proof: Vitest `Card.test.ts > excerpt containing an img onerror tag renders as text and creates no element` (AD-17), and the existing ESLint rule `svelte/no-at-html-tags`.
6. The header shows the "N waiting" pill only when N > 0 and the stale banner when the board is stale with its error (FR-8 part, FR-1). Proof: Vitest `Header.test.ts > pill hidden at 0 and shown with count`, Playwright `board.spec.ts > stale fixture shows the banner with the error`.
7. Relative times are computed by the frontend on a 30 s tick from absolute timestamps (AD-5). Proof: Vitest `src/lib/time.test.ts` with fake timers, and `Card.test.ts > relative time advances on tick without a new board`.
8. Styling uses the framing section 8 tokens as CSS custom properties in `app.css`, amber only for waiting, Plus Jakarta Sans embedded through `@fontsource` (AD-19, AD-20). Proof: Playwright `board.spec.ts > only waiting elements use the attention colour` (computed styles of every element compared against the token value read from `:root`) and `board.spec.ts > page makes no network request outside the app origin`.
9. Project colours go through `Record<ProjectColor, string>` so a new Rust variant breaks `pnpm check` (AD-13). Proof: CI step `pnpm check`.
10. The e2e harness follows AD-17: `src/e2e-mock.ts` loaded only in `e2e` mode, boards injected by `page.addInitScript`, pushes through `window.__officinaPushBoard`, fixtures produced by `pnpm e2e:fixtures` from the real CLI against the fake claude into git-ignored `app/e2e/.fixtures/`. Proof: Playwright suite green on CI; script `pnpm e2e:fixtures` runs in CI before `pnpm e2e`.
11. The production bundle contains no e2e mock. Proof: CI step after `pnpm build` asserting `dist/` contains neither `__officinaPushBoard` nor `mockIPC` (script `app/scripts/check-no-e2e-mock.mjs`).
12. Manual run on the workstation: the window shows the live board, no console window flashes during 2 minutes of polling (R2), and WebView2 rendering matches the Playwright screenshots by eye (AD-17). Proof: manual checklist below, results in Completion Notes.

## Tasks

- [ ] `app/src-tauri`: start the collector thread, commands `get_board`, `refresh`, event emission from the collector callback, `Visibility` messages on minimize, hide and restore (AC 1)
- [ ] Declare commands in `build.rs` through the app manifest; update `capabilities/default.json`; capability test (AC 1, 2)
- [ ] `lib/ipc.ts`, `lib/board.svelte.ts`, `lib/time.ts` (AC 1, 3, 7)
- [ ] Components `Header`, `Column`, `Card`, `StaleBanner` (`Gauge` comes with story 1.9) (AC 4, 5, 6)
- [ ] `app.css` tokens from framing section 8; `@fontsource/plus-jakarta-sans` import (AC 8)
- [ ] `Record<ProjectColor, string>` colour map (AC 9)
- [ ] ESLint `no-restricted-imports` override (AC 3)
- [ ] `src/e2e-mock.ts`, `vite.config.ts` e2e mode, `playwright.config.ts` on `vite preview` (AC 10)
- [ ] `pnpm e2e:fixtures` script: builds `officina` and the fake, writes hand-written event files and agent fixtures into a temp home, runs `board --json` per scenario into `app/e2e/.fixtures/`; add `.fixtures/` to `.gitignore` (AC 10)
- [ ] Vitest and Playwright tests listed in the ACs (AC 1, 4-8)
- [ ] `check-no-e2e-mock.mjs` and the CI steps `e2e:fixtures`, `build --mode e2e`, `playwright install chromium`, `e2e`, bundle check, Playwright report upload on failure (AC 10, 11)
- [ ] Manual checklist (AC 12)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Decisions: AD-2, AD-5, AD-13, AD-17 (frontend unit, e2e, fixtures rows), AD-19 (surface; single instance, title count and overlay are story 1.12), AD-20 (structure). Requirements: FR-7, FR-8 (pill and stale banner only; gauges in 1.9, contract in 1.11), FR-14.
- The tokens live in framing section 8, which is in the git-ignored run folder `_bmad-output/planning-artifacts/framing-brief.md`. `app.css` becomes their only committed definition; tests read token values from `:root`, never retype them.
- Fixtures must come from the real CLI output, never be hand-edited JSON boards: a hand-edited board is a transcription of the Rust contract.
- The e2e build must not ship: the mock is imported behind `import.meta.env.MODE === 'e2e'` so Vite drops it from production.
- The window cannot be driven by Playwright (no Tauri WebDriver layer in AD-17); the in-process wiring of the collector and visibility is covered by host tests (story 1.5) plus the manual run.

Manual checklist:
- [ ] Launch `officina-app` with three live sessions; the board matches `officina board --json`.
- [ ] No console flash over 2 minutes (Task Manager or eye).
- [ ] Minimize: safety poll drops to the hidden interval (observe spawn rate in Process Monitor or the fake log if run against the fake).
- [ ] WebView2 rendering vs Playwright screenshots, by eye.

## Testing

- Vitest: `app/src/lib/**/*.test.ts`.
- Playwright: `app/e2e/board.spec.ts`.
- Rust: `app/src-tauri/src/commands.rs` unit, `app/src-tauri/tests/capabilities.rs`.
- CI: `pnpm check`, `pnpm lint`, bundle check.
- Manual: checklist above.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
