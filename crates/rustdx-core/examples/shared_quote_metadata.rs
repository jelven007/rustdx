//! One explicit directory lifetime shared by clients in the same process.
use rustdx_complete::tcp::{
    TcpConfig,
    stock::{Client, QuoteMetadataStore},
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(QuoteMetadataStore::default());
    let config = TcpConfig::default();
    let mut first = Client::with_quote_metadata(&config, Arc::clone(&store))?;
    // Prewarm all required markets before latency-sensitive quote requests.
    first.refresh_quote_metadata(&[0, 1])?;
    let mut second = Client::with_quote_metadata(&config, store)?;
    let page = second.quotes_with_metadata(&[(0, "000001"), (1, "600000")])?;
    println!(
        "directory generation={}, records={}",
        page.metadata_generation,
        page.records.len()
    );
    // Each query rechecks freshness. A failed refresh preserves the previous
    // generation; callers still receive errors when its date is stale.
    Ok(())
}
