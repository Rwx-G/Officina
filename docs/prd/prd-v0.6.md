---
title: "PRD: Officina v0.6 - The Isolated Workshop"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions), with the amendments of ../architecture-v0.6.md section 4 applied; gates 0 and 7 open
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.1.md
  - prd/prd-v0.2.md
  - prd/prd-v0.3.md
  - prd/prd-v0.4.md
  - prd/prd-v0.5.md
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - architecture-v0.5.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - stories/epic-4-decisions.md
  - stories/epic-5-decisions.md
  - https://code.claude.com/docs/en/devcontainer
  - https://code.claude.com/docs/en/authentication
  - https://code.claude.com/docs/en/legal-and-compliance
  - https://code.claude.com/docs/en/network-config
  - https://code.claude.com/docs/en/sandboxing
  - https://code.claude.com/docs/en/sandbox-environments
  - https://code.claude.com/docs/en/permission-modes
  - https://code.claude.com/docs/en/headless
  - https://code.claude.com/docs/en/cli-reference
  - https://code.claude.com/docs/en/env-vars
  - https://code.claude.com/docs/en/memory
---

# PRD: Officina v0.6 - The Isolated Workshop

## 0. Document Purpose

Scopes v0.6, Epic 6. v0.1 to v0.5 hold. Framing principle 9 ("security comes
from the capabilities the workshop does not hold, not from deny lists")
becomes code. Until now every session, work orders included, runs as the
author, and the relay is a guardrail, not a boundary (v0.2 section 1). v0.6
adds the boundary: one Linux container per work order, holding a clone, the
tools and a model token, reaching the network only through a gateway. Its
value is twofold: it protects the runs the author clicks today, and it is the
precondition v0.7 needs before anything starts without him.

Principle 8 does not move: a workshop run starts only from v0.5's validated
work order and the author's click (v0.5 FR-3, FR-4), and the orchestrator
proposes workshop work orders through v0.5 FR-12 like any other. Principle 2
gets one reading for workers, stated in FR-7 with its strongest argument and
requested as a spine amendment so it is not reopened. The framing's exit
criteria are the targets: an escape battery played, every attempt refused and
journaled; the workspace's hooks run inside the workshop.

Revised after a requirement challenge: gates 7 (terms and account) and 0 (the
work network) come before any Slice A code, with an upstream proxy and a CA
file supported from the start; project images join Slice A; hook programs and
the battery's freshness become launch conditions; the boundary's limits are
stated; sensitive groups direct attention rather than claim a boundary;
merged detection moves to v0.7; candidate templates, a workshop share and
failure thresholds join the metrics.

Facts about Claude Code rest on `docs/research-notes.md` (E-numbers) or on the
documented pages listed at the head of section 4 (S-numbers). Facts about the
container engine and the network are not Claude Code's and no page here
covers them: each is a gate. What neither settles is in section 8 with the
check that settles it; the gating ones run first (FR-0).

## 1. Vision

The author wants to hand a routine change to an employee and get it back
done, without lending it his keys. v0.6 builds the room where that is safe.

A work order the author marks for the workshop runs in a Linux container of
its own. The container holds a clone of the base branch made from a git
bundle, the project's instructions and hooks mounted read-only, the tools of
an image the author approved, and one model token. It holds no SSH key, no
forge token, no vault, no host disk beyond those read-only folders, no engine
socket, no Windows interop. Its only network exit is a gateway that lets
through the hosts the author approved and journals every refusal. What the
container does not hold, no command gives it.

The work comes back as a review package: a bundle Officina verifies on the
host, the diff Officina computes itself, the worker's report, what it tried to
reach, and the paths that need a second look. The author imports it as a local
branch or discards it, one click each. Nothing is ever pushed.

**Where the boundary ends.** The workshop keeps the author's keys, forge,
vault and other repositories out of reach, and keeps the worker's changes off
his host until Import. It does not keep the cloned repository confidential:
anything the worker reads can leave through `api.anthropic.com`, which every
run must reach and which accepts any account's key, while TLS hides from the
gateway whose key a request carries. Do not give the workshop a repository you
would not paste into a model. After Import the branch is the author's code:
review it as a pull request from a stranger (FR-11). A container or kernel
escape lands in the engine's VM, not on a bare host (FR-18).

v0.6 launches nothing by itself. A workshop run starts from the same card and
click as any v0.5 work order.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Delegate without lending my identity.** A routine change on one of my
  repositories, free or guarded, runs start to finish in a container that
  holds nothing of mine but a revocable model token.
- **J2 - Review once, decide in one gesture.** I read what came back (report,
  commits, diff, what it tried to reach, what touched sensitive paths) and
  import it as a branch or discard it.
- **J3 - Keep my workspace's rules.** The project's CLAUDE.md, rules, skills
  and hooks apply inside the workshop as they do in my terminals, and a guard
  that cannot start stops the run instead of vanishing.
- **J4 - Stop everything at once.** One button or one command stops every
  worker, keeps their work for examination and leaves my own sessions alone.
- **J5 - Trust by evidence.** An escape battery played on my own machine, on
  the engine and images I actually launch with, shows the boundary holds, and
  what it refused is in the journal.

### 2.2 Key User Journeys

1. **Setup.** The author runs `claude setup-token` once in a terminal with a
   browser and stores the printed token with the `cmdkey` line doctor prints;
   Officina never shows a field for it. He gives `backend`, a Rust project, a
   `[project.workshop]` table: read-only mounts `.claude` and `_bmad`, hosts
   `index.crates.io` and `static.crates.io`, and `tools/workshop.Dockerfile`,
   which adds the Rust toolchain to `officina-workshop`. He gives the template
   `dependency-bump` a `[template.workshop]` table: base `main`, 60 minutes,
   USD 3. The catalogue banner shows the diff; one click approves it. In the
   Orchestrator view's Workshop section he clicks Build images: the project
   image is built and its inventory shows every hook program present. Then
   Run escape battery: every attempt refused.
2. **Launch.** The card for `dependency-bump`, `package = serde_json`,
   `level = minor`, now shows a workshop chip, base `main @ 3f2a1c0`, "2
   uncommitted changes are not carried", the allowed hosts, the mounts marked
   read-only, the project image, 60 minutes and USD 3. One click: the board
   shows the employee in Working with an "isolated" chip.
3. **Review.** Twenty minutes later Needs you shows "review:
   dependency-bump-4". Receive opens the package: the worker's last answer, two
   commits, the diff, connections to three allowed hosts and two refusals
   (`upload.example.com`), hooks ran on every event, and `Cargo.toml` changed
   (toolchain config, a sensitive group). The approval shows that file's
   hunks: one version line. The author approves it, then clicks Import: branch
   `officina/dependency-bump-4` appears in his repository, and he reviews and
   merges it in his editor.
4. **Stop.** A worker loops on a failing build. The author clicks Stop all:
   within 10 seconds every worker and gateway stops, their containers are
   kept, the relay stops too, and his editor sessions keep running. After a
   window restart the workshop still reads stopped until he clicks Resume.

### 2.3 Candidate templates (proposed, to confirm by the owner)

No v0.5 template has run yet, so the jobs v0.6 isolates are proposed here for
the owner's personal Rust, Python and Svelte projects. Each names its image,
its hosts and why the workshop beats the host for it. The Use metric counts
the ones he confirms.

1. **`dependency-bump`** (Rust or Python): bump one dependency from a closed
   list to its latest patch or minor release, run the project's build and
   tests, fix what breaks, commit. Image: the project's toolchain (`cargo`, or
   `uv`). Hosts: the package registry's. Why isolated: it runs third-party
   build code (build scripts, package hooks) fresh from a registry, which in
   the workshop never meets the author's keys.
2. **`lint-fix`** (Rust, Python or Svelte): run the project's lint and format
   gates, the commands its CLAUDE.md names, fix what they report, commit.
   Image: the toolchain and its linters. Hosts: the registry's. Why isolated:
   a long unattended edit-and-check loop that needs no identity, whose result
   is a branch to read.
3. **`missing-tests`** (any): write tests for one module from a closed list
   until the project's test command passes, without changing that module,
   commit. Image and hosts as above. Why isolated: same as 2, and the review
   panel shows at once any change outside the tests.

