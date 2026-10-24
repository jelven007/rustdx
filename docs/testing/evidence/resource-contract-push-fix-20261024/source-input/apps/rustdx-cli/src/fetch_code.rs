use crate::http::{self, HttpOperation};
use eyre::{Result, anyhow};
use rustdx_complete::operation::{self, CheckedIo};
use std::collections::HashSet;
use std::io::Read;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    CheckedIo(reader)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        eyre::bail!("股票名单响应超出{}字节限制", limit);
    }
    Ok(bytes)
}

// 股票上限
const SH8: &str = "800";
const SH1: &str = "2000";

pub type StockList = HashSet<String>;

#[derive(Debug)]
pub struct SHSZ {
    sh1: usize,
    sh8: usize,
    sz: usize,
}

impl SHSZ {
    /// 计算总和
    pub fn count(&self) -> usize {
        let SHSZ { sh1, sh8, sz } = &self;
        sh1 + sh8 + sz
    }
}

/// 获取上证和深证的股票代码
///
/// 实际数量通过返回值SHSZ提供，不能使用历史快照数量判断完整性。
pub fn offical_stocks(set: &mut StockList) -> Result<SHSZ> {
    let count = SHSZ {
        sh1: get_sh_stocks(set, "1", SH1)?,
        sh8: get_sh_stocks(set, "8", SH8)?,
        sz: get_sz_stocks(set)?,
    };
    info!("股票数量 {count:?}");
    Ok(count)
}

pub fn get_offical_stocks(cond: &str) -> Result<StockList> {
    let mut set = StockList::with_capacity(6000);
    let len = match cond {
        "official" => offical_stocks(&mut set)?.count(),
        "szse" => get_sz_stocks(&mut set)?,
        "sse" => get_sh_stocks(&mut set, "8", SH8)? + get_sh_stocks(&mut set, "1", SH1)?,
        _ => {
            eyre::bail!("请输入 official | szse | sse 中的一个");
        }
    };

    info!("获得上证和深证股票数量：{len}");
    Ok(set)
}

/// 深交所官网的 A 股和创业板股票信息。
pub fn get_sz_stocks(set: &mut StockList) -> Result<usize> {
    use calamine::{Data, Reader, Xlsx};
    let (url, ex) = (
        "https://www.szse.cn/api/report/ShowReport?\
        SHOWTYPE=xlsx&CATALOGID=1110&TABKEY=tab1",
        "sz",
    );
    let mut operation = HttpOperation::new(LIST_TIMEOUT)?;
    let mut resp = http::get(url)
        .config()
        .timeout_global(Some(operation.timeout()))
        .build()
        .call()?;
    let bytes = read_bounded(resp.body_mut().as_reader(), MAX_RESPONSE_BYTES)?;
    operation.complete(bytes.is_empty())?;
    let mut workbook = Xlsx::new(std::io::Cursor::new(bytes))?;
    // 每个单元格被解析的类型可能会不一样，所以把股票代码统一转化成字符型
    if let Some(Ok(range)) = workbook.worksheet_range_at(0) {
        let mut codes = StockList::new();
        for (i, row) in range.rows().enumerate().skip(1) {
            operation::check()?;
            let code = match row.get(4) {
                None | Some(Data::Empty) => continue,
                Some(Data::Int(x)) if (0..=999999).contains(x) => format!("{x:06}"),
                Some(Data::Float(x))
                    if x.is_finite() && x.fract() == 0.0 && (0.0..=999999.0).contains(x) =>
                {
                    format!("{:06}", *x as u32)
                }
                Some(Data::String(x))
                    if x.trim().len() <= 6 && x.trim().bytes().all(|b| b.is_ascii_digit()) =>
                {
                    format!("{:0>6}", x.trim())
                }
                _ => {
                    eyre::bail!("深交所Excel第{}行证券代码非法", i + 1);
                }
            };
            let full = format!("{ex}{code}");
            full.parse::<rustdx_complete::data::Symbol>()?;
            if !codes.insert(full) {
                eyre::bail!("深交所股票名单包含重复证券");
            }
        }
        if codes.is_empty() {
            eyre::bail!("深交所股票名单为空");
        }
        let count = codes.len();
        set.extend(codes);
        Ok(count)
    } else {
        Err(anyhow!("xlsx parse error"))
    }
}

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 6.1; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.107 Safari/537.36";
const ACCEPT_LANGUAGE: &str =
    "zh-CN,zh;q=0.9,de;q=0.8,ko;q=0.7,ru;q=0.6,it;q=0.5,ga;q=0.4,en;q=0.3";

