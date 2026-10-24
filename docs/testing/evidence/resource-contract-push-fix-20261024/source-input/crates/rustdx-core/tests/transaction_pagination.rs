//! Offline wire-level regression for raw 0fc5/0fc6 full enumeration.
use rustdx_complete::{
    operation::OperationContext,
    tcp::{
        PACK1, TcpConfig,
        stock::{
            Client, TransactionCollection, TransactionPaginationError,
            TransactionPaginationFailure as Failure, TransactionPaginationOptions as Options,
            TransactionPaginationTermination,
        },
    },
};
use std::{
    io::{self, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

fn options() -> Options {
    Options {
        page_size: 3,
        page_delay: Duration::ZERO,
        ..Default::default()
    }
}

// Independently formed fixed-price records. Price starts at 1000 cents in
// each page, then zero deltas; volumes/statuses fit one varint byte.
fn page(history: bool, ticks: &[(u16, u8, u8)]) -> Vec<u8> {
    let mut bytes = (ticks.len() as u16).to_le_bytes().to_vec();
    if history {
        bytes.extend_from_slice(&[0; 4]);
    }
    for (index, &(minutes, volume, status)) in ticks.iter().enumerate() {
        bytes.extend_from_slice(&minutes.to_le_bytes());
        bytes.extend_from_slice(if index == 0 { &[0xa8, 0x0f] } else { &[0] });
        bytes.push(volume);
        bytes.push(1); // Repeated num is legal, not a unique trade ID.
        bytes.extend_from_slice(&[status, 0]);
    }
    bytes
}

fn position(request: &[u8], history: bool) -> (u16, u16) {
    let (command, symbol, offset) = if history {
        (0x0fc6u16, 16, 24)
    } else {
        (0x0fc5u16, 12, 20)
    };
    assert_eq!(&request[10..12], &command.to_le_bytes());
    assert_eq!(&request[symbol..symbol + 2], &[0, 0]);
    assert_eq!(&request[symbol + 2..symbol + 8], b"000001");
    if history {
        assert_eq!(&request[12..16], &20261022u32.to_le_bytes());
    }
    (
        u16::from_le_bytes(request[offset..offset + 2].try_into().unwrap()),
        u16::from_le_bytes(request[offset + 2..offset + 4].try_into().unwrap()),
    )
}

fn mock(
    connections: usize,
    mut response: impl FnMut(&[u8]) -> Option<Vec<u8>> + Send + 'static,
) -> (TcpConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            loop {
                let mut header = [0; 10];
                if stream.read_exact(&mut header).is_err() {
                    break;
                }
                let mut request = header.to_vec();
                request.resize(
                    10 + usize::from(u16::from_le_bytes([header[6], header[7]])),
                    0,
                );
                if stream.read_exact(&mut request[10..]).is_err() {
                    break;
                }
                let body = if request == PACK1 {
                    vec![0; 2]
                } else if let Some(body) = response(&request) {
                    body
                } else {
                    break; // Drop connection to exercise actual retry/reconnect.
                };
                let mut frame = [0; 16];
                frame[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                frame[5..9].copy_from_slice(&request[1..5]);
                frame[10..12].copy_from_slice(&request[10..12]);
                frame[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
                frame[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
                if stream
                    .write_all(&frame)
                    .and_then(|_| stream.write_all(&body))
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    (config, server)
}

fn fetch(
    client: &mut Client,
    history: bool,
    options: Options,
) -> io::Result<TransactionCollection> {
    if history {
        client.history_transaction_all(0, "000001", 20261022, options)
    } else {
        client.transaction_all(0, "000001", options)
    }
}

fn failure(error: &io::Error) -> &TransactionPaginationError {
    error.get_ref().unwrap().downcast_ref().unwrap()
}

#[test]
fn raw_short_pages_continue_and_page_order_preserves_identical_ticks() {
    for history in [false, true] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = calls.clone();
        let (config, server) = mock(1, move |request| {
            let (start, count) = position(request, history);
            observed.lock().unwrap().push((start, count));
            Some(page(
                history,
                match start {
                    0 => &[(915, 1, 5), (916, 2, 5)],
                    2 => &[(570, 3, 8), (570, 3, 8), (570, 4, 8)],
                    5 => &[(565, 5, 0), (570, 3, 8)],
                    7 => &[],
                    _ => panic!("raw offsets must not use filtered length or requested count"),
                },
            ))
        });
        let mut client = Client::with_config(&config).unwrap();
        let all = fetch(&mut client, history, options()).unwrap();
        assert_eq!(
            all.records.iter().map(|row| row.vol).collect::<Vec<_>>(),
            [5, 3, 3, 3, 4, 1, 2]
        );
        assert_eq!(
            all.records.iter().filter(|row| row.buyorsell == 8).count(),
            4
        );
        assert_eq!(
            all.records.iter().filter(|row| row.buyorsell == 5).count(),
            2
        );
        assert!(all.records.iter().all(|row| row.num == 1));
        assert_eq!(all.event_pages.len(), all.pages.len());
        assert_eq!(
            all.event_pages
                .iter()
                .map(|page| page.raw_count())
                .sum::<usize>(),
            all.records.len()
        );
        assert_eq!(all.records[1], all.records[2]); // Across the page boundary.
        assert_eq!(all.records[2], all.records[3]); // Inside a page.
        assert_eq!(*calls.lock().unwrap(), [(0, 3), (2, 3), (5, 3), (7, 3)]);
        assert_eq!(
            all.pages
                .iter()
                .map(|p| p.record_range.clone())
                .collect::<Vec<_>>(),
            [5..7, 2..5, 0..2, 0..0]
        );
        assert_eq!(all.source_endpoint, config.ip.unwrap());
        assert_eq!(all.requested_date, history.then_some(20261022));
        assert_eq!(all.command, if history { 0x0fc6 } else { 0x0fc5 });
        assert_eq!(all.requests, 4);
        assert_eq!(
            all.termination,
            TransactionPaginationTermination::EmptyPageObserved
        );
        assert_eq!(
            all.payload_bytes,
            all.pages.iter().map(|p| p.payload_bytes).sum::<usize>()
        );
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn empty_pages_are_rechecked_at_the_same_offset_without_reconnecting() {
    for history in [false, true] {
        let mut calls = 0;
        let (mut config, server) = mock(1, move |request| {
            let (start, count) = position(request, history);
            assert_eq!(count, 3);
            calls += 1;
            assert_eq!(start, if calls <= 2 { 0 } else { 2 });
            Some(page(
                history,
                if calls == 2 {
                    &[(570, 1, 0), (571, 2, 1)]
                } else {
                    &[]
                },
            ))
        });
        config.recheck_empty = true;
        let mut client = Client::with_config(&config).unwrap();
        let all = fetch(&mut client, history, options()).unwrap();
        assert_eq!(all.records.len(), 2);
        assert_eq!(all.requests, 4);
        assert_eq!(
            all.pages.iter().map(|p| p.requests).collect::<Vec<_>>(),
            [2, 2]
        );
        assert_eq!(all.connection_generation, 0);
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn record_budget_requires_an_empty_probe_even_at_the_exact_limit() {
    for history in [false, true] {
        for extra in [false, true] {
            let (config, server) = mock(1, move |request| {
                let (start, count) = position(request, history);
                Some(page(
                    history,
                    match (start, count) {
                        (0, 2) => &[(570, 1, 0), (571, 2, 1)],
                        (2, 1) if !extra => &[],
                        (2, 1) => &[(569, 3, 0)],
                        _ => panic!(
                            "record budget should narrow the request and allow one end probe"
                        ),
                    },
                ))
            });
            let mut client = Client::with_config(&config).unwrap();
            let result = fetch(
                &mut client,
                history,
                Options {
                    max_records: 2,
                    ..options()
                },
            );
            if extra {
                let error = result.unwrap_err();
                assert_eq!(failure(&error).reason, Failure::RecordLimit);
                assert_eq!(failure(&error).records, 2);
            } else {
                assert_eq!(result.unwrap().records.len(), 2);
            }
            drop(client);
            server.join().unwrap();
        }
    }
}

#[test]
fn page_limit_is_an_error_but_an_empty_first_page_completes_in_one_slot() {
    for empty in [false, true] {
        let (config, server) = mock(1, move |request| {
            assert_eq!(position(request, false), (0, 3));
            Some(page(false, if empty { &[] } else { &[(570, 1, 0)] }))
        });
        let mut client = Client::with_config(&config).unwrap();
        let result = fetch(
            &mut client,
            false,
            Options {
                max_pages: 1,
                ..options()
            },
        );
        if empty {
            let all = result.unwrap();
            assert!(all.records.is_empty());
            assert_eq!(all.pages.len(), 1);
        } else {
            let error = result.unwrap_err();
            let stop = failure(&error);
            assert_eq!(
                (stop.reason, stop.start, stop.pages, stop.records),
                (Failure::PageLimit, 1, 1, 1)
            );
        }
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn byte_budget_counts_all_payloads_including_discarded_empty_rechecks() {
    for recheck in [false, true] {
        let (mut config, server) = mock(1, move |request| {
            let (start, _) = position(request, false);
            Some(page(
                false,
                if recheck || start > 0 {
                    &[]
                } else {
                    &[(570, 1, 0)]
                },
            ))
        });
        config.recheck_empty = recheck;
        let max_bytes = if recheck {
            3
        } else {
            page(false, &[(570, 1, 0)]).len() + 1
        };
        let mut client = Client::with_config(&config).unwrap();
        let error = fetch(
            &mut client,
            false,
            Options {
                max_bytes,
                ..options()
            },
        )
        .unwrap_err();
        assert_eq!(failure(&error).reason, Failure::ByteLimit);
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn identical_whole_pages_and_reversed_minute_order_are_not_normal_completion() {
    for mode in 0..3 {
        let (config, server) = mock(1, move |request| {
            let (start, _) = position(request, false);
            Some(page(
                false,
                match (mode, start) {
                    (0, _) => &[(570, 1, 0), (570, 2, 1)], // Repeated complete page.
                    (1, _) => &[(571, 1, 0), (570, 2, 1)], // Reversed inside page.
                    (2, 0) => &[(570, 1, 0)],
                    (2, _) => &[(571, 2, 0)], // Later than the newest page.
                    _ => unreachable!(),
                },
            ))
        });
        let mut client = Client::with_config(&config).unwrap();
        let error = fetch(&mut client, false, options()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            failure(&error).reason,
            if mode == 0 {
                Failure::RepeatedPage
            } else {
                Failure::InvalidOrder
            }
        );
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn u16_offset_boundary_allows_65535_but_never_wraps_or_truncates() {
    for tail in [1535, 1536, 2000] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let (config, server) = mock(1, move |request| {
            let (start, count) = position(request, false);
            seen.lock().unwrap().push(start);
            assert_eq!(count, 2000);
            let count = if start == 64000 {
                tail
            } else if start == u16::MAX {
                0
            } else {
                2000
            };
            Some(page(
                false,
                &vec![(570, (start / 2000 + 1) as u8, 0); count],
            ))
        });
        let mut client = Client::with_config(&config).unwrap();
        let result = fetch(
            &mut client,
            false,
            Options {
                page_size: 2000,
                max_records: 100_000,
                ..options()
            },
        );
        if tail == 1535 {
            let all = result.unwrap();
            assert_eq!(all.records.len(), 65535);
            assert_eq!(all.pages.last().unwrap().start, u16::MAX);
        } else {
            let error = result.unwrap_err();
            let stop = failure(&error);
            assert_eq!(stop.reason, Failure::OffsetLimit);
            assert_eq!(stop.start, (64000 + tail) as u32);
        }
        assert!(
            calls
                .lock()
                .unwrap()
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn malformed_later_page_propagates_instead_of_returning_the_first_page() {
    for history in [false, true] {
        let (config, server) = mock(1, move |request| {
            let (start, _) = position(request, history);
            let mut bytes = page(history, &[(570, (start + 1) as u8, 0)]);
            if start > 0 {
                bytes.pop();
            }
            Some(bytes)
        });
        let mut client = Client::with_config(&config).unwrap();
        assert_eq!(
            fetch(&mut client, history, options()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn auto_reconnect_to_the_same_endpoint_cannot_mix_pages() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = seen.clone();
    let mut failed = false;
    let (mut config, server) = mock(2, move |request| {
        let (start, _) = position(request, false);
        calls.lock().unwrap().push(start);
        if start == 1 && !failed {
            failed = true;
            None
        } else {
            Some(page(false, if start == 0 { &[(570, 1, 0)] } else { &[] }))
        }
    });
    config.auto_reconnect = 1;
    let mut client = Client::with_config(&config).unwrap();
    let error = fetch(&mut client, false, options()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
    assert_eq!(failure(&error).reason, Failure::ConnectionChanged);
    assert_eq!(*seen.lock().unwrap(), [0, 1, 1]);
    drop(client);
    server.join().unwrap();
}

#[test]
fn all_pages_share_one_deadline_and_honor_outer_cancellation() {
    for cancel in [false, true] {
        let context = OperationContext::new(Duration::from_secs(2), None).unwrap();
        let signal = context.clone();
        let (config, server) = mock(1, move |request| {
            let (start, _) = position(request, false);
            if cancel && start == 1 {
                signal.cancel();
            }
            if !cancel {
                std::thread::sleep(Duration::from_millis(40));
            }
            Some(page(false, &[(570, (start + 1) as u8, 0)]))
        });
        let mut client = Client::with_config(&config).unwrap();
        let guard = context.enter();
        let started = Instant::now();
        let error = fetch(
            &mut client,
            false,
            Options {
                timeout: Duration::from_millis(100),
                ..options()
            },
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(500));
        if cancel {
            assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        } else {
            assert!(matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ));
        }
        drop(guard);
        drop(client);
        server.join().unwrap();
    }
}

#[test]
fn pacing_cannot_extend_an_outer_operation_deadline() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let (config, server) = mock(1, move |request| {
        let start = position(request, false).0;
        observed.lock().unwrap().push(start);
        assert_eq!(start, 0);
        Some(page(false, &[(570, 1, 0)]))
    });
    let mut client = Client::with_config(&config).unwrap();
    // Allow the first wire exchange, including delayed ACK, to finish before
    // pacing consumes the deadline. A 40ms budget also tested socket timing.
    let context = OperationContext::new(Duration::from_millis(250), None).unwrap();
    let guard = context.enter();
    let started = Instant::now();
    let error = fetch(
        &mut client,
        false,
        Options {
            timeout: Duration::from_secs(2),
            page_delay: Duration::from_secs(1),
            ..options()
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_millis(750));
    assert_eq!(*calls.lock().unwrap(), [0]);
    drop(guard);
    drop(client);
    server.join().unwrap();
}

#[test]
fn invalid_parameters_do_not_send_any_page() {
    let (config, server) = mock(1, |request| {
        assert_eq!(position(request, true), (0, 3));
        Some(page(true, &[]))
    });
    let mut client = Client::with_config(&config).unwrap();
    for invalid in [
        Options {
            page_size: 0,
            ..options()
        },
        Options {
            page_size: 2001,
            ..options()
        },
        Options {
            max_pages: 0,
            ..options()
        },
        Options {
            max_records: 0,
            ..options()
        },
        Options {
            max_bytes: 0,
            ..options()
        },
        Options {
            timeout: Duration::ZERO,
            ..options()
        },
    ] {
        assert_eq!(
            fetch(&mut client, false, invalid).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(client.transaction_all(2, "000001", options()).is_err());
    assert!(client.transaction_all(0, "0000/1", options()).is_err());
    assert!(
        client
            .history_transaction_all(0, "000001", 20260230, options())
            .is_err()
    );
    assert!(
        fetch(&mut client, true, options())
            .unwrap()
            .records
            .is_empty()
    );
    drop(client);
    server.join().unwrap();
}
