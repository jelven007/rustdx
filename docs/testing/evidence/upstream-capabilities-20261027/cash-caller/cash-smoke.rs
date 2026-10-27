use rustdx_complete::tcp::stock::{FinanceField, FinanceRequest, decode_finance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("finance payload path required")?;
    let bytes = std::fs::read(path)?;
    let snapshot = decode_finance(&FinanceRequest::new(0, "000001")?, &bytes)?
        .remove(0);
    let comparison = snapshot.cash_flow_reconciliation()?;
    assert_eq!(comparison.source_total_excluding_fx_yuan, 8_515_000_000.0);
    assert_eq!(comparison.reported_net_change_yuan, 5_872_000_000.0);
    assert_eq!(
        comparison.source_total_excluding_fx_yuan + comparison.exchange_rate_effect_yuan,
        comparison.reported_net_change_yuan
    );
    assert!(snapshot.require_fields(&[FinanceField::CashFlowNetChange]).is_err());
    assert!(snapshot.field(FinanceField::CashFlowNetChange).candidate_value().is_none());
    println!("{}", serde_json::to_string_pretty(&comparison)?);
    Ok(())
}
