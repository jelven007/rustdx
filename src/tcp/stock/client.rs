//! 通达信行情高层客户端。
//!
//! 封装连接管理与各协议请求，
//! 全部返回 owned 数据，开箱即用：
//!
//! ```ignore
//! use rustdx_complete::tcp::stock::Client;
//!
//! let mut client = Client::new()?; // 内置服务器故障转移
//! let quotes = client.quotes(&[(0, "000001"), (1, "600519")])?;
//! for q in quotes {
//!     println!("{}: {:.2} ({:+.2}%)", q.code, q.price, q.change_percent);
//! }
//! ```

use super::{
    BlockRecord, CompanyInfoCategory, CompanyInfoCategoryItem, CompanyInfoContent, FinanceInfoData,
    HistoryMinuteTime, HistoryTransaction, IndexKline, IndexKlineData, Kline, KlineData,
    MinuteTimeData, QuoteData, SecurityListData, TransactionData, Xdxr, XdxrData,
};
use crate::pool::{ConnectionPool, PoolConfig};
use crate::tcp::helper::DateTime;
use crate::tcp::{Tcp, TcpConfig, Tdx};
use chrono::Datelike;
use std::sync::Mutex;

mod batch;

/// 高层客户端：持有连接，提供行情查询与复权计算 API。
#[derive(Debug)]
pub struct Client {
    /// 底层连接（需要更细粒度控制时可直接使用）。
    pub tcp: Tcp,
    batch_lock: batch::BatchGate,
    batch_pool: Mutex<Option<(usize, TcpConfig, ConnectionPool)>>,
}

impl Client {
    /// 连接服务器（内置故障转移），等价于 `Tcp::new()`。
    pub fn new() -> std::io::Result<Self> {
        Self::with_config(&TcpConfig::default())
    }

    /// 以指定配置连接服务器。
    pub fn with_config(config: &TcpConfig) -> std::io::Result<Self> {
        Ok(Self {
            tcp: Tcp::with_config(config)?,
            batch_lock: batch::BatchGate::default(),
            batch_pool: Mutex::new(None),
        })
    }

    /// 重连（见 [`Tcp::reconnect`]）。
    pub fn reconnect(&mut self) -> std::io::Result<()> {
        self.tcp.reconnect()
    }

    /// 心跳（见 [`Tcp::heartbeat`]）。
    pub fn heartbeat(&mut self) -> std::io::Result<u16> {
        self.tcp.heartbeat()
    }

    /// 请求失败时自动重连重试（见 [`Tcp::retry`]）。
    pub fn retry<T>(
        &mut self,
        f: impl FnMut(&mut Tcp) -> std::io::Result<T>,
        attempts: usize,
    ) -> std::io::Result<T> {
        self.tcp.retry(f, attempts)
    }

