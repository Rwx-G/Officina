# Story 4.13: Dark theme and theme switching

**Status:** In progress

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

- [x] Dark values and new tokens in `app.css`; replace the hard-coded colours (AC 1, 2)
- [x] `tokens.test.ts` cases (AC 1, 2, 6)
- [x] `lib/view.svelte.ts` `theme`; `ThemeSwitch.svelte`; `main.ts`; `logo-dark.svg` and the logo switch (AC 3)
- [ ] `themes.spec.ts`; the dark run of `alert.spec.ts`; dark baselines from the CI snapshot run; `assets.test.ts` (AC 3-5). Done but the dark baselines: they wait for the CI snapshot run and the owner's comparison (D-508)
- [x] Update `CHANGELOG.md` and the README (AC 7)

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

- Dark values (AC 1): every colour token of `app.css` the prototype defines takes the prototype v2's dark value (`tokens.mjs` `base.dark` and `scene.dark`, `build.mjs`'s `extra.dark`), read by a script from the prototype's modules, not retyped; the light values in `app.css` matched the prototype's on every token. Tokens the prototype has and `app.css` does not use (`--scene-ante`, `--figure-head`, `--figure-head-shade`, `--figure-hair`, `--orch-tunic`, `--orch-tunic-shade`) were not ported. Both dark blocks also set `color-scheme: dark` (the light block `light`), as the prototype's `scene.css` does, so scrollbars and form controls follow; the equality test compares every declaration, `color-scheme` included.
- D-189's window tokens: `--color-danger` `#8a2b14` and `--color-danger-surface` `#f6e3dc` held the same values as v0.2's `--color-stopped-text` and `--color-stopped-surface`, so those two became aliases (`var(--color-danger)`, `var(--color-danger-surface)`): one value each, one dark value each. Dark values derived by AD-67's rules: `--color-danger` `#f29a80` (a text role, lightened: 8.18 on the background, 7.31 on the surface, 6.60 on its surface), `--color-danger-surface` `#3b2520` (a warm near-black of the light hue; text on it 12.16), `--color-column` the dark surface at the light wash's 0.55, `--color-attention-wash` unchanged (amber stays amber; text on it 11.86 in dark). The error banner's border was terracotta (`#c0603c`); D-189 makes `--color-danger` the border colour too, so it is darker in the light theme.
- One literal beyond D-189's list (AC 2's "any colour literal v0.2 and v0.3 components added"): the session file's shadow (`SessionFile.svelte`, story 4.8) gets `--color-shadow`, `rgb(42 36 28 / 0.12)` light and `rgb(0 0 0 / 0.4)` dark. `lib/scene/furniture.ts`'s `SHADE_1`, `SHADE_2` and `HIGHLIGHT` stay: they are black and white overlays that shade a face of any colour in either theme, as in the prototype, and they live in a module, not a component.
- The literal finder (AC 2): Svelte's own parser (`svelte/compiler`, already a dependency) gives every CSS declaration, markup attribute, style directive and script string of every `.svelte` file under `src/`, found by walking the folder. A notation is a hex value or a colour function (AC 2's list, plus `hwb`, `lab`, `lch`, `oklab`, `oklch`); a named colour is any word jsdom's CSS parser accepts as a `color` value, so no list of names is kept. Named colours are looked for in CSS values and attribute values, not in script strings: CSS keeps deprecated system colours named like ordinary words (`Background`), and `card.kind === "background"` was the first false positive. The CSS-wide keywords (`initial`, `unset`, `revert`, `revert-layer`) join AD-67's four exceptions, since none is a colour. A detector case proves each notation, a named colour in mixed case, a presentation attribute, a style directive and a script string are found, and that tokens and the exceptions are not.
- Contrast pairs (AC 2, 6): the test constant holds D-189's pairs and the prototype v2's, and completes "trousers and tunic on each new floor and rug" (the tunic on `--scene-rug-2` and the trousers on `--scene-rug` were missing); a translucent token is composited over what it lies on (`--color-column` and `--color-attention-wash` over `--color-bg`). The rim pairs are derived, not listed: every `--hair-*` token against `--figure-rim` over every floor a tunic pair names, at 3:1 (light: the rim is transparent; dark: 4.99 at worst, the darkest hair on the alert floor). A further case requires a dark value for every theme colour token except aliases and the people, and no dark token without a light one.
- Switching (AC 3): `lib/theme.ts` holds the choices (cycle order Auto, Light, Dark), the resolution and `applyTheme` (Auto removes `data-theme`, so the media query decides); `ViewStore` gains `theme`, `systemDark` (followed through `matchMedia`'s `change` event) and `resolvedTheme`, and every write stores the whole state, so a mode change keeps the theme (the earlier `setBase` wrote `base` alone, which would have dropped it). `main.ts` applies the stored theme before mounting, through `storedViewState`, which a throwing storage turns into the defaults. The theme cannot be set before the stylesheet paints without an inline script, which the window's policy forbids (`default-src 'self'`): the module script runs before the app draws anything, which `themes.spec.ts` checks at the first child of `#app`.
- The control (D-517, D-526): a button in the header's fixed tools, after Pin on top, whose `StableLabel` shows `Auto (light)`, `Auto (dark)`, `Light` or `Dark`; the accessible name is `Theme: ` plus that label, the tooltip says what a click gives. A first version with a visible "Theme: " prefix left the one-line header's status slot 50 px wide at 1920 px with the relay in use; the short labels give it 133 px. The eight-project cases at 1920 x 1001 now run with the relay in use (its toggle makes the widest fixed set) and assert that the header's items start on one line; the header measures 73.7 px with the relay in use, 72.7 without, and nothing scrolls, alert band included.
- Logo (AC 3): `app/src/assets/logo-dark.svg` is a byte copy of `docs/assets/logo-dark.svg` (it existed); `assets.test.ts` walks `src/assets` and compares every file with its namesake in `docs/assets`, so a third logo is covered too.
- Test names: story 4.7's `> every role pair meets its contrast target in the light theme` and `> every project colour meets 3:1 on the surface` now run in both themes and are named so; story 4.5's `> an alert changes no theme token` runs once per theme, `(light)` and `(dark)`. The light baselines case is now one of two cases built by a loop, its name unchanged.
- Not built: D-118's fallback (AD-67's live-update gate is story 4.14's workstation check). Playwright's `emulateMedia` proves the window follows a `prefers-color-scheme` change, not that WebView2 reports one when Windows switches.
- Found, not changed: the header's "relay stopped" chip draws `--color-surface` on `--color-stopped` (terracotta), 4.15:1 in the light theme for 13.5 px bold text, under the 4.5 a text pair needs (6.16 in dark). It predates this story (v0.2) and its look is approved, so it stays out of the pairs; it belongs in the backlog.
- One run of the whole suite with `--repeat-each=3` failed once on `stable-size.spec.ts > pin on top keeps its box and moves nothing after it` (whose follower is now the theme control); 40 repeats of that case and a second whole run with `--repeat-each=3` passed, and the failure's detail was overwritten before it was read.

### Completion Notes

- AC 1: Vitest `tokens.test.ts > every role pair meets its contrast target in both themes`, `> the dark values are the same under the theme attribute and under the media query`, `> the people tokens have no dark override`; also `> every colour token has a dark value, and every dark value a light token`, `> every project colour meets 3:1 on the surface in both themes`.
- AC 2: Vitest `tokens.test.ts > no component hard-codes a colour`, `> the detector finds every kind of colour literal and nothing else`, `> every role pair meets its contrast target in both themes` (D-189's pairs and the prototype v2's).
- AC 3: Playwright `themes.spec.ts > the system dark scheme gives the dark tokens` (Auto following a system switch live included), `> a manual Light or Dark wins over the system and survives a reload (light over dark)` and `(dark over light)` (from the keyboard, focus kept, back to Auto), `> the theme is set before the app mounts`, `> the header logo follows the theme`; Vitest `view.test.ts > a stored view without theme reads as Auto`, `> the theme cycles Auto, Light, Dark, is applied to the root and stored with the mode`, `> the resolved theme is the choice, or the system's scheme on Auto`, `> a storage that throws keeps the defaults` (the theme added); `assets.test.ts > the app's logos are byte copies of docs/assets`. D-517 and D-526: `stable-size.spec.ts > theme switch keeps its box and moves nothing after it`, `> theme switch on one line keeps its box and moves nothing after it`, `> header status chips move neither the gauges nor the buttons (board mode)` and `(scene mode)` (the theme control among the buttons); `scene.spec.ts > eight projects fit a maximised Full HD window` and `> ... with the alert band` (relay in use, one-line header asserted).
- AC 4: Playwright `alert.spec.ts > an alert changes no theme token (light)` and `(dark)`: tokens, `data-theme` and the resolved theme unchanged.
- AC 5: **Open.** Playwright `scene.spec.ts > the three densities match their dark baselines` is `test.fixme` until the baselines are committed (D-508). To close it: run the CI workflow by hand with `update_snapshots` ticked (the same run gives story 4.7's light ones), download the `playwright-snapshots` artifact, compare `scene-a-dark`, `scene-b-dark` and `scene-c-dark` with the prototype's `scene-interactive-dark-target.png` (its v0.4 parts: the baselines show the scene settled, without the prototype's panels, legend, taskbar mock or event log), write the date and verdict here, then commit the files under `app/e2e/scene.spec.ts-snapshots/`. A local render (not committed) showed the dark scene in the prototype's palette.
- AC 6: Vitest `tokens.test.ts > the allowed looks are the skin and hair pairs at 3:1 or more`, `> every role pair meets its contrast target in both themes` (the figure pairs, and the hair against the rim over every floor).
- AC 7: `CHANGELOG.md` `[Unreleased]` Added: the dark theme following the system, the manual override; README "The scene (v0.4, in progress)" gains "Light and dark" (the control, Auto, the kept choice, the logo, no change on alert).

## File List

- `app/src/app.css`
- `app/src/App.svelte`
- `app/src/main.ts`
- `app/src/lib/theme.ts` (new)
- `app/src/lib/view.svelte.ts`
- `app/src/lib/view.test.ts`
- `app/src/components/ThemeSwitch.svelte` (new)
- `app/src/components/Header.svelte`
- `app/src/components/BoardColumn.svelte`
- `app/src/components/SessionFile.svelte`
- `app/src/assets/logo-dark.svg` (new, a copy of `docs/assets/logo-dark.svg`)
- `app/src/tokens.test.ts`
- `app/src/assets.test.ts` (new)
- `app/e2e/themes.spec.ts` (new)
- `app/e2e/alert.spec.ts`
- `app/e2e/scene.spec.ts`
- `app/e2e/stable-size.spec.ts`
- `app/e2e/fixtures.ts`
- `CHANGELOG.md`
- `README.md`
- `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created as story 4.8
- 2026-10-28 reworked after review (F3, F15, F28)
- 2026-10-28 renumbered 4.11 for the scene v2 (D-460): the prototype v2's tokens, the people palette, no theme change on alert
- 2026-10-28 renumbered 4.13 after the plan challenge (D-497, D-498, D-508): the dark baseline comparison is recorded; story numbers updated
- 2026-10-28 implemented: the dark values and D-189's window tokens in `app.css`, no colour literal in any component, the contrast pairs in both themes, the theme control with Auto, Light and Dark, the theme set before mounting, the logo following it, the alert's no-theme-change case in both themes. Status In progress until the owner compares the CI dark baselines (D-508)
