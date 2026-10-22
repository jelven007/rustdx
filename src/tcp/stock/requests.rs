//! Checked immutable requests. Response decoders are independent of transport.
use std::io::{self, ErrorKind};

use super::{
    FinanceInfo, FinanceInfoData, HistoryTransaction, QuoteData, SecurityQuotes, Transaction,
    TransactionData,
};
use crate::tcp::{Tcp, Tdx, reader::Reader};

fn symbol(market: u16, code: &str) -> io::Result<()> {
    if market > 1 || code.len() != 6 || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "市场或六位ASCII代码非法",
        ));
    }
    Ok(())
}

fn checked_date(value: u32, kind: ErrorKind) -> io::Result<()> {
    if !(1..=9999).contains(&(value / 10000)) {
        return Err(io::Error::new(kind, "协议日期须为有效YYYYMMDD"));
    }
    crate::data::date_from_u32(value)
        .map(|_| ())
        .map_err(|e| io::Error::new(kind, e))
}

fn finish(reader: &Reader<'_>) -> io::Result<()> {
    if reader.remaining() != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "协议响应包含多余字节",
        ));
    }
    Ok(())
}

/// One to eighty distinct, explicit market/code pairs. Parameters cannot be
/// changed independently of the encoded request.
///
/// ```
/// use rustdx_complete::tcp::stock::QuotesRequest;
/// let request = QuotesRequest::new(vec![(0, "000001"), (1, "000001")]).unwrap();
/// assert_eq!(request.encode().len(), 36);
/// assert!(QuotesRequest::new(vec![(2, "920001")]).is_err());
/// ```
///
/// ```compile_fail
/// use rustdx_complete::tcp::stock::QuotesRequest;
/// let mut request = QuotesRequest::new(vec![(0, "000001")]).unwrap();
/// request.stocks.push((2, "920001"));
/// ```
#[derive(Debug, Clone)]
pub struct QuotesRequest<'a> {
    stocks: Vec<(u16, &'a str)>,
}

impl<'a> QuotesRequest<'a> {
    pub fn new(stocks: Vec<(u16, &'a str)>) -> io::Result<Self> {
        if !(1..=80).contains(&stocks.len()) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "行情证券数量须为1–80",
            ));
        }
        for (i, &(market, code)) in stocks.iter().enumerate() {
            symbol(market, code)?;
            if stocks[..i].contains(&(market, code)) {
                return Err(io::Error::new(ErrorKind::InvalidInput, "行情请求证券重复"));
            }
        }
        Ok(Self { stocks })
    }

    pub fn stocks(&self) -> &[(u16, &'a str)] {
        &self.stocks
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = SecurityQuotes::SEND.to_vec();
        let len = (self.stocks.len() * 7 + 12) as u16;
        bytes[6..8].copy_from_slice(&len.to_le_bytes());
        bytes[8..10].copy_from_slice(&len.to_le_bytes());
        bytes[20..22].copy_from_slice(&(self.stocks.len() as u16).to_le_bytes());
        for &(market, code) in &self.stocks {
            bytes.push(market as u8);
            bytes.extend_from_slice(code.as_bytes());
        }
        bytes
    }

    pub fn recv(&self, tcp: &mut Tcp) -> io::Result<Vec<QuoteData>> {
        let bytes = crate::tcp::send_recv_decompress(tcp, &self.encode(), "行情快照")?;
        decode_quotes(self, &bytes)
    }
}

/// Missing quotes remain missing; empty results do not establish a security's
/// trading status. Returned pairs must be distinct members of the request.
pub fn decode_quotes(request: &QuotesRequest<'_>, bytes: &[u8]) -> io::Result<Vec<QuoteData>> {
    let mut reader = Reader::new(bytes);
    reader.skip(2)?;
    let count = reader.u16()? as usize;
    if count > request.stocks.len() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "行情数量超过请求数量",
        ));
    }
    let mut seen = [false; 80];
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        crate::operation::check()?;
        let (market, quote) = super::quotes::parse_quote(&mut reader)?;
        let Some(index) = request
            .stocks
            .iter()
            .position(|&(m, code)| m == u16::from(market) && code == quote.code)
        else {
            return Err(io::Error::new(ErrorKind::InvalidData, "行情返回未请求证券"));
        };
        if std::mem::replace(&mut seen[index], true) {
            return Err(io::Error::new(ErrorKind::InvalidData, "行情返回重复证券"));
        }
        quote.validate()?;
        rows.push(quote);
    }
    finish(&reader)?;
    Ok(rows)
}