## 3. Glossary

- **Workshop** - the container engine, the workshop images and the runs: the
  framing's isolated workshop.
- **Engine** - Docker Desktop on the workstation, driven only through its
  `docker` CLI (FR-1).
- **Worker** - the `claude` process running in one container for one work
  order, holding the author's model token and nothing else of his.
- **Run** - one work order executed in the workshop: one container, one
  internal network, one gateway, one review package.
- **Gateway** - Officina's own HTTPS proxy, one per run, the worker's only
  network exit (FR-4).
- **Allow-list** - the hosts a run's gateway lets through: the fixed Claude
  Code set plus the project's approved hosts.
- **Project image** - an image built from the project's own Dockerfile on top
  of `officina-workshop`, carrying its toolchain (FR-1).
- **Inventory** - the record, per image, of which programs named by the
  project's hooks and `.mcp.json` the image provides (FR-1).
- **Base bundle** - a `git bundle` of the base branch at its tip, written on
  the host and copied into the container.
- **Review package** - what a run hands back: a bundle of the work branch,
  the worker's report, and Officina's own facts about the run.
- **Quarantine** - a package Officina refuses to offer for import (FR-10),
  kept for examination.
- **Sensitive group** - a set of paths whose change needs its own approval
  before import (FR-11); it directs attention, it is not a boundary.
- **Import** - fetching a verified package into the project repository as a
  new local branch; never a merge, never a push.
- **Memory proposal** - a change the worker made to its copy of the project's
  auto memory, shown for the author to paste (FR-13).
- **Escape battery** - a fixed set of attempts to leave the workshop, played
  with the worker's own container vector on each image (FR-15).

## 4. Features

Documented surfaces this PRD rests on, beyond E1 to E17 and v0.5's S1 to S7.
Each becomes an E-entry once the check of section 8 that uses it has run.
Pages are under `https://code.claude.com/docs/en/`. Numbers are local to this
PRD; v0.5's are cited as "v0.5 S4".

- **S1** (`devcontainer`) Claude Code runs inside a container with commands
  confined to it. "Because the container runs Claude Code as a non-root user
  and confines command execution to the container, you can pass
  `--dangerously-skip-permissions` for unattended operation. The CLI rejects
  this flag when launched as root." With that flag a container does "not
  prevent a malicious project from exfiltrating anything accessible inside
  the container, including the Claude Code credentials". Avoid mounting host
  secrets such as `~/.ssh`. Claude Code auto-updates inside a container by
  default; `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` and
  `DISABLE_AUTOUPDATER=1` turn telemetry and updates off. A firewall inside the
  container needs the `NET_ADMIN` and `NET_RAW` capabilities.
- **S2** (`authentication`, `cli-reference`) `claude setup-token` opens the
  browser flow, prints a one-year OAuth token and saves it nowhere; it is
  consumed through `CLAUDE_CODE_OAUTH_TOKEN`, meant "for CI pipelines,
  scripts, or other environments where interactive browser login isn't
  available", requires a Pro, Max, Team or Enterprise plan, and "can only make
  model requests" (no Remote Control, no claude.ai connectors). Precedence
  puts `ANTHROPIC_API_KEY` above `CLAUDE_CODE_OAUTH_TOKEN`, and in print mode a
  present API key is always used. On Linux, credentials live in
  `<config dir>/.credentials.json`. An unattended session that outlives its
  login stops making progress. Bare mode does not read
  `CLAUDE_CODE_OAUTH_TOKEN`.
- **S3** (`legal-and-compliance`) Free, Pro and Max use falls under the
  Consumer Terms; Team, Enterprise and API use under the Commercial Terms.
  "OAuth authentication is intended exclusively for purchasers of Claude
  Free, Pro, Max, Team, and Enterprise subscription plans and is designed to
  support ordinary use of Claude Code". Developers building products "should
  use API key authentication" and "may not collect, store, or intermediate
  Claude.ai credentials or session tokens". This does not "prevent an end user
  from signing in to the unmodified Claude Code binary with their own Claude
  subscription, including where a platform hosts Claude Code". Advertised
  limits "assume ordinary, individual usage".
- **S4** (`network-config`) Claude Code honours `HTTPS_PROXY` (and lowercase
  variants); SOCKS is not supported; a proxy URL it cannot parse stops launch.
  It trusts its bundled Mozilla certificates and the operating system's store
  (the native build reads it), so a TLS-inspecting proxy works when its root
  certificate is in the system store; `NODE_EXTRA_CA_CERTS` adds a file.
  Required hosts: `api.anthropic.com` (API requests, feature flags, telemetry
  events, the WebFetch safety check), `claude.ai`, `platform.claude.com` (OAuth
  token exchange, refresh and revocation), `downloads.claude.ai` (updates);
  two Datadog hosts carry optional telemetry, turned off by
  `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`.
- **S5** (`sandboxing`, `sandbox-environments`) The built-in sandbox runs on
  macOS, Linux and WSL2; "On native Windows, Claude Code runs commands
  unsandboxed." It covers shell commands only; hooks and MCP servers run
  outside it. Containers and VMs put the whole process inside the boundary.
  "Any approach that allows network egress can still leak data the agent can
  read." "Always run `--dangerously-skip-permissions` sessions inside a
  container, a VM, or the sandbox runtime"; on Linux Claude Code refuses that
  flag as root. Auto mode's classifier "is a per-action control, not an
  isolation boundary".
- **S6** (`permission-modes`) `bypassPermissions` is for "Isolated containers
  and VMs only". "Deny rules block in every mode, including
  `bypassPermissions`." `bypassPermissions` set in `.claude/settings.json` or
  `.claude/settings.local.json` does not take effect; a flag enables it. "In
  non-interactive mode no dialog is shown." `AskUserQuestion` is never
  auto-approved. The mode "offers no protection against prompt injection".
- **S7** (`headless`, `cli-reference`) Without `--bare`, a `-p` run loads
  project hooks from `.claude/settings.json` and connects `.mcp.json` servers,
  "even in a folder you've never trusted", with no trust dialog; `--bare`
  skips hooks, skills and CLAUDE.md and reads no OAuth credential. In
  `stream-json` output, `system/init` reports model, tools and MCP servers;
  `system/api_retry` carries an `error` category (`authentication_failed`,
  `rate_limit`, others); the last line is a `result` with cost and session
  metadata, the cost a client-side estimate. `--include-hook-events` adds hook
  lifecycle events (`SessionEnd` and `Notification` produce no
  `hook_started`). `--max-budget-usd` stops a print-mode run at an estimated
  spend that "can pass the cap". `--permission-prompts none` (2.1.259+)
  denies what would prompt and removes `AskUserQuestion`. Piped stdin is
  capped at 10 MB. `-p` rejects `--bg`.
- **S8** (`env-vars`, `memory`) `CLAUDE_CODE_OAUTH_TOKEN` is an "Alternative
  to `/login` for SDK and automated environments".
  `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` turns off auto-updates,
  telemetry, error reporting and feature-flag fetching.
  `CLAUDE_CODE_DISABLE_ARTIFACT=1` turns off the tool that publishes session
  output to claude.ai. CLAUDE.md files in the directories above the working
  directory load at launch. Auto memory lives in
  `<config dir>/projects/<project>/memory/`, the name "derived from the git
  repository"; `CLAUDE_CODE_PROJECT_DIR_NAME` beside `CLAUDE_CONFIG_DIR` fixes
  that name (2.1.234+).

### 4.0 Slice 0: checks

#### FR-0: Gates first
The section 8 questions marked **gate** are run on the workstation and
recorded as E-entries. Gate 7 and gate 0 open Slice 0, because they decide
whether the workshop can be used at all. Stories may be written before the
gates, but no Slice A story is implemented before gates 0 and 7 are recorded,
and none that a gate names before that gate is. If gate 7 says no and no API
key is available, v0.6 stops at Slice 0. A failed gate changes the FR it
names first, as E12 reshaped v0.2's approvals.

### 4.1 The workshop

