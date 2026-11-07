//! `officina-entry auth-status` (D-608): whether the floor's stored login
//! is usable, answered off the network from the stored file only. One line
//! leaves the container; the email and organisation `claude auth status`
//! may print stay in it.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Stdio};

use officina_core::workshop::credential::{AuthStatusLine, method_is_readable};
use officina_core::workshop::vector::LOGIN_DIR;

use super::Layout;

/// The credentials file Claude Code writes in its config dir.
pub const CREDENTIALS_FILE: &str = ".credentials.json";
/// The most of `claude auth status`'s output read.
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// The one line the probe prints, core's type, which the host reads.
pub fn line(logged_in: bool, auth_method: Option<&str>) -> String {
    AuthStatusLine {
        logged_in,
        auth_method: auth_method.map(str::to_owned),
    }
    .to_line()
}

/// Copies the stored credentials into a config dir under `/tmp`, runs
/// `claude auth status` there and prints [`line`]; always exits 0, the
/// answer being the line.
pub fn auth_status(layout: &Layout) -> i32 {
    let config = layout.at("/tmp/officina-auth/config");
    let stored = fs::read(layout.at(LOGIN_DIR).join(CREDENTIALS_FILE));
    let copied = stored.ok().is_some_and(|bytes| {
        fs::create_dir_all(&config).is_ok()
            && fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(config.join(CREDENTIALS_FILE))
                .and_then(|mut f| f.write_all(&bytes))
                .is_ok()
    });
    if !copied {
        println!("{}", line(false, None));
        return 0;
    }
    let output = Command::new(&layout.claude)
        .args(["auth", "status"])
        .env("CLAUDE_CONFIG_DIR", &config)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let (logged_in, method) = match output {
        Ok(out) => {
            let text = &out.stdout[..out.stdout.len().min(MAX_OUTPUT_BYTES)];
            let method = serde_json::from_slice::<serde_json::Value>(text)
                .ok()
                .and_then(|v| {
                    v.get("authMethod")
                        .and_then(|m| m.as_str())
                        .map(str::to_owned)
                })
                .filter(|m| method_is_readable(m));
            (out.status.success(), method)
        }
        Err(_) => (false, None),
    };
    println!("{}", line(logged_in, method.as_deref()));
    0
}
