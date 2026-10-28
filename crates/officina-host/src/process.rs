//! What every child process Officina starts shares: no console window, and
//! one way to wait for it against a deadline.

use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

/// How often a running child is polled against its deadline.
pub const POLL_TICK: Duration = Duration::from_millis(50);

/// Starts `command` without a console window: Officina runs from the window
/// and from hooks, where a console flashing up for each child would take the
/// focus. Nothing to do off Windows.
pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// How a wait for a child ended.
#[derive(Debug)]
pub enum Waited {
    Exited(ExitStatus),
    /// Still running at the deadline: ending it is the caller's.
    Deadline,
    /// The child could not be waited for.
    Lost(std::io::Error),
}

/// Waits for `child` until `deadline`, a look every [`POLL_TICK`]. Never
/// kills: what a deadline ends (the child, its job) is the caller's choice.
pub fn wait_until(child: &mut Child, deadline: Instant) -> Waited {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Waited::Exited(status),
            Ok(None) if Instant::now() >= deadline => return Waited::Deadline,
            Ok(None) => std::thread::sleep(POLL_TICK),
            Err(e) => return Waited::Lost(e),
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn cmd(script: &str) -> Command {
        let mut command = Command::new("cmd.exe");
        command
            .args(["/d", "/c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        command
    }

    #[test]
    fn an_exit_before_the_deadline_is_its_status() {
        let mut child = cmd("exit 3").spawn().unwrap();
        let waited = wait_until(&mut child, Instant::now() + Duration::from_secs(30));
        assert!(
            matches!(&waited, Waited::Exited(status) if status.code() == Some(3)),
            "{waited:?}"
        );
    }

    #[test]
    fn a_child_running_at_the_deadline_is_left_running() {
        let mut child = cmd("ping -n 30 127.0.0.1").spawn().unwrap();
        let waited = wait_until(&mut child, Instant::now() + POLL_TICK);
        assert!(matches!(waited, Waited::Deadline), "{waited:?}");
        assert!(child.try_wait().unwrap().is_none(), "not killed");
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