// stockType=8为科创板，stockType=1为主板；以响应total驱动翻页。
pub fn get_sh_stocks(set: &mut StockList, stocktype: &str, pagesize: &str) -> Result<usize> {
    let size: usize = pagesize.parse()?;
    if size == 0 || size > 5000 || !matches!(stocktype, "1" | "8") {
        eyre::bail!("上交所股票类型或分页大小非法");
    }
    let mut codes = StockList::new();
    let mut page = 1usize;
    let mut expected = None;
    let deadline = Instant::now() + LIST_TIMEOUT;
    loop {
        operation::check()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            eyre::bail!("上交所股票名单分页超过总时限");
        }
        let url = format!(
            "https://query.sse.com.cn/sseQuery/commonQuery.do?\
        jsonCallBack=jsonpCallback37525685&STOCK_TYPE={stocktype}\
        &REG_PROVINCE=&CSRC_CODE=&STOCK_CODE=&sqlId=COMMON_SSE_CP_GPJCTPZ_GPLB_GP_L\
        &COMPANY_STATUS=2%2C4%2C5%2C7%2C8&type=inParams&isPagination=true\
        &pageHelp.cacheSize=1&pageHelp.beginPage={page}&pageHelp.pageSize={pagesize}\
        &pageHelp.pageNo={page}&pageHelp.endPage={page}"
        );
        let mut operation = HttpOperation::new(remaining)?;
        let mut resp = http::get(&url)
            .config()
            .timeout_global(Some(operation.timeout()))
            .build()
            .header("ACCEPT", "*/*")
            .header("ACCEPT_LANGUAGE", ACCEPT_LANGUAGE)
            .header("CACHE_CONTROL", "no-cache")
            .header("PRAGMA", "no-cache")
            .header("REFERER", "https://www.sse.com.cn/")
            .header("USER_AGENT", USER_AGENT)
            .call()?;
        let text = String::from_utf8(read_bounded(
            resp.body_mut().as_reader(),
            MAX_RESPONSE_BYTES,
        )?)?;
        let (rows, total) = parse_sh_page(&text)?;
        operation.complete(rows.is_empty())?;
        if expected.is_some_and(|n| n != total) {
            eyre::bail!("上交所分页total发生变化");
        }
        expected = Some(total);
        for row in rows {
            if !codes.insert(row) {
                eyre::bail!("上交所分页包含重复证券");
            }
        }
        if codes.len() == total {
            break;
        }
        if codes.len() > total || page > total.div_ceil(size).max(1) {
            eyre::bail!("上交所分页数量不完整或无进展");
        }
        page += 1;
    }
    let count = codes.len();
    set.extend(codes);
    Ok(count)
}

fn parse_sh_page(text: &str) -> Result<(Vec<String>, usize)> {
    let text = text.trim();
    let json = if text.starts_with('{') {
        text
    } else {
        let start = text
            .find('(')
            .ok_or_else(|| anyhow!("上交所JSONP缺少开始括号"))?;
        let end = text
            .rfind(')')
            .filter(|end| *end > start)
            .ok_or_else(|| anyhow!("上交所JSONP缺少结束括号"))?;
        &text[start + 1..end]
    };
    let value: serde_json::Value = serde_json::from_str(json)?;
    let total = value
        .pointer("/pageHelp/total")
        .or_else(|| value.get("total"))
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or_else(|| anyhow!("上交所total字段缺失"))?;
    let total = usize::try_from(total)?;
    if total > 100_000 {
        eyre::bail!("上交所股票名单total超出100000限制");
    }
    let rows = value
        .get("result")
        .and_then(|v| v.as_array())
        .or_else(|| value.pointer("/pageHelp/data").and_then(|v| v.as_array()))
        .ok_or_else(|| anyhow!("上交所证券数组缺失"))?;
    if rows.is_empty() && total > 0 {
        eyre::bail!("上交所返回空页");
    }
    let mut codes = Vec::with_capacity(rows.len());
    for row in rows {
        let code = row
            .get("COMPANY_CODE")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("上交所COMPANY_CODE缺失"))?;
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            eyre::bail!("上交所证券代码非法");
        }
        codes.push(format!("sh{code}"));
    }
    Ok((codes, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sse_jsonp_is_structural_and_public_inputs_do_not_panic() {
        let (rows, count) =
            parse_sh_page(r#"cb({"pageHelp":{"total":1},"result":[{"COMPANY_CODE":"600000"}]});"#)
                .unwrap();
        assert_eq!(rows, vec!["sh600000"]);
        assert_eq!(count, 1);
        for text in ["", "abc", "cb()", r#"{"total":1,"result":[]}"#] {
            assert!(parse_sh_page(text).is_err());
        }
        assert!(get_offical_stocks("unknown").is_err());
    }

    #[test]
    fn stocklist_responses_are_bounded_and_read_errors_propagate() {
        assert_eq!(read_bounded(b"abcd".as_slice(), 4).unwrap(), b"abcd");
        assert!(read_bounded(b"abcde".as_slice(), 4).is_err());
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "truncated",
                ))
            }
        }
        assert!(read_bounded(Broken, 4).is_err());
    }
}
