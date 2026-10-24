//! Bounded CSV-to-RowBinary streaming preserves the exact exported Float64 bits.
use eyre::{Result, eyre};
use rustdx_complete::operation::CheckedIo;
use std::io::{self, BufRead, BufReader, Read};

// All supported fields are scalar dates, eight-byte symbols or finite numbers;
// none can contain a newline. Bound even malformed input before CSV allocation.
const MAX_CSV_RECORD: usize = 1024;

pub(super) struct RowBinary<R: Read> {
    source: BufReader<CheckedIo<R>>,
    fields: Vec<String>,
    line: Vec<u8>,
    record: csv::StringRecord,
    encoded: Vec<u8>,
    offset: usize,
    failed: bool,
}

impl<R: Read> RowBinary<R> {
    pub(super) fn new(source: R) -> Result<Self> {
        let mut reader = Self {
            source: BufReader::new(CheckedIo(source)),
            fields: Vec::new(),
            line: Vec::with_capacity(MAX_CSV_RECORD + 1),
            record: csv::StringRecord::new(),
            encoded: Vec::with_capacity(75),
            offset: 0,
            failed: false,
        };
        if !reader.read_record()? {
            eyre::bail!("RowBinary输入CSV缺少列定义");
        }
        reader.fields = super::verify::columns(&reader.record)?;
        Ok(reader)
    }

    pub(super) fn columns(&self) -> String {
        self.fields.join(",")
    }

    fn read_record(&mut self) -> Result<bool> {
        self.line.clear();
        let bytes = self
            .source
            .by_ref()
            .take((MAX_CSV_RECORD + 1) as u64)
            .read_until(b'\n', &mut self.line)?;
        if bytes == 0 {
            return Ok(false);
        }
        if bytes > MAX_CSV_RECORD {
            eyre::bail!("RowBinary输入CSV行超过{MAX_CSV_RECORD}字节");
        }
        let mut parser = csv::ReaderBuilder::new()
            .has_headers(false)
            .buffer_capacity(MAX_CSV_RECORD + 1)
            .from_reader(self.line.as_slice());
        if !parser.read_record(&mut self.record)? {
            eyre::bail!("RowBinary输入CSV包含空行");
        }
        Ok(true)
    }

    fn encode_record(&mut self) -> Result<bool> {
        self.encoded.clear();
        self.offset = 0;
        if !self.read_record()? {
            return Ok(false);
        }
        if self.record.len() != self.fields.len() {
            eyre::bail!("RowBinary输入CSV行与列定义长度不一致");
        }
        for (field, text) in self.fields.iter().zip(self.record.iter()) {
            match field.as_str() {
                "date" => {
                    let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")?;
                    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
                        .expect("valid ClickHouse Date epoch");
                    let days = u16::try_from(date.signed_duration_since(epoch).num_days())
                        .map_err(|_| eyre!("日期{text}超出ClickHouse Date的UInt16范围"))?;
                    self.encoded.extend(days.to_le_bytes());
                }
                "code" => {
                    text.parse::<rustdx_complete::data::Symbol>()?;
                    // ClickHouse String uses a VarUInt byte length followed by UTF-8.
                    // Validated symbols are exactly eight ASCII bytes: one length byte.
                    self.encoded.push(8);
                    self.encoded.extend(text.as_bytes());
                }
                _ => {
                    let number = text.parse::<f64>()?;
                    if !number.is_finite() {
                        eyre::bail!("{field}不是有限数值");
                    }
                    self.encoded.extend(number.to_le_bytes());
                }
            }
        }
        Ok(true)
    }
}

