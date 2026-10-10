use eyre::{Result, anyhow};
use std::collections::HashSet;

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
/// 数量如下：SHSZ { sh1: 1646, sh8: 407, sz: 2745, } -> 4798
///
/// 见 `tests-integration::fetch_code::offical_stocks` 测试
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
    use std::io::Read;
    let (url, ex) = (
        "http://www.szse.cn/api/report/ShowReport?\
        SHOWTYPE=xlsx&CATALOGID=1110&TABKEY=tab1&random=0.8587844061443386",
        "sz",
    );
    let bytes = &mut Vec::with_capacity(1 << 20);
    let mut resp = ureq::get(url).call()?;
    resp.body_mut().as_reader().read_to_end(bytes)?;
    let mut workbook = Xlsx::new(std::io::Cursor::new(bytes))?;
    // 每个单元格被解析的类型可能会不一样，所以把股票代码统一转化成字符型
    if let Some(Ok(range)) = workbook.worksheet_range_at(0) {
        let mut codes = StockList::new();
        for (i, row) in range.rows().enumerate().skip(1) {
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

fn cookie() -> String {
    "ba17301551dcbaf9_gdp_user_key=; \
     ba17301551dcbaf9_gdp_session_id=0876b773-38a3-44d0-bb4a-2b5569025b82; \
     gdp_user_id=gioenc-2g8894g6%2C764a%2C50d8%2C8d6g%2C3bg6194752ce; \
     ba17301551dcbaf9_gdp_session_id_0876b773-38a3-44d0-bb4a-2b5569025b82=true; \
     JSESSIONID=0311A5533F5FD798EE9DAFDE6A1D70A7; \
     ba17301551dcbaf9_gdp_sequence_ids=\
     {%22globalKey%22:14%2C%22VISIT%22:2%2C%22PAGE%22:5%2C%22VIEW_CHANGE%22:2%2C%22CUSTOM%22:3%2C%22VIEW_CLICK%22:6}"
        .to_string()
}

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 6.1; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.107 Safari/537.36";
const ACCEPT_LANGUAGE: &str =
    "zh-CN,zh;q=0.9,de;q=0.8,ko;q=0.7,ru;q=0.6,it;q=0.5,ga;q=0.4,en;q=0.3";

// 上交所 科创板 68 开头（目前 350 只，只需一次请求） => stockType=8, pagesize=400
//        A 股 60 开头（目前 1650 只，只需一次请求） => stockType=1, pagesize=1700
// 上交所 科创板 68 开头（目前 350 只，只需一次请求） => stockType=8, pagesize=400
//        A 股 60 开头（目前 1650 只，只需一次请求） => stockType=1, pagesize=1700
pub fn get_sh_stocks(set: &mut StockList, stocktype: &str, pagesize: &str) -> Result<usize> {
    let size: usize = pagesize.parse()?;
    if size == 0 || size > 5000 || !matches!(stocktype, "1" | "8") {
        eyre::bail!("上交所股票类型或分页大小非法");
    }
    let mut codes = StockList::new();
    let mut page = 1usize;
    let mut expected = None;
    loop {
        let url = format!(
            "http://query.sse.com.cn/sseQuery/commonQuery.do?\
        jsonCallBack=jsonpCallback37525685&STOCK_TYPE={stocktype}\
        &REG_PROVINCE=&CSRC_CODE=&STOCK_CODE=&sqlId=COMMON_SSE_CP_GPJCTPZ_GPLB_GP_L\
        &COMPANY_STATUS=2%2C4%2C5%2C7%2C8&type=inParams&isPagination=true\
        &pageHelp.cacheSize=1&pageHelp.beginPage={page}&pageHelp.pageSize={pagesize}\
        &pageHelp.pageNo={page}&pageHelp.endPage={page}"
        );
        let mut resp = ureq::get(&url)
            .header("ACCEPT", "*/*")
            .header("ACCEPT_LANGUAGE", ACCEPT_LANGUAGE)
            .header("CACHE_CONTROL", "no-cache")
            .header("COOKIE", cookie())
            .header("PRAGMA", "no-cache")
            .header("REFERER", "http://www.sse.com.cn/")
            .header("USER_AGENT", USER_AGENT)
            .call()?;
        let text = resp.body_mut().read_to_string()?;
        let (rows, total) = parse_sh_page(&text)?;
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
        .ok_or_else(|| anyhow!("上交所total字段缺失"))? as usize;
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
}
