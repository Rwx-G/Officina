//! The hook-program inventory (AD-118, D-211): which programs a project's
//! hooks and stdio MCP servers name, extracted here from the settings files
//! and `.mcp.json` the host read, resolved in the image by `officina-entry
//! inventory`, and judged here: a `PreToolUse` guard whose program is
//! missing refuses the launch, since under `bypassPermissions` a guard that
//! cannot start would be a guard that is gone.

use serde::{Deserialize, Serialize};

/// The most bytes of one settings file or of `.mcp.json` read (story 6.9).
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
/// The most commands one inventory holds.
pub const MAX_COMMANDS: usize = 256;
/// The most bytes of a tracked script's first line the host passes.
pub const MAX_FIRST_LINE_BYTES: usize = 256;
/// The variable a hook command may name its project by, the one the
/// inventory expands (AD-118).
pub const PROJECT_DIR_VAR: &str = "CLAUDE_PROJECT_DIR";
/// The event whose hooks are guards: a missing program refuses (FR-6).
pub const GUARD_EVENT: &str = "PreToolUse";

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a settings file comes from (D-211): a mounted `.claude`, or
    /// the base commit when the project tracks its settings (review F12).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum SettingsSource {
        MountedSettings,
        MountedLocal,
        TrackedSettings,
        TrackedLocal,
    }
}

/// One settings file as the host read it, bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFile {
    pub source: SettingsSource,
    pub bytes: Vec<u8>,
}

/// A hook program that is a tracked file of the project (D-211): its path
/// relative to the project, whether the base commit holds it, and its
/// first line, raw, at most [`MAX_FIRST_LINE_BYTES`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedScript {
    pub path: String,
    pub present: bool,
    pub first_line: Option<String>,
    /// The blob id at the base commit, which the key covers.
    pub blob: Option<String>,
}

/// One command the inventory resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryCommand {
    /// The hook's event, or `None` for an MCP server.
    pub event: Option<String>,
    pub matcher: Option<String>,
    /// The MCP server's name, for an MCP server.
    pub server: Option<String>,
    /// The command text as written (a hook), or the server's command and
    /// arguments joined by spaces.
    pub command: String,
    /// Set when the program is a path under the project outside every
    /// mount: the host fills it from the base commit (D-211).
    pub tracked: Option<TrackedScript>,
}

/// Why an inventory's input is refused: never truncated, so nothing a
/// file says is silently left out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InventoryError {
    #[error("{0:?} is larger than {MAX_INPUT_BYTES} bytes")]
    SettingsTooLarge(SettingsSource),
    #[error(".mcp.json is larger than {MAX_INPUT_BYTES} bytes")]
    McpTooLarge,
    #[error("more than {MAX_COMMANDS} commands")]
    TooManyCommands,
}

fn hooks_of(value: &serde_json::Value, out: &mut Vec<InventoryCommand>) {
    let Some(events) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return;
    };
    for (event, groups) in events {
        for group in groups.as_array().into_iter().flatten() {
            let matcher = group
                .get("matcher")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            for hook in group
                .get("hooks")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                let is_command =
                    hook.get("type").and_then(serde_json::Value::as_str) == Some("command");
                if let (true, Some(command)) = (
                    is_command,
                    hook.get("command").and_then(serde_json::Value::as_str),
                ) {
                    out.push(InventoryCommand {
                        event: Some(event.clone()),
                        matcher: matcher.clone(),
                        server: None,
                        command: command.to_owned(),
                        tracked: None,
                    });
                }
            }
        }
    }
}

fn servers_of(value: &serde_json::Value, out: &mut Vec<InventoryCommand>) {
    let Some(servers) = value
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
    else {
        return;
    };
    for (name, server) in servers {
        let stdio = matches!(
            server.get("type").and_then(serde_json::Value::as_str),
            None | Some("stdio")
        );
        let Some(command) = server.get("command").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !stdio {
            continue;
        }
        let mut text = command.to_owned();
        for arg in server
            .get("args")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            text.push(' ');
            text.push_str(arg);
        }
        out.push(InventoryCommand {
            event: None,
            matcher: None,
            server: Some(name.clone()),
            command: text,
            tracked: None,
        });
    }
}

