use super::Client;
use crate::tcp::stock::*;
use crate::tcp::{
    Tcp,
    governance::{deadline_after, remaining},
    network_policy,
};
use std::{
    io,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

pub(super) fn validate_batch_len(len: usize) -> io::Result<()> {
    if len > 100_000 {
        return Err(super::super::command_request::input("批次超过10万项"));
    }
    Ok(())
}

impl Client {
    pub fn special_limits(&mut self, start: u16) -> io::Result<SpecialLimitPage> {
        SpecialLimitsRequest::new(start).recv(&mut self.tcp)
    }
    pub fn special_limits_all(
        &mut self,
        options: CommandScanOptions,
    ) -> io::Result<SpecialLimitCollection> {
        super::super::special_limits::scan(&mut self.tcp, options)
    }
    pub fn intraday_aux(
        &mut self,
        market: u16,
        code: &str,
        kind: IntradayAuxKind,
    ) -> io::Result<IntradayAuxSeries> {
        IntradayAuxRequest::new(market, code, kind)?.recv(&mut self.tcp)
    }
    pub fn sparkline(
        &mut self,
        market: u16,
        code: &str,
        options: SparklineOptions,
    ) -> io::Result<SparklineSeries> {
        SparklineRequest::new(market, code, options)?.recv(&mut self.tcp)
    }
    pub fn recent_minutes(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
    ) -> io::Result<MinuteSeries> {
        let request = RecentMinutesRequest::new(market, code, date)?;
        let deadline = deadline_after(network_policy().request_timeout)?;
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(
                tcp,
                &self.quote_metadata,
                &[market],
                false,
            )
        })?;
        self.tcp
            .with_deadline(deadline, |tcp| request.recv(tcp, &metadata))
    }
    pub fn auction_series(
        &mut self,
        market: u16,
        code: &str,
        date: Option<u32>,
        options: AuctionOptions,
    ) -> io::Result<AuctionSeries> {
        AuctionSeriesRequest::new(market, code, date, options)?.recv(&mut self.tcp)
    }
    pub fn auction_series_all(
        &mut self,
        market: u16,
        code: &str,
        date: Option<u32>,
        page: AuctionOptions,
        scan: CommandScanOptions,
    ) -> io::Result<AuctionCollection> {
        super::super::auctions::scan(
            &mut self.tcp,
            AuctionSeriesRequest::new(market, code, date, page)?,
            scan,
        )
    }
    pub fn money_flow_daily(&mut self, market: u16, code: &str) -> io::Result<MoneyFlowBlock> {
        MoneyFlowRequest::new(market, code)?.recv(&mut self.tcp)
    }
    pub fn snapshots(&mut self, stocks: &[(u16, &str)]) -> io::Result<SnapshotPage> {
        let request = SnapshotsRequest::new(stocks)?;
        let deadline = deadline_after(network_policy().request_timeout)?;
        let markets: Vec<_> = stocks.iter().map(|s| s.0).collect();
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(
                tcp,
                &self.quote_metadata,
                &markets,
                false,
            )
        })?;
        self.tcp
            .with_deadline(deadline, |tcp| request.recv(tcp, &metadata))
    }
    pub fn category_quotes(
        &mut self,
        category: QuoteCategory,
        sort: QuoteSortField,
        options: CategoryQuoteOptions,
    ) -> io::Result<CategoryQuotePage> {
        let request = CategoryQuotesRequest::new(category, sort, options)?;
        let deadline = deadline_after(network_policy().request_timeout)?;
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(tcp, &self.quote_metadata, &[0, 1], false)
        })?;
        self.tcp
            .with_deadline(deadline, |tcp| request.recv(tcp, &metadata))
    }
    pub fn category_quotes_all(
        &mut self,
        category: QuoteCategory,
        sort: QuoteSortField,
        page: CategoryQuoteOptions,
        scan: CommandScanOptions,
    ) -> io::Result<CategoryQuoteCollection> {
        let request = CategoryQuotesRequest::new(category, sort, page)?;
        scan.validate()?;
        let deadline = deadline_after(scan.timeout)?;
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(tcp, &self.quote_metadata, &[0, 1], false)
        })?;
        self.tcp.with_deadline(deadline, |tcp| {
            super::super::category_quotes::scan(tcp, request, &metadata, scan)
        })
    }
    /// Opens one additional managed connection, retaining the client's query socket.
    pub fn subscribe_depth(
        &mut self,
        stocks: &[(u16, &str)],
        options: SubscriptionOptions,
    ) -> io::Result<QuoteSubscription> {
        let request = RefreshRequest::from_stocks(stocks)?;
        options.validate()?;
        let deadline = deadline_after(network_policy().request_timeout)?;
        let markets: Vec<_> = stocks.iter().map(|s| s.0).collect();
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(
                tcp,
                &self.quote_metadata,
                &markets,
                false,
            )
        })?;
        let tcp = Tcp::with_config_until(self.tcp.config(), deadline)?;
        QuoteSubscription::from_tcp(tcp, request, metadata, options)
    }
    /// Per-fragment results in input order. A failed fragment remains an error.
    pub fn snapshots_batch(
        &mut self,
        stocks: &[(u16, &str)],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<SnapshotPage>>> {
        validate_batch_len(stocks.len())?;
        if stocks.is_empty() {
            return Ok(Vec::new());
        }
        let chunks: Vec<_> = stocks
            .chunks(SNAPSHOT_BATCH_SIZE)
            .map(SnapshotsRequest::new)
            .collect::<io::Result<_>>()?;
        let deadline = deadline_after(network_policy().batch_timeout)?;
        let markets: Vec<_> = stocks.iter().map(|s| s.0).collect();
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(
                tcp,
                &self.quote_metadata,
                &markets,
                false,
            )
        })?;
        self.command_batch(&chunks, max_parallel, deadline, |request, tcp| {
            request.recv(tcp, &metadata)
        })
    }
    pub fn intraday_aux_batch(
        &self,
        stocks: &[(u16, &str)],
        kind: IntradayAuxKind,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<IntradayAuxSeries>>> {
        validate_batch_len(stocks.len())?;
        let requests = stocks
            .iter()
            .map(|&(m, c)| IntradayAuxRequest::new(m, c, kind))
            .collect::<io::Result<Vec<_>>>()?;
        self.command_batch(
            &requests,
            max_parallel,
            deadline_after(network_policy().batch_timeout)?,
            |r, tcp| r.recv(tcp),
        )
    }
    pub fn sparkline_batch(
        &self,
        stocks: &[(u16, &str)],
        options: SparklineOptions,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<SparklineSeries>>> {
        validate_batch_len(stocks.len())?;
        let requests = stocks
            .iter()
            .map(|&(m, c)| SparklineRequest::new(m, c, options))
            .collect::<io::Result<Vec<_>>>()?;
        self.command_batch(
            &requests,
            max_parallel,
            deadline_after(network_policy().batch_timeout)?,
            |r, tcp| r.recv(tcp),
        )
    }
    pub fn recent_minutes_batch(
        &mut self,
        stocks: &[(u16, &str)],
        date: u32,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<MinuteSeries>>> {
        validate_batch_len(stocks.len())?;
        if stocks.is_empty() {
            return Ok(Vec::new());
        }
        let requests = stocks
            .iter()
            .map(|&(m, c)| RecentMinutesRequest::new(m, c, date))
            .collect::<io::Result<Vec<_>>>()?;
        let deadline = deadline_after(network_policy().batch_timeout)?;
        let markets: Vec<_> = stocks.iter().map(|s| s.0).collect();
        let metadata = self.tcp.with_deadline(deadline, |tcp| {
            super::super::quote_metadata::ensure_metadata(
                tcp,
                &self.quote_metadata,
                &markets,
                false,
            )
        })?;
        self.command_batch(&requests, max_parallel, deadline, |r, tcp| {
            r.recv(tcp, &metadata)
        })
    }
    pub fn money_flow_daily_batch(
        &self,
        stocks: &[(u16, &str)],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<MoneyFlowBlock>>> {
        validate_batch_len(stocks.len())?;
        let requests = stocks
            .iter()
            .map(|&(m, c)| MoneyFlowRequest::new(m, c))
            .collect::<io::Result<Vec<_>>>()?;
        self.command_batch(
            &requests,
            max_parallel,
            deadline_after(network_policy().batch_timeout)?,
            |r, tcp| r.recv(tcp),
        )
    }
    pub fn auction_series_batch(
        &self,
        stocks: &[(u16, &str)],
        date: Option<u32>,
        options: AuctionOptions,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<AuctionSeries>>> {
        validate_batch_len(stocks.len())?;
        let requests = stocks
            .iter()
            .map(|&(m, c)| AuctionSeriesRequest::new(m, c, date, options))
            .collect::<io::Result<Vec<_>>>()?;
        self.command_batch(
            &requests,
            max_parallel,
            deadline_after(network_policy().batch_timeout)?,
            |r, tcp| r.recv(tcp),
        )
    }
    pub(super) fn command_batch<R: Sync, T: Send>(
        &self,
        requests: &[R],
        parallel: usize,
        deadline: Instant,
        run: impl Fn(&R, &mut Tcp) -> io::Result<T> + Sync,
    ) -> io::Result<Vec<io::Result<T>>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        validate_batch_len(requests.len())?;
        let _guard = self.batch_lock.acquire(deadline)?;
        let budget = network_policy().max_connections.saturating_sub(1);
        if budget == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "批量查询需要至少两个连接许可",
            ));
        }
        let workers = if parallel == 0 {
            std::thread::available_parallelism().map_or(4, usize::from)
        } else {
            parallel
        };
        let workers = workers.max(1).min(budget).min(requests.len());
        let pool = self.batch_pool(workers)?;
        let next = AtomicUsize::new(0);
        let results = Mutex::new((0..requests.len()).map(|_| None).collect::<Vec<_>>());
        let context = crate::operation::OperationContext::current();
        std::thread::scope(|scope| -> io::Result<()> {
            let mut handles = Vec::new();
            for _ in 0..workers {
                let pool = &pool;
                let next = &next;
                let results = &results;
                let run = &run;
                let context = context.clone();
                handles.push(
                    std::thread::Builder::new()
                        .name("rustdx-command-batch".into())
                        .spawn_scoped(scope, move || -> io::Result<()> {
                            let _guard = context
                                .as_ref()
                                .map(crate::operation::OperationContext::enter);
                            let mut connection =
                                remaining(deadline).and_then(|t| pool.get_connection_timeout(t));
                            loop {
                                let i = next.fetch_add(1, Ordering::Relaxed);
                                let Some(request) = requests.get(i) else {
                                    break;
                                };
                                let result = remaining(deadline).and_then(|_| {
                                    let connection = connection
                                        .as_mut()
                                        .map_err(|e| io::Error::new(e.kind(), e.to_string()))?;
                                    connection.execute(|tcp| {
                                        tcp.with_deadline(deadline, |tcp| run(request, tcp))
                                    })
                                });
                                results
                                    .lock()
                                    .map_err(|_| io::Error::other("批量结果锁损坏"))?[i] =
                                    Some(result);
                            }
                            Ok(())
                        })?,
                );
            }
            for handle in handles {
                handle
                    .join()
                    .map_err(|_| io::Error::other("批量线程异常"))??;
            }
            Ok(())
        })?;
        Ok(results
            .into_inner()
            .map_err(|_| io::Error::other("批量结果锁损坏"))?
            .into_iter()
            .map(|r| r.unwrap_or_else(|| Err(io::Error::other("批量项未执行"))))
            .collect())
    }
}
