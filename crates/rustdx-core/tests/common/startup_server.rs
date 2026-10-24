//! Local protocol peer shared by library and binary startup tests.
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub enum Mode {
    Good,
    Flaky,
    Silent,
    BadHeartbeat,
}

#[derive(Default)]
pub struct Counts {
    pub connections: AtomicUsize,
    pub handshake: AtomicUsize,
    pub legacy: AtomicUsize,
    pub heartbeat: AtomicUsize,
}

pub struct Server {
    pub address: SocketAddr,
    pub counts: Arc<Counts>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let counts = Arc::new(Counts::default());
        let worker_stop = stop.clone();
        let worker_counts = counts.clone();
        let worker = thread::spawn(move || {
            thread::scope(|scope| {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut socket, _)) => {
                            socket.set_nonblocking(false).unwrap();
                            socket
                                .set_read_timeout(Some(Duration::from_millis(200)))
                                .unwrap();
                            socket
                                .set_write_timeout(Some(Duration::from_secs(1)))
                                .unwrap();
                            let number =
                                worker_counts.connections.fetch_add(1, Ordering::SeqCst) + 1;
                            let stop = worker_stop.clone();
                            let counts = worker_counts.clone();
                            scope.spawn(move || {
                                while !stop.load(Ordering::Acquire) {
                                    let mut header = [0; 10];
                                    if socket.read_exact(&mut header).is_err() {
                                        break;
                                    }
                                    let length =
                                        u16::from_le_bytes([header[6], header[7]]) as usize;
                                    let mut request = vec![0; length];
                                    if socket.read_exact(&mut request).is_err() {
                                        break;
                                    }
                                    let body = match request.first().copied() {
                                        Some(0x0d) => {
                                            counts.handshake.fetch_add(1, Ordering::SeqCst);
                                            vec![0; 2]
                                        }
                                        Some(0x4e) => {
                                            counts.legacy.fetch_add(1, Ordering::SeqCst);
                                            if matches!(mode, Mode::Flaky) && number == 2 {
                                                vec![1]
                                            } else {
                                                123u16.to_le_bytes().to_vec()
                                            }
                                        }
                                        Some(4) => {
                                            counts.heartbeat.fetch_add(1, Ordering::SeqCst);
                                            if matches!(mode, Mode::BadHeartbeat) {
                                                vec![0]
                                            } else {
                                                let mut body = vec![0; 6];
                                                body.extend(20261024u32.to_le_bytes());
                                                body
                                            }
                                        }
                                        other => panic!("unexpected command: {other:?}"),
                                    };
                                    if matches!(mode, Mode::Silent) {
                                        while !stop.load(Ordering::Acquire) {
                                            thread::sleep(Duration::from_millis(5));
                                        }
                                        break;
                                    }
                                    let mut response = [0; 16];
                                    response[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                                    response[5..9].copy_from_slice(&header[1..5]);
                                    response[10..12].copy_from_slice(&request[..2]);
                                    response[12..14]
                                        .copy_from_slice(&(body.len() as u16).to_le_bytes());
                                    response[14..16]
                                        .copy_from_slice(&(body.len() as u16).to_le_bytes());
                                    if socket
                                        .write_all(&response)
                                        .and_then(|_| socket.write_all(&body))
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            });
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("{error}"),
                    }
                }
            });
        });
        Self {
            address,
            counts,
            stop,
            worker: Some(worker),
        }
    }

    pub fn wait_for_handshake(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.counts.handshake.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline, "startup did not send PACK1");
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