/// The project-relative path of a command's program when it is written as
/// `$CLAUDE_PROJECT_DIR/...` (bare, braced or double-quoted), else `None`.
pub fn project_path(command: &str) -> Option<String> {
    let first = command.trim_start();
    let forms = [
        format!("\"${PROJECT_DIR_VAR}\"/"),
        format!("\"${{{PROJECT_DIR_VAR}}}\"/"),
        format!("\"${PROJECT_DIR_VAR}/"),
        format!("\"${{{PROJECT_DIR_VAR}}}/"),
        format!("${{{PROJECT_DIR_VAR}}}/"),
        format!("${PROJECT_DIR_VAR}/"),
    ];
    let rest = forms
        .iter()
        .find_map(|form| first.strip_prefix(form.as_str()))?;
    let path: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '"')
        .collect();
    (!path.is_empty() && !path.split('/').any(|s| s == "..")).then_some(path)
}

/// True when `path`, relative to the project, lies in one of `mounts`
/// (segment by segment, ASCII case ignored as NTFS compares).
fn under_mount(path: &str, mounts: &[String]) -> bool {
    let segments: Vec<String> = path.split('/').map(str::to_ascii_lowercase).collect();
    mounts.iter().any(|mount| {
        let mount: Vec<String> = mount
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        !mount.is_empty() && segments.len() > mount.len() && segments[..mount.len()] == mount[..]
    })
}

/// Every command the inventory checks (AD-118, D-211): each hook of the
/// settings files, in their order, and each stdio MCP server of `.mcp.json`;
/// a hook whose program is a path under the project outside every mount
/// is marked tracked, with its path, for the host to read at the base
/// commit. A file that does not parse lists nothing; an input over its
/// bound refuses the inventory, never truncated.
pub fn extract(
    settings: &[SettingsFile],
    mcp_json: Option<&[u8]>,
    mounts: &[String],
) -> Result<Vec<InventoryCommand>, InventoryError> {
    let mut commands = Vec::new();
    for file in settings {
        if file.bytes.len() > MAX_INPUT_BYTES {
            return Err(InventoryError::SettingsTooLarge(file.source));
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&file.bytes) {
            hooks_of(&value, &mut commands);
        }
    }
    if let Some(bytes) = mcp_json {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(InventoryError::McpTooLarge);
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) {
            servers_of(&value, &mut commands);
        }
    }
    if commands.len() > MAX_COMMANDS {
        return Err(InventoryError::TooManyCommands);
    }
    for command in &mut commands {
        if command.event.is_some()
            && let Some(path) = project_path(&command.command)
            && !under_mount(&path, mounts)
        {
            command.tracked = Some(TrackedScript {
                path,
                present: false,
                first_line: None,
                blob: None,
            });
        }
    }
    Ok(commands)
}

/// `inventory_key` (AD-118, D-211): the SHA-256 of the command list (their
/// texts and tracked paths), the tracked files' blob ids and the image id,
/// so a changed hook, script or image runs the inventory again.
pub fn inventory_key(commands: &[InventoryCommand], image_id: &str) -> String {
    let mut text = String::new();
    for command in commands {
        let line = serde_json::json!({
            "event": command.event,
            "matcher": command.matcher,
            "server": command.server,
            "command": command.command,
            "tracked": command.tracked.as_ref().map(|t| (&t.path, t.present, &t.blob)),
        });
        text.push_str(&line.to_string());
        text.push('\n');
    }
    text.push_str(image_id);
    crate::sha256_hex(text.as_bytes())
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How one command's program resolved in the image (AD-118).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Resolution {
        /// Found, with its path.
        Found,
        /// Not found; the detail says what (`crlf_shebang`, `interpreter`,
        /// `not_on_path`, `not_a_file`, `tracked_absent`).
        Missing,
        /// Computed at run time: not resolvable without running it.
        Dynamic,
    }
}

/// One command's result, in the list's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved {
    pub index: usize,
    pub resolution: Resolution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What a launch makes of an inventory (AD-118, FR-6): whether a guard's
/// program is missing (`hook_program_missing`), and the warnings the card
/// shows (a `dynamic` command, a missing program of another event or of an
/// MCP server), by command index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Consequences {
    pub refused: Vec<usize>,
    pub warnings: Vec<usize>,
}

/// Judges `results` against `commands` (AD-118): only a missing
/// `PreToolUse` program refuses.
pub fn consequences(commands: &[InventoryCommand], results: &[Resolved]) -> Consequences {
    let mut out = Consequences::default();
    for result in results {
        let Some(command) = commands.get(result.index) else {
            continue;
        };
        let guard = command.event.as_deref() == Some(GUARD_EVENT);
        match result.resolution {
            Resolution::Found => {}
            Resolution::Missing if guard => out.refused.push(result.index),
            Resolution::Missing | Resolution::Dynamic => out.warnings.push(result.index),
        }
    }
    out
}

