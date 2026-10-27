# Story 1.13: Release and README

**Status:** Approved

## Story

As the author installing Officina on a workstation, and as a future reader of the repository,
I want a CI-built installer and portable zip, a README that explains what Officina is and how to set it up, and a changelog,
so that v0.1 can be installed in minutes and understood without reading the code.

## Acceptance Criteria

1. `.github/workflows/release.yml` runs on tag `v*` on Windows, builds the NSIS installer and the release CLI, packages the portable zip with exactly the four files AD-18 names, computes SHA-256 sums and creates the release with `gh release create`, with no third-party release action (AD-18). Proof: a `workflow_dispatch` dry run of the same workflow that stops before `gh release create` and asserts the zip listing equals the expected four entries (so `officina-fake-claude.exe` can never ship); run link in Completion Notes.
2. The release binaries are the ones CI tested: the workflow runs the `windows` job gates before packaging. Proof: workflow structure, reviewed in the PR.
3. `pnpm readme-assets` produces the README screenshots and animated GIF from the e2e scenarios with frozen time, using `gifenc` and `pngjs` (AD-17, spine section 4), into `docs/assets/`. Two consecutive runs produce byte-identical files. Proof: run twice, `git diff --exit-code docs/assets` after the second run, recorded in Completion Notes.
4. `README.md` contains: logo, what and why, install (installer and portable), setup through `officina doctor` (hooks block and statusline wiring printed by doctor, not copied into the README), floor file example with placeholder names, privacy section (what is stored, where, retention, `OFFICINA_NO_EXCERPTS`, no network surface, no writes under the Claude config dir), unsigned binaries and SmartScreen note (R7), autostart through a Startup-folder shortcut (spine section 3), screenshots and GIF, contributing note on SynologyDrive and `CARGO_TARGET_DIR` (R8). Proof: manual review against this list (prose cannot be tested automatically).
5. The README transcribes no list owned by code: it names `officina doctor` for the hooks block, the contract constant for checked fields, and `officina.toml` schema by pointing at the core type. Proof: review; `grep -n "SessionStart\|UserPromptSubmit" README.md` returns nothing outside a link to doctor output.
6. `CHANGELOG.md` `[Unreleased]` entries accumulated by stories 1.1-1.12 are moved under `[0.1.0]` with the release date. Proof: review.
7. Release smoke test on a clean Windows user profile: install with NSIS, run doctor, open the board; and the AD-17 per-release WebView2 eyeball. Proof: manual checklist, results in Completion Notes.

## Tasks

- [ ] Write `release.yml` with the tag trigger and a `workflow_dispatch` dry-run input (AC 1, 2)
- [ ] Packaging step with zip listing assertion and SHA-256 sums (AC 1)
- [ ] Add `gifenc` and `pngjs` devDependencies; write `app/scripts/readme-assets.mjs` and the `readme-assets` script on top of Playwright with `page.clock.install` (AC 3)
- [ ] Generate and commit `docs/assets/` screenshots and GIF (AC 3)
- [ ] Write `README.md` (AC 4, 5)
- [ ] Cut `[0.1.0]` in `CHANGELOG.md` (AC 6)
- [ ] Run the release smoke checklist (AC 7)

## Dev Notes

- Decisions: AD-17 (README assets row, WebView2 eyeball), AD-18 (release workflow, unsigned binaries), spine section 3 (autostart rejected, documented instead), section 4 (Node devDependencies). Risks: R7, R8, R11 (privacy section).
- Requirements: PRD section 6.1 (two binaries, portable and installer, README with logo, screenshots and animated demo).
- No employer content anywhere in the README or the screenshots: scenarios use placeholder repository names (officina-project rule).
- The logo already exists at `docs/assets/logo.svg` and PNGs; reuse it.
- Tagging rule: `.claude/rules/officina-git.md` says to write it when the release pipeline is built; add the rule there in this story.

Manual checklist:
- [ ] Clean user profile: NSIS install, SmartScreen behaviour noted.
- [ ] `officina doctor` from the installed path prints a hooks block with that path.
- [ ] Board opens; WebView2 rendering matches the README screenshots by eye.
- [ ] Portable zip: extract, run `officina-app.exe`, same result.

## Testing

- Release workflow dry run (automated listing assertion).
- `pnpm readme-assets` determinism check.
- Manual: release smoke checklist.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
