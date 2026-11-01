//! `officina-entry` on Windows (story 6.5 AC 1, review F23): the crate
//! compiles in the `windows` job's workspace build, and the program says it
//! runs in a Linux container only, exit 64.
#![cfg(windows)]

#[test]
fn officina_entry_exits_64_on_windows() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_officina-entry"))
        .arg("run")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Linux"));
}
