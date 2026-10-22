//! Capture raw response bodies for independent offline decoder verification.
use rustdx_complete::tcp::{Tcp, TcpConfig, send_recv_decompress, stock::KlineRequest};
use std::{fmt::Write as _, path::Path, time::Duration};

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2 + 1);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to String");
    }
    text.push('\n');
    text
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("usage: protocol_capture <new-fixture-directory>")?;
    let directory = Path::new(&directory);
    // Refuse to replace existing verification evidence.
    std::fs::create_dir(directory)?;
    let mut tcp = Tcp::with_config(&TcpConfig {
        timeout: Duration::from_secs(5),
        auto_reconnect: 2,
        retry_delay_ms: 200,
        ..Default::default()
    })?;
    for (name, market, code, category) in [
        ("stock-day", 0, "000001", 9),
        ("index-day", 1, "000001", 9),
        ("stock-minute", 0, "000001", 0),
    ] {
        let request = KlineRequest::new(market, code, category, 0, 3)?;
        let encoded = request.encode();
        let bytes = send_recv_decompress(&mut tcp, &encoded, name)?;
        let count = if name == "index-day" {
            rustdx_complete::tcp::stock::decode_index_bars(&request, &bytes)?.len()
        } else {
            rustdx_complete::tcp::stock::decode_stock_bars(&request, &bytes)?.len()
        };
        if count != 3 {
            return Err(format!("{name}: expected 3 bars, received {count}").into());
        }
        std::fs::write(directory.join(format!("{name}.request.hex")), hex(&encoded))?;
        std::fs::write(directory.join(format!("{name}.response.hex")), hex(&bytes))?;
        println!("{name}: {count} bars, {} response bytes", bytes.len());
    }
    Ok(())
}
