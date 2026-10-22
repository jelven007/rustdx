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
    unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
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
        std::array::from_fn(|offset| crate::http::agent(EastmoneyResolver { offset }))
    })
}

/// 获取股票数据
///
/// 东财接口偶发网络抖动（Unexpected EOF 等），依次尝试多个 HTTPS 节点。
pub fn get(page_size: u16, page_number: u16) -> Result<String> {
    crate::observation::measure(crate::observation::Stage::Download, || {
        get_inner(page_size, page_number)
    })
}

fn get_inner(page_size: u16, page_number: u16) -> Result<String> {
    if page_size == 0 || page_number == 0 {
        eyre::bail!("page_size和page_number必须大于0");
    }
    const MAX_RETRIES: usize = 1;
    let mut last_err = None;
    for node_offset in 0..eastmoney_agents().len() {
        let node = (page_number as usize - 1 + node_offset) % eastmoney_agents().len();
        let url = request_url(page_size, page_number);
        for attempt in 1..=MAX_RETRIES {
            let mut operation = crate::http::HttpOperation::new(Duration::from_secs(15))?;
            info!("Get: {url}");
            match eastmoney_agents()[node]
                .get(&url)
                .config()
                .timeout_global(Some(operation.timeout()))
                .build()
                .header("Accept", "application/json,text/plain,*/*")
                .header("Referer", "https://quote.eastmoney.com/")
                .header("User-Agent", "Mozilla/5.0 rustdx-cli")
                .call()
            {
                Ok(mut resp) => {
                    let text = crate::http::text(resp.body_mut(), 16 * 1024 * 1024)?;
                    operation.complete(text.trim().is_empty())?;
                    return Ok(text);
                }
                Err(e) => {
                    warn!("东财节点#{node}请求失败（第 {attempt}/{MAX_RETRIES} 次）：{e}");
                    last_err = Some(e);
                    rustdx_complete::operation::sleep(Duration::from_millis(500 * attempt as u64))?;
                }
            }
        }
    }
    Err(eyre::eyre!(
        last_err.ok_or_else(|| eyre::eyre!("没有可用东财节点"))?
    ))
    .wrap_err_with(|| {
        format!(
            "获取东财股票数据失败（{}个HTTPS节点各重试{MAX_RETRIES}次）",
            eastmoney_agents().len()
        )
    })
}

pub const PARSER_VERSION: &str = "eastmoney-clist/2";
pub const MAX_PAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;

fn request_url(page_size: u16, page_number: u16) -> String {
    format!(
        "https://push2.eastmoney.com/api/qt/clist/get?cb=jQuery112407375845698232317_1631693257414&\
        pn={page_number}&pz={page_size}&po=0&np=1&ut=bd1d9ddb04089700cf9c27f6f7426281&fltt=2&\
        invt=2&fid=f12&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23&fields=f18,f16,f12,f13,f124,f17,f15,f2,f6,f5&_=1631693257534"
    )
}

/// Exact UTF-8 HTTP response body before JSONP parsing or normalization.
/// Transport/TLS bytes are not captured; f13/f124 and unknown source fields remain intact.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPage {
    pub parser_version: String,
    pub request_url: String,
    pub page_size: u16,
    pub page_number: u16,
    pub max: Option<u16>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub body_sha256: String,
    pub body: String,
}

impl RawPage {
    fn new(page_size: u16, page_number: u16, max: Option<u16>, body: String) -> Self {
        use sha2::{Digest, Sha256};
        Self {
            parser_version: PARSER_VERSION.into(),
            request_url: request_url(page_size, page_number),
            page_size,
            page_number,
            max,
            fetched_at: chrono::Utc::now(),
            body_sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            body,
        }
    }

    pub fn validate(&self) -> Result<()> {
        use sha2::{Digest, Sha256};
        ensure!(
            self.parser_version == PARSER_VERSION,
            "不支持的东财解析器版本"
        );
        ensure!(
            self.max != Some(0)
                && self.page_size > 0
                && self.page_size <= 6000
                && self.page_number > 0,
            "东财证据请求参数非法"
        );
        ensure!(
            self.request_url == request_url(self.page_size, self.page_number),
            "东财证据URL与参数不符"
        );
        ensure!(self.body.len() <= MAX_PAGE_BYTES, "东财证据页超过限制");
        ensure!(
            format!("{:x}", Sha256::digest(self.body.as_bytes())) == self.body_sha256,
            "东财原始响应摘要不一致"
        );
        Ok(())
    }
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
    fetch_with_calendar(max, &rustdx_complete::calendar::BuiltinCalendar)
}

