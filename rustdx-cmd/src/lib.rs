/// 东财数据
pub mod eastmoney;
/// 从交易所获取股票代码
pub mod fetch_code;

pub mod http;
/// 日历、规则及证券状态证据的覆盖告警。
pub mod maintenance;

#[macro_use]
extern crate log;
#[macro_use]
extern crate eyre;
