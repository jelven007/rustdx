//! Bounded day parsing: private disk snapshot, sorted chunk runs, then disk merge.
use crate::quality::{FileReport, QualityAccumulator, QualityRow, SourceEvidence};
use chrono::NaiveDate;
use eyre::{Result, eyre};
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::CalendarProvider,
    file::gbbq::Factor,
    operation::{self, CheckedIo},
};
use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub(super) const CHUNK_ROWS: usize = 4096;
const SOURCE_LIMIT: u64 = 32 * 1024 * 1024;
const MERGE_FAN_IN: usize = 8;

pub(super) struct ParsedSource {
    pub evidence: Option<SourceEvidence>,
    pub quality: FileReport,
    pub spool: Option<AtomicFile>,
    pub rows: usize,
}

fn snapshot(source: &Path, output: &str) -> Result<(SourceEvidence, AtomicFile)> {
    let mut reader = CheckedIo(File::open(source)?).take(SOURCE_LIMIT + 1);
    let (temporary, mut file) = AtomicFile::create(output)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 128 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > SOURCE_LIMIT {
            eyre::bail!("来源文件超出{SOURCE_LIMIT}字节限制: {}", source.display());
        }
        file.write_all(&buffer[..n])?;
        hash.update(&buffer[..n]);
    }
    Ok((
        SourceEvidence {
            source: source.display().to_string(),
            sha256: format!("{:x}", hash.finalize()),
            bytes,
        },
        temporary,
    ))
}

pub(super) fn parse_source<F, T>(
    code: &str,
    source: &Path,
    output: &str,
    strict: bool,
    calendar: &dyn CalendarProvider,
    parse: &F,
) -> ParsedSource
where
    F: Fn(&str, &[u8], Option<&Factor>) -> Result<Vec<T>>,
    T: serde::Serialize + QualityRow,
{
    let mut quality = QualityAccumulator::new(source.display().to_string(), code.to_owned());
    let mut evidence = None;
    let result = (|| -> Result<Option<(AtomicFile, usize)>> {
        let (proof, temporary) = snapshot(source, output)?;
        let bytes = proof.bytes;
        evidence = Some(proof);
        if !bytes.is_multiple_of(32) {
            eyre::bail!("day文件末尾记录截断");
        }
        let mut reader = CheckedIo(File::open(temporary.temporary_path())?);
        let mut buffer = vec![0; CHUNK_ROWS * 32];
        let mut remaining = bytes;
        let mut carry = None;
        let mut runs = Vec::new();
        let equity = code.parse::<rustdx_complete::data::Symbol>()?.is_a_share();
        let (direct, file) = AtomicFile::create(output)?;
        let mut writer = csv::Writer::from_writer(file);
        while remaining > 0 {
            operation::check()?;
            let size = remaining.min(buffer.len() as u64) as usize;
            reader.read_exact(&mut buffer[..size])?;
            let rows = parse(code, &buffer[..size], carry.as_ref())?;
            if rows.len() != size / 32 {
                eyre::bail!("分块解析行数与源记录数不一致");
            }
            quality.extend(&rows, calendar)?;
            carry = rows.last().and_then(QualityRow::carry_factor);
            if equity {
                // Equity quality requires strict ascending dates. Failed
                // validation discards this private spool before publication.
                for row in rows {
                    operation::check()?;
                    writer.serialize(row)?;
                }
            } else {
                runs.push(sorted_run(rows, output)?);
            }
            remaining -= size as u64;
        }
        if equity && bytes > 0 {
            writer.flush()?;
            return Ok(Some((direct, (bytes / 32) as usize)));
        }
        if runs.is_empty() {
            return Ok(None);
        }
        // Fixed fan-in also bounds the number of simultaneously open run files.
        while runs.len() > MERGE_FAN_IN {
            let mut next = Vec::new();
            let mut pending = runs.into_iter();
            loop {
                let group: Vec<_> = pending.by_ref().take(MERGE_FAN_IN).collect();
                if group.is_empty() {
                    break;
                }
                next.push(merge_runs(&group, output)?.0);
            }
            runs = next;
        }
        let (spool, rows) = merge_runs(&runs, output)?;
        Ok(Some((spool, rows)))
    })();
    let (spool, rows) = match result {
        Ok(Some((spool, rows))) => (Some(spool), rows),
        Ok(None) => (None, 0),
        Err(error) => {
            quality.error(format!("{error:#}"));
            (None, 0)
        }
    };
    let mut quality = quality.finish();
    if spool.is_some() && rows != quality.rows {
        if quality.warnings.len() < 64 {
            quality.warnings.push(format!(
                "输出前按日期去重，保留源文件最后一条：{}条 -> {rows}条",
                quality.rows
            ));
        } else {
            quality.warnings_omitted += 1;
        }
    }
    let invalid = !quality.errors.is_empty() || (strict && !quality.warnings.is_empty());
    ParsedSource {
        evidence,
        quality,
        spool: if invalid { None } else { spool },
        rows,
    }
}

