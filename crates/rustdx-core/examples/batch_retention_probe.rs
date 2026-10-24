//! Synthetic local protocol workload for external process-tree RSS sampling.
use rustdx_complete::tcp::{
    TcpConfig,
    stock::{BatchResultBudget, Client, IntradayAuxKind},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

#[path = "../tests/common/ip_pool.rs"]
mod ip_pool_test;

fn main() {
    let count: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    assert!((1..=10000).contains(&count));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        std::thread::scope(|scope| {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                scope.spawn(move || {
                    socket
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    loop {
                        let mut header = [0; 10];
                        if socket.read_exact(&mut header).is_err() {
                            break;
                        }
                        let len = usize::from(u16::from_le_bytes([header[6], header[7]]));
                        let mut request = header.to_vec();
                        request.resize(10 + len, 0);
                        if socket.read_exact(&mut request[10..]).is_err() {
                            break;
                        }
                        let body = if request[10..12] == [0x1b, 0x05] {
                            // Keep each allocation phase observable at 100ms sampling.
                            std::thread::sleep(Duration::from_millis(40));
                            let mut body = 4096u16.to_le_bytes().to_vec();
                            body.resize(8194, 1);
                            body
                        } else {
                            vec![0, 0]
                        };
                        let mut reply = [0; 16];
                        reply[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                        reply[5..9].copy_from_slice(&request[1..5]);
                        reply[10..12].copy_from_slice(&request[10..12]);
                        reply[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
                        reply[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
                        if socket
                            .write_all(&reply)
                            .and_then(|_| socket.write_all(&body))
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }
        });
    });
    let mut client = Client::with_config(&config).unwrap();
    client
        .set_batch_result_budget(BatchResultBudget {
            max_retained_bytes: 8 * 1024 * 1024,
            ..Default::default()
        })
        .unwrap();
    let codes: Vec<_> = (0..count).map(|i| format!("{i:06}")).collect();
    let inputs: Vec<_> = codes.iter().map(|code| (0, code.as_str())).collect();
    let result = client.intraday_aux_batch(&inputs, IntradayAuxKind::BuySellStrength, 2);
    let outcome = match result {
        Ok(values) => {
            assert!(values.iter().all(Result::is_ok));
            assert_eq!(values.len(), count);
            std::thread::sleep(Duration::from_millis(300));
            "complete"
        }
        Err(error) => {
            assert!(error.to_string().contains("预算耗尽"), "{error}");
            "budget_exhausted"
        }
    };
    assert_eq!(
        outcome,
        if count == 16 {
            "complete"
        } else {
            "budget_exhausted"
        }
    );
    println!(
        "{}",
        serde_json::json!({
            "synthetic": true, "inputs": count, "rows_per_item":4096,
            "max_retained_bytes":8388608, "workers":2, "outcome":outcome
        })
    );
    drop(client);
    server.join().unwrap();
}
