//! 所有受管 Tcp（包括 Client 主连接及各连接池）共享的进程预算。
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// 在首次建立 Tcp 前调用 [`configure_network`] 可调整进程策略。
#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    pub max_connections: usize,
    pub acquire_timeout: Duration,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub pagination_timeout: Duration,
    pub batch_timeout: Duration,
    pub endpoint_cooldown: Duration,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            max_connections: 8,
            acquire_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(15),
            pagination_timeout: Duration::from_secs(120),
            batch_timeout: Duration::from_secs(300),
            endpoint_cooldown: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EndpointStats {
    pub attempts: u64,
    pub failures: u64,
    pub cooldown_remaining: Duration,
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub active: usize,
    pub peak: usize,
    pub max_connections: usize,
    pub acquisition_timeouts: u64,
    pub requests: u64,
    pub request_failures: u64,
    pub endpoints: HashMap<SocketAddr, EndpointStats>,
}

#[derive(Debug, Default)]
struct Endpoint {
    attempts: u64,
    failures: u64,
    cooling_until: Option<Instant>,
}

#[derive(Debug, Default)]
struct State {
    active: usize,
    peak: usize,
    cursor: usize,
    acquisition_timeouts: u64,
    requests: u64,
    request_failures: u64,
    endpoints: HashMap<SocketAddr, Endpoint>,
}

#[derive(Debug)]
pub(crate) struct Governor {
    pub policy: NetworkPolicy,
    state: Mutex<State>,
    available: Condvar,
}

static GLOBAL: OnceLock<Arc<Governor>> = OnceLock::new();

/// 仅允许在进程首次使用网络预算前设置一次；已初始化时返回 AlreadyExists。
pub fn configure_network(policy: NetworkPolicy) -> io::Result<()> {
    if policy.max_connections == 0
        || [
            policy.acquire_timeout,
            policy.connect_timeout,
            policy.request_timeout,
            policy.pagination_timeout,
            policy.batch_timeout,
        ]
        .iter()
        .any(Duration::is_zero)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "网络预算必须大于0",
        ));
    }
    for timeout in [
        policy.acquire_timeout,
        policy.connect_timeout,
        policy.request_timeout,
        policy.pagination_timeout,
        policy.batch_timeout,
        policy.endpoint_cooldown,
    ] {
        deadline_after(timeout)?;
    }
    GLOBAL
        .set(Arc::new(Governor::new(policy)))
        .map_err(|_| io::Error::new(io::ErrorKind::AlreadyExists, "网络策略已初始化"))
}

pub fn network_policy() -> &'static NetworkPolicy {
    &global().policy
}

pub fn network_stats() -> NetworkStats {
    global().stats()
}

pub(crate) fn global() -> &'static Arc<Governor> {
    GLOBAL.get_or_init(|| Arc::new(Governor::new(NetworkPolicy::default())))
}

pub(crate) fn deadline_after(timeout: Duration) -> io::Result<Instant> {
    Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "超时时间过大"))
}

pub(crate) fn remaining(deadline: Instant) -> io::Result<Duration> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "操作总体时限已耗尽",
        ))
    } else {
        Ok(duration)
    }
}

impl Governor {
    fn new(policy: NetworkPolicy) -> Self {
        Self {
            policy,
            state: Mutex::new(State::default()),
            available: Condvar::new(),
        }
    }

    pub fn acquire(self: &Arc<Self>, deadline: Instant) -> io::Result<Permit> {
        let deadline = deadline.min(deadline_after(self.policy.acquire_timeout)?);
        let mut state = self.state.lock().unwrap();
        loop {
            let wait = match remaining(deadline) {
                Ok(wait) => wait,
                Err(error) => {
                    state.acquisition_timeouts += 1;
                    return Err(error);
                }
            };
            if state.active < self.policy.max_connections {
                state.active += 1;
                state.peak = state.peak.max(state.active);
                return Ok(Permit(self.clone()));
            }
            state = self.available.wait_timeout(state, wait).unwrap().0;
        }
    }

