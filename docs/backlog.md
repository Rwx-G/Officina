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

- Split `build_board` into one card builder per source; the two `Card`
  literals can drift.
- Split the collector loop (`watch::run`) into a state struct with one method
  per trigger, and test it with an injected clock instead of wall time.
- The floor's `no_excerpts` does not reach the hook, which only reads
  `OFFICINA_NO_EXCERPTS`; excerpts are still written, then hidden.
- Pin GitHub Actions to commit SHAs.

## Low

- `doctor::run` is long; hook detection is a substring match.
- The claude pipe readers are not bounded in time after a kill, and the kill
  reaches only the direct child.
- "Copy attach command" leaves out the floor's config dir.
- The zero-write test does not include a collector thread run.
