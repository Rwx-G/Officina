---
title: "Architecture addendum: Officina v0.6 - The Isolated Workshop"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions; new workspace crate officina-container and CI-only hadolint accepted; gates open)
inputs:
  - prd/prd-v0.6.md (revised after the requirement challenge)
  - (requirement challenge, not published)
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
  - (the code: crates/officina-core/src/relay, crates/officina-host/src/relay, platform/windows.rs, relay/tree.rs; v0.2 in progress)
  - (Docker Desktop and the toolchains on the workstation, probed 2026-10-28)
---

# Architecture addendum: Officina v0.6

AD-1 to AD-96 stay in force except where section 3 amends them; this addendum adds AD-97 onward and risks from R47. **Verified** marks a fact observed on the workstation for this addendum (section 1). **Gate** marks a fact still to check, with the procedure of AD-126, before the dependent story closes. No documentation page was fetched for this addendum: the PRD's S1 to S8 stand as the PRD states them, three of them checked against the binary's own help (section 1, item 12).

One property organises everything below: **nothing written inside a container is believed on the host**. The container holds what the author lends it and nothing else; what comes back (stream lines, the package, the manifest, the gateway's lines about hosts the worker asked for) is data that the host verifies, recomputes or displays as untrusted text, and every rule that decides what a container may do is computed in core, on Windows, before the engine sees it.

## 1. Checked on the workstation

Windows 11 Pro 10.0.26200, 2026-10-28. Docker Desktop 4.95.0 (build 242831), engine 29.8.2 (API 1.56), containerd 2.3.6, runc 1.5.1, docker-init 0.19.0, kernel `6.18.33.2-microsoft-standard-WSL2`, WSL 2.7.14.0 (kernel 6.18.33.2-2), cgroup v2, 24 CPUs and about 47 GiB given to the engine. Claude Code 2.1.312 on the host. Rust 1.94.0 (the pinned toolchain), Git for Windows 2.51.0.

Probes used only images already in the local store (a generic Debian-based Node image and `busybox`), throwaway containers and networks named `officina-probe-*` and one anonymous volume, all removed afterwards (checked: no container, network or volume with the probe label left). No registry login, no push, no pull, no Docker Desktop setting changed. The only bind mount was a scratch folder under the user's temporary directory. `claude` never ran in a container. One HTTPS request without credentials went to `api.anthropic.com`. No tool output carried instructions addressed to an assistant.

**Verified.**

1. **The engine's identity.** `docker version --format "{{json .}}"` reports `Server.Os` `linux`, `Server.Platform.Name` `Docker Desktop 4.95.0 (242831)`, `Server.Version` and `Server.KernelVersion`; this is the identity the battery keys on (AD-117). `docker info` lists seccomp (built-in profile) and cgroupns as security options, the `runc` and `nvidia` runtimes only, no user namespace remapping, and swarm mode active (its `ingress` and `docker_gwbridge` networks exist).
2. **The engine's defaults are the author's.** The default log driver is `local`, set on this machine; the engine reports Docker Desktop's own proxy `http.docker.internal:3128` for its pulls. Nothing is injected into containers: a `busybox` container's environment held `HOME`, `HOSTNAME`, `PATH` and its shell's own two names, no proxy variable.
3. **Where `docker.exe` is.** `HKLM\SOFTWARE\Docker Inc.\Docker\1.0` holds `AppPath` and `BinPath`, and `docker.exe` is in `BinPath`. The same key's `HumanVersion` reads 4.47.0 while the engine reports 4.95.0: in-app updates do not refresh it, so versions come from `docker version` only.
4. **Gate 0, container half, this workstation's network.** As uid 1000 on the default network, TLS 1.3 to `api.anthropic.com` verified against the public chain (issuer Google Trust Services, `WE1`). On the host: no proxy variable in the environment, Windows `ProxyEnable` 0, no `ProxyServer` or `AutoConfigURL` value. Whether this is the network the work floor uses is the owner's to say (AD-126, gate 0).
5. **Gate 2, with a stub gateway.** A network created with `--internal` gave the container a route to its own subnet only, no default route and no IPv6 address but loopback. The embedded resolver answered the peer container's name and nothing else: `api.anthropic.com`, `example.org`, `host.docker.internal`, `gateway.docker.internal` and `http.docker.internal` did not resolve. TCP to public addresses, to the engine host's address on ports 80 and 2375 and to Docker Desktop's proxy failed at once; UDP to a public resolver failed with "network unreachable". A stub CONNECT proxy attached to the internal network and to a user-defined bridge relayed `HTTPS_PROXY=http://<its name>:3128` traffic to `api.anthropic.com`, and logged one CONNECT.
6. **curl and the proxy variables.** curl 7.88.1 ignored an uppercase `HTTP_PROXY` for an `http://` URL and tried a direct connection, which failed for want of a route. Tools differ on the case they read, so the vector sets both cases (AD-102).
7. **Gate 1's properties, on a stand-in image.** With `--user 1000:1000 --read-only --cap-drop ALL --security-opt no-new-privileges:true --pids-limit 256 --memory 512m --memory-swap 512m --cpus 1 --init --tmpfs /tmp`: inherited, permitted, effective, bounding and ambient capability sets all zero, `NoNewPrivs` 1, `Seccomp` 2; the cgroup showed `pids.max` 256, `memory.max` 512 MiB and one CPU; writes to `/` and `/usr` failed with "Read-only file system" while `/tmp` took them; `mount` failed; no engine socket; `/mnt` held only the probe's mount; `su` failed authentication and `sudo` was absent.
8. **A read-only bind mount.** A folder on `C:` mounted with `--mount type=bind,...,readonly` was readable, and both rewriting a file and creating one failed with "Read-only file system". The synced-tree case stays in gate 1.
9. **Copy in, copy out.** On a container created with `--read-only`, an anonymous volume and `--network none`, `docker cp` of a folder into the volume's path succeeded before start; the files arrived owned by uid 0, mode 0755, readable by uid 1000; the container, as uid 1000, wrote into the volume (the image's content had been copied into the empty volume with its owner); `docker cp` out of the exited container succeeded; `docker rm -v` removed the volume.
10. **A credential by name.** `--env NAME` with no value took the value from the CLI's own environment, and `docker inspect` showed that value in `Config.Env`.
11. **Detached means detached.** A container started by `docker start` kept running after the CLI returned (observed for over 40 s). `docker network connect` works on a created, not yet started, container. Labels filter in `docker ps --filter label=...`.
12. **The binary's own help (2.1.312).** `--permission-prompts <target>` exists with `none`: "anything that would prompt is denied automatically; the permission mode still decides everything else". `--include-hook-events`, `--max-budget-usd` (print mode only) and `--disallowedTools` exist. `--bare` skips hooks, CLAUDE.md auto-discovery, auto-memory and keychain reads. `--permission-mode` accepts `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`. `setup-token` "requires Claude subscription".
13. **Toolchains.** The pinned Rust toolchain ships `rust-lld.exe`; the `x86_64-unknown-linux-musl` target is not installed, and this probe did not install it (gate 9). `HKLM\SOFTWARE\GitForWindows` holds `InstallPath`; `git` on `PATH` resolves to `mingw64\bin\git.exe`, then `cmd\git.exe`.
14. **Git Bash rewrites arguments.** Run from Git Bash, `docker create ... node /tmp/x.js` received a Windows path in place of `/tmp/x.js` (MSYS path conversion). Officina spawns without a shell and is not affected; the gate procedures run from Git Bash set `MSYS_NO_PATHCONV=1`.

**Gate.** PRD section 8, questions 0 to 9, by AD-126. Items 4, 5, 7, 8, 9 and 11 run part of gates 0, 1 and 2; what they leave open is in AD-126.

## 2. Decisions

### AD-97: Module layout and where authority lives

- `officina-core::workshop` (pure, `now_ms` as argument; compiles for Windows, Linux and the musl target):
  - `floor`: the `[workshop]`, `[project.workshop]` and `[template.workshop]` types, their phase 2 rules and canonical forms (AD-101);
  - `names`: floor key, object names, labels (AD-98); `image`: the base Dockerfiles, label sets, Dockerfile stage parsing, the launch-time image check (AD-99);
  - `vector`: `ContainerRole`, `ContainerVector`, `RunPlan`, one builder per role (AD-102); `engine`: `EngineCall` vectors and parsers of the engine's JSON outputs (AD-98);
  - `gateway`: request parsing, host and allow-list rules, address classes, ClientHello parsing, the refusal limiter, line types (AD-103);
  - `launch`: `WorkshopLaunch`, base choices, launch checks (AD-107); `run`: `RunState`, `FinishCause`, transitions and restart reconciliation (AD-109, AD-110, AD-116); `stream`: the stream-json fold (AD-109);
  - `bundle`, `scan`, `sensitive`, `review`: AD-111 to AD-114; `memory` (AD-115); `battery` (AD-117); `inventory` (AD-118); `view`: `WorkshopView` and board cards (AD-121).
