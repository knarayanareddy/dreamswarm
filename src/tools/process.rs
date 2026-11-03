//! Cancellation-aware process execution for tool subprocesses.
use std::process::{Output, Stdio};
use tokio::process::Command;

/// Run a command while retaining the ability to stop its process group if the
/// future is cancelled. A process that explicitly leaves the group is outside
/// this mechanism; this is not a sandbox.
pub async fn output(command: &mut Command) -> std::io::Result<Output> {
    command
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(working_dir) = super::workspace::working_dir() {
        command.current_dir(working_dir);
    }
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn()?;
    #[cfg(unix)]
    let mut group = ProcessGroup(child.id().map(|id| id as libc::pid_t));
    let output = child.wait_with_output().await;
    #[cfg(unix)]
    {
        group.0 = None;
    }
    output
}

#[cfg(unix)]
struct ProcessGroup(Option<libc::pid_t>);

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Some(id) = self.0 {
            // SAFETY: kill with a negative pid targets the child's process
            // group. The group was created for this command above.
            unsafe {
                libc::kill(-id, libc::SIGKILL);
            }
        }
    }
}
