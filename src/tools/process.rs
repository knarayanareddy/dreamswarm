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
    #[cfg(windows)]
    let _job = windows::attach(&child)?;
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

#[cfg(windows)]
mod windows {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A job closes on cancellation and terminates processes still attached to it.
    /// Assignment occurs immediately after spawn; it cannot contain processes
    /// that escaped before assignment, or an administrator's breakaway process.
    pub fn attach(child: &tokio::process::Child) -> std::io::Result<OwnedHandle> {
        // SAFETY: no security attributes or name are supplied; the returned
        // handle is checked and immediately wrapped for automatic closure.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        // SAFETY: the Windows ABI defines the all-zero value as a valid starting
        // value for the limit-information structure.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let process = child
            .raw_handle()
            .ok_or_else(|| std::io::Error::other("child exited before job assignment"))?;
        // SAFETY: both handles are live and owned by the job/child respectively.
        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn cancelling_a_command_kills_job_descendants() {
        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("ready");
        let survived = temp.path().join("survived");
        let quote = |p: &std::path::Path| p.to_string_lossy().replace('\'', "''");
        let child_script = format!(
            "Start-Sleep -Seconds 5; Set-Content -LiteralPath '{}' -Value x",
            quote(&survived)
        );
        let script = format!(
            "$s = '{}'; $e = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($s)); $p = Start-Process powershell.exe -ArgumentList \"-NoProfile -EncodedCommand $e\" -PassThru -WindowStyle Hidden; Set-Content -LiteralPath '{}' -Value x; $p.WaitForExit()",
            child_script.replace('\'', "''"), quote(&ready)
        );
        let task = tokio::spawn(async move {
            let mut cmd = Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-Command", &script]);
            output(&mut cmd).await
        });
        tokio::time::timeout(Duration::from_secs(20), async {
            while !ready.exists() {
                assert!(!task.is_finished(), "parent command exited prematurely");
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .expect("child failed to start");
        task.abort();
        let _ = task.await;
        tokio::time::sleep(Duration::from_secs(6)).await;
        assert!(!survived.exists(), "job descendant survived cancellation");
    }
}
