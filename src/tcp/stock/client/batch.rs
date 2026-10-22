use super::{Client, KlineData, ctx_err, fetch_bars_range, fetch_k_checked};
use crate::tcp::governance::{deadline_after, remaining};
use crate::tcp::network_policy;
use std::{
    collections::VecDeque,
    io,
    sync::{Condvar, Mutex},
    time::Instant,
};

#[derive(Debug, Default)]
pub(super) struct BatchGate {
    busy: Mutex<bool>,
    ready: Condvar,
}

impl BatchGate {
    fn acquire(&self, deadline: Instant) -> io::Result<BatchGuard<'_>> {
        let mut busy = self
            .busy
            .lock()
            .map_err(|_| io::Error::other("批量查询锁已损坏"))?;
        loop {
            let wait = remaining(deadline)?;
            if !*busy {
                *busy = true;
                return Ok(BatchGuard(self));
            }
            busy = self
                .ready
                .wait_timeout(busy, wait)
                .map_err(|_| io::Error::other("批量查询锁已损坏"))?
                .0;
        }
    }
}

struct BatchGuard<'a>(&'a BatchGate);

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        *self.0.busy.lock().unwrap() = false;
        self.0.ready.notify_one();
    }
}

type RowResult<'a> = io::Result<Vec<KlineData<'a>>>;

pub(super) fn fetch<'a>(
    client: &Client,
    stocks: &[(u16, &'a str)],
    category: Option<u16>,
    begin: Option<u32>,
    end: Option<u32>,
    max_parallel: usize,
) -> io::Result<Vec<RowResult<'a>>> {
    if stocks.is_empty() {
        return Ok(Vec::new());
    }
    let deadline = deadline_after(network_policy().batch_timeout)?;
    let _guard = client.batch_lock.acquire(deadline)?;
    let workers = if max_parallel == 0 {
        std::thread::available_parallelism().map_or(4, usize::from)
    } else {
        max_parallel
    }
    .min(network_policy().max_connections.saturating_sub(1).max(1))
    .min(stocks.len())
    .max(1);
    let pool = client.batch_pool(workers)?;
    let queue = Mutex::new(stocks.iter().copied().enumerate().collect::<VecDeque<_>>());
    let results = Mutex::new(
        (0..stocks.len())
            .map(|_| None)
            .collect::<Vec<Option<RowResult<'a>>>>(),
    );
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let pool = &pool;
            let queue = &queue;
            let results = &results;
            scope.spawn(move || {
                let mut connection =
                    remaining(deadline).and_then(|timeout| pool.get_connection_timeout(timeout));
                while let Some((index, (market, code))) = { queue.lock().unwrap().pop_front() } {
                    let result = (|| {
                        remaining(deadline)?;
                        let connection = connection
                            .as_mut()
                            .map_err(|error| io::Error::new(error.kind(), error.to_string()))?;
                        connection.execute(|tcp| {
                            tcp.with_deadline(deadline, |tcp| match category {
                                Some(category) => {
                                    fetch_bars_range(tcp, market, code, category, begin, end)
                                }
                                None => fetch_k_checked(tcp, market, code, begin, end),
                            })
                        })
                    })()
                    .map_err(|error| {
                        ctx_err(error, format_args!("batch(market={market}, code={code})"))
                    });
                    results.lock().unwrap()[index] = Some(result);
                }
            });
        }
    });
    Ok(results
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|result| result.unwrap_or_else(|| Err(io::Error::other("批量任务未完成"))))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_batches_have_a_deadline() {
        let gate = BatchGate::default();
        let guard = gate
            .acquire(deadline_after(std::time::Duration::from_secs(1)).unwrap())
            .unwrap();
        assert_eq!(
            gate.acquire(deadline_after(std::time::Duration::from_millis(5)).unwrap())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        drop(guard);
        assert!(
            gate.acquire(deadline_after(std::time::Duration::from_secs(1)).unwrap())
                .is_ok()
        );
    }
}
