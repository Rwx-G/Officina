---
title: Epic 1 QA report (v0.1, The Board)
author: Rwx-G
date: 2026-10-27
---

# Epic 1 QA report

## Executive summary

Gate decision: **pass with open items**. Slice A (usable alone) and most of
slice B are implemented and covered by automated tests; two stories keep a
manual check open (1.7 focus on real editor windows, 1.11 canary display),
and two are partly done (1.12 taskbar overlay, 1.13 release tag).

Top findings, all fixed in the epic:

1. A hook exiting 2 blocks Claude Code: usage errors now exit 64, and every
   path of `officina hook` exits 0 (story review).
2. The contract canary reported "held" on an empty sample and never re-ran:
   it now reruns on every non-empty list and stays unknown on an empty one.
3. One invalid byte dropped a whole day of events, and a line written after
   a partial line was lost with it: decoding is per line, appends close a
   partial line first.
4. `claude attach` started in the session folder could, through a
   `claude.cmd` shim, run a planted `node.cmd`: it now starts in the binary's
   folder with current-directory search disabled.
5. A blocked state reported next to a live `busy` status put a working
   session in Needs you: the live status is authoritative, with vector cases.

## Evidence

| Gate | Result |
|---|---|
| Rust unit, integration, CLI end to end, vector set, TS bindings drift | 79 passed |
| Hook latency, release binary, 200 KB payload | p95 21 ms (budget 30 ms) |
| Concurrent appends, 16 processes x 500 | 8000 lines, 0 corrupt |
| Frontend component tests (Vitest) | 11 passed |
| Playwright e2e on boards from the real CLI | 20 passed, stable over 3 runs |
| Zero write under the Claude config dir | fingerprint identical after every command |
| Real window on the workstation | starts, 35 MB, idle CPU 0.013 % of the machine over 30 s |
| clippy `-D warnings`, rustfmt, svelte-check, eslint | clean |

## Audits

- **Requirement challenge** (before stories): verdict "build smaller"; the
  blocking points (interactive idle is "your turn", stale notifications,
  unverified waiting values) were resolved by step 0 measurements and PRD
  amendments; the scope cut became a delivery order.
- **Story validation**: 11 ready with notes, 2 reworked in code (1.3 exit
  codes, 1.4 classification precedence).
- **Security audit**: 0 critical, 0 high, 2 medium (both fixed: attach
  folder, release token scope), 7 low, 5 info.
- **Quality audit**: 2 high (both fixed: canary, event loss), 9 medium
  (6 fixed, 3 deferred to the backlog), 9 low.

## Open items

See `docs/backlog.md`. None blocks daily use.