    /// 实时行情快照。返回 owned 数据。
    ///
    /// [`TcpConfig::recheck_empty`](crate::tcp::TcpConfig::recheck_empty)
    /// 开启时空结果二次确认（等 300ms → 重连 → 重拉一次）——盘中高频轮询
    /// 场景下服务器会静默返回空帧，二次确认可消除抖动误判。
    ///
    /// 注意：非交易时段 quotes 本就为空，业务层应按时段判断后再调用，
    /// 否则每次都会多付一次重查代价。
    pub fn quotes(&mut self, stocks: &[(u16, &str)]) -> std::io::Result<Vec<QuoteData>> {
        if stocks.is_empty() {
            return Ok(Vec::new());
        }
        for &(market, code) in stocks {
            validate_request(market, code, 9)?;
        }
        if stocks.len() > 80 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "单次行情查询最多80只证券",
            ));
        }
        let run = |tcp: &mut Tcp| {
            let mut quotes = SecurityQuotesRef::new(stocks.to_vec());
            quotes.recv_parsed(tcp)?;
            Ok(quotes.result().to_vec())
        };
        let result = if self.tcp.config().recheck_empty {
            recheck_empty(&mut self.tcp, run)
        } else {
            run(&mut self.tcp)
        };
        result.map_err(|e| ctx_err(e, format_args!("Client::quotes(n={})", stocks.len())))
    }

    /// 股票K线。
    ///
    /// 单页请求（最多 `count` 根）；解析层保证按时间**升序**返回。
    /// 区间回补请用 [`Client::bars_range`]（自动翻页 + 窗口过滤）。
    pub fn bars<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        start: u16,
        count: u16,
    ) -> std::io::Result<Vec<super::KlineData<'a>>> {
        validate_request(market, code, category)?;
        if count > 800 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "K线单页上限800",
            ));
        }
        let mut kline = Kline::new(market, code, category, start, count);
        kline.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::bars(market={market}, code={code}, category={category})"),
            )
        })?;
        Ok(kline.result().to_vec())
    }

    /// 按日期区间拉取任意周期 K 线（自动翻页，时间升序，去重）。
    ///
    /// 与 [`Client::k`](Self::k)（仅日线）不同，本方法支持全部 K 线种类
    /// （`category`：0=5m、1=15m、2=30m、3=1h、…、9=日线），适用于
    /// 分钟线等短周期数据的**区间回补**。
    ///
    /// 内置防护（均来自全市场回补实测）：
    /// - 自动翻页直至覆盖 `begin` 或历史尽头（页序无关，末端统一升序）；
    /// - 翻页偏移可能重叠 → 按时间戳去重；
    /// - 分钟线历史深度耗尽时服务器返回**乱码日期**（实测 2004/2035 年）→
    ///   遇不可信年份即终止，且最终按 `[begin, end]` 窗口过滤兜底；
    /// - [`TcpConfig::recheck_empty`](crate::tcp::TcpConfig::recheck_empty)
    ///   开启时空结果二次确认（防服务器静默限流）。
    ///
    /// # 参数
    ///
    /// - `begin`/`end`: 日期区间（YYYYMMDD，闭区间），`None` 表示不设限。
    pub fn bars_range<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::KlineData<'a>>> {
        let ctx =
            format_args!("Client::bars_range(market={market}, code={code}, category={category})");
        fetch_bars_range(&mut self.tcp, market, code, category, begin, end)
            .map_err(|e| ctx_err(e, ctx))
    }

    /// 指数K线，含涨跌家数。
    ///
    /// 股票和指数使用相同命令0x052d，但指数响应包含涨跌家数，
    /// 需使用指数解析器；区间语义请用 [`Client::index_bars_range`]。
    pub fn index_bars<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        start: u16,
        count: u16,
    ) -> std::io::Result<Vec<super::IndexKlineData<'a>>> {
        validate_request(market, code, category)?;
        if count > 800 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "K线单页上限800",
            ));
        }
        let mut kline = IndexKline::new(market, code, category, start, count);
        kline.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::index_bars(market={market}, code={code})"),
            )
        })?;
        Ok(kline.result().to_vec())
    }

    /// 指数K线区间回补：[`Client::index_bars`] 的自动翻页版，
    /// 与 [`Client::bars_range`](Self::bars_range) 对称，使用指数响应解析器。
    ///
    /// - 自动翻页直至覆盖 `begin` 或历史尽头（页序无关，末端统一升序）；
    /// - 翻页偏移可能重叠 → 按时间戳去重；
    /// - 最终按 `[begin, end]` 窗口过滤。
    ///
    /// # 参数
    ///
    /// - `begin`/`end`: 日期区间（YYYYMMDD，闭区间），`None` 表示不设限。
    pub fn index_bars_range<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::IndexKlineData<'a>>> {
        let ctx = format_args!(
            "Client::index_bars_range(market={market}, code={code}, category={category})"
        );
        fetch_index_bars_range(&mut self.tcp, market, code, category, begin, end)
            .map_err(|e| ctx_err(e, ctx))
    }

    /// 按日期区间拉取日K线。
    ///
    /// 自动翻页直到覆盖 `begin`，并过滤出 `[begin, end]`（格式 YYYYMMDD）的数据。
    /// `begin`/`end` 传 `None` 表示不设下/上限。
    ///
    /// [`TcpConfig::recheck_empty`](crate::tcp::TcpConfig::recheck_empty)
    /// 开启时空结果二次确认（防服务器静默限流误判为停牌真空）。
    pub fn k<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::KlineData<'a>>> {
        let run = |tcp: &mut Tcp| fetch_k(tcp, market, code, begin, end);
        let result = if self.tcp.config().recheck_empty {
            recheck_empty(&mut self.tcp, run)
        } else {
            run(&mut self.tcp)
        };
        result.map_err(|e| ctx_err(e, format_args!("Client::k(market={market}, code={code})")))
    }

    /// 批量拉取多只股票日K（内部连接池并行，不占用当前连接）。
    ///
    /// # 参数
    ///
    /// - `stocks`: `(market, code)` 列表（market: 0=深市、1=沪市）；
    /// - `begin`/`end`: 日期区间（YYYYMMDD），`None` 不设限；
    /// - `max_parallel`: 并行连接数上限；传 `0` 按 CPU 核数自动。
    ///   受进程共享的 [`crate::tcp::NetworkPolicy`] 连接预算约束。
    ///   同一 Client 的批次排队执行，批次内并行，避免相互耗尽或关闭连接池。
    ///
    /// # 返回
    ///
    /// 按输入顺序返回；单只股票拉取失败不影响其他股票
    /// （失败信息在对应 [`BatchKline::result`] 中）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use rustdx_complete::tcp::stock::Client;
    ///
    /// let client = Client::new()?;
    /// let rows = client.k_batch(&[(1, "600000"), (0, "000001"), (1, "600519")],
    ///                           Some(20260101), None, 4)?;
    /// for row in rows {
    ///     println!("{}: {} 根", row.code, row.result?.len());
    /// }
    /// ```
    pub fn k_batch<'a>(
        &self,
        stocks: &[(u16, &'a str)],
        begin: Option<u32>,
        end: Option<u32>,
        max_parallel: usize,
    ) -> std::io::Result<Vec<BatchKline<'a>>> {
        Ok(stocks
            .iter()
            .zip(batch::fetch(self, stocks, None, begin, end, max_parallel)?)
            .map(|(&(market, code), result)| BatchKline {
                market,
                code,
                result,
            })
            .collect())
    }

    /// 批量拉取多只股票任意周期 K 线区间（内部连接池并行，不占用当前连接）。
    ///
    /// 分钟线等短周期数据的**全市场回补**配套：每只股票走 [`Client::bars_range`]
    /// 同一套防护（自动翻页、去重、乱码年份终止、窗口过滤、升序），多连接并行。
    ///
    /// # 参数
    ///
    /// - `stocks`: `(market, code)` 列表（market: 0=深市、1=沪市）；
    /// - `category`: K 线种类（0=5m、1=15m、…、9=日线）；
    /// - `begin`/`end`: 日期区间（YYYYMMDD，闭区间），`None` 不设限；
    /// - `max_parallel`: 并行连接数上限；传 `0` 按 CPU 核数自动。
    ///   受进程共享的 [`crate::tcp::NetworkPolicy`] 连接预算约束。
    ///   与同一 Client 的其他批次排队执行，批次内并行。
    ///
    /// 按输入顺序返回；单只失败不影响其他（失败信息在对应 [`BatchBars::result`] 中）。
    pub fn bars_batch<'a>(
        &self,
        stocks: &[(u16, &'a str)],
        category: u16,
        begin: Option<u32>,
        end: Option<u32>,
        max_parallel: usize,
    ) -> std::io::Result<Vec<BatchBars<'a>>> {
        Ok(stocks
            .iter()
            .zip(batch::fetch(
                self,
                stocks,
                Some(category),
                begin,
                end,
                max_parallel,
            )?)
            .map(|(&(market, code), result)| BatchBars {
                market,
                code,
                result,
            })
            .collect())
    }

    /// 按日期区间拉取**复权**日K线（前复权/后复权，本地计算）。
    ///
    /// 通达信服务器返回的 K 线均为不复权数据（实测请求中的复权字段实为
    /// 「取样步长」，见 `tests/adj_field_probe.rs`），因此本方法在本地用
    /// 除权除息事件（[`Client::xdxr`]）计算复权价：
    /// 除权参考价 `(preclose*10 - fh + pg*pgj) / (10 + pg + sg)`，
    /// 除权日价格按 `preclose_adj / preclose` 缩放，逐日累积。
    ///
    /// 复权只调整 OHLC 价格；`vol`/`amount` 保持原始值。
    ///
    /// # 参数
    ///
    /// - `adj`: 前复权（[`Adj::Qfq`]，最新价 = 实际价）或后复权
    ///   （[`Adj::Hfq`]，服务器可用历史的首日价 = 实际价）；
    /// - `begin`/`end`: 日期区间（YYYYMMDD），`None` 不设限。
    pub fn k_adjusted<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        adj: Adj,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::KlineData<'a>>> {
        self.tcp
            .with_timeout(crate::tcp::network_policy().pagination_timeout, |tcp| {
                validate_request(market, code, 9)?;
                validate_range(begin, end)?;
                // 从服务器可用历史首日连续累积，再在本地过滤区间。
                let all = fetch_k_checked(tcp, market, code, None, None).map_err(|e| {
                    ctx_err(
                        e,
                        format_args!("Client::k_adjusted(market={market}, code={code})"),
                    )
                })?;
                if all.is_empty() {
                    return Ok(all);
                }
                let mut xdxrs = Xdxr::new(market, code);
                xdxrs.recv_parsed(tcp).map_err(|e| {
                    ctx_err(
                        e,
                        format_args!("Client::k_adjusted(xdxr, market={market}, code={code})"),
                    )
                })?;
                let multipliers = adjusted_multipliers(&all, xdxrs.result(), adj)?;

                all.into_iter()
                    .zip(multipliers)
                    .filter(|(bar, _)| {
                        let d = DateTime::to_u32(bar.dt);
                        begin.is_none_or(|b| d >= b) && end.is_none_or(|e| d <= e)
                    })
                    .map(|(mut bar, m)| {
                        bar.open *= m;
                        bar.high *= m;
                        bar.low *= m;
                        bar.close *= m;
                        if [bar.open, bar.high, bar.low, bar.close]
                            .iter()
                            .any(|p| !p.is_finite() || *p <= 0.0)
                        {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "复权价格溢出或非法",
                            ));
                        }
                        Ok(bar)
                    })
                    .collect()
            })
    }

    /// 当日分时。
    ///
    /// 2026 新协议已直接支持（响应含 market+code 头与盘口快照，数据块定位
    /// 见 [`super::MinuteTime`] 文档）。若某台服务器解析仍为空（异常响应），
    /// 本方法**自动回退**：「今日历史分时接口」（[`HistoryMinuteTime`] 查询
    /// 当天）协议稳定，作为兜底数据源。
    ///
    /// 两个接口返回相同的数据结构（每分钟一个价格/成交量点）。
    pub fn minute(&mut self, market: u16, code: &str) -> std::io::Result<Vec<MinuteTimeData>> {
        validate_request(market, code, 9)?;
        let ctx = format_args!("Client::minute(market={market}, code={code})");
        let mut mt = super::MinuteTime::new(market, code);
        match mt.recv_parsed(&mut self.tcp) {
            Ok(data) if !data.is_empty() => return Ok(data.to_vec()),
            Err(e)
                if !matches!(
                    e.kind(),
                    std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
                ) =>
            {
                return Err(ctx_err(e, ctx));
            }
            _ => {}
        }

        // 回退：当日分时接口不可用 → 今日历史分时接口（协议稳定）
        let mut hmt = HistoryMinuteTime::new(market, code, today_yyyymmdd());
        hmt.recv_parsed(&mut self.tcp)
            .map_err(|e| ctx_err(e, format_args!("{ctx}（回退到今日历史分时）")))?;
        Ok(hmt.result().to_vec())
    }

    /// 历史分时。
    pub fn history_minute(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
    ) -> std::io::Result<Vec<MinuteTimeData>> {
        validate_request(market, code, 9)?;
        let mut mt = HistoryMinuteTime::new(market, code, date);
        mt.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::history_minute(market={market}, code={code}, date={date})"),
            )
        })?;
        Ok(mt.result().to_vec())
    }

    /// 当日逐笔成交。
    pub fn transaction(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
    ) -> std::io::Result<Vec<TransactionData>> {
        validate_request(market, code, 9)?;
        let mut tx = super::Transaction::new(market, code, start, count);
        tx.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::transaction(market={market}, code={code})"),
            )
        })?;
        Ok(tx.result().to_vec())
    }

    /// 历史逐笔成交。
    pub fn history_transaction(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
        date: u32,
    ) -> std::io::Result<Vec<TransactionData>> {
        validate_request(market, code, 9)?;
        let mut tx = HistoryTransaction::new(market, code, start, count, date);
        tx.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::history_transaction(market={market}, code={code})"),
            )
        })?;
        Ok(tx.result().to_vec())
    }

    /// 财务信息。
    pub fn finance(&mut self, market: u16, code: &str) -> std::io::Result<FinanceInfoData> {
        validate_request(market, code, 9)?;
        let mut fin = super::FinanceInfo::new(market as u8, code);
        fin.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::finance(market={market}, code={code})"),
            )
        })?;
        fin.result().first().cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Client::finance({market}, {code}): 无财务数据"),
            )
        })
    }

    /// 除权除息信息。
    pub fn xdxr(&mut self, market: u16, code: &str) -> std::io::Result<Vec<super::XdxrData>> {
        validate_request(market, code, 9)?;
        let mut xdxr = Xdxr::new(market, code);
        xdxr.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::xdxr(market={market}, code={code})"),
            )
        })?;
        Ok(xdxr.result().to_vec())
    }

    /// 板块文件，如 `"block_gn.dat"` 概念板块。
    pub fn block(&mut self, block_file: &str) -> crate::Result<Vec<BlockRecord>> {
        super::get_block_info(&mut self.tcp, block_file).map_err(|e| {
            crate::Error::Io(std::io::Error::other(format!(
                "Client::block({block_file}): {e}"
            )))
        })
    }

    /// 证券数量。
    pub fn stock_count(&mut self, market: u16) -> std::io::Result<u16> {
        validate_request(market, "000001", 9)?;
        let mut sc = super::super::basic::SecurityCount::new(market);
        let c = sc
            .recv_parsed(&mut self.tcp)
            .map_err(|e| ctx_err(e, format_args!("Client::stock_count(market={market})")))?;
        Ok(*c)
    }

    /// 全部证券列表，自动分页聚合。
    pub fn stocks(&mut self, market: u16) -> crate::Result<Vec<SecurityListData>> {
        validate_request(market, "000001", 9)?;
        super::stocks(&mut self.tcp, market).map_err(|e| match e {
            crate::Error::Io(error) => crate::Error::Io(ctx_err(
                error,
                format_args!("Client::stocks(market={market})"),
            )),
            other => crate::Error::Context {
                context: format!("Client::stocks(market={market})"),
                source: Box::new(other),
            },
        })
    }

    /// F10 公司资料全部栏目，返回 `(栏目名, 内容)`。
    pub fn f10(&mut self, market: u16, code: &str) -> std::io::Result<Vec<(String, String)>> {
        self.tcp
            .with_timeout(crate::tcp::network_policy().pagination_timeout, |tcp| {
                validate_request(market, code, 9)?;
                let ctx = format_args!("Client::f10(market={market}, code={code})");
                let mut cat = CompanyInfoCategory::new(market, code);
                cat.recv_parsed(tcp).map_err(|e| ctx_err(e, ctx))?;
                let mut result = Vec::with_capacity(cat.result().len());
                for item in cat.result() {
                    let mut content = CompanyInfoContent::new(
                        market,
                        code,
                        &item.filename,
                        item.start,
                        item.length,
                    );
                    content
                        .recv_parsed(tcp)
                        .map_err(|e| ctx_err(e, format_args!("{ctx}（栏目 {}）", item.name)))?;
                    result.push((item.name.clone(), std::mem::take(&mut content.data)));
                }
                Ok(result)
            })
    }

    /// F10 公司资料栏目目录。
    pub fn f10_categories(
        &mut self,
        market: u16,
        code: &str,
    ) -> std::io::Result<Vec<CompanyInfoCategoryItem>> {
        validate_request(market, code, 9)?;
        let mut cat = CompanyInfoCategory::new(market, code);
        cat.recv_parsed(&mut self.tcp).map_err(|e| {
            ctx_err(
                e,
                format_args!("Client::f10_categories(market={market}, code={code})"),
            )
        })?;
        Ok(cat.result().to_vec())
    }

    fn batch_pool(&self, workers: usize) -> std::io::Result<ConnectionPool> {
        let config = self.tcp.config();
        let mut slot = self.batch_pool.lock().unwrap();
        if let Some((size, saved, pool)) = slot.as_ref()
            && *size == workers
            && saved == config
        {
            return Ok(pool.clone());
        }
        if let Some((_, _, pool)) = slot.take() {
            pool.close();
        }
        let pool = ConnectionPool::with_tcp_config(
            PoolConfig {
                max_size: workers,
                // 批次完成释放全局许可，避免多个 Client 的空闲池相互饿死。
                max_idle: 0,
                ..Default::default()
            },
            config.clone(),
        )?;
        *slot = Some((workers, config.clone(), pool.clone()));
        Ok(pool)
    }
}