#### FR-1: Engine and images
Officina targets Docker Desktop with Linux containers (WSL 2 backend), which
is installed on the workstation, and drives it only through `docker.exe` with
argument vectors built in core and golden-tested like AD-79's. `docker.exe`
is resolved from Docker Desktop's install folder, then `PATH`, `.exe` only (a
shim is refused, as AD-51), or from `OFFICINA_DOCKER_BIN` for tests. Officina
owns no worker process: containers run detached, the engine holds them, and a
window restart finds them again by label (principle 1).

Podman, which the framing recommended, is not installed; Docker Desktop is.
Gives up: a rootless, daemonless engine by default, and an engine free of
subscription terms in every context. Docker Desktop's own terms apply where
it runs (gate 7); Officina ships no engine, and the README says "The workshop
needs a Docker-compatible engine; Docker Desktop's subscription terms apply
where it runs." FR-3 recovers what rootless mattered for through what the
worker holds: a non-root user, no capability, no engine socket. Officina calls
a small subcommand set (`version`, `build`, `image inspect`, `network create`
and `rm`, `create`, `cp`, `start`, `logs`, `stop`, `rm`, `ps`) that Podman's
and Rancher Desktop's compatible CLIs also have, so either can be tried later.

Images are built locally on the author's click (Build images), never during a
launch, never pulled ready-made. Each carries labels: its Dockerfile's
SHA-256, the CA file's when set, the Claude Code version and Officina's
version. A launch refuses an image whose labels do not match the running
Officina and the approved catalogue.
- `officina-workshop`: a Debian slim base pinned by digest, a non-root user
  `worker`, git, Python 3 with `python` provided as `python3`, uv, bash,
  coreutils, CA certificates, and the Claude Code native build at the version
  the host runs, unmodified (S3), its auto-update off (S1, S8). Its Dockerfile
  is compiled into Officina.
- `officina-gateway`: `officina-gateway`, a static Linux program of the crate
  `officina-container`, which also builds `officina-entry`, the workshop
  image's entry point and battery: `cargo build --release -p
  officina-container --target x86_64-unknown-linux-musl`. A crate of its own
  because `officina` links SQLite's C sources, which a musl build cannot
  compile without a musl C toolchain. On an empty base, linked with Rust's
  self-contained `rust-lld`, on the workstation during development and in the
  release job, which ships the binary beside `officina.exe`; Build images
  copies it into the gateway image's context. No Rust image is pulled (gate 9).
- **Project images** (Slice A): `[project.workshop] dockerfile` names an
  author-written Dockerfile whose first stage is `FROM officina-workshop` (or,
  in Slice B, `FROM officina-workshop-browser`) and which adds the project's
  toolchain: a Rust toolchain, pnpm, Node. Its SHA-256 and its build
  context's are in the canonical catalogue, so a change voids the approval.
  It cannot change what FR-3 fixes: the vector sets the user, the entry point
  and every security property at run time.
- **CA file**: when `[workshop] ca_file` is set, every image installs that
  certificate in its system store, which Claude Code, git, curl and pip read
  (S4). Without it the images are as above.
- Slice B: the browser base (FR-16).

Each build ends with an inventory run of the new image, with no network: it
records which programs named by the project's mounted hooks and `.mcp.json`
the image provides on `PATH`. At launch, if those programs changed since, the
inventory runs again first (FR-6).

A build runs with the engine's default network: it builds author-approved
text, it is not a run, and the gateway does not cover it. A project
Dockerfile's steps fetch what they fetch at the author's risk.

Images are tagged `:local`. A project Dockerfile's final stage descends from
`officina-workshop:local` or `officina-workshop-browser:local`, checked before
the build and by the inherited `officina.base` label after it; its build
context is the Dockerfile's own folder. The base also carries `curl`, strips
setuid and setgid bits and points Python, Node and uv at the system
certificate store. A launch compares every label but the Claude Code version,
which doctor compares.

#### FR-2: Floor keys
Schema 2 gains keys; the floor schema number does not change (AD-75's
reasoning). Every key below belongs to the canonical catalogue: a new or
changed key is usable only after the author approves the catalogue, and the
canonical format goes up once for v0.6 (AD-76, D-131).

```toml
[workshop]
auth = "oauth-token"                  # "oauth-token" (default) | "api-key"
max_workers = 2                       # 1..=4
cpus = 2                              # per worker
memory_mib = 4096                     # per worker
upstream_proxy = "proxy.example.com:8080"   # optional (FR-4)
ca_file = "C:/path/to/corporate-ca.pem"     # optional (FR-1)

[[project]]
name = "backend"
path = "Backend"

[project.workshop]
mounts = [".claude", "_bmad"]         # git-ignored folders, mounted read-only
hosts = ["index.crates.io", "static.crates.io"]
dockerfile = "tools/workshop.Dockerfile"    # optional (FR-1)
review = "groups"                     # "groups" (default) | "all" (FR-11)
sensitive_paths = ["deploy/**"]       # added to FR-11's fixed groups
memory_name = "<folder name under projects/>"   # Slice B, optional (FR-13)

[[template]]
name = "dependency-bump"
# v0.5 keys unchanged

