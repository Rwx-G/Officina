---
title: "PRD: Officina v1 - First Functional Release"
author: Rwx-G
date: 2026-10-28
status: approved as direction 2026-10-28 (owner delegated design decisions); re-baselined at v0.8's close before any v1 architecture or story
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.7.md
  - (v0.7 requirement challenge, not published)
  - architecture-v0.7.md
  - prd/prd-v0.8.md
  - architecture-v0.8.md
  - (v0.8 requirement challenge, not published)
  - (v0.1 to v0.6 requirement challenges, not published)
  - prd/prd-v0.1.md
  - prd/prd-v0.2.md
  - prd/prd-v0.3.md
  - prd/prd-v0.4.md
  - prd/prd-v0.5.md
  - prd/prd-v0.6.md
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - architecture-v0.5.md
  - architecture-v0.6.md
  - research-notes.md
  - backlog.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - stories/epic-5-decisions.md
  - stories/epic-6-decisions.md
  - stories/story-1.13-release-and-readme.md
  - https://v2.tauri.app/plugin/updater/
  - https://v2.tauri.app/distribute/windows-installer/
  - https://v2.tauri.app/start/prerequisites/
  - https://v2.tauri.app/distribute/debian/
  - https://v2.tauri.app/distribute/appimage/
  - https://v2.tauri.app/reference/config/
  - https://code.claude.com/docs/en/setup
  - https://code.claude.com/docs/en/cross-session-messaging
  - https://code.claude.com/docs/en/settings
  - https://code.claude.com/docs/en/settings-reference
  - https://code.claude.com/docs/en/tools-reference
---

# PRD: Officina v1 - First Functional Release

## 0. Document Purpose

Scopes v1, Epic 9, the last row of the framing's roadmap: the first
functional release, with an installer, `doctor`, updates, documentation and a
Linux build. Its exit criteria are the targets: an install on a fresh machine
in under 15 minutes with `doctor` green; a week of use with no intervention on
Officina. The framing's platform line holds: Windows until v1, Linux built at
v1, macOS after v1.

Revised after a requirement challenge: gate 0 now decides Slice B's size, and
by default the Linux build is a CI job that publishes no package; release
candidates come from a `workflow_dispatch` run that pushes no tag; FR-5
shrinks to doctor's upgrade section plus a CHANGELOG convention; the Setup
view becomes the existing diagnostics banner; reference pages are generated
from the closed enums; the mechanical fresh-machine checks move to a CI
install job with one human run; and names from unbuilt versions give way to
derived phrases until the re-baseline at v0.8's close.

