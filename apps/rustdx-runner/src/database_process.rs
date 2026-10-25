//! One bounded pipe worker and one process group per MongoDB target session.
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{Error, Result, invalid, supervisor::Worker};

const TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

enum Request {
    Exchange { bytes: Vec<u8>, evidence: File },
    Finish,
}

pub(crate) struct Process {
    worker: Option<Worker>,
    requests: Option<SyncSender<Request>>,
    replies: Option<Receiver<io::Result<Vec<u8>>>>,
    pipe: Option<JoinHandle<()>>,
    _permit: rustdx_complete::tcp::NetworkPermit,
}

impl Process {
    pub(crate) fn spawn(command: &mut Command, limit: u64) -> Result<Self> {
        let permit =
            rustdx_complete::tcp::acquire_network(rustdx_complete::operation::timeout(TIMEOUT)?)?;
        rustdx_complete::operation::check().map_err(Error::from)?;
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut worker = Worker {
            child: command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?,
        };
        let stdin = worker.child.stdin.take().expect("piped database stdin");
        let stdout = worker.child.stdout.take().expect("piped database stdout");
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(1);
        let pipe = thread::Builder::new()
            .name("runner-mongodb".into())
            .spawn(move || {
                let mut stdin = Some(stdin);
                let mut reader = BufReader::new(stdout);
                for request in incoming {
                    let finish = matches!(request, Request::Finish);
                    let result = match request {
                        Request::Exchange {
                            bytes,
                            mut evidence,
                        } => {
                            let result = (|| {
                                let input = stdin.as_mut().expect("exchange before finish");
                                input.write_all(&bytes)?;
                                input.write_all(b"\n")?;
                                input.flush()?;
                                let mut bytes = Vec::new();
                                loop {
                                    let buffer = reader.fill_buf()?;
                                    if buffer.is_empty() {
                                        return Err(io::Error::new(
                                            io::ErrorKind::UnexpectedEof,
                                            "MongoDB response is incomplete",
                                        ));
                                    }
                                    let available = (limit + 1).saturating_sub(bytes.len() as u64);
                                    let n = buffer
                                        .iter()
                                        .position(|&b| b == b'\n')
                                        .map_or(buffer.len(), |n| n + 1)
                                        .min(available as usize);
                                    evidence.write_all(&buffer[..n])?;
                                    bytes.extend_from_slice(&buffer[..n]);
                                    reader.consume(n);
                                    if bytes.len() as u64 > limit {
                                        return Err(io::Error::other(
                                            "MongoDB response limit exceeded",
                                        ));
                                    }
                                    if bytes.last() == Some(&b'\n') {
                                        return Ok(bytes);
                                    }
                                }
                            })();
                            let synced = evidence.sync_all();
                            result.and_then(|bytes| synced.map(|()| bytes))
                        }
                        Request::Finish => {
                            drop(stdin.take());
                            let mut extra = [0];
                            reader.read(&mut extra).and_then(|n| {
                                if n == 0 {
                                    Ok(Vec::new())
                                } else {
                                    Err(io::Error::other("MongoDB sent an extra response"))
                                }
                            })
                        }
                    };
                    let failed = result.is_err();
                    if outgoing.send(result).is_err() || finish || failed {
                        break;
                    }
                }
            })?;
        Ok(Self {
            worker: Some(worker),
            requests: Some(requests),
            replies: Some(replies),
            pipe: Some(pipe),
            _permit: permit,
        })
    }

    fn request(&mut self, request: Request) -> Result<Vec<u8>> {
        let timeout = rustdx_complete::operation::timeout(TIMEOUT)?;
        let start = Instant::now();
        self.requests
            .as_ref()
            .ok_or_else(|| invalid("MongoDB session closed"))?
            .try_send(request)
            .map_err(|_| invalid("MongoDB request pipe unavailable"))?;
        loop {
            rustdx_complete::operation::check().map_err(Error::from)?;
            if start.elapsed() >= timeout {
                return Err(Error::TimedOut);
            }
            match self
                .replies
                .as_ref()
                .ok_or_else(|| invalid("MongoDB session closed"))?
                .recv_timeout(POLL.min(timeout.saturating_sub(start.elapsed())))
            {
                Ok(result) => return Ok(result?),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(invalid("MongoDB response pipe closed")),
            }
        }
    }

    pub(crate) fn exchange(&mut self, bytes: Vec<u8>, evidence: File) -> Result<Vec<u8>> {
        let result = self.request(Request::Exchange { bytes, evidence });
        if result.is_err() {
            self.close();
        }
        result
    }

