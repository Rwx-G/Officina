//! `officina-entry <subcommand>` (AD-108, AD-110, D-608, D-609): the fixed
//! code a workshop container runs. Linux only; on Windows it exits 64. An
//! unknown subcommand or a missing argument is a usage error, exit 64.
//! `--root` and `--claude` move `/` and Claude Code for the tests; in a
//! container the vector never passes them.

#[cfg(unix)]
fn main() {
    use std::path::Path;

    use officina_container::entry::{self, Layout, flag};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let layout = Layout::new(
        flag(&args, "--root").map(Path::new),
        flag(&args, "--claude").map(Path::new),
    );
    let code = match args.first().map(String::as_str) {
        Some("run") => entry::run::run(&layout),
        Some("pack") => match (
            flag(&args, "--branch"),
            flag(&args, "--base"),
            flag(&args, "--clone"),
        ) {
            (Some(branch), Some(base), Some(clone)) => entry::pack::pack(
                &layout,
                &entry::pack::PackArgs {
                    branch: branch.to_owned(),
                    base: base.to_owned(),
                    clone: clone.to_owned(),
                },
            ),
            _ => {
                eprintln!("officina-entry pack: --branch, --base and --clone are required");
                64
            }
        },
        Some("auth-status") => entry::auth_status::auth_status(&layout),
        Some("scan") => entry::scan::scan(&layout),
        Some("inventory") => entry::inventory::inventory(&layout),
        other => {
            eprintln!(
                "officina-entry: unknown subcommand {:?}",
                other.unwrap_or("")
            );
            64
        }
    };
    std::process::exit(code);
}

#[cfg(not(unix))]
fn main() {
    eprintln!("officina-entry runs in a Linux workshop container only");
    std::process::exit(64);
}
