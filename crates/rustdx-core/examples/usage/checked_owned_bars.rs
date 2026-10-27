//! Offline checked request -> decoder -> owned canonical bars -> atomic JSON.
use rustdx_complete::{
    data::{CanonicalBar, Market, Period, Symbol, VolumeUnit},
    tcp::stock::KlineRequest,
};
use sha2::{Digest, Sha256};
use std::{io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hex = include_str!("../../tests/fixtures/kline/stock-day.response.hex").trim();
    let bytes: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| -> Result<u8, Box<dyn std::error::Error>> {
            Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?)
        })
        .collect::<Result<_, _>>()?;
    // Drop the borrowed request/symbol before serialization. CanonicalBar owns
    // its symbol and retains period, price basis, decoder and source unit basis.
    let bars: Vec<CanonicalBar> = {
        let symbol = Symbol::new(Market::Shenzhen, "000001")?;
        let request = KlineRequest::for_symbol(&symbol, Period::Day, 0, 3)?;
        // CallerSpecified is explicit. A successful structural decoder does
        // not independently establish lots, prices or requested-date coverage.
        request.decode_canonical_stock(&bytes, VolumeUnit::Lots)?
    };
    let payload = serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version":1,
        "fixture":"tests/fixtures/kline/stock-day.response.hex",
        "raw_payload_sha256":format!("{:x}", Sha256::digest(&bytes)),
        "acquisition":"offline replay; no endpoint/time qualification",
        "semantic_qualification":"caller-specified source unit; no publication permission",
        "bars":bars,
    }))?;
    if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
        if path.exists() {
            return Err("use a new output file".into());
        }
        let (atomic, mut file) = rustdx_complete::atomic_file::AtomicFile::create(&path)?;
        file.write_all(&payload)?;
        file.write_all(b"\n")?;
        drop(file);
        atomic.commit()?;
    } else {
        println!("{}", std::str::from_utf8(&payload)?);
    }
    Ok(())
}
