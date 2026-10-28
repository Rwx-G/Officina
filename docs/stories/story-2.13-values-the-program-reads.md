# Story 2.13: Values the program reads, and approvals that name their action

**Status:** Approved

## Story

As the author writing relay actions for ssh and Ansible,
I want Officina to tell me when a parameter value reaches a remote shell or an Ansible template, and every approval line to name its action,
so that I do not trust a closed argument vector to close what the program does with a value, and the approval metrics of later versions can be computed from the journal.

## Acceptance Criteria

1. Catalogue notices (D-172): phase 2 produces, beside issues, non-blocking notices that keep the action. Two kinds, in a `closed_enum!`: `RemoteCommandValue` (the action's program file name is `ssh`, case-insensitive, `.exe` stripped, and a placeholder sits in an `args` item after the first non-option argument, the destination) and `TemplatedValue` (the program file name starts with `ansible`, and a placeholder sits in the item following `-e`, `--extra-vars`, `-a` or `--args`, or in an item starting with `--extra-vars=` or `--args=`). Each notice carries the action, the parameter and the item index. Proof: core `crates/officina-core/src/relay/catalogue.rs::tests::{ssh_placeholder_after_the_destination_is_a_remote_command_value, ssh_placeholder_as_the_destination_is_no_notice, ansible_extra_vars_placeholder_is_a_templated_value, ansible_args_placeholder_is_a_templated_value, notices_keep_the_action}` and a notice section in the catalogue vector set if one exists, iterating the notice kinds.
2. Notices are not part of the canonical form or its hash (they are derived from it), so they never void an approval. Proof: `catalogue.rs::tests::notices_do_not_change_the_hash`.
3. The window's catalogue banner and `officina relay --json` list the notices with a one-line explanation per kind; doctor prints one warning line per notice (a warning, not a failure). The explanation for `RemoteCommandValue` says the remote shell reads the value, so a pattern admitting spaces or shell characters lets a session run any command on that host; for `TemplatedValue` it says Ansible templates the value, so a pattern admitting `{` or `%` lets a session run code on the control node. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::each_notice_is_one_warning_line`, CLI `crates/officina/tests/relay.rs::relay_json_lists_notices`, Playwright `app/e2e/relay.spec.ts > the banner lists catalogue notices`.
4. The README's sentence "always becomes exactly one argument: no shell ever reads it" is corrected: no local shell reads a value, but the program may (an ssh remote command, Ansible templating of extra vars and module arguments, and path expansion of `~` and `$VAR` by Ansible), and the README tells the author to give such parameters `values` lists or patterns without whitespace and shell or template characters; it names the notice kinds' constant, never a list. Proof: `crates/officina/tests/readme.rs` gains `readme_names_the_notice_kinds` (the README's sentence and the constant agree).
5. Approval journal lines name their action (D-172): `approval_requested` gains `project`, `action` and `dry_run` (true when the action names a check); `approval_given` and `approval_rejected` gain `project` and `action`. The fields are optional on read (skipped when absent) and `JOURNAL_LINE_VERSION` does not change. Proof: core `crates/officina-core/src/relay/journal.rs::tests::{approval_requested_names_its_action, older_approval_lines_still_parse}`, CLI `crates/officina/tests/approvals.rs::every_request_and_decision_leaves_exactly_one_line` extended to assert the new fields.

## Tasks

- [ ] Core: notice type, the two detection rules, notices beside issues in phase 2, out of the canonical form (AC 1, 2)
- [ ] Host and window: notices in the relay view, banner, `relay --json`, doctor warnings (AC 3)
- [ ] README correction and readme test (AC 4)
- [ ] Journal: approval event fields, writers in `run` and `authority` (AC 5)
- [ ] Update `CHANGELOG.md` `[Unreleased]` (Security for AC 1-4, Changed for AC 5)

## Dev Notes

- Source: the Epic 8 story review (F1, F8). OpenSSH joins every argument after the destination into one remote command string that the remote user's shell parses. Ansible templates `{{ }}` and `{% %}` in extra vars and module arguments when they are used, and expands `~` and `$VAR` in paths; whether ansible-core 2.19's trust model changes templating of extra vars is not verified, so the notice stays whatever the version.
- These are notices, not issues: the author may want a remote command parameter with a closed `values` list, which is safe. Refusing values for workers is v0.8's job (`relay::worker::WORKER_VALUE_REFUSED`, Epic 8), not this story's.
- AD-30's value rules are unchanged.
- Decisions: D-172 (this story), D-27, D-34.

## Testing

- Core unit: `crates/officina-core/src/relay/{catalogue,journal}.rs`.
- Host unit: `crates/officina-host/src/relay/doctor.rs`.
- CLI integration: `crates/officina/tests/{relay,approvals,readme}.rs`. Playwright: `app/e2e/relay.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created from the Epic 8 story review (F1, F8): the README's "no shell ever reads it" was false for ssh and Ansible, and approval lines could not be attributed to actions (D-172)
