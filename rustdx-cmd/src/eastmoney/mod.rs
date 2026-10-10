use eyre::{Result, WrapErr};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    sync::OnceLock,
    time::Duration,
};
use ureq::{
    Agent, Error,
    config::Config,
    unversioned::{
        resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
        transport::DefaultConnector,
    },
};

#[derive(Debug)]
struct EastmoneyResolver {
    offset: usize,
}

impl Resolver for EastmoneyResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> std::result::Result<ResolvedSocketAddrs, Error> {
        let system = DefaultResolver::default()
            .resolve(uri, config, timeout)
            .unwrap_or_else(|_| self.empty());
        let mut addresses = self.empty();
        let port = uri.port_u16().unwrap_or(443);
        let configured = std::env::var("RUSTDX_EASTMONEY_IPS").unwrap_or_default();
        let mut fallbacks: Vec<_> = configured
            .split(',')
            .filter(|value| !value.trim().is_empty())
            .chain([
                "120.79.191.232",
                "119.3.232.150",
                "120.76.218.228",
                "14.103.191.91",
                "47.112.165.11",
            ])
            .filter_map(|value| value.trim().parse::<IpAddr>().ok())
            .collect();
        if !fallbacks.is_empty() {
            let len = fallbacks.len();
            fallbacks.rotate_left(self.offset % len);
        }
        for ip in fallbacks {
            let address = SocketAddr::new(ip, port);
            if addresses.len() < 16 && !addresses.contains(&address) {
                addresses.push(address);
            }
        }
        for address in system.iter().copied() {
            if addresses.len() < 16 && !addresses.contains(&address) {
                addresses.push(address);
            }
        }
        if addresses.is_empty() {
            Err(Error::HostNotFound)
        } else {
            Ok(addresses)
        }
    }
}

fn eastmoney_agents() -> &'static [Agent; 5] {
    static AGENTS: OnceLock<[Agent; 5]> = OnceLock::new();
    AGENTS.get_or_init(|| {
        std::array::from_fn(|offset| {
            let config = Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(15)))
                .build();
            Agent::with_parts(
                config,
                DefaultConnector::default(),
                EastmoneyResolver { offset },
            )
        })
    })
}

/// 获取股票数据
///
/// 东财接口偶发网络抖动（Unexpected EOF 等），依次尝试多个 HTTPS 节点。
pub fn get(page_size: u16, page_number: u16) -> Result<String> {
    if page_size == 0 || page_number == 0 {
        eyre::bail!("page_size和page_number必须大于0");
    }
    const HOST: &str = "push2.eastmoney.com";
    const MAX_RETRIES: usize = 1;
    let mut last_err = None;
    for node_offset in 0..eastmoney_agents().len() {
        let node = (page_number as usize - 1 + node_offset) % eastmoney_agents().len();
        let url = format!(
            "https://{HOST}/api/qt/clist/get?cb=jQuery112407375845698232317_1631693257414&\
            pn={page_number}&pz={page_size}&po=0&np=1&ut=bd1d9ddb04089700cf9c27f6f7426281&fltt=2&\
            invt=2&fid=f12&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23&fields=f18,f16,f12,f13,f124,f17,f15,f2,f6,f5&_=1631693257534"
        );
        for attempt in 1..=MAX_RETRIES {
            info!("Get: {url}");
            match eastmoney_agents()[node]
                .get(&url)
                .header("Accept", "application/json,text/plain,*/*")
                .header("Referer", "https://quote.eastmoney.com/")
                .header("User-Agent", "Mozilla/5.0 rustdx-cli")
                .call()
            {
                Ok(mut resp) => return Ok(resp.body_mut().read_to_string()?),
                Err(e) => {
                    warn!("东财节点#{node}请求失败（第 {attempt}/{MAX_RETRIES} 次）：{e}");
                    last_err = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(500 * attempt as u64));
                }
            }
        }
    }
    Err(eyre::eyre!(last_err.unwrap())).wrap_err_with(|| {
        format!(
            "获取东财股票数据失败（{}个HTTPS节点各重试{MAX_RETRIES}次）",
            eastmoney_agents().len()
        )
    })
}