pub fn fetch_with_calendar(
    max: Option<u16>,
    calendar: &dyn rustdx_complete::calendar::CalendarProvider,
) -> Result<EastMarket> {
    fetch_pages_with_calendar(max, get, calendar)
}

/// Observer receives each bounded raw page before parsing, including malformed responses.
pub fn fetch_with_evidence(
    max: Option<u16>,
    calendar: &dyn rustdx_complete::calendar::CalendarProvider,
    mut observer: impl FnMut(&RawPage) -> Result<()>,
) -> Result<EastMarket> {
    let mut bytes = 0usize;
    fetch_pages_with_calendar(
        max,
        |size, number| {
            let page = RawPage::new(size, number, max, get(size, number)?);
            bytes = bytes
                .checked_add(page.body.len())
                .ok_or_else(|| eyre::eyre!("东财证据长度溢出"))?;
            ensure!(
                page.body.len() <= MAX_PAGE_BYTES && bytes <= MAX_CAPTURE_BYTES,
                "东财原始响应超过留存限制"
            );
            observer(&page)?;
            Ok(page.body)
        },
        calendar,
    )
}

/// Offline replay validates page order, request parameters, parser version and body hashes.
/// The iterator can load one evidence file at a time, without retaining all raw pages.
pub fn replay(
    max: Option<u16>,
    pages: impl IntoIterator<Item = Result<RawPage>>,
    calendar: &dyn rustdx_complete::calendar::CalendarProvider,
) -> Result<EastMarket> {
    let mut pages = pages.into_iter();
    let market = fetch_pages_with_calendar(
        max,
        |size, number| {
            let page = pages.next().ok_or_else(|| eyre::eyre!("东财重放缺页"))??;
            page.validate()?;
            ensure!(
                page.page_size == size && page.page_number == number && page.max == max,
                "东财重放页序或请求参数不符"
            );
            Ok(page.body)
        },
        calendar,
    )?;
    ensure!(pages.next().is_none(), "东财重放包含额外页");
    Ok(market)
}

#[cfg(test)]
fn fetch_pages(
    max: Option<u16>,
    mut get_page: impl FnMut(u16, u16) -> Result<String>,
) -> Result<EastMarket> {
    fetch_pages_with_calendar(
        max,
        &mut get_page,
        &rustdx_complete::calendar::BuiltinCalendar,
    )
}

