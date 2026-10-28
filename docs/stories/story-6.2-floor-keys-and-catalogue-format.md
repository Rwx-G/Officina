# Story 6.2: Workshop floor keys, phase 2 rules, catalogue files and the canonical format

**Status:** Approved

## Story

As the author who decides what a workshop run may hold before any run exists,
I want the `[workshop]`, `[project.workshop]` and `[template.workshop]` tables validated in core, their files read within bounds, and every key part of the catalogue I approve,
so that a changed host, mount, Dockerfile, CA file or limit voids my approval for the workshop, a malformed table turns the workshop off without touching the board, and no workshop file can stop my relay actions.

## Acceptance Criteria

1. **Phase 1** (AD-101, AD-23). `ProjectEntry` gains `workshop: Option<toml::Spanned<toml::Value>>` and the floor a raw `workshop`, both declared raw fields (the structs are `deny_unknown_fields`); `RELAY_KEYS` gains `workshop` and `project.workshop`. A malformed workshop table never invalidates the floor the board uses; the hook's gate reads phase 1 only. Proof: core `crates/officina-core/src/floor.rs::tests::{a_malformed_workshop_table_keeps_the_board_floor, workshop_keys_are_relay_keys}`; the existing D-7 test over `RELAY_KEYS` stays green.
2. **Types and rules** (AD-101, FR-2 as amended by amendment 3, D-236, D-248). `officina-core::workshop::floor` declares `WorkshopTable`, `ProjectWorkshop`, `TemplateWorkshop` and the `closed_enum!` `CredentialKind` (story 6.4's migration reads its `ALL`); `officina-core::workshop::image::stages` parses a Dockerfile's `FROM` lines and stage aliases (story 6.8 adds labels and the image check). Defaults are explicit. Each phase 2 rule is an `IssueKind` variant with a failing fixture under `every_issue_kind_has_a_case` (D-132): `auth` value; `max_workers` 1 to 4; `cpus`, `memory_mib` positive; `upstream_proxy` `host:port`, DNS name or IPv4, no scheme, no credentials; `ca_file` absolute, a PEM file of at most 64 KiB holding only `CERTIFICATE` blocks with valid base64; `mounts` relative, no `.` or `..` segment, at most 8, no duplicate ignoring case, not `.git` or under it (existence, ignore status and reparse points are launch conditions, story 6.11); `hosts` lowercase DNS names, at most 32, no IP literal, no port, a leading `*.` only on a name of three labels or more, a last label holding a letter (repeating a fixed host is story 6.5's rule, with `CLAUDE_CODE_HOSTS`); `dockerfile` inside the project, no `..` segment, at most 64 KiB, its folder at most 32 regular files and 1 MiB, no subfolder, no symlink, its final stage descending, through stage aliases, from `officina-workshop:local` or `officina-workshop-browser:local` (other stages may name any image, AD-99); `review` `groups` or `all`; `sensitive_paths` at most 32 glob patterns; `memory_name` one path segment that is not `.` or `..` and holds no `\`, `/` or `:` (validated now, used by story 6.21, P-53, D-236); `base` at most 8 names passing git's branch-name rules in core; `max_minutes` 5 to 240, default 60; `max_usd` at most two decimals, stored as cents (only if the gate 6 entry kept it, D-203); a workshop project's name passing `is_valid_action_name` and not ending in `-` (D-248); two projects whose image names collide; a workshop template naming a project without `[project.workshop]`; an invalid `[workshop]` turning the workshop off and dropping every workshop template with an issue, host templates unaffected. Proof: core `crates/officina-core/src/workshop/floor.rs::tests::{defaults_are_explicit, an_invalid_workshop_table_drops_every_workshop_template_only, a_template_with_a_workshop_table_runs_only_in_the_workshop, branch_names_follow_git_rules, a_project_name_ending_in_a_dash_is_an_issue, memory_name_refuses_dot_dotdot_backslash_and_colon, mounts_differing_only_in_case_are_duplicates}`; `crates/officina-core/src/workshop/image.rs::tests::{the_final_stage_must_descend_from_a_base, an_alias_chain_is_followed, a_builder_stage_may_name_any_image}`; one fixture per new `IssueKind` through `crates/officina-core/src/relay/catalogue.rs::tests::every_issue_kind_has_a_case` (iterates `IssueKind::ALL`).
3. **Catalogue files** (AD-101, D-244). `officina-host::workshop::files::read(&FloorFile) -> CatalogueFiles` reads each workshop project's Dockerfile, its folder's files (names, lengths, bytes) and the CA file, each read bounded at its limit plus one byte; a larger file, a subfolder, a symlink or a read failure is an issue, never a partial read. Phase 2 takes `&CatalogueFiles`. The window, doctor and `relay --json` call it; `officina run` and the hook never do. Proof: host `crates/officina-host/src/workshop/files.rs::tests::{reads_within_bounds, an_oversized_dockerfile_is_an_issue_not_a_truncation, a_subfolder_or_symlink_in_the_context_is_an_issue}`; host integration `crates/officina-host/tests/relay_home.rs::the_hook_never_reads_catalogue_files` (a floor whose Dockerfile path is unreadable: the gate's answer is unchanged).
4. **Canonical form, format and the relay-section hash** (AD-101, AD-24, AD-76, D-131, D-244, P-53). `CanonicalWorkshop` holds `auth`, `max_workers`, `cpus`, `memory_mib`, `upstream_proxy`, CA path and SHA-256; per project mounts and hosts sorted, the Dockerfile path, its SHA-256 and the context SHA-256 (over the sorted file names, lengths and bytes), `review`, `sensitive_paths` sorted, `memory_name`; per template the base list in order, minutes and cents; every default explicit. `approved.json` keeps, beside the global hash, the relay-section hash: the hash of the canonical sections `officina run` uses, computed with no workshop file. `CANONICAL_FORMAT` goes up by one relative to `main` at this merge; AD-24's golden hash and the relay-section golden hash are pinned once. Proof: core `crates/officina-core/tests/catalogue_hash.rs::{golden_hash_is_pinned, relay_section_hash_is_pinned, a_previous_format_approval_is_void}` (the last loads an `approved.json` serialized from the real previous form and asserts AD-76's four facts); `catalogue.rs::tests::{a_changed_dockerfile_byte_changes_the_global_hash_only, a_changed_context_file_changes_the_global_hash_only, a_changed_ca_file_changes_the_global_hash_only, a_changed_action_changes_both_hashes, host_order_does_not_change_the_hash, base_order_does_change_the_hash}`.
5. **Diff and banner** (AD-101, AD-75). `catalogue::diff` gains the workshop, project and template sections; the Relay view's approval banner shows them with the same rules as actions and templates. Proof: core `catalogue.rs::tests::diff_lists_workshop_changes`; `app/e2e/relay.spec.ts > the approval banner shows a workshop change` (fixture from `pnpm e2e:fixtures` with a changed `hosts` entry).
6. **`officina run` compares the relay-section hash** (AD-29 step 2 as D-244 reads it). `officina run` recomputes and compares the relay-section hash only, reading no workshop file: a project Dockerfile edited after approval, a moved workshop project folder or an unreadable CA file leaves every relay action runnable, and a workshop launch is refused on the global hash (story 6.11, AD-78's check). Proof: CLI `crates/officina/tests/run.rs::{an_edited_project_dockerfile_does_not_block_relay_actions, an_unreadable_ca_file_does_not_block_relay_actions, an_edited_action_still_refuses_until_approved}`.
7. **Documentation** (D-226). `CHANGELOG.md` `[Unreleased]` under Added: the workshop floor keys; under Changed: the catalogue format goes up once, voiding the approval (R39's upgrade note), and workshop changes need re-approval before a workshop launch but never block relay actions. README "The workshop (v0.6, in progress)": the keys with a placeholder example, beside `hosts` the sentence that an allowed host accepting anonymous uploads is a leak the allow-list cannot close. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [ ] `floor.rs`: raw tables, `RELAY_KEYS` (AC 1)
- [ ] `crates/officina-core/src/workshop/{mod,floor,image}.rs`: types, `CredentialKind`, defaults, rules, branch-name rule, `image::stages` (AC 2)
- [ ] `IssueKind` variants and fixtures (AC 2)
- [ ] `crates/officina-host/src/workshop/files.rs`; phase 2 signature; callers (AC 3)
- [ ] `CanonicalWorkshop`, the relay-section hash in `approved.json`, the format bump, both golden hashes, the previous-format fixture (AC 4)
- [ ] `officina run` on the relay-section hash (AC 6)
- [ ] `catalogue::diff` sections and the banner (AC 5)
- [ ] `CHANGELOG.md`, README (AC 7)

## Dev Notes

- Requirements: FR-2 as amended (amendment 3), FR-1 (`dockerfile`, `ca_file`), FR-4 (`upstream_proxy`, `hosts`), FR-11 (`review`, `sensitive_paths`), FR-13 (`memory_name`). Decisions: AD-23, AD-24, AD-29, AD-75, AD-76, AD-101. Gates: the gate 7 entry (no code change unless neither kind, D-203), the gate 6 entry (`max_usd` kept or not). Risks: R39, R55. Epic decisions: P-7, D-7, D-131, D-132; P-50, P-53, P-56, D-200, D-203, D-226, D-236, D-244, D-248.
- Seams (D-200): `floor::RELAY_KEYS` (2.1), `relay::catalogue` phase 2, canonical form and diff (2.1, 2.2, 2.11, 5.4), the Relay view's banner (2.7, 2.9), `officina run`'s hash (2.5).
- Waits for: the gate 7, 0 and 6 entries (review F46: `CLAUDE_CODE_HOSTS` moved to its first reader, story 6.5).
- Why mounts are syntactic here (AD-101): existence and ignore status change without an edit of the floor; they are launch conditions and doctor checks (story 6.11).
- Why a second hash (D-244, review F25, an owner decision taken as recommended and listed in the QA report): `officina run` recomputes on every call; with one hash, a Dockerfile edit, a moved project folder or a sync placeholder in place of the CA file (R55) would refuse every relay action in every project until re-approval, and each call would read up to 1 MiB per workshop project. It gives up one number for the whole catalogue.
- `image::stages` is the one Dockerfile parser: Build images (story 6.8) reuses it before each build. FR-1's "first stage" and FR-2's "first `FROM`" read as amendment 2's final stage (P-50).
- Every story of Epic 6 that adds a canonical key would bump the format again; none does (P-53). `memory_name` validates now and does nothing until 6.21.
- No employer content in fixtures: `example.com` hosts, `/path/to/` paths.

## Testing

- CI, no Docker. Core unit: `crates/officina-core/src/{floor,workshop/floor,workshop/image,relay/catalogue}.rs`; core integration: `crates/officina-core/tests/catalogue_hash.rs`.
- Host unit: `crates/officina-host/src/workshop/files.rs`; host integration: `crates/officina-host/tests/relay_home.rs`.
- CLI integration: `crates/officina/tests/run.rs`. Playwright: `app/e2e/relay.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: relay-section hash, `officina run` reads no workshop file (F25); `memory_name` refuses `.`, `..`, `\`, `:` and mounts compare ignoring case (F11, F3); a project name may not end in `-` (F30); `CLAUDE_CODE_HOSTS` and its issue move to 6.5, so this story no longer waits for the gate 3 entry (F46); deny-unknown-fields noted
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
