---
title: Technical backlog
author: Rwx-G
---

# Technical backlog

Non-blocking items from reviews and audits. The feature roadmap is in the
README, not here.

## High

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
