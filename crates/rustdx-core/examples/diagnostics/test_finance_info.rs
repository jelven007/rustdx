use rustdx_complete::tcp::{
    Tcp, Tdx,
    stock::{FinanceInfo, FinanceSnapshot},
};

fn show(info: &FinanceSnapshot) {
    println!("证券：{}:{}", info.market(), info.code());
    println!(
        "更新日：{:?}，报告期：{:?}",
        info.raw().updated_date(),
        info.report_period
    );
    println!(
        "EPS口径：{}，净利润口径：{}",
        info.eps_basis, info.net_profit_basis
    );
    for field in info.fields() {
        match field.value() {
            Some(value) => println!("{:?}：{value} {:?}（已验证）", field.field(), field.unit()),
            None => println!(
                "{:?}：raw={}，候选值={:?} {:?}，状态={:?}",
                field.field(),
                field.raw_value(),
                field.candidate_value(),
                field.unit(),
                field.semantic_status(),
            ),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tcp = Tcp::new()?;
    for (market, code) in [(0, "000001"), (1, "600000")] {
        let mut request = FinanceInfo::new(market, code);
        for info in request.recv_parsed(&mut tcp)? {
            show(info);
        }
    }
    Ok(())
}
