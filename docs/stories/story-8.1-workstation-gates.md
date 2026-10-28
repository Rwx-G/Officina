# Story 8.1: Gates for production scope and worker reads, and the research notes they feed

**Status:** Approved

## Story

As the author about to let workers read servers through my own tree, and to decide whether production needs more than story 2.8,
I want the production writes of the measured session counted first, then Ansible's behaviour on the work floor, the worker's channel on Docker Desktop and the SSH agent under the relay's vector each checked by AD-166's procedures and recorded as an E-entry,
so that no v0.8 story rests on a guess about how many production writes there are, what Ansible prints, whether a container can reach the window without a port, or whose key a worker's read uses (FR-0).

## Acceptance Criteria

1. **Gate 0: the production count** (AD-166 row 0, PRD question 0 (a), FR-0; owner, agent-prepared; first, before any other v0.8 story). The agent writes a scratch script, never committed and run only on the work machine, that reads the measured session's Bash commands from the owner's own record of it and prints counts only: `ansible-playbook` runs (direct, through `wsl`, through `bash -c`), with or without `--check` or `-C`, their limit (`--limit`, `-l`, `--limit=`) matched against a lab pattern the owner passes as an argument, and their playbook as an opaque index; `ssh`, `scp` and `rsync` runs by host class, listed as indexes for the owner to mark state-changing by hand. The owner records: total commands; production writes; distinct playbooks and groups they touched; per guarded project, the write actions an action-pair catalogue would need. Over 10 in one project is recorded as Slice A's trigger (D-404). Proof: the gate 0 entry in `docs/research-notes.md`, counts only; the script's absence from the repository (`git status`).
2. **Gate 1 (a): the listing** (AD-166 row 1 (a), AD-153, PRD question 1, D-403; owner, agent-prepared). Per guarded project, through the relay's own vector (`wsl.exe [-d <distro>] --cd <project> --exec /usr/bin/env <env> <program> ...` from Git Bash with `MSYS_NO_PATHCONV=1`, or a scratch catalogue action run with `officina run`), stdout and stderr to separate scratch files outside every repository: `ansible <lab group> -i <inventory> --list-hosts`, an empty group and an unknown one, each with and without `ANSIBLE_NOCOLOR=1`. Recorded: exit codes, times, stdout's shape as a redacted sample (`hosts (N):` header, names as `host01`, `web01`), stderr's line prefixes, ESC byte counts per stream; and the verdict against AD-153's grammar (parsed or not), which decides whether the resolver is built (D-403). Proof: the gate 1 entry; the redacted samples are what 8.5 turns into `targets` cases.
3. **Gate 1 (b) to (d): outputs, check-mode exceptions, ignored paths** (AD-166 row 1 (b) to (d), PRD question 1, D-403; owner). (b) The size distribution and ESC counts of story 2.8's kept outputs of production checks, no new run. (c) `grep -rEc` of FR-7's four forms under each guarded project, ignored folders included: a count per form and a number of files. (d) `git -C <project> status --ignored --porcelain=v1` filtered to roles, collections, vault files, `host_vars` and `group_vars`; `ansible-galaxy collection list` and `ansible-galaxy role list` through the vector, each entry classed inside the tree and ignored, or outside it; `ansible-config dump --only-changed` through the vector, the names of changed settings and, for path settings, inside or outside the tree, never a value; the inventory static or dynamic, and the count of host entries setting `ansible_host`. No playbook runs and no host is contacted. The entry states whether ignored roles or collections exist under a guarded project, which decides 8.8 (D-403). Proof: the gate 1 entry, counts and classes only.
4. **Gate 2, first half: the channel** (AD-166 row 2, AD-157, D-402; agent). On Docker Desktop, from Git Bash with `MSYS_NO_PATHCONV=1`: a std-only scratch mailbox built for musl, in a probe image `FROM` the v0.6 base image when it exists, else a probe base; AD-104's network pair by hand; the mailbox started with `docker start --attach --interactive` by a scratch host program holding its standard streams and answering each request with a fixed result; a probe worker on the internal network sending 100 requests one per 2 s, timing each, then one 1 MiB result holding every byte value (SHA-256 compared); the host program killed and the mailbox re-attached with `docker attach --sig-proxy=false`, 10 more requests, recording whether any line the mailbox printed before the re-attach reappears on the re-attached stream (review F17; D-429 drops such lines either way); the host program stopped, 3 requests that time out within 10 s. Before and after: `Get-NetTCPConnection -State Listen`, every probe container's mounts (`docker inspect`). Pass: p95 under 2 s, byte-exact, re-attach works, the timeouts, no new listener, no bind mount; a replay is recorded, not a failure. On failure, AD-157's fallback is run the same way; both failing, Slice B is not built (P-72). Proof: the gate 2 entry (first half).
5. **Gate 3, first half: the agent under `--exec`** (AD-166 row 3, addendum section 1 items 3 and 4, D-402, PRD question 3 as amended by amendment 13; owner, agent-prepared). Every WSL terminal closed first (`wsl.exe --list --running` names no work distribution). Scratch read actions `agent-probe` (`/usr/bin/ssh-add -l`) and `config-probe` (`ansible --version`) with the `env` the real actions declare, run by `officina run` from a guarded session and from a plain console started from the Start menu. Recorded per path: exit codes, line counts, the SHA-256 of the `config file =` line, `WSL_DISTRO_NAME`. Then, keys loaded as the owner works (`ssh-add -c` if he uses it), a lab action running `ssh -o BatchMode=yes <lab host> true` from both: whether a confirmation appeared, where, whether the call waited, its time. When `ssh-add -l` fails, `agent-probe` runs again with the agent's socket named in its `env`, recording whether it then succeeds with every WSL terminal still closed, since naming a socket helps only when an agent process exists (review F40). The entry states which branch of D-403 holds (agent reachable; the actions' `env` naming the socket, only if that rerun succeeded; or question 7's dedicated key as a precondition of 8.11). Proof: the gate 3 entry (first half), with item 3 of the addendum's section 1 copied in as verified.
6. **Order** (FR-0, D-401). Gate 0 is recorded before any other v0.8 story starts; gate 1 before 8.3, 8.5 and any Slice A or B story; gate 2's and gate 3's first halves before 8.9. Proof: each entry's date against the order the Epic 8 block of the sprint status states.
7. **Nothing left behind** (D-401). Every probe container, network, volume and image carries `officina-probe=1` and is removed; `docker ps -a`, `docker network ls`, `docker volume ls` and `docker image ls` filtered on that label are empty before and after gate 2; scratch programs and outputs lie outside the repository; no work record, inventory name, host name, path, playbook name or key reaches a file of the repository; the working tree changes only in `docs/research-notes.md` and this decisions file's gate numbers. Proof: the gate 2 entry's listings; `git status` at the end.
8. **Recording rules** (D-401). Each entry says verified or documented, the date, the versions, who ran it, the procedure, what was seen, the outcome against AD-166's pass column and, on failure, which branch of D-403 now applies; `epic-8-decisions.md` gains each entry's number in the same change. Proof: review of `docs/research-notes.md` and `epic-8-decisions.md`.