- `officina-host::workshop`:
  - `engine`, read half, always compiled: resolution, `version`, `ps`, `inspect`, `image inspect`, `logs`, `stop`. `officina.exe` uses it for `stop --all`, `workshop --json` and doctor.
  - Launch half under `window-authority` (AD-21), its error type carrying `AUTHORITY_MARKER` (D-9): `network`, `volume`, `create`, `cp`, `start`, `rm`, `build`, and the modules `build`, `launch`, `follow`, `collect`, `verify`, `import`, `battery`, `inventory`, `resume`. Built without the feature, the launch half refuses before any process starts; a host unit test compiled only without it iterates `EngineCall::ALL` (D-128's pattern).
  - `files`: reads the catalogue files of AD-101, bounded. `host::git` gains the v0.6 rule and resolution (AD-111). `platform::windows` gains the HKLM string read and AD-36's credential read.
- **New crate `crates/officina-container`** (AD-100): binaries `officina-gateway` and `officina-entry`, depending on `officina-core` and `serde_json` only, std networking and processes, built for `x86_64-unknown-linux-musl`.
- `officina`: subcommands `stop --all` and `workshop --json` (AD-121).
- Lint: the `Command::new` sites of `officina-host` (D-128's test counts them) go from three to four: `workshop::engine` joins `spawn`, `relay::run` and `git`.

Gives up: a fourth crate and a cross target in the workspace. In exchange the Linux programs build without a C toolchain (AD-100), every rule a container obeys is decided and tested in core on Windows, and "no CLI path launches a container" is a property of the shipped `officina.exe`, checked by the existing marker step.

### AD-98: The engine driver

- **Resolution:** `OFFICINA_DOCKER_BIN` (tests); else `BinPath` from `HKLM\SOFTWARE\Docker Inc.\Docker\1.0` joined with `docker.exe`; else `docker.exe` on `PATH`. A file ending in `.exe` only; a shim is refused (AD-51). HKLM comes first because no process running as the author can write it, while `PATH` is whatever started the window. The key's version values are never read (section 1, item 3).
- **Identity:** core's `engine::EngineInfo` parses `docker version --format "{{json .}}"`. No `Server` object gives `engine_unavailable` (`not_running`); `Server.Os` other than `linux` gives `engine_unavailable` (`windows_containers`); otherwise `EngineKey` is `Platform.Name`, `Server.Version` and `KernelVersion` joined. Read at window start, at each launch, build and battery, and by doctor, with a 10 s deadline.
- **Calls:** `EngineCall` is a closed enum, one variant per subcommand use: `version`, `build`, `image inspect`, `network create`, `network connect`, `network rm`, `volume create`, `volume rm`, `create`, `cp` (in and out), `start` (detached, or attached for the packer, inventory and battery), `logs --follow --timestamps`, `stop`, `rm`, `ps`, `inspect`. Core builds each vector from typed inputs; `engine::execute(&ResolvedDocker, &EngineCall, &EngineEnv)` is the one host function that turns a vector into a process. Golden cases per variant in `tests/vectors/workshop.json`. Podman's and Rancher Desktop's CLIs have the same subcommands (PRD FR-1); untested.
- **Process:** `CREATE_NO_WINDOW`, working directory the Officina home, `NoDefaultCurrentDirectoryInExePath=1`, stdin null except for attached starts, stdout capped at 1 MiB except followers, the first 2 KiB of stderr kept. Each child is assigned to a kill-on-close job right after spawn (D-64), so a closed window leaves no `docker.exe` behind; containers are the engine's and keep running. Deadlines: 10 s for `version`, `ps`, `inspect`, `network`, `volume`, `stop` and `rm`; 30 s for `create` and `start`; 120 s for `cp`; 30 minutes for `build`.
- **Environment:** the window's, minus `SESSION_ENV_REMOVED` (AD-80), every `OFFICINA_*` name, `CLAUDE_CODE_OAUTH_TOKEN` and `ANTHROPIC_API_KEY`; the worker's `create` alone gets the one credential (AD-105). `DOCKER_HOST`, `DOCKER_CONTEXT` and `DOCKER_CONFIG` are the author's to set; doctor names them when present.
- **Names:** a run's id is its card id (32 hex, AD-78). Its objects are `officina-worker-<id>`, `officina-gateway-<id>`, `officina-pack-<id>`, `officina-int-<id>`, `officina-out-<id>` and `officina-vol-<id>`. Images are tagged `:local` (AD-99). Every object carries `officina.managed=1`, `officina.floor=<floor key>` and `officina.version`; a run's objects also `officina.run`, `officina.role` and `officina.work_order`. The floor key is AD-8's FNV-1a of the normalised floor path, 16 hex.
- **Logs:** every container is created with `--log-driver local --log-opt max-size=32m --log-opt max-file=3`, because the default driver is the author's daemon setting (section 1, item 2) and `docker logs` does not work with every driver; 96 MiB covers FR-8's 64 MiB cap.
- **Threads** (AD-3 amended): the collector never runs `docker`. Thread `workshop` runs short engine jobs one at a time (launch steps, stopping one run, collection, removal, reconciliation); thread `workshop-build` runs builds, inventories and batteries; each live container has a follower (AD-109). Stop all runs on its command's blocking thread so it never waits behind a job (AD-116). Outcomes return to the collector as `AuthorRequest`s without deadline (D-135's outcome pattern).

Gives up: the engine API (a socket client means a crate, an async runtime and the named pipe); each call costs a `docker.exe` start, which only followers would repeat often, and they are long-lived.

### AD-99: Images

- **Dockerfiles live in the repository** under `docker/`: `workshop.Dockerfile`, `gateway.Dockerfile` and, in Slice B, `browser.Dockerfile`. Core embeds them with `include_str!`; each one's SHA-256 is a label. hadolint runs on them in CI (section 3).
- **`officina-workshop:local`:**
  - `FROM debian:trixie-slim@sha256:<digest>`, the digest bumped in one commit with the Dockerfile;
  - Debian packages, sorted, `--no-install-recommends`: bash, ca-certificates, coreutils, curl, git, python3, python-is-python3 (`python` for the workspace's hooks); `curl` joins the PRD's list for the build's downloads and the tools that call it, while the gateway decides what it reaches;
  - uv from its pinned release archive, checked against the SHA-256 written in the Dockerfile;
  - Claude Code: a build stage runs the documented native installer for `ARG CLAUDE_CODE_VERSION`, the host's version (AD-79's `Version` kind), and the final stage copies the binary to `/opt/officina/bin/claude`, root-owned, 0755. Trust is anchored in TLS to Anthropic's distribution host, as for the host's own install and updates. Gate 1 records the hosts the installer contacts and that it verifies the binary's checksum; if it cannot pin a version or verifies nothing, the stage downloads the release binary and checks the checksum the release manifest publishes;
  - `officina-entry` copied from the build context to `/opt/officina/bin/` (AD-100);
  - user `worker`, uid and gid 1000, shell bash; `/work` owned by `worker`, holding `.officina/in`, `.officina/out`, `.officina/home`, `.officina/config` and `.officina/mounts`; every setuid and setgid bit on the image's file system removed;
  - CA: the context always holds a folder `ca/` with zero or one `officina-ca.crt` (the floor's `ca_file`); it is copied to `/usr/local/share/ca-certificates/` and `update-ca-certificates` runs. `SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE` and `NODE_EXTRA_CA_CERTS` point at the system bundle and `UV_NATIVE_TLS=1` is set, with or without a CA file, so Python, Node-based tools and uv read the store git and curl read;
  - `PATH` with `/opt/officina/bin` first, `WORKDIR /work`, `USER worker`, `ENTRYPOINT ["/opt/officina/bin/officina-entry"]`, OCI labels.
- **`officina-gateway:local`:** `FROM scratch`, the static `officina-gateway`, `USER 65534:65534`, `ENTRYPOINT`. No shell, no certificate, no resolver configuration of its own: the engine mounts it.
- **`officina-workshop-browser:local`** (Slice B, gate 8): `FROM officina-workshop:local`, Chromium and the libraries it needs from Debian, setuid and setgid bits removed again, `USER worker`. Chromium's own sandbox cannot start under the engine's default seccomp profile without user namespaces; the container is the boundary, and gate 8 records the launch flags the project's Playwright configuration needs.
- **Project images** `officina-project-<floor key, 8 hex>-<project>:local`: built from the project's Dockerfile with the Dockerfile's own folder as the build context (AD-101's bounds). Core's `image::stages` parses every `FROM`: the final stage must descend, through stage aliases, from `officina-workshop:local` or `officina-workshop-browser:local`; other stages may name any image (a builder stage pulling a toolchain). After the build, the image must carry the base's inherited label `officina.base`, else the build is refused `not_from_base` and the image removed.
- **Labels**, set with `--label` at build: the OCI set (`title`, `description`, `source`, `version`, `revision`, `licenses`, `created`) and `officina.base` (`workshop`, `browser`, `gateway`), `officina.version`, `officina.dockerfile.sha256`, `officina.entry.sha256`, `officina.gateway.sha256`, `officina.claude.version`, `officina.ca.sha256`; on project images `officina.floor`, `officina.project`, `officina.project.dockerfile.sha256`, `officina.project.context.sha256` and `officina.base.id` (the base image's id at build time).
- **Builds** happen on Build images only (a pointer click), on `workshop-build`, with the engine's default network, in this order: gateway, base, browser (Slice B), project images, then inventories (AD-118). The context of an Officina image is written fresh under `<home>/workshop/build/<image>/`; a project's is read in place. The last 64 KiB of build output is kept as untrusted text. Journal `workshop_image_built`.
- **Image check at launch** (`image_mismatch`, also shown on the card): the image exists and its labels equal what the running Officina would build now: its version; the Dockerfile, entry and gateway SHA-256; the CA SHA-256; for a project image the two SHA-256 of the approved catalogue and `officina.base.id` equal to the current base image's id. `officina.claude.version` is not compared: Claude Code updates itself on the host (E11 saw one within a day), and comparing it would force a rebuild and a battery each time; doctor warns when it differs (FR-18).

Gives up: reproducible images (the digest pins the base, apt mirrors move); Dockerfiles that copy files from outside their own folder.

### AD-100: The Linux programs and their build

- Crate `crates/officina-container`: `src/bin/officina-gateway.rs` (AD-103) and `src/bin/officina-entry.rs` with subcommands `run` (AD-108), `pack` (AD-110), `inventory` (AD-118), `battery` and `decoy` (AD-117). Dependencies: `officina-core`, `serde_json`; std only; workspace lints, `unsafe` denied.
- **Why a crate, not `officina gateway`** (PRD FR-1): by v0.6 the `officina` package links `officina-host`, which links `rusqlite` with SQLite's C sources bundled (AD-38's dependency). A musl build of it needs a musl C compiler, which the workstation does not have. `officina-core` and `serde_json` are pure Rust. The entry point and the battery also need code the gateway does not, and a scratch image should hold one small program.
- **Build:** `rust-toolchain.toml` gains `targets = ["x86_64-unknown-linux-musl"]`; `.cargo/config.toml` sets `[target.x86_64-unknown-linux-musl] linker = "rust-lld"` with the flavour flag gate 9 records; `cargo build --release -p officina-container --target x86_64-unknown-linux-musl`. Verified: the toolchain ships `rust-lld.exe`; the target is not installed yet.
- **Where Officina finds them:** `<exe dir>/linux/` (the window's executable directory is an argument, D-144, D-167); `OFFICINA_LINUX_BIN_DIR` for development and tests. Missing programs refuse Build images with `linux_programs_missing` and fail doctor.
- **Release and CI:** `release.yml` installs the target, builds both programs, ships them in the portable zip under `linux/` and as Tauri bundle resources in the NSIS installer, with their SHA-256 beside the others. The `windows` CI job builds them too, so the path cannot rot between releases.
- **Gate 9 fallback:** if the cross build fails, both Dockerfiles gain a builder stage from a Rust image pinned by digest, listed for the owner's approval (PRD 6.1.2); the label SHA-256 then come from the binaries copied out of the built image.

Gives up: one more target in CI and release, and a second compilation of core's code; it is the same source, so the gateway and the host cannot disagree on a rule.

### AD-101: Floor keys, phase 2, canonical form

- **Phase 1:** `ProjectEntry` gains `workshop: Option<toml::Spanned<toml::Value>>`, kept raw, and the floor gains a raw `workshop`; `RELAY_KEYS` gains `workshop` and `project.workshop` (D-7's test). A malformed workshop table never invalidates the floor the board uses. Templates are already raw (AD-75).
- **Phase 2**, each rule an `IssueKind` variant with a failing fixture under `every_issue_kind_has_a_case` (D-132): FR-2's rules, plus:
  - a workshop project's name passes `is_valid_action_name`, since it names `/work/<project>` and an image; two projects whose image names collide is an issue;
  - `hosts`: the last label holds a letter, so no numeric spelling of an address can match; an entry of the fixed set is not repeated;
  - `dockerfile`: at most 64 KiB; its folder, the build context, inside the project, at most 32 regular files and 1 MiB, no subfolder, no symlink; AD-99's stage rule;
  - `ca_file`: FR-2's rules; core checks that it holds only PEM `CERTIFICATE` blocks with valid base64;
  - `max_usd`: at most two decimals, stored as cents;
  - `mounts`: the syntactic rules only (relative, no `.` or `..` segment, at most 8, no duplicate, not `.git` or under it). **Existence and ignore status are launch conditions and doctor checks** (AD-107, AD-123), not phase 2: phase 2's output is the canonical form that `officina run` recomputes on every call (AD-29 step 2), so a `git check-ignore` per mount there would add tens of milliseconds per mount to every relay call, and creating or tracking a folder later would change the hash with no edit of the floor. PRD amendment 3.
- **Catalogue files:** the host's `workshop::files::read(&FloorFile) -> CatalogueFiles` reads each workshop project's Dockerfile and its folder, and the CA file, within the bounds above; a read failure is an issue. Phase 2 takes `&CatalogueFiles`. The window, `officina run`, doctor and `relay --json` call it; the hook's gate runs phase 1 only and never reads them.
- **Canonical form:** `CANONICAL_FORMAT` goes up by one relative to `main` at that merge (D-131's rule), once for v0.6. `CanonicalWorkshop` (auth, max workers, CPUs, memory, upstream proxy, CA path and SHA-256); per project, mounts and hosts sorted, the Dockerfile path, SHA-256 and context SHA-256 (over the sorted file names, lengths and bytes), review mode, sensitive paths sorted, memory name; per template, the base list in order (its first entry is the default), minutes and cents. Defaults explicit. A test loads an `approved.json` in the previous format and asserts AD-76's four facts again.
- `catalogue::diff` gains the three sections; the approval banner shows them. An invalid `[workshop]` turns the workshop off and drops every workshop template with an issue; host templates are unaffected.

### AD-102: Container vectors and the run plan

Core builds every container from typed inputs only; the host executes what core built.

| Role | Image | User | Network | Volume and mounts | Command | Limits |
|---|---|---|---|---|---|---|
| `Worker` | run image | 1000:1000 | the run's internal | run volume at `/work`; each mount read-only at `/work/.officina/mounts/<i>` | `officina-entry run` | `[workshop]` CPUs and memory; 1024 processes; `/tmp` tmpfs 512 MiB |
| `Gateway` | gateway | 65534:65534 | the run's egress, then internal | none | `officina-gateway <args>` (AD-103) | 0.5 CPU, 128 MiB, 128 processes |
| `Packer` | run image | 1000:1000 | none | run volume | `officina-entry pack` | 1 CPU, 1 GiB, 256 processes |
| `Inventory` | image | 1000:1000 | none | the project's mounts read-only | `officina-entry inventory`, list on stdin | 1 CPU, 512 MiB |
| `Battery` | image | 1000:1000 | battery internal | battery volume; battery mount read-only | `officina-entry battery --phase <n> <plan>` | as `Worker` |
| `BatteryGateway`, `Decoy` | gateway, run image | 65534, 1000 | AD-117 | none | AD-117 | as `Gateway` |

- **Every role:** `--read-only --cap-drop ALL --security-opt no-new-privileges:true --init --stop-timeout 3`, AD-98's log options and labels, `--memory-swap` equal to `--memory`, the engine's default seccomp profile. Never `--privileged`, `--device`, `--pid host`, `--ipc host`, `--network host`, `--userns host`, a `--security-opt` other than the one above, or an engine socket. The image's `ENTRYPOINT`, `CMD` and `USER` are always overridden by `--entrypoint`, the command and `--user`, so a project Dockerfile cannot change what FR-3 fixes.
- **Worker environment** (golden): `HOME=/work/.officina/home`; `CLAUDE_CONFIG_DIR=/work/.officina/config`; `HTTPS_PROXY`, `https_proxy`, `HTTP_PROXY` and `http_proxy` set to `http://officina-gateway-<id>:3128` (section 1, item 6); `NO_PROXY` and `no_proxy` set to `localhost,127.0.0.1,::1`; `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`; `DISABLE_AUTOUPDATER=1` (S1 names it; the read-only file system and the gateway would stop an update anyway); `CLAUDE_CODE_DISABLE_ARTIFACT=1`; `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`; `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME`, `GIT_COMMITTER_EMAIL` from the project repository's configuration (FR-6); in Slice B `CLAUDE_CODE_PROJECT_DIR_NAME`; the credential's name alone (AD-105). The approved image's own `ENV` (its `PATH`, AD-99's CA variables, a project Dockerfile's additions) applies under these; the vector's names win. FR-3's "exactly" therefore reads "the vector's names, the credential's name and the approved image's `ENV` names" (PRD amendment 4); the battery checks that set (AD-117).
- **Mount sources:** the project folder joined with the mount path, resolved absolute, an existing folder; a path holding `,`, `"` or a control character is refused `mount_path_unsupported` (AD-107), because `--mount` reads its value as comma-separated fields and a comma in a path could add a field such as one turning `readonly` off. Golden cases include spaces and accented letters.
- **The plan:** `RunPlan`, written by the window to `/work/.officina/in/plan.json` and parsed by the entry point with the same core type: schema, project, clone path `/work/<project>`, branch `officina/<work order name>`, base branch and commit, mount links (index and relative path), the exclude list, the `claude` argument vector (AD-106, FR-7), the deadline in seconds, the prompt file, and in Slice B the memory copy. The worker's argument vector is `-p --output-format stream-json --verbose --include-hook-events --permission-mode bypassPermissions --permission-prompts none --disallowedTools <WORKER_DISALLOWED_TOOLS>`, then `--max-budget-usd <amount>` and `--model <alias>` when the template sets them; the prompt is on stdin (E16).
- **Golden:** `tests/vectors/workshop.json`, section `vectors`: per role, the create arguments, connects, environment names and, for `Worker`, the plan. A test replaces the run id in the `Worker` and `Battery` vectors and asserts that what still differs is the command, the role label and the network and volume names (FR-15's "differ only in the command"). A coverage test iterates `ContainerRole::ALL`.
- **Plan hash:** `plan_sha256` is the SHA-256 of the canonical JSON of the worker's and gateway's vectors and the plan, with the run's absolute deadline and the credential's value left out. The card shows it and the click sends it back (AD-107).

### AD-103: The gateway

- **Arguments** (core builds and parses them, `GatewayConfig::to_args` and `from_args`, golden both ways): `--run <id> --listen 0.0.0.0:3128 --allow <host>...` (the fixed set, core's `CLAUDE_CODE_HOSTS`, then the project's hosts), `--deadline-ms <Unix ms>`, `--refusal-limit 20 --refusal-window-s 300`, `[--upstream <host:port>]`. No file, no environment.
- **Per connection** (one thread, at most 64 tunnels, refusal `busy` beyond):
  1. read the request, at most 8 KiB, 10 s deadline; core's `gateway::parse_request` accepts `CONNECT <host>:<port> HTTP/1.x` only;
  2. the port is 443; the host is a DNS name (lowercased, trailing dot removed, labels and length checked), not an IP literal; it matches the allow-list exactly or through a leading `*.` entry (one or more labels before the suffix);
  3. direct mode: resolve the name once (`ToSocketAddrs`), classify every address with core's `gateway::addr::is_public` (a constant table of non-public IPv4 and IPv6 ranges, IPv4-mapped and NAT64 forms unwrapped, a vector case per range); any non-public address refuses `private_address`, no IPv4 address refuses `no_ipv4`. Docker Desktop's own addresses fall in a private range;
  4. answer `200 Connection established`, read the first TLS records, at most 16 KiB, 10 s deadline; core's `gateway::tls::server_name` parses the ClientHello: not a handshake refuses `not_tls`, no server name `sni_missing`, a name other than the CONNECT host `sni_mismatch`;
  5. connect (10 s) to the address checked in step 3, never resolving again, write the buffered bytes, relay both ways with a second thread; close both sides at end of stream or at the deadline.
- **Upstream mode:** connect to the upstream proxy, send `CONNECT <host>:443 HTTP/1.1` with a `Host` header, read its answer (at most 8 KiB of headers), require a 2xx status (`upstream_refused` otherwise), then relay as above. Steps 1, 2 and 4 hold; step 3 is the proxy's (FR-4). At start the gateway opens one TCP connection to the upstream and reports `reachable` or `unreachable` in its `started` line, which doctor reads (AD-123).
- **Refusals** answer `400` (malformed, oversized), `405` (method) or `403` (the rest) and close. `GatewayRefusal` is a closed enum: `malformed`, `oversized`, `method`, `port`, `ip_literal`, `not_allowed`, `private_address`, `no_ipv4`, `resolve_failed`, `not_tls`, `sni_missing`, `sni_mismatch`, `connect_failed`, `upstream_refused`, `busy`, `deadline`, `limit`. The limiter (`gateway::Limiter`, a pure state machine over `now_ms`) counts refusals in a sliding window; at the limit the gateway writes a `limit` line and exits 3. After the deadline every request is refused `deadline` and open tunnels are closed.
- **Lines** on stdout, `gateway::GatewayLine`, one JSON object each: `started` (configuration digest, upstream state), `allowed` (host, port, whether the ClientHello carried an `encrypted_client_hello` extension), `refused` (host as printable ASCII, cut to 253 characters, port, cause), `limit`. The window journals each `refused` as `gateway_refused` and, at collection, one `gateway_summary` it computes itself, allowed connections per host included. The PRD's summary at exit is replaced (PRD amendment 5): std cannot catch the engine's stop signal, and a summary written at exit would be lost whenever the gateway is stopped or killed.
- **Encrypted ClientHello** is recorded, not refused: a client's decoy (GREASE) extension cannot be told from a real one by design, and refusing it would break clients that send it by default. The outer name must still be an allowed host. FR-4's "gives up" stands.
- The gateway holds no credential, writes no file, runs no shell; its image holds nothing else.

Gives up: the scrutiny of a widely deployed proxy (FR-4); its own vector set (`tests/vectors/gateway.json`), the battery's gateway family and the CI battery stand in.

### AD-104: Two networks per run

- `officina-int-<id>` is created `--internal` and holds the worker and the gateway; `officina-out-<id>` is a user-defined bridge holding the gateway alone. The gateway is created on `officina-out-<id>`, connected to `officina-int-<id>`, then the worker is created on `officina-int-<id>`. Verified with a stub gateway (section 1, item 5).
- **Not the engine's default network** (PRD FR-4 named it): the default bridge lets every container started without a network reach every other, and this engine runs other projects' containers. A per-run bridge holds the gateway alone, so a gateway fault cannot reach another container. Gives up: two networks per run from the engine's address pools; a failed creation ends the launch with step outcome `network_failed`, and doctor counts and offers to remove orphan `officina.managed` networks.
- The worker's resolver answers peer names on the internal network only (verified); `HTTPS_PROXY` names the gateway container. If gate 2 ever shows outside names resolving, the worker gets `--dns 0.0.0.0` and the gateway a fixed address, as the PRD's gate 2 says.

### AD-105: The credential

- `[workshop] auth`: `oauth-token` is passed as `CLAUDE_CODE_OAUTH_TOKEN`, `api-key` as `ANTHROPIC_API_KEY`; exactly one name per worker. The value is the Windows Credential Manager generic credential `officina:workshop`, its blob UTF-16LE, read with AD-36's `CredReadW` (`Win32_Security_Credentials`, approved for AD-36; story 2.11's function is reused if it has merged). Doctor prints `cmdkey /generic:officina:workshop /user:workshop /pass` (D-38).
- **Read** at launch, for the worker's `create` only; at collection, for the scan (AD-112); and for presence checks (card readiness, doctor), which drop it at once. The host holds it in a `Secret` type with no `Debug`, `Display` or `Serialize`, dropped after use. Tests inject a `CredentialSource` as an argument (D-86's pattern); no environment variable overrides the real source.
- **Passed** by setting `<NAME>=<value>` in the environment of the one `docker create` child of the worker while the vector carries `--env <NAME>` alone (section 1, item 10). Never on a command line, in a file, the journal, the registry, a log or an IPC payload. The battery passes a dummy value under the same name.
- **Lifetime in the engine:** `docker inspect` shows the value while the container exists (verified), to any process running as the author (FR-5's last paragraph). Hence containers are removed right after collection (AD-110), and stopped runs keep theirs at most 7 days (AD-116).

### AD-106: Principle 2 for workers

- The worker vector passes `--permission-mode bypassPermissions --permission-prompts none` (PRD FR-7, with its strongest argument stated there). Officina still never sets a permission mode on a session that runs as the author on the host. Verified: both flags exist on 2.1.312, and `none` denies anything that would prompt while the mode decides the rest (section 1, item 12). The worker runs as uid 1000, so S1's refusal of the flag as root does not arise.
- **Spine amendment** (section 3): the spine gains, after AD-20, a reading of principle 2: it governs sessions running as the author; a workshop worker's mode is AD-106's, because what a worker may do is what its container holds (FR-3). Decided under the owner's delegation, listed for his confirmation.

Gives up: per-call prompts in workers; a `PreToolUse` deny still blocks in every mode (E12), which is why AD-107 and AD-109 stop a run whose guard cannot start.

### AD-107: Card, launch checks, launch flow

- **Issue:** for a template with `[template.workshop]`, `issue_card` (AD-78) adds `WorkshopCardFacts`:
  - base choices: the template's `base` list, each with its tip (`rev-parse --verify refs/heads/<branch>^{commit}`, AD-111's rule); then the `officina/<name>` branches imported from that template's runs in the last 7 days that still exist, each with its parent run (registry, then `rev-parse`). The default is the first listed base;
  - the count of uncommitted changes (`status --porcelain=v1 -z --untracked-files=normal`, `GIT_OPTIONAL_LOCKS=0`); mounts with their state; the allow-list; the image's name, short id and build time; minutes and budget; the credential's kind;
  - readiness: every check below that has no side effect, each shown as its `LaunchRefusal`; and `plan_sha256` (AD-102).
- `EditCard` gains `Base(choice)`, which issues a new card (AD-78). `WorkshopLaunch::new(&WorkOrder, &ApprovedWorkshopTemplate, BaseChoice, &LaunchFacts)` is the only constructor of `WorkshopLaunch`, and `vector::run_plan(&WorkshopLaunch, ...)` the only builder of a worker's plan (`compile_fail` doctests, AD-74's pattern).
- **Launch:** `launch_work_order(card_id, prompt_sha256, plan_sha256)`, pointer only (D-28). After AD-78's checks (the `claude` shim check does not apply to workshop cards), in this order, each a `LaunchRefusal` variant with a test and a case in the injection battery's coverage (D-147): `workshop_off`, `workshop_stopped`, `engine_unavailable`, `image_mismatch`, `battery_stale`, `mount_missing`, `mount_not_ignored` (through untracked rules included, FR-2), `mount_path_unsupported`, `hook_program_missing` (after a fresh inventory when its key changed, AD-118), `workshop_full`, `reviews_pending`, `credential_missing`, `project_missing`, `base_moved`, `plan_changed`. Then the card is consumed (single use).
- **Steps,** on thread `workshop`; the first failure ends the launch with outcome `failed`, removes what was created and records the failing step:
  1. one transaction inserts the `work_order` row (runner `workshop`, outcome `launching`) and the `workshop_run` row (state `launching`);
  2. `<home>/workshop/runs/<id>/in/`: `git bundle create base.bundle refs/heads/<base>` (AD-111's rule), then core reads the bundle header: its single reference must point at the card's base commit (`base_moved`) and the file must not exceed 1 GiB (`base_too_large`); `prompt.txt`; `plan.json`;
  3. engine: both networks, the volume, the gateway (create, connect), the worker (create, with the credential), `cp` of `in/` into the worker's volume; the stop flag is read again; start the gateway, then the worker;
  4. state `running`, `work_order` outcome `started`, one `workshop_started` line with every step's outcome; followers start (AD-109).
- The stop flag is also read between steps 2 and 3; set, it ends the launch with `workshop_stopped` and removes what was created.
- **The `work_order` row** (AD-82 as amended by D-133): `argv_json` holds the worker's `claude` vector; AD-81's binding skips rows whose runner is `workshop`; the subject is filed when the run reaches `running`, as v0.5 files at `started`.

### AD-108: The entry point

`officina-entry run` is fixed code. It reads `/work/.officina/in/plan.json` with core's type (an unknown schema exits non-zero) and prints one JSON status line per step on stderr (`EntryLine`):

1. `git clone --branch <base> /work/.officina/in/base.bundle /work/<project>`; `git checkout -b officina/<name> <base commit>`; `HEAD` must equal the base commit;
2. append each mount path to the clone's `.git/info/exclude`; for each mount, create its parent folders and a symbolic link at its place in the clone pointing at `/work/.officina/mounts/<i>`;
3. Slice B: copy `/work/.officina/memory` into `$CLAUDE_CONFIG_DIR/projects/<memory_name>/memory/` (AD-115);
4. run `timeout --kill-after=30 <deadline_s> /opt/officina/bin/claude <argv>` in `/work/<project>`, stdin the prompt file, stdout and stderr inherited, so `claude`'s stream-json is the container's stdout; record the exit code on stderr and exit with it.

The entry point never packages (AD-110). Officina adds no setting file, hook, CLAUDE.md or MCP server; the plan's flags and the vector's variables are its only additions (FR-9).

### AD-109: Following a run, limits, restart

- **`RunState`** (closed enum, transitions in core with a vector case per edge): `launching`, `running`, `stopping`, `collecting`, then `review`, `quarantined` or `no_change`; `stopped` (Stop all, AD-116); terminal `imported`, `discarded`, `no_change`, `failed`, `unknown`.
- **Followers:** per live container (worker and gateway), one `docker logs --follow --timestamps` child in a kill-on-close job, two reader threads (stdout, stderr), lines handed to the collector with their stream. At window start, or when a follower ends while its container still runs, it restarts from the beginning and the collector rebuilds that run's facts from nothing: the fold is deterministic, so no line counts twice. Re-reading at most 96 MiB once per restart is simpler than a cursor kept exact across log rotation.
- **Fold:** `stream::fold(&mut StreamFacts, &StreamLine)`, permissive parsing; the fields read are core's `STREAM_FIELDS`, which gate 5 turns into canary constants (AD-12). Kept: the first `system/init` (model, tools, MCP servers); hook events counted per event and outcome; `system/api_retry` error categories and the run of consecutive `authentication_failed`; the last `result` (subtype, error flag, cost, turns, duration, session id); the last 4 KiB of assistant text as `Untrusted<String>`. Lines over 1 MiB and non-JSON lines are counted and skipped. The entry point's stderr lines and the gateway's lines are parsed with their own core types.
- **Stream facts are untrusted.** Any process in the container runs as the same uid as the entry point and can write into the container's stdout, so a line can be forged. Hence only the first `system/init` counts, `result` is display only, and no limit relies on the stream to let a run continue; the stream can only stop a run sooner: `unexpected_tool` (a name of core's `WORKER_DISALLOWED_TOOLS`, the constant the vector reads, present in the init's tools), `hook_failed` (a hook event with exit code 126 or 127; gate 5), `auth_failed` (three `authentication_failed` in a row).
- **Limits:** the deadline (the entry point's `timeout`, the gateway's deadline, and the window stopping the worker at the deadline plus 60 s, applied at window start too); the budget (`--max-budget-usd`, gate 6); the gateway's limit (it exits 3 and the worker's next request fails); the output cap (64 MiB of stdout and stderr counted by the follower, `output_cap`); the engine's memory and process limits.
- **`FinishCause`** (closed enum): `exited`, `failed`, `deadline`, `budget`, `gateway_limit`, `output_cap`, `unexpected_tool`, `hook_failed`, `auth_failed`, `memory`, `stopped`, `engine_lost`. Core decides it from the engine's exit code and `OOMKilled`, the window's stop reason, the gateway's exit code and the result subtype, with a vector case per variant. What the run produced is a separate fact, `WorkshopOutcome` (`package`, `no_change`, `failed`, `not_collected`, `unknown`), decided after collection (AD-110). Core's `ENVIRONMENT_FAILURE_CAUSES` is the numerator of PRD section 7's Environment failures row: `gateway_limit`, `hook_failed`, and `failed` runs with at least one gateway refusal.
- **Stopping one run** (a limit or the deadline): `docker stop` the worker, record the reason, collect (AD-110). No flag is set (FR-14).
- **Reconciliation** at window start and after the engine comes back (core's `run::reconcile(rows, containers, now_ms)` returns actions): a running row with its container running is followed, and stopped if past its deadline plus 60 s; with its container exited, followed to the end, then collected; a `launching` row older than 2 minutes becomes `failed` and its objects are removed (AD-81's rule); a row whose container the engine no longer knows, with no package on the host, becomes `unknown`, never finished (principle 3); a labelled container with no row is listed as an orphan, removable on a click, never adopted. **An unreachable engine changes no row**: nothing becomes `unknown` until the engine answers and lacks the container.

### AD-110: Collection

- When the worker container has exited, however it ended, and its follower has drained: stop the gateway, drain its follower; create the `Packer` (the run's image and volume, no network, no credential, no mount) and start it attached, 120 s deadline. `officina-entry pack` empties `/work/.officina/out/` first, so nothing the worker wrote there survives, then writes:
  - `tip`: the commit id `refs/heads/officina/<name>` points at, or nothing when the branch is gone;
  - `work.bundle`, only when the clone holds at least one commit past the base (`rev-list --count <base>..<tip>` above 0): `git bundle create` of `refs/heads/officina/<name>` excluding the base commit. With no such commit there is no bundle to write (`git bundle create` refuses an empty range), and none is expected. A bundle over 64 MiB is deleted and reported `too_large`;
  - `manifest.json`: what it found (the entry point's recorded steps and exit code, the count of uncommitted files left in the clone), all of it untrusted; in Slice B the worker's memory folder (AD-115).
- **What a run produced** (`WorkshopOutcome`, closed enum) is decided on the host after AD-111's checks, separately from how the worker ended (`FinishCause`, AD-109). Core's `run::outcome(finish_cause, &PackageAssessment)` returns, with a vector case per row:

  | Worker ended | Host git finds past the base | Outcome |
  |---|---|---|
  | `exited` (exit 0) | no commit: the tip is the base commit or one of its ancestors, an object the host already holds | `no_change` |
  | any other cause | no commit | `failed`, with the cause |
  | any | at least one commit | `package` (review or quarantine, AD-111), with the cause shown |
  | any | the branch is gone, or a tip the host does not hold arrives without a bundle | `package`, quarantined (`branch_missing`, `bundle_invalid`) |
  | collection failed (packer error or deadline) | nothing to check | `failed` (`collect_failed`) |

  The count is host git's, on objects the host holds (AD-111, step 1); the packer's files only say where to look, and no stream line, report or manifest text can make a run `no_change`. A worker that resets its own branch to the base produces `no_change`: it controls its clone, so that is what the run produced.
- **`no_change`** is terminal: the run reaches state `no_change`, journals `workshop_finished` with that outcome, deletes `out/` and `verify.git` at once, never becomes a package, a quarantine or a Needs you card, never counts toward `reviews_pending`, and offers neither Import nor Discard. It is not a failure in PRD section 7's metrics (AD-121) and gives v0.7 the signal that a standing order found nothing to do on its base.
- Git in the packer runs with `-c core.hooksPath=/dev/null -c core.fsmonitor=false`, `GIT_CONFIG_NOSYSTEM=1` and `GIT_CONFIG_GLOBAL=/dev/null`. The clone's own configuration still applies: whatever it makes git run, it runs in a container with no network and no credential, and its output is verified on the host (AD-111).
- `docker cp officina-pack-<id>:/work/.officina/out <home>/workshop/runs/<id>/out`; the host sums the sizes and refuses over 64 MiB (`too_large`).
- Then the worker, the packer, the gateway, the volume and both networks are removed (`workshop_removed`, cause `collected`): the credential leaves the engine as soon as the package is on the host. `in/` is deleted. PRD FR-12 removed them after the verdict; this is earlier (PRD amendment 10).
- Why a packer instead of the entry point packaging at the end: a stopped, killed or out-of-memory worker never reaches its last step, and std cannot catch the engine's stop signal; a separate container packages every ending the same way.
- After Stop all nothing is collected until Resume (AD-116).

### AD-111: Host git and bundle verification

- **Resolution:** `OFFICINA_GIT_BIN`; else `InstallPath` from `HKLM\SOFTWARE\GitForWindows` joined with `cmd\git.exe`; else `git.exe` on `PATH`; `.exe` only. `host::git` (D-124) uses it for every call, which closes story 2.6's note for the tree fingerprint as well.
- **The v0.6 rule** (`GitRule::Workshop`), on every call that touches a package or the project for the workshop: D-22's flags, `GIT_OPTIONAL_LOCKS=0` and the caller's `GIT_*` variables removed (as `relay/tree.rs` does); `-c core.hooksPath=<home>/workshop/empty-hooks` (created by the window, refused unless empty), which also silences the `reference-transaction` hook; `-c transfer.fsckObjects=true -c fetch.fsckObjects=true`; `-c protocol.allow=never -c protocol.file.allow=always`, since every transfer is between local paths; `-c gc.auto=0 -c maintenance.auto=false -c fetch.writeCommitGraph=false -c fetch.recurseSubmodules=false -c submodule.recurse=false`; `CREATE_NO_WINDOW`; a 60 s deadline per call. Reading the commit identity (`config --get user.name`, `user.email`) is the one call that keeps the global configuration's meaning (FR-6).
- **Scratch repository** `<home>/workshop/runs/<id>/verify.git`: `git init --bare`, its `objects/info/alternates` naming the project's object directory (`rev-parse --git-common-dir`, then `objects`). It reads the project's objects and never writes them.
- **Checks,** in order; each failure is a `QuarantineCause`:
  1. the tip: `out/tip` must hold one commit id (else `branch_missing`). Without `work.bundle`, host git checks in the scratch repository, whose alternates are the project's objects, that `cat-file -e <tip>^{commit}` succeeds and `rev-list --count <base>..<tip>` is 0: then the run's outcome is decided by AD-110's table (`no_change` or `failed`) and checks 2 to 7 do not run; a tip the host does not hold gives `bundle_invalid`. With a bundle: at most 64 MiB (`too_large`), and its single reference must point at `out/tip` (`bundle_invalid`);
  2. core's `bundle::parse_header` on the first 64 KiB: a v2 or v3 signature; v3 capabilities limited to the repository's object format, `@filter` refused; exactly one prerequisite, the recorded base commit (`wrong_base`); exactly one reference, `refs/heads/officina/<name>` (`extra_ref`); anything else `bundle_invalid`;
  3. `git bundle verify` in the scratch repository (`bundle_invalid`);
  4. `git fetch --no-tags --no-write-fetch-head <bundle> refs/heads/officina/<name>:refs/heads/officina/<name>` into the scratch repository, with fsck (`bundle_invalid`);
  5. `merge-base --is-ancestor <base> <tip>` (`wrong_base`); `rev-list --count <base>..<tip>` at most 500 (`too_large`); a count of 0 here falls back to AD-110's table, like step 1;
  6. `diff --raw -z -M --no-ext-diff --no-textconv <base> <tip>` and `--numstat`: a path under a mount, compared with core's path normaliser, gives `mount_committed`;
  7. `rev-list --objects <base>..<tip>`, then `cat-file --batch`: at most 512 MiB inflated (`too_large`, against compression bombs), each blob, commit message and path scanned (AD-112);
  8. for display: commits (`log -z --format=%H%x00%s%x00%an%x00%at`), the diff capped as AD-84 (256 KiB, per-file offsets), the file list never capped.
- The host runs the commands on thread `workshop`; core decides from their outputs (`bundle::assess(&Outputs) -> PackageAssessment`, which carries `commits_past_base`). One `review_package` journal line for a package; none for `no_change` or `failed`, whose `workshop_finished` line says so (AD-120).

### AD-112: Credential scan

- `scan::needles(&[u8]) -> Needles` (core): the value raw; its standard and URL-safe base64 at each of the three byte alignments, minus the edge characters that depend on neighbouring bytes; its lowercase and uppercase hex. Search is a plain substring search per needle; no crate.
- **Scope:** every blob added or changed between base and tip, every commit message and path, the report tail and every stream text kept, the gateway's refused host names, the manifest, and in Slice B the memory proposals.
- **Found:** quarantine `credential_found`; its locations are recorded (commit and path, or "report", "commit message", "gateway line"), never the value; the value is replaced by `[credential]` in every text Officina stores; `out/` and `verify.git` are deleted at once, since they hold it; the card tells the author to revoke the token (README, gate 10).
- The texts are scanned for every outcome, `no_change` and `failed` included: a value found there is replaced the same way and the run's card carries the revoke advice, though there is no package to quarantine.
- Careless leaks only: a steered worker can send the value through `api.anthropic.com` (PRD section 1).
- Tests derive every form from a seeded value programmatically; no test types an encoded form.

### AD-113: Sensitive groups and approvals

- `sensitive::SensitiveGroup` (closed enum): `claude_config`, `git`, `ci`, `build_scripts`, `toolchain_config`, `editor_shell`, `officina`, `file_modes`, `project`, `file`. `SENSITIVE_PATTERNS` pairs each group with its patterns, seeded once from FR-11's list; from then on the constant is the list, this addendum does not restate it, and `tests/vectors/sensitive.json` holds a case per pattern under a coverage test that iterates the constant.
- **Matching** (`sensitive::groups`): every added, changed and deleted path and both sides of a rename, case-insensitively. A pattern without `/` matches the last segment; a pattern with `/` matches any trailing run of segments, so `.claude/**` matches at any depth; `**` spans segments, `*` stays within one. Patterns compile to anchored expressions with core's `regex`. Modes: a new symbolic link (120000) or an executable bit added gives `file_modes`; a gitlink (160000) gives `git`. `officina` is the floor file's path relative to the project when it lies inside. `project` is the project's `sensitive_paths`. With `review = "all"` every changed path is its own `file` group.
- **Approval:** each group shows its hunks, at most 1 MiB; a group over that cannot be approved in the panel, so such a package can only be discarded. `approve_sensitive(run_id, group_key, hunks_sha256)`, pointer only: the collector recomputes the group's hunks and refuses `hunks_changed` when their SHA-256 differs from the one displayed (AD-24's pattern), then records the approval and journals `sensitive_approved`. A `file` group's key is the file's index in the package's sorted list, so no worker text becomes a key.
- Import is enabled when the run is in state `review` and every group is approved.
- **Notice** (FR-11): from the journal, a template with 10 verdicts or more where one group appears in more than half of its packages gets a notice in the per-template table.

### AD-114: Import and discard

- **Import**, `import_package(run_id, tip)`, pointer only, on thread `workshop`:
  1. the run is in `review`, every group approved, and `tip` equals the recorded tip (`state_changed`);
  2. `rev-parse --verify --quiet refs/heads/officina/<name>` in the project: present refuses `branch_exists`;
  3. `git fetch --no-tags --no-write-fetch-head <scratch repository> refs/heads/officina/<name>`: objects only, no reference written;
  4. `cat-file -e <tip>^{commit}` (`package_changed` otherwise);
  5. `update-ref refs/heads/officina/<name> <tip> <zero object id>`, which creates the reference only if it does not exist, atomically;
  6. state `imported`, journal `review_imported` (branch, tip).

  The work tree, the index, `HEAD` and every other reference stay untouched; nothing is pushed.
- **Discard**, `discard_package(run_id, reason)`: `DiscardReason` holds the author's three reasons and two the system gives, `expired` (a package still undecided when its texts reach D-85's bound, AD-124) and `not_collected` (a stopped run removed without collection, AD-116). State `discarded`, journal `review_discarded`. A quarantined package offers Discard only.
- The run folder stays until AD-124's bound, so a discarded package can still be examined; `credential_found` deletes its package at once (AD-112).
- A `no_change` run (AD-110) has no verdict: nothing to import or discard, and it never enters the per-template table's decided counts.
- **Per-template table** (FR-12), from the journal: runs, `no_change` runs, failed runs, quarantined, imported, discarded by reason, imported over decided (`no_change` and failed runs are not decided), and AD-113's notice.

### AD-115: Memory as proposal (Slice B)

- `memory_name` names `<floor config dir>/projects/<memory_name>/memory` (AD-7's resolution; no literal path in the floor). It is mounted read-only at `/work/.officina/memory`; the entry point copies it into the worker's own memory folder and the vector sets `CLAUDE_CODE_PROJECT_DIR_NAME` to the same name (S8).
- The packer copies the worker's memory folder into `out/memory/`: text files only, at most 64 files of 256 KiB each.
- Core's `memory::proposals(host, worker)` lists added and changed files with a unified diff, as `Untrusted<String>`; deletions are not proposed. The host reads its own memory folder read-only; Officina never writes under a Claude config dir (D-80).
- `copy_memory_proposal(run_id, path)` returns the proposed text and the host path; the frontend copies the text on the click (AD-85's pattern). Proposals end with their package. The zero-write test covers the config dir around a run with memory.

### AD-116: Stop all, the flags, Resume, kept containers

- **Flags:** `<home>/workshop/stopped` and `relay/stopped` (AD-34), JSON `{"since_ms", "by"}` with `by` `window` or `cli`. A writer creates a missing flag and leaves a present one. AD-34's `is_stopped` reads both: a flag that cannot be read counts as set.
- **Stop all** (`stop_workshop`, in the header while any run is live and in the Workshop section, keyboard-reachable since it only reduces): write both flags; journal `relay_stopped` and `workshop_stopped` (scope, `by`); `docker ps -q --filter label=officina.managed=1`, then one `docker stop -t 3` with every id; terminate the job of any running build or battery and of the `workshop` thread's current engine call; live runs become `stopped` with `stopped_at_ms`. Containers are kept. The author's own sessions are untouched. Budget: under 10 s with the engine answering.
- **`officina stop --all`** (no authority): the same flags, `ps` and `stop` through the read half, one journal line; exit 0, or 1 when the engine could not be reached (the flags are written and the message says containers may still run). It stops every `officina.managed` container of the engine, of any floor: the CLI cannot know the window's floor (D-30), and stopping more is safe.
- **While the workshop flag is set:** no launch, build, inventory, battery or packer starts. Followers keep reading. Import and Discard stay possible: they start nothing.
- **Resume** (window only): a dialog lists the kept runs, each to collect (the default) or remove, and offers to resume the relay too (checked by default; the Relay view's own toggle stays, AD-34). Confirming deletes the workshop flag (and the relay's when checked), journals `workshop_resumed` with the counts, then collects (AD-110) or removes (state `discarded`, reason `not_collected`).
- **Kept containers** older than 7 days after `stopped_at_ms` are removed by the hourly pass, even while stopped, since removal only reduces (`workshop_removed`, cause `kept_expired`).
- Window start reads both flags before anything else.

### AD-117: The escape battery

- **Program:** `officina-entry battery --phase <1|2> <plan arguments>`, the plan built by core (`battery::plan`): the gateway's name, the test host, the decoy's address and the addresses it resolved, the battery mount, the expected environment names, the limits. Each attempt prints one `AttemptLine` (family, attempt, expected gateway refusal if any, outcome `refused`, `escaped`, `inconclusive` or `not_applicable`, detail).
- **Attempts** are core's `battery::ATTEMPTS`, seeded from FR-15's families, each entry naming its family and how it is judged; a coverage test iterates `BatteryFamily::ALL` and the constant. How each family is played, with std only:
  - egress: TCP to public addresses, an IPv6 socket, a UDP DNS query to a public resolver, resolution of an outside name, an ICMP datagram socket through `python3`;
  - gateway: an off-list host, an IP literal, ports 80 and 22 of the test host, an absolute-form `GET`, a malformed and an oversized request, the test host with a ClientHello naming another host (core's `gateway::tls::client_hello(name)` builds it), and two allow-listed test names the battery gateway resolves to loopback and to the engine host (its own `--add-host officina-loopback.test:127.0.0.1` and `officina-host.test:host-gateway`), both expected `private_address`;
  - host: the addresses the decoy resolved for `host.docker.internal` and `gateway.docker.internal`, and the battery networks' gateways, on ports 22, 80, 2375 and 2376; the gateway's ports other than 3128; the decoy's listening port on its other network; the engine socket paths;
  - files: writes in the battery mount, in `/`, `/usr`, `/opt/officina/bin` and `/etc`; the Docker Desktop host paths and `/mnt` children that must not exist; `mount`;
  - privileges: uid and gid, the five capability sets, `NoNewPrivs`, `Seccomp` from `/proc/self/status`; no setuid or setgid file on the root file system; `su` and `sudo` do not give uid 0;
  - credentials: no SSH key, agent socket, git credential store or helper, `.netrc`, cloud credential folder or engine configuration in the home or the image; the environment's names equal the expected set (AD-102);
  - resources: child processes until a spawn fails, at most the process limit; a child that touches memory past the limit is killed.
- **Containers per battery,** all labelled and removed afterwards: a decoy (`Decoy`, the run image, `officina-entry decoy`, on its own plain bridge: it resolves Docker Desktop's names, prints the addresses and listens); phase 1, gateway A (`BatteryGateway`, deadline in 10 minutes, limit 1000, the test allow-list) and the battery worker (`Battery`: the `Worker` vector with the battery command, the battery mount and the dummy credential) on a fresh network pair; phase 2, gateway B (deadline in 15 s, limit 20) and a second battery worker, which waits for the deadline, tries once (`deadline`), then sends 20 bad requests (`limit`); the window reads gateway B's exit code, which must be 3.
- **Judgement** (`battery::judge`, window): pass when every attempt is `refused`, each gateway-family attempt is matched, in order, by a gateway `refused` line with the expected cause, and gateway B exited 3. `escaped` and `inconclusive` fail. `not_applicable` is allowed only off Docker Desktop (the engine's platform name says which), for the attempts aimed at Docker Desktop's names. Journal: one `escape_attempt` per attempt, one `escape_battery` with the totals, image labels, `EngineKey` and Officina's version.
- **Validity** (`battery_stale`): the newest passing row with the same `EngineKey`, image id, gateway image id and Officina version, finished in the last 30 days.
- **Where it runs:** from the Workshop section (per image or all stale ones) or a stale card, on `workshop-build`; on CI (AD-125) on every change to the images, the programs or the vectors.
- **Upstream mode** (story review F51, stories/epic-6-decisions.md P-55): the battery's gateways run in direct mode. Steps 1, 2 and 4 of AD-103 are the same code in upstream mode; with `upstream_proxy` set, resolution and the private-address check are the proxy's (FR-4), so the battery's two loopback and engine-host names prove the direct path only.
- **Host family** (no Docker, PRD FR-15): crafted packages for each quarantine cause, each sensitive group, the credential's forms, oversized and forged stream lines and gateway lines, in CLI integration tests with the fake engine (AD-125). v0.5's injection battery gains workshop sections (AD-125).

### AD-118: Inventory

- **Input:** the project's mounted settings files (`settings.json` and `settings.local.json` in a mounted `.claude`) and the `.mcp.json` of the base commit (`git show <base>:.mcp.json`, since the clone carries the committed one). Core extracts each hook (event, matcher, command text) and each stdio MCP server's command; `inventory_key` is the SHA-256 of that list and the image id.
- **Run:** an `Inventory` container from the image, no network, the project's mounts read-only and linked at their places in `/work/<project>`, `CLAUDE_PROJECT_DIR=/work/<project>`, the list on stdin. `officina-entry inventory` finds each command's program with core's lexer (`relay::shell`, the gate's command-position rules, compiled into the entry point), expands `$CLAUDE_PROJECT_DIR` and nothing else, resolves the program on `PATH` or as a path, and resolves a script's interpreter from its first line (`#!/usr/bin/env x` resolves `x`). One result per command: `found` (with the path), `missing` or `dynamic` (not resolvable without running it).
- **When:** after each build, and at launch when the key changed (it takes seconds). Recorded per image and project; shown in the Workshop section and doctor. A `PreToolUse` hook whose program is `missing` refuses the launch (`hook_program_missing`); `dynamic` is a warning on the card, and FR-8's `hook_failed` still applies at run time.
- The image's own `ENV` names, from `docker image inspect`, are recorded at build for the battery's environment check (AD-117).

### AD-119: Registry migration 3

`MIGRATIONS[2]`, written whole in the first Epic 6 story that writes the registry, its SHA-256 pinned there (D-133's rule: a hole found later is migration 4). `VACUUM INTO registry/backup/pre-v3.db` first. `CHECK` lists come from the enums' `ALL`; causes that will grow carry none (D-133).

| Table | Key | Columns |
|---|---|---|
| `work_order` (added column) | | `runner TEXT NOT NULL DEFAULT 'host'`, `CHECK` from `Runner::ALL` (`host`, `workshop`) |
| `workshop_run` | `id` (= `work_order.id`) | `image`, `image_id`, `gateway_image_id`, `base_branch`, `base_commit`, `parent_run_id`, `uncommitted`, `plan_json`, `plan_sha256`, `credential_kind`, `deadline_ms`, `state`, `state_at_ms`, `cause`, `outcome`, `started_at_ms`, `finished_at_ms`, `exit_code`, `facts_json`, `texts_json`, `package_sha256`, `tip_commit`, `commits_past_base`, `files_changed`, `groups_json`, `quarantine_cause`, `opened_at_ms`, `verdict`, `discard_reason`, `decided_at_ms`, `branch`, `stopped_at_ms`, `removed_at_ms` |
| `workshop_approval` | (`run_id`, `group_key`), WITHOUT ROWID | `hunks_sha256`, `approved_at_ms` |
| `workshop_image` | `name` | `kind`, `project`, `image_id`, `labels_json`, `env_names_json`, `built_at_ms`, `outcome`, `build_tail`, `inventory_key`, `inventory_json`, `inventory_at_ms` |
| `workshop_battery` | `id` | `image`, `image_id`, `gateway_image_id`, `engine_key`, `officina_version`, `started_at_ms`, `finished_at_ms`, `outcome`, `attempts`, `refused`, `escaped`, `inconclusive`, `not_applicable`, `cause` |

- `CHECK` on `workshop_run.state` (`RunState::ALL`, `no_change` among its terminal states), `outcome` (`WorkshopOutcome::ALL`), `credential_kind`, `verdict`, `discard_reason`; `workshop_image.kind`, `outcome`; `workshop_battery.outcome`. `commits_past_base` is host git's count (AD-111), `NULL` until collection. None on `cause` and `quarantine_cause`, read back as unknown when no variant parses (D-133).
- `facts_json` holds the folded stream and gateway counts (typed in core, versioned); `texts_json` holds every untrusted text of the run (report tail, commit subjects, paths, refused hosts, memory proposals) and is the only column read back as `Untrusted<String>`, with `build_tail`. Indexes: `workshop_run(state)`, `workshop_run(finished_at_ms)`, `workshop_battery(image_id, finished_at_ms)`.
- No `setting` change. Slice B adds no column: memory proposals live in `texts_json`. No foreign key (D-133); retention deletes a `workshop_run` with its `work_order` in one transaction.

### AD-120: Journal kinds

`JOURNAL_LINE_VERSION` stays 1; `JournalKind` gains these variants, which earlier readers skip as unknown kinds. Untrusted text appears only where marked, cut and made printable.

| kind | fields |
|---|---|
| `workshop_started` | `run_id`, `name`, `template`, `project`, `values`, `image`, `image_id`, `base_branch`, `base_commit`, `parent_run_id?`, `hosts`, `mounts`, `deadline_ms`, `max_usd_cents?`, `credential_kind`, `argv` (the worker's `claude` vector), `plan_sha256`, `catalogue_hash`, `prompt_sha256`, `steps` (step, outcome, detail), `outcome` |
| `workshop_finished` | `run_id`, `template`, `project`, `base_branch`, `base_commit`, `cause`, `outcome` (`WorkshopOutcome`), `commits_past_base?` (host git's count), `tip?`, `exit_code?`, `duration_ms`, `cost_usd?`, `turns?`, `result_subtype?`, `hooks` (per event: ran, failed), `refused`, `stream_bytes`. Written once, when the outcome is known: after collection, at removal without collection, or at reconciliation (`unknown`) |
| `gateway_refused` | `run_id` or `battery_id`, `host` (untrusted, printable, 253 characters), `port`, `cause` |
| `gateway_summary` | `run_id`, `allowed` (per host, counts), `refused`, `exit_code`, `upstream?` |
| `review_package` | `run_id`, `bundle_sha256?`, `commits`, `files`, `insertions`, `deletions`, `groups`, `outcome` (`review`, `quarantined`), `cause?`, `credential_locations?` (count); only for outcome `package` |
| `review_opened` | `run_id` (first Receive only; PRD section 7's Review cost starts here) |
| `sensitive_approved` | `run_id`, `group`, `hunks_sha256` |
| `review_imported`, `review_discarded` | `run_id`, `branch` and `tip`; or `reason` |
| `workshop_stopped`, `workshop_resumed` | `by`, `containers`; or `collected`, `removed` |
| `workshop_removed` | `run_id`, `cause` (`collected`, `kept_expired`, `resume`, `launch_failed`, `orphan`) |
| `workshop_image_built` | `image`, `image_id?`, `kind`, `outcome`, `duration_ms`, `inventory` (counts) |
| `escape_attempt` | `battery_id`, `image`, `family`, `attempt`, `outcome`, `detail` |
| `escape_battery` | `battery_id`, `image`, `image_id`, `gateway_image_id`, `engine_key`, `officina_version`, totals, `outcome` |

The credential's value never appears; `review_package` counts its locations, the registry keeps them (AD-112).

### AD-121: Payloads, IPC commands, CLI

- **`Board`:** a card kind `workshop`, key `ws:<run id>`, built by core's `view::workshop_cards` from the runs: Working while `launching`, `running`, `stopping` or `collecting` (an "isolated" chip); Needs you while `review` or `quarantined` ("review", counted in `waiting_count`); To resume while `stopped` (not counted); Done for 24 h after a verdict, `no_change` (badge "no change", never Needs you), `failed` or `unknown`.
- **Metrics** (PRD section 7, read by core's `view` from the journal's `workshop_finished` outcomes): a `no_change` run counts as a run in Use's run count, Workshop share and Environment failures' denominator, never in Environment failures' numerator, Review cost or Acceptance (it has no verdict). `failed` runs are failures; `no_change` is not. Cases join `tests/vectors/classification.json`, whose coverage test sees the new kind and states. `Board.workshop_live: u32` feeds the header chip. `BOARD_SCHEMA` goes up by one for v0.6 in the first Epic 6 story that changes `Board` (P-13's rule).
- **`WorkshopView`** (`WORKSHOP_VIEW_SCHEMA = 1`, built in core, event `workshop://updated`, emitted on byte change, AD-5): the engine state and key; both flags; images (state `absent`, `building`, `ready`, `mismatch`, `failed`; labels compared; inventory per project; the battery's last outcome, age and validity); live, kept and orphan runs; the per-template table and notices; this week's workshop share and environment failures; the limits. Separate from `OrchestratorView` so that followers' updates do not churn it.
- **`ReviewView`** (request and response, asked again when the run changes): the report tail, the run's facts, commits, files with their groups, the diff, groups with hunks, `hunks_sha256` and approval state, memory proposals, import readiness with its reasons, the quarantine cause and credential locations. For a `no_change` run it carries the facts, the report tail, the base and the tip, and `nothing_to_review: true`, with no commit, diff, group, Import or Discard. Every worker text is `Untrusted<String>` (AD-74).
- **`WorkOrderCard`** gains `workshop: Option<WorkshopCardFacts>` (AD-107); `OrchestratorView`'s history rows gain the runner and the run's state; `ORCHESTRATOR_VIEW_SCHEMA` goes up by one for v0.6. The briefing (AD-83) gains live runs, packages awaiting a verdict, and quarantined, failed and unknown runs of the last 24 h; `officina mcp` serves them through `briefing` and `board`, and no tool is added.
- **Commands,** in `build.rs` and `capabilities/default.json` under `every_declared_command_is_allowed`, thin wrappers over `AuthorRequest` with a deadline (D-92):
  - read: `get_workshop`, `open_review(run_id)` (journals `review_opened` once);
  - author: `build_images`, `run_battery(image?)`, `stop_workshop`, `resume_workshop(choices, resume_relay)`, `remove_run(run_id)` (kept or orphan), `approve_sensitive(run_id, group_key, hunks_sha256)`, `import_package(run_id, tip)`, `discard_package(run_id, reason)`, `copy_memory_proposal(run_id, path)` (Slice B); amended: `edit_card` (`Base`), `launch_work_order(card_id, prompt_sha256, plan_sha256?)`.
  - Engine calls never run on the collector thread (AD-98).
- **CLI:** `officina stop --all` (AD-116) and `officina workshop --json`: `WorkshopView` from the read-only registry and the engine's read half when it answers; exit 0, 3 when the registry cannot be read (D-47), 64 on usage, never 2. It feeds the e2e fixtures (D-25's pattern). `readme.rs` (D-34) covers both. `quota_doctor.rs` covers both (D-80).

### AD-122: Frontend

- **Header:** a neutral chip "N in the workshop" (AD-55: quiet surface, never `--color-attention`, not in the title or the overlay) and Stop all while any run is live; Stop all is keyboard-reachable.
- **Orchestrator view:** `WorkshopSection.svelte` holds `ImageTable.svelte` (states, inventories, Build images), `BatteryStatus.svelte` (per image, Run escape battery), `RunList.svelte` (live, kept and orphan runs; Stop all; remove), `ResumeDialog.svelte`, `TemplateTable.svelte` (FR-12's table, notices, share, environment failures). Work order history shows workshop runs with their state.
- **Card:** `WorkOrderCard.svelte` gains the workshop chip, a base `<select>` from the closed list (no free text), the tip, the uncommitted count, hosts, mounts marked read-only, the image, minutes, budget, parent run, readiness causes, and a pointer-only Launch disabled while any cause holds.
- **Review panel:** `ReviewPanel.svelte` takes AD-84's side slot, 640 px wide for diffs: the report as text in a `<pre>`, the facts, commits, `DiffView.svelte` (AD-93), `SensitiveGroup.svelte` per group (hunks inline, pointer-only Approve), `MemoryProposals.svelte` (Slice B, Copy), a pointer-only Import showing the branch and tip, and Discard with three reason buttons (keyboard-reachable). Receive on a workshop card opens it. A `no_change` run, opened from the Done card or the history, shows "Nothing to review: the work branch has no commit past <base> @ <commit>" above its facts and report, and no action.
- **Board:** `WorkshopCard.svelte` for the `workshop` kind. **Scene** (Slice B): `SceneInput.workshop` is nullable (AD-68); a live worker sits at a desk of its project's workshop with a neutral glass outline; a package awaiting review gives it the waiting halo (AD-65). AD-70's truthful-motion test stands.
- **Store:** `lib/workshop.svelte.ts` (view, review panel, approvals in flight). Every run text renders as text (`svelte/no-at-html-tags`); Approve, Import, Launch, Build images, Run escape battery and Resume act on a pointer click only (D-28), since each starts a container or writes the repository.

### AD-123: Doctor

- **Slice A:**
  - `docker.exe` and `git` resolved, with the source (HKLM, `PATH`, override), not shims; `DOCKER_HOST`, `DOCKER_CONTEXT` and `DOCKER_CONFIG` named when set; the engine reachable, Linux, its `EngineKey`; the Linux programs present;
  - images present and matching (AD-99), their inventories, the battery's last outcome and age per image, `battery_stale` named;
  - `upstream_proxy`: the `upstream` state of the newest gateway `started` line (no container is started by doctor); `ca_file` readable and its SHA-256 equal to the images' label;
  - the credential present (never read for display) and its kind;
  - per project: each mount present and ignored, and any mount ignored only through untracked rules (`check-ignore -v` names the rule's source; `.git/info/exclude`, `core.excludesFile`, or a `.gitignore` that `ls-files --error-unmatch` does not list); each mounted settings file with an `env` block or `apiKeyHelper` (a warning, names only); the programs of mounted hooks and of `.mcp.json` against the inventory, `PreToolUse` ones as failures; the Dockerfile present and approved;
  - both flags; kept containers and their age; orphan containers and networks; the image's Claude Code version against the host's (warning); `max_workers` times `memory_mib` against the engine's memory (warning);
  - the two notes of FR-18: the WSL 2 shared kernel (the cautious reading; section 1, item 1 shows one kernel version for both, consistent with it, not proof), and the SSH agent advice (`ssh-add -c`).
- **Slice B:** memory folders present; the browser base present and matching.
- Doctor runs the read half only; it starts no container.

### AD-124: The Officina home and retention

| Path | Writer | Content |
|---|---|---|
| `workshop/stopped` | window, CLI | AD-116 |
| `workshop/runs/<id>/in/` | window | base bundle, prompt, plan; deleted at collection |
| `workshop/runs/<id>/out/`, `verify.git/` | window | the package, the scratch repository |
| `workshop/build/<image>/` | window | build contexts, recreated per build |
| `workshop/battery/mount/` | window | the battery's read-only mount, recreated per battery |
| `workshop/empty-hooks/` | window | empty; refused unless empty (AD-111) |

- D-13's protected directories gain `<home>/workshop`, so a guarded session's command naming it is denied, with the relay's limits (AD-25).
- **Retention** (the hourly pass, D-134's pattern): `texts_json` and `build_tail` cleared `EXCERPT_TTL_MS` after the run's launch; a run folder deleted `RUN_FOLDER_TTL_MS` after launch, that constant derived as 30 days minus the hourly interval, since run folders are not in registry copies; a package still undecided at `EXCERPT_TTL_MS` discarded `expired`; `workshop_run` with its `work_order` deleted at 90 days (D-134), `workshop_battery` rows at 90 days; kept containers at 7 days (AD-116). D-85's bound test gains both derivations.

### AD-125: Test strategy

| Layer | What | Where and how |
|---|---|---|
| core unit | phase 2 (a failing fixture per new `IssueKind`); canonical format (golden hash, previous-format fixture); container vectors per role and the worker and battery comparison; engine call vectors and output parsers; gateway request, host, allow-list, address ranges, ClientHello parsing (byte fixtures), limiter; bundle headers; sensitive patterns; scan needles; stream fold (forged and oversized lines); finish causes; AD-110's outcome table, a case per row; reconciliation; launch refusal order; battery judgement and validity; inventory extraction; review and workshop views | CI, Windows and Linux, no Docker; vector sets `workshop.json`, `gateway.json`, `bundle.json`, `sensitive.json`, `stream.json`, `battery.json`, each with a coverage test over its enums' `ALL` and constants; formatter-exempt |
| type proofs | `WorkshopLaunch` and the run plan only from a validated order (`compile_fail`) | `cargo test --doc` |
| gateway | the real `officina-gateway` built natively on loopback: upstream mode against a test CONNECT proxy (allow, every refusal reachable without the internet, crafted ClientHello, oversized, deadline, the limit's exit 3, line format); direct mode refusing `localhost` (`private_address`) | CI, Windows and Linux, no Docker, no internet |
| entry point | `officina-entry run` and `pack` on Linux with a fake `claude` script on `PATH` and real git in a temp folder: clone, branch, exclude, links, timeout exit 124, exit codes, bundle and manifest, `out/` emptied first; `pack` with no commit past the base, and with the branch reset behind it, writes `tip` and no bundle | CI `core-linux`, no Docker |
| fake engine | new test bin `officina-fake-docker` (fake-claude pattern, D-86 sidecar, D-129 log format with variable names only): state for networks, volumes, containers, images and labels; modes for `version` (linux, windows, down), `logs` fixtures per role, `cp` from fixture packages, stop latency, `build` recording labels, attached starts printing fixture lines | `crates/officina` |
| launch to verdict | with the fake engine and real git on fixture repositories (D-23): zero `create` and `start` before the click, exactly one gateway and one worker after, their vectors equal to the golden rebuild; the credential's value reaches only the worker's `create` (the fake logs names and a presence flag); limits and causes; collection; `no_change` (exit 0, packer out with the base as tip and no bundle): state `no_change`, one `workshop_finished` with that outcome and no `review_package`, no Needs you card, not counted in `reviews_pending`, `out/` deleted, the review view says nothing to review and refuses Import and Discard; the same out with a non-zero exit gives `failed`; a report tail and stream text claiming "no changes" while the bundle holds a commit gives a package (model text decides nothing); a tip the host does not hold without a bundle gives `bundle_invalid`, a missing tip `branch_missing`; every quarantine cause from crafted packages; credential forms (quarantine, placeholders, files deleted); approvals with stale hunks refused; Import adds exactly one reference; Discard; Stop all within 10 s and flags surviving a restart; nothing starts while set; `stop --all` never removes a flag; reconciliation (running, exited, gone, orphan, engine down); retention | host and CLI integration, `window-authority` through the dev-dependency |
| injection battery | `injection.json` gains `workshop_card` (forged and superseded cards, prompt and plan hash mismatch, a base outside the list, keyboard launch, flag set) and `workshop_import` (keyboard Import, tip mismatch, unapproved group, stale hunks, quarantined package, existing branch); the fake engine counts starts | D-147's harness |
| mutation | `mutant-accept-extra-ref` (bundle check skipped) and `mutant-skip-group-approval` (Import without approvals) join D-148's job | CI `mutants` |
| zero write | `quota_doctor.rs` fingerprints the Claude config dir and the project repository (references, index, work tree) around a launch, collection, quarantine, discard and memory run with the fake engine, `stop --all` and `workshop --json`; Import changes only `refs/heads/officina/<name>` and objects | CLI integration (D-80) |
| credential | a seeded value searched in every file under the test home, every journal line, registry row, log and IPC payload after a full run | host integration |
| real engine | job `workshop` (ubuntu-latest, on changes to `docker/**`, `crates/officina-container/**`, `crates/officina-core/src/workshop/**`, the workshop vectors, and weekly): musl build, hadolint, base and gateway images built on the runner's engine, the full battery through `examples/battery_ci.rs` with a dummy credential, and one allowed and one refused connection through the real gateway | CI with a Linux engine |
| frontend | Vitest: card facts and base select, review texts inert (markup in report, subjects, paths, hosts), Approve and Import ignore `detail === 0`, Stop all keyboard-reachable, chips never use the attention colour (Playwright, D-163); Playwright with fixtures from `examples/workshop_fixture.rs` (fake engine, real git) through `officina workshop --json` and `board --json` | `app/e2e/workshop.spec.ts`, `review.spec.ts` |

Manual, on the workstation, recorded in research notes: AD-126's gates; the battery on Docker Desktop before the first launch and after any engine, image or Officina change; Stop all once; one run per owner project with hooks; section 7's Use, Workshop share, Environment failures, Review cost and Acceptance rows.

### AD-126: The gates of PRD section 8, exact procedures

Each records an E-entry from E26 on (E18 to E25 are Epic 5's, D-125), in FR-0's order, with the date, versions and who ran it. An agent runs the probes that need no credential and mounts no real folder; the owner runs the rest. From Git Bash, `MSYS_NO_PATHCONV=1` (section 1, item 14). Every probe removes its containers, networks, volumes and images.

| # | Procedure | Pass | On failure |
|---|---|---|---|
| 7 | Owner: record the account the work floor uses (personal or an employer plan); whether a `setup-token` token driving workers is ordinary use under it and, for an employer plan, the employer's position; whether an API key is available; whether Docker Desktop's subscription terms hold where it runs | a token accepted, or an API key available | token refused, key available: floors use `auth = "api-key"`; neither: v0.6 stops at Slice 0 |
| 0 | On the machine and network the work floor uses. Host: proxy variable names in a host `claude` session's environment; Windows proxy settings (`ProxyEnable`, presence of `ProxyServer` and `AutoConfigURL`); `api.anthropic.com`'s certificate issuer seen from the host. Container: as uid 1000 on the default network, `curl -v https://api.anthropic.com`, issuer and outcome. With a PAC file, the proxy it names for that host | direct with the public chain (no key set); or an unauthenticated proxy (`upstream_proxy`) and, when the issuer is not public, `ca_file`, both checked again through the gateway at gate 2 | an authenticating proxy: the workshop cannot run on that network (PRD section 5). **Verified on this workstation's network** (section 1, item 4): direct, public chain; the owner states whether it is the work floor's |
| 9 | Install the musl target (`rust-toolchain.toml`), `cargo build --release -p officina-container --target x86_64-unknown-linux-musl`; build `officina-gateway:local`; run it with AD-102's `Gateway` vector on a probe network pair and let a probe container `curl` an allowed host through it and an off-list host | both programs build; the gateway runs in an empty image, resolves names (the engine's resolver file is mounted) and logs `allowed` and `refused` lines | AD-100's fallback, listed for the owner's approval |
| 1 | Build `officina-workshop:local` (record the installer's hosts and its checksum check, the image size, the labels); build a probe project image `FROM officina-workshop:local` with the default builder (it must not pull); run AD-102's `Worker` vector by hand with a probe command instead of `officina-entry run`, mounting read-only a git-ignored folder of an owner project inside the synced tree (owner): read a file, try a write and a create (expect "Read-only file system"); `docker cp` in and out; close the starting console and see the container keep running; the `local` log driver options accepted. Record engine, Docker Desktop and WSL versions | all of these | a writable mount stops Slice A; a failing copy moves `in/` to a read-only bind mount of the run's folder; a pulled base: builds pass `--build-context` naming the local image. **Verified in part** (section 1, items 7 to 11) on a stand-in image and a temporary folder |
| 2 | The battery's egress and host families (AD-117) against the real gateway on Docker Desktop; then, when gate 0 set one, a CONNECT through `upstream_proxy` | every attempt refused; the allowed host reached through `HTTPS_PROXY` | outside names resolving: `--dns 0.0.0.0` and a fixed gateway address (AD-104). **Verified with a stub gateway** (section 1, item 5) |
| 3 | Owner: `claude setup-token` in a terminal with a browser; store it with doctor's `cmdkey` line; Build images; launch the real worker from the window on a scratch repository (git-ignored `.claude` with no hook) with a prompt that edits a file, runs `git status` and commits. Record: no dialog or login prompt, exit 0, the commit in the package, every host the gateway allowed and refused, and `claude --help` in the image listing `--permission-prompts`. This is framing step 0 item 7, its own E-entry | all of these | a missing host: add the recorded hosts (first `platform.claude.com`) to `CLAUDE_CODE_HOSTS` and run again; a dialog or a blocking prompt stops Slice A and reopens FR-7 |
| 4 | Owner: a scratch repository whose git-ignored `.claude/settings.json` declares, for `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop` and `SessionEnd`, a hook appending one line to `$HOME/hooks-gate.log` (on the work volume: `/tmp` is a tmpfs, gone once the container stops); run a worker; copy the log out of the stopped container before collection and compare it with the stream's hook events. In one real host session at work, record each instruction file's and hook's source (`/memory`, `/hooks`). Run one worker per owner project that declares hooks, with its project image | every configured event's hooks ran in the worker, and the stream reports the events it can | a hook that does not run, or a source the mounts cannot carry that a project relies on, is a finding against FR-9 before Slice A closes |
| 5 | On gate 3's run, plus a run whose `PreToolUse` hook names a missing program: the fields of `STREAM_FIELDS`, the hook events' exit codes | all present; they become canary constants | a missing field reads unknown; without exit codes, FR-8's `hook_failed` goes and `hook_program_missing` is the only check |
| 6 | Owner: `max_usd = 0.01` on a multi-step prompt; record the result subtype and the reported spend | the run stops on the budget | `max_usd` is dropped; the deadline remains |
| 8 | Slice B: a project image `FROM officina-workshop-browser:local` runs that project's Playwright suite with Chromium in a worker against the project's own server, and a page on an off-list host fails with a `gateway_refused` line; record the Chromium flags needed | both | the browser base leaves v0.6 and returns with v0.8 |
| 10 | Owner: find where a `setup-token` token is revoked | recorded; the README states it beside the quarantine advice | the README says it is unknown and advises an API key for the workshop |

Question 11 is not a gate (FR-7 disallows WebSearch either way); questions 12 and 13 are measured over two weeks of use.

### Slices

- **Slice 0:** gates 7 and 0 first; then 9, 1 and 2 (an agent can run them now); then 3 to 6 (the owner, with a token). No Slice A story is implemented before gates 0 and 7 are recorded (FR-0). Gate 8 before Slice B's browser.
- **Slice A:** AD-97 to AD-114, AD-116 to AD-121 with AD-119's migration 3, AD-122 without the scene and memory proposals, AD-123's Slice A part, AD-124, AD-125's rows for what it lands.
- **Slice B:** AD-115, the browser base of AD-99, `memory_name` (AD-101), the scene and memory proposals of AD-122, AD-123's Slice B part.

### FR to AD

| FR | Built by |
|---|---|
| FR-0 | AD-126, Slices |
| FR-1 | AD-98, AD-99, AD-100, AD-118 |
| FR-2 | AD-101 |
| FR-3 | AD-102, AD-104 |
| FR-4 | AD-103, AD-104 |
| FR-5 | AD-105 |
| FR-6 | AD-107, AD-108 |
| FR-7 | AD-102, AD-106, AD-108 |
| FR-8 | AD-109 |
| FR-9 | AD-108, AD-118, gate 4 |
| FR-10 | AD-110, AD-111, AD-112 |
| FR-11 | AD-113 |
| FR-12 | AD-114 |
| FR-13 | AD-115 |
| FR-14 | AD-116 |
| FR-15 | AD-117, AD-125 |
| FR-16 | AD-99, gate 8 |
| FR-17 | AD-121, AD-122 |
| FR-18 | AD-123 |

### Touch points with v0.2 to v0.5

- `core/floor.rs` (`RELAY_KEYS`, raw `workshop` tables), `core/relay/catalogue.rs` (phase 2 with `CatalogueFiles`, the format bump, the diff), `core/relay/journal.rs` (kinds), `core/board.rs` (card kind, `workshop_live`, schema), the v0.5 `orders` module (card facts, `EditCard::Base`, `LaunchRefusal` variants, the history's runner), `core/relay/shell.rs` (reused by the inventory; it must keep compiling for the musl target).
- `host/git.rs` (D-124: resolution, `GitRule::Workshop`; the tree fingerprint moves to the resolved binary), `host/relay/home.rs` (the `workshop` folder, D-13's protected directories), `host/relay/run.rs` (the catalogue files for its hash), `platform/windows.rs` (HKLM string read, the credential read), `host/watch.rs` (workshop state, followers' channel, `WorkshopView`, reconciliation), the registry schema (migration 3), the hourly pass, doctor.
- `officina/main.rs` (`stop`, `workshop`, usage), `quota_doctor.rs`, `readme.rs`, `injection.rs`; the app's `main.rs`, `build.rs` and capabilities; `ipc.ts`, `e2e-mock.ts`, `Header.svelte`, `OrchestratorView.svelte`, `WorkOrderCard.svelte`, the side slot of `MeetingPanel.svelte`, the board columns, `lib/scene/*` (Slice B); `e2e-fixtures.mjs` (kind `workshop`, D-181's index).
- `rust-toolchain.toml`, `.cargo/config.toml`, `.github/workflows/ci.yml` (musl build on `windows`, the `workshop` job), `release.yml`, `app/src-tauri/tauri.bundle.conf.json` (resources).

## 3. Amendments to earlier decisions

- **Spine, principle 2:** a note after AD-20: principle 2 governs sessions running as the author; a workshop worker's permission mode is AD-106's.
- **AD-1:** a fourth crate, `officina-container`, built for `x86_64-unknown-linux-musl`; `officina-core` must stay pure Rust with no C dependency, since the musl build compiles it.
- **AD-3:** the collector never runs `docker` or the workshop's `git`; threads `workshop` and `workshop-build` and one follower per live container (AD-98).
- **AD-7:** overrides `OFFICINA_DOCKER_BIN`, `OFFICINA_GIT_BIN` and `OFFICINA_LINUX_BIN_DIR`.
- **AD-18:** the `windows` job builds the musl programs; a `workshop` job with a Linux engine; the release ships `linux/`.
- **AD-21:** `window-authority` also gates the engine's launch half, builds, the battery, collection, verification, approvals, Import and Resume.
- **AD-23:** the workshop tables are phase 2; their files are read by the host for phase 2 (AD-101).
- **AD-24, AD-76:** the canonical format goes up by one for v0.6 (D-131's rule).
- **AD-25:** the `workshop` folder (AD-124); D-13's protected directories gain it.
- **AD-32:** AD-120's kinds.
- **AD-34:** the CLI may create either stop flag, never delete one; Resume of the workshop may resume the relay too.
- **AD-36:** the credential read is built for v0.6 whether or not HTTP actions are.
- **AD-44, D-85, D-134:** AD-124's retention and its two derivations.
- **AD-73, D-128:** four `Command::new` sites; the engine's launch half carries the marker.
- **AD-78, AD-81:** workshop cards (AD-107); binding skips workshop rows.
- **AD-82, D-133:** migration 3 (AD-119), `work_order.runner`.
- **AD-83, AD-92:** briefing sections; `WorkOrderCard.workshop`; `ORCHESTRATOR_VIEW_SCHEMA` up by one.
- **AD-84, AD-93:** the review panel shares the side slot, 640 px wide.
- **AD-95, D-147, D-148:** injection sections and two mutants.
- **Story 2.6's note** (`git` from `PATH`): closed by AD-111's resolution.

**Dependencies for approval.** No crate is added.

| Item | Where | Licence | Why |
|---|---|---|---|
| windows-sys feature `Win32_System_Registry` | host, existing crate | MIT/Apache-2.0 | HKLM reads for `docker.exe` and `git` (AD-98, AD-111) |
| windows-sys feature `Win32_Security_Credentials` | host, existing crate | MIT/Apache-2.0 | AD-36's credential read, approved then for HTTP actions, needed now whatever 2.11's fate |
| Rust target `x86_64-unknown-linux-musl`, linked by `rust-lld` | workstation, CI, release | MIT/Apache-2.0 | AD-100 (gate 9) |
| `debian:trixie-slim` pinned by digest; bash, ca-certificates, coreutils, curl, git, python3, python-is-python3 | built locally | Debian (DFSG) and the packages' own | AD-99 |
| uv, pinned release with checksum | built locally | MIT or Apache-2.0 | AD-99 |
| Claude Code native build at the host's version, unmodified | built locally | Anthropic's terms (S3) | the worker |
| Chromium and its libraries (Debian), Slice B | built locally | BSD-3-Clause and others | AD-99, gate 8 |
| hadolint, pinned release with checksum | CI `workshop` job only | GPL-3.0, run as a tool, not linked or shipped | the repository's Docker rule |
| A Rust build image pinned by digest, only if gate 9 fails | built locally | MIT/Apache-2.0 | AD-100's fallback |

Ruled out: a Docker API crate (an async runtime and the named pipe), `tar` and `flate2` (`docker cp` handles archives), `rustls` and `hyper` (the gateway never terminates TLS), `libc`, `nix` and `signal-hook` (the packer replaces signal handling), `memchr`, `globset` and `ipnet` (core's `regex` and a range table suffice), `git2`.

## 4. PRD amendments

1. **FR-1, FR-4, section 6.1.2.** Replace "a static Linux build of `officina` running `officina gateway`" and its build path with: "`officina-gateway`, a static Linux program of the crate `officina-container`, which also builds `officina-entry`, the workshop image's entry point and battery: `cargo build --release -p officina-container --target x86_64-unknown-linux-musl`. A crate of its own because `officina` links SQLite's C sources, which a musl build cannot compile without a musl C toolchain."
2. **FR-1.** Append: "Images are tagged `:local`. A project Dockerfile's final stage descends from `officina-workshop:local` or `officina-workshop-browser:local`, checked before the build and by the inherited `officina.base` label after it; its build context is the Dockerfile's own folder. The base also carries `curl`, strips setuid and setgid bits and points Python, Node and uv at the system certificate store. A launch compares every label but the Claude Code version, which doctor compares."
3. **FR-2.** Replace the `mounts` rule's "each an existing folder that `git check-ignore` reports ignored; a tracked or missing folder is an issue and leaves the list" with "existence and ignore status are checked at launch and by doctor (`mount_missing`, `mount_not_ignored`)". Append: "A workshop project's name follows the action name rule. A host's last label holds a letter. `max_usd` has at most two decimals. A Dockerfile's folder holds at most 32 files and 1 MiB."
4. **FR-3, FR-7.** Replace "an environment that is exactly FR-7's list plus the credential" with "an environment of FR-7's names, the credential's name and the approved image's own `ENV` names". In FR-7, replace "`HOME`, `PATH`" with "`HOME` on the work volume; `PATH` from the image", set the proxy variables in both cases, add `DISABLE_AUTOUPDATER=1` and `::1` to `NO_PROXY`.
5. **FR-4.** Replace "attached to that network and to the engine's default one" with "attached to that network and to a bridge network of its own run". Replace the summary at exit with: "one line per allowed connection, which the window counts per host into `gateway_summary`". Append: "A name with any non-public address is refused. An Encrypted ClientHello is recorded on the allowed line, not refused. The gateway's first line says whether the upstream proxy answered."
6. **FR-6.** Add the causes `workshop_off`, `mount_missing`, `mount_not_ignored`, `mount_path_unsupported`, `project_missing`, `plan_changed` and, in step 1, `base_too_large` (1 GiB). Append: "Launch sends the card's plan hash with its prompt hash. Officina's own folders sit under `/work/.officina/`."
7. **FR-8.** Append: "Stream lines are untrusted: only the first `system/init` counts and a stream fact can stop a run, never keep it going. Causes add `memory` and `engine_lost`. A restarted window re-reads a live container's output from the start. An engine that does not answer makes no run unknown."
8. **FR-10.** Replace "the entry point writes into `/work/out`" with "a packer container, from the run's image and volume with no network and no credential, empties the out folder and writes the work branch's tip and, when the branch holds commits past the base, its bundle". Add to the git rule: "`core.hooksPath` empty, `fetch.fsckObjects`, `protocol.allow=never` with `protocol.file.allow=always`, `gc.auto=0`". Add the quarantine cause `branch_missing`. Append: "A bundle header with any prerequisite but the base, any reference but the work branch or a `@filter` capability is refused; blobs inflate to at most 512 MiB; `credential_found` deletes the package at once. **No change:** when the worker exited 0 and host git, on objects the host holds, counts no commit past the base, the run ends `no_change`: journaled in `workshop_finished`, never a package, never quarantined, never a Needs you card, nothing to review, import or discard. With another ending and no commit, the run is `failed` with its cause. Neither model text nor the packer's files decide it."
9. **FR-11.** Append: "An approval binds to the SHA-256 of the hunks displayed. A group whose hunks exceed 1 MiB cannot be approved in the panel."
10. **FR-12.** Replace "After either verdict the container, its volume, its network and its gateway are removed" with "Once the package is on the host, the containers, the volume and the networks are removed". Append: "Import fetches the objects, then creates the branch with a zero old value, so it never moves an existing reference. A package undecided when its texts reach the 30-day bound is discarded `expired`."
11. **FR-14.** Append: "`officina stop --all` stops every container Officina labelled on the engine, whatever its floor. While stopped, no build, battery or packer starts; Resume lists the kept runs, each to collect or remove, and can resume the relay too."
12. **FR-15.** Append: "The battery runs in two phases with a decoy on another network; its gateway resolves two test names to loopback and to the engine host. An inconclusive attempt fails the battery. Validity also keys on the gateway image and the engine's kernel."
13. **FR-17.** Append: "The Workshop section has its own payload. A stopped run sits in To resume. `officina workshop --json` prints the section read-only."
14. **FR-18.** Replace "`upstream_proxy` reachable from the engine's default network" with "the upstream state the newest gateway reported". Append: "`DOCKER_HOST` and `DOCKER_CONTEXT` named when set; `max_workers` times `memory_mib` against the engine's memory."
15. **Section 7.** Review cost: "from `review_opened`". Environment failures: "causes of `ENVIRONMENT_FAILURE_CAUSES`". Append: "A `no_change` run counts as a run in Use, Workshop share and Environment failures' denominator; it is not a failure, has no verdict, and Review cost and Acceptance leave it out. v0.7 reads it to skip or slow a standing order whose base did not change." FR-12's per-template table gains `no_change` and failed run counts.
16. **Section 8.** Question 0: "Its container half was verified on 2026-10-28 on the workstation's network (addendum section 1); the owner states whether that network is the work floor's." Question 1: add "build the base image, recording the installer's hosts and checksum check, and a project image from it without a pull; the `local` log driver". Question 4: replace "a file under `/tmp`" with "a file under `$HOME`, on the work volume".

## 5. Risks

| # | Risk | Mitigation |
|---|---|---|
| R47 | A Docker Desktop update changes networking or name handling under a battery that passed | `EngineKey` in the battery's validity; `battery_stale` refuses launches |
| R48 | A worker fills the engine VM's disk (no volume quota on this storage driver) | deadline, output cap, removal after collection, kept containers at 7 days; doctor shows orphans; accepted |
| R49 | The credential lingers in kept containers' configuration | removal right after collection; 7 days for stopped runs; `workshop_removed` lines |
| R50 | A process in the container forges stream lines | facts untrusted; only the first init counts; stops only shorten a run; limits do not read the stream |
| R51 | A gateway defect lets a request through | std-only code in core with vectors, the battery's gateway family, a CI battery on a real engine, the per-run bridge holding only the gateway |
| R52 | The musl cross build fails on Windows | gate 9 before Slice A; a pinned builder image as fallback with the owner's approval |
| R53 | The Claude Code installer changes, cannot pin a version or fails during a build | pinned version argument, gate 1 records its behaviour, a release-manifest checksum fallback, a failed build changes nothing that runs |
| R54 | The engine's address pools run out (two networks per run, swarm networks present) | `max_workers` at most 4, removal after collection, orphan networks in doctor, `network_failed` |
| R55 | A mounted folder holds Synology placeholder files the engine cannot read | gate 1 in the synced tree; doctor checks mounts with AD-39's Cloud Files attribute test |
| R56 | A kernel escape reaches the shared WSL 2 VM and the author's distributions | stated limit (PRD section 1), doctor's note and `ssh-add -c` advice |
| R57 | Hooks written on Windows misbehave on Linux and are lost under bypass | gate 4, the inventory, `hook_program_missing`, `hook_failed` |
| R58 | The repository leaves through `api.anthropic.com` with another account's key | stated limit; README; credential scan for careless leaks only |
| R59 | Sensitive approvals become reflexes | hunks inline with their hash, the notice after 10 verdicts, `review = "all"` for infrastructure repositories |
| R60 | Host git runs configured programs on worker-made objects | scratch repository, fsck, empty hooks path, no checkout, local protocols only, Import by objects then a zero-old-value `update-ref` |
| R61 | The engine's or the author's defaults (log driver, proxy, builder) change behaviour | every container sets its driver and options; builds pass `--network default`; gate 1 checks the builder sees local images |
| R62 | A battery passes on attempts that could not run | `inconclusive` fails; `not_applicable` only off Docker Desktop |
| R63 | A host session drives `docker` itself (inspects the token, starts containers) | outside the threat model, as every process running as the author (v0.2 section 1); stated in the README |
| R64 | A project Dockerfile's final stage escapes the base | stage parsing before the build, the inherited label after it, and the vector overriding user, entry point and every security option |
| R65 | A worker resets its branch and its work reads as `no_change`, so v0.7 slows an order that had work to do | `no_change` requires exit 0 and host git's count on host objects; the report tail stays visible on the Done card; accepted, since the worker's branch is what the run produced |
