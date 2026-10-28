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
- Story 2.10 AC 10: with a guarded repository whose catalogue declares a WSL
  `ansible-playbook` check (`--check --diff`) and its `after-check` apply on
  a lab group, approved in the window and with doctor's `CLAUDE.md`
  paragraph pasted, ask a session to deliver a small change on the lab
  group and let it run check then apply through `officina run` without
  pasting anything (PRD journey 2, "Delegation" metric); record the Claude
  Code and WSL versions, the journal lines and any refusal in
  `docs/research-notes.md` (E13). The `--cd` part of D-6 is already
  verified there.