## Tasks

- [ ] Gate 0 script (counts only) and the owner's checklist; the owner's run; the entry (AC 1)
- [ ] Gate 1 command lines for the owner; the entry (AC 2, 3)
- [ ] Gate 2's first half: scratch mailbox and host program, probe image and networks, timings, listings (AC 4)
- [ ] Gate 3's first half command lines and scratch actions; the owner's run; the entry (AC 5)
- [ ] Cleanup listings; numbers into `epic-8-decisions.md` (AC 6 to 8)

## Dev Notes

- Requirements: FR-0; PRD section 8 questions 0 (a), 1, 2, 3 as amended (amendments 10, 13). Decisions: AD-153, AD-157, AD-166. Facts: addendum section 1 items 1 to 4; E13 (WSL runner), E28 (musl scratch build), E30 (AD-104's layout by hand). Risks: R83, R86, R88, R89. Epic decisions: P-72, D-401 to D-404.
- Seams: none; no code changes. Gate 3 uses v0.2's `officina run` and catalogue as they are.
- What the agent may run and what it may not (D-401): it prepares command lines and scripts; it runs gate 2's first half and nothing at work. It never reads the measured session's record, an inventory, a key or an `ansible.cfg`.
- Gate 0's script reads the owner's own record of the session on his work machine and prints counts and indexes: a playbook or a host appears as an index the owner can resolve, never as text. Its output is what he types into the entry.
- Gate 1's listing verdict decides more than 8.5: with the listing unparsed, 8.3 lands no `targets` or `hosts` key and Slice B's worker dry runs are judged on their target value alone (D-403).
- Gate 2's scratch mailbox exists to answer one question before Slice B code is written: whether an attached container's stdin and stdout carry byte-exact frames on Docker Desktop and survive a re-attach. The real mailbox, the gateway's golden vector and the battery with the mailbox present close the gate in 8.12 (D-402).
- No employer content: projects are "a guarded project", groups `lab`, hosts `host01`, `web01`, `example.org`.

## Testing

- Manual, recorded in `docs/research-notes.md` (the gate 0 to 3 entries). No code changes in this story.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: gate 2's re-attach records any replayed line (F17, D-402, D-429); gate 3 reruns `ssh-add -l` with the socket named before choosing that branch (F40, D-403); AC 6's order is read from the Epic 8 block, kept in `sprint-status-epic8.yaml` until it is appended (F37, P-74)
