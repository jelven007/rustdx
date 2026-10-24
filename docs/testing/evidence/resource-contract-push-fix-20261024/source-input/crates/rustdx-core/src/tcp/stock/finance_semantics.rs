//! Versioned finance semantics. Candidate conversions require explicit access.
use super::{FinanceRaw, FinanceRequest, decode_finance_raw};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::{collections::BTreeMap, io, net::SocketAddr};

pub const FINANCE_SEMANTIC_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceField {
    CirculatingShares,
    TotalShares,
    Slot01,
    Slot02,
    Slot03,
    Slot04,
    Slot05,
    ReportedEps,
    TotalAssets,
    CurrentAssets,
    FixedAssets,
    IntangibleAssets,
    ShareholderCount,
    CurrentLiabilities,
    LongTermLiabilities,
    CapitalReserve,
    NetAssets,
    OperatingRevenue,
    Slot17,
    Receivables,
    OperatingProfit,
    InvestmentIncome,
    OperatingCashFlow,
    CashFlowNetChange,
    Inventory,
    ProfitBeforeTax,
    AfterTaxProfit,
    NetProfit,
    RetainedProfit,
    NetAssetsPerShare,
    Slot29,
}

impl FinanceField {
    pub const ALL: [Self; 31] = [
        Self::CirculatingShares,
        Self::TotalShares,
        Self::Slot01,
        Self::Slot02,
        Self::Slot03,
        Self::Slot04,
        Self::Slot05,
        Self::ReportedEps,
        Self::TotalAssets,
        Self::CurrentAssets,
        Self::FixedAssets,
        Self::IntangibleAssets,
        Self::ShareholderCount,
        Self::CurrentLiabilities,
        Self::LongTermLiabilities,
        Self::CapitalReserve,
        Self::NetAssets,
        Self::OperatingRevenue,
        Self::Slot17,
        Self::Receivables,
        Self::OperatingProfit,
        Self::InvestmentIncome,
        Self::OperatingCashFlow,
        Self::CashFlowNetChange,
        Self::Inventory,
        Self::ProfitBeforeTax,
        Self::AfterTaxProfit,
        Self::NetProfit,
        Self::RetainedProfit,
        Self::NetAssetsPerShare,
        Self::Slot29,
    ];
    pub fn body_offset(self) -> usize {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .expect("all fields registered");
        if index == 0 { 0 } else { 16 + (index - 1) * 4 }
    }
    fn unit(self) -> FinanceUnit {
        match self {
            Self::CirculatingShares | Self::TotalShares => FinanceUnit::Shares,
            Self::ReportedEps | Self::NetAssetsPerShare => FinanceUnit::YuanPerShare,
            Self::ShareholderCount => FinanceUnit::Count,
            Self::Slot01
            | Self::Slot02
            | Self::Slot03
            | Self::Slot04
            | Self::Slot05
            | Self::Slot17
            | Self::Slot29 => FinanceUnit::Unknown,
            _ => FinanceUnit::Yuan,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceUnit {
    Shares,
    Yuan,
    YuanPerShare,
    Count,
    Unknown,
}

impl FinanceUnit {
    fn multiplier(self) -> Option<f64> {
        match self {
            Self::Shares => Some(10_000.0),
            Self::Yuan => Some(1_000.0),
            Self::YuanPerShare | Self::Count => Some(1.0),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceSemanticStatus {
    Verified,
    BaselineCandidate,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceFieldValue {
    field: FinanceField,
    raw_bits: u32,
    unit: FinanceUnit,
    semantic_status: FinanceSemanticStatus,
    normalized_value: Option<f64>,
    evidence: Option<String>,
}

impl FinanceFieldValue {
    pub fn field(&self) -> FinanceField {
        self.field
    }
    pub fn raw_bits(&self) -> u32 {
        self.raw_bits
    }
    pub fn raw_value(&self) -> f32 {
        f32::from_bits(self.raw_bits)
    }
    pub fn unit(&self) -> FinanceUnit {
        self.unit
    }
    pub fn semantic_status(&self) -> FinanceSemanticStatus {
        self.semantic_status
    }
    /// Only independently verified fields have a default business value.
    pub fn value(&self) -> Option<f64> {
        self.normalized_value
    }
    /// Explicitly opt into the baseline's proposed conversion. Not verification.
    pub fn candidate_value(&self) -> Option<f64> {
        self.unit
            .multiplier()
            .map(|scale| f64::from(self.raw_value()) * scale)
    }
    pub fn evidence(&self) -> Option<&str> {
        self.evidence.as_deref()
    }
}

/// A caller-supplied verification scope. Exact raw bits prevent evidence from
/// silently applying to a later or different financial record.
#[derive(Debug, Clone)]
pub struct FinanceProfile {
    market: u16,
    code: String,
    updated_date: u32,
    report_period: NaiveDate,
    evidence: String,
    expected: BTreeMap<FinanceField, u32>,
}

impl FinanceProfile {
    pub fn new(
        market: u16,
        code: &str,
        updated_date: u32,
        report_period: NaiveDate,
        evidence: &str,
    ) -> io::Result<Self> {
        FinanceRequest::new(market, code)?;
        if crate::data::date_from_u32(updated_date).is_err() || evidence.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "财务验证日期或证据为空",
            ));
        }
        Ok(Self {
            market,
            code: code.into(),
            updated_date,
            report_period,
            evidence: evidence.into(),
            expected: BTreeMap::new(),
        })
    }
    /// The caller is responsible for the independent evidence behind this mapping.
    pub fn verify(mut self, field: FinanceField, expected_raw_bits: u32) -> io::Result<Self> {
        if field.unit() == FinanceUnit::Unknown || !f32::from_bits(expected_raw_bits).is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "不能验证未知或非法财务槽",
            ));
        }
        self.expected.insert(field, expected_raw_bits);
        Ok(self)
    }
    fn matches(&self, raw: &FinanceRaw) -> bool {
        raw.market() == self.market
            && raw.code() == self.code
            && raw.updated_date_raw() == self.updated_date
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceSnapshot {
    raw: FinanceRaw,
    pub semantic_schema_version: u32,
    pub profile_version: String,
    pub report_period: Option<NaiveDate>,
    /// Never inferred to be TTM, annualized or basic rather than diluted.
    pub eps_basis: &'static str,
    pub net_profit_basis: &'static str,
    pub net_assets_basis: &'static str,
    pub fetched_at: Option<DateTime<Utc>>,
    pub source_endpoint: Option<SocketAddr>,
    fields: [FinanceFieldValue; 31],
}

impl FinanceSnapshot {
    pub fn raw(&self) -> &FinanceRaw {
        &self.raw
    }
    pub fn code(&self) -> &str {
        self.raw.code()
    }
    pub fn market(&self) -> u16 {
        self.raw.market()
    }
    pub fn field(&self, field: FinanceField) -> &FinanceFieldValue {
        &self.fields[FinanceField::ALL
            .iter()
            .position(|item| *item == field)
            .expect("registered field")]
    }
    pub fn fields(&self) -> &[FinanceFieldValue] {
        &self.fields
    }
    pub fn total_shares(&self) -> Option<f64> {
        self.field(FinanceField::TotalShares).value()
    }
    pub fn circulating_shares(&self) -> Option<f64> {
        self.field(FinanceField::CirculatingShares).value()
    }
    pub fn total_assets_yuan(&self) -> Option<f64> {
        self.field(FinanceField::TotalAssets).value()
    }
    pub fn net_profit_yuan(&self) -> Option<f64> {
        self.field(FinanceField::NetProfit).value()
    }
    pub fn reported_eps(&self) -> Option<f64> {
        self.field(FinanceField::ReportedEps).value()
    }
}

pub fn normalize_finance(raw: FinanceRaw, profile: Option<&FinanceProfile>) -> FinanceSnapshot {
    let scope = profile.filter(|profile| profile.matches(&raw));
    let fields = FinanceField::ALL.map(|field| {
        let raw_bits = raw.u32(field.body_offset());
        let unit = field.unit();
        let verified = scope.is_some_and(|scope| scope.expected.get(&field) == Some(&raw_bits));
        FinanceFieldValue {
            field,
            raw_bits,
            unit,
            semantic_status: if verified {
                FinanceSemanticStatus::Verified
            } else if unit == FinanceUnit::Unknown {
                FinanceSemanticStatus::Unknown
            } else {
                FinanceSemanticStatus::BaselineCandidate
            },
            normalized_value: verified.then(|| {
                f64::from(f32::from_bits(raw_bits)) * unit.multiplier().expect("known unit")
            }),
            evidence: verified.then(|| scope.expect("verified scope").evidence.clone()),
        }
    });
    let any_verified = fields
        .iter()
        .any(|field| field.semantic_status == FinanceSemanticStatus::Verified);
    FinanceSnapshot {
        raw,
        fields,
        semantic_schema_version: FINANCE_SEMANTIC_SCHEMA_VERSION,
        profile_version: "tdx-0010-field-units-v1".into(),
        report_period: scope
            .filter(|_| any_verified)
            .map(|scope| scope.report_period),
        eps_basis: "reported_unspecified",
        net_profit_basis: "unspecified",
        net_assets_basis: "unspecified",
        fetched_at: None,
        source_endpoint: None,
    }
}

/// Default scoped evidence from the preserved Ping An 2026-H1 sample. Numeric
/// equality alone never verifies another symbol, update date or field value.
pub fn decode_finance(
    request: &FinanceRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<FinanceSnapshot>> {
    let mut profile = FinanceProfile::new(
        0,
        "000001",
        20260815,
        NaiveDate::from_ymd_opt(2026, 6, 30).expect("fixed report date"),
        "https://ebank.pingan.com.cn/ir/file/upload/2026-08-15%20平安银行股份有限公司2026年半年度报告摘要.pdf",
    )?;
    // Exact bits from the preserved response, independently checked against
    // issuer disclosures in finance.semantic.expected.json.
    for (field, raw_bits) in [
        (FinanceField::TotalShares, 0x49ec_e37f),
        (FinanceField::ReportedEps, 0x3f9e_b852),
        (FinanceField::TotalAssets, 0x4fb3_abfb),
        (FinanceField::NetAssets, 0x4e02_b454),
        (FinanceField::OperatingRevenue, 0x4c86_b0f5),
        (FinanceField::OperatingProfit, 0x4beb_32f0),
        (FinanceField::OperatingCashFlow, 0x4d4d_0d2a),
        (FinanceField::ProfitBeforeTax, 0x4bea_f27c),
        (FinanceField::NetProfit, 0x4bc4_0b80),
        (FinanceField::NetAssetsPerShare, 0x41c1_0a3d),
    ] {
        profile = profile.verify(field, raw_bits)?;
    }
    Ok(decode_finance_raw(request, bytes)?
        .into_iter()
        .map(|raw| {
            let mut value = normalize_finance(raw, Some(&profile));
            if value.reported_eps().is_some() {
                value.eps_basis = "basic_and_diluted_interim";
            }
            if value.net_profit_yuan().is_some() {
                value.net_profit_basis = "attributable_to_bank_shareholders";
            }
            if value.field(FinanceField::NetAssets).value().is_some() {
                value.net_assets_basis = "consolidated_total_equity";
            }
            value
        })
        .collect())
}
