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
        // The group leader can be reaped before orphaned descendants disappear
        // from the OS process table. Bound this cleanup independently of a
        // cancelled task; callers still reconcile if the group remains alive.
        let until = Instant::now() + Duration::from_millis(200);
        while process_group_alive(self.child.id()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
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
    let remaining = context.remaining().map_err(Error::from)?;
    let budget = remaining
        .checked_sub(Duration::from_secs(config.publish_reserve_seconds))
        .filter(|d| d.as_secs() > 0)
        .ok_or(Error::TimedOut)?;
    let mut command = Command::new(directory.join("collector.bin"));
    let policy = rustdx_complete::tcp::network_policy();
    let network_limit = policy
        .shared
        .as_ref()
        .map_or(policy.max_connections, |shared| shared.max_connections);
    let network_directory = policy.shared.as_ref().map_or_else(
        || directory.join("network"),
        |shared| shared.directory.clone(),
    );
    let network_directory = if network_directory.is_absolute() {
        network_directory
    } else {
        std::env::current_dir()?.join(network_directory)
    };
    // Parent and child cannot transfer a TCP socket. Release the parent's
    // parked startup client before the collector reserves its own connection.
    rustdx_complete::tcp::startup::release_client();
    if config.no_tcp_startup {
        command.arg("--no-tcp-startup");
    }
    command
        .current_dir(directory)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("RUSTDX_NETWORK_DIR", network_directory)
        .args(["--network-limit", &network_limit.to_string()])
        .args(["--timeout", &budget.as_secs().to_string(), "--cancel-file"])
        .arg(directory.join("cancel"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(new_log(&directory.join("stdout.log"))?))
        .stderr(Stdio::from(new_log(&directory.join("stderr.log"))?));
    if config.tcp_day.is_some() {
        // Explicit source does its own three-round probes; generic warmup would
        // park an unused socket and use sources outside the frozen contract.
        command
            .arg("--no-tcp-startup")
            .arg("tcp-day")
            .arg("--directory")
            .arg(directory);
    } else {
        command
            .arg("day")
            .arg(directory.join("raw"))
            .arg("--calendar")
            .arg(directory.join("calendar.json"))
            .arg("--workers")
            .arg(config.workers.to_string())
            .arg("--output")
            .arg(directory.join("collected.csv"))
            .arg("--report")
            .arg(directory.join("collection.json"));
    }
    if !config.no_tcp_startup {
        let initial = std::env::var_os("RUSTDX_TCP_INITIAL_IPS").or_else(|| {
            rustdx_complete::tcp::startup::snapshot().map(|_| {
                rustdx_complete::tcp::ip::default_pool()
                    .list()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
                    .into()
            })
        });
        if let Some(initial) = initial {
            command.env("RUSTDX_TCP_INITIAL_IPS", initial);
        }
    }
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
    // Process liveness/cancellation stays at 100ms; advisory directory accounting
    // is a separate 1s sample. Hard filesystem quotas remain a deployment concern.
    let mut workspace_check_at = Instant::now();
    loop {
        if let Some(status) = worker.child.try_wait()? {
            drop(worker);
            ledger.worker_exited(attempt, status.code())?;
            return match failure {
                Some(e) => Err(e),
                None => {
                    context.remaining()?;
                    if Instant::now() >= stop_at {
                        return Err(Error::TimedOut);
                    }
                    check_workspace(config, directory)?;
                    Ok(status)
                }
            };
        }
        if cancelled_at.is_none() {
            let reason = match context.remaining() {
                Err(error) => Some(error.into()),
                Ok(_) if Instant::now() >= stop_at => Some(Error::TimedOut),
                Ok(_) if Instant::now() >= workspace_check_at => {
                    workspace_check_at = Instant::now() + Duration::from_secs(1);
                    check_workspace(config, directory).err()
                }
                Ok(_) => None,
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

fn check_workspace(config: &Config, directory: &Path) -> Result<()> {
    if storage::tree_bytes(directory)? > config.max_workspace_bytes {
        return Err(invalid("worker exceeded workspace byte budget"));
    }
    Ok(())
}
