//! Compare every exposed field to a separately pinned pytdx implementation.
use rustdx_complete::tcp::{
    Tdx,
    stock::{
        FinanceInfo, FinanceRequest, HistoryTransaction, HistoryTransactionRequest, QuotesRequest,
        SecurityQuotes, Transaction, TransactionRequest, decode_finance,
        decode_history_transactions, decode_quotes, decode_transactions,
    },
};
use std::path::Path;

fn bytes(name: &str, suffix: &str) -> Vec<u8> {
    let hex = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/protocol")
            .join(format!("{name}.{suffix}.hex")),
    )
    .unwrap();
    hex.trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

fn compare<T: serde::Serialize>(name: &str, rows: &[T]) {
    let mut writer = csv::Writer::from_writer(Vec::new());
    for row in rows {
        writer.serialize(row).unwrap();
    }
    let output = writer.into_inner().unwrap();
    let mut actual = csv::Reader::from_reader(output.as_slice());
    let headers = actual.headers().unwrap().clone();
    let mut expected = csv::Reader::from_path(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/protocol")
            .join(format!("{name}.expected.csv")),
    )
    .unwrap();
    let reference_headers = expected.headers().unwrap().clone();
    assert_eq!(
        headers.len(),
        reference_headers.len(),
        "{name}: field coverage"
    );
    let reference: Vec<_> = expected.records().collect::<Result<_, _>>().unwrap();
    let actual: Vec<_> = actual.records().collect::<Result<_, _>>().unwrap();
    assert_eq!(actual.len(), reference.len());
    for (row, expected) in actual.iter().zip(reference) {
        for (i, field) in headers.iter().enumerate() {
            let j = reference_headers.iter().position(|h| h == field).unwrap();
            if matches!(field, "code" | "time" | "servertime") {
                assert_eq!(row[i], expected[j], "{name}.{field}");
            } else {
                assert_eq!(
                    row[i].parse::<f64>().unwrap(),
                    expected[j].parse::<f64>().unwrap(),
                    "{name}.{field}",
                );
            }
        }
    }
}

#[test]
fn real_samples_match_independent_requests_and_all_fields() {
    let quotes = QuotesRequest::new(vec![(0, "000001"), (1, "600000")]).unwrap();
    let finance = FinanceRequest::new(0, "000001").unwrap();
    let ticks = TransactionRequest::new(0, "000001", 0, 3).unwrap();
    let history = HistoryTransactionRequest::new(0, "000001", 0, 3, 20261022).unwrap();
    for (name, request) in [
        ("quotes", quotes.encode()),
        ("finance", finance.encode().to_vec()),
        ("transaction", ticks.encode().to_vec()),
        ("history-transaction", history.encode().to_vec()),
    ] {
        assert_eq!(request, bytes(name, "request"));
    }
    let q = bytes("quotes", "response");
    compare("quotes", &decode_quotes(&quotes, &q).unwrap());
    let mut old = SecurityQuotes::new(quotes.stocks().to_vec());
    old.try_parse(q).unwrap();
    compare("quotes", &old.data);
    let f = bytes("finance", "response");
    compare("finance", &decode_finance(&finance, &f).unwrap());
    let mut old = FinanceInfo::new(0, "000001");
    old.try_parse(f).unwrap();
    compare("finance", &old.data);
    let t = bytes("transaction", "response");
    compare("transaction", &decode_transactions(&ticks, &t).unwrap());
    let mut old = Transaction::new(0, "000001", 0, 3);
    old.try_parse(t).unwrap();
    compare("transaction", &old.data);
    let h = bytes("history-transaction", "response");
    compare(
        "history-transaction",
        &decode_history_transactions(&history, &h).unwrap(),
    );
    let mut old = HistoryTransaction::new(0, "000001", 0, 3, 20261022);
    old.try_parse(h).unwrap();
    compare("history-transaction", &old.data);
}

