//! 标准行情附带周期、来源和价格口径；收益指数与价格乘数不允许互换。
use serde::Serialize;

use super::{Bar, Market, Symbol, VolumeUnit};
use crate::{Error, Result, file::lc::Lc, tcp::stock::KlineData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Period {
    Minute1,
    Minute5,
    Minute15,
    Minute30,
    Hour1,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl Period {
    pub fn tdx_category(self) -> u16 {
        match self {
            Self::Minute1 => 8,
            Self::Minute5 => 0,
            Self::Minute15 => 1,
            Self::Minute30 => 2,
            Self::Hour1 => 3,
            Self::Day => 9,
            Self::Week => 5,
            Self::Month => 6,
            Self::Quarter => 10,
            Self::Year => 11,
        }
    }
}

impl TryFrom<u16> for Period {
    type Error = Error;
    fn try_from(value: u16) -> Result<Self> {
        Ok(match value {
            0 => Self::Minute5,
            1 => Self::Minute15,
            2 => Self::Minute30,
            3 => Self::Hour1,
            4 | 9 => Self::Day,
            5 => Self::Week,
            6 => Self::Month,
            7 | 8 => Self::Minute1,
            10 => Self::Quarter,
            11 => Self::Year,
            _ => return Err(Error::Custom("未知通达信K线周期")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PriceMultiplier(f64);

impl PriceMultiplier {
    pub fn new(value: f64) -> Result<Self> {
        if !value.is_finite() || value <= 0.0 {
            return Err(Error::Custom("价格乘数必须为正且有限"));
        }
        Ok(Self(value))
    }
    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ReturnIndex(f64);

impl ReturnIndex {
    pub fn new(value: f64) -> Result<Self> {
        if !value.is_finite() || value <= 0.0 {
            return Err(Error::Custom("累计收益指数必须为正且有限"));
        }
        Ok(Self(value))
    }
    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PriceBasis {
    Raw,
    ForwardAdjusted,
    BackwardAdjusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BarSource {
    TdxTcp,
    TdxDayFile,
    TdxMinuteFile,
    Eastmoney,
}

/// 标准Bar为UTC+8、人民币元及股；附带源与周期以避免跨口径混用。
#[derive(Debug, Clone, Serialize)]
pub struct CanonicalBar {
    pub bar: Bar,
    pub period: Period,
    pub source: BarSource,
    pub price_basis: PriceBasis,
}

impl CanonicalBar {
    /// TCP适配必须显式传源成交量单位；不假设指数/基金也以100股为手。
    pub fn from_kline(
        value: &KlineData<'_>,
        market: Market,
        period: Period,
        volume_unit: VolumeUnit,
        price_basis: PriceBasis,
    ) -> Result<Self> {
        Ok(Self {
            bar: Bar::from_kline_with_unit(value, market, volume_unit)?,
            period,
            source: BarSource::TdxTcp,
            price_basis,
        })
    }

    pub fn from_day(value: &crate::file::day::Day) -> Result<Self> {
        Ok(Self {
            bar: Bar::from_day(value)?,
            period: Period::Day,
            source: BarSource::TdxDayFile,
            price_basis: PriceBasis::Raw,
        })
    }

    /// lc文件不包含交易所；调用方须提供带市场的证券代码。
    pub fn from_lc(value: &Lc, symbol: Symbol, period: Period) -> Result<Self> {
        if !matches!(period, Period::Minute1 | Period::Minute5) {
            return Err(Error::Custom("lc文件只支持1分钟或5分钟周期"));
        }
        if symbol.code().parse::<u32>().ok() != Some(value.code) {
            return Err(Error::Custom("分钟文件证券代码与标准代码不一致"));
        }
        let bar = Bar {
            symbol,
            datetime: value.try_datetime()?,
            open: f64::from(value.open),
            high: f64::from(value.high),
            low: f64::from(value.low),
            close: f64::from(value.close),
            volume_shares: f64::from(value.vol),
            amount: f64::from(value.amount),
        };
        bar.validate()?;
        Ok(Self {
            bar,
            period,
            source: BarSource::TdxMinuteFile,
            price_basis: PriceBasis::Raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_keep_market_period_and_volume() {
        let lc = Lc {
            date: (2025 - 2004) * 2048 + 106,
            min: 570,
            code: 1,
            open: 10.0,
            high: 11.0,
            low: 9.0,
            close: 10.5,
            amount: 1000.0,
            vol: 123,
        };
        let stock =
            CanonicalBar::from_lc(&lc, "sz000001".parse().unwrap(), Period::Minute1).unwrap();
        let index =
            CanonicalBar::from_lc(&lc, "sh000001".parse().unwrap(), Period::Minute1).unwrap();
        assert_ne!(stock.bar.symbol, index.bar.symbol);
        assert_eq!(stock.bar.volume_shares, 123.0);
        assert!(CanonicalBar::from_lc(&lc, "sz000002".parse().unwrap(), Period::Minute1).is_err());
        assert!(CanonicalBar::from_lc(&lc, "sz000001".parse().unwrap(), Period::Day).is_err());
        let kline = KlineData {
            dt: crate::tcp::helper::DateTime {
                year: 2025,
                month: 1,
                day: 6,
                hour: 15,
                minute: 0,
            },
            code: "000001",
            open: 10.0,
            high: 11.0,
            low: 9.0,
            close: 10.5,
            vol: 12.5,
            amount: 1000.0,
        };
        assert_eq!(
            Bar::from_kline(&kline, Market::Shenzhen)
                .unwrap()
                .volume_shares,
            1250.0
        );
        assert!(Bar::from_kline(&kline, Market::Shanghai).is_err());
        assert_eq!(
            CanonicalBar::from_kline(
                &kline,
                Market::Shanghai,
                Period::Day,
                VolumeUnit::Shares,
                PriceBasis::Raw
            )
            .unwrap()
            .bar
            .volume_shares,
            12.5
        );
        for value in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
            assert!(PriceMultiplier::new(value).is_err());
            assert!(ReturnIndex::new(value).is_err());
        }
        for category in 0..=11 {
            assert!(Period::try_from(category).is_ok());
        }
        assert!(Period::try_from(12).is_err());
    }
}
