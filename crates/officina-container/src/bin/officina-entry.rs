//! `officina-entry <subcommand>` (AD-108, AD-110, AD-117, AD-118): the
//! fixed code a workshop container runs. It runs in a Linux container only;
//! its subcommands join with the stories that build them (`run` and `pack`
//! with story 6.7), and every other word is a usage error, exit 64.

#[cfg(unix)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    eprintln!(
        "officina-entry: unknown subcommand {:?}; this build knows none yet",
        args.first().map(String::as_str).unwrap_or("")
    );
    std::process::exit(64);
}

#[cfg(not(unix))]
fn main() {
    eprintln!("officina-entry runs in a Linux workshop container only");
    std::process::exit(64);
}
