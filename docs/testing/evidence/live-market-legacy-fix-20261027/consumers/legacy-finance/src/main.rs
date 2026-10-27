use rustdx_complete::tcp::stock::{Client, FinanceInfoData};
fn legacy(client: &mut Client) -> std::io::Result<FinanceInfoData> {
    client.finance(0, "000001")
}
fn main() { let _ = legacy; }