/// A tracked script's first line as the host passes it: cut at the first
/// newline and at [`MAX_FIRST_LINE_BYTES`], a carriage return kept, since a
/// CRLF shebang is what the inventory must see (review F12).
pub fn first_line(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|b| *b == b'\n')
        .unwrap_or(bytes.len())
        .min(MAX_FIRST_LINE_BYTES);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// What `officina-entry inventory` reads on stdin (AD-118): the project's
/// folder in the container, its mounts' relative paths (linked there to
/// `/work/.officina/mounts/<i>`, as the worker's are) and the commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryInput {
    pub schema: u32,
    pub project_dir: String,
    pub mounts: Vec<String>,
    pub commands: Vec<InventoryCommand>,
}

/// The schema of [`InventoryInput`].
pub const INVENTORY_INPUT_SCHEMA: u32 = 1;

/// `command` with every spelling of `$CLAUDE_PROJECT_DIR` replaced by
/// `project_dir`, the only expansion the inventory makes (AD-118); any
/// other expansion stays and makes the program `dynamic`.
pub fn expand_project_dir(command: &str, project_dir: &str) -> String {
    command
        .replace(&format!("${{{PROJECT_DIR_VAR}}}"), project_dir)
        .replace(&format!("${PROJECT_DIR_VAR}"), project_dir)
}

/// A script's first line, read raw (AD-118, review F12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shebang {
    /// No `#!`: the shell runs it.
    None,
    /// A carriage return in the line: `#!/usr/bin/env bash\r` exits 127.
    Crlf,
    /// `#!/usr/bin/env x`: `x` resolved on `PATH`.
    Env(String),
    /// `#!/path/to/interpreter`: that file.
    Path(String),
}