**Direction, not yet a baseline.** v0.2 is in review; v0.3 to v0.5 are
approved and unbuilt; v0.6 to v0.8 are drafts. This PRD fixes the direction.
At v0.8's close, gate 1 re-baselines it against the code that then exists,
before any v1 architecture or story. Until then, where a requirement depends
on later code it uses a derived phrase ("every closed enum whose values reach
the author, as present at v1's start"; "every compatibility constant FR-4
reports"), never a type or family name from an unbuilt version.

**What v1 starts from.**
- No version has ever been cut: `CHANGELOG.md` holds `[Unreleased]` only,
  `tauri.conf.json` carries its own `0.1.0`, and story 1.13, the release
  story, is in progress. `release.yml` has no `workflow_dispatch` trigger and
  no file-list assertion, although story 1.13 requires both. v1.0.0 will most
  likely be the first published release.
- Windows only. On a version tag, `release.yml` builds an NSIS installer of
  the window and a portable zip of both executables, unsigned (R7), and
  publishes a draft release that the owner publishes by hand. The installer
  runs in Tauri's default mode, per user (S2, S6).
- `officina doctor` reads, compares and prints what to paste; it never writes
  under a Claude config dir. An absent or unparsable settings file reads as
  "not registered". The relay's `CLAUDE.md` paragraph tells sessions to call
  bare `officina run`, so the CLI must be on the sessions' PATH.
- The README is the only documentation.
- The signals that explain a change after an update already exist where the
  author meets it: AD-76's banner for a canonical catalogue format bump,
  AD-41's backup before a registry migration, doctor failing on a changed
  hooks block, and each refusal naming its cause.
- The `core-linux` CI job builds and tests the core, host and CLI crates on
  Linux. The app crate is excluded "before v1" (AD-18). Every platform call
  has a Windows body; `platform::other` returns `Unsupported` for focus
  (AD-1).
- Officina opens no network connection and no TCP port (README, privacy).

**What v1 adds.**
- **Precondition of Slice A:** story 1.13's `workflow_dispatch` dry run and
  its file-list assertion, so the release pipeline has run once before
  anything depends on it.
- **Slice A, the Windows release:** release candidates from a
  `workflow_dispatch` run, with no tag until the owner asks; one version
  source; a per-user install that puts the CLI on the user's PATH; in-place
  updates that never half-install while `officina.exe` is in use, checked by
  a CI install job; doctor for a fresh machine, shown through the board's
  diagnostics banner; `officina version --json` and doctor's upgrade section;
  a click that opens the latest release; a health log; a small hand-written
  guide and reference pages generated from the code. Usable alone: Officina
  on Windows, installed in one sitting and updated knowingly.
- **Slice B, the Linux build, sized by gate 0.** Default text (section 4.6):
  a Linux CI job that builds and tests the whole workspace, the app crate
  included; no package is published. Alternative text (section 4.7), only if
  the owner names a Linux machine he will use weekly or a second Linux user:
  the Linux product, with packages and the Linux counterparts of the
  Windows-only pieces. Otherwise section 4.7 moves to a post-v1 "Linux
  product" PRD, an explicit exception to P-7 for the owner's confirmation.

**Decisions taken here, for the owner's confirmation** (section 8):
1. Updates: a manual download with a click-only check, not Tauri's updater
   (FR-6).
2. Slice B's default text, and the after-v1 groups of section 4.1, confirmed
   in one line at gate 0.
3. The installer appends its folder to the user's PATH (FR-1).

**Null option.** Tag v1.0.0 on the v0.8 code with today's `release.yml`, the
README, and the Linux CI widened to the app crate. What it loses: the
15-minute install stays unmeasured, and a fresh machine needs JSON merged by
hand; for the one user, who wrote doctor, that costs minutes, once per
machine, on two machines. Updates void the catalogue approval, but AD-76's
banner says so and the relay's refusal names why. The honest losses are a
release process that has never run once, and the framing's Linux build,
which the default Slice B delivers at small cost.

**History.** The draft found that the v0.2 gate matched `Bash` only, while
Claude Code's PowerShell tool is on by default on Windows (S10). Story 2.12
handles it (D-171); FR-8 requires the matcher that story prints.

Facts about Claude Code rest on `docs/research-notes.md` (E-numbers) and on
pages S7 to S10; facts about Tauri on pages S1 to S6. What none settles is a
gate in section 8.

## 1. Vision

The author installs Officina on a new workstation as he would any desktop
tool. He downloads it, runs it, follows the banner and the guide, pastes two
blocks, and he is done. Then he forgets it: it starts with his session, keeps
up when Claude Code updates itself, and says plainly when something needs
him. An update is a deliberate act. Before it, he reads the release's
CHANGELOG, whose Upgrade subsection CI keeps honest; after it, doctor lists
what changed and what to redo.

Linux is built and tested on every change, so the code stays portable. A
Linux package waits for a Linux user (section 4.6), unless gate 0 names one
(section 4.7).

**Where the boundary ends.** v1 adds no boundary. Install and update run as
the author. The installer writes only its own folder, one entry of the user's
PATH and a Start menu shortcut. Officina still never writes under a Claude
config dir or in a project: doctor prints, the author pastes. The binaries
stay unsigned (R7). A download's integrity rests on GitHub and the published
SHA-256 sums, which prove the transfer, not the author.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Install in one sitting.** On a new Windows workstation, 15 minutes
  from the release page to a board that shows my sessions, with nothing to
  know beyond the guide.
- **J2 - Forget it is there.** A week of work without restarting, repairing
  or reconfiguring Officina.
- **J3 - Update knowingly.** Before an update, read what it changes for me;
  after it, see the list in doctor; never meet a voided approval whose cause
  nothing names.
- **J4 - Look it up.** When I meet a code, a refusal or a doctor line, find
  what it means and what to do, with the same words the screen showed.
- **J5 - Use it on Linux** (section 4.7 only). On a Linux desktop, what
  Officina offers there works, and what it does not offer says why.

### 2.2 Key User Journeys

Placeholders throughout: user `user01`, home `C:/Users/user01`, project
`api`.

1. **A fresh Windows machine (J1).** Claude Code is installed and signed in.
   The author opens the latest release page and downloads
   `officina-1.0.0-windows-x64-setup.exe`. He passes SmartScreen's warning
   ("More info", "Run anyway", R7) and installs without an administrator
   prompt. The window opens on the board. Its diagnostics banner lists the
   failing lines: "hooks: `C:/Users/user01/.claude/settings.json` does not
   exist: create it with this content", with the content and a Copy button,
   and a link to the guide's first-run page. He creates the file, starts
   `claude` in `api` and sends a prompt; the banner clears. A click on the
   card brings his terminal forward.
2. **An update (J3).** Weeks later the author clicks **Latest release** in
   the About panel. The release notes are the version's CHANGELOG section;
   its Upgrade subsection says "catalogue format: the relay's approval waits
   for one approval of the shown diff". He closes the window and runs the
   installer while a long-lived `officina` process runs. After the restart,
   AD-76's banner shows the catalogue change, and doctor's upgrade section
   lists the same constant with what to do. He approves the diff.
3. **A week (J2).** A release candidate is installed on Monday, started from
   the Startup folder. Each evening the author writes one line in the
   research notes: "touched Officina: no". Claude Code updates itself twice;
   the canary says "contract held". One morning the board is stale for two
   minutes while `claude agents --json` fails during its update; that period
   is `claude`'s and is listed. On Friday, doctor is green and its health
   section reads zero unclean exits.
4. **A code on screen (J4).** A session reports a refusal ending in
   `(check_required)`. The author opens the generated reference page and
   finds the same meaning and fix text the refusal printed.
5. **Rollback.** A release misbehaves. The author runs the previous
   installer. Doctor's upgrade section says the registry is read-only and
   the catalogue needs approval again; the guide's rollback page names the
   backup copy.
6. **Uninstall.** `officina doctor --uninstall` lists the hooks entries, the
   statusline prefix and the `CLAUDE.md` paragraphs to remove. He removes
   them and uninstalls. `~/.officina` stays until he deletes it.

### 2.3 Test environments

1. **CI:** a `windows-latest` runner, fresh on every job, runs the install
   job (FR-14 item 1).
2. **The human run:** Windows Sandbox on the owner's home machine (Windows 11
   Pro, virtualization on), Claude Code installed and signed in before the
   timer starts. The employer machine is not used for it.
3. **The update scenario and the week:** the owner's workstation, on a
   release candidate.
4. **Linux:** the Linux CI job only, under the default text; the machine gate
   0 names, under section 4.7.

## 3. Glossary

- **Fresh machine** - a supported OS install that has never run Officina,
  with Claude Code installed and signed in (section 8, question 10).
- **Release candidate** - the artifacts of a `workflow_dispatch` run on one
  commit, with `version-<platform>.json` and `SHA256SUMS.txt`, kept as a
  workflow artifact; no tag, no release (FR-13).
- **Compatibility constant** - a value in the code that decides whether
  stored state or a pasted block from an earlier version still holds: a
  format or schema version, or the hash of a block doctor prints (FR-4).
- **Re-baseline** - gate 1: at v0.8's close, this PRD is checked against the
  code that exists and each derived phrase is resolved to the types it then
  denotes.
- **Intervention** - an action on Officina other than using it: restarting a
  hung window, editing a file under `~/.officina` by hand, killing an
  `officina` process, re-pasting a block that no version change asked for, or
  any other fix that use requires.
- **Designed upkeep** - an action Officina asks for through doctor or a
  banner by design, such as an approval after the author's own floor edit.
  Counted, never an intervention.
- **Health log** - the window's own lifecycle and staleness record (FR-9).

## 4. Features

Documented surfaces this PRD rests on, beyond E1 to E18 and earlier PRDs'
S-numbers. Each becomes an E-entry once a check that uses it has run. Read on
2026-10-28. Numbers are local to this PRD.

