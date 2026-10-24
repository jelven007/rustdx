//! 0feb uses a date selector. The server does not echo the actual trading date.
use super::{PrecisionSnapshot, PriceScale, command_request::*};
use crate::tcp::{Tcp, reader::Reader};
use serde::Serialize;
use std::io;

pub const RECENT_MINUTES_COMMAND: u16 = 0x0feb;
/// Version 2 encodes the negated YYYYMMDD integer, not a Gregorian ordinal.
pub const RECENT_MINUTES_DATE_SCHEMA_VERSION: u16 = 2;

/// A selector, nonempty response, or synthetic time label cannot qualify a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum MinuteDateUnavailableReason {
    ServerDoesNotEchoTradingDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum MinuteDateQualification {
    Unverified { reason: MinuteDateUnavailableReason },
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentMinutesRequest {
    symbol: CommandSymbol,
    requested_date: u32,
    selector: u32,
}
impl RecentMinutesRequest {
    pub fn new(market: u16, code: &str, requested_date: u32) -> io::Result<Self> {
        date(requested_date)?;
        // Wire selector is the two's complement of YYYYMMDD. The former
        // ordinal-based formula selected unrelated dates (Aug 17 -> Jun 9).
        let selector = requested_date.wrapping_neg();
        Ok(Self {
            symbol: CommandSymbol::new(market, code)?,
            requested_date,
            selector,
        })
    }
    pub fn encode(&self, id: u32) -> io::Result<Vec<u8>> {
        let mut body = self.selector.to_le_bytes().to_vec();
        self.symbol.encode(&mut body, false);
        crate::tcp::frame::request(RECENT_MINUTES_COMMAND, id, &body)
    }
    pub fn recv(&self, tcp: &mut Tcp, metadata: &PrecisionSnapshot) -> io::Result<MinuteSeries> {
        if !metadata.is_fresh(self.symbol.market(), chrono::Utc::now()) {
            return Err(input("近期分时精度目录过期"));
        }
        metadata.scale(self.symbol.market(), self.symbol.code())?;
        let bytes = tcp.exchange_checked(&self.encode(crate::tcp::frame::sequence())?)?;
        if !metadata.is_fresh(self.symbol.market(), chrono::Utc::now()) {
            return Err(invalid("近期分时请求跨日，请重取目录"));
        }
        let mut series = decode_recent_minutes(self, &bytes, metadata)?;
        series.page.source.attach(tcp)?;
        Ok(series)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct MinutePoint {
    pub index: u16,
    /// Synthetic session label; it is not present on the wire.
    pub time_label: String,
    pub price: f64,
    pub average_price: f64,
    pub volume_raw: i64,
    pub price_field_raw: i64,
    pub average_field_raw: i64,
    pub price_raw: i64,
    pub average_raw: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct MinuteSeries {
    pub request: RecentMinutesRequest,
    pub requested_date: u32,
    /// Compatibility metadata, never authoritative for publication.
    pub server_trading_date: Option<u32>,
    date_qualification: MinuteDateQualification,
    pub date_selector_raw: u32,
    pub previous_close: f32,
    pub open_price: f32,
    pub previous_close_bits: u32,
    pub open_bits: u32,
    pub scale: PriceScale,
    pub page: CommandPage<MinutePoint>,
}

impl MinuteSeries {
    pub fn date_qualification(&self) -> MinuteDateQualification {
        self.date_qualification
    }

    /// Date-dependent adapters must call this before assigning a partition.
    /// Current 0feb decoding has no independently verified date evidence.
    /// Changing the public compatibility date fields cannot qualify the result.
    pub fn require_trading_date(&self) -> io::Result<u32> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "0feb交易日期未核实：主站不回显真实交易日，不能按请求日期发布",
        ))
    }
}

pub fn decode_recent_minutes(
    request: &RecentMinutesRequest,
    bytes: &[u8],
    metadata: &PrecisionSnapshot,
) -> io::Result<MinuteSeries> {
    bounded(bytes)?;
    let scale = metadata.scale(request.symbol.market(), request.symbol.code())?;
    let mut r = Reader::new(bytes);
    let n = usize::from(r.u16()?);
    let previous_close = r.f32()?;
    let open_price = r.f32()?;
    if previous_close < 0.0 || open_price < 0.0 {
        return Err(invalid("0feb昨收或今开为负"));
    }
    if n > 240 || n > r.remaining() / 3 {
        return Err(invalid("0feb点数或正文长度异常"));
    }
    let mut base = (0, 0);
    let mut points = Vec::with_capacity(n);
    for index in 0..n {
        let price_field_raw = r.varint64()?;
        let average_field_raw = r.varint64()?;
        let volume_raw = r.varint64()?;
        let price_raw = add(base.0, price_field_raw)?;
        let average_raw = add(base.1, average_field_raw)?;
        if index == 0 {
            base = (price_raw, average_raw);
        }
        if price_raw < 0 || average_raw < 0 || volume_raw < 0 {
            return Err(invalid("0feb负价格/均价/数量"));
        }
        let minute = if index < 120 {
            571 + index
        } else {
            781 + index - 120
        };
        points.push(MinutePoint {
            index: index as u16,
            time_label: format!("{:02}:{:02}", minute / 60, minute % 60),
            price: price_raw as f64 / f64::from(scale.divisor()),
            average_price: average_raw as f64 / (100.0 * f64::from(scale.divisor())),
            volume_raw,
            price_field_raw,
            average_field_raw,
            price_raw,
            average_raw,
        });
    }
    exact(&r)?;
    let mut page = CommandPage::new(RECENT_MINUTES_COMMAND, points, bytes);
    page.source.metadata_generation = Some(metadata.generation());
    Ok(MinuteSeries {
        request: request.clone(),
        requested_date: request.requested_date,
        server_trading_date: None,
        date_qualification: MinuteDateQualification::Unverified {
            reason: MinuteDateUnavailableReason::ServerDoesNotEchoTradingDate,
        },
        date_selector_raw: request.selector,
        previous_close,
        open_price,
        previous_close_bits: previous_close.to_bits(),
        open_bits: open_price.to_bits(),
        scale,
        page,
    })
}