#[test]
fn truncation_excess_trailing_and_stale_results_are_rejected() {
    for name in ["quotes", "finance", "transaction", "history-transaction"] {
        let decode = |input: &[u8]| match name {
            "quotes" => decode_quotes(
                &QuotesRequest::new(vec![(0, "000001"), (1, "600000")]).unwrap(),
                input,
            )
            .map(|v| v.len()),
            "finance" => {
                decode_finance(&FinanceRequest::new(0, "000001").unwrap(), input).map(|v| v.len())
            }
            "transaction" => {
                decode_transactions(&TransactionRequest::new(0, "000001", 0, 3).unwrap(), input)
                    .map(|v| v.len())
            }
            _ => decode_history_transactions(
                &HistoryTransactionRequest::new(0, "000001", 0, 3, 20261022).unwrap(),
                input,
            )
            .map(|v| v.len()),
        };
        let sample = bytes(name, "response");
        for end in 0..sample.len() {
            assert!(decode(&sample[..end]).is_err(), "{name}: prefix {end}");
        }
        let mut invalid = sample.clone();
        invalid.push(0);
        assert!(decode(&invalid).is_err());
        let mut excess = sample.clone();
        let offset = if name == "quotes" { 2 } else { 0 };
        excess[offset..offset + 2].copy_from_slice(&2001u16.to_le_bytes());
        assert!(decode(&excess).is_err());
        macro_rules! stale {
            ($request:expr) => {{
                let mut legacy = $request;
                legacy.try_parse(sample.clone()).unwrap();
                assert!(legacy.try_parse(invalid.clone()).is_err());
                assert!(legacy.data.is_empty());
            }};
        }
        match name {
            "quotes" => stale!(SecurityQuotes::new(vec![(0, "000001"), (1, "600000")])),
            "finance" => stale!(FinanceInfo::new(0, "000001")),
            "transaction" => stale!(Transaction::new(0, "000001", 0, 3)),
            _ => stale!(HistoryTransaction::new(0, "000001", 0, 3, 20261022)),
        }
    }
}

#[test]
fn response_identity_numeric_and_date_constraints() {
    let finance = FinanceRequest::new(0, "000001").unwrap();
    let original = bytes("finance", "response");
    for (offset, value) in [(2, 1), (8, b'2')] {
        let mut wrong = original.clone();
        wrong[offset] = value;
        assert!(decode_finance(&finance, &wrong).is_err());
    }
    for offset in [9, 25, 141] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad = original.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(decode_finance(&finance, &bad).is_err());
        }
    }
    let mut bad = original.clone();
    bad[17..21].copy_from_slice(&20260230u32.to_le_bytes());
    assert!(decode_finance(&finance, &bad).is_err());
    assert!(decode_finance(&finance, &[0, 0]).unwrap().is_empty());
    let quotes = QuotesRequest::new(vec![(0, "000001"), (1, "600000")]).unwrap();
    let mut bad = bytes("quotes", "response");
    bad[4] = 1;
    assert!(decode_quotes(&quotes, &bad).is_err());
    let one = QuotesRequest::new(vec![(0, "000001")]).unwrap();
    // Existing real single-quote sample, duplicated without a second identity.
    let hex = "0114010000303030303031fe08a61242440a45bb9fe30ae612a9853d0d31160e4e91981e98ed1e00bfaf0700019f12074102a724ab3b4203911295164304a8288a234405a33296457501000000000000fe08";
    let body: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(decode_quotes(&one, &body).unwrap().len(), 1);
    let mut duplicate = body.clone();
    duplicate[2..4].copy_from_slice(&2u16.to_le_bytes());
    duplicate.extend_from_slice(&body[4..]);
    assert!(decode_quotes(&quotes, &duplicate).is_err());
    for historical in [false, true] {
        let name = if historical {
            "history-transaction"
        } else {
            "transaction"
        };
        let mut bad = bytes(name, "response");
        let offset = if historical { 6 } else { 2 };
        bad[offset..offset + 2].copy_from_slice(&1440u16.to_le_bytes());
        if historical {
            assert!(
                decode_history_transactions(
                    &HistoryTransactionRequest::new(0, "000001", 0, 3, 20261022).unwrap(),
                    &bad
                )
                .is_err()
            );
        } else {
            assert!(
                decode_transactions(&TransactionRequest::new(0, "000001", 0, 3).unwrap(), &bad)
                    .is_err()
            );
        }
    }
}

