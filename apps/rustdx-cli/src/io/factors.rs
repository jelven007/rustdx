//! Preserve factor input evidence and validate local/database rows identically.
use std::{collections::HashMap, io::Write, path::PathBuf};

use eyre::Result;
use rustdx_complete::{file::gbbq::Factor, operation};

use crate::quality::CollectionReport;

type Previous = Result<HashMap<String, Factor>>;

pub fn tracked_previous(
    path: &Option<PathBuf>,
    table: &str,
    keep_factor: bool,
    report: &mut CollectionReport,
) -> Previous {
    if let Some(path) = crate::paths::previous_input(path.as_deref()) {
        let (evidence, bytes) = crate::quality::source_snapshot(&path, 64 * 1024 * 1024)?;
        report.sources.push(evidence);
        parse_previous(bytes.as_slice())
    } else {
        let text = clickhouse_factor_text(table)?;
        report.sources.push(crate::quality::SourceEvidence {
            source: format!("clickhouse:{table}:previous"),
            sha256: crate::quality::sha256(text.as_bytes()),
            bytes: text.len() as u64,
        });
        save_factor_and_parse(&text, keep_factor)
    }
}

/// Full market-prefixed codes are required, matching day CSV output.
#[cfg(test)]
fn previous_csv(path: impl AsRef<std::path::Path>) -> Previous {
    parse_previous(std::fs::File::open(path)?)
}

fn parse_previous(reader: impl std::io::Read) -> Previous {
    let mut prev = HashMap::new();
    for row in csv::Reader::from_reader(reader).deserialize::<Factor>() {
        operation::check()?;
        let f = row?;
        f.code.parse::<rustdx_complete::data::Symbol>()?;
        chrono::NaiveDate::parse_from_str(&f.date, "%Y-%m-%d")?;
        if !f.factor.is_finite() || f.factor <= 0.0 || !f.preclose.is_finite() || f.preclose <= 0.0
        {
            eyre::bail!("{}前因子或收盘价非法", f.code);
        }
        if prev.insert(f.code.clone(), f).is_some() {
            eyre::bail!("前因子文件包含重复代码");
        }
    }
    if prev.is_empty() {
        eyre::bail!("前因子数据为空");
    }
    Ok(prev)
}

fn clickhouse_factor_text(table: &str) -> Result<String> {
    super::database_table(table)?;
    let query = format!(
        "SELECT code, t.1 AS date, t.2 AS close, t.3 AS factor FROM \
        (SELECT code, argMax(tuple(date, close, factor), date) AS t FROM {table} FINAL GROUP BY code) \
        FORMAT CSVWithNames"
    );
    super::ClickHouseConfig::from_env().exec(&query)
}

fn save_factor_and_parse(text: &str, keep_factor: bool) -> Previous {
    let rows = parse_previous(text.as_bytes())?;
    if keep_factor {
        let (atomic, mut file) = rustdx_complete::atomic_file::AtomicFile::create("factor.csv")?;
        file.write_all(text.as_bytes())?;
        drop(file);
        atomic.commit()?;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn previous_inputs_are_preserved_and_invalid_rows_fail() {
        let dir = std::env::temp_dir().join(format!("rustdx-factor-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("factor.csv");
        fs::write(&path, "date,code,close,factor\n2026-09-01,sh600000,10,2\n").unwrap();
        assert_eq!(previous_csv(&path).unwrap()["sh600000"].factor, 2.0);
        assert!(path.exists());
        fs::write(
            &path,
            "date,code,close,factor\n2026-09-01,sh600000,invalid,2\n",
        )
        .unwrap();
        assert!(previous_csv(&path).is_err());
        assert!(path.exists());
        for text in [
            "",
            "date,code,close,factor\n",
            "date,code,close,factor\n2026-09-01,sh600000,10,NaN\n",
            "date,code,close,factor\n2026-09-01,sh600000,10,2\n2026-09-02,sh600000,11,2\n",
        ] {
            assert!(parse_previous(text.as_bytes()).is_err());
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