fn validate_request(market: u16, code: &str, category: u16) -> std::io::Result<()> {
    if market > 1
        || code.len() != 6
        || !code.bytes().all(|b| b.is_ascii_alphanumeric())
        || category > 11
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "市场、六位ASCII代码或K线周期非法",
        ));
    }
    Ok(())
}

fn validate_range(begin: Option<u32>, end: Option<u32>) -> std::io::Result<()> {
    for date in [begin, end].into_iter().flatten() {
        if chrono::NaiveDate::from_ymd_opt((date / 10000) as i32, date / 100 % 100, date % 100)
            .is_none()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "日期必须是合法YYYYMMDD",
            ));
        }
    }
    if begin.zip(end).is_some_and(|(b, e)| b > e) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "开始日期晚于结束日期",
        ));
    }
    Ok(())
}

fn fetch_k_checked<'a>(
    tcp: &mut Tcp,
    market: u16,
    code: &'a str,
    begin: Option<u32>,
    end: Option<u32>,
) -> std::io::Result<Vec<KlineData<'a>>> {
    if tcp.config().recheck_empty {
        recheck_empty(tcp, |tcp| fetch_k(tcp, market, code, begin, end))
    } else {
        fetch_k(tcp, market, code, begin, end)
    }
}

/// 今日日期，格式 YYYYMMDD（用于当日分时回退到今日历史分时）。
fn today_yyyymmdd() -> u32 {
    let today = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .date_naive();
    (today.year() * 10000 + today.month() as i32 * 100 + today.day() as i32) as u32
}

