use calamine::{Data, Reader, open_workbook_auto};
use eyre::{Result, bail, eyre};

/// 读取调用方提供的交易所Excel名单，不依赖仓库外的assets或在线下载。
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path, exchange] = args.as_slice() else {
        bail!("用法：cargo run -p jelven-rustdx-cli --example excel -- <名单.xls或xlsx> <sh|sz>");
    };
    let column = match exchange.as_str() {
        "sh" => 0,
        "sz" => 4,
        _ => bail!("交易所须为sh或sz"),
    };
    let mut workbook = open_workbook_auto(path)?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| eyre!("工作簿没有工作表"))??;
    let row = range
        .rows()
        .next_back()
        .ok_or_else(|| eyre!("工作表为空"))?;
    let cell = row
        .get(column)
        .ok_or_else(|| eyre!("最后一行缺少代码列{column}"))?;
    let code = match cell {
        Data::String(value) => value.trim().to_owned(),
        Data::Int(value) if (0..=999_999).contains(value) => format!("{value:06}"),
        Data::Float(value)
            if value.is_finite() && value.fract() == 0.0 && (0.0..=999_999.0).contains(value) =>
        {
            format!("{:06}", *value as u32)
        }
        _ => bail!("最后一行代码单元格不是有效六位代码"),
    };
    let symbol = format!("{exchange}{code}").parse::<rustdx_complete::data::Symbol>()?;
    println!(
        "{}：{}行、{}个非空单元格，末行代码{}",
        path,
        range.height(),
        range.used_cells().count(),
        symbol
    );
    Ok(())
}
