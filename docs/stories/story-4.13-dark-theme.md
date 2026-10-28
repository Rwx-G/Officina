# Story 4.13: Dark theme and theme switching

**Status:** Approved

## Story

As the author who works late,
I want the whole window, board and scene alike, to follow the system's dark or light theme, with a manual override that sticks, and an alert never to change it,
so that the company is comfortable day and night and never flashes light when it opens.

## Acceptance Criteria

1. Dark values (FR-8, AD-67 as amended, D-117): the dark values of every base token, every `--scene-*`, `--figure-*` and alert token, and the plum project colour move from the prototype v2's tokens (`tokens.mjs` and `build.mjs`'s additions) into `app.css`, under `:root[data-theme="dark"]` and under `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) }`; light values stay; the people tokens (`--skin-*`, `--hair-*`) are the same in both themes and appear once. Proof: Vitest `app/src/tokens.test.ts > every role pair meets its contrast target in both themes`, `> the dark values are the same under the theme attribute and under the media query`, `> the people tokens have no dark override`.
2. The whole window (AD-67, D-189): `--color-danger`, `--color-danger-surface`, `--color-column` and `--color-attention-wash` replace the colours hard-coded in `App.svelte`, `Header.svelte`, `BoardColumn.svelte` and any colour literal v0.2 and v0.3 components added; the test constant gains D-189's pairs and the prototype v2's (text on the alert colour 4.5; the alert colour on the background 3; trousers and tunic on each new floor and rug 3; the alert colour on amber as decor). A colour literal is a hex value, `rgb(`, `rgba(`, `hsl(`, `hsla(`, `color(` or a named colour, except `transparent`, `currentColor`, `inherit` and `none`; named colours are found by the parser (D-189's method). Proof: Vitest `tokens.test.ts > no component hard-codes a colour`, `> every role pair meets its contrast target in both themes`.
3. Switching (AD-67, D-113): `ViewState` gains `theme` (Auto by default); `components/ThemeSwitch.svelte` cycles Auto, Light, Dark and shows the resolved theme; `main.ts` sets `data-theme` before mounting; the header logo follows the resolved theme (`app/src/assets/logo-dark.svg`, a byte copy of `docs/assets/logo-dark.svg`). Proof: Playwright `app/e2e/themes.spec.ts > the system dark scheme gives the dark tokens`, `> a manual Light or Dark wins over the system and survives a reload`, `> the theme is set before the app mounts`, `> the header logo follows the theme`; Vitest `app/src/lib/view.test.ts > a stored view without theme reads as Auto`, `app/src/assets.test.ts > the app's logos are byte copies of docs/assets`.
4. No theme change on alert (FR-8, D-473): story 4.5's case, as story 4.7 extended it, runs in both themes. Proof: Playwright `app/e2e/alert.spec.ts > an alert changes no theme token` (light and dark).
5. Dark baselines (AD-70, D-119): `scene-a`, `scene-b` and `scene-c`, settled, match their dark baselines with `maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO`; the baselines come from the CI snapshot run, and the owner's comparison with `scene-interactive-dark-target.png` (its v0.4 parts) is written in Completion Notes with its date and verdict before they are committed (D-508). Proof: Playwright `app/e2e/scene.spec.ts > the three densities match their dark baselines`; the Completion Notes entry.
6. Looks in both themes (D-478): the allowed skin and hair pairs keep 3:1 whatever the theme, since both are theme-independent, and the figure rim (`--figure-rim`) keeps hair readable on dark floors. Proof: `tokens.test.ts > the allowed looks are the skin and hair pairs at 3:1 or more`, `> every role pair meets its contrast target in both themes` (figure pairs).
7. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: dark theme following the system, manual override); README "The scene": the theme control. Proof: review.

## Tasks

- [ ] Dark values and new tokens in `app.css`; replace the hard-coded colours (AC 1, 2)
- [ ] `tokens.test.ts` cases (AC 1, 2, 6)
- [ ] `lib/view.svelte.ts` `theme`; `ThemeSwitch.svelte`; `main.ts`; `logo-dark.svg` and the logo switch (AC 3)
- [ ] `themes.spec.ts`; the dark run of `alert.spec.ts`; dark baselines from the CI snapshot run; `assets.test.ts` (AC 3-5)
- [ ] Update `CHANGELOG.md` and the README (AC 7)

## Dev Notes

- Requirements: FR-8, FR-6 (contrast in both themes). Decisions: AD-67 as amended, AD-70. Epic decisions: D-113, D-117, D-118, D-119, D-121, D-189, D-473, D-478, D-508.
- Dependencies: stories 4.5 (alert tokens and band), 4.7 (light tokens, scene) and 4.9 (the alert room's tokens).
- The values were derived in the prototype and the prototype v2 (`build.mjs` checks every pair it adds and refuses a miss); they move once, and `app.css` stays the only committed copy.
- AD-67's two selectors need the dark block twice; AC 1's second case keeps the copies equal.
- AD-67's live-update gate (WebView2 following a Windows theme switch) is a workstation check closed in story 4.14, with D-118's fallback if it fails.

## Testing

- Vitest: `app/src/tokens.test.ts`, `app/src/assets.test.ts`, `app/src/lib/view.test.ts`.
- Playwright: `app/e2e/{themes,scene,alert}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created as story 4.8
- 2026-10-28 reworked after review (F3, F15, F28)
- 2026-10-28 renumbered 4.11 for the scene v2 (D-460): the prototype v2's tokens, the people palette, no theme change on alert
- 2026-10-28 renumbered 4.13 after the plan challenge (D-497, D-498, D-508): the dark baseline comparison is recorded; story numbers updated
