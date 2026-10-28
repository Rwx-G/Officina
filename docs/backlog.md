---
title: Technical backlog
author: Rwx-G
---

# Technical backlog

Non-blocking items from reviews and audits. The feature roadmap is in the
README, not here.

## High

- Story 2.15 AC 11, before the first published release: generate a
  third-party notices file at release time (the Rust crates and npm
  packages the binaries and the window ship, with their license texts, and
  the bundled font's OFL) and put it in the portable zip and the NSIS
  installer. The binaries carry MIT, BSD, ISC (ring, with its
  OpenSSL-derived parts), MPL-2.0 and OFL-1.1 code whose terms want the
  notice to travel with them; `cargo deny` checks that a license is
  allowed, not that its notice ships. Not built until the owner asks for a
  release.
- Story 1.7: run the manual focus checklist (two editor windows, a subfolder
  session, a minimised window, another virtual desktop, Windows Terminal) and
  record the results in `docs/research-notes.md`.
- Story 2.4 AC 18: in an interactive session in `bypassPermissions` mode,
  with the `PreToolUse` hook doctor prints registered and a guarded project
  approved, ask for a raw `ssh` and check that the call is refused with the
  gate's reason; record it in `docs/research-notes.md` (E12 and E15 were
  `claude -p` and background sessions).
- Story 2.12 AC 6: in the same kind of interactive session with the
  PowerShell tool on (Windows, claude.ai account) and the `PreToolUse`
  matcher doctor prints merged, ask for any command through PowerShell in
  the guarded project and check that the call is refused with the gate's
  reason pointing at the Bash tool; record the Claude Code version in
  `docs/research-notes.md`.
- Story 2.11 AC 18: store a credential with the `cmdkey` line `officina
  doctor` prints (it prompts for the secret), then run an HTTP action with
  that credential against a real internal `https` endpoint whose CA is
  trusted only through the Windows store; check that the request succeeds
  (the platform verifier accepts the enterprise CA) and that the journal
  shows the run without the secret; record it in `docs/research-notes.md`
  with Windows, `ureq` and endpoint details redacted.
- Story 2.10 AC 10: with a guarded repository whose catalogue declares a WSL
  `ansible-playbook` check (`--check --diff`) and its `after-check` apply on
  a lab group, approved in the window and with doctor's `CLAUDE.md`
  paragraph pasted, ask a session to deliver a small change on the lab
  group and let it run check then apply through `officina run` without
  pasting anything (PRD journey 2, "Delegation" metric); record the Claude
  Code and WSL versions, the journal lines and any refusal in
  `docs/research-notes.md` (E13). The `--cd` part of D-6 is already
  verified there.

- Story 3.12 AC 4: on the workstation, with the Claude Code version each
  time, record in `docs/research-notes.md` the `SessionEnd.reason` sent
  when a VS Code terminal hosting an interactive session is closed; for
  `/clear` and `/resume`, the session ids before and after and the gap
  between `SessionEnd` and `SessionStart` (revisit `SUPERSESSION_WINDOW_MS`
  if it does not fit, D-52); that `claude --resume <id>` and `claude -c`
  keep the session id (D-75); and the Subjects view, the Triage panel and
  the evidence in WebView2 against Playwright Chromium, by eye.