    pub fn candidates(&self, addresses: &[SocketAddr]) -> Vec<SocketAddr> {
        let mut state = self.state.lock().unwrap();
        let start = state.cursor;
        state.cursor = state.cursor.wrapping_add(1);
        (0..addresses.len())
            .map(|offset| addresses[(start.wrapping_add(offset)) % addresses.len()])
            .filter(|address| {
                state
                    .endpoints
                    .get(address)
                    .and_then(|endpoint| endpoint.cooling_until)
                    .is_none_or(|until| until <= Instant::now())
            })
            .collect()
    }

    pub fn endpoint_result(&self, address: SocketAddr, success: bool) {
        let mut state = self.state.lock().unwrap();
        let endpoint = state.endpoints.entry(address).or_default();
        endpoint.attempts += 1;
        if success {
            endpoint.cooling_until = None;
        } else {
            endpoint.failures += 1;
            endpoint.cooling_until = Instant::now().checked_add(self.policy.endpoint_cooldown);
        }
    }

    pub fn request_result(&self, success: bool) {
        let mut state = self.state.lock().unwrap();
        state.requests += 1;
        state.request_failures += u64::from(!success);
    }

    fn stats(&self) -> NetworkStats {
        let state = self.state.lock().unwrap();
        NetworkStats {
            active: state.active,
            peak: state.peak,
            max_connections: self.policy.max_connections,
            acquisition_timeouts: state.acquisition_timeouts,
            requests: state.requests,
            request_failures: state.request_failures,
            endpoints: state
                .endpoints
                .iter()
                .map(|(&address, endpoint)| {
                    (
                        address,
                        EndpointStats {
                            attempts: endpoint.attempts,
                            failures: endpoint.failures,
                            cooldown_remaining: endpoint
                                .cooling_until
                                .map_or(Duration::ZERO, |until| {
                                    until.saturating_duration_since(Instant::now())
                                }),
                        },
                    )
                })
                .collect(),
        }
    }
}

/// 先关闭 Tcp 的两个句柄再释放预算；重连在原许可内进行。
#[derive(Debug)]
pub(crate) struct Permit(Arc<Governor>);

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.active -= 1;
        self.0.available.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_budget_releases_and_wakes_waiters() {
        let governor = Arc::new(Governor::new(NetworkPolicy {
            max_connections: 1,
            ..Default::default()
        }));
        let permit = governor
            .acquire(deadline_after(Duration::from_secs(1)).unwrap())
            .unwrap();
        assert_eq!(
            governor
                .acquire(deadline_after(Duration::from_millis(5)).unwrap())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        std::thread::scope(|scope| {
            let governor = &governor;
            let waiter = scope.spawn(|| {
                governor
                    .acquire(deadline_after(Duration::from_secs(1)).unwrap())
                    .unwrap()
            });
            drop(permit);
            drop(waiter.join().unwrap());
        });
        let stats = governor.stats();
        assert_eq!(
            (stats.active, stats.peak, stats.acquisition_timeouts),
            (0, 1, 1)
        );
    }

    #[test]
    fn endpoint_rotation_and_failure_cooldown() {
        let governor = Governor::new(NetworkPolicy::default());
        let addresses = [
            "127.0.0.1:1".parse().unwrap(),
            "127.0.0.1:2".parse().unwrap(),
        ];
        assert_eq!(governor.candidates(&addresses), addresses);
        assert_eq!(
            governor.candidates(&addresses),
            [addresses[1], addresses[0]]
        );
        governor.endpoint_result(addresses[0], false);
        assert_eq!(governor.candidates(&addresses), [addresses[1]]);
        governor.endpoint_result(addresses[0], true);
        assert_eq!(governor.candidates(&addresses).len(), 2);
        assert_eq!(governor.stats().endpoints[&addresses[0]].failures, 1);
    }
}
