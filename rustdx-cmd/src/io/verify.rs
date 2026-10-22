//! Independent, bounded readback of every exported key and numeric field.
use crate::quality::DatabaseEvidence;
use eyre::{Result, eyre};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, time::Instant};

const DEFAULT_BATCH_ROWS: usize = 4096;
pub(super) const MAX_BATCH_ROWS: usize = 16384;

#[derive(Clone, Copy)]
pub(super) struct BatchRows(usize);

impl BatchRows {
    fn new(rows: usize) -> Result<Self> {
        if !(1..=MAX_BATCH_ROWS).contains(&rows) {
            eyre::bail!("RUSTDX_VERIFY_BATCH_ROWS必须在1–{MAX_BATCH_ROWS}之间");
        }
        Ok(Self(rows))
    }

    pub(super) fn from_env() -> Result<Self> {
        let rows = match std::env::var("RUSTDX_VERIFY_BATCH_ROWS") {
            Ok(value) => value.parse()?,
            Err(std::env::VarError::NotPresent) => DEFAULT_BATCH_ROWS,
            Err(error) => return Err(error.into()),
        };
        Self::new(rows)
    }
}
const NUMERIC_FIELDS: [&str; 8] = [
    "open", "high", "low", "close", "amount", "vol", "preclose", "factor",
];
pub(super) type Key = (String, String);

#[derive(Debug, PartialEq)]
pub(super) struct Row {
    pub key: Key,
    pub values: Vec<f64>,
}

pub(super) fn columns(headers: &csv::StringRecord) -> Result<Vec<String>> {
    let fields = headers.iter().map(str::to_owned).collect::<Vec<_>>();
    if !fields.iter().any(|f| f == "date") || !fields.iter().any(|f| f == "code") {
        eyre::bail!("数据库验收CSV缺少date或code");
    }
    let unique = fields.iter().collect::<std::collections::HashSet<_>>();
    if unique.len() != fields.len()
        || fields
            .iter()
            .any(|f| f != "date" && f != "code" && !NUMERIC_FIELDS.contains(&f.as_str()))
    {
        eyre::bail!("数据库验收CSV包含重复或不支持的字段");
    }
    for required in &NUMERIC_FIELDS[..6] {
        if !fields.iter().any(|f| f == required) {
            eyre::bail!("数据库验收CSV缺少{required}");
        }
    }
    Ok(fields)
}