- Story 3.12 AC 5: a week of real use on a build holding the Epic 3 audit
  fixes, with the counts the AC names (restarts and closed nights, sessions,
  subjects and confirmations before and after each restart, sessions filed
  by rule and how many were moved, confirmations per finished subject,
  subjects woken up, doctor's state), recorded in `docs/research-notes.md`.
- Epic 3 audits, D-439: the triage call's environment is now an allow list.
  Run `examples/triage_probe.rs` once more on the workstation and record in
  E16 that the call still authenticates and answers (through the corporate
  proxy if one is set) with the variables the allow lists keep; if a
  variable the author's setup needs is missing, add it to the list that
  owns it, with its reason.
- Epic 3 security audit, Info: whether `--setting-sources ""` stops Claude
  Code reading a `CLAUDE.md` in the folders above the triage work folder
  (`<home>/.officina/triage/`, `<home>/.officina/`) is not proved. Extend
  the probe with a `CLAUDE.md` planted in `<home>/.officina/triage/` and
  check the input tokens do not move; if they do, refuse a call while a
  `CLAUDE.md` or `.claude/` sits in those folders, with a vector case.

## Owner requested

- French UI (owner request, 2026-10-28; after v0.4, not a v0.4 story):
  the window's strings move to a typed dictionary in the frontend, one
  module per language (`en`, `fr`), a missing key failing the typecheck,
  with no new dependency. The language follows the Windows locale, with a
  setting that overrides it and is kept like the theme. The CLI and
  `officina doctor` stay English, since their output is pasted into
  settings files and issues. Playwright runs pinned to one language, so
  its cases keep matching text; a Vitest case checks every key exists in
  both. Slot to confirm with the owner; plan it before v0.5's UI grows,
  since every new string adds to the extraction.

## Recurring, at each PRD close

The dependency checks no advisory database raises (Epic 3 dependency
audit, D-450), run before the PRD's QA report and recorded in it:

- The bundled SQLite: compare `SQLITE_VERSION` in the locked
  `libsqlite3-sys`'s `bindgen_bundled_version.rs` with the latest release
  on sqlite.org, and run `cargo update -p libsqlite3-sys` when it is
  behind (a SQLite fix without a CVE never reaches RustSec).
- The toolchain: move `rust-toolchain.toml` to the latest point release of
  its minor version, with CI, which reads the same file.
- The audit tools: bump the exact cargo-deny and pnpm versions of
  `.github/workflows/ci.yml` (and pnpm in `release.yml`) on purpose, reading
  their release notes for changes in what they report.
- The ignores: re-check each `auditConfig.ignoreGhsas` entry of
  `app/pnpm-workspace.yaml` and each `deny.toml` exception, and date the
  review in its comment.

## Low

From the Epic 2 security delta audit (story 2.16 AC 6, D-175); each waits
for the reason given. The relay stays a guardrail, not a boundary: the
isolated workshop (v0.6) is the boundary.

- The gate resolves the session's folders by handle before it matches
  them as given, and Claude Code ends a hook after 5 s; a folder on a share
  that accepts the connection and then stalls makes the gate give no
  decision. Waits for: matching the folders as given first and resolving
  only on no match, a time limit on each resolution read as "unreadable"
  (stricter, D-15), and Claude Code's behaviour on a hook timeout checked
  at the workstation and recorded in `docs/research-notes.md`.
- A shell command from a free project can write or delete relay files (the
  gate decides nothing outside a guarded project, D-14), while the file
  tools are denied there. Waits for: a decision on applying the
  protected-directory rule to shell commands from every project once the
  relay guards one, with its vector cases and a reason that names no
  project. Since D-175 the costliest effects are refusals rather than
  silent passes: a deleted or broken decision record refuses every run
  until the author acknowledges it, and a forged approval or catalogue is
  refused.
- A hard link to a relay file placed outside the home, or a loopback UNC
  name of the home, is not recognised as under the home by the file-tool
  rule. Waits for: comparing file identities (volume serial and file
  index) in `platform/windows.rs`, or refusing writes to files with more
  than one link while the relay guards a project. Officina's atomic writes
  break a hard link to a file it writes at its next write.
- The window hashes a pending request's tree on its collector thread, up to
  the fingerprint's limits, every pass: a session can slow the board with
  a large tree. Waits for: fingerprints off the collector thread with a
  deadline, a streamed listing that stops at the file limit, and a longer
  recheck after a large tree. No integrity effect: the run checks the tree
  itself.
- A tool an MCP server adds that writes files is outside `FILE_TOOLS`, so
  the file-tool rule does not see it (the README says so). Waits for: a
  decision on `mcp__*` tools whose input holds a path under the home, once
  a workstation uses such a server.

From the Epic 4 plan challenge (`docs/stories/epic-4-decisions.md` D-509):

- Story 6.23 (scene workers): rework it against D-460 to D-514 before
  Epic 6 starts. Its "a review worker keeps its desk with the halo" predates
  v0.4's rule that every wait queues at the decision office (a package to
  review queues there like a pending approval, `architecture-v0.4.md`
  section 4), and it cites Epic 4 story numbers from before D-460 and
  D-498.
