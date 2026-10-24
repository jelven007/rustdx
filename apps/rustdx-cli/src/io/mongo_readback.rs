//! One mongosh connection per import; a single bounded pipe worker avoids
//! blocking the cancellation/deadline checks on child stdin/stdout.
use super::verify::{Key, Row};
use eyre::{Result, eyre};
use rustdx_complete::operation;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(20);

enum Request {
    Batch(Vec<u8>),
    Finish,
}

struct ReadbackProcess {
    child: Child,
    requests: Option<SyncSender<Request>>,
    replies: Option<Receiver<io::Result<String>>>,
    worker: Option<JoinHandle<()>>,
    _permit: rustdx_complete::tcp::NetworkPermit,
}

impl ReadbackProcess {
    fn spawn(command: &mut Command, limit: usize) -> Result<Self> {
        let permit = rustdx_complete::tcp::acquire_network(operation::timeout(TIMEOUT)?)?;
        operation::check()?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("mongo-readback".into())
            .spawn(move || {
                let mut stdin = Some(stdin);
                let mut reader = BufReader::new(stdout);
                for request in incoming {
                    let finish = matches!(request, Request::Finish);
                    let result = (|| -> io::Result<String> {
                        match request {
                            Request::Batch(bytes) => {
                                let input = stdin.as_mut().expect("batch before finish");
                                input.write_all(&bytes)?;
                                input.write_all(b"\n")?;
                                input.flush()?;
                                let mut bytes = Vec::new();
                                reader
                                    .by_ref()
                                    .take(limit as u64 + 1)
                                    .read_until(b'\n', &mut bytes)?;
                                if bytes.len() > limit {
                                    return Err(io::Error::other("MongoDB回读响应超过字节限制"));
                                }
                                if bytes.last() != Some(&b'\n') {
                                    return Err(io::Error::new(
                                        io::ErrorKind::UnexpectedEof,
                                        "MongoDB回读响应不完整",
                                    ));
                                }
                                String::from_utf8(bytes).map_err(io::Error::other)
                            }
                            Request::Finish => {
                                drop(stdin.take());
                                let mut extra = [0];
                                if reader.read(&mut extra)? != 0 {
                                    return Err(io::Error::other("MongoDB回读包含多余响应"));
                                }
                                Ok(String::new())
                            }
                        }
                    })();
                    let failed = result.is_err();
                    if outgoing.send(result).is_err() || finish || failed {
                        break;
                    }
                }
            });
        match worker {
            Ok(worker) => Ok(Self {
                child,
                requests: Some(requests),
                replies: Some(replies),
                worker: Some(worker),
                _permit: permit,
            }),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(error.into())
            }
        }
    }

    fn exchange(&mut self, request: Request, start: Instant, timeout: Duration) -> Result<String> {
        operation::check()?;
        // At most one outstanding request. try_send also makes accidental
        // protocol misuse fail rather than block the cancellation loop.
        self.requests
            .as_ref()
            .ok_or_else(|| eyre!("MongoDB回读已关闭"))?
            .try_send(request)
            .map_err(|_| eyre!("MongoDB回读请求未被接收"))?;
        loop {
            operation::check()?;
            if start.elapsed() >= timeout {
                eyre::bail!("MongoDB回读超时；CSV保留待核实");
            }
            match self
                .replies
                .as_ref()
                .ok_or_else(|| eyre!("MongoDB回读已关闭"))?
                .recv_timeout(POLL.min(timeout.saturating_sub(start.elapsed())))
            {
                Ok(result) => return Ok(result?),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn finish(&mut self) -> Result<()> {
        let start = Instant::now();
        let timeout = operation::timeout(TIMEOUT)?;
        self.exchange(Request::Finish, start, timeout)?;
        loop {
            operation::check()?;
            match self.child.try_wait()? {
                Some(status) if status.success() => return Ok(()),
                Some(status) => eyre::bail!("MongoDB回读进程异常退出: {status}；CSV保留待核实"),
                None if start.elapsed() < timeout => thread::sleep(POLL),
                None => eyre::bail!("MongoDB回读进程未按时退出；CSV保留待核实"),
            }
        }
    }
}

impl Drop for ReadbackProcess {
    fn drop(&mut self) {
        // Drop both channels first so neither a pending receive nor a full
        // response channel can prevent the worker from joining after kill.
        self.requests.take();
        self.replies.take();
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) struct MongoReadback<'a> {
    process: ReadbackProcess,
    database: &'a str,
    collection: &'a str,
    sequence: u64,
}

impl<'a> MongoReadback<'a> {
    pub(super) fn new(uri: &str, database: &'a str, collection: &'a str) -> Result<Self> {
        let mut command = Command::new("mongosh");
        // Encourage V8 to collect between bounded batches instead of growing
        // its default multi-gigabyte heap. Honor an explicit deployment setting.
        // This caps the old generation, not total RSS or native allocations.
        if std::env::var_os("NODE_OPTIONS").is_none() {
            command.env("NODE_OPTIONS", "--max-old-space-size=128");
        }
        command.env("RUSTDX_MONGODB_URI", uri).args([
            "--nodb",
            "--quiet",
            "--norc",
            "--eval",
            include_str!("mongo_readback.js"),
        ]);
        Ok(Self {
            process: ReadbackProcess::spawn(&mut command, RESPONSE_BYTES)?,
            database,
            collection,
            sequence: 0,
        })
    }

    pub(super) fn fetch(&mut self, fields: &[String], keys: &[Key]) -> Result<Vec<Row>> {
        self.sequence += 1;
        let request = serde_json::to_vec(&serde_json::json!({
            "id": self.sequence,
            "database": self.database,
            "collection": self.collection,
            "fields": fields,
            "keys": keys,
        }))?;
        if request.len() > RESPONSE_BYTES {
            eyre::bail!("MongoDB回读请求超过字节限制");
        }
        let text = self.process.exchange(
            Request::Batch(request),
            Instant::now(),
            operation::timeout(TIMEOUT)?,
        )?;
        super::verify::json_rows(fields, &text, keys.len(), self.sequence)
    }

    pub(super) fn finish(mut self) -> Result<()> {
        self.process.finish()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn request(process: &mut ReadbackProcess) -> Result<String> {
        process.exchange(
            Request::Batch(b"{}".to_vec()),
            Instant::now(),
            Duration::from_secs(2),
        )
    }

    #[test]
    fn persistent_process_checks_complete_response_eof_and_exit_status() {
        for (script, success) in [
            ("while read r; do printf 'ok\\n'; done", true),
            ("while read r; do printf 'ok\\n'; done; exit 7", false),
        ] {
            let mut process =
                ReadbackProcess::spawn(Command::new("sh").args(["-c", script]), 1024).unwrap();
            for _ in 0..3 {
                assert_eq!(request(&mut process).unwrap(), "ok\n");
            }
            assert_eq!(process.finish().is_ok(), success);
            assert!(process.child.try_wait().unwrap().is_some());
        }
        for script in [
            "read r; printf partial",
            "read r; printf 'too much response\\n'",
            "exit 9",
        ] {
            let mut process =
                ReadbackProcess::spawn(Command::new("sh").args(["-c", script]), 10).unwrap();
            assert!(request(&mut process).is_err());
        }
        let mut process = ReadbackProcess::spawn(
            Command::new("sh").args(["-c", "read r; printf 'ok\\nextra\\n'"]),
            1024,
        )
        .unwrap();
        assert_eq!(request(&mut process).unwrap(), "ok\n");
        assert!(process.finish().is_err());
    }

    #[test]
    fn deadline_cancellation_and_blocked_pipe_are_reaped() {
        for cancel in [false, true] {
            let task = operation::OperationContext::new(Duration::from_millis(80), None).unwrap();
            let _guard = task.enter();
            let mut process = ReadbackProcess::spawn(Command::new("sleep").arg("5"), 1024).unwrap();
            let context = task.clone();
            let canceller = thread::spawn(move || {
                if cancel {
                    thread::sleep(Duration::from_millis(20));
                    context.cancel();
                }
            });
            let start = Instant::now();
            // More than the pipe buffer: timeout must also interrupt write_all.
            assert!(
                process
                    .exchange(
                        Request::Batch(vec![b'x'; 1024 * 1024]),
                        start,
                        Duration::from_secs(5)
                    )
                    .is_err()
            );
            drop(process);
            canceller.join().unwrap();
            assert!(start.elapsed() < Duration::from_secs(1));
        }
    }
}
