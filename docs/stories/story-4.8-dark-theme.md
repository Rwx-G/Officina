# Story 4.8: Dark theme and theme switching

**Status:** Approved

## Story

As the author who works late,
I want the whole window, board and scene alike, to follow the system's dark or light theme, with a manual override that sticks,
so that the workshop is comfortable day and night and never flashes light when it opens.

## Acceptance Criteria

1. Dark values (FR-8, AD-67, D-117): the dark values of every base token and every `--scene-*` and `--figure-*` token move from the prototype's `tokens.mjs` into `app.css`, under `:root[data-theme="dark"]` and under `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) }`; light values stay the framing's. Proof: Vitest `app/src/tokens.test.ts > every role pair meets its contrast target in both themes`, `> the dark values are the same under the theme attribute and under the media query`.
2. The whole window (AD-67, D-189): the new tokens `--color-danger` (light `#8A2B14`, for text and border), `--color-danger-surface` (light `#F6E3DC`), `--color-column` and `--color-attention-wash` (light: the column washes as they are today), with dark values derived by AD-67's rules, replace the colours hard-coded in `App.svelte` (error banner), `Header.svelte` (broken contract) and `BoardColumn.svelte` (column washes), and any colour literal v0.2 and v0.3 components added; the test constant gains D-189's pairs: danger on bg, on surface and on danger-surface 4.5; text on danger-surface 7; text on each wash composited over `--color-bg` 7; text-muted on surface 4.5 (review F15). A colour literal is a hex value, `rgb(`, `rgba(`, `hsl(`, `hsla(`, `color(` or a named colour, except `transparent`, `currentColor`, `inherit` and `none`; named colours are found by assigning each bare word of a declaration to a scratch element's `style.color` and keeping those the parser accepts, so no list of names is typed (review F28). Proof: Vitest `tokens.test.ts > no component hard-codes a colour` (scans every `.svelte` file under `app/src/`; `app.css` is the only place allowed), `> every role pair meets its contrast target in both themes`.
3. Switching (AD-67, D-113): `ViewState` gains `theme` (Auto by default); `components/ThemeSwitch.svelte` in the header cycles Auto, Light, Dark and shows the resolved theme, with "auto" while it follows the system; `main.ts` sets `data-theme` from the stored value before mounting (none for Auto), so the first paint has the right theme; the header logo follows the resolved theme (`app/src/assets/logo-dark.svg`, a byte copy of `docs/assets/logo-dark.svg`). Proof: Playwright `app/e2e/themes.spec.ts > the system dark scheme gives the dark tokens`, `> a manual Light or Dark wins over the system and survives a reload`, `> the theme is set before the app mounts`, `> the header logo follows the theme`; Vitest `app/src/lib/view.test.ts > a stored view without theme reads as Auto`, `app/src/assets.test.ts > the app's logos are byte copies of docs/assets`.
4. Dark baselines (AD-70, D-119): `scene-a`, `scene-b` and `scene-c` match their dark baselines with `maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO`; the baselines come from the CI `windows` job's snapshot run and the owner compared them once with the mock's dark panels before they were committed (review F3). Proof: Playwright `app/e2e/scene.spec.ts > the three densities match their dark baselines`.
5. Contrast held in both themes for the scene (FR-6): the figure, room and antechamber pairs of D-117 are part of AC 1's test. Proof: `tokens.test.ts > every role pair meets its contrast target in both themes`.
6. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: dark theme following the system, manual override); README "The scene": the theme control. Proof: review.

## Tasks

- [ ] Dark values and new tokens in `app.css`; replace the hard-coded colours (AC 1, 2)
- [ ] `tokens.test.ts` cases: both themes, D-189's pairs, identical dark blocks, no colour literal in components (AC 1, 2, 5)
- [ ] `lib/view.svelte.ts` `theme` and resolved theme (a `matchMedia` listener); `ThemeSwitch.svelte`; `main.ts`; `logo-dark.svg` and the logo switch in `Header.svelte` (AC 3)
- [ ] `app/e2e/themes.spec.ts`; dark baselines from the CI snapshot run; `assets.test.ts` (AC 3, 4)
- [ ] Update `CHANGELOG.md` and the README (AC 6)

## Dev Notes

- Requirements: FR-8, FR-6 (contrast in both themes). Decisions: AD-67, AD-70 (tokens, themes, screenshots). Epic decisions: D-98, D-113, D-117, D-118, D-119, D-121, D-189.
- Derivation rules and contrast targets are AD-67's; the values were derived in the prototype (`epic-4-decisions.md` says where) and move once, with D-189's light values for the four window tokens, which the prototype lacks. `app.css` stays the only committed copy (frontend rule): the test reads values from it, its pairs are roles with targets, never values.
- `#C0603C` as the 12.5 px broken-contract text on `--color-bg` is 3.85:1, under 4.5: that is why one `--color-danger` takes today's darker `#8A2B14` for both the text and the banner border (D-189).
- AD-67's two selectors need the dark block twice in `app.css`; AC 1's second case is what keeps the two copies equal (D-117).
- Seams (D-98): v0.2 (relay chips, approval cards, the Relay view) and v0.3 (Subjects view, Triage panel) components may carry their own colour literals; AC 2's scan finds them and they move to tokens here; a terracotta literal marking a fault or a stop (v0.2's "relay stopped" chip, AD-35) becomes `--color-danger`, like the error banner's.
- The logo already exists in dark form (`docs/assets/logo-dark.svg`, the dark `--color-working` comes from it); the app keeps byte copies of the docs logos, as it does for `logo.svg`.
- `main.ts` reads the stored view synchronously before `mount` (it already awaits the e2e mock first). Storage errors fall back to Auto (D-113).
- AD-67's live-update gate (WebView2 following a Windows theme switch) is a workstation check closed in story 4.9, with D-118's fallback if it fails.

## Testing

- Vitest: `app/src/tokens.test.ts`, `app/src/assets.test.ts`, `app/src/lib/view.test.ts`.
- Playwright: `app/e2e/{themes,scene}.spec.ts` (`emulateMedia({ colorScheme })`).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: dark baselines from the CI snapshot run with an explicit diff ratio (F3, D-119); light values of the four window tokens and their contrast pairs (F15, D-189); "colour literal" defined, named colours found by the parser (F28)