#[test]
fn invalid_parameters_fail_without_panicking() {
    for code in ["", "12345", "0000010", "中文", "0000/1"] {
        assert!(FinanceRequest::new(0, code).is_err());
        assert!(QuotesRequest::new(vec![(0, code)]).is_err());
        assert!(TransactionRequest::new(0, code, 0, 3).is_err());
        assert!(HistoryTransactionRequest::new(0, code, 0, 3, 20261022).is_err());
    }
    for count in [0, 2001, u16::MAX] {
        assert!(TransactionRequest::new(0, "000001", 0, count).is_err());
        assert!(HistoryTransactionRequest::new(0, "000001", 0, count, 20261022).is_err());
    }
    assert!(QuotesRequest::new(Vec::new()).is_err());
    assert!(QuotesRequest::new(vec![(0, "000001"); 81]).is_err());
    assert!(QuotesRequest::new(vec![(0, "000001"); 2]).is_err());
    assert!(FinanceRequest::new(2, "000001").is_err());
    assert!(TransactionRequest::new(2, "000001", 0, 3).is_err());
    for date in [0, 101, 20260230, 100000101, u32::MAX] {
        assert!(HistoryTransactionRequest::new(0, "000001", 0, 3, date).is_err());
    }
}

#[test]
fn tick_values_are_checked_and_unknown_direction_is_preserved() {
    let request = TransactionRequest::new(0, "000001", 0, 1).unwrap();
    let valid = [1, 0, 0x3a, 2, 1, 1, 1, 8, 0];
    assert_eq!(
        decode_transactions(&request, &valid).unwrap()[0].buyorsell,
        8
    );
    for offset in [4, 5, 6] {
        let mut invalid = valid;
        invalid[offset] = 0x41; // -1: price, volume or trade number.
        assert!(decode_transactions(&request, &invalid).is_err());
    }
    assert!(decode_transactions(&request, &[0, 0]).unwrap().is_empty());
    let history = HistoryTransactionRequest::new(0, "000001", 0, 1, 20261022).unwrap();
    assert!(
        decode_history_transactions(&history, &[0; 6])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn client_sends_checked_bytes_and_uses_the_independent_decoders() {
    use rustdx_complete::tcp::{PACK1, TcpConfig, stock::Client};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    fn frame(stream: &mut TcpStream, body: &[u8]) {
        let mut header = [0; 16];
        header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
        header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
        stream.write_all(&header).unwrap();
        stream.write_all(body).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        timeout: std::time::Duration::from_secs(2),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut handshake = vec![0; PACK1.len()];
        stream.read_exact(&mut handshake).unwrap();
        assert_eq!(handshake, PACK1);
        frame(&mut stream, &[0, 0]);
        for name in ["quotes", "finance", "transaction", "history-transaction"] {
            let expected = bytes(name, "request");
            let mut received = vec![0; expected.len()];
            stream.read_exact(&mut received).unwrap();
            assert_eq!(received, expected);
            frame(&mut stream, &bytes(name, "response"));
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    assert!(client.quotes(&[(0, "000001"), (0, "000001")]).is_err());
    assert!(client.transaction(0, "000001", 0, 0).is_err());
    assert!(client.history_transaction(0, "000001", 0, 3, 0).is_err());
    compare(
        "quotes",
        &client.quotes(&[(0, "000001"), (1, "600000")]).unwrap(),
    );
    compare("finance", &[client.finance(0, "000001").unwrap()]);
    compare(
        "transaction",
        &client.transaction(0, "000001", 0, 3).unwrap(),
    );
    compare(
        "history-transaction",
        &client
            .history_transaction(0, "000001", 0, 3, 20261022)
            .unwrap(),
    );
    server.join().unwrap();
}