- **S1** (Tauri, `plugin/updater`) The updater "needs a signature to verify
  that the update is from a trusted source. This cannot be disabled." The
  public key sits in the configuration; the private key comes from
  `TAURI_SIGNING_PRIVATE_KEY`. Endpoints serve a static JSON (`version`,
  `platforms.<target>.url`, `.signature`). Windows install modes: `passive`
  (default), `basicUi`, `quiet`; the app exits during the install. On Linux
  the page describes AppImage only. Crates `tauri-plugin-updater` and, for
  the restart, `tauri-plugin-process`; capability `updater:default`. The app
  itself contacts the endpoints.
- **S2** (Tauri, `distribute/windows-installer`) "By default the installer
  will install your application for the current user only", which "does not
  require Administrator privileges", under `%LOCALAPPDATA%`; `perMachine` and
  `both` need administrator rights. WebView2 default `downloadBootstrapper`;
  the runtime ships with Windows 10 (April 2018 or later) and Windows 11.
  NSIS hooks `NSIS_HOOK_PREINSTALL`, `POSTINSTALL`, `PREUNINSTALL`,
  `POSTUNINSTALL` through `installerHooks`.
- **S3** (Tauri, `start/prerequisites`) Linux builds need WebKitGTK 4.1
  (`libwebkit2gtk-4.1-dev` on Debian and Ubuntu, with `build-essential`,
  `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`); WebView2 is preinstalled on Windows 10 1803 and later.
- **S4** (Tauri, `distribute/debian`) A `.deb` declares `libwebkit2gtk-4.1-0`
  and `libgtk-3-0`; "you must build your Tauri application using the oldest
  base system you intend to support", with Ubuntu 22.04 and Debian 12 as
  baseline examples.
- **S5** (Tauri, `distribute/appimage`) An AppImage "bundles all dependencies
  and files", and its size "grows from the 2-6 MB range to 70+ MB"; same
  oldest-base rule. "GUI apps on macOS and Linux don't inherit `$PATH` from
  shell dotfiles."
- **S6** (Tauri, `reference/config`) Top-level `version`: "If removed the
  version number from `Cargo.toml` is used." `nsis.installMode`:
  `currentUser` (default), `perMachine`, `both`. `bundle.targets` include
  `deb`, `rpm`, `appimage`, `nsis`, `msi`.
- **S7** (Claude Code, `setup`) Supported: Windows 10 1809 or later, Ubuntu
  20.04 or later, Debian 10 or later, Alpine 3.19 or later. On Linux the
  native installer "manages the launcher at `~/.local/bin/claude` as a
  symlink into `~/.local/share/claude/versions/`"; on Windows the binary is
  `%USERPROFILE%\.local\bin\claude.exe`. Git for Windows is optional; without
  it "Claude Code runs shell commands via the PowerShell tool". Linux
  binaries "are not individually code-signed".
- **S8** (Claude Code, `cross-session-messaging`) Each session binds "a Unix
  domain socket on macOS and Linux, including Linux inside WSL 2, and a named
  pipe on native Windows", exported as `CLAUDE_CODE_MESSAGING_SOCKET`, with
  `CLAUDE_CODE_MESSAGING_TOKEN`. On Linux own-child messages are verified "by
  process evidence". "A session inside WSL 2 and a native Windows session on
  the same computer can't reach each other."
- **S9** (Claude Code, `settings`, `settings-reference`) User settings at
  `~/.claude/settings.json` (`%USERPROFILE%\.claude` on Windows) or under
  `CLAUDE_CONFIG_DIR`; "Installing Claude Code doesn't create any settings
  file"; settings files are strict JSON. `disableAllHooks`: "Turn off hooks,
  a custom status line, and a custom `@` file suggestion command at once"
  (any file). `allowManagedHooksOnly`: "Run only the hooks your organization
  deploys" (managed). `/status` shows the "Setting sources" line.
- **S10** (Claude Code, `tools-reference`) The PowerShell tool is "enabled
  automatically" on Windows without Git Bash and "on by default for claude.ai
  and Console accounts" with it. "Match `Bash|PowerShell` in hooks that
  inspect shell commands."

### 4.0 Slice 0: checks

#### FR-0: Gates first
Gate 0 is answered by the owner before this PRD is final; its answer selects
Slice B's text. Then, in order:
1. **Gate 1, the re-baseline**, at v0.8's close, before any v1 architecture
   or story.
2. **Gate 2** before FR-1 and FR-2.
3. Under section 4.7 only: gates 3 to 6 before its stories.

Each gate is recorded as an E-entry. A failed gate changes the FR it names
first; section 8 says how for each.

### 4.1 What earlier versions hand v1

Every item an earlier PRD, addendum, challenge or story named for v1, with
its disposition, in three groups. The owner confirms the two after-v1 groups
in one line at gate 0. Under section 4.7, the "Linux product" group moves to
"built at v1". FR-22 reads this table at the close.

**Built at v1.**

| Input | Named in | v1 |
|---|---|---|
| The app crate on Linux; WebKitGTK build packages "not worth it before v1" | AD-18 | FR-21 |
| `platform::other`, whose focus returns `Unsupported` | AD-1 | kept; `officina run` and contained calls refuse on Linux (FR-21) |
| Append atomicity and hook latency measured on Windows only | AD-10, AD-17 | the perf tests run on Linux (FR-21) |
| The installer, updates, doctor for a fresh machine, documentation, the Linux build | v0.7 and v0.8 section 5 | this PRD |
| The Linux programs the window ships for container images | v0.6 AD-100 | stay in the Windows artifacts (FR-1) |
| Autostart documented, not built | spine section 3 | the guide's first-run page (FR-10) |
| The WebView2 eyeball per release | AD-17 | FR-14 |
| The release smoke test on a clean profile | story 1.13 AC 7 | FR-14's CI install job and the human run |
| Story 1.13's dispatch dry run and file-list assertion | story 1.13 AC 1 | precondition of Slice A |
| Unsigned binaries | R7 | kept (question 9) |
| The High items of `docs/backlog.md` | backlog | closed at v0.1's and v0.2's close; v1 checks the section is empty (FR-22) |

**Windows-only by nature** (nothing to build on Linux under either text).

