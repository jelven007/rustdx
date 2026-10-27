use rustdx_complete::{
    operation::OperationContext,
    tcp::{PACK1, Tcp, TcpConfig, network_stats, stock::*},
};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};
#[path = "common/extended_protocol.rs"]
mod common;
#[path = "common/ip_pool.rs"]
mod ip_pool;
use common::*;
static SERIAL: Mutex<()> = Mutex::new(());

fn request(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 10];
    stream.read_exact(&mut header).unwrap();
    let n = u16::from_le_bytes([header[6], header[7]]) as usize;
    let mut bytes = header.to_vec();
    bytes.resize(10 + n, 0);
    stream.read_exact(&mut bytes[10..]).unwrap();
    bytes
}
fn setup(run: impl FnOnce(TcpStream) + Send + 'static) -> (TcpConfig, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pool = ip_pool::prepare(&listener);
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(pool),
        timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let hello = request(&mut stream);
        assert_eq!(hello, PACK1);
        stream.write_all(&response(&hello, &[0, 0])).unwrap();
        run(stream);
    });
    (config, server)
}
fn subscribe(config: &TcpConfig, options: SubscriptionOptions) -> QuoteSubscription {
    QuoteSubscription::with_config(config, &[(0, "000001")], Arc::new(metadata()), options).unwrap()
}
#[test]
fn partial_push_received_before_refresh_cannot_satisfy_the_new_request() {
    let _guard = SERIAL.lock().unwrap();
    let (sent_tx, sent_rx) = mpsc::channel();
    let (config, server) = setup(move |mut stream| {
        let baseline = request(&mut stream);
        stream
            .write_all(&response(&baseline, &refresh_payload(10)))
            .unwrap();
        // An unsolicited message id can coincide with the next request id.
        // Its first bytes were received before that request was sent.
        let mut push = response(&baseline, &refresh_payload(11));
        push[5..9].copy_from_slice(&2u32.to_le_bytes());
        stream.write_all(&push[..7]).unwrap();
        sent_tx.send(()).unwrap();
        let refresh = request(&mut stream);
        assert_eq!(&refresh[1..5], &2u32.to_le_bytes());
        stream.write_all(&push[7..]).unwrap();
        stream
            .write_all(&response(&refresh, &refresh_payload(12)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let tcp = Tcp::with_config(&config).unwrap();
    let arrival = tcp.get_ref().0.try_clone().unwrap();
    let mut sub = QuoteSubscription::from_tcp(
        tcp,
        RefreshRequest::from_stocks(&[(0, "000001")]).unwrap(),
        Arc::new(metadata()),
        SubscriptionOptions::default(),
    )
    .unwrap();
    sub.refresh().unwrap();
    sent_rx.recv().unwrap();
    // A server write can still be buffered by TCP. Establish arrival before
    // the short poll so this test actually stages a pre-request frame.
    arrival
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    assert!(arrival.peek(&mut [0; 7]).unwrap() > 0);
    assert!(sub.poll_push(Duration::from_millis(10)).unwrap().is_none());
    let refresh = sub.refresh().unwrap();
    let push = sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    let state = sub.synchronized_snapshot().unwrap();
    sub.close();
    server.join().unwrap();
    assert_eq!(refresh.page.records[0].update_cursor, Some(12));
    assert_eq!(push.refresh.page.records[0].update_cursor, Some(11));
    assert_eq!(push.message_id, 2);
    assert_eq!(state.states[0].cursor, 12);
}

#[test]
fn idle_update_refreshes_a_node_without_passive_push() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let baseline = request(&mut stream);
        stream
            .write_all(&response(&baseline, &refresh_payload(10)))
            .unwrap();
        let refresh = request(&mut stream);
        assert_eq!(
            u16::from_le_bytes(refresh[10..12].try_into().unwrap()),
            0x0547
        );
        assert_eq!(u32::from_le_bytes(refresh[21..25].try_into().unwrap()), 10);
        stream
            .write_all(&response(&refresh, &refresh_payload(11)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    let connection = sub.synchronized_snapshot().unwrap().connection_id;
    let update = sub.poll_update(Duration::from_millis(10)).unwrap();
    let QuoteUpdate::RequestedRefresh(refresh) = update else {
        panic!("a requested response must not be labeled as passive push");
    };
    assert_eq!(refresh.page.records[0].update_cursor, Some(11));
    let state = sub.synchronized_snapshot().unwrap();
    assert_eq!(state.connection_id, connection);
    assert_eq!(state.generation, 0);
    assert_eq!(state.states[0].cursor, 11);
    sub.close();
    server.join().unwrap();
}

#[test]
fn idle_update_returns_a_queued_push_without_sending_a_refresh() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let baseline = request(&mut stream);
        let mut push = response(&baseline, &refresh_payload(10));
        push[5..9].copy_from_slice(&99u32.to_le_bytes());
        stream.write_all(&push).unwrap();
        stream
            .write_all(&response(&baseline, &refresh_payload(11)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    let update = sub.poll_update(Duration::from_millis(10)).unwrap();
    let QuoteUpdate::PassivePush(push) = update else {
        panic!("queued pushes must preserve their source");
    };
    assert_eq!(push.message_id, 99);
    assert_eq!(sub.synchronized_snapshot().unwrap().states[0].cursor, 11);
    sub.close();
    server.join().unwrap();
}

#[test]
fn update_initial_baseline_and_zero_wait_preserve_the_session_contract() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        stream
            .write_all(&response(&req, &refresh_payload(10)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    assert_eq!(
        sub.poll_update(Duration::ZERO).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::AwaitingBaseline);
    assert!(matches!(
        sub.poll_update(Duration::from_secs(1)).unwrap(),
        QuoteUpdate::RequestedRefresh(_)
    ));
    sub.synchronized_snapshot().unwrap();
    sub.close();
    server.join().unwrap();
}

#[test]
fn update_retains_partial_push_during_requested_refresh_and_propagates_eof() {
    let _guard = SERIAL.lock().unwrap();
    let (sent_tx, sent_rx) = mpsc::channel();
    let (config, server) = setup(move |mut stream| {
        let baseline = request(&mut stream);
        stream
            .write_all(&response(&baseline, &refresh_payload(10)))
            .unwrap();
        let mut push = response(&baseline, &refresh_payload(11));
        push[5..9].copy_from_slice(&99u32.to_le_bytes());
        stream.write_all(&push[..7]).unwrap();
        sent_tx.send(()).unwrap();
        let refresh = request(&mut stream);
        stream.write_all(&push[7..]).unwrap();
        stream
            .write_all(&response(&refresh, &refresh_payload(12)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    sent_rx.recv().unwrap();
    assert!(matches!(
        sub.poll_update(Duration::from_millis(10)).unwrap(),
        QuoteUpdate::RequestedRefresh(_)
    ));
    assert_eq!(sub.synchronized_snapshot().unwrap().states[0].cursor, 12);
    assert!(matches!(
        sub.poll_update(Duration::from_millis(10)).unwrap(),
        QuoteUpdate::PassivePush(_)
    ));
    assert_eq!(sub.synchronized_snapshot().unwrap().states[0].cursor, 12);
    sub.close();
    server.join().unwrap();

    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        stream
            .write_all(&response(&req, &refresh_payload(10)))
            .unwrap();
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    server.join().unwrap();
    assert_eq!(
        sub.poll_update(Duration::from_secs(1)).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
}

#[test]
fn poll_preserves_partial_header_and_body_and_close_releases_permit() {
    let _guard = SERIAL.lock().unwrap();
    let baseline_active = network_stats().active;
    let (continue_tx, continue_rx) = mpsc::channel();
    let (sent_tx, sent_rx) = mpsc::channel();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        stream
            .write_all(&response(&req, &refresh_payload(10)))
            .unwrap();
        let mut push = response(&req, &refresh_payload(11));
        push[5..9].copy_from_slice(&99u32.to_le_bytes());
        stream.write_all(&push[..7]).unwrap();
        sent_tx.send(()).unwrap();
        continue_rx.recv().unwrap();
        stream.write_all(&push[7..22]).unwrap();
        sent_tx.send(()).unwrap();
        continue_rx.recv().unwrap();
        stream.write_all(&push[22..]).unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    assert_eq!(network_stats().active, baseline_active + 1);
    sub.refresh().unwrap();
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Synchronized);
    sent_rx.recv().unwrap();
    assert!(sub.poll_push(Duration::from_millis(10)).unwrap().is_none());
    continue_tx.send(()).unwrap();
    sent_rx.recv().unwrap();
    assert!(sub.poll_push(Duration::from_millis(10)).unwrap().is_none());
    continue_tx.send(()).unwrap();
    let push = sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(push.message_id, 99);
    assert_eq!(sub.snapshot().states[0].cursor, 11);
    sub.close();
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Closed);
    assert_eq!(network_stats().active, baseline_active);
    server.join().unwrap();
}
#[test]
fn pushes_before_response_are_queued_with_two_budgets() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        let mut push = response(&req, &refresh_payload(10));
        push[5..9].copy_from_slice(&99u32.to_le_bytes());
        stream.write_all(&push).unwrap();
        stream
            .write_all(&response(&req, &refresh_payload(11)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    assert_eq!(sub.snapshot().queued_frames, 1);
    assert!(sub.snapshot().queued_bytes > 0);
    assert_eq!(sub.snapshot().states[0].cursor, 11);
    assert_eq!(
        sub.poll_push(Duration::ZERO).unwrap().unwrap().message_id,
        99
    );
    assert_eq!(sub.snapshot().queued_bytes, 0);
    // Draining an older notification does not roll state backward.
    assert_eq!(sub.snapshot().states[0].cursor, 11);
    sub.close();
    server.join().unwrap();
}
#[test]
fn frame_and_byte_overflow_invalidate_and_release_connection() {
    let _guard = SERIAL.lock().unwrap();
    for options in [
        SubscriptionOptions {
            max_queue_frames: 1,
            ..Default::default()
        },
        SubscriptionOptions {
            max_queue_bytes: 1,
            ..Default::default()
        },
    ] {
        let before = network_stats().active;
        let (config, server) = setup(move |mut stream| {
            let req = request(&mut stream);
            for id in [99u32, 100] {
                let mut push = response(&req, &refresh_payload(10));
                push[5..9].copy_from_slice(&id.to_le_bytes());
                if stream.write_all(&push).is_err() {
                    break;
                }
            }
        });
        let mut sub = subscribe(&config, options);
        assert!(sub.refresh().unwrap_err().to_string().contains("队列溢出"));
        assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
        assert_eq!(sub.snapshot().queued_frames, 0);
        assert_eq!(network_stats().active, before);
        server.join().unwrap();
    }
}
#[test]
fn wrong_opcode_and_cursor_regression_are_not_published() {
    let _guard = SERIAL.lock().unwrap();
    for wrong_opcode in [false, true] {
        let (config, server) = setup(move |mut stream| {
            let req = request(&mut stream);
            stream
                .write_all(&response(&req, &refresh_payload(10)))
                .unwrap();
            let req = request(&mut stream);
            let mut reply = response(&req, &refresh_payload(9));
            if wrong_opcode {
                reply[10..12].copy_from_slice(&0x054cu16.to_le_bytes());
            }
            stream.write_all(&reply).unwrap();
        });
        let mut sub = subscribe(&config, SubscriptionOptions::default());
        sub.refresh().unwrap();
        assert!(sub.refresh().is_err());
        assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
        assert_eq!(sub.snapshot().states[0].cursor, 10);
        server.join().unwrap();
    }
}
#[test]
fn endpoint_revocation_and_operation_cancellation_close_the_subscription() {
    let _guard = SERIAL.lock().unwrap();
    for revoke in [true, false] {
        let (config, server) = setup(move |mut stream| {
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let mut sub = subscribe(&config, SubscriptionOptions::default());
        let operation = OperationContext::new(Duration::from_secs(1), None).unwrap();
        let _guard = operation.enter();
        if revoke {
            config.ip_pool.as_ref().unwrap().remove(config.ip.unwrap());
        } else {
            operation.cancel();
        }
        assert!(sub.poll_push(Duration::from_millis(10)).is_err());
        assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
        server.join().unwrap();
    }
}
#[test]
fn update_revocation_and_cancellation_release_the_connection() {
    let _guard = SERIAL.lock().unwrap();
    for revoke in [true, false] {
        let (config, server) = setup(move |mut stream| {
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let before = network_stats().active;
        let mut sub = subscribe(&config, SubscriptionOptions::default());
        let operation = OperationContext::new(Duration::from_secs(1), None).unwrap();
        let _operation_guard = operation.enter();
        if revoke {
            config.ip_pool.as_ref().unwrap().remove(config.ip.unwrap());
        } else {
            operation.cancel();
        }
        assert!(sub.poll_update(Duration::from_millis(10)).is_err());
        assert_eq!(network_stats().active, before);
        assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
        server.join().unwrap();
    }
}

#[test]
fn checked_query_rejects_wrong_message_identity() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        let mut reply = response(&req, &[0, 0]);
        reply[5..9].copy_from_slice(&0xffffu32.to_le_bytes());
        stream.write_all(&reply).unwrap();
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    let error = SpecialLimitsRequest::new(0).recv(&mut tcp).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!tcp.is_reusable());
    drop(tcp);
    server.join().unwrap();
}
#[test]
fn rebuild_reopens_through_daily_pool_and_resets_cursors() {
    let _guard = SERIAL.lock().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pool = ip_pool::prepare(&listener);
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(pool),
        ..Default::default()
    };
    let server = thread::spawn(move || {
        for cursor in [10, 1] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let req = request(&mut stream);
            stream.write_all(&response(&req, &[0, 0])).unwrap();
            let req = request(&mut stream);
            assert_eq!(&req[21..25], &0u32.to_le_bytes());
            stream
                .write_all(&response(&req, &refresh_payload(cursor)))
                .unwrap();
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        }
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    let first = sub.snapshot().connection_id;
    sub.rebuild(Arc::new(metadata())).unwrap();
    assert_ne!(first, sub.snapshot().connection_id);
    assert_eq!(sub.snapshot().generation, 1);
    assert_eq!(sub.snapshot().states[0].cursor, 1);
    sub.close();
    server.join().unwrap();
}

#[test]
fn empty_first_refresh_keeps_baseline_pending_and_duplicate_is_not_accumulated() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        for payload in [vec![0x93, 0x93], refresh_payload(10), refresh_payload(10)] {
            let req = request(&mut stream);
            stream.write_all(&response(&req, &payload)).unwrap();
        }
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    assert!(sub.refresh().unwrap().page.records.is_empty());
    assert_eq!(sub.snapshot().status, DepthSyncStatus::AwaitingBaseline);
    sub.refresh().unwrap();
    let volume = sub.snapshot().states[0]
        .quote
        .as_ref()
        .unwrap()
        .total_volume_raw;
    sub.refresh().unwrap();
    assert_eq!(
        sub.snapshot().states[0]
            .quote
            .as_ref()
            .unwrap()
            .total_volume_raw,
        volume
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Synchronized);
    sub.close();
    server.join().unwrap();
}

#[derive(Debug)]
struct RejectPush;
impl PushObserver for RejectPush {
    fn observe(&self, _: std::net::SocketAddr, _: &QuotePush) -> io::Result<()> {
        Err(io::Error::other("push archive failed"))
    }
}

#[test]
fn observer_failure_does_not_publish_push() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        stream
            .write_all(&response(&req, &refresh_payload(10)))
            .unwrap();
        let mut push = response(&req, &refresh_payload(11));
        push[5..9].copy_from_slice(&99u32.to_le_bytes());
        stream.write_all(&push).unwrap();
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    sub.set_push_observer(Some(Arc::new(RejectPush)));
    assert!(
        sub.poll_push(Duration::from_secs(1))
            .unwrap_err()
            .to_string()
            .contains("archive failed")
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
    assert_eq!(sub.snapshot().states[0].cursor, 10);
    server.join().unwrap();
}

#[test]
fn direct_poll_and_refresh_enforce_resident_memory_budget() {
    let _guard = SERIAL.lock().unwrap();
    for refresh in [false, true] {
        let (config, server) = setup(move |mut stream| {
            let req = if refresh {
                request(&mut stream)
            } else {
                let mut req = vec![0; 12];
                req[10..12].copy_from_slice(&0x0547u16.to_le_bytes());
                req
            };
            stream
                .write_all(&response(&req, &refresh_payload(10)))
                .unwrap();
        });
        let mut sub = subscribe(
            &config,
            SubscriptionOptions {
                max_queue_bytes: 128,
                ..Default::default()
            },
        );
        let error = if refresh {
            sub.refresh().unwrap_err()
        } else {
            sub.poll_push(Duration::from_secs(1)).unwrap_err()
        };
        assert!(error.to_string().contains("队列溢出"));
        assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
        assert!(sub.snapshot().states[0].quote.is_none());
        server.join().unwrap();
    }
}

#[test]
fn rebuild_rejects_expired_metadata_before_reconnecting() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    let expired = PrecisionSnapshot::from_decimals(
        &[(0, "000001", 2)],
        chrono::Utc::now() - chrono::Duration::days(1),
        None,
    )
    .unwrap();
    assert_eq!(
        sub.rebuild(Arc::new(expired)).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
    assert!(sub.snapshot().connection_id.is_none());
    server.join().unwrap();
}

#[test]
fn initial_response_below_requested_cursor_invalidates_without_publishing() {
    let _guard = SERIAL.lock().unwrap();
    let before = network_stats().active;
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        assert_eq!(&req[21..25], &10u32.to_le_bytes());
        stream
            .write_all(&response(&req, &refresh_payload(9)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let request = RefreshRequest::new(vec![RefreshCursor {
        symbol: CommandSymbol::new(0, "000001").unwrap(),
        cursor: 10,
    }])
    .unwrap();
    let mut sub = QuoteSubscription::from_tcp(
        Tcp::with_config(&config).unwrap(),
        request,
        Arc::new(metadata()),
        SubscriptionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        sub.refresh().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let state = sub.snapshot();
    assert_eq!(state.status, DepthSyncStatus::Invalid);
    assert!(state.states[0].quote.is_none());
    assert_eq!(state.states[0].cursor, 10);
    assert_eq!(network_stats().active, before);
    server.join().unwrap();
}

#[test]
fn mixed_valid_and_regressing_records_do_not_partially_publish_response_or_push() {
    let _guard = SERIAL.lock().unwrap();
    for push in [false, true] {
        let (config, server) = setup(move |mut stream| {
            let req = request(&mut stream);
            let payload = |second_cursor| {
                let mut bytes = vec![2, 0];
                bytes.extend(quote(0, "000001", true, 10));
                bytes.extend(quote(0, "159915", true, second_cursor));
                bytes.into_iter().map(|b| b ^ 0x93).collect::<Vec<_>>()
            };
            stream.write_all(&response(&req, &payload(10))).unwrap();
            let req = if push { req } else { request(&mut stream) };
            let mut bytes = vec![2, 0];
            bytes.extend(quote(0, "000001", true, 11));
            bytes.extend(quote(0, "159915", true, 9));
            let payload: Vec<_> = bytes.into_iter().map(|b| b ^ 0x93).collect();
            let mut reply = response(&req, &payload);
            if push {
                reply[5..9].copy_from_slice(&99u32.to_le_bytes());
            }
            stream.write_all(&reply).unwrap();
        });
        let mut sub = QuoteSubscription::with_config(
            &config,
            &[(0, "000001"), (0, "159915")],
            Arc::new(metadata()),
            SubscriptionOptions::default(),
        )
        .unwrap();
        sub.refresh().unwrap();
        let error = if push {
            sub.poll_push(Duration::from_secs(1)).unwrap_err()
        } else {
            sub.refresh().unwrap_err()
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let snapshot = sub.snapshot();
        assert_eq!(snapshot.status, DepthSyncStatus::Invalid);
        assert!(snapshot.states.iter().all(|s| s.cursor == 10));
        server.join().unwrap();
    }
}

#[test]
fn local_poll_timeout_preserves_session_but_operation_deadline_closes_it() {
    let _guard = SERIAL.lock().unwrap();
    let before = network_stats().active;
    let (config, server) = setup(move |mut stream| {
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    assert!(sub.poll_push(Duration::ZERO).unwrap().is_none());
    assert!(sub.poll_push(Duration::from_millis(5)).unwrap().is_none());
    assert_eq!(sub.snapshot().status, DepthSyncStatus::AwaitingBaseline);
    let operation = OperationContext::new(Duration::from_millis(20), None).unwrap();
    let guard = operation.enter();
    assert_eq!(
        sub.poll_push(Duration::from_secs(1)).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    drop(guard);
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
    assert!(sub.snapshot().connection_id.is_none());
    assert_eq!(network_stats().active, before);
    server.join().unwrap();
}

#[test]
fn malformed_push_prefix_compression_and_truncated_body_close_session() {
    let _guard = SERIAL.lock().unwrap();
    for case in 0..3 {
        let (config, server) = setup(move |mut stream| {
            let mut req = vec![0; 12];
            req[10..12].copy_from_slice(&0x0547u16.to_le_bytes());
            let mut push = response(&req, &refresh_payload(0));
            match case {
                0 => push[0] ^= 1,
                1 => {
                    // Announced inflate size differs, but the body is not zlib.
                    push[14..16].copy_from_slice(&100u16.to_le_bytes());
                }
                _ => {
                    push.pop();
                }
            }
            stream.write_all(&push).unwrap();
        });
        let mut sub = subscribe(&config, SubscriptionOptions::default());
        assert!(sub.poll_push(Duration::from_secs(1)).is_err());
        assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
        assert!(sub.snapshot().states[0].quote.is_none());
        server.join().unwrap();
    }
}

#[test]
fn invalid_subscription_budgets_fail_before_opening_a_connection() {
    let _guard = SERIAL.lock().unwrap();
    let before = network_stats().active;
    // No daily pool is configured: opening a socket would fail for a different
    // reason. Invalid options must be rejected before transport is attempted.
    for options in [
        SubscriptionOptions {
            max_queue_frames: 0,
            ..Default::default()
        },
        SubscriptionOptions {
            max_queue_frames: 257,
            ..Default::default()
        },
        SubscriptionOptions {
            max_queue_bytes: 0,
            ..Default::default()
        },
        SubscriptionOptions {
            max_queue_bytes: 8 * 1024 * 1024 + 1,
            ..Default::default()
        },
        SubscriptionOptions {
            refresh_timeout: Duration::ZERO,
            ..Default::default()
        },
        SubscriptionOptions {
            refresh_timeout: Duration::MAX,
            ..Default::default()
        },
    ] {
        assert_eq!(
            QuoteSubscription::with_config(
                &TcpConfig::default(),
                &[(0, "000001")],
                Arc::new(metadata()),
                options
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(network_stats().active, before);
}

/// Synthetic full-state rows: market, code, cursor, price integer, cumulative volume.
fn depth_payload(rows: &[(u8, &str, u32, i64, i64)]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    for &(market, code, cursor, price, volume) in rows {
        bytes.push(market);
        bytes.extend(code.as_bytes());
        bytes.extend(1u16.to_le_bytes());
        for value in [price, 0, 0, 0, 0] {
            bytes.extend(varint(value));
        }
        bytes.extend(cursor.to_le_bytes());
        for value in [0, volume, 0] {
            bytes.extend(varint(value));
        }
        bytes.extend(0u32.to_le_bytes());
        for value in [0, volume, 0, 0] {
            bytes.extend(varint(value));
        }
        for _ in 0..5 {
            for value in [0, 0, volume, volume] {
                bytes.extend(varint(value));
            }
        }
        bytes.extend([0; 46]);
    }
    bytes.into_iter().map(|byte| byte ^ 0x93).collect()
}

fn push_frame(
    req: &[u8],
    id: u32,
    rows: &[(u8, &str, u32, i64, i64)],
    compressed: bool,
) -> Vec<u8> {
    let payload = depth_payload(rows);
    let mut frame = response(req, &payload);
    frame[5..9].copy_from_slice(&id.to_le_bytes());
    if compressed {
        let wire = miniz_oxide::deflate::compress_to_vec_zlib(&payload, 6);
        assert_ne!(wire.len(), payload.len());
        frame[12..14].copy_from_slice(&(wire.len() as u16).to_le_bytes());
        frame.truncate(16);
        frame.extend(wire);
    }
    frame
}

#[derive(Debug, Default)]
struct ObservedPushes(Mutex<Vec<(u32, u64, u64)>>);
impl PushObserver for ObservedPushes {
    fn observe(&self, _: std::net::SocketAddr, push: &QuotePush) -> io::Result<()> {
        self.0
            .lock()
            .unwrap()
            .push((push.message_id, push.connection_id, push.generation));
        Ok(())
    }
}

#[test]
fn passive_multi_symbol_stream_completes_baseline_and_replaces_full_depth_without_requests() {
    let _guard = SERIAL.lock().unwrap();
    let before = network_stats().active;
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        assert_eq!(&req[12..14], &2u16.to_le_bytes());
        stream
            .write_all(&response(
                &req,
                &depth_payload(&[(0, "000001", 10, 1234, 100)]),
            ))
            .unwrap();
        let mut frames = push_frame(&req, 99, &[(0, "159915", 20, 2702, 200)], true);
        // Out-of-request-order, coalesced frames, then a legitimate zero state.
        frames.extend(push_frame(
            &req,
            100,
            &[(0, "159915", 21, 2701, 201), (0, "000001", 11, 1235, 105)],
            false,
        ));
        frames.extend(push_frame(&req, 101, &[(0, "000001", 12, 0, 0)], true));
        stream.write_all(&frames).unwrap();
        // There is only the initial baseline request. Passive reads send nothing,
        // including heartbeat traffic; the next received byte must be EOF.
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = QuoteSubscription::with_config(
        &config,
        &[(0, "000001"), (0, "159915")],
        Arc::new(metadata()),
        SubscriptionOptions::default(),
    )
    .unwrap();
    let observer = Arc::new(ObservedPushes::default());
    sub.set_push_observer(Some(observer.clone()));
    let connection = sub.snapshot().connection_id.unwrap();
    sub.refresh().unwrap();
    assert_eq!(sub.snapshot().status, DepthSyncStatus::AwaitingBaseline);
    assert!(sub.snapshot().states[1].quote.is_none());
    sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Synchronized);
    let second = sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(second.message_id, 100);
    let snapshot = sub.snapshot();
    let a = snapshot.states[0].quote.as_ref().unwrap();
    let b = snapshot.states[1].quote.as_ref().unwrap();
    assert_eq!(
        (snapshot.states[0].cursor, snapshot.states[1].cursor),
        (11, 21)
    );
    assert_eq!(
        (a.price, a.total_volume_raw, a.bids[4].volume_raw),
        (Some(12.35), 105, 105)
    );
    assert_eq!(
        (b.price, b.total_volume_raw, b.asks[4].volume_raw),
        (Some(2.701), 201, 201)
    );
    let b_received = snapshot.states[1].received_at;
    sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    let final_state = sub.snapshot();
    let zero = final_state.states[0].quote.as_ref().unwrap();
    assert_eq!(
        (zero.price, zero.total_volume_raw, zero.outside_volume_raw),
        (Some(0.0), 0, 0)
    );
    assert!(
        zero.bids
            .iter()
            .chain(&zero.asks)
            .all(|level| level.price == Some(0.0) && level.volume_raw == 0)
    );
    assert_eq!(final_state.states[1].cursor, 21);
    assert_eq!(final_state.states[1].received_at, b_received);
    assert_eq!(final_state.queued_frames, 0);
    assert_eq!(
        *observer.0.lock().unwrap(),
        [
            (99, connection, 0),
            (100, connection, 0),
            (101, connection, 0)
        ]
    );
    sub.close();
    assert_eq!(network_stats().active, before);
    server.join().unwrap();
}

#[test]
fn passive_updates_are_used_as_next_request_cursors_instead_of_resetting_baseline() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        stream
            .write_all(&response(
                &req,
                &depth_payload(&[(0, "000001", 10, 1234, 100)]),
            ))
            .unwrap();
        stream
            .write_all(&push_frame(&req, 99, &[(0, "000001", 42, 1240, 107)], true))
            .unwrap();
        let next = request(&mut stream);
        assert_eq!(&next[21..25], &42u32.to_le_bytes());
        assert_ne!(&next[1..5], &req[1..5]);
        stream.write_all(&response(&next, &[0x93, 0x93])).unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(sub.snapshot().states[0].cursor, 42);
    assert!(sub.refresh().unwrap().page.records.is_empty());
    assert_eq!(
        sub.snapshot().states[0]
            .quote
            .as_ref()
            .unwrap()
            .total_volume_raw,
        107
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Synchronized);
    sub.close();
    server.join().unwrap();
}

#[test]
fn invalid_last_symbol_in_passive_page_never_partially_publishes_first_symbol() {
    let _guard = SERIAL.lock().unwrap();
    for regression in [false, true] {
        let (config, server) = setup(move |mut stream| {
            let req = request(&mut stream);
            let baseline = [(0, "000001", 10, 1234, 100), (0, "159915", 20, 2702, 200)];
            stream
                .write_all(&response(&req, &depth_payload(&baseline)))
                .unwrap();
            let invalid = if regression {
                [(0, "000001", 11, 1235, 105), (0, "159915", 19, 2701, 201)]
            } else {
                [(0, "000001", 11, 1235, 105), (1, "510300", 21, 2701, 201)]
            };
            stream
                .write_all(&push_frame(&req, 99, &invalid, true))
                .unwrap();
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let mut sub = QuoteSubscription::with_config(
            &config,
            &[(0, "000001"), (0, "159915")],
            Arc::new(metadata()),
            SubscriptionOptions::default(),
        )
        .unwrap();
        sub.refresh().unwrap();
        assert_eq!(
            sub.poll_push(Duration::from_secs(1)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let final_state = sub.snapshot();
        assert_eq!(final_state.status, DepthSyncStatus::Invalid);
        assert_eq!(
            (final_state.states[0].cursor, final_state.states[1].cursor),
            (10, 20)
        );
        assert_eq!(
            final_state.states[0]
                .quote
                .as_ref()
                .unwrap()
                .total_volume_raw,
            100
        );
        assert_eq!(final_state.connection_id, None);
        server.join().unwrap();
    }
}

#[test]
fn passive_duplicate_and_empty_frames_preserve_state_and_cursor_max_requires_rebuild() {
    let _guard = SERIAL.lock().unwrap();
    let (config, server) = setup(move |mut stream| {
        let req = request(&mut stream);
        let rows = [(0, "000001", u32::MAX, 1234, 100)];
        stream
            .write_all(&response(&req, &depth_payload(&rows)))
            .unwrap();
        let mut frames = push_frame(&req, 99, &rows, false);
        frames.extend(push_frame(&req, 100, &[], false));
        frames.extend(push_frame(&req, 101, &[(0, "000001", 0, 1235, 101)], false));
        stream.write_all(&frames).unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut sub = subscribe(&config, SubscriptionOptions::default());
    sub.refresh().unwrap();
    sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    let duplicate = sub.snapshot();
    assert_eq!(
        duplicate.states[0].quote.as_ref().unwrap().total_volume_raw,
        100
    );
    assert_eq!(duplicate.states[0].cursor, u32::MAX);
    assert!(
        sub.poll_push(Duration::from_secs(1))
            .unwrap()
            .unwrap()
            .refresh
            .page
            .records
            .is_empty()
    );
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Synchronized);
    assert_eq!(
        sub.snapshot().states[0].received_at,
        duplicate.states[0].received_at
    );
    let error = sub.poll_push(Duration::from_secs(1)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{error}");
    assert_eq!(sub.snapshot().states[0].cursor, u32::MAX);
    assert_eq!(sub.snapshot().status, DepthSyncStatus::Invalid);
    server.join().unwrap();
}

#[derive(Debug, Default)]
struct PassiveRecorder {
    refresh_requests: Mutex<usize>,
    pushes: Mutex<Vec<(u32, u64, u64)>>,
}

impl rustdx_complete::tcp::FrameObserver for PassiveRecorder {
    fn observe(&self, event: rustdx_complete::tcp::FrameEvent<'_>) -> io::Result<()> {
        if let rustdx_complete::tcp::FrameEvent::Request { request, .. } = event
            && request.get(10..12) == Some(&0x0547u16.to_le_bytes())
        {
            *self.refresh_requests.lock().unwrap() += 1;
        }
        Ok(())
    }
}

impl PushObserver for PassiveRecorder {
    fn observe(&self, _: std::net::SocketAddr, push: &QuotePush) -> io::Result<()> {
        self.pushes
            .lock()
            .unwrap()
            .push((push.message_id, push.connection_id, push.generation));
        Ok(())
    }
}

#[test]
fn passive_updates_reuse_message_id_without_refresh_or_heartbeat_writes() {
    // Synthetic transport regression; this is not live trading-hour acceptance.
    let _guard = SERIAL.lock().unwrap();
    let (sent_tx, sent_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let (config, server) = setup(move |mut stream| {
        let baseline = request(&mut stream);
        stream
            .write_all(&response(&baseline, &refresh_payload(10)))
            .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        for (cursor, ask_volume) in [(11, 201), (12, 202)] {
            continue_rx.recv().unwrap();
            let mut row = quote(0, "000001", true, cursor);
            row.truncate(row.len() - varint(200).len());
            row.extend(varint(ask_volume));
            let mut payload = 1u16.to_le_bytes().to_vec();
            payload.extend(row);
            payload.iter_mut().for_each(|b| *b ^= 0x93);
            // Servers may reuse the baseline message ID for unsolicited updates.
            stream.write_all(&response(&baseline, &payload)).unwrap();
            let mut byte = [0];
            let error = stream.read(&mut byte).unwrap_err();
            assert!(matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ));
            sent_tx.send(()).unwrap();
        }
        continue_rx.recv().unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let observer = Arc::new(PassiveRecorder::default());
    let mut tcp = Tcp::with_config_and_observer(&config, observer.clone()).unwrap();
    tcp.set_heartbeat_interval(Some(Duration::from_millis(20)))
        .unwrap();
    let mut sub = QuoteSubscription::from_tcp(
        tcp,
        RefreshRequest::from_stocks(&[(0, "000001")]).unwrap(),
        Arc::new(metadata()),
        SubscriptionOptions::default(),
    )
    .unwrap();
    sub.set_push_observer(Some(observer.clone()));
    sub.refresh().unwrap();
    let baseline = sub.snapshot();
    for (cursor, ask_volume) in [(11, 201), (12, 202)] {
        continue_tx.send(()).unwrap();
        let push = sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(push.message_id, 1);
        assert_eq!(push.connection_id, baseline.connection_id.unwrap());
        assert_eq!(push.generation, baseline.generation);
        assert_eq!(push.refresh.page.records[0].update_cursor, Some(cursor));
        let state = sub.snapshot();
        assert_eq!(state.status, DepthSyncStatus::Synchronized);
        assert_eq!(state.states[0].cursor, cursor);
        assert_eq!(
            state.states[0].quote.as_ref().unwrap().asks[4].volume_raw,
            ask_volume
        );
        sent_rx.recv().unwrap();
    }
    assert_eq!(*observer.refresh_requests.lock().unwrap(), 1);
    assert_eq!(observer.pushes.lock().unwrap().len(), 2);
    sub.close();
    continue_tx.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn overflow_rebuild_requires_every_symbol_before_business_consumption() {
    let _guard = SERIAL.lock().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let pool = ip_pool::prepare(&listener);
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(pool),
        ..Default::default()
    };
    let server = thread::spawn(move || {
        for generation in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let hello = request(&mut stream);
            stream.write_all(&response(&hello, &[0, 0])).unwrap();
            let req = request(&mut stream);
            assert_eq!(&req[21..25], &0_u32.to_le_bytes());
            assert_eq!(&req[32..36], &0_u32.to_le_bytes());
            if generation == 0 {
                let mut frames = push_frame(&req, 99, &[(0, "000001", 10, 1234, 100)], false);
                frames.extend(push_frame(
                    &req,
                    100,
                    &[(0, "159915", 20, 2702, 200)],
                    false,
                ));
                stream.write_all(&frames).unwrap();
            } else {
                stream
                    .write_all(&response(
                        &req,
                        &depth_payload(&[(0, "000001", 1, 1234, 100)]),
                    ))
                    .unwrap();
                stream
                    .write_all(&push_frame(&req, 99, &[(0, "159915", 2, 2702, 200)], true))
                    .unwrap();
            }
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        }
    });
    let mut sub = QuoteSubscription::with_config(
        &config,
        &[(0, "000001"), (0, "159915")],
        Arc::new(metadata()),
        SubscriptionOptions {
            max_queue_frames: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(sub.synchronized_snapshot().is_err());
    assert!(sub.refresh().unwrap_err().to_string().contains("溢出"));
    assert!(sub.synchronized_snapshot().is_err());
    sub.rebuild(Arc::new(metadata())).unwrap();
    assert_eq!(sub.snapshot().generation, 1);
    assert_eq!(sub.snapshot().states[1].cursor, 0);
    assert!(sub.synchronized_snapshot().is_err());
    sub.poll_push(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(sub.synchronized_snapshot().unwrap().states.len(), 2);
    sub.close();
    server.join().unwrap();
}