/// 给 IO 错误附加接口上下文（保留错误 kind，便于调用方按 kind 分支处理）。
fn ctx_err(e: std::io::Error, ctx: std::fmt::Arguments<'_>) -> std::io::Error {
    std::io::Error::new(e.kind(), format!("{ctx}: {e}"))
}

/// 避免与 facade 文档中的 `SecurityQuotes` 混淆的内部别名。
use super::SecurityQuotes as SecurityQuotesRef;

/// 空结果二次确认：TDX 服务端在高频请求下会**静默返回空响应**（不报错），
/// 与停牌/退市股「窗口内本就无 K 线」的真空无法从返回值区分。
/// 结果为空时：等待 300ms 并重连，重拉一次，仍空才返回空。
fn recheck_empty<T>(
    tcp: &mut Tcp,
    mut fetch: impl FnMut(&mut Tcp) -> std::io::Result<Vec<T>>,
) -> std::io::Result<Vec<T>> {
    tcp.with_timeout(crate::tcp::network_policy().pagination_timeout, |tcp| {
        let first = fetch(tcp)?;
        if !first.is_empty() {
            return Ok(first);
        }
        tcp.delay(std::time::Duration::from_millis(300))?;
        tcp.reconnect()?;
        fetch(tcp)
    })
}

/// 区间翻页页大小（协议单页上限）。
const BARS_PAGE: u16 = 800;
/// 区间翻页页数上限：分钟线历史深度有限（约近年），超深分页服务器会返回
/// 乱码日期；最多82页，下一页偏移不可超出协议的u16范围。
const MAX_BARS_PAGES: usize = 82;