| Input | Named in |
|---|---|
| `CREATE_NO_WINDOW`, `NoDefaultCurrentDirectoryInExePath`, `cmd.exe /d /c` for a `.cmd` shim, the shim refusal for contained calls: nothing on Linux re-parses an argument vector or flashes a console | AD-6, AD-15, v0.5 AD-51 |
| The `wsl` runner and `wsl.exe` from `GetSystemDirectoryW` | AD-30 |
| `.bat` and `.cmd` programs refused | v0.2 relay rules |
| The taskbar overlay | AD-19 |
| The OneDrive and Cloud Files sync rules | v0.3 AD-39 |

**After v1, with the Linux product** (section 4.7 if gate 0 names a Linux
user, else the post-v1 "Linux product" PRD).

| Input | Named in |
|---|---|
| `claude` resolution by Windows names and `%USERPROFILE%\.local\bin\claude.exe` (E8) | AD-6 |
| Focus by window: parent walk, window list, raising a window | AD-15, v0.5 addendum |
| Console spawn for `claude attach` and the orchestrator | AD-15, v0.5 FR-10 |
| Single instance, verified on Linux | AD-19 |
| Job objects around `officina run` and the contained calls | AD-31, D-64, v0.5 addendum |
| Credential Manager read for HTTP actions | AD-36 |
| The canonical floor path lowercased; "Linux (v1) will need the original spelling" | story 2.2 |
| Network drive detection (`GetDriveTypeW`) | v0.3 AD-39 |
| WebKitGTK compositing of the scene (R32); the dark theme following the system live | v0.4 AD-59, v0.4 addendum |
| Screenshot baselines generated on Windows | v0.4 addendum (they stay Windows-only) |
| Messaging socket and token names observed on Windows (E18) | v0.5 AD-80 |
| `git` resolved from HKLM | v0.6 AD-111 |

**After v1, other.**

| Input | Named in |
|---|---|
| The isolated workshop on a Linux host, its credential store, the engine's resolution | v0.6 section 5, FR-5, AD-98 |
| `RegisterHotKey` and the local time offset, "inputs of v1's Linux build": the governor launches only workshop runs | v0.7 section 5 and 6.1.2, v0.7 addendum, v0.7 challenge |
| Voice, macOS, the write relay with a restricted account, extra outbound connectors | framing section 9 |
| A drawn meeting room; the workstation browser slice | v0.5 section 5, v0.8's backlog note |

**Not collected by v1.** v0.7's two-week measure and its Slice B, and v0.8
Slice A's conditional parts, which their PRDs allow "at the latest" at v1's
close, are read at their own versions' closes. Gate 1 checks that those
closes recorded them; v1 is not where earlier deferrals collect.

### 4.2 Install and update (Slice A)

#### FR-1: The Windows installer
- NSIS in `currentUser` mode, written explicitly in the bundle configuration
  even though it is Tauri's default (S6), so a default change cannot move it.
  No administrator rights; the install folder is under `%LOCALAPPDATA%` (S2).