fn fetch_pages_with_calendar(
    max: Option<u16>,
    mut get_page: impl FnMut(u16, u16) -> Result<String>,
    calendar: &dyn rustdx_complete::calendar::CalendarProvider,
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
    let mut captured_bytes = 0usize;
    loop {
        rustdx_complete::operation::check()?;
        let txt = get_page(page_size, page_number)?;
        ensure!(txt.len() <= MAX_PAGE_BYTES, "东财单页超过16MiB限制");
        captured_bytes = captured_bytes
            .checked_add(txt.len())
            .ok_or_else(|| eyre::eyre!("东财响应长度溢出"))?;
        ensure!(
            captured_bytes <= MAX_CAPTURE_BYTES,
            "东财响应合计超过64MiB限制"
        );
        let data = crate::observation::measure(crate::observation::Stage::Parse, || parse(&txt))?;
        let expected_total = *total.get_or_insert(data.data.total);
        ensure!(data.data.total == expected_total, "东财分页total发生变化");
        let len = data.data.diff.len();
        let target = max.unwrap_or(expected_total).min(expected_total) as usize;
        if len == 0 && diff.len() < target {
            rustdx_complete::tcp::observe_empty_response();
            eyre::bail!("东财分页返回空页，尚未覆盖全部证券");
        }
        crate::observation::measure(crate::observation::Stage::Parse, || -> Result<()> {
            for mut row in data.data.diff {
                row.normalize_with_calendar(calendar)?;
                if !seen.insert(row.code.clone()) {
                    eyre::bail!("东财分页包含重复证券：{}", row.code);
                }
                diff.push(row);
            }
            Ok(())
        })?;
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
        rustdx_complete::operation::sleep(Duration::from_millis(150))?;
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

/// 东财A股收盘快照的标准适配结果。OHLC/amount为人民币元，
/// 标准量为股；源量为100股/手，源时间戳与标准15:00日线时点分开保留。
/// 未计算的旧CSV factor=0对应None，不是有效收益指数或价格乘数。
#[derive(Debug, Clone, Serialize)]
pub struct EastCanonicalBar {
    pub candle: rustdx_complete::data::CanonicalBar,
    pub source_timestamp: chrono::DateTime<chrono::FixedOffset>,
    pub source_volume_unit: rustdx_complete::data::VolumeUnit,
    /// 源前收（除权日为除权参考价），单位人民币元。
    pub preclose: f64,
    pub return_index: Option<rustdx_complete::data::ReturnIndex>,
}

impl EastCanonicalBar {
    /// 承接已经校验过日期的上一交易日累计收益，保持原始OHLC。
    pub fn advance_return_index(
        &self,
        prior: rustdx_complete::data::ReturnIndex,
    ) -> Result<rustdx_complete::data::ReturnIndex> {
        Ok(rustdx_complete::data::ReturnIndex::new(
            self.candle.bar.close / self.preclose * prior.value(),
        )?)
    }
}

impl Day {
    pub fn to_canonical(&self) -> Result<EastCanonicalBar> {
        self.to_canonical_with_calendar(&rustdx_complete::calendar::BuiltinCalendar)
    }

    /// 接受已normalize的A股记录，独立核对市场、源时间戳及日期。
    /// 是否为当前行情日由采集任务决定，此入口也支持历史样本验证。
    pub fn to_canonical_with_calendar(
        &self,
        calendar: &dyn rustdx_complete::calendar::CalendarProvider,
    ) -> Result<EastCanonicalBar> {
        use rustdx_complete::data::{
            Bar, BarSource, CanonicalBar, Period, PriceBasis, ReturnIndex, Symbol, VolumeUnit,
            exchange_timezone,
        };
        self.ensure_closed()?;
        let symbol: Symbol = self.code.parse()?;
        if !symbol.is_a_share()
            || !symbol.code().bytes().all(|b| b.is_ascii_digit())
            || self.market != Some(symbol.market().tdx_market()?)
        {
            eyre::bail!("{}东财市场/证券代码不匹配或非已核实A股", self.code);
        }
        let source_timestamp = self
            .timestamp
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .ok_or_else(|| eyre::eyre!("{}缺少合法行情时间戳", self.code))?
            .with_timezone(&exchange_timezone());
        let date = chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")?;
        if date != source_timestamp.date_naive() || !calendar.is_trading_day(date)? {
            eyre::bail!("{}日线日期与源时间戳/交易日不一致", self.code);
        }
        let field =
            |value: Option<f64>| value.ok_or_else(|| eyre::eyre!("{}行情字段缺失", self.code));
        let bar = Bar {
            symbol,
            datetime: date.and_hms_opt(15, 0, 0).expect("固定收盘时间15:00"),
            open: field(self.open)?,
            high: field(self.high)?,
            low: field(self.low)?,
            close: field(self.close)?,
            volume_shares: VolumeUnit::Lots.to_shares(field(self.vol)?),
            amount: field(self.amount)?,
        };
        bar.validate()?;
        let preclose = field(self.preclose)?;
        if !preclose.is_finite() || preclose <= 0.0 {
            eyre::bail!("{}前收非法", self.code);
        }
        let return_index = if self.factor == 0.0 {
            None
        } else {
            Some(ReturnIndex::new(self.factor)?)
        };
        Ok(EastCanonicalBar {
            candle: CanonicalBar {
                bar,
                period: Period::Day,
                source: BarSource::Eastmoney,
                price_basis: PriceBasis::Raw,
            },
            source_timestamp,
            source_volume_unit: VolumeUnit::Lots,
            preclose,
            return_index,
        })
    }

    pub fn normalize(&mut self) -> Result<()> {
        self.normalize_with_calendar(&rustdx_complete::calendar::BuiltinCalendar)
    }

    pub fn normalize_with_calendar(
        &mut self,
        calendar: &dyn rustdx_complete::calendar::CalendarProvider,
    ) -> Result<()> {
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
        if !calendar.is_trading_day(date)? {
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
    fn offline_replay_preserves_source_fields_and_rejects_tampering_and_page_errors() {
        let body = r#"cb({"data":{"total":1,"diff":[{"f12":"000001","f13":0,"f124":1792652460,"f17":11.41,"f15":11.42,"f16":11.26,"f2":11.3,"f6":1468424722.09,"f5":1297649,"f18":11.41,"future_source_field":7}]}});"#;
        let make = || RawPage::new(2, 1, Some(2), body.into());
        let market = replay(
            Some(2),
            [Ok(make())],
            &rustdx_complete::calendar::BuiltinCalendar,
        )
        .unwrap();
        assert_eq!(market.data.diff[0].market, Some(0));
        assert_eq!(market.data.diff[0].timestamp, Some(1792652460));
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.serialize(&market.data.diff[0]).unwrap();
        assert_eq!(
            String::from_utf8(csv.into_inner().unwrap())
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            include_str!("../../tests/fixtures/east-20261022.csv")
                .lines()
                .take(2)
                .collect::<Vec<_>>()
        );
        let saved = serde_json::to_vec(&make()).unwrap();
        let restored: RawPage = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.body, body);
        for mutate in [
            |p: &mut RawPage| p.body.push(' '),
            |p: &mut RawPage| p.page_number = 2,
            |p: &mut RawPage| p.parser_version = "future".into(),
            |p: &mut RawPage| p.max = None,
            |p: &mut RawPage| p.request_url.push_str("&altered=1"),
        ] {
            let mut page = make();
            mutate(&mut page);
            assert!(
                replay(
                    Some(2),
                    [Ok(page)],
                    &rustdx_complete::calendar::BuiltinCalendar
                )
                .is_err()
            );
        }
        assert!(replay(Some(2), [], &rustdx_complete::calendar::BuiltinCalendar).is_err());
        assert!(
            replay(
                Some(2),
                [Ok(make()), Ok(make())],
                &rustdx_complete::calendar::BuiltinCalendar
            )
            .is_err()
        );
    }

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

    #[test]
    fn canonical_adapter_preserves_csv_units_time_and_return_semantics() {
        use rustdx_complete::data::{BarSource, Period, PriceBasis, ReturnIndex, VolumeUnit};
        let ts = chrono::DateTime::parse_from_rfc3339("2026-10-22T15:01:00+08:00")
            .unwrap()
            .timestamp();
        // Values from the independently captured closed-session CSV fixture.
        let text = format!(
            r#"{{"data":{{"total":1,"diff":[{{"f12":"000001","f13":0,"f124":{ts},"f17":11.41,"f15":11.42,"f16":11.26,"f2":11.3,"f6":1468424722.09,"f5":1297649,"f18":11.41}}]}}}}"#
        );
        let mut row = parse(&text).unwrap().data.diff.remove(0);
        row.normalize().unwrap();
        let canonical = row.to_canonical().unwrap();
        assert_eq!(canonical.candle.source, BarSource::Eastmoney);
        assert_eq!(canonical.candle.period, Period::Day);
        assert_eq!(canonical.candle.price_basis, PriceBasis::Raw);
        assert!(matches!(canonical.source_volume_unit, VolumeUnit::Lots));
        assert_eq!(canonical.candle.bar.symbol.to_string(), "sz000001");
        assert_eq!(canonical.candle.bar.volume_shares, 129764900.0);
        assert_eq!(canonical.candle.bar.amount, 1468424722.09);
        assert_eq!(canonical.source_timestamp.offset().local_minus_utc(), 28800);
        assert_eq!(
            canonical.candle.bar.datetime.to_string(),
            "2026-10-22 15:00:00"
        );
        assert_eq!(
            canonical.source_timestamp.naive_local().to_string(),
            "2026-10-22 15:01:00"
        );
        assert!(canonical.return_index.is_none());
        assert_eq!(
            canonical
                .advance_return_index(ReturnIndex::new(2.0).unwrap())
                .unwrap()
                .value(),
            11.3 / 11.41 * 2.0
        );
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.serialize(&row).unwrap();
        let csv = String::from_utf8(csv.into_inner().unwrap()).unwrap();
        let captured = include_str!("../../tests/fixtures/east-20261022.csv");
        assert_eq!(
            csv.lines().collect::<Vec<_>>(),
            captured.lines().take(2).collect::<Vec<_>>()
        );

        let valid = row.clone();
        for mutate in [
            |row: &mut Day| row.market = Some(1),
            |row: &mut Day| row.date = "2026-10-21".into(),
            |row: &mut Day| row.timestamp = row.timestamp.map(|value| value - 3600),
            |row: &mut Day| row.code = "sh000001".into(),
            |row: &mut Day| row.open = None,
            |row: &mut Day| row.vol = Some(f64::MAX),
            |row: &mut Day| row.preclose = Some(0.0),
            |row: &mut Day| row.factor = -1.0,
            |row: &mut Day| row.factor = f64::NAN,
        ] {
            row = valid.clone();
            mutate(&mut row);
            assert!(row.to_canonical().is_err(), "{row:?}");
        }
        row = valid;
        row.factor = 2.0;
        assert_eq!(
            row.to_canonical().unwrap().return_index.unwrap().value(),
            2.0
        );
    }
}