pub fn parse(text: &str) -> Result<EastMarket> {
    let text = text.trim();
    let json = if text.starts_with('{') {
        text
    } else {
        let begin = text
            .find('(')
            .ok_or_else(|| eyre::eyre!("东财JSONP缺少开始括号"))?;
        let end = text
            .rfind(')')
            .filter(|end| *end > begin)
            .ok_or_else(|| eyre::eyre!("东财JSONP缺少结束括号"))?;
        &text[begin + 1..end]
    };
    serde_json::from_str(json).wrap_err("解析东财股票数据失败")
}

/// 获取并解析股票数据
pub fn fetch(max: Option<u16>) -> Result<EastMarket> {
    fetch_pages(max, get)
}

fn fetch_pages(
    max: Option<u16>,
    mut get_page: impl FnMut(u16, u16) -> Result<String>,
) -> Result<EastMarket> {
    if max == Some(0) {
        eyre::bail!("max必须大于0");
    }
    // 正常全市场可在一页返回；若上游限制页大小，下面仍会按实际返回长度翻页。
    let page_size = max.unwrap_or(6000).min(6000);
    let mut page_number = 1u16;
    let mut total = None;
    let mut diff = Vec::with_capacity(6000);
    let mut seen = std::collections::HashSet::new();
    loop {
        let txt = get_page(page_size, page_number)?;
        let data = parse(&txt)?;
        let expected_total = *total.get_or_insert(data.data.total);
        ensure!(data.data.total == expected_total, "东财分页total发生变化");
        let len = data.data.diff.len();
        let target = max.unwrap_or(expected_total).min(expected_total) as usize;
        if len == 0 && diff.len() < target {
            eyre::bail!("东财分页返回空页，尚未覆盖全部证券");
        }
        for mut row in data.data.diff {
            row.normalize()?;
            if !seen.insert(row.code.clone()) {
                eyre::bail!("东财分页包含重复证券：{}", row.code);
            }
            diff.push(row);
        }
        ensure!(
            diff.len() <= expected_total as usize,
            "东财返回数量超过total"
        );
        info!(
            "page_number={page_number} len={len} done={} max={max:?} total={expected_total}",
            diff.len()
        );
        if diff.len() >= target {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        page_number = page_number
            .checked_add(1)
            .ok_or_else(|| eyre::eyre!("东财分页溢出"))?;
    }
    if let Some(max) = max {
        diff.truncate(max as usize);
    }
    Ok(EastMarket {
        data: EastData {
            diff,
            total: total.unwrap_or(0),
        },
    })
}

/// 用于（反）序列化：比如读取东方财富网页返回的 json ；把结果写入到 csv
/// 注意：factor 需要提供前一天的 factor 数据才会计算（即 -p xx.csv）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Day {
    /// `date` 为 `%Y-%m-%d` 文本格式
    #[serde(skip_deserializing, default)]
    pub date: String,
    #[serde(rename(deserialize = "f12"))]
    pub code: String,
    #[serde(rename(deserialize = "f13"), default, skip_serializing)]
    pub market: Option<u16>,
    #[serde(rename(deserialize = "f124"), default, skip_serializing)]
    pub timestamp: Option<i64>,
    #[serde(rename(deserialize = "f17"), deserialize_with = "deser_opt_f32")]
    pub open: F32,
    #[serde(rename(deserialize = "f15"), deserialize_with = "deser_opt_f32")]
    pub high: F32,
    #[serde(rename(deserialize = "f16"), deserialize_with = "deser_opt_f32")]
    pub low: F32,
    #[serde(rename(deserialize = "f2"), deserialize_with = "deser_opt_f32")]
    pub close: F32,
    #[serde(rename(deserialize = "f6"), deserialize_with = "deser_opt_f32")]
    pub amount: F32,
    #[serde(rename(deserialize = "f5"), deserialize_with = "deser_opt_f32")]
    pub vol: F32,
    #[serde(rename(deserialize = "f18"), deserialize_with = "deser_opt_f32")]
    pub preclose: F32,
    #[serde(skip_deserializing, default)]
    pub factor: f64,
}

