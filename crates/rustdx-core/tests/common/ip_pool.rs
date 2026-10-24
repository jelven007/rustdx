use rustdx_complete::tcp::{PACK1, SecurityCount, Tdx, ip::IpPool};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

/// 用同一个监听地址完成真实三轮预验证，随后交给原有业务故障测试。
pub fn prepare(listener: &TcpListener) -> IpPool {
    let pool = IpPool::new(vec![listener.local_addr().unwrap()]).unwrap();
    let listener = listener.try_clone().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..3 {
            let started = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(started.elapsed() < Duration::from_secs(5));
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            for request in [PACK1, SecurityCount::SEND] {
                let mut bytes = vec![0; request.len()];
                stream.read_exact(&mut bytes).unwrap();
                assert_eq!(bytes, request);
                let mut reply = [0; 18];
                reply[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                reply[5..9].copy_from_slice(&request[1..5]);
                reply[10..12].copy_from_slice(&request[10..12]);
                reply[12..14].copy_from_slice(&2u16.to_le_bytes());
                reply[14..16].copy_from_slice(&2u16.to_le_bytes());
                reply[16..18].copy_from_slice(&123u16.to_le_bytes());
                stream.write_all(&reply).unwrap();
            }
        }
    });
    let daily = pool.refresh().unwrap();
    assert_eq!(daily.valid_nodes, pool.list());
    server.join().unwrap();
    pool
}
