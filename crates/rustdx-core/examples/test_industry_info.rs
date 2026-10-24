//! 查询原始行业、省份编码；名称需使用同源且带版本的字典解析。
use rustdx_complete::tcp::stock::FinanceInfo;
use rustdx_complete::tcp::{Tcp, Tdx};

fn main() -> std::io::Result<()> {
    let mut tcp = Tcp::new()?;
    for (market, code) in [(0, "000001"), (1, "600000"), (1, "600519")] {
        let mut finance = FinanceInfo::new(market, code);
        for info in finance.recv_parsed(&mut tcp)? {
            println!(
                "{code}: industry={}, province={}",
                info.raw().industry(),
                info.raw().province()
            );
        }
    }
    Ok(())
}
