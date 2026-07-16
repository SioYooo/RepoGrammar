//! Shared process and lock-owner liveness policy.

#[cfg(unix)]
use std::process::Stdio;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessLiveness {
    Live,
    Dead,
    Unknown,
}

pub(crate) fn process_liveness_for_lock(
    pid: u32,
    lock_started_unix_seconds: Option<u64>,
) -> ProcessLiveness {
    platform_process_liveness_for_lock(pid, lock_started_unix_seconds)
}

pub(crate) fn autosync_daemon_process_liveness(
    pid: u32,
    lock_started_unix_seconds: Option<u64>,
) -> ProcessLiveness {
    let liveness = process_liveness_for_lock(pid, lock_started_unix_seconds);
    if liveness != ProcessLiveness::Live {
        return liveness;
    }

    #[cfg(unix)]
    {
        classify_autosync_daemon_command_line(process_command_line(pid).as_deref())
    }
    #[cfg(windows)]
    {
        // Windows keeps the existing existence-only daemon confirmation because
        // this application layer does not own a stable command-line probe there.
        ProcessLiveness::Live
    }
    #[cfg(not(any(unix, windows)))]
    {
        ProcessLiveness::Unknown
    }
}

#[cfg(unix)]
fn classify_autosync_daemon_command_line(command_line: Option<&str>) -> ProcessLiveness {
    match command_line {
        Some(command_line) if command_line_is_autosync_daemon(command_line) => {
            ProcessLiveness::Live
        }
        Some(_) => ProcessLiveness::Dead,
        None => ProcessLiveness::Unknown,
    }
}

/// True when a process command line is a RepoGrammar autosync daemon invocation
/// (`<binary> autosync run ...`). Matching the `autosync` + `run` argument pair
/// rather than a bare substring avoids misidentifying another subcommand run
/// against a repository whose path merely contains `autosync`.
pub(crate) fn command_line_is_autosync_daemon(command_line: &str) -> bool {
    let tokens = command_line.split_whitespace().collect::<Vec<_>>();
    tokens
        .windows(2)
        .any(|pair| pair[0] == "autosync" && pair[1] == "run")
}

#[cfg(unix)]
fn platform_process_liveness_for_lock(
    pid: u32,
    lock_started_unix_seconds: Option<u64>,
) -> ProcessLiveness {
    const MAX_POSITIVE_PID_T: u32 = i32::MAX as u32;

    if pid == 0 || pid > MAX_POSITIVE_PID_T {
        return ProcessLiveness::Dead;
    }
    match std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
    {
        Ok(output) if output.status.success() => classify_live_process_for_lock(
            process_start_unix_seconds(pid),
            lock_started_unix_seconds,
        ),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Operation not permitted") || stderr.contains("not permitted") {
                ProcessLiveness::Unknown
            } else {
                ProcessLiveness::Dead
            }
        }
        Err(_) => ProcessLiveness::Unknown,
    }
}

#[cfg(unix)]
fn classify_live_process_for_lock(
    process_started_unix_seconds: Option<u64>,
    lock_started_unix_seconds: Option<u64>,
) -> ProcessLiveness {
    // `ps etimes` exposes only whole elapsed seconds. Around a wall-clock
    // second boundary it can therefore make a process that created the lock
    // appear to have started one second after that lock. Preserve ownership
    // for that single probe-granularity interval; a larger gap still proves
    // that the PID was reused after the recorded owner disappeared.
    const PROCESS_START_PROBE_GRANULARITY_SECONDS: u64 = 1;
    match (process_started_unix_seconds, lock_started_unix_seconds) {
        (Some(process_started), Some(lock_started))
            if process_started
                > lock_started.saturating_add(PROCESS_START_PROBE_GRANULARITY_SECONDS) =>
        {
            ProcessLiveness::Dead
        }
        _ => ProcessLiveness::Live,
    }
}

#[cfg(unix)]
fn process_start_unix_seconds(pid: u32) -> Option<u64> {
    let pid_text = pid.to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "etimes=", "-p", pid_text.as_str()])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let elapsed_seconds = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.trim().parse::<u64>().ok())?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    now.checked_sub(elapsed_seconds)
}