/// Checked single-security finance request; dates in the response may be zero
/// (source unknown), but nonzero dates must be real calendar dates.
#[derive(Debug, Clone, Copy)]
pub struct FinanceRequest<'a> {
    market: u16,
    code: &'a str,
}

impl<'a> FinanceRequest<'a> {
    pub fn new(market: u16, code: &'a str) -> io::Result<Self> {
        symbol(market, code)?;
        Ok(Self { market, code })
    }
    pub fn market(self) -> u16 {
        self.market
    }
    pub fn code(self) -> &'a str {
        self.code
    }
    pub fn encode(self) -> [u8; 21] {
        let mut bytes = [0; 21];
        bytes[..14].copy_from_slice(FinanceInfo::SEND);
        bytes[14] = self.market as u8;
        bytes[15..].copy_from_slice(self.code.as_bytes());
        bytes
    }
    pub fn recv(self, tcp: &mut Tcp) -> io::Result<Vec<FinanceInfoData>> {
        let bytes = crate::tcp::send_recv_decompress(tcp, &self.encode(), "财务信息")?;
        decode_finance(&self, &bytes)
    }
}

pub fn decode_finance(
    request: &FinanceRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<FinanceInfoData>> {
    let mut reader = Reader::new(bytes);
    match reader.u16()? {
        0 => {
            finish(&reader)?;
            return Ok(Vec::new());
        }
        1 => {}
        _ => return Err(io::Error::new(ErrorKind::InvalidData, "财务数量须为0或1")),
    }
    let market = reader.u8()?;
    let code = reader.code()?;
    if u16::from(market) != request.market || code != request.code {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "财务证券与请求不一致",
        ));
    }
    let float = |reader: &mut Reader<'_>| -> io::Result<f64> {
        let value = f64::from(f32::from_bits(reader.u32()?));
        if !value.is_finite() {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "财务字段包含非有限值",
            ));
        }
        Ok(value)
    };
    let liutongguben = float(&mut reader)? * 10000.0;
    let province = reader.u16()?;
    let industry = reader.u16()?;
    let updated_date = reader.u32()?;
    let ipo_date = reader.u32()?;
    for date in [updated_date, ipo_date] {
        if date != 0 {
            checked_date(date, ErrorKind::InvalidData)?;
        }
    }
    let mut values = [0.0; 30];
    for value in &mut values {
        *value = float(&mut reader)?;
    }
    finish(&reader)?;
    let scaled = |index: usize| values[index] * 10000.0;
    // Scale after widening to f64, as the independent pytdx decoder does.
    // Negative profits and cash flows are legitimate and remain unchanged.
    Ok(vec![FinanceInfoData {
        market,
        code,
        liutongguben,
        province,
        industry,
        updated_date,
        ipo_date,
        zongguben: scaled(0),
        guojiagu: scaled(1),
        faqirenfarengu: scaled(2),
        farengu: scaled(3),
        bgu: scaled(4),
        hgu: scaled(5),
        zhigonggu: scaled(6),
        zongzichan: scaled(7),
        liudongzichan: scaled(8),
        gudingzichan: scaled(9),
        wuxingzichan: scaled(10),
        gudongrenshu: values[11],
        liudongfuzhai: scaled(12),
        changqifuzhai: scaled(13),
        zibengongjijin: scaled(14),
        jingzichan: scaled(15),
        zhuyingshouru: scaled(16),
        zhuyinglirun: scaled(17),
        yingshouzhangkuan: scaled(18),
        yingyelirun: scaled(19),
        touzishouyu: scaled(20),
        jingyingxianjinliu: scaled(21),
        zongxianjinliu: scaled(22),
        cunhuo: scaled(23),
        lirunzonghe: scaled(24),
        shuihoulirun: scaled(25),
        jinglirun: scaled(26),
        weifenpeilirun: scaled(27),
        meigujingzichan: values[28],
        baoliu2: values[29],
    }])
}

/// Checked current-day ticks; at most 2000 records per page.
#[derive(Debug, Clone, Copy)]
pub struct TransactionRequest<'a> {
    market: u16,
    code: &'a str,
    start: u16,
    count: u16,
}

