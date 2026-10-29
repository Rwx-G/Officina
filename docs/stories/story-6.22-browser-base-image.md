# Story 6.22: The browser base image

**Status:** Approved

## Story

As the author whose Svelte project's end-to-end suite runs a headless browser,
I want a second workshop base with Chromium, built, labelled, inventoried and battery-tested like the first, for project images that need it,
so that a worker can run that suite inside the boundary, reaching only the allow-list.

## Acceptance Criteria

1. **The Dockerfile** (AD-99, FR-16, the gate 8 entry). `docker/browser.Dockerfile`: `FROM officina-workshop:local`, Chromium and the libraries it needs from Debian, setuid and setgid bits removed again, `USER worker`, OCI labels; embedded by core, its SHA-256 a label; hadolint clean. Built only if the gate 8 entry passed (D-203); otherwise this story records that the browser base left v0.6 and closes with no code. Proof: CI `workshop` job (hadolint, build); core `crates/officina-core/src/workshop/image.rs::tests::embedded_dockerfiles_hash_to_their_labels` (now over three files).
2. **Build and check** (AD-99, FR-1). Build images builds `officina-workshop-browser:local` after the base, labelled `officina.base=browser`; a project Dockerfile `FROM officina-workshop-browser:local` passes `image::stages` and the inherited label check; `ImageKind::Browser` (declared in story 6.4) gets its behaviour; the image check covers it; the browser base and every project image built on it have their programs measured and their `ENV` names checked like any other image, a replaced program or a reserved name refused `not_from_base` (D-233, review F4 b). Proof: section `images` of `tests/vectors/workshop.json` (browser cases); CLI `crates/officina/tests/workshop_build.rs::{the_browser_base_builds_after_the_base, a_project_on_the_browser_base_is_accepted, a_project_on_the_browser_base_replacing_the_entry_point_is_refused}`.
3. **Battery and inventory** (AD-117, AD-118, FR-15). The battery plays on the browser base and on each project image built on it; the inventory runs on them; the CI `workshop` job builds and battery-tests the browser base too. Proof: CLI `crates/officina/tests/workshop_battery.rs::the_browser_base_gets_its_own_battery`; the CI job.
4. **Gate 8, second half** (AD-126 row 8, D-202; owner). On Docker Desktop: the owner's Svelte project image `FROM officina-workshop-browser:local` with its Playwright; a window run whose prompt runs the project's end-to-end suite against its own server inside the container (reached through `NO_PROXY`), with the Chromium flags the gate 8 entry recorded in the project's own configuration; a page on an off-list host fails with a `gateway_refused` line. Proof: the research notes' gate 8 entry, second half.
5. **Doctor** (AD-123 Slice B). The browser base present and matching. Proof: host `crates/officina-host/src/doctor.rs::tests::the_browser_base_is_checked_when_a_project_uses_it`.
6. **Documentation** (D-226). README: the browser base, when to use it, Chromium's own sandbox off under the engine's seccomp profile with the container as the boundary (AD-99), the flags the project's configuration needs; `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] `docker/browser.Dockerfile`; embedding and labels (AC 1)
- [ ] Build order, stage and label checks; image check (AC 2)
- [ ] Battery and inventory on it; CI job (AC 3)
- [ ] Gate 8's second half with the owner (AC 4); doctor (AC 5); README, `CHANGELOG.md` (AC 6)

## Dev Notes

- Requirements: FR-16, FR-1 (second base), FR-15 (battery on every image), FR-18 Slice B. Decisions: AD-99, AD-117, AD-118, AD-123. Facts: the gate 8 entry (first half). Epic decisions: P-52, D-200, D-202, D-203, D-226, D-233.
- Seams (D-200): the `workshop` CI job (stories 6.8, 6.10); Build images (story 6.8).
- Waits for: the gate 8 entry (first half); Slice A.
- Chromium's own sandbox cannot start under the engine's default seccomp profile without user namespaces; the container is the boundary (AD-99), and the vector stays the worker's, unchanged.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/image.rs`, `crates/officina-core/tests/workshop_vectors.rs`; CLI with the fake engine `crates/officina/tests/{workshop_build,workshop_battery}.rs`; host `crates/officina-host/src/doctor.rs`.
- CI `workshop` job (Linux engine): hadolint, build, battery on the browser base.
- Workstation, Docker Desktop (manual): gate 8's second half.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: programs measured and `ENV` names checked on the browser base and its project images (F4 b)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