/// 从给定连接按日期区间拉取任意周期 K 线：自动翻页、去重、窗口过滤、升序。
///
/// 与 `Client::bars_range` 共用逻辑（`Client::k` 的日K专用版见 [`fetch_k`]）。
fn fetch_bars_range<'a>(
    tcp: &mut Tcp,
    market: u16,
    code: &'a str,
    category: u16,
    begin: Option<u32>,
    end: Option<u32>,
) -> std::io::Result<Vec<KlineData<'a>>> {
    tcp.with_timeout(crate::tcp::network_policy().pagination_timeout, |tcp| {
        validate_request(market, code, category)?;
        validate_range(begin, end)?;
        let mut all: Vec<KlineData<'a>> = Vec::new();
        let mut start = 0u16;
        let mut pages = 0usize;
        let mut previous_oldest = None;
        loop {
            let kline = Kline::new(market, code, category, start, BARS_PAGE);
            let run = |tcp: &mut Tcp| {
                let mut kline = kline.clone();
                kline.recv_parsed(tcp)?;
                Ok(kline.result().to_vec())
            };
            let bars = if tcp.config().recheck_empty {
                recheck_empty(tcp, run)?
            } else {
                run(tcp)?
            };
            validate_bar_dates(bars.iter().map(|b| b.dt))?;
            let n = bars.len();
            // 页内最旧一根（顺序无关；乱码日期页由下方年份校验拦截）
            let oldest = bars.iter().map(|b| b.dt).min();
            if n > 0 && previous_oldest.is_some_and(|prev| oldest >= Some(prev)) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "K线分页未向历史方向推进",
                ));
            }
            previous_oldest = oldest;
            all.extend(bars);
            pages += 1;
            if n < BARS_PAGE as usize {
                break; // 历史尽头
            }
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(begin) = begin {
                // 页内最旧数据已早于 begin，停止翻页（min 判停，顺序无关）
                if DateTime::to_u32(oldest) < begin {
                    break;
                }
            }
            if pages >= MAX_BARS_PAGES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "K线分页超过协议偏移上限",
                ));
            }
            start = start.checked_add(BARS_PAGE).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "K线分页偏移溢出")
            })?;
        }

        // 升序 + 翻页偏移重叠去重（排序后重复时间戳相邻）+ 窗口过滤（丢弃乱码日期页残留）
        all.sort_by_key(|b| b.dt);
        all.dedup_by_key(|b| b.dt);
        all.retain(|bar| {
            let d = DateTime::to_u32(bar.dt);
            begin.is_none_or(|b| d >= b) && end.is_none_or(|e| d <= e)
        });
        Ok(all)
    })
}