#[cfg(unix)]
fn process_command_line(pid: u32) -> Option<String> {
    let pid_text = pid.to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", pid_text.as_str()])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(windows)]
fn platform_process_liveness_for_lock(
    pid: u32,
    _lock_started_unix_seconds: Option<u64>,
) -> ProcessLiveness {
    if pid == std::process::id() {
        return ProcessLiveness::Live;
    }
    if pid == 0 {
        return ProcessLiveness::Dead;
    }
    windows_process_liveness(pid)
}

#[cfg(not(any(unix, windows)))]
fn platform_process_liveness_for_lock(
    pid: u32,
    _lock_started_unix_seconds: Option<u64>,
) -> ProcessLiveness {
    if pid == std::process::id() {
        return ProcessLiveness::Live;
    }
    if pid == 0 {
        return ProcessLiveness::Dead;
    }
    ProcessLiveness::Unknown
}

#[cfg(windows)]
fn windows_process_liveness(pid: u32) -> ProcessLiveness {
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            ProcessLiveness::Dead
        } else {
            ProcessLiveness::Unknown
        };
    }
    let handle = WindowsProcessHandle(handle);
    let mut exit_code = 0_u32;
    let ok = unsafe { GetExitCodeProcess(handle.0, &mut exit_code) != 0 };
    if !ok {
        return ProcessLiveness::Unknown;
    }
    if exit_code == STILL_ACTIVE {
        ProcessLiveness::Live
    } else {
        ProcessLiveness::Dead
    }
}

#[cfg(windows)]
struct WindowsProcessHandle(*mut std::ffi::c_void);

#[cfg(windows)]
impl Drop for WindowsProcessHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(
        desired_access: u32,
        inherit_handle: i32,
        process_id: u32,
    ) -> *mut std::ffi::c_void;
    fn GetExitCodeProcess(process: *mut std::ffi::c_void, exit_code: *mut u32) -> i32;
    fn GetLastError() -> u32;
    fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_reported_live_and_invalid_pids_are_dead() {
        assert_eq!(
            process_liveness_for_lock(std::process::id(), None),
            ProcessLiveness::Live
        );
        assert_eq!(process_liveness_for_lock(0, None), ProcessLiveness::Dead);
    }

    #[cfg(unix)]
    #[test]
    fn pid_values_outside_positive_pid_t_are_dead() {
        assert_eq!(
            process_liveness_for_lock(u32::MAX, None),
            ProcessLiveness::Dead
        );
    }

    #[cfg(unix)]
    #[test]
    fn live_process_start_probe_tolerates_one_second_rounding_only() {
        assert_eq!(
            classify_live_process_for_lock(Some(20), Some(10)),
            ProcessLiveness::Dead
        );
        assert_eq!(
            classify_live_process_for_lock(Some(12), Some(10)),
            ProcessLiveness::Dead
        );
        assert_eq!(
            classify_live_process_for_lock(Some(11), Some(10)),
            ProcessLiveness::Live
        );
        assert_eq!(
            classify_live_process_for_lock(Some(10), Some(10)),
            ProcessLiveness::Live
        );
        assert_eq!(
            classify_live_process_for_lock(None, Some(10)),
            ProcessLiveness::Live
        );
    }

    #[test]
    fn autosync_daemon_command_line_is_recognized_precisely() {
        assert!(command_line_is_autosync_daemon(
            "/opt/bin/repogrammar autosync run --path /repo --quiet"
        ));
        assert!(!command_line_is_autosync_daemon(
            "/opt/bin/repogrammar index --path /home/user/autosync-project"
        ));
        assert!(!command_line_is_autosync_daemon("/usr/bin/vim /etc/hosts"));
        assert!(!command_line_is_autosync_daemon("autosync"));
        assert!(!command_line_is_autosync_daemon(""));
    }

    #[cfg(unix)]
    #[test]
    fn unavailable_daemon_command_probe_is_unknown_not_dead() {
        assert_eq!(
            classify_autosync_daemon_command_line(None),
            ProcessLiveness::Unknown
        );
        assert_eq!(
            classify_autosync_daemon_command_line(Some(
                "/opt/bin/repogrammar autosync run --quiet"
            )),
            ProcessLiveness::Live
        );
        assert_eq!(
            classify_autosync_daemon_command_line(Some("/usr/bin/vim /etc/hosts")),
            ProcessLiveness::Dead
        );
    }
}