impl<R: Read> Read for RowBinary<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.failed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "RowBinary输入CSV已失败",
            ));
        }
        if self.offset == self.encoded.len() {
            match self.encode_record() {
                Ok(true) => {}
                Ok(false) => return Ok(0),
                Err(error) => {
                    self.failed = true;
                    self.encoded.clear();
                    return Err(io::Error::other(error.to_string()));
                }
            }
        }
        let count = buffer.len().min(self.encoded.len() - self.offset);
        buffer[..count].copy_from_slice(&self.encoded[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "date,code,open,high,low,close,amount,vol\n";
    const ROW: &str = "2005-01-13,sh600000,6.95,7.02,6.94,6.96,8976785,12848.27\n";
    // Independently packed Python <H, VarUInt+ASCII, 6d; verified against CH25.8.
    const HEX: &str = "fc31087368363030303030cdcccccccccc1b4014ae47e17a141c40c3f5285c8fc21b40d7a3703d0ad71b4000000020321f6141f6285c8f2218c940";

    fn expected() -> Vec<u8> {
        (0..HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&HEX[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn independent_bytes_and_small_reads_preserve_all_float_bits() {
        for size in [1, 2, 7, 128] {
            let input = format!("{HEADER}{ROW}{ROW}");
            let mut reader = RowBinary::new(input.as_bytes()).unwrap();
            assert_eq!(reader.columns(), "date,code,open,high,low,close,amount,vol");
            assert_eq!(reader.read(&mut []).unwrap(), 0);
            let mut bytes = Vec::new();
            let mut buffer = vec![0; size];
            loop {
                let n = reader.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            assert_eq!(bytes, expected().repeat(2));
            assert_eq!(reader.read(&mut buffer).unwrap(), 0);
        }
    }

    #[test]
    fn adjusted_fields_reordering_and_signed_zero_are_preserved() {
        let input = "code,date,factor,preclose,vol,amount,close,low,high,open\r\n\
                     \"sz000001\",1970-01-01,0.1,-0,2,3,4,5,6,7";
        let mut reader = RowBinary::new(input.as_bytes()).unwrap();
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        let mut expected = b"\x08sz000001\x00\x00".to_vec();
        for number in [0.1f64, -0.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            expected.extend(number.to_le_bytes());
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn malformed_fields_and_unbounded_records_fail_before_encoding() {
        for header in [
            "",
            "date,code\n",
            "date,code,open,high,low,close,amount,vol,version\n",
            "date,code,open,high,low,close,amount,vol,vol\n",
        ] {
            assert!(RowBinary::new(header.as_bytes()).is_err());
        }
        for row in [
            "\n".to_owned(),
            ROW.replace("2005-01-13", "1969-12-31"),
            ROW.replace("2005-01-13", "2150-01-01"),
            ROW.replace("2005-01-13", "2005-02-30"),
            ROW.replace("sh600000", "600000"),
            ROW.replace("6.94", "NaN"),
            ROW.replace("6.94", "inf"),
            ROW.replace("6.94", "oops"),
            ROW.replace(",12848.27", ""),
            ROW.replace("12848.27", "12848.27,1"),
            "1".repeat(MAX_CSV_RECORD + 1),
        ] {
            let input = format!("{HEADER}{row}");
            let mut reader = RowBinary::new(input.as_bytes()).unwrap();
            let mut bytes = Vec::new();
            assert!(reader.read_to_end(&mut bytes).is_err(), "{row}");
            assert!(bytes.is_empty());
            assert!(reader.read(&mut [0]).is_err());
        }
    }

    #[test]
    fn date_upper_bound_and_cancellation_are_enforced() {
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let last = epoch + chrono::Duration::days(u16::MAX.into());
        let input = format!("{HEADER}{}", ROW.replace("2005-01-13", &last.to_string()));
        let mut reader = RowBinary::new(input.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(&bytes[..2], &u16::MAX.to_le_bytes());

        let task = rustdx_complete::operation::OperationContext::new(
            std::time::Duration::from_secs(1),
            None,
        )
        .unwrap();
        let _guard = task.enter();
        task.cancel();
        assert!(RowBinary::new(input.as_bytes()).is_err());
    }
}
