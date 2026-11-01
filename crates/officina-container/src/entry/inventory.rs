//! `officina-entry inventory` (AD-118, D-211): reads the command list on
//! stdin, links the mounts at their places in the project's tmpfs folder,
//! and resolves each command's program with the gate's lexer and the
//! image's own `PATH`, printing one result per command. It runs no hook.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use officina_core::relay::shell::lex;
use officina_core::workshop::inventory::{
    INVENTORY_INPUT_SCHEMA, InventoryCommand, InventoryInput, Resolution, Resolved, Shebang,
    expand_project_dir, first_line, shebang,
};
use officina_core::workshop::vector::MOUNTS_DIR;

use super::Layout;

/// The most input read on stdin.
pub const MAX_INPUT_BYTES: u64 = 2 * 1024 * 1024;

fn missing(index: usize, detail: &str) -> Resolved {
    Resolved {
        index,
        resolution: Resolution::Missing,
        path: None,
        detail: Some(detail.to_owned()),
    }
}

fn found(index: usize, path: &Path) -> Resolved {
    Resolved {
        index,
        resolution: Resolution::Found,
        path: Some(path.to_string_lossy().into_owned()),
        detail: None,
    }
}

fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// `name` on the image's `PATH`.
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|candidate| executable(candidate))
}

/// The interpreter a script's first line names, resolved; `Err` with the
/// missing detail.
fn interpreter(line: &str) -> Result<(), &'static str> {
    match shebang(line) {
        Shebang::None => Ok(()),
        Shebang::Crlf => Err("crlf_shebang"),
        Shebang::Env(name) => on_path(&name).map(|_| ()).ok_or("interpreter"),
        Shebang::Path(path) => executable(Path::new(&path))
            .then_some(())
            .ok_or("interpreter"),
    }
}

fn script_line(path: &Path) -> String {
    let mut bytes = Vec::new();
    let _ = fs::File::open(path).and_then(|f| f.take(256).read_to_end(&mut bytes));
    first_line(&bytes)
}

fn resolve(
    index: usize,
    command: &InventoryCommand,
    input: &InventoryInput,
    layout: &Layout,
) -> Resolved {
    if let Some(tracked) = &command.tracked {
        if !tracked.present {
            return missing(index, "tracked_absent");
        }
        let path = PathBuf::from(&input.project_dir).join(&tracked.path);
        return match interpreter(tracked.first_line.as_deref().unwrap_or_default()) {
            Ok(()) => found(index, &path),
            Err(detail) => missing(index, detail),
        };
    }
    let text = expand_project_dir(&command.command, &input.project_dir);
    let lexed = match lex(&text) {
        Ok(lexed) => lexed,
        Err(_) => {
            return Resolved {
                index,
                resolution: Resolution::Dynamic,
                path: None,
                detail: Some("lex".to_owned()),
            };
        }
    };
    if !lexed.dynamic.is_empty() {
        return Resolved {
            index,
            resolution: Resolution::Dynamic,
            path: None,
            detail: None,
        };
    }
    let Some(program) = lexed.programs.first() else {
        return missing(index, "no_program");
    };
    let written = program.written.as_str();
    let resolved = if written.contains('/') {
        let path = layout.at(written);
        executable(&path).then_some(path)
    } else {
        on_path(written)
    };
    let Some(path) = resolved else {
        return missing(
            index,
            if written.contains('/') {
                "not_a_file"
            } else {
                "not_on_path"
            },
        );
    };
    match interpreter(&script_line(&path)) {
        Ok(()) => found(index, &path),
        Err(detail) => missing(index, detail),
    }
}

/// Links each mount at its place in the project folder, as the worker's
/// entry point does (D-211).
fn link_mounts(input: &InventoryInput, layout: &Layout) -> Result<(), String> {
    let project = layout.at(&input.project_dir);
    fs::create_dir_all(&project).map_err(|e| e.to_string())?;
    for (i, mount) in input.mounts.iter().enumerate() {
        let place = project.join(mount);
        if let Some(parent) = place.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        symlink(layout.at(&format!("{MOUNTS_DIR}/{i}")), &place)
            .map_err(|e| format!("{mount}: {e}"))?;
    }
    Ok(())
}

/// Runs the inventory: exit 0 with one JSON line per command, or 1 with
/// one error line on stderr.
pub fn inventory(layout: &Layout) -> i32 {
    let mut text = String::new();
    if std::io::stdin()
        .take(MAX_INPUT_BYTES)
        .read_to_string(&mut text)
        .is_err()
    {
        eprintln!("officina-entry inventory: stdin is not UTF-8");
        return 1;
    }
    let input: InventoryInput = match serde_json::from_str(&text) {
        Ok(input) => input,
        Err(e) => {
            eprintln!("officina-entry inventory: the list does not parse: {e}");
            return 1;
        }
    };
    if input.schema != INVENTORY_INPUT_SCHEMA {
        eprintln!("officina-entry inventory: unknown schema {}", input.schema);
        return 1;
    }
    if let Err(why) = link_mounts(&input, layout) {
        eprintln!("officina-entry inventory: {why}");
        return 1;
    }
    let mut out = std::io::stdout().lock();
    for (index, command) in input.commands.iter().enumerate() {
        let result = resolve(index, command, &input, layout);
        let line = serde_json::to_string(&result).unwrap_or_default();
        if writeln!(out, "{line}").is_err() {
            return 1;
        }
    }
    0
}