/// Reads a first line's shebang.
pub fn shebang(first_line: &str) -> Shebang {
    let Some(rest) = first_line.strip_prefix("#!") else {
        return Shebang::None;
    };
    if rest.contains('\r') {
        return Shebang::Crlf;
    }
    let mut words = rest.split_whitespace();
    let Some(program) = words.next() else {
        return Shebang::None;
    };
    if program.rsplit('/').next() == Some("env") {
        let target = words.find(|w| !w.starts_with('-') && !w.contains('='));
        return target.map_or(Shebang::None, |t| Shebang::Env(t.to_owned()));
    }
    Shebang::Path(program.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shebangs_are_read_raw() {
        assert_eq!(shebang("#!/usr/bin/env bash"), Shebang::Env("bash".into()));
        assert_eq!(
            shebang("#!/usr/bin/env -S python3 -u"),
            Shebang::Env("python3".into())
        );
        assert_eq!(shebang("#!/bin/sh -e"), Shebang::Path("/bin/sh".into()));
        assert_eq!(shebang("#!/usr/bin/env bash\r"), Shebang::Crlf);
        assert_eq!(shebang("echo hi"), Shebang::None);
        assert_eq!(
            expand_project_dir(
                "\"${CLAUDE_PROJECT_DIR}/a\" $CLAUDE_PROJECT_DIR/b $HOME",
                "/work/api"
            ),
            "\"/work/api/a\" /work/api/b $HOME"
        );
    }

    fn settings(source: SettingsSource, json: &str) -> SettingsFile {
        SettingsFile {
            source,
            bytes: json.as_bytes().to_vec(),
        }
    }

    const GUARD: &str = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"python \"$CLAUDE_PROJECT_DIR/.claude/hooks/guard.py\""}]}],"Stop":[{"hooks":[{"type":"command","command":"$CLAUDE_PROJECT_DIR/scripts/notify.sh"}]}]}}"#;

    #[test]
    fn oversized_inputs_are_refused_not_truncated() {
        let big = format!("{{\"x\":\"{}\"}}", "a".repeat(MAX_INPUT_BYTES));
        assert_eq!(
            extract(
                &[settings(SettingsSource::MountedSettings, &big)],
                None,
                &[]
            ),
            Err(InventoryError::SettingsTooLarge(
                SettingsSource::MountedSettings
            ))
        );
        assert_eq!(
            extract(&[], Some(big.as_bytes()), &[]),
            Err(InventoryError::McpTooLarge)
        );
        let hook = r#"{"type":"command","command":"true"}"#;
        let many = format!(
            "{{\"hooks\":{{\"Stop\":[{{\"hooks\":[{}]}}]}}}}",
            vec![hook; MAX_COMMANDS + 1].join(",")
        );
        assert_eq!(
            extract(&[settings(SettingsSource::MountedLocal, &many)], None, &[]),
            Err(InventoryError::TooManyCommands)
        );
    }

    #[test]
    fn a_tracked_script_absent_at_the_base_is_missing() {
        let commands = extract(
            &[settings(SettingsSource::TrackedSettings, GUARD)],
            None,
            &[".claude".into()],
        )
        .unwrap();
        // The guard's program is `python`; its script is under the mount.
        // The Stop hook's program is a tracked script outside it.
        let stop = commands
            .iter()
            .find(|c| c.event.as_deref() == Some("Stop"))
            .unwrap();
        let tracked = stop.tracked.as_ref().expect("marked tracked");
        assert_eq!(tracked.path, "scripts/notify.sh");
        assert!(
            !tracked.present,
            "the host fills presence; absent until then"
        );
        let guard = commands
            .iter()
            .find(|c| c.event.as_deref() == Some(GUARD_EVENT))
            .unwrap();
        assert_eq!(guard.tracked, None);
    }

    #[test]
    fn a_tracked_pretooluse_hook_is_checked() {
        let tracked = r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"${CLAUDE_PROJECT_DIR}/tools/guard.sh --strict"}]}]}}"#;
        let commands = extract(
            &[settings(SettingsSource::TrackedSettings, tracked)],
            None,
            &[".claude".into()],
        )
        .unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].matcher.as_deref(), Some("Edit"));
        assert_eq!(
            commands[0].tracked.as_ref().map(|t| t.path.as_str()),
            Some("tools/guard.sh")
        );
        let missing = Resolved {
            index: 0,
            resolution: Resolution::Missing,
            path: None,
            detail: Some("tracked_absent".into()),
        };
        assert_eq!(consequences(&commands, &[missing]).refused, vec![0]);
    }

    #[test]
    fn only_a_missing_pretooluse_program_refuses() {
        let commands = extract(
            &[settings(SettingsSource::MountedSettings, GUARD)],
            None,
            &[],
        )
        .unwrap();
        let guard = commands
            .iter()
            .position(|c| c.event.as_deref() == Some(GUARD_EVENT))
            .unwrap();
        for resolution in Resolution::ALL {
            let result = Resolved {
                index: guard,
                resolution: *resolution,
                path: None,
                detail: None,
            };
            let judged = consequences(&commands, &[result]);
            assert_eq!(
                judged.refused.is_empty(),
                *resolution != Resolution::Missing,
                "{resolution:?}"
            );
        }
    }

    #[test]
    fn other_events_and_mcp_servers_warn() {
        let mcp = br#"{"mcpServers":{"db":{"command":"uvx","args":["mcp-db"]},"web":{"type":"http","url":"https://mcp.example.com"}}}"#;
        let commands = extract(
            &[settings(SettingsSource::MountedSettings, GUARD)],
            Some(mcp),
            &[],
        )
        .unwrap();
        let server = commands
            .iter()
            .position(|c| c.server.as_deref() == Some("db"))
            .unwrap();
        assert_eq!(commands[server].command, "uvx mcp-db");
        assert!(
            commands.iter().all(|c| c.server.as_deref() != Some("web")),
            "a remote server starts no program"
        );
        let stop = commands
            .iter()
            .position(|c| c.event.as_deref() == Some("Stop"))
            .unwrap();
        let results: Vec<Resolved> = [server, stop]
            .iter()
            .flat_map(|index| {
                [Resolution::Missing, Resolution::Dynamic].map(|resolution| Resolved {
                    index: *index,
                    resolution,
                    path: None,
                    detail: None,
                })
            })
            .collect();
        let judged = consequences(&commands, &results);
        assert!(judged.refused.is_empty());
        assert_eq!(judged.warnings.len(), 4);
    }

    #[test]
    fn the_first_line_keeps_a_carriage_return() {
        assert_eq!(
            first_line(b"#!/usr/bin/env bash\r\necho"),
            "#!/usr/bin/env bash\r"
        );
        assert_eq!(first_line(&[b'x'; 400]).len(), MAX_FIRST_LINE_BYTES);
    }

    #[test]
    fn project_paths_take_every_spelling_and_nothing_else() {
        for command in [
            "$CLAUDE_PROJECT_DIR/a/b.sh x",
            "${CLAUDE_PROJECT_DIR}/a/b.sh",
            "\"$CLAUDE_PROJECT_DIR\"/a/b.sh",
            "\"$CLAUDE_PROJECT_DIR/a/b.sh\" --flag",
        ] {
            assert_eq!(
                project_path(command).as_deref(),
                Some("a/b.sh"),
                "{command}"
            );
        }
        for command in ["python a.py", "$HOME/a.sh", "$CLAUDE_PROJECT_DIR/../x.sh"] {
            assert_eq!(project_path(command), None, "{command}");
        }
    }
}