/// 从给定连接按日期区间拉取指数K线：自动翻页、去重、窗口过滤、升序。
///
/// 与 [`fetch_bars_range`] 对称，使用指数响应解析器；
/// 翻页判停/去重/乱码年份终止/窗口过滤同一套约定。
fn fetch_index_bars_range<'a>(
    tcp: &mut Tcp,
    market: u16,
    code: &'a str,
    category: u16,
    begin: Option<u32>,
    end: Option<u32>,
) -> std::io::Result<Vec<IndexKlineData<'a>>> {
    tcp.with_timeout(crate::tcp::network_policy().pagination_timeout, |tcp| {
        validate_request(market, code, category)?;
        validate_range(begin, end)?;
        let mut all: Vec<IndexKlineData<'a>> = Vec::new();
        let mut start = 0u16;
        let mut pages = 0usize;
        let mut previous_oldest = None;
        loop {
            let kline = IndexKline::new(market, code, category, start, BARS_PAGE);
            let run = |tcp: &mut Tcp| {
                let mut kline = kline.clone();
                kline.recv_parsed(tcp)?;
                Ok(kline.result().to_vec())
            };
            let bars = if tcp.config().recheck_empty {
                recheck_empty(tcp, run)?
            } else {
                run(tcp)?
            };
            validate_bar_dates(bars.iter().map(|b| b.dt))?;
            let n = bars.len();
            // 页内最旧一根（顺序无关；乱码日期页由下方年份校验拦截）
            let oldest = bars.iter().map(|b| b.dt).min();
            if n > 0 && previous_oldest.is_some_and(|prev| oldest >= Some(prev)) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "指数K线分页未推进",
                ));
            }
            previous_oldest = oldest;
            all.extend(bars);
            pages += 1;
            if n < BARS_PAGE as usize {
                break; // 历史尽头
            }
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(begin) = begin {
                // 页内最旧数据已早于 begin，停止翻页（min 判停，顺序无关）
                if DateTime::to_u32(oldest) < begin {
                    break;
                }
            }
            if pages >= MAX_BARS_PAGES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "指数K线分页超过偏移上限",
                ));
            }
            start = start.checked_add(BARS_PAGE).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "指数K线分页偏移溢出")
            })?;
        }

        // 升序 + 翻页偏移重叠去重 + 窗口过滤
        all.sort_by_key(|b| b.dt);
        all.dedup_by_key(|b| b.dt);
        all.retain(|bar| {
            let d = DateTime::to_u32(bar.dt);
            begin.is_none_or(|b| d >= b) && end.is_none_or(|e| d <= e)
        });
        Ok(all)
    })
}

/// 除权参考价（`category == 1` 除权除息事件）：
/// `(前收盘 × 10 − 每10股派现 + 配股价 × 每10股配股) ÷ (10 + 每10股送转 + 每10股配股)`，
/// **四舍五入到分**（与交易所/行情软件的除权日昨收一致）。
///
/// 输入或计算结果非法时原样返回 `preclose`（兼容接口）；
/// [`Client::k_adjusted`] 会将这类错误返回给调用方。
/// 除权日 `prev_close` 应传本函数结果，随后即可做涨跌停判定
/// （见 [`crate::limit`]）与日线 `prev_close` 链构建。
pub fn ex_right_reference(preclose: f64, ev: &XdxrData) -> f64 {
    checked_ex_right_reference(preclose, ev).unwrap_or(preclose)
}

fn checked_ex_right_reference(preclose: f64, ev: &XdxrData) -> Option<f64> {
    if !preclose.is_finite()
        || preclose <= 0.0
        || [ev.fh_qltp, ev.pgj_qzgb, ev.sg_hltp, ev.pg_hzgb]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
    {
        return None;
    }
    let numer = preclose * 10.0 - ev.fh_qltp as f64 + ev.pgj_qzgb as f64 * ev.pg_hzgb as f64;
    let denom = 10.0 + ev.sg_hltp as f64 + ev.pg_hzgb as f64;
    if denom > 0.0 {
        let adj = numer / denom;
        if adj.is_finite() && adj > 0.0 {
            let rounded = (adj * 100.0).round() / 100.0;
            return (rounded.is_finite() && rounded > 0.0).then_some(rounded);
        }
    }
    None
}

/// 从给定连接拉取单只股票日K：自动翻页、日期区间过滤、按日期升序排序。
///
/// 与 `Client::k` 共用同一套逻辑，供 `k_batch` 在连接池连接上复用。
fn fetch_k<'a>(
    tcp: &mut Tcp,
    market: u16,
    code: &'a str,
    begin: Option<u32>,
    end: Option<u32>,
) -> std::io::Result<Vec<KlineData<'a>>> {
    fetch_bars_range(tcp, market, code, 9, begin, end)
}

fn validate_bar_dates(dates: impl Iterator<Item = DateTime>) -> std::io::Result<()> {
    let today = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .date_naive();
    for dt in dates {
        let date = chrono::NaiveDate::from_ymd_opt(dt.year as i32, dt.month as u32, dt.day as u32);
        if date.is_none_or(|date| date.year() < 1990 || date > today)
            || dt.hour > 23
            || dt.minute > 59
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("K线日期非法: {dt:?}"),
            ));
        }
    }
    Ok(())
}

/// 批量 K 线单项结果。
///
/// `result` 为该股票独立的结果：`Ok` 含按日期升序的日 K 数据，
/// `Err` 为该股票拉取失败（不影响其他股票）。
#[derive(Debug)]
pub struct BatchKline<'a> {
    /// 市场代码（0=深市、1=沪市）
    pub market: u16,
    /// 6 位股票代码
    pub code: &'a str,
    /// 该股票拉取结果
    pub result: std::io::Result<Vec<KlineData<'a>>>,
}

/// [`Client::bars_batch`] 单项结果（结构同 [`BatchKline`]，任意周期）。
#[derive(Debug)]
pub struct BatchBars<'a> {
    /// 市场代码（0=深市、1=沪市）
    pub market: u16,
    /// 6 位股票代码
    pub code: &'a str,
    /// 该股票拉取结果（按时间升序）
    pub result: std::io::Result<Vec<KlineData<'a>>>,
}

/// 复权方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adj {
    /// 前复权：以最新价为基准，历史价按比例缩放（最新价 = 实际价）。
    Qfq,
    /// 后复权：以服务器可用历史首日为基准（首日价 = 实际价）。
    Hfq,
}