fn sorted_run<T: QualityRow + serde::Serialize>(rows: Vec<T>, output: &str) -> Result<AtomicFile> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    let mut dates = Vec::with_capacity(rows.len());
    for row in rows {
        operation::check()?;
        dates.push(row.date()?);
        writer.serialize(row)?;
    }
    let bytes = writer.into_inner()?;
    let mut reader = csv::Reader::from_reader(bytes.as_slice());
    let header = reader.headers()?.clone();
    let mut records = reader
        .records()
        .zip(dates)
        .enumerate()
        .map(|(index, (record, date))| Ok((date, index, record?)))
        .collect::<Result<Vec<_>>>()?;
    records.sort_by_key(|(date, index, _)| (*date, *index));
    let (run, file) = AtomicFile::create(output)?;
    let mut writer = csv::Writer::from_writer(file);
    writer.write_record(&header)?;
    let mut records = records.into_iter().peekable();
    while let Some((date, _, record)) = records.next() {
        operation::check()?;
        if records.peek().is_none_or(|(next, _, _)| *next != date) {
            writer.write_record(&record)?;
        }
    }
    writer.flush()?;
    Ok(run)
}

/// Later runs contain later source positions, including across merge passes.
fn merge_runs(runs: &[AtomicFile], output: &str) -> Result<(AtomicFile, usize)> {
    let (spool, file) = AtomicFile::create(output)?;
    let mut writer = csv::Writer::from_writer(file);
    let mut readers = runs
        .iter()
        .map(|run| csv::Reader::from_path(run.temporary_path()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let header = readers
        .first_mut()
        .ok_or_else(|| eyre!("没有待合并的数据块"))?
        .headers()?
        .clone();
    let date_index = header
        .iter()
        .position(|h| h == "date")
        .ok_or_else(|| eyre!("CSV缺少date"))?;
    writer.write_record(&header)?;
    let mut heap = BinaryHeap::new();
    let mut heads = vec![csv::StringRecord::new(); readers.len()];
    for (index, reader) in readers.iter_mut().enumerate() {
        if reader.headers()? != &header {
            eyre::bail!("分块CSV列定义不一致");
        }
        if reader.read_record(&mut heads[index])? {
            heap.push(Reverse((record_date(&heads[index], date_index)?, index)));
        }
    }
    let mut pending: Option<(NaiveDate, csv::StringRecord)> = None;
    let mut rows = 0;
    while let Some(Reverse((date, index))) = heap.pop() {
        operation::check()?;
        if let Some((previous, record)) = pending.take()
            && previous != date
        {
            writer.write_record(&record)?;
            rows += 1;
        }
        pending = Some((date, heads[index].clone()));
        if readers[index].read_record(&mut heads[index])? {
            heap.push(Reverse((record_date(&heads[index], date_index)?, index)));
        }
    }
    if let Some((_, record)) = pending {
        writer.write_record(&record)?;
        rows += 1;
    }
    writer.flush()?;
    Ok((spool, rows))
}

fn record_date(record: &csv::StringRecord, index: usize) -> Result<NaiveDate> {
    Ok(record
        .get(index)
        .ok_or_else(|| eyre!("CSV日期缺失"))?
        .parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustdx_complete::file::day::{self, fq::Day};

    fn raw(date: u32, close: u32) -> Vec<u8> {
        [
            date,
            close,
            close,
            close,
            close,
            1000.0f32.to_bits(),
            100,
            0,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect()
    }

    #[test]
    fn chunk_carry_matches_whole_file_with_suspended_events() {
        let dir = std::env::temp_dir().join(format!("rustdx-fq-chunks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("sz000001.day");
        let output = dir.join("out.csv");
        let mut bytes = Vec::new();
        let first: NaiveDate = "2000-01-01".parse().unwrap();
        use chrono::Datelike;
        for i in 0..CHUNK_ROWS + 3 {
            let d = first + chrono::Duration::days(i as i64 * 2);
            bytes.extend(raw(
                d.year() as u32 * 10000 + d.month() * 100 + d.day(),
                1000,
            ));
        }
        let mut event_date: NaiveDate = first + chrono::Duration::days(CHUNK_ROWS as i64 * 2 - 1);
        let event = |d: NaiveDate| rustdx_complete::file::gbbq::Gbbq {
            market: 0,
            code: "000001",
            date: d.year() as u32 * 10000 + d.month() * 100 + d.day(),
            category: 1,
            fh_qltp: 1.0,
            pgj_qzgb: 0.0,
            sg_hltp: 0.0,
            pg_hzgb: 0.0,
        };
        let mut events = vec![event(event_date)];
        event_date += chrono::Duration::days(2);
        events.push(event(event_date));
        std::fs::write(&source, &bytes).unwrap();
        for previous in [
            None,
            Some(Factor {
                code: "sz000001".into(),
                date: "1999-12-31".into(),
                preclose: 9.5,
                factor: 2.0,
            }),
        ] {
            let whole = if previous.is_some() {
                Day::concat_bytes("sz000001", &bytes, Some(&events), previous.as_ref()).unwrap()
            } else {
                Day::from_bytes("sz000001", &bytes, Some(&events)).unwrap()
            };
            let parsed = parse_source(
                "sz000001",
                &source,
                output.to_str().unwrap(),
                false,
                &rustdx_complete::calendar::BuiltinCalendar,
                &|code, chunk, carry| {
                    Ok(if let Some(carry) = carry.or(previous.as_ref()) {
                        Day::concat_bytes(code, chunk, Some(&events), Some(carry))?
                    } else {
                        Day::from_bytes(code, chunk, Some(&events))?
                    })
                },
            );
            assert!(parsed.quality.errors.is_empty());
            let spool = parsed.spool.unwrap();
            let mut expected = csv::Writer::from_writer(Vec::new());
            for row in whole {
                expected.serialize(row).unwrap();
            }
            assert_eq!(
                expected.into_inner().unwrap(),
                std::fs::read(spool.temporary_path()).unwrap()
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn multipass_merge_keeps_last_source_duplicate_and_bounds_diagnostics() {
        let dir = std::env::temp_dir().join(format!("rustdx-chunks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("sh000001.day");
        let output = dir.join("out.csv");
        // Nine runs force a second merge pass; duplicate winners lie in different runs.
        let mut bytes = Vec::new();
        for i in 0..CHUNK_ROWS * 9 + 1 {
            bytes.extend(raw(
                if i % 2 == 0 { 20250107 } else { 20250106 },
                100 + i as u32,
            ));
        }
        std::fs::write(&source, &bytes).unwrap();
        let parsed = parse_source(
            "sh000001",
            &source,
            output.to_str().unwrap(),
            false,
            &rustdx_complete::calendar::BuiltinCalendar,
            &|code, bytes, _| Ok(day::Day::from_bytes_into_vec(code, bytes)?),
        );
        assert_eq!(
            parsed.evidence.unwrap().sha256,
            crate::quality::sha256(&bytes)
        );
        assert!(parsed.quality.errors.is_empty());
        assert_eq!(parsed.quality.warnings.len(), 64);
        assert!(parsed.quality.warnings_omitted > CHUNK_ROWS);
        assert_eq!(parsed.rows, 2);
        let spool = parsed.spool.unwrap();
        let mut reader = csv::Reader::from_path(spool.temporary_path()).unwrap();
        let rows: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
        assert_eq!(&rows[0][0], "2025-01-06");
        assert_eq!(&rows[1][0], "2025-01-07");
        assert_eq!(
            rows[0][5].parse::<f64>().unwrap(),
            (100 + CHUNK_ROWS * 9 - 1) as f64 / 100.0
        );
        assert_eq!(
            rows[1][5].parse::<f64>().unwrap(),
            (100 + CHUNK_ROWS * 9) as f64 / 100.0
        );
        drop(spool);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
