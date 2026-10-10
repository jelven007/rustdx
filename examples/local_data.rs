use rustdx_complete::{data::Bar, file::day::Day, indicators::RollingSma};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "assets/sz000001.day".into());
    let code = args.next().unwrap_or_else(|| "sz000001".into());
    let mut average = RollingSma::new(20);
    for row in Day::from_file_into_vec(&code, path)? {
        let bar = Bar::from_day(&row)?;
        let sma = average.update(bar.close);
        println!(
            "{} {} close={} SMA20={sma:?}",
            bar.symbol, bar.datetime, bar.close
        );
    }
    Ok(())
}
