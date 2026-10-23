//! One process group per worker, absolute deadline and bounded cancellation grace.
use std::{
    fs::{File, OpenOptions},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use rustdx_complete::operation::OperationContext;

use crate::{Error, Result, config::Config, invalid, ledger::Ledger, storage};

pub(crate) struct Worker {
    pub(crate) child: Child,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Clean descendants even if a worker exits before its children.
        terminate_group(self.child.id());
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
pub(crate) fn terminate_group(pid: u32) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: negative PID addresses only the dedicated group established at spawn.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn terminate_group(_pid: u32) {}

pub fn process_group_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return true;
        };
        // SAFETY: signal 0 inspects existence without sending a signal.
        let result = unsafe { libc::kill(-pid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

fn new_log(path: &Path) -> Result<File> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

pub fn collect(
    config: &Config,
    directory: &Path,
    attempt: &str,
    ledger: &Ledger,
    context: &OperationContext,
) -> Result<ExitStatus> {
    let remaining = context.remaining().map_err(|_| Error::Cancelled)?;
    let budget = remaining
        .checked_sub(Duration::from_secs(config.publish_reserve_seconds))
        .filter(|d| d.as_secs() > 0)
        .ok_or(Error::Cancelled)?;
    let mut command = Command::new(directory.join("collector.bin"));
    command
        .current_dir(directory)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("RUSTDX_NETWORK_DIR", directory.join("network"))
        .args(["--timeout", &budget.as_secs().to_string(), "--cancel-file"])
        .arg(directory.join("cancel"))
        .arg("day")
        .arg(directory.join("raw"))
        .arg("--calendar")
        .arg(directory.join("calendar.json"))
        .arg("--workers")
        .arg(config.workers.to_string())
        .arg("--output")
        .arg(directory.join("collected.csv"))
        .arg("--report")
        .arg(directory.join("collection.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(new_log(&directory.join("stdout.log"))?))
        .stderr(Stdio::from(new_log(&directory.join("stderr.log"))?));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    return Err(invalid("managed process groups require Unix"));
    let mut worker = Worker {
        child: command.spawn()?,
    };
    ledger.set_pid(attempt, worker.child.id())?;
    let stop_at = Instant::now() + budget;
    let mut cancelled_at: Option<Instant> = None;
    let mut failure: Option<Error> = None;
    loop {
        if let Some(status) = worker.child.try_wait()? {
            drop(worker);
            ledger.worker_exited(attempt, status.code())?;
            return match failure {
                Some(e) => Err(e),
                None => Ok(status),
            };
        }
        if cancelled_at.is_none() {
            let reason = if context.remaining().is_err() || Instant::now() >= stop_at {
                Some(Error::Cancelled)
            } else {
                match storage::tree_bytes(directory) {
                    Ok(bytes) if bytes <= config.max_workspace_bytes => None,
                    Ok(_) => Some(invalid("worker exceeded workspace byte budget")),
                    Err(e) => Some(e),
                }
            };
            if let Some(reason) = reason {
                storage::write_new(&directory.join("cancel"), b"cancel\n")?;
                cancelled_at = Some(Instant::now());
                failure = Some(reason);
            }
        }
        if cancelled_at
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(config.cancel_grace_seconds))
        {
            let pid = worker.child.id();
            drop(worker);
            if process_group_alive(pid) {
                return Err(invalid(
                    "worker descendants remain; reconciliation required",
                ));
            }
            ledger.worker_exited(attempt, None)?;
            return Err(failure.unwrap_or(Error::Cancelled));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