- It ships `officina-app.exe`, `officina.exe` and the Linux programs the
  window needs (v0.6 AD-100, as present at v1's start), and nothing else;
  FR-13 asserts the list.
- A Start menu shortcut for the window. No desktop shortcut, autostart entry,
  file association or service.
- WebView2: Tauri's default bootstrapper (S2).
- **PATH.** The installer appends its folder to the user's `Path` (HKCU), so
  a session's bare `officina run` resolves, and removes exactly that entry on
  uninstall. It never rewrites, reorders or drops another entry. If it cannot
  read and write the whole value back (its string limit), it changes nothing
  and the diagnostics banner prints the line to add. FR-14's CI job presets a
  long PATH. Shells, editors and sessions started before the install do not
  see the new entry; the guide says to restart them. An absolute path in the
  `CLAUDE.md` paragraph is no alternative: it would put a user's home path
  into committed repository files.
- Uninstall removes the folder, the shortcut and the PATH entry. It keeps
  `~/.officina` and says so on its last page, pointing at the guide's
  uninstall page (FR-8's `--uninstall`).
- No MSI. The portable zip stays, with the same files.

#### FR-2: In-place update
- Running a newer installer over an installed Officina upgrades it in place,
  in the same folder with the same file names, so every configuration that
  names `officina.exe` by path (the hooks block, the statusline command, any
  server configuration Officina builds) keeps naming a valid program.
- The window must be closed. The installer asks for it, or closes it after
  consent; it never installs beside a running window.
- `officina.exe` may be in use: by a hook for milliseconds, by `officina run`
  waiting for an approval or running an action, by any long-lived `officina`
  process present at v1's start. The installer replaces it anyway, without
  failing and without leaving a folder that mixes versions. A process already
  running finishes on the old image; the next call starts the new one. The
  architecture picks the mechanism; gate 2 checks it.
- A hook call made at the instant of replacement may find no program. Claude
  Code reports the hook error and continues; the relay gate does not answer
  that one call. Accepted: the relay is a guardrail (v0.2 section 1), and the
  window is closed during the update.
- Installing an older version over a newer one works as a file operation;
  FR-5's downgrade line says what the older one does with newer data.
- For the portable zip, the guide says to replace both executables together
  with the window closed.

#### FR-3: One version source
- The workspace `version` in `Cargo.toml` is the only version.
  `tauri.conf.json` drops its own `version`, so Tauri uses the crate's (S6).
  The installer's name, the About panel, `officina version` and
  `release.yml`'s tag check read that one value. CI asserts that the NSIS
  file name carries the workspace version.
- v1 is `1.0.0`. After v1, Semantic Versioning, read from the user's side: a
  patch release moves no compatibility constant (FR-4); a release that moves
  one is at least a minor; removing a CLI subcommand, an exit code, a floor
  key, or the reading of a stored format is a major.

#### FR-4: `officina version --json`
Prints the version, the build's commit, the target platform and every
compatibility constant the code holds, each read from the constant that owns
it: format and schema versions (among those that exist today, the canonical
catalogue format of AD-24, and the floor, event line, quota and board
schemas), and a SHA-256 of each block doctor prints for a fixed fixture.
Gate 1 lists the constants present at v1's start. No authority, no write,
exit 0. The About panel shows the same values, and a bug report quotes them.

#### FR-5: Doctor's upgrade section
- Doctor's upgrade section compares `state/last_run.json` with the running
  version's FR-4 constants and prints one line per changed constant, with
  what to do. The window writes `state/last_run.json` when it starts a
  version other than the one recorded, keeping the previous version's
  constants beside the current ones.
- The downgrade line, when the previous version is newer, says the registry
  is read-only and the catalogue needs approval again.
- The lines are informational. The conditions they describe keep their own
  checks, which are the ones that fail: the catalogue approval (AD-76's
  banner and the relay's refusal), the hooks registration, and each feature's
  own section.
- **Before an update** the author reads the CHANGELOG. Each version's section
  carries a hand-written "Upgrade" subsection naming each compatibility
  constant that moved and what it asks of the author. FR-13's candidate run
  diffs FR-4's JSON against the last published release and turns CI red when
  a constant moved without a line in that subsection. With no previous
  published release, the release notes say "First published release: no
  upgrade impact".
- v1 itself moves no compatibility constant on Windows.
- Revisit a window banner for upgrades only if the research notes record an
  update in which a voided approval or a stale block cost the author time
  because no existing signal named it.

#### FR-6: The update check
- The About panel shows the version, the commit, the build date and the
  platform. Its **Latest release** button opens
  `https://github.com/Rwx-G/Officina/releases/latest` in the default browser,
  which lands on the newest published release and never on a draft. Its
  **Guide** button opens the guide (FR-10). Both URLs are compile-time
  constants; no other text reaches the call (`ShellExecuteW`).
- Officina makes no request itself: no background check, no telemetry, no
  download. The README's privacy statement stays true.
- The update is the installer (FR-2) or the zip. The guide's update page:
  read the release's Upgrade subsection, close the window, run the installer,
  pass SmartScreen's warning (R7), then read doctor's upgrade section.
- **Rollback:** run the previous release's installer; FR-5's downgrade line;
  restoring the registry's pre-migration copy (AD-41) by hand when a
  migration ran, from the guide.
- **Second choice, not built: Tauri's updater** (S1). It verifies a
  signature against a key compiled into the app, which this check does not,
  and installs in one click without SmartScreen's prompt. It costs two
  plugins and their HTTP and TLS stack in the window; a signing key held by
  CI beside a build that runs third-party code; an outbound request from the
  app, which contradicts the privacy statement; an install path that must
  handle `officina.exe` in use while the app restarts itself; and on Linux it
  updates an AppImage only (S1). Revisit if the author defers an update for
  more than four weeks because of the manual steps, or if a second regular
  user appears.

### 4.3 First run and doctor (Slice A)

#### FR-7: Doctor in the diagnostics banner
While doctor has a failing line, the board's diagnostics banner lists the
failing lines with their paste blocks and a Copy button
(`navigator.clipboard`, as the attach command), and links the guide's
first-run page. No separate view and no `setup.json`. The window runs the
checks in-process (AD-2) for the `officina` beside it. Revisit a dedicated
view only if the human run (section 7) exceeds 15 minutes because the author
had to find a terminal and run doctor after the install.

#### FR-8: Doctor for a fresh machine
Doctor keeps its rule: it reads, compares and prints; it never writes under a
Claude config dir or in a project. v1 adds:
- **Settings file states.** An absent user settings file gives the complete
  file to create, valid JSON holding only Officina's entries, instead of a
  block to merge. An unparsable file fails with the parse error's line and
  column, never "not registered". `disableAllHooks` set in it fails with that
  cause (S9). Doctor never prints the content of a settings file, only blocks
  or files of its own making, so a secret in the user's `env` block never
  reaches a screen.
- **Hooks registered, no event.** When the hooks are registered, a live
  session exists and no hook event arrived in the last 24 hours, the detail
  names the documented causes: `disableAllHooks` in another settings file,
  or an organization's `allowManagedHooksOnly` (S9), with `/status`'s
  "Setting sources" line as the way to see which applies.
- **Matchers.** Doctor requires the hook matchers story 2.12 prints (D-171),
  through its existing comparison of the hooks block.
- **Install.** Which `officina` and window run, from where, and their
  versions (equal, else warn). Whether `officina` on the PATH doctor sees is
  the same file (else warn, with the line to add). The program each
  registered hook command names: missing fails (the hooks run nothing and
  the gate answers no call); another version warns, with the block to paste.
  A path that the shell Claude Code uses would reinterpret inside double
  quotes (`"`, `$`, a backquote) fails with "move Officina to a plain path"
  instead of printing a command.
- **Platform.** The Windows version and the WebView2 runtime's version.
- **Features not in use.** A section whose feature the floor does not use
  prints one Ok line, "not in use", never a warning.
- **Upgrade.** FR-5's lines.
- **Health.** The last 7 days of the health log (FR-9).
- **`officina doctor --uninstall`** prints, for each file doctor knows, the
  entries to remove: the hooks entries that run `officina hook`, the
  statusline prefix, each guarded project's `CLAUDE.md` paragraph; and the
  Officina home, for an author who wants no data left. It removes nothing.
- **Green** means exit 0, and no warning other than the ones a feature not in
  use or, under section 4.7, the platform matrix produces.

#### FR-9: The health log
- `state/health.jsonl`, append-only, written by the window only. Lines:
  `start` (version, platform); `stop` on a clean exit; `unclean_exit` at a
  start that finds the previous start without a stop; `stale` for a board
  stale more than 5 minutes (since, until, the error's class, never its
  text, and whether `claude agents --json` succeeded during the period);
  `registry_quarantine` when a registry exists; `collector_error` by class.
  No path, and no text from a session. The hourly pass drops lines older
  than 90 days.
- Doctor and the About panel show the last 7 days. The week's measure reads
  its `unclean_exit` count and its stale periods (section 7).

### 4.4 Documentation (Slice A)

#### FR-10: The hand-written guide
- `docs/guide/`, Markdown rendered by GitHub, linked from the README and
  opened by the Guide button (FR-6). Hand-written pages only: install (the
  installer and the zip; the precondition of Claude Code installed and signed
  in); first run (the diagnostics banner, hooks, statusline, autostart from
  the Startup folder); update; rollback; uninstall; platform notes.
- Platform notes, and the README, say "Windows 11; Windows 10 may work,
  untested", and, under the default Slice B, "Linux: builds and tests in CI;
  no package until a Linux user exists".
- The README keeps the pitch, the screenshots, the feature descriptions and
  the links. `readme.rs`'s rules extend to the guide: no list the code owns
  transcribed.
- Examples use placeholders only (`example.org`, `user01`, `/path/to/`); no
  employer content.

#### FR-11: The configuration reference
- The floor file's key table is generated from the floor parser's key list,
  with each key's table, type, required or default, and constraints; a drift
  test fails on a stale page. Hand-written prose covers only the "why" of a
  key, and what a change voids when the key is canonical (AD-24).
- Every TOML example in the reference parses with zero issues, except an
  example marked as showing an issue, which must produce exactly that issue.

#### FR-12: Generated reference pages
- Each closed enum whose values reach the author (refusals, denies, launch
  refusals, holds, issues, as present at v1's start) carries its meaning and
  its fix beside the variant. `docs/guide/reference/*.md` is generated from
  them, and a drift test like the TypeScript one fails on a stale page.
  Refusal and doctor output print the same text, so the screen and the page
  cannot differ.
- **Doctor titles become a closed enum, as a named story.** Its size today:
  about 50 call sites in `doctor.rs` and `relay/doctor.rs` build a title from
  a string, some interpolating a project or action name, which becomes a
  field of the variant. Each version up to v0.8 adds doctor sections, so gate
  1 restates the size.
- The CLI reference is generated from the usage text, with each
  subcommand's exit codes, under the same drift test.

### 4.5 Release process (Slice A)

#### FR-13: Release candidates and the tag
- **The candidate.** A `workflow_dispatch` run on a commit builds every
  artifact, runs the same gates, uploads them with `version-<platform>.json`
  and `SHA256SUMS.txt` as a workflow artifact, and creates no tag and no
  release. This is the release candidate. It also diffs FR-4's JSON against
  the last published release's and fails on a constant that moved without a
  CHANGELOG Upgrade line (FR-5).
- Each archive's and installer's file list is asserted against an expected
  list, so a test program (`officina-fake-claude`, `officina-fake-tool`) can
  never ship.
- **The tag.** Pushed only when the owner asks, and it must point at the
  candidate's commit. Either the tag run rebuilds the artifacts and its
  publish job compares `SHA256SUMS.txt` with the candidate run's, a mismatch
  publishing nothing; or the tag run publishes the candidate run's own
  artifacts, fetched by run id with the read token. The architecture picks
  one.
- The publish job (`gh` only, the one job with a write token, no third-party
  code) creates a **draft** release whose notes are the version's CHANGELOG
  section, Upgrade subsection included, with "First published release: no
  upgrade impact" when there is no previous published release. The owner
  publishes the draft by hand.

#### FR-14: The release candidate checklist
On a candidate's own artifacts:
1. **CI install job.** An install job on `windows-latest` installs the
   candidate silently per user and asserts the folder, the Start menu entry,
   the PATH entry with a long PATH preset, `officina version --json`,
   doctor's exit and lines against a fixture `settings.json` made from
   doctor's own printed file, and, after uninstall, a byte-identical PATH and
   a kept `~/.officina`.
2. **Update scenario.** An update over the last installed build on the
   workstation (a v0.8 install from a dispatch candidate), with the window
   open, a long-lived `officina` process running and an approval pending.
3. The WebView2 eyeball (AD-17).
4. Doctor green on the workstation, with every feature in use checked.
5. CHANGELOG's `[Unreleased]` moved under the version, with its Upgrade
   subsection; README and guide current.

The human timed run (section 7) is not repeated per candidate. The results
go to `docs/qa/release-<version>.md`.

### 4.6 Slice B, default text: the Linux build

Applies unless gate 0 names a Linux machine the owner will use weekly, or a
second Linux user.

#### FR-21: Linux CI
- The `linux` CI job builds and tests the whole workspace, the app crate
  included, on Ubuntu 22.04, and runs the perf and zero-write tests. It
  installs the Tauri build packages (S3, without the tray library: Officina
  has no tray).
- Every platform call keeps `platform::other`, which returns `Unsupported`.
- `officina run` and every contained call (each spawn whose tree a Windows
  kill-on-close job ends, as present at v1's start) refuse on Linux with
  `platform_unsupported` until process groups exist, so no relay action runs
  uncontained. Every `cfg(windows)` test has a Linux counterpart or a
  one-line reason; the relay run tests' counterpart asserts that refusal.
- No `.deb` or `.tar.gz` is published. The README says "Linux: builds and
  tests in CI; no package until a Linux user exists".
- If the owner wants an artifact anyway (gate 0), the same job publishes a
  `.tar.gz` marked "unsupported preview, untested", and no install run is
  accepted for it.

### 4.7 Slice B, alternative text: the Linux product

Applies only if gate 0 names a Linux machine the owner will use weekly
(distribution and desktop), or a second Linux user. Otherwise this section
moves to a post-v1 "Linux product" PRD, with the owner's confirmation: an
explicit exception to P-7. Under this text, FR-21's refusal becomes FR-17's
process groups, and FR-21 gains Linux bodies for the relay run tests, D-9's
authority marker step on the Linux `officina` built alone, and the e2e suite
under Playwright's WebKit.

#### FR-15: Linux targets and packages
- x86_64 only, built on Ubuntu 22.04 (glibc 2.35, WebKitGTK 4.1), the oldest
  base supported (S4, S5).
- Supported: Ubuntu 22.04 and later and Debian 12 and later through the
  `.deb`; other distributions with glibc 2.35 or later and WebKitGTK 4.1
  through the `.tar.gz`. Tested on the distribution gate 0 names.
- **`.deb`:** the window, `officina` in `/usr/bin` (a stable absolute path
  for the hooks), a desktop entry; it depends on `libwebkit2gtk-4.1-0` and
  `libgtk-3-0` (S4), plus whatever gate 3 finds missing. It needs
  administrator rights once; the no-administrator promise is Windows-only.
- **`.tar.gz`:** both binaries, README and LICENSE, unpacked into
  `~/.local/bin`; it needs the distribution's WebKitGTK 4.1 runtime.
- **No AppImage:** it bundles the libraries, 70 MB and more (S5), with their
  LGPL obligations, and runs `officina` from a mount path that changes at
  every start, which no hook can name. No rpm, Flatpak, Snap or ARM64.
- The Officina home stays `~/.officina` (AD-7).

#### FR-16: Claude Code on Linux
- `claude` resolution: `OFFICINA_CLAUDE_BIN`; `claude` on PATH;
  `~/.local/bin/claude` (S7), which a window started from the desktop may not
  have on its PATH (S5). A launcher script is accepted: Linux runs it with
  the argument vector intact.
- Config dir, sessions trigger and contract canary unchanged; gate 4 records
  E1, E2, E3, E10, E12 and E14 on Linux.
- Messaging variables `CLAUDE_CODE_MESSAGING_SOCKET` and
  `CLAUDE_CODE_MESSAGING_TOKEN` (S8); gate 4 re-observes E18's names, and the
  removed-environment constant gains any missing one.
- `git`: `OFFICINA_GIT_BIN`, else `git` on PATH.

#### FR-17: Process trees on Linux
Wherever a Windows job object ends a tree: the child starts in a process
group of its own; a timeout kills the group, with the exit codes unchanged;
when `officina run` itself is killed, the parent-death signal kills its
direct child, and what that child started survives unless it ends it. Gate 6
records what survives; the guide states it. No privilege, cgroup or systemd
dependency.

#### FR-18: Focus and consoles on Linux
- The parent walk reads `/proc/<pid>/stat`. When `DISPLAY` is set, windows
  are listed and raised through the X11 window manager protocol; in a Wayland
  session this reaches XWayland windows only. A native Wayland window is
  invisible to that protocol and cannot be raised by another program by
  design: the card shows `unsupported`. Gate 5 records the named desktop's
  behaviour.
- Consoles: a terminal from a host constant list, each with the form that
  passes an argument vector unchanged (for example `gnome-terminal --`,
  `konsole -e`, `xterm -e`); never a form that hands one string to a shell;
  none found gives `failed(no_terminal)` and the copy command.

#### FR-19: Paths, files and storage on Linux
- Case-sensitive path comparison, keeping the spelling as read; where a
  comparison guards (v0.2 D-14), a path is guarded when any spelling of it
  is. Windows keeps today's lowercase forms, so no hash or approval moves
  there.
- The filesystem type from `/proc/self/mountinfo` refuses the registry on a
  network filesystem, as `DRIVE_REMOTE` does on Windows.
- HTTP action credentials (AD-36): the file `<officina home>/credentials/
  <name>`, read only when it is a regular file owned by the user with no
  group or other permission bit, else refused (`credential_unsafe`); doctor
  prints the commands to create it with a placeholder, never a value.

#### FR-20: The platform matrix
One core constant maps each feature, per platform, to `available`,
`limited(reason)` or `unavailable(reason)`; an unavailable feature's view
says why and links the guide's platform page; doctor prints the platform's
non-available entries at Ok level; a test holds the page against the
constant. Windows-only at v1: the `wsl` runner (on Linux such an action stays
in the catalogue and `officina run` refuses it with `runner_unavailable`),
the features that need a container engine (as present at v1's start), and
the taskbar overlay. Limited on Linux: focus, consoles, always-on-top under
Wayland.

**Additions to Slice A's FRs under this text.** FR-8's platform section adds
WebKitGTK's version, the display server, what focus can do there and the
terminal found. FR-10's install, first-run and platform pages cover the
Linux packages and `~/.config/autostart`. FR-14 adds the WebKitGTK eyeball
and the Linux CI install of the `.deb` on Ubuntu 22.04.

### 4.8 The close

#### FR-22: The v1 close
The close (P-7, P-16) reads and records:
- section 4.1's groups: each "built at v1" row built, and the after-v1 groups
  as the owner confirmed them at gate 0;
- the High section of `docs/backlog.md`, empty;
- section 7's outcomes.

## 5. Non-Goals (Explicit)

- After v1, as the framing says: voice, macOS, the write relay with a
  restricted account, extra outbound connectors. Also after v1: the groups
  section 4.1 lists, among them the Linux product under the default Slice B.
- Tauri's updater, any automatic or background update check, any download
  by Officina, telemetry (FR-6).
- A window banner for upgrades, per-constant troubleshooting pages, and an
  upgrade impact computed by the release workflow (FR-5).
- A separate setup view (FR-7).
- A public tag, or a pre-release tag, as a test vehicle (FR-13).
- Code signing at v1 (R7, question 9).
- MSI; ARM64; under the default text, any Linux package.
- A per-machine install, a shared install for several users, a managed
  deployment.
- Officina installing or updating Claude Code, Git for Windows or a
  container engine; editing a Claude settings file, a statusline script or a
  `CLAUDE.md`, during install, update or uninstall (FR-7, FR-8).
- Watching Claude Code sessions that run inside WSL from the Windows window:
  they cannot even message each other (S8), and their config dirs, buffers
  and homes differ.
- Localising the interface.
- Reworking any v0.x feature beyond what v1's FRs need.

## 6. MVP Scope

### 6.1.1 Delivery order
- **Slice 0:** gate 0 before this PRD is final; gate 1, the re-baseline, at
  v0.8's close before any architecture or story; gate 2 before FR-1 and
  FR-2.
- **Precondition of Slice A:** story 1.13's dispatch dry run and file-list
  assertion.
- **Slice A, the Windows release**, in this order, each step usable alone:
  FR-13's candidate run; FR-3; FR-1 and FR-2 with FR-14's CI install job;
  FR-8 and FR-7; FR-10 to FR-12, with the doctor-titles story; FR-4 and
  FR-5; FR-6 and FR-9. Usable alone: Officina on Windows, installed in one
  sitting, updated knowingly, documented, with a release process that has
  run.
- **Slice B:** FR-21 under the default text; section 4.7 under the
  alternative, with gates 3 to 6 first.
- **Close:** FR-22 with section 7.

v1.0.0 is one release. Before the tag, any candidate can be installed and
used; the tag is pushed when the owner asks, at a candidate's commit, after
FR-14 passed on it. What a slice defers is done before the close (P-7); the
after-v1 groups of section 4.1 are the listed exceptions, confirmed at gate
0.

### 6.1.2 Dependencies for the owner's approval

| Item | Where | Licence | Why |
|---|---|---|---|
| `windows-sys` feature `Win32_UI_Shell` | host, existing crate | MIT or Apache-2.0 | `ShellExecuteW` for FR-6's two fixed URLs |
| An NSIS hooks file (`installerHooks`, S2) | repository | Apache-2.0 | FR-1's PATH entry, FR-2's replacement of an image in use |
| Ubuntu build packages (S3): `libwebkit2gtk-4.1-dev`, `librsvg2-dev`, `libxdo-dev`, `libssl-dev`, `build-essential` | CI only, not shipped | LGPL-2.1 and others | the app crate on Linux (FR-21) |

No Tauri plugin, HTTP client, TLS stack or signing key is added.

Under section 4.7 only: `libc` and `x11` as direct dependencies under
`cfg(target_os = "linux")`, both already in the lock file through Tauri's
Linux stack (MIT or Apache-2.0; MIT), for process groups, the parent-death
signal and the user id (FR-17, FR-19), and for the window protocol (FR-18,
dropped if gate 5 shows the desktop refuses activation); Playwright's WebKit
browser in CI; `libwebkit2gtk-4.1-0` and `libgtk-3-0` on the user's system.
`unsafe` code on Linux would then live in `platform/linux.rs` only, under
AD-1's rule, which the architecture amends.

## 7. Success Metrics

Outcomes only; what CI proves sits in the FRs.

| Metric | Target | How measured |
|---|---|---|
| Fresh install | one timed human run at v1, by the owner, in Windows Sandbox on the home machine, following the guide only: under 15 minutes from opening the candidate's artifacts to doctor green (FR-8) with one session on the board, focused by one click. Repeated only when the install or first-run pages of the guide change; any step outside the guide is a guide fix and a new run | run record in research notes |
| Update scenario | FR-14 item 2 passes: the installer completes, no folder mixes versions, the running processes finish, data and approvals are untouched by the installer, doctor's upgrade section lists every constant that moved and the CHANGELOG's Upgrade subsection names each; no refusal or held launch in the scenario whose cause neither the refusal, an existing banner nor doctor names | `docs/qa/release-<version>.md` |
| PATH | byte-identical outside Officina's entry after install and after uninstall, a long PATH included | FR-14's CI install job |
| A week without intervention | 7 consecutive days with at least 5 working days of sessions on the board, on a candidate installed on the workstation: the daily line in the research notes ("touched Officina: no / yes, what") reads "no" every day; zero `unclean_exit`; zero stale period counted against Officina; doctor green at the week's end. A stale period counts against Officina when `claude agents --json` succeeded during it, since the collector then had data; otherwise it is `claude`'s and is listed. Designed upkeep is counted, not an intervention. One restart is allowed after a fix; a second failure goes to the owner instead of looping | research notes, health log, doctor |

The exit criteria are the Fresh install and Week rows. Under section 4.7,
two rows join: a timed install on the named Linux machine under 15 minutes,
package step included; and idle CPU under 1 % with the scene, 10 employees
and 5 waiting over 10 minutes, measured on that machine with its GPU, not in
a VM (R32). The v1 close reads the table with FR-22 (P-16).

## 8. Open Questions

0. **Owner gate, before this PRD is final.** Will you use Officina on a
   Linux machine of your own, weekly, within three months of v1, and if so
   which distribution and desktop? Or does a second user run it on Linux?
   No: Slice B is section 4.6, and section 4.7 moves to a post-v1 "Linux
   product" PRD (a P-7 exception). Yes: Slice B is section 4.7, tested on
   that machine. In the same line, confirm section 4.1's after-v1 groups, and
   say whether an unsupported `.tar.gz` preview is wanted (FR-21).
1. **Gate (FR-0's first), the re-baseline, at v0.8's close.** Resolve each
   derived phrase to the types it then denotes (the closed enums that reach
   the author, the compatibility constants, the long-lived `officina`
   processes, the contained calls, the Linux programs the window ships);
   check every FR's referents exist; restate the doctor-titles story's size;
   check that v0.7's and v0.8's closes recorded their own deferred reads;
   update this PRD. Only then the architecture.
2. **Gate (FR-1, FR-2), Windows install and update**, once, on the home
   machine with a candidate:
   - do the silent install of FR-14's CI job and the interactive one produce
     the same folder, Start menu entry and PATH entry? If not, the CI job
     proves nothing about the interactive path, and the human run covers it;
   - the installer started with the window open: does it ask, close it, or
     fail?
   - `officina.exe` held by an `officina run` waiting on an approval and by
     a long-lived `officina` process: does the replacement succeed, fail or
     retry, and does renaming the image in use work there?
   - uninstall: what remains; SmartScreen on the downloaded installer and on
     the zip's executables.
   If replacing an image in use cannot be made reliable, FR-2 changes: the
   installer lists the running `officina` processes and waits until they
   end, and the guide says to close them first.
3. **Gate (FR-15), section 4.7 only:** on the named machine, the `.deb`
   installs with its dependencies resolved; the window starts and renders;
   single instance works over D-Bus; the dark theme follows the desktop
   live; idle CPU and the halo (R32).
4. **Gate (FR-16), section 4.7 only:** E1, E2, E3, E10, E12, E14, E18 and E4
   on Linux, with the native installer; the hook's latency in a real
   session.
5. **Gate (FR-18), section 4.7 only:** on the named desktop, in its session
   types, does an active-window request raise an editor window, and does the
   first terminal of the list keep an argument with a space intact? A
   refusal makes focus `unsupported` on Linux and drops `x11`.
6. **Gate (FR-17), section 4.7 only:** an action whose program starts a
   grandchild is ended whole at its timeout; what survives `officina run`
   killed with SIGKILL; what Claude Code's Bash tool ends at its own timeout.
7. **Owner, updates (FR-6).** Confirm the manual download with a click that
   opens the latest release, over Tauri's updater, and the trigger to
   revisit.
8. **Owner, PATH (FR-1).** Confirm that the installer appends its folder to
   the user's PATH.
9. **Owner, signing (R7).** Kept at v1. Worth checking after v1: a free
   code-signing programme for open-source projects, or a paid certificate.
   Until then the guide shows how to check the SHA-256 and says what it
   proves.
10. **Owner, the fresh machine.** Recommended: Claude Code installed and
    signed in is a precondition, outside the 15 minutes, since Officina
    cannot shorten it.
11. The values 15 minutes, 7 days, 5 working days, the 5-minute stale
    record, 90 days of health log, 4 weeks before revisiting the updater,
    and 1 % idle CPU are starting values, read at the close.
