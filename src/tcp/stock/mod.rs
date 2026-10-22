mod kline;
pub use kline::{Kline, KlineData};

mod index_kline;
pub use index_kline::{IndexKline, IndexKlineData};

mod kline_request;
pub use kline_request::{KlineRequest, decode_index_bars, decode_stock_bars};

mod xdxr;
pub use xdxr::*;

mod quotes;
pub use quotes::{QuoteData, SecurityQuotes};

mod security_list;
pub use security_list::{SecurityList, SecurityListData};

mod minute_time;
pub use minute_time::{MinuteTime, MinuteTimeData};

mod transaction;
pub use transaction::{Transaction, TransactionData};

mod finance_info;
pub use finance_info::{FinanceInfo, FinanceInfoData};

mod history_minute_time;
pub use history_minute_time::HistoryMinuteTime;

mod history_transaction;
pub use history_transaction::HistoryTransaction;

mod block_info;
pub use block_info::{
    BlockInfoChunk, BlockInfoMeta, BlockRecord, get_block_info, parse_block_content,
};

mod company_info;
pub use company_info::{CompanyInfoCategory, CompanyInfoCategoryItem, CompanyInfoContent};

mod client;
pub use client::{Adj, BatchBars, BatchKline, Client, ex_right_reference};

/// 根据六位股票代码推断市场代码。
///
/// - `6` 开头 → 沪市（`1`），如 600xxx、688xxx
/// - `0` / `3` 开头 → 深市（`0`），如 000xxx、300xxx
/// - 其他（4/8 开头的北交所等）暂不支持，返回 `None`
///
/// ## 示例
/// ```
/// use rustdx_complete::tcp::stock::market_of;
///
/// assert_eq!(market_of("600519"), Some(1));
/// assert_eq!(market_of("300750"), Some(0));
/// assert_eq!(market_of("830000"), None);
/// ```
pub fn market_of(code: &str) -> Option<u16> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match code.as_bytes().first()? {
        b'6' => Some(1),
        b'0' | b'3' => Some(0),
        _ => None,
    }
}

/// 获取市场全部证券列表（自动按 1000 条分页聚合）。
///
/// 数量不一致、重复代码或未达到总数就返回空页时，返回错误。
///
/// ## 示例
/// ```ignore
/// use rustdx_complete::tcp::{Tcp, stock::stocks};
///
/// let mut tcp = Tcp::new()?;
/// let all = stocks(&mut tcp, 0)?; // 深市全部证券
/// println!("深市共 {} 只证券", all.len());
/// ```
pub fn stocks(tcp: &mut crate::tcp::Tcp, market: u16) -> crate::Result<Vec<SecurityListData>> {
    Ok(
        tcp.with_timeout(crate::tcp::network_policy().pagination_timeout, |tcp| {
            stocks_until(tcp, market)
        })?,
    )
}

fn stocks_until(tcp: &mut crate::tcp::Tcp, market: u16) -> std::io::Result<Vec<SecurityListData>> {
    use crate::tcp::Tdx;
    use std::io::{Error, ErrorKind};
    if market > 1 {
        return Err(Error::new(ErrorKind::InvalidInput, "market必须为0或1"));
    }
    let count = {
        let mut sc = crate::tcp::basic::SecurityCount::new(market);
        *sc.recv_parsed(tcp)?
    };
    let mut all = Vec::with_capacity(count as usize);
    let mut codes = std::collections::HashSet::with_capacity(count as usize);
    while all.len() < count as usize {
        let mut list = SecurityList::new(market, all.len() as u16);
        list.recv_parsed(tcp)?;
        let n = list.result().len();
        if n == 0 {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                format!("证券列表未完成: 已获取{}，预期{count}", all.len()),
            ));
        }
        if n > 1000 || all.len() + n > count as usize {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "证券列表数量超出预期，请重新获取",
            ));
        }
        for stock in list.result() {
            if !codes.insert(stock.code.clone()) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("证券列表代码重复: {}", stock.code),
                ));
            }
        }
        all.extend(list.result().iter().cloned());
    }
    Ok(all)
}

mod industry_mapping;
#[allow(deprecated)]
pub use industry_mapping::{get_industry_info, get_industry_name, get_province_name};

mod concept_mapping;
#[allow(deprecated)]
pub use concept_mapping::{ConceptStock, get_concept_info, get_concept_names, get_concept_stocks};

// 数据验证模块
pub mod validator;
pub use validator::{
    DataLocation, Validatable, ValidationLevel, ValidationResult, detect_anomalies,
    validate_finance_consistency, validate_kline_continuity,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn page(codes: &[&str]) -> Vec<u8> {
        let mut bytes = (codes.len() as u16).to_le_bytes().to_vec();
        for code in codes {
            let mut record = [0u8; 29];
            record[..6].copy_from_slice(code.as_bytes());
            bytes.extend_from_slice(&record);
        }
        bytes
    }

    #[test]
    fn stocks_rejects_incomplete_duplicate_and_excess_pages() {
        for last in [page(&[]), page(&["000001"]), page(&["000002", "000003"])] {
            let (config, server) = crate::tcp::tests::mock_server(1, move |req| {
                if req[10] == 0x0d {
                    vec![0, 0]
                } else if req[10] == 0x4e {
                    vec![2, 0]
                } else if req[14..16] == [0, 0] {
                    page(&["000001"])
                } else {
                    last.clone()
                }
            });
            let mut tcp = crate::tcp::Tcp::with_config(&config).unwrap();
            assert!(stocks(&mut tcp, 0).is_err());
            drop(tcp);
            server.join().unwrap();
        }
    }

    #[test]
    fn stocks_collects_pages_in_order() {
        let (config, server) = crate::tcp::tests::mock_server(1, |req| {
            if req[10] == 0x0d {
                vec![0, 0]
            } else if req[10] == 0x4e {
                vec![2, 0]
            } else if req[14..16] == [0, 0] {
                page(&["000001"])
            } else {
                page(&["000002"])
            }
        });
        let mut tcp = crate::tcp::Tcp::with_config(&config).unwrap();
        let rows = stocks(&mut tcp, 0).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(),
            ["000001", "000002"]
        );
        assert!(stocks(&mut tcp, 2).is_err());
        drop(tcp);
        server.join().unwrap();
        for invalid in ["6", "60000x", "0000010", ""] {
            assert_eq!(market_of(invalid), None);
        }
    }
}
