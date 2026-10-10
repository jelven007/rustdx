use rustdx_complete::tcp::stock::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let concept = std::env::args().nth(1).unwrap_or_else(|| "锂电池".into());
    let mut client = Client::new()?;
    let records = client.block("block_gn.dat")?;
    let stocks: Vec<_> = records
        .iter()
        .filter(|row| row.blockname == concept)
        .collect();
    println!("通达信板块：{concept}，本次获取 {} 只成分股", stocks.len());
    for stock in stocks {
        println!("{}", stock.code);
    }
    Ok(())
}