fn csv_row(fields: &[String], record: &csv::StringRecord) -> Result<Row> {
    let get = |name: &str| -> Result<&str> {
        let i = fields
            .iter()
            .position(|f| f == name)
            .ok_or_else(|| eyre!("缺少{name}"))?;
        record.get(i).ok_or_else(|| eyre!("字段{name}缺失"))
    };
    let key = (get("date")?.to_owned(), get("code")?.to_owned());
    chrono::NaiveDate::parse_from_str(&key.0, "%Y-%m-%d")?;
    key.1.parse::<rustdx_complete::data::Symbol>()?;
    let values = fields
        .iter()
        .filter(|f| NUMERIC_FIELDS.contains(&f.as_str()))
        .map(|f| {
            let value = get(f)?.parse::<f64>()?;
            if !value.is_finite() {
                eyre::bail!("{f}不是有限数值");
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Row { key, values })
}

pub(super) fn csv_rows(fields: &[String], text: &str) -> Result<Vec<Row>> {
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    if reader
        .headers()?
        .iter()
        .ne(fields.iter().map(String::as_str))
    {
        eyre::bail!("ClickHouse回读列与输出列不一致");
    }
    reader.records().map(|r| csv_row(fields, &r?)).collect()
}

pub(super) fn json_rows(
    fields: &[String],
    text: &str,
    limit: usize,
    sequence: u64,
) -> Result<Vec<Row>> {
    let response: serde_json::Value = serde_json::from_str(text)?;
    if response["id"].as_u64() != Some(sequence) {
        eyre::bail!("MongoDB回读响应序号不匹配");
    }
    let values = response["rows"]
        .as_array()
        .ok_or_else(|| eyre!("MongoDB回读缺少rows数组"))?;
    if values.len() > limit {
        eyre::bail!("MongoDB返回过多记录");
    }
    let mut rows = Vec::new();
    for value in values {
        let string = |name: &str| {
            value[name]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| eyre!("MongoDB {name}类型错误"))
        };
        let mut values = Vec::new();
        for f in fields
            .iter()
            .filter(|f| NUMERIC_FIELDS.contains(&f.as_str()))
        {
            let number = value[f]
                .as_f64()
                .ok_or_else(|| eyre!("MongoDB {f}不是数值"))?;
            if !number.is_finite() {
                eyre::bail!("MongoDB {f}不是有限数值");
            }
            values.push(number);
        }
        rows.push(Row {
            key: (string("date")?, string("code")?),
            values,
        });
    }
    Ok(rows)
}

/// Memory is O(batch_rows), independently of total CSV size. Existing unrelated
/// database keys are deliberately outside the scope of this import's acceptance.
pub(super) fn verify(
    path: &Path,
    target: &str,
    batch: BatchRows,
    mut fetch: impl FnMut(&[String], &[Key]) -> Result<Vec<Row>>,
) -> Result<DatabaseEvidence> {
    let start = Instant::now();
    let mut reader = csv::Reader::from_path(path)?;
    let fields = columns(reader.headers()?)?;
    let numeric_fields = fields
        .iter()
        .filter(|f| NUMERIC_FIELDS.contains(&f.as_str()))
        .cloned()
        .collect();
    let mut evidence = DatabaseEvidence {
        target: target.to_owned(),
        method: "all_exported_keys_and_numeric_fields",
        scope: "exported_keys",
        rows: 0,
        logical_keys: 0,
        batches: 0,
        batch_rows: batch.0,
        fetch_elapsed_ms: 0.0,
        elapsed_ms: 0.0,
        sha256_order: "csv_row_order",
        first_date: None,
        last_date: None,
        numeric_fields,
        sha256: String::new(),
    };
    let mut digest = Sha256::new();
    let mut records = reader.records();
    loop {
        rustdx_complete::operation::check()?;
        let mut expected = Vec::with_capacity(batch.0);
        let mut unique = std::collections::HashSet::with_capacity(batch.0);
        for record in records.by_ref().take(batch.0) {
            let row = csv_row(&fields, &record?)?;
            if !unique.insert(row.key.clone()) {
                eyre::bail!("验收批次包含重复CSV键");
            }
            expected.push(row);
        }
        if expected.is_empty() {
            break;
        }
        let keys = expected
            .iter()
            .map(|row| row.key.clone())
            .collect::<Vec<_>>();
        let fetch_start = Instant::now();
        let actual = fetch(&fields, &keys)?;
        evidence.fetch_elapsed_ms += fetch_start.elapsed().as_secs_f64() * 1000.0;
        if actual.len() != expected.len() {
            eyre::bail!(
                "数据库回读行数不一致: 期望{}，实际{}；CSV保留待核实",
                expected.len(),
                actual.len()
            );
        }
        let mut received = BTreeMap::new();
        for row in actual {
            if received.insert(row.key.clone(), row).is_some() {
                eyre::bail!("数据库包含重复(date,code)，CSV保留待核实");
            }
        }
        for row in expected {
            let key = &row.key;
            let actual = received
                .remove(key)
                .ok_or_else(|| eyre!("数据库缺少键{key:?}"))?;
            if actual != row {
                eyre::bail!("数据库数值与CSV不一致: {key:?}；CSV保留待核实");
            }
            // Digest describes independently read back, canonicalized values,
            // not the original CSV bytes. Both input and readback use f64.
            // CSV row order makes the digest independent of batch size.
            digest.update(serde_json::to_vec(&(key, &actual.values))?);
            digest.update(b"\n");
            if evidence.first_date.as_ref().is_none_or(|d| &key.0 < d) {
                evidence.first_date = Some(key.0.clone());
            }
            if evidence.last_date.as_ref().is_none_or(|d| &key.0 > d) {
                evidence.last_date = Some(key.0.clone());
            }
            evidence.rows += 1;
        }
        evidence.batches += 1;
    }
    if evidence.rows == 0 {
        eyre::bail!("数据库验收输入为空");
    }
    evidence.logical_keys = evidence.rows;
    evidence.sha256 = format!("{:x}", digest.finalize());
    evidence.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_readback_detects_values_keys_duplicates_and_crosses_batches() {
        let path = std::env::temp_dir().join(format!("rustdx-verify-{}.csv", std::process::id()));
        let mut writer = csv::Writer::from_path(&path).unwrap();
        writer
            .write_record([
                "date", "code", "open", "high", "low", "close", "amount", "vol",
            ])
            .unwrap();
        for i in 0..513 {
            writer
                .write_record([
                    "2026-10-09",
                    &format!("sh{:06}", 600512 - i),
                    "10",
                    "11",
                    "9",
                    "10.5",
                    "1000",
                    "100",
                ])
                .unwrap();
        }
        writer.flush().unwrap();
        let fetch = |_: &[String], keys: &[Key]| -> Result<Vec<Row>> {
            Ok(keys
                .iter()
                .rev()
                .map(|key| Row {
                    key: key.clone(),
                    values: vec![10., 11., 9., 10.5, 1000., 100.],
                })
                .collect())
        };
        let evidence = verify(&path, "test", BatchRows::new(512).unwrap(), fetch).unwrap();
        assert_eq!((evidence.rows, evidence.batches), (513, 2));
        for batch in [1, 11, DEFAULT_BATCH_ROWS, MAX_BATCH_ROWS] {
            let other = verify(&path, "test", BatchRows::new(batch).unwrap(), fetch).unwrap();
            assert_eq!(other.sha256, evidence.sha256);
            assert_eq!(other.batches, 513usize.div_ceil(batch));
        }
        for mutation in 0..4 {
            assert!(
                verify(
                    &path,
                    "test",
                    BatchRows::new(512).unwrap(),
                    |fields, keys| {
                        let mut rows = fetch(fields, keys)?;
                        match mutation {
                            0 => {
                                rows[0].values[4] += 1.;
                            }
                            1 => {
                                rows.pop();
                            }
                            2 => {
                                rows[0].key.1 = "sh699999".into();
                            }
                            _ => {
                                rows[0].key = rows[1].key.clone();
                            }
                        }
                        Ok(rows)
                    }
                )
                .is_err()
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn batch_and_mongo_response_limits_reject_invalid_input() {
        assert!(BatchRows::new(0).is_err());
        assert!(BatchRows::new(MAX_BATCH_ROWS + 1).is_err());
        let fields = vec!["date".into(), "code".into(), "open".into()];
        let valid = r#"{"id":1,"rows":[{"date":"2026-10-09","code":"sh600000","open":10}]}"#;
        assert_eq!(json_rows(&fields, valid, 1, 1).unwrap().len(), 1);
        assert!(json_rows(&fields, valid, 1, 2).is_err());
        assert!(json_rows(&fields, valid, 0, 1).is_err());
        for text in [
            r#"{"id":1}"#,
            r#"{"id":1,"rows":[{}]}"#,
            r#"{"id":1,"rows":[{"date":3,"code":"sh600000","open":10}]}"#,
            r#"{"id":1,"rows":[{"date":"2026-10-09","code":"sh600000","open":"10"}]}"#,
        ] {
            assert!(json_rows(&fields, text, 1, 1).is_err());
        }
    }
}
