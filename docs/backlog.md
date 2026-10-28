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

## Low

- The kill of a timed-out `claude` reaches only the direct child: a process
  it started keeps running, and keeps the abandoned pipe reader threads
  blocked, until it exits. A Job Object around the child would end the whole
  tree (E13).