fn deser_opt_f32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<F32, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(s) if s == "-" => Ok(None),
        serde_json::Value::Number(n) => n
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("非法数值")),
        _ => Err(serde::de::Error::custom("期望数值、null或'-'")),
    }
}

/// 排除掉 "-" 无实际数据的股票（完全可以不必考虑这些无效数据）
pub type F32 = Option<f64>;

impl Day {
    pub fn normalize(&mut self) -> Result<()> {
        let market = self
            .market
            .ok_or_else(|| eyre::eyre!("{}缺少市场字段", self.code))?;
        let prefix = match market {
            0 => "sz",
            1 => "sh",
            _ => {
                eyre::bail!("未知东财市场：{market}");
            }
        };
        if self.code.len() != 6 || !self.code.bytes().all(|b| b.is_ascii_digit()) {
            eyre::bail!("东财证券代码非法：{}", self.code);
        }
        let code = format!("{prefix}{}", self.code);
        if self.close.is_none() && self.timestamp.is_none_or(|t| t <= 0) {
            self.code = code;
            self.date.clear();
            return Ok(());
        }
        let ts = self
            .timestamp
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .ok_or_else(|| eyre::eyre!("{}缺少合法行情时间戳", self.code))?;
        let date = ts
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .date_naive();
        if !rustdx_complete::calendar::TradingCalendar::try_is_trading_day(&date)? {
            eyre::bail!("{}行情时间戳落在非交易日：{date}", self.code);
        }
        self.code = code;
        self.date = date.to_string();
        Ok(())
    }

    /// 日线导入只接受源时间戳已到收盘时点的快照。
    pub fn ensure_closed(&self) -> Result<()> {
        use chrono::Timelike;
        let ts = self
            .timestamp
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .ok_or_else(|| eyre::eyre!("缺少行情时间戳"))?;
        if ts
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .hour()
            < 15
        {
            eyre::bail!("{}行情尚未收盘，不能作为完整日线导入", self.code);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EastMarket {
    pub data: EastData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EastData {
    // pub diff:  Vec<Factor>,
    pub diff: Vec<Day>,
    pub total: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_rejects_zero_and_stops_at_available_total() {
        assert!(get(1, 0).is_err());
        assert!(get(0, 1).is_err());
        let page = |total, code: &str| {
            format!(
                r#"{{"data":{{"total":{total},"diff":[{{"f12":"{code}","f13":1,"f17":null,"f15":null,"f16":null,"f2":null,"f6":null,"f5":null,"f18":null}}]}}}}"#
            )
        };
        let data = fetch_pages(Some(100), |_, number| {
            assert_eq!(number, 1);
            Ok(page(1, "600000"))
        })
        .unwrap();
        assert_eq!(data.data.diff.len(), 1);
        let mut calls = 0;
        assert!(
            fetch_pages(None, |_, _| {
                calls += 1;
                Ok(page(3, "600000"))
            })
            .is_err()
        );
        assert_eq!(calls, 2);
        assert!(
            fetch_pages(None, |_, number| Ok(page(
                number + 1,
                if number == 1 { "600000" } else { "600519" }
            )))
            .is_err()
        );
    }

    #[test]
    fn jsonp_boundaries_and_source_date() {
        let ts = chrono::DateTime::parse_from_rfc3339("2026-09-01T15:01:00+08:00")
            .unwrap()
            .timestamp();
        let text = format!(
            "callback({{\"data\":{{\"total\":1,\"diff\":[{{\"f12\":\"600000\",\"f13\":1,\"f124\":{ts},\"f17\":10,\"f15\":11,\"f16\":9,\"f2\":10,\"f6\":1000,\"f5\":100,\"f18\":9}}]}}}});"
        );
        let mut data = parse(&text).unwrap();
        let row = &mut data.data.diff[0];
        row.normalize().unwrap();
        row.ensure_closed().unwrap();
        assert_eq!(row.code, "sh600000");
        assert_eq!(row.date, "2026-09-01");
        for malformed in ["", "abc", "cb()", "cb({)", "中文"] {
            assert!(parse(malformed).is_err());
        }
    }
}