[template.workshop]
base = ["main"]                       # closed list; the first is the default
max_minutes = 60                      # 5..=240, default 60
max_usd = 3.0                         # optional
```

Phase 2 rules (AD-23), each an issue kind with a failing fixture:
- `upstream_proxy`: `host:port`, a DNS name or an IPv4 address, no scheme, no
  credentials.
- `ca_file`: an absolute path to a PEM file of at most 64 KiB holding
  certificates only.
- `mounts`: relative to the project folder, no `.` or `..` segment, at most 8;
  existence and ignore status are checked at launch and by doctor
  (`mount_missing`, `mount_not_ignored`). Doctor names a mount ignored only through untracked rules
  (`.git/info/exclude`, `core.excludesFile`), which a clone does not carry
  (FR-6 handles it).
- `hosts`: lowercase DNS names, at most 32, no IP literal, no port, no
  wildcard but a leading `*.` on a name of at least three labels.
- `dockerfile`: a path inside the project, no `..` segment, whose first
  `FROM` names an Officina base image.
- `review`: `groups` or `all`. `sensitive_paths`: glob patterns, at most 32.
- `memory_name`: one path segment.
- `base`: at most 8 branch names passing git's branch-name rules, checked in
  core.
- A template with `[template.workshop]` runs only in the workshop; one without
  runs on the host as in v0.5. A workshop template naming a project without
  `[project.workshop]` is an issue.
- Without a valid `[workshop]` table the workshop is off and every workshop
  template is dropped with an issue.

A workshop project's name follows the action name rule. A host's last label
holds a letter. `max_usd` has at most two decimals. A Dockerfile's folder
holds at most 32 files and 1 MiB.

#### FR-3: What the worker holds
One container per run, created from a vector core builds from the validated
work order (golden test).

| The worker holds | The worker never holds |
|---|---|
| a clone of the base branch, made in the container from the base bundle (FR-6) | the author's SSH key or agent, forge token, git credentials, vault, cloud credentials |
| the project's declared folders, read-only (FR-6) | any other host path: the Officina home, the Claude config dir, a writable host path of any kind |
| the image's tools | the engine's socket or API; Windows interop |
| one model credential (FR-5) | any other variable of the window's environment |
| the internal network of its run, whose only peer is its gateway | a route out; DNS for outside names; the host; other runs |
| a work volume and size-bounded temporary folders, removed with the container | root, a capability, a setuid program that works, new privileges |

The vector fixes: user `worker` (uid 1000); the entry point; every capability
dropped; `no-new-privileges`; the engine's default seccomp profile; a
read-only root file system; CPU, memory and process-count limits from
`[workshop]`; an init process; no privileged mode, host network, device,
engine socket, or host IPC or PID namespace; labels naming the floor, the run
and the work order; an environment of FR-7's names, the credential's name
and the approved image's own `ENV` names. Nothing of the window's environment is inherited.

S1 states what this does not prevent, and section 1 says where the boundary
ends.

#### FR-4: The gateway
Each run gets its own internal network, with no route out, and its own gateway
container attached to that network and to a bridge network of its own run.
Runs cannot reach each other. The gateway is `officina-gateway` (FR-1), an
HTTP proxy that accepts only `CONNECT <host>:443` and:
- refuses a host off the run's allow-list, an IP literal, any port but 443,
  any method but `CONNECT`, a malformed or oversized request (8 KiB);
- resolves the host itself and refuses a loopback, private, link-local,
  multicast or unspecified address, so no name leads back to the host or the
  LAN;
- reads the TLS ClientHello and refuses when its server name differs from the
  CONNECT host; it neither terminates nor inspects TLS;
- refuses everything after the run's deadline, and everything after 20
  refusals within 5 minutes, then exits non-zero (FR-8);
- writes to its stdout one JSON line per refusal (the requested host cut to
  253 characters, port, cause) and one line per allowed connection, which the
  window counts per host into `gateway_summary`. The window reads them through the engine and journals
  `gateway_refused` and `gateway_summary`. The gateway holds no credential and
  writes no file.

**Upstream proxy.** With `[workshop] upstream_proxy` set, the gateway sends
its own `CONNECT <host>:443` to that proxy instead of connecting directly, and
relays the bytes. It stays CONNECT-only and keeps every check above but two:
name resolution and the private-address refusal become the upstream proxy's,
since the gateway does not see the address the proxy picks. A TLS-inspecting
proxy also needs `ca_file` (FR-1). A floor without `upstream_proxy` behaves
as described above. Gate 0 tells which applies at work.

The allow-list is the fixed Claude Code set plus the project's approved
`hosts`. The fixed set starts as `api.anthropic.com`, the host S4 names for
model requests; gate 3 records every host a real worker asks for and decides
whether `platform.claude.com` (token refresh, S4) joins. Updates and
telemetry are off in the worker (S8); if attempted, they are refused like
anything else.

The worker reaches the gateway through `HTTPS_PROXY` (S4). A program that
ignores the variable has no route at all: it fails closed. Gives up: hosts
reached by plain HTTP, SSH or UDP; per-URL filtering; any view of what goes to
an allowed host, so `api.anthropic.com` carries whatever the worker sends
(section 1); fronting inside TLS (a Host header that differs behind a
matching server name); and, with an Encrypted ClientHello, the inner name,
since the gateway sees only the outer one. An allowed host that accepts
anonymous uploads is a leak the allow-list cannot close; the README says so
beside `hosts`.

The gateway is Officina code rather than an off-the-shelf proxy: CONNECT-only
filtering and chaining are small, write the journal's format, and need no new
crate, while a general proxy brings a configuration language whose defaults
would become the boundary. Gives up: the scrutiny a widely deployed proxy has
had; the gateway gets its own vector set and the battery's gateway family
(FR-15).

A name with any non-public address is refused. An Encrypted ClientHello is
recorded on the allowed line, not refused. The gateway's first line says
whether the upstream proxy answered.

#### FR-5: Authentication
The worker authenticates with one credential the author created and stored
himself in the Windows Credential Manager under `officina:workshop` (doctor
prints the `cmdkey` line with a placeholder, AD-36's pattern):
- `oauth-token` (default): a `claude setup-token` token, consumed through
  `CLAUDE_CODE_OAUTH_TOKEN`; it "can only make model requests" (S2, E6);
- `api-key`: a Console key through `ANTHROPIC_API_KEY`, billed per use (S2,
  S3).

The window reads the credential at launch, sets it in the engine CLI's
environment and passes only its name to the container, so the value sits on
no command line. Officina never writes it to a file, the journal, the
registry or a log, never displays it, and never asks for it in the window. The
worker gets exactly one of the two names, since S2's precedence would let an
API key win over the token. `--bare` is never used: it reads no OAuth token
and skips hooks and CLAUDE.md (S7).

A process running as the author can read the value back from the engine or
the Credential Manager: the workshop protects the host from the worker, not
the worker from the author's own sessions (v0.2 section 1).

Terms (gate 7). S3 reserves OAuth to "ordinary use of Claude Code" by plan
purchasers, forbids developers to "collect, store, or intermediate Claude.ai
credentials", and does not prevent "an end user from signing in to the
unmodified Claude Code binary with their own Claude subscription", including
where a platform hosts it. Officina runs the unmodified binary for its own
author on his machine, stores no token itself, and in v0.6 launches only on
his click. Whether that is ordinary use under the plan of the account the
work floor uses, and, for an employer plan, whether the employer accepts it,
is the owner's to record before any Slice A story is implemented. `api-key` is
the fallback S3 permits for scripted use without that question.

### 4.2 A run

#### FR-6: Launch and work in
A workshop template issues a card and launches on a click exactly as v0.5
FR-4: the same `WorkOrder::validate`, card id, prompt hash and pointer-only
Launch. The card adds a workshop chip; the base, chosen from the template's
list, with the commit its tip points to; the count of uncommitted changes on
the host that will not be carried; the allow-list; the mounts, marked
read-only; the image; the deadline and budget. The base list also offers the
imported `officina/<name>` branches of the same template from the last 7
days, so a follow-up starts from the previous run's result; the card then
shows the parent run.

Launch is refused, with the cause on the card, when:
- the workshop is off (`workshop_off`, FR-2);
- the workshop stop flag is set (`workshop_stopped`, FR-14);
- the engine does not answer or runs Windows containers
  (`engine_unavailable`);
- an image is missing or its labels do not match (`image_mismatch`, FR-1);
- the last passing escape battery for this engine version, image labels and
  Officina version is missing, differs, or is older than 30 days
  (`battery_stale`); the card then offers Run escape battery (FR-15);
- a mounted `PreToolUse` hook's program is absent from the image's inventory
  (`hook_program_missing`): under bypass, a guard that cannot start is a guard
  that is gone;
- `max_workers` runs are live (`workshop_full`); 10 packages await a verdict
  (`reviews_pending`);
- a mount is missing (`mount_missing`), not ignored (`mount_not_ignored`), or
  on a path the engine's mount syntax cannot carry (`mount_path_unsupported`);
- the credential is absent (`credential_missing`);
- the project folder is missing (`project_missing`);
- the base branch moved since the card was issued (`base_moved`; a new card
  shows the new tip);
- the run's plan differs from the card's (`plan_changed`).

Then, each step's outcome in one `workshop_started` journal line:
1. The window writes the base bundle (`git bundle create` of the base branch,
   FR-10's git rule) and the rendered prompt into
   `<officina home>/workshop/runs/<id>/in/`; a base bundle over 1 GiB ends the
   launch (`base_too_large`).
2. The engine creates the run's network, its gateway and the worker's
   container (FR-3), copies the bundle and the prompt in, and starts the
   gateway, then the worker.
3. The image's entry point, fixed code, clones the bundle into
   `/work/<project>`, creates branch `officina/<work order name>` at the base
   commit, appends every mount path to the clone's `.git/info/exclude`, links
   each mount at its place in the clone, and runs the worker (FR-7).

Mounts are read-only bind mounts of the host folders: the worker reads the
author's current `.claude/` and cannot change it. Nothing else of the host is
visible. Only committed history travels; uncommitted changes and ignored files
outside the mounts do not.

Commits carry the author's name and email as the project repository's git
configuration gives them (read with FR-10's git rule), because the author
answers for what he imports. Gives up: the commits do not say a worker wrote
them; the branch name and the registry do.

Launch sends the card's plan hash with its prompt hash. Officina's own folders
sit under `/work/.officina/`.

#### FR-7: The worker
The entry point runs `claude` with a vector fixed in core (golden test), the
prompt on stdin (E16; S7 caps stdin at 10 MB, v0.5 FR-2 the prompt at 8,000
characters):
- `-p --output-format stream-json --verbose --include-hook-events` (S7);
- `--permission-mode bypassPermissions --permission-prompts none` (S6, S7);
- `--disallowedTools WebSearch` (below);
- `--max-budget-usd <max_usd>` when the template sets it (S7; gate 6);
- `--model <alias>` when the template names one (v0.5 FR-2).

Environment, exactly: `HOME` on the work volume; `PATH` from the image;
`CLAUDE_CONFIG_DIR`, a fresh folder in the container (E7); `HTTPS_PROXY` and
`https_proxy`, `HTTP_PROXY` and `http_proxy`, and `NO_PROXY` and `no_proxy`
set to `localhost,127.0.0.1,::1`; `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`
(updates, telemetry and error reporting off, S8); `DISABLE_AUTOUPDATER=1`
(S1); `CLAUDE_CODE_DISABLE_ARTIFACT=1` (no
publishing to claude.ai, S8); `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` (no
background session outliving the run, v0.5 S7); in Slice B
`CLAUDE_CODE_PROJECT_DIR_NAME` (FR-13); the commit identity; the credential
(FR-5).

**Permission mode.** Principle 2 forbids Officina to set a permission mode on
the sessions that do the author's work, so that Officina never widens what
the author's identity does unprompted. The strongest argument for this
reading: a project's settings cannot enable bypass (S6), and the author's free
repositories already run in bypass from his user settings (v0.2 section 1),
so a worker widens nothing relative to his own posture, while holding far
less. What a worker may do is what the container holds; its mode decides
nothing FR-3 does not already bound, and an unattended run has nobody to
answer a prompt. The vector therefore passes `bypassPermissions`, which S6
reserves for isolated containers and S1 and S5 pair with a container and a
non-root user. Print mode shows no dialog (S6). Deny rules still block in
every mode (S6), and so does a `PreToolUse` hook's deny (E12), which is why a
hook that cannot start stops the run (FR-6, FR-8). Gives up: per-call
prompts. Auto mode was set aside: its classifier is a per-action control, not
an isolation boundary (S5), and the boundary is what v0.6 claims. Decided on
2026-10-28 under the owner's delegation of design decisions, listed for his
confirmation, and requested as an amendment to the architecture spine's
reading of principle 2.

WebSearch is disallowed because the pages read do not say whether its
requests leave from Anthropic's side, outside the gateway (section 8,
question 11). WebFetch stays: it fetches through the gateway like any other
tool, and its safety check calls `api.anthropic.com` (S4).

#### FR-8: Follow and limits
The window reads the worker's stdout through the engine (`docker logs`,
incrementally) and parses its stream-json lines (S7), at most 1 MiB per line
and 64 MiB per run. It keeps only:
- `system/init`: model, tools, MCP servers. A tool the vector disallowed
  present in the list stops the run (`unexpected_tool`);
- hook lifecycle events, counted per event and outcome (FR-9). A hook that
  exits 126 or 127, its program not executable or not found, stops the run
  (`hook_failed`). Gate 5 confirms the events carry the exit code; if they do
  not, FR-6's `hook_program_missing` is the only check and this line is
  amended;
- `system/api_retry`: its `error` category; three `authentication_failed` in a
  row stop the run (`auth_failed`);
- `result`: subtype, error flag, cost estimate, turns, duration, session id;
- the last 4 KiB of the worker's text, as `Untrusted<String>` (AD-74),
  display only.

Limits, each a cause on the card and in the `workshop_finished` journal line:
- deadline: the entry point runs the worker under `timeout`, the gateway
  refuses everything after the deadline, and the window stops the container
  at the deadline plus 60 s; any one of the three suffices;
- budget: Claude Code's own cap (FR-7);
- refusals: the gateway closes (FR-4), the worker's next model request fails
  and it ends;
- output: past 64 MiB the window stops the run (`output_cap`);
- CPU, memory, processes: the engine's limits (FR-3).

The board shows the run in Working while its container runs. The window may
close: the run goes on under the engine, and at the next start the window
finds it by label, reads what it missed and applies the deadline. A container
the engine no longer knows, with no package, ends `unknown`, never finished
(principle 3).

Stream lines are untrusted: only the first `system/init` counts and a stream
fact can stop a run, never keep it going. Causes add `memory` and
`engine_lost`. A restarted window re-reads a live container's output from the
start. An engine that does not answer makes no run unknown.

#### FR-9: The workspace applies inside
Tracked instructions (CLAUDE.md files, `.mcp.json`) come with the clone; the
mounted folders bring the rest (`.claude/` with its CLAUDE.md, rules, skills,
agents, commands, settings and hooks). Claude Code loads them as in any
print-mode run in that folder: project hooks run and `.mcp.json` servers
connect, with no trust dialog (S7). Officina adds no setting file, hook,
CLAUDE.md or MCP server; its only additions are FR-7's flags and variables.

What the mounts cannot carry:
- instructions above the project folder: a CLAUDE.md in a parent directory,
  which Claude Code loads at launch on the host (S8), or a workspace-level
  `.claude/` there, since mounts never leave the project (FR-2);
- the author's user-level configuration (user CLAUDE.md, skills, hooks,
  settings, MCP servers), which sits beside his credentials (S2);
- `officina hook` and the relay gate: the worker has no relay;
- anything neither tracked nor in a declared mount;
- Windows behaviour: a hook written and tested on Windows runs on Linux here,
  and one that branches on the operating system takes its other branch.

Gate 4 records, for one real session at work, where each instruction file and
hook comes from (`/memory`, `/hooks`), and the README lists what the workshop
leaves out. A hook or skill that writes into a mounted folder fails there: the
worker cannot rewrite its own instructions, by design.

The exit criterion is read from the stream: `--include-hook-events` reports
hook lifecycle events (S7), and the review panel shows, per event, hooks run
and failed. Gate 4 checks with a logging hook the events the stream does not
report.

### 4.3 Review

#### FR-10: The review package
When the worker ends, by exit or stop, a packer container, from the run's
image and volume with no network and no credential, empties the out folder
and writes the work branch's tip and, when the branch holds commits past the
base, its bundle (`officina/<name>` from the base commit), and a manifest.
The window copies the out folder out of the stopped packer (`docker cp`),
64 MiB at most, into `<officina home>/workshop/runs/<id>/out/`.

**Git rule.** Host `git` is resolved as `docker.exe` is (install folder, then
`PATH`, `.exe` only), and every v0.6 call runs with D-22's flags,
`transfer.fsckObjects`, `core.hooksPath` set to an empty directory and
`protocol.file.allow` limited to the scratch repository; also `core.hooksPath`
empty, `fetch.fsckObjects`, `protocol.allow=never` with
`protocol.file.allow=always`, `gc.auto=0`. This closes story 2.6's note that
`git` came from `PATH`.

Nothing written in the container is believed: the worker ran as the same user
as the entry point and could have replaced either file. On the host, the
window:
1. verifies the bundle in a scratch repository under the run folder that
   borrows the project's objects read-only. Its only prerequisite is the
   recorded base commit; it carries exactly one ref,
   `refs/heads/officina/<name>`, whose history from the base holds at most 500
   commits;
2. computes the commits and the diff itself in that scratch repository
   (AD-84's caps and jumps for display; the file list is never capped);
3. scans every added or changed blob, every commit message, the report tail
   and the memory proposals for the credential's value, raw and in base64 and
   hex. The scan catches careless leaks only: a worker steered to leak can
   send anything through `api.anthropic.com` (section 1);
4. records the package in the registry and journals `review_package` (bundle
   SHA-256, counts, sensitive groups, outcome).

A failed check quarantines the package with its cause (`bundle_invalid`,
`branch_missing`, `wrong_base`, `extra_ref`, `too_large`, `credential_found`,
and
`mount_committed` when it adds a path under a mount): shown with its cause, it
offers Discard only. With `credential_found` the card also tells the author to
revoke the token (section 8, question 10), and the value is replaced by a
placeholder in every text Officina stores. Before the author's click nothing
is written to the project repository.

A package waits on the author, so it is a Needs you card ("review", amber,
counted in `waiting_count`). Receive opens the review panel in AD-84's side
slot: the report (the worker's last text, untrusted); the run's facts
(outcome, cause, duration, cost estimate, turns, hooks run and failed per
event, allowed connections per host, refusals with causes); the commits; the
diff; the sensitive groups (FR-11); memory proposals (FR-13).

A bundle header with any prerequisite but the base, any reference but the
work branch or a `@filter` capability is refused; blobs inflate to at most
512 MiB; `credential_found` deletes the package at once. **No change:** when
the worker exited 0 and host git, on objects the host holds, counts no commit
past the base, the run ends `no_change`: journaled in `workshop_finished`,
never a package, never quarantined, never a Needs you card, nothing to review,
import or discard. With another ending and no commit, the run is `failed`
with its cause. Neither model text nor the packer's files decide it.

#### FR-11: Sensitive paths
Sensitive groups direct attention; they are not the boundary. The boundary
ends at Import: review an imported branch as a pull request from a stranger.

A package that touches a sensitive path needs one approval click per group
before Import is enabled. The approval shows that group's hunks inline, not
only its name. Patterns match every added, changed, deleted or renamed path,
at any depth, case-insensitively:
- Claude Code configuration: `.claude/**`, `CLAUDE.md`, `CLAUDE.local.md`,
  `AGENTS.md`, `.mcp.json`;
- git: `.git*` (`.gitattributes`, `.gitignore`, `.gitmodules` and the
  others), submodule entries;
- CI: `.github/**`, `.gitlab-ci.yml`, `.gitlab/**`, `azure-pipelines.yml`,
  `Jenkinsfile`;
- build and package scripts: `build.rs`, `package.json`,
  `pnpm-workspace.yaml`, `.npmrc`, `pyproject.toml`, `setup.py`, `setup.cfg`,
  `Makefile`, `justfile`, `Dockerfile*`;
- toolchain config: `.cargo/config.toml`, `.cargo/config`, `Cargo.toml`,
  `conftest.py`, `tox.ini`, `noxfile.py`, `requirements*.txt`, `ansible.cfg`,
  and ansible's `library/**`, `filter_plugins/**`, `callback_plugins/**`;
- editor and shell: `.vscode/**`, `.envrc`, `.pre-commit-config.yaml`,
  `.husky/**`;
- Officina: the floor file, when it lies in the project;
- file modes: a symlink, an executable bit added;
- the project's own `sensitive_paths`.

`[project.workshop] review = "all"` makes every changed file sensitive, one
approval per file, for infrastructure repositories where every line runs on a
server with the author's key. The groups live in core with a vector case per
pattern. An approval holds for one package and is journaled
(`sensitive_approved`). After 10 verdicts on a template, if over half its
packages needed the same group, the Orchestrator view says so; the author
keeps the group or narrows the template.

An approval binds to the SHA-256 of the hunks displayed. A group whose hunks
exceed 1 MiB cannot be approved in the panel.

#### FR-12: Import or discard
- **Import** (pointer click only, D-28) fetches the verified package from the
  scratch repository into the project repository as the new local branch
  `officina/<name>`, under FR-10's git rule. It refuses when that branch
  exists. It never touches the work tree, the index, `HEAD` or another ref,
  and nothing is ever pushed. Journal `review_imported` (branch, tip).
- **Discard** asks one reason (`should_not_have_run`, `poor_result`,
  `not_needed`). Journal `review_discarded`.
- Once the package is on the host, the containers, the volume and the
  networks are removed. The run folder stays 30 days; its texts fall under
  D-85's bound.
- The Orchestrator view shows per template, from the journal: runs,
  `no_change` runs, failed runs, quarantined, imported, discarded by reason,
  imported over decided, and FR-11's group notice. Whether an imported branch
  was merged is v0.7's measure (section 5).

Import fetches the objects, then creates the branch with a zero old value, so
it never moves an existing reference. A package undecided when its texts reach
the 30-day bound is discarded `expired`.

#### FR-13: Memory as proposal (Slice B)
When a project names its memory folder (`[project.workshop] memory_name`, the
folder name `/memory` shows in a host session), the host folder is
`<floor config dir>/projects/<memory_name>/memory`, the config dir being
AD-7's (the floor's `config_dir`, else `CLAUDE_CONFIG_DIR`, else the default);
no literal path is written in the floor. That folder is mounted read-only, and
the entry point copies it into the worker's own memory folder before the run,
with `CLAUDE_CODE_PROJECT_DIR_NAME` set to the same name (S8). After the run
the manifest carries the worker's folder, and the window diffs it against the
host's. Each added or changed file is a memory proposal: its path within the
folder and its diff, as `Untrusted<String>`. Officina never writes a memory
file (principle 2, D-80): accepting a proposal copies its text to the
clipboard and shows the host path to paste it into. Proposals end with their
package. The name is declared, not computed, because S8 says it is derived
from the git repository without giving the rule.

### 4.4 Control and proof

#### FR-14: Stop
- **Stop all**, in the header while any run is live and in the Workshop
  section, keyboard-reachable since it only reduces, and `officina stop --all`
  from the CLI. Both write `<officina home>/workshop/stopped` and the relay's
  `relay/stopped` (AD-34), stop every worker and gateway container of the
  floor within 10 s, and keep the containers for examination. The author's
  interactive and background sessions are not touched. Journal
  `workshop_stopped` (scope, cause).
- The CLI may write a stop flag, never remove one; Resume is the window's
  (AD-21). This amends AD-34, where only the window wrote `relay/stopped`:
  stopping is safe from any process.
- Both flags are read before any launch and at window start. A flag that
  cannot be read counts as set (fail closed, as AD-34).
- Kept containers still hold the credential in their engine configuration:
  they are removed after 7 days, or at Resume once the author confirms he has
  examined them.
- FR-8's limits stop one run and set no flag.

`officina stop --all` stops every container Officina labelled on the engine,
whatever its floor. While stopped, no build, battery or packer starts; Resume
lists the kept runs, each to collect or remove, and can resume the relay too.

#### FR-15: Escape battery
A fixed program in the workshop image, run instead of `claude` in a container
built by the worker's own vector with a test allow-list (the two golden
vectors differ only in the command), plays the attempts below and prints one
line per attempt. It is played on each image a launch may use: the base, each
project image and, in Slice B, the browser base. The window journals one
`escape_attempt` line per attempt and one `escape_battery` line with the
totals, the image labels, the engine version and Officina's version; the
gateway journals its own refusals. Families:
- egress: TCP to a public address, IPv6, UDP and DNS to a public resolver,
  resolution of an outside name, ICMP;
- gateway: a host off the list, an IP literal, ports 80 and 22 on an allowed
  host, a plain `GET` in absolute form, a malformed and an oversized request,
  a mismatched server name, a name that resolves to loopback, a request after
  the deadline, a request after the refusal limit;
- host: `host.docker.internal`, the engine's API ports, an engine socket path,
  the gateway's other ports, another run's network;
- files: a write in each mount and in the root file system, host paths that
  must not exist (`/mnt` and Docker Desktop's host mount paths), `mount`;
- privileges: uid, effective capabilities, `no_new_privs`, setuid programs
  and `sudo` or `su` failing to raise privileges;
- credentials: no SSH key, agent socket, git credential, `.netrc`, cloud
  credential file or engine socket; the environment's names are exactly
  FR-7's list plus the credential's;
- resources: processes past the limit, memory past the limit.

Pass: every attempt refused, and each gateway-family attempt matched by a
`gateway_refused` line. A passing battery is valid for its engine version,
image labels and Officina version, for 30 days (FR-6, `battery_stale`). The
author runs it from the Workshop section or from a stale card; doctor shows
the last outcome per image and its age. A host family, played in tests with
fixtures, feeds FR-10 and FR-11 crafted packages (each quarantine cause, each
sensitive group, a token raw, in base64 and in hex) and oversized or forged
stream-json lines. v0.5's injection battery (AD-95) gains a workshop card
section.

The battery runs in two phases with a decoy on another network; its gateway
resolves two test names to loopback and to the engine host. An inconclusive
attempt fails the battery. Validity also keys on the gateway image and the
engine's kernel.

### 4.5 Window, browser, doctor

#### FR-16: Browser base image (Slice B)
`officina-workshop-browser` is the base image plus headless Chromium and the
system libraries a browser needs, from the base distribution. It is a second
base for project Dockerfiles. Its consumer: a project's own end-to-end suite
(Playwright with Chromium) run by the worker, for instance in a Svelte
project; the project's Dockerfile, `FROM officina-workshop-browser`, adds the
project's Playwright and its browser build at build time. Through
`HTTPS_PROXY` the browser reaches the allow-list only; `NO_PROXY` keeps the
project's own servers inside the container reachable. Gate 8.

#### FR-17: In the window
- **Board:** a card kind `workshop` (key `ws:<run id>`): Working while live,
  with an "isolated" chip; Needs you while its package awaits a verdict; Done
  for 24 h after the verdict. `BOARD_SCHEMA` goes up once for v0.6 (P-13's
  rule).
- **Header:** a neutral chip "N in the workshop" (AD-55: not amber, not in the
  title) and Stop all while any run is live.
- **Orchestrator view:** a Workshop section with the image states, their
  inventories and Build images, the battery's last outcome per image and Run
  escape battery, live and kept runs, the stop flag with Resume, FR-12's
  per-template table, and the week's workshop share (section 7). The work
  order history shows workshop runs with their outcome.
- **Briefing** (v0.5 FR-1): live runs, packages awaiting review, quarantined
  and failed runs. `officina mcp` serves them through `briefing` and `board`;
  no tool is added, and none imports, discards or stops.
- **Scene** (Slice B): a live worker sits at a desk of its project's workshop
  with a neutral glass outline; a package awaiting review gives it the waiting
  halo (v0.4 FR-3, AD-65).
- Every text from a run is `Untrusted<T>` (AD-74): report tail, commit
  subjects, paths, requested hosts, memory proposals, error text.

The Workshop section has its own payload. A stopped run sits in To resume.
`officina workshop --json` prints the section read-only.

#### FR-18: Doctor
Slice A:
- the engine (reachable, Linux containers, version); `docker.exe` and `git`
  resolved and not shims;
- images present, matching, and their inventories; the battery's last outcome
  and age per image, `battery_stale` named;
- the upstream state the newest gateway reported when `upstream_proxy` is
  set; `ca_file` readable and baked into the images;
- the credential present (never read for display) and its kind;
- per project: each mount present and ignored, and any mount ignored only
  through untracked rules; each mounted settings file checked for an `env`
  block or `apiKeyHelper` (warn, names only: the worker can read them); the
  programs named by mounted hooks and the project's `.mcp.json` against the
  inventory, `PreToolUse` ones as failures; the project Dockerfile present
  and approved;
- both stop flags; kept containers and their age; the image's Claude Code
  version against the host's;
- two printed notes. On Docker Desktop's WSL 2 backend the engine runs in a
  WSL 2 utility VM; WSL 2 distributions share one kernel, so a container or
  kernel escape may reach the author's own WSL distributions, their keys and
  the Windows drives they mount (the cautious reading, not verified here).
  Hence the framing's standing advice: never leave the SSH agent unlocked,
  and failing that use `ssh-add -c`.

Slice B: memory folders present; the browser base present and matching.

`DOCKER_HOST` and `DOCKER_CONTEXT` named when set; `max_workers` times
`memory_mib` against the engine's memory.

## 5. Non-Goals (Explicit)

- Launches without a gesture, pre-approved permanent templates, the attention
  budget, the governor, template suspension by acceptance rate, the daily
  autonomy window, and the global stop shortcut (v0.7). v0.6 records the
  verdicts v0.7 reads.
- Merged detection, whether an imported branch reached its base: moved to
  v0.7 with its consumer, the acceptance rate, not dropped. v0.6 keeps
  imported over decided, from the journal.
- Relay actions, server identities or the framing's `iterative` profile in the
  workshop; commands and the workstation's browser for workers; intervention
  orders with a dry run (v0.8). v0.6 is the framing's `strict` profile: branch,
  tests, review, no server identity.
- Talking to a running worker: no attach, no answer, no question. The worker
  runs to its end with `--permission-prompts none`; a follow-up is a new work
  order, which may start from the imported branch (FR-6).
- Pushing, opening a pull request, or merging, from the container or from
  Officina. Import creates a local branch only.
- A gateway that injects the credential as an `ANTHROPIC_BASE_URL` reverse
  proxy, so the worker holds no key and `api.anthropic.com` leaves the CONNECT
  list. It would close section 1's confidentiality gap for `api-key` only:
  with OAuth it would intermediate credentials (S3), and it needs TLS
  termination and a TLS client crate. Later, if the owner moves the workshop
  to API keys.
- An upstream proxy that requires authentication (basic, NTLM, Kerberos):
  `upstream_proxy` chains without credentials.
- Carrying uncommitted changes or ignored files other than the read-only
  mounts; writing back to ignored folders (section 8, question 12).
- The author's user-level Claude Code configuration, and instructions above
  the project folder, in the workshop (FR-9).
- Podman, Rancher Desktop or another engine (untested; FR-1 keeps to a CLI
  subset they also have); Linux and macOS hosts (v1 and later).
- Claude Code's built-in sandbox as the boundary: it does not run on native
  Windows and covers shell commands only (S5).
- TLS inspection or per-URL rules at the gateway; plain HTTP, SSH or UDP
  egress.
- Sharing a container between work orders; long-lived workers.
- The hash-chained journal, deferred by v0.2 to v0.6 "when writer and attacker
  differ", is dropped. In v0.6 the worker has no path to the journal at all
  (FR-3, checked by the battery), and host sessions run as the author and
  could recompute any chain, so a chain would prove nothing against either.
  Decided under the owner's delegation, listed for his confirmation.
- Officina writing any file under a Claude config dir, memory included
  (FR-13).
- Publishing or signing images; pulling ready-made workshop images.

## 6. MVP Scope

### 6.1.1 Delivery order
- Slice 0: FR-0. Gate 7 and gate 0 first; then gate 9 and gates 1 to 6.
  Stories may be written before the gates, but no Slice A story is implemented
  before gates 0 and 7 are recorded; if gate 7 says no and no API key is
  available, v0.6 stops at Slice 0. Gate 8 before FR-16.
- Slice A: FR-1 (base, gateway and project images, CA file, inventory), FR-2
  (all keys but `memory_name`), FR-3 to FR-12, FR-14, FR-15, FR-17 (board,
  header, Workshop section, per-template table, review panel, briefing) and
  FR-18's Slice A part. Usable alone: one isolated work order on a project's
  own toolchain, from click to imported branch, with a fresh battery showing
  the boundary holds.
- Slice B: FR-13, FR-16, FR-17's scene, `memory_name` and FR-18's Slice B
  part.

What any slice defers is done before v0.7 starts (P-7).

### 6.1.2 Dependencies for the owner's approval

| Item | Where | Licence | Why |
|---|---|---|---|
| Docker Desktop (installed by the owner, not shipped) | workstation | Docker's subscription terms (gate 7) | the engine (FR-1) |
| Debian slim base image, pinned by digest | built locally | Debian (DFSG) | worker OS |
| git, Python 3, `python-is-python3`, bash, coreutils, CA certificates (Debian packages) | built locally, run as programs | GPL-2.0, PSF and others | worker tools; `python` for the workspace's hooks |
| uv, pinned release with checksum | built locally | MIT or Apache-2.0 | the workspace's Python hooks and tools |
| Claude Code native build, host's version, unmodified | built locally | Anthropic's terms (S3) | the worker |
| Rust target `x86_64-unknown-linux-musl` with `rust-lld` | workstation and CI release job | MIT or Apache-2.0 | the static build of the crate `officina-container` (`officina-gateway`, `officina-entry`): `cargo build --release -p officina-container --target x86_64-unknown-linux-musl`; a crate of its own because `officina` links SQLite's C sources, which a musl build cannot compile without a musl C toolchain (gate 9) |
| A pinned Rust build image, only if gate 9 fails | built locally | MIT or Apache-2.0 | fallback multi-stage gateway build |
| Chromium and its libraries (Debian packages), Slice B | built locally | BSD-3-Clause and others | FR-16 |

Project Dockerfiles bring their own toolchains, approved with the catalogue.
Images are built on the owner's machine from Dockerfiles in this repository
or in his projects; Officina redistributes no image. No crate is expected: the
gateway uses std networking, chains CONNECT and parses the ClientHello's
server name by hand; the Credential Manager read is AD-36's.

## 7. Success Metrics

| Metric | Target | How measured |
|---|---|---|
| Containment | every escape battery attempt refused, on every image; each gateway-family attempt journaled as a refusal | battery on CI with a real Linux engine on every change to the images, the gateway or the vectors; on the workstation with Docker Desktop before any launch whose engine, image or Officina version changed, and every 30 days (`battery_stale`) |
| Hooks | on each owner project that declares hooks, every configured event's hooks ran inside the workshop; zero run continued past a hook exit of 126 or 127 | gate 4, then one run per project; review panel counts; `hook_failed` causes |
| Gesture | zero container started and zero import without a pointer click | injection battery gains workshop card and import sections; a fake engine counts `create` and `start`: 0 before the click, one gateway and one worker after; vectors equal to the golden rebuild |
| Host untouched | a run, a quarantine and a discard leave the project repository (refs, index, work tree) and the Claude config dir unchanged; Import adds only `refs/heads/officina/<name>` | fingerprint test (D-80 extended to the project repository) with the fake engine and fixture packages |
| Credential | the credential's value in no Officina file, journal line, registry row, log or IPC payload; a package carrying it raw, in base64 or in hex is quarantined | e2e with a seeded value searched in every output |
| Stop | Stop all ends every run within 10 s; the flags survive a restart; nothing launches while set | e2e with the fake engine; once on the workstation |
| Use | 10 working days with at least 10 runs reaching a verdict, from at least two of section 2.3's templates as the owner confirms them, on at least 2 projects | registry |
| Workshop share | workshop runs over workshop runs plus host work orders, per week; reported, read at the close | journal (`workshop_started`, `work_order_launched`) |
| Environment failures | runs ended by causes of `ENVIRONMENT_FAILURE_CAUSES`, over runs: under 20 % in week 2 | `workshop_finished` causes, `gateway_refused` |
| Review cost | median under 5 minutes from `review_opened` to verdict; share of packages needing a sensitive approval, with FR-11's notice after 10 verdicts | journal |
| Acceptance | imported over decided at least 50 % per template with 5 or more verdicts; v0.7 starts autonomous launches only for templates above that | Orchestrator view, from the journal |

The v0.6 close reads Use, Workshop share, Environment failures, Review cost
and Acceptance (P-16).

A `no_change` run counts as a run in Use, Workshop share and Environment
failures' denominator; it is not a failure, has no verdict, and Review cost
and Acceptance leave it out. v0.7 reads it to skip or slow a standing order
whose base did not change.

## 8. Open Questions

0. **Gate (FR-4, FR-1), first.** Can a container on the workstation reach
   `api.anthropic.com` at work, and through what? Check: from a container on
   Docker Desktop's default network, as `worker`, `curl -v
   https://api.anthropic.com`; record whether it reaches the public
   certificate chain, and whether the host uses `HTTPS_PROXY`, a PAC file or a
   TLS-inspecting CA (the certificate issuer seen from a host `claude`
   session's network). An upstream proxy sets `upstream_proxy`; a
   TLS-inspecting CA sets `ca_file`; a PAC file is read once for the proxy it
   names for that host. A proxy that requires authentication is out of v0.6
   (section 5): the workshop then cannot run on that network. Its container
   half was verified on 2026-10-28 on the workstation's network (addendum
   section 1); the owner states whether that network is the work floor's.
1. **Gate (FR-1, FR-3, FR-6).** Does Docker Desktop on the workstation run the
   base image with FR-3's properties as uid 1000, mount read-only a git-ignored
   folder of a project inside the synced tree, and copy files into a created
   container's work volume and out of a stopped one? Check: build the base
   image, recording the installer's hosts and checksum check, and a project
   image from it without a pull; run it with the exact vector; read a file in
   the mount and try a write (expect a read-only file system error); `docker
   cp` in and out; end the process that started the container and see it keep
   running; the `local` log driver; record the engine and WSL versions. A mount that accepts a write stops Slice A. A
   failing copy into the volume moves the bundle and prompt to a read-only
   mount of the run's `in` folder.
2. **Gate (FR-4).** Does an internal network on Docker Desktop give the worker
   no route out, no resolution of outside names and no reach to
   `host.docker.internal`, while a gateway attached to both networks reaches an
   allowed host (directly, or through `upstream_proxy` when gate 0 set one) and
   the worker reaches it through `HTTPS_PROXY`? Check with the battery's
   egress and host families against a stub gateway. If outside names resolve,
   the worker gets no resolver at all and reaches the gateway by its fixed
   address on the run's network.
3. **Gate (FR-4, FR-5, FR-7).** Does a worker authenticate and work behind a
   gateway that allows only `api.anthropic.com`, with a token generated on the
   host's browser and nothing else? This is framing step 0 item 7 ("setup-token
   usable in a container without a browser"), recorded as its own E-entry.
   Check: in the base image as `worker`, a fresh config dir,
   `CLAUDE_CODE_OAUTH_TOKEN`, FR-7's vector with a prompt that edits a file,
   runs `git status` and commits; expect no dialog, no login prompt, exit 0 and
   the commit; record every host the gateway allowed and refused, and that
   `claude --help` at the image's version lists `--permission-prompts`. If the
   run fails for want of a host, add the hosts recorded (first
   `platform.claude.com`, S4) to the fixed set and run again. A dialog or a
   prompt that blocks stops Slice A and reopens FR-7.
4. **Gate (FR-9).** Do hooks from a read-only mounted `.claude/` run in the
   worker, which does the stream report, and what of a real session's setup
   do the mounts miss? Check: a scratch repository whose git-ignored
   `.claude/settings.json` declares a hook that appends one line to a file
   under `$HOME`, on the work volume, for `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
   `PostToolUse`, `Stop` and `SessionEnd`; run a worker; compare the file
   (copied out) with the stream's hook events. Then, in one real host session
   at work, record where each instruction file and hook comes from (`/memory`,
   `/hooks`), and run one worker per owner project that declares hooks, with
   its project image. A hook that does not run, or a source the mounts cannot
   carry that the project relies on, is a finding against FR-9 before Slice A
   closes.
5. **Gate (FR-8).** Are the stream-json fields FR-8 reads present on the
   image's version: `system/init` (model, tools, MCP servers), `result`
   (subtype, error flag, cost, turns, duration, session id),
   `system/api_retry` (`error`), the hook events with their exit code? Check
   on gate 3's run, with a hook whose program is missing; the fields become
   canary constants (AD-12) for the stream. A missing field reads unknown,
   never done.
6. **Gate (FR-7).** Does `--max-budget-usd` stop a run authenticated with a
   setup token? Check: a cap of USD 0.01 on a multi-step prompt; record the
   result subtype and the reported spend. If not, `max_usd` is dropped and the
   deadline remains.
7. **Owner gate (FR-5, FR-1), first.** Which Claude account does the work
   floor use, personal or an employer plan? Is the workshop's use of a
   `setup-token` token ordinary use under that plan (S3), and, for an employer
   plan, does the employer accept a one-year token driving unattended workers
   on that machine? Or will the owner, or his employer, pay for an API key?
   And do Docker Desktop's subscription terms hold where it runs (free for
   personal use and small businesses, paid otherwise, an obligation of the
   organisation)? The owner records the answers in the research notes before
   any Slice A story is implemented. If the token is refused, `api-key`
   becomes the only kind; if no API key is available either, v0.6 stops at
   Slice 0.
8. **Gate (FR-16, Slice B).** Does a project image built `FROM
   officina-workshop-browser` run that project's Playwright suite with
   Chromium in a worker, against the project's server inside the container,
   while a page on an off-list host fails with a `gateway_refused` line? If
   not, the browser base leaves v0.6 and returns with v0.8's browser work.
9. **Gate (FR-1).** Does `cargo build -p officina --target
   x86_64-unknown-linux-musl` succeed on the workstation with `rust-lld`, and
   does the binary run as the gateway in an empty image? If not, the gateway
   image builds it in a multi-stage step from a pinned Rust image, listed for
   the owner's approval (section 6.1.2).
10. Where is a `setup-token` token revoked? S2 does not say. Check in the
    account settings; the README records it beside FR-10's quarantine advice.
11. Does WebSearch reach the web from Anthropic's side, outside the gateway?
    The pages read do not say; FR-7 disallows it either way. Recorded if a
    page states it.
12. Ignored folders are read-only, as decided with the owner: a template that
    writes BMad run outputs or other ignored files fails there. If real use
    asks for it, a later version returns ignored outputs as a package section
    with its own approval.
13. The values 2 workers, 2 CPUs, 4 GiB, 60 minutes, 10 packages awaiting a
    verdict, 20 refusals in 5 minutes, 30 days of battery validity, 7 days of
    kept containers, 64 MiB and 500 commits are starting values, measured over
    two weeks of use.
