//! Explicit live query; caller supplies independently established unit/scope evidence.
use rustdx_complete::tcp::stock::{
    Client, TradeAmountContext, TradeCollection, TradeEstimateOptions, TradeQuantityUnit,
    TradeTermination, TransactionPaginationOptions, TurnoverOptions, TurnoverProfile,
    TurnoverScope, reconcile_trade_amounts,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "usage: transaction_amount MARKET CODE YYYYMMDD UNIT_AND_SCOPE_EVIDENCE".into(),
        );
    }
    let market = args[1].parse()?;
    let date = args[3].parse()?;
    let mut client = Client::new()?;
    let raw = client.history_transaction_all(
        market,
        &args[2],
        date,
        TransactionPaginationOptions::default(),
    )?;
    let query = raw
        .event_pages
        .first()
        .ok_or("missing trade event pages")?
        .query()
        .clone();
    let context =
        TradeAmountContext::a_share(query.clone(), TradeQuantityUnit::LotsOf100Shares, &args[4])?;
    let estimates = raw.amount_estimate_summary(&context, TradeEstimateOptions::default())?;
    println!("{}", serde_json::to_string_pretty(&estimates)?);
    let profile = TurnoverProfile::a_share(market, &args[2], date, raw.source_endpoint, &args[4])?;
    let options = TurnoverOptions {
        profile: Some(profile),
        ..Default::default()
    };
    let minute = client.reported_turnover(
        market,
        &args[2],
        date,
        TurnoverScope::Minute1,
        options.clone(),
    )?;
    let daily = client.reported_turnover(
        market,
        &args[2],
        date,
        TurnoverScope::RegularWithAuctions,
        options.clone(),
    )?;
    let combined =
        client.reported_turnover(market, &args[2], date, TurnoverScope::Combined, options)?;
    println!(
        "combined vendor total: {:?}; reason: {:?}",
        combined.reported_amount_yuan(),
        combined.unavailable_reason()
    );
    let trades = TradeCollection::new(query, raw.event_pages, TradeTermination::EmptyPageObserved)?;
    let reconciliation = reconcile_trade_amounts(&trades, &context, &minute, Some(&daily))?;
    println!("{}", serde_json::to_string_pretty(&reconciliation)?);
    Ok(())
}
