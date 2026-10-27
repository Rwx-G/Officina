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
- Story 1.4: verify whether `/clear` changes the `sessionId` reported for the
  same pid; if not, write `claude_pid` in the hook line (reserved in schema 1).

## Medium

- Pin GitHub Actions to commit SHAs.

## Low

- The kill of a timed-out `claude` reaches only the direct child: a process
  it started keeps running, and keeps the abandoned pipe reader threads
  blocked, until it exits. A Job Object around the child would end the whole
  tree (E13).
