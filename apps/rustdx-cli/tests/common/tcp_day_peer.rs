//! Synthetic loopback protocol peer; not exchange data or unit evidence.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

pub struct Peer {
    pub endpoint: SocketAddr,
    pub close: Arc<AtomicUsize>,
    pub missing: Arc<AtomicBool>,
    pub offsets: Arc<Mutex<Vec<u16>>>,
    pub active: Arc<AtomicUsize>,
    pub peak: Arc<AtomicUsize>,
    pub delay_ms: Arc<AtomicUsize>,
    pub failed_code: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn varint(value: i32, out: &mut Vec<u8>) {
    let mut value_abs = value.unsigned_abs();
    let mut first = (value_abs & 63) as u8 | if value < 0 { 64 } else { 0 };
    value_abs >>= 6;
    if value_abs != 0 {
        first |= 128;
    }
    out.push(first);
    while value_abs != 0 {
        let mut byte = (value_abs & 127) as u8;
        value_abs >>= 7;
        if value_abs != 0 {
            byte |= 128;
        }
        out.push(byte);
    }
}

fn payload(dates: &[u32], close: i32, command: u16) -> Vec<u8> {
    let mut bytes = (dates.len() as u16).to_le_bytes().to_vec();
    let mut base = 0;
    for date in dates {
        bytes.extend(date.to_le_bytes());
        for value in [10_000 - base, close - 10_000, 2000, -1000] {
            varint(value, &mut bytes);
        }
        base = close;
        if command == 0x052b {
            // Non-lot-multiple value catches a lossy lots -> shares path.
            varint(137_983_701, &mut bytes);
        } else {
            bytes.extend(1_235_775_464_u32.to_le_bytes());
        }
        bytes.extend(12345.5_f32.to_le_bytes());
        if command == 0x052b {
            bytes.extend([0; 4]);
        }
    }
    bytes
}

impl Peer {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let close = Arc::new(AtomicUsize::new(10_000));
        let missing = Arc::new(AtomicBool::new(false));
        let offsets = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let delay_ms = Arc::new(AtomicUsize::new(0));
        let failed_code = Arc::new(Mutex::new(None::<String>));
        let state = (
            stop.clone(),
            close.clone(),
            missing.clone(),
            offsets.clone(),
            active.clone(),
            peak.clone(),
            delay_ms.clone(),
            failed_code.clone(),
        );
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !state.0.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                let state = state.clone();
                workers.push(std::thread::spawn(move || {
                    let mut business = false;
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    loop {
                        let mut prefix = [0; 12];
                        if stream.read_exact(&mut prefix).is_err() {
                            break;
                        }
                        let command = u16::from_le_bytes(prefix[10..12].try_into().unwrap());
                        let length = match command {
                            0x052d => 38,
                            0x052b => 24,
                            0x044e => 18,
                            _ => 13,
                        };
                        let mut request = prefix.to_vec();
                        request.resize(length, 0);
                        if stream.read_exact(&mut request[12..]).is_err() {
                            break;
                        }
                        let body = if matches!(command, 0x052d | 0x052b) {
                            if !business {
                                business = true;
                                let active = state.4.fetch_add(1, Ordering::SeqCst) + 1;
                                state.5.fetch_max(active, Ordering::SeqCst);
                            }
                            std::thread::sleep(Duration::from_millis(
                                state.6.load(Ordering::SeqCst) as u64,
                            ));
                            if command == 0x052b
                                && state.7.lock().unwrap().as_ref().is_some_and(|code| {
                                    request.get(14..20) == Some(code.as_bytes())
                                })
                            {
                                break;
                            }
                            let offset = if command == 0x052b { 20 } else { 24 };
                            let start =
                                u16::from_le_bytes(request[offset..offset + 2].try_into().unwrap());
                            state.3.lock().unwrap().push(start);
                            let dates = match start {
                                0 => vec![20260707],
                                1 if !state.2.load(Ordering::SeqCst) => vec![20260706, 20260707],
                                _ => vec![],
                            };
                            payload(&dates, state.1.load(Ordering::SeqCst) as i32, command)
                        } else {
                            vec![0, 0]
                        };
                        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&body, 6);
                        let mut header = [0; 16];
                        header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                        header[5..9].copy_from_slice(&request[1..5]);
                        header[10..12].copy_from_slice(&request[10..12]);
                        header[12..14].copy_from_slice(&(compressed.len() as u16).to_le_bytes());
                        header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
                        if stream
                            .write_all(&header)
                            .and_then(|_| stream.write_all(&compressed))
                            .is_err()
                        {
                            break;
                        }
                    }
                    if business {
                        state.4.fetch_sub(1, Ordering::SeqCst);
                    }
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            endpoint,
            close,
            missing,
            offsets,
            active,
            peak,
            delay_ms,
            failed_code,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