    pub(crate) fn finish(&mut self) -> Result<()> {
        self.request(Request::Finish)?;
        let start = Instant::now();
        let timeout = rustdx_complete::operation::timeout(TIMEOUT)?;
        loop {
            rustdx_complete::operation::check().map_err(Error::from)?;
            let status = self
                .worker
                .as_mut()
                .ok_or_else(|| invalid("MongoDB session closed"))?
                .child
                .try_wait()?;
            match status {
                Some(status) if status.success() => {
                    self.close();
                    return Ok(());
                }
                Some(_) => return Err(invalid("MongoDB session exited unsuccessfully")),
                None if start.elapsed() < timeout => thread::sleep(POLL),
                None => return Err(Error::TimedOut),
            }
        }
    }

    fn close(&mut self) {
        self.requests.take();
        self.replies.take();
        // Killing the entire group releases both a blocked writer and reader.
        self.worker.take();
        if let Some(pipe) = self.pipe.take() {
            let _ = pipe.join();
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rustdx_complete::operation::OperationContext;

    fn evidence() -> File {
        tempfile::tempfile().unwrap()
    }

    #[test]
    fn one_process_handles_batches_and_requires_clean_exit() {
        for (script, success) in [
            ("while read r; do printf 'ok\\n'; done", true),
            ("while read r; do printf 'ok\\n'; done; exit 7", false),
            ("while read r; do printf 'ok\\n'; done; printf extra", false),
        ] {
            let mut process =
                Process::spawn(Command::new("sh").args(["-c", script]), 1024).unwrap();
            let pid = process.worker.as_ref().unwrap().child.id();
            for _ in 0..3 {
                assert_eq!(
                    process.exchange(b"{}".to_vec(), evidence()).unwrap(),
                    b"ok\n"
                );
                assert_eq!(process.worker.as_ref().unwrap().child.id(), pid);
            }
            assert_eq!(process.finish().is_ok(), success);
            drop(process);
            assert!(!crate::supervisor::process_group_alive(pid));
        }
    }

    #[test]
    fn malformed_responses_retain_bounded_evidence() {
        for (script, expected) in [
            ("read r; printf partial", "partial"),
            ("read r; printf 'oversized response\\n'", "oversized r"),
        ] {
            let mut process = Process::spawn(Command::new("sh").args(["-c", script]), 10).unwrap();
            let mut saved = evidence();
            assert!(
                process
                    .exchange(b"{}".to_vec(), saved.try_clone().unwrap())
                    .is_err()
            );
            use std::io::{Seek, SeekFrom};
            saved.seek(SeekFrom::Start(0)).unwrap();
            let mut text = String::new();
            saved.read_to_string(&mut text).unwrap();
            assert_eq!(text, expected);
        }
    }

    #[test]
    fn cancellation_and_deadline_kill_blocked_pipe_and_descendants() {
        for cancel_delay in [None, Some(Duration::ZERO), Some(Duration::from_millis(200))] {
            let directory = tempfile::tempdir().unwrap();
            let ready = directory.path().join("ready");
            let mut process = Process::spawn(
                Command::new("sh")
                    .args(["-c", "dd bs=1 count=1 of=/dev/null 2>/dev/null; touch \"$RUSTDX_TEST_READY\"; sleep 30 & wait"])
                    .env("RUSTDX_TEST_READY", &ready),
                1024,
            )
            .unwrap();
            let pid = process.worker.as_ref().unwrap().child.id();
            let bytes = vec![b'x'; 1024 * 1024];
            let saved = evidence();
            // Cancellation has its own budget; process startup is outside it.
            let budget = if cancel_delay.is_some() {
                Duration::from_secs(10)
            } else {
                Duration::from_millis(100)
            };
            let task = OperationContext::new(budget, None).unwrap();
            let _guard = task.enter();
            let context = task.clone();
            let canceller = cancel_delay.map(|delay| {
                thread::spawn(move || {
                    let waiting = Instant::now();
                    // Reading one byte proves the large pipe write has begun.
                    while !ready.exists() {
                        assert!(waiting.elapsed() < Duration::from_secs(5));
                        thread::sleep(POLL);
                    }
                    // The delayed case exceeds the old 100ms cancellation deadline.
                    thread::sleep(delay);
                    context.cancel();
                })
            });
            let start = Instant::now();
            let error = process.exchange(bytes, saved).unwrap_err();
            drop(process);
            if let Some(canceller) = canceller {
                canceller.join().unwrap();
            }
            assert_eq!(
                error.category(),
                if cancel_delay.is_some() {
                    "cancelled"
                } else {
                    "timeout"
                }
            );
            assert!(start.elapsed() < Duration::from_secs(5));
            assert!(!crate::supervisor::process_group_alive(pid));
        }
    }
}