impl<'a> TransactionRequest<'a> {
    pub fn new(market: u16, code: &'a str, start: u16, count: u16) -> io::Result<Self> {
        symbol(market, code)?;
        if !(1..=2000).contains(&count) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "逐笔数量须为1–2000",
            ));
        }
        Ok(Self {
            market,
            code,
            start,
            count,
        })
    }
    pub fn market(self) -> u16 {
        self.market
    }
    pub fn code(self) -> &'a str {
        self.code
    }
    pub fn start(self) -> u16 {
        self.start
    }
    pub fn count(self) -> u16 {
        self.count
    }
    pub fn encode(self) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..12].copy_from_slice(Transaction::SEND);
        bytes[12..14].copy_from_slice(&self.market.to_le_bytes());
        bytes[14..20].copy_from_slice(self.code.as_bytes());
        bytes[20..22].copy_from_slice(&self.start.to_le_bytes());
        bytes[22..24].copy_from_slice(&self.count.to_le_bytes());
        bytes
    }
    pub fn recv(self, tcp: &mut Tcp) -> io::Result<Vec<TransactionData>> {
        let bytes = crate::tcp::send_recv_decompress(tcp, &self.encode(), "逐笔成交")?;
        decode_transactions(&self, &bytes)
    }
}

/// Historical ticks have a different record layout (no trade number).
#[derive(Debug, Clone, Copy)]
pub struct HistoryTransactionRequest<'a> {
    page: TransactionRequest<'a>,
    date: u32,
}

impl<'a> HistoryTransactionRequest<'a> {
    pub fn new(market: u16, code: &'a str, start: u16, count: u16, date: u32) -> io::Result<Self> {
        let page = TransactionRequest::new(market, code, start, count)?;
        checked_date(date, ErrorKind::InvalidInput)?;
        Ok(Self { page, date })
    }
    pub fn market(self) -> u16 {
        self.page.market
    }
    pub fn code(self) -> &'a str {
        self.page.code
    }
    pub fn start(self) -> u16 {
        self.page.start
    }
    pub fn count(self) -> u16 {
        self.page.count
    }
    pub fn date(self) -> u32 {
        self.date
    }
    pub fn encode(self) -> [u8; 28] {
        let mut bytes = [0; 28];
        bytes[..12].copy_from_slice(&HistoryTransaction::SEND[..12]);
        bytes[12..16].copy_from_slice(&self.date.to_le_bytes());
        bytes[16..].copy_from_slice(&self.page.encode()[12..]);
        bytes
    }
    pub fn recv(self, tcp: &mut Tcp) -> io::Result<Vec<TransactionData>> {
        let bytes = crate::tcp::send_recv_decompress(tcp, &self.encode(), "历史逐笔")?;
        decode_history_transactions(&self, &bytes)
    }
}

pub fn decode_transactions(
    request: &TransactionRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<TransactionData>> {
    ticks(bytes, request.count, false)
}

pub fn decode_history_transactions(
    request: &HistoryTransactionRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<TransactionData>> {
    ticks(bytes, request.page.count, true)
}

fn ticks(bytes: &[u8], requested: u16, historical: bool) -> io::Result<Vec<TransactionData>> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()?;
    if count > requested {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "逐笔数量超过请求数量",
        ));
    }
    if historical {
        reader.skip(4)?;
    }
    let mut rows = Vec::with_capacity(count as usize);
    let mut price = 0i64;
    for _ in 0..count {
        crate::operation::check()?;
        let minutes = reader.u16()?;
        price += i64::from(reader.varint()?);
        let vol = reader.varint()?;
        let num = if historical { 0 } else { reader.varint()? };
        let buyorsell = reader.varint()?;
        reader.varint()?;
        if minutes >= 1440 || price <= 0 || vol < 0 || num < 0 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "逐笔时间、价格、成交量或编号非法",
            ));
        }
        // Preserve undocumented direction values rather than assigning meaning.
        rows.push(TransactionData {
            time: format!("{:02}:{:02}", minutes / 60, minutes % 60),
            price: price as f64 / 100.0,
            vol,
            num,
            buyorsell,
        });
    }
    finish(&reader)?;
    Ok(rows)
}