/// 计算每根 K 线的复权乘数（基于收盘价序列与除权除息事件）。
///
/// 算法：
/// - 除权参考价 `preclose_adj = (preclose*10 - fh + pg*pgj) / (10 + pg + sg)`
///   （`fh`=每 10 股派现、`sg`=每 10 股送转、`pg`=每 10 股配股、`pgj`=配股价）；
/// - 除权日价格缩放乘数 `scale *= preclose_adj / preclose`，逐日累积；
///   停牌期间连续事件以前一次事件的除权参考价继续计算；
/// - 后复权乘数 `= 1/scale`（除权后价格放大回除权前量纲，首日价不变）；
/// - 前复权乘数 `= scale[last]/scale`（以最新价为基准，最新价不变）。
///
/// 只使用 `category == 1`（除权除息）事件；增发/回购/转配等不改变价格基准。
fn adjusted_multipliers(
    days: &[KlineData<'_>],
    xdxrs: &[XdxrData],
    adj: Adj,
) -> std::io::Result<Vec<f64>> {
    let Some(first) = days.first() else {
        return Ok(Vec::new());
    };
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "复权价格、事件或累计因子非法",
        )
    };
    let first_date = DateTime::to_u32(first.dt);
    let mut events: Vec<&XdxrData> = xdxrs
        .iter()
        // 首日收盘已包含此前事件，缺少更早价格时不能重复应用。
        .filter(|x| x.category == 1 && x.date > first_date)
        .collect();
    events.sort_by_key(|e| e.date);
    let mut ev = events.iter().peekable();

    let mut scales: Vec<f64> = Vec::with_capacity(days.len());
    let mut scale = 1.0f64;
    let mut preclose = first.close;
    for d in days {
        if [d.open, d.high, d.low, d.close]
            .iter()
            .any(|p| !p.is_finite() || *p <= 0.0)
        {
            return Err(invalid());
        }
        let date = DateTime::to_u32(d.dt);
        // 应用所有日期 ≤ 当前交易日且尚未应用的除权事件（含停牌场景：
        // 除权日无 K 线时顺延到下一个交易日）
        while let Some(&event) = ev.peek() {
            if event.date > date {
                break;
            }
            let preclose_adj = checked_ex_right_reference(preclose, event).ok_or_else(invalid)?;
            scale *= preclose_adj / preclose;
            if !scale.is_finite() || scale <= 0.0 {
                return Err(invalid());
            }
            preclose = preclose_adj;
            ev.next();
        }
        scales.push(scale);
        preclose = d.close;
    }

    let last = *scales.last().unwrap_or(&1.0);
    scales
        .into_iter()
        .map(|s| match adj {
            Adj::Hfq => 1.0 / s,
            Adj::Qfq => last / s,
        })
        .map(|m| {
            if m.is_finite() && m > 0.0 {
                Ok(m)
            } else {
                Err(invalid())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn overlapping_batches_wait_and_preserve_results_when_pool_size_changes() {
        use super::Client;
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        use std::time::Duration;

        for second_workers in [1, 2] {
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let release_rx = Mutex::new(release_rx);
            let first = AtomicBool::new(true);
            let (config, server) = crate::tcp::tests::mock_server(
                if second_workers == 1 { 3 } else { 4 },
                move |request| {
                    if request.get(14..20) == Some(b"600000".as_slice())
                        && first.swap(false, Ordering::SeqCst)
                    {
                        started_tx.send(()).unwrap();
                        release_rx.lock().unwrap().recv().unwrap();
                    }
                    vec![0, 0]
                },
            );
            let client = Client::with_config(&config).unwrap();
            std::thread::scope(|scope| {
                let first = scope.spawn(|| client.k_batch(&[(1, "600000")], None, None, 1));
                started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                let (done_tx, done_rx) = mpsc::channel();
                let client = &client;
                let second = scope.spawn(move || {
                    let result = client.bars_batch(
                        &[(0, "000001"), (1, "600519")],
                        9,
                        None,
                        None,
                        second_workers,
                    );
                    done_tx.send(()).unwrap();
                    result
                });
                let pending = done_rx.recv_timeout(Duration::from_millis(100));
                release_tx.send(()).unwrap();
                assert!(first.join().unwrap().unwrap()[0].result.is_ok());
                assert!(
                    second
                        .join()
                        .unwrap()
                        .unwrap()
                        .iter()
                        .all(|row| row.result.is_ok())
                );
                assert!(matches!(pending, Err(mpsc::RecvTimeoutError::Timeout)));
            });
            drop(client);
            server.join().unwrap();
        }
    }

    #[test]
    fn k_range_retain_logic() {
        // 直接测试日期过滤逻辑所依赖的 to_u32 排序
        use crate::tcp::helper::DateTime;
        let d = DateTime {
            year: 2026,
            month: 9,
            day: 1,
            hour: 15,
            minute: 0,
        };
        assert_eq!(d.to_u32(), 20260901);
    }

    #[test]
    fn adjusted_multipliers_no_xdxr() {
        use super::Adj;
        use super::adjusted_multipliers;
        // 无除权：scale 恒 1 → 前/后复权乘数均为 1
        let days = vec![kd(20260101, 10.0), kd(20260102, 11.0), kd(20260105, 12.0)];
        assert_eq!(
            adjusted_multipliers(&days, &[], Adj::Qfq).unwrap(),
            vec![1.0, 1.0, 1.0]
        );
        assert_eq!(
            adjusted_multipliers(&days, &[], Adj::Hfq).unwrap(),
            vec![1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn adjusted_multipliers_split() {
        use super::Adj;
        use super::adjusted_multipliers;
        // 10 送 10：除权参考价 = 前收/2 → scale 减半
        let days = vec![
            kd(20260105, 20.0),
            kd(20260106, 10.0), // 除权日，前收 20 → 参考价 10（10送10）
            kd(20260107, 10.5),
        ];
        let xdxrs = vec![xdxr(20260106, 0.0, 10.0, 0.0, 0.0)]; // 每 10 股送 10
        // 后复权：除权后价格放大回除权前量纲
        let m = adjusted_multipliers(&days, &xdxrs, Adj::Hfq).unwrap();
        assert!((m[0] - 1.0).abs() < 1e-9);
        assert!((m[1] - 2.0).abs() < 1e-9, "除权日乘数应为 2，实际 {m:?}");
        assert!((m[2] - 2.0).abs() < 1e-9);
        // 前复权：最新价不变，历史价按比例缩小
        let m = adjusted_multipliers(&days, &xdxrs, Adj::Qfq).unwrap();
        assert!((m[2] - 1.0).abs() < 1e-9);
        assert!((m[0] - 0.5).abs() < 1e-9, "除权前乘数应为 0.5，实际 {m:?}");
    }

    #[test]
    fn adjusted_multipliers_cash_and_rights() {
        use super::Adj;
        use super::adjusted_multipliers;
        // 每 10 股派 5 元 + 每 10 股配 3 股（配股价 2 元）：
        // 参考价 = (20*10 - 5 + 3*2) / (10 + 3) = 201/13 ≈ 15.4615 → 四舍五入 15.46
        let days = vec![kd(20260105, 20.0), kd(20260106, 15.0), kd(20260107, 15.5)];
        let xdxrs = vec![xdxr(20260106, 5.0, 0.0, 3.0, 2.0)];
        let m = adjusted_multipliers(&days, &xdxrs, Adj::Hfq).unwrap();
        assert!((m[1] - (20.0 / 15.46)).abs() < 1e-9, "实际 {m:?}");
    }

    /// 除权参考价与交易所口径一致（四舍五入到分）。
    #[test]
    fn ex_right_reference_rounds_to_fen() {
        use super::ex_right_reference;
        // (20*10 - 5 + 3*2) / 13 = 201/13 = 15.4615… → 15.46
        let ev = xdxr(20260106, 5.0, 0.0, 3.0, 2.0);
        assert_eq!(ex_right_reference(20.0, &ev), 15.46);
        // 无变动 → 原样返回
        let none = xdxr(20260106, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(ex_right_reference(20.0, &none), 20.0);
    }

    #[test]
    fn adjusted_multipliers_suspended_xdxr_day() {
        use super::Adj;
        use super::adjusted_multipliers;
        // 除权日为停牌日（无 K 线）：事件顺延到下一交易日处理，
        // 仍用除权前最后一个交易日收盘作为 preclose
        let days = vec![kd(20260102, 20.0), kd(20260106, 10.0)];
        let xdxrs = vec![xdxr(20260105, 0.0, 10.0, 0.0, 0.0)]; // 01-05 除权，01-06 才恢复交易
        let m = adjusted_multipliers(&days, &xdxrs, Adj::Hfq).unwrap();
        assert!((m[0] - 1.0).abs() < 1e-9);
        assert!(
            (m[1] - 2.0).abs() < 1e-9,
            "停牌顺延后乘数应为 2，实际 {m:?}"
        );
    }

    #[test]
    fn adjustment_anchors_first_bar_and_chains_suspension_events() {
        use super::{Adj, adjusted_multipliers};
        let days = [kd(20260102, 10.0), kd(20260106, 8.0)];
        let events = [
            xdxr(20260101, 0.0, 10.0, 0.0, 0.0),
            xdxr(20260102, 0.0, 10.0, 0.0, 0.0),
            xdxr(20260103, 10.0, 0.0, 0.0, 0.0),
            xdxr(20260105, 10.0, 0.0, 0.0, 0.0),
        ];
        assert_eq!(
            adjusted_multipliers(&days, &events, Adj::Hfq).unwrap(),
            [1.0, 1.25]
        );
        let qfq = adjusted_multipliers(&days, &events, Adj::Qfq).unwrap();
        assert!((qfq[0] - 0.8).abs() < 1e-12);
        assert_eq!(qfq[1], 1.0);
        for cash in [f32::NAN, -1.0, 1000.0] {
            assert!(
                adjusted_multipliers(&days, &[xdxr(20260105, cash, 0.0, 0.0, 0.0)], Adj::Qfq)
                    .is_err()
            );
        }
        assert!(adjusted_multipliers(&[kd(20260102, f64::NAN)], &[], Adj::Hfq).is_err());
    }

    fn kd(date: u32, close: f64) -> super::KlineData<'static> {
        let (y, m, d) = (date / 10000, date / 100 % 100, date % 100);
        super::KlineData {
            dt: crate::tcp::helper::DateTime {
                year: y as u16,
                month: m as u16,
                day: d as u16,
                hour: 15,
                minute: 0,
            },
            code: "",
            open: close,
            close,
            high: close,
            low: close,
            vol: 0.0,
            amount: 0.0,
        }
    }

    fn xdxr(date: u32, fh: f32, sg: f32, pg: f32, pgj: f32) -> super::XdxrData {
        super::XdxrData {
            market: 1,
            code: "600000".into(),
            date,
            category: 1,
            fh_qltp: fh,
            pgj_qzgb: pgj,
            sg_hltp: sg,
            pg_hzgb: pg,
        }
    }

    /// 回退日期使用上海时区，不受部署机器本地时区影响。
    #[test]
    fn today_yyyymmdd_format() {
        use super::today_yyyymmdd;
        let v = today_yyyymmdd();
        assert!((20000101..=20991231).contains(&v), "非法日期: {v}");
        let today = chrono::Utc::now()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .date_naive();
        let expected = {
            use chrono::Datelike;
            (today.year() * 10000 + today.month() as i32 * 100 + today.day() as i32) as u32
        };
        assert_eq!(v, expected);
    }
}
