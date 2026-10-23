/// 东财数据
pub mod eastmoney;
/// 从交易所获取股票代码
pub mod fetch_code;

pub mod http;
/// 日历、规则及证券状态证据的覆盖告警。
pub mod maintenance;
/// 每任务阶段耗时与私有临时文件逻辑字节观测。
pub mod observation;
/// Bounded TCP request/wire-frame archives and checked offline replay.
pub mod tcp_evidence;

#[macro_use]
extern crate log;
#[macro_use]
extern crate eyre;
