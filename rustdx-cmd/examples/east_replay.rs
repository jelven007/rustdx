//! cargo run -p jelven-rustdx-cli --example east_replay -- manifest.json replay.csv [calendar.json]
//! Replays source snapshots; does not recompute previous-file return indices.
use eyre::{Result, ensure};
use rustdx_cli::eastmoney::{MAX_PAGE_BYTES, RawPage, replay};
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::{BuiltinCalendar, CalendarDataset, CalendarProvider, HolidayCalendar},
};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "evidence file exceeds limit");
    Ok(bytes)
}

fn page(evidence: &serde_json::Value) -> Result<RawPage> {
    let path = evidence["source"]
        .as_str()
        .ok_or_else(|| eyre::eyre!("missing source"))?;
    // JSON escaping can expand each body byte sixfold.
    let bytes = bounded(Path::new(path), 6 * MAX_PAGE_BYTES + 8192)?;
    ensure!(
        evidence["bytes"].as_u64() == Some(bytes.len() as u64),
        "evidence length mismatch"
    );
    ensure!(
        evidence["sha256"].as_str() == Some(format!("{:x}", Sha256::digest(&bytes)).as_str()),
        "evidence file digest mismatch"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        (2..=3).contains(&args.len()),
        "usage: east_replay manifest.json replay.csv [calendar.json]"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&bounded(Path::new(&args[0]), 16 * 1024 * 1024)?)?;
    let pages = manifest["raw_pages"]
        .as_array()
        .ok_or_else(|| eyre::eyre!("missing raw_pages"))?;
    let first = page(
        pages
            .first()
            .ok_or_else(|| eyre::eyre!("no raw evidence"))?,
    )?;
    let output = Path::new(&args[1]);
    if output.exists() {
        let canonical = std::fs::canonicalize(output)?;
        for input in std::iter::once(args[0].as_str())
            .chain(args.get(2).map(String::as_str))
            .chain(pages.iter().filter_map(|p| p["source"].as_str()))
        {
            ensure!(
                std::fs::canonicalize(input)? != canonical,
                "output must not overwrite evidence"
            );
        }
    }
    let calendar: Box<dyn CalendarProvider> = match args.get(2) {
        Some(path) => Box::new(HolidayCalendar::try_from(serde_json::from_slice::<
            CalendarDataset,
        >(&bounded(
            Path::new(path),
            1024 * 1024,
        )?)?)?),
        None => Box::new(BuiltinCalendar),
    };
    let market = replay(
        first.max,
        std::iter::once(Ok(first)).chain(pages[1..].iter().map(page)),
        calendar.as_ref(),
    )?;
    let (atomic, file) = AtomicFile::create(output)?;
    let mut writer = csv::Writer::from_writer(file);
    for row in market.data.diff {
        if row.close.is_some() {
            row.to_canonical_with_calendar(calendar.as_ref())?;
            writer.serialize(row)?;
        }
    }
    writer.flush()?;
    drop(writer);
    atomic.commit()?;
    Ok(())
}
