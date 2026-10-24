//! 有界同步 TCP 连接池。借用对象持有连接，归还不依赖可移动的数组下标。
use crate::tcp::{Tcp, TcpConfig};
use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_size: usize,
    /// 最大空闲秒数；0 表示归还后不保留。
    pub max_idle: u64,
    pub max_lifetime: u64,
    /// 借出空闲连接时的心跳超时；0 表示关闭探活。
    pub health_check_timeout: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 3,
            max_idle: 600,
            max_lifetime: 3600,
            health_check_timeout: 5,
        }
    }
}

#[derive(Debug)]
struct IdleConnection {
    tcp: Tcp,
    created_at: Instant,
    returned_at: Instant,
}

#[derive(Debug, Default)]
struct State {
    idle: VecDeque<IdleConnection>,
    /// 包含正在建立、探活和借出的连接。
    active: usize,
    closed: bool,
}

#[derive(Debug)]
struct Inner {
    state: Mutex<State>,
    available: Condvar,
}

#[derive(Debug, Clone)]
pub struct ConnectionPool {
    inner: Arc<Inner>,
    config: PoolConfig,
    tcp_config: TcpConfig,
}

impl ConnectionPool {
    pub fn new(max_size: usize) -> io::Result<Self> {
        Self::with_config(PoolConfig {
            max_size,
            ..Default::default()
        })
    }

    pub fn with_config(config: PoolConfig) -> io::Result<Self> {
        Self::with_tcp_config(config, TcpConfig::default())
    }

    /// 创建池时显式继承客户端服务器、超时与重试策略。
    pub fn with_tcp_config(config: PoolConfig, tcp_config: TcpConfig) -> io::Result<Self> {
        if config.max_size == 0 || config.max_lifetime == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "连接池大小和生命周期必须大于0",
            ));
        }
        Ok(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                available: Condvar::new(),
            }),
            config,
            tcp_config,
        })
    }

    pub fn get_connection(&self) -> io::Result<PooledConn> {
        self.acquire(None)
    }

    /// 等待槽位、建连及探活共享该总体时限。
    pub fn get_connection_timeout(&self, timeout: Duration) -> io::Result<PooledConn> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "等待时间过大"))?;
        self.acquire(Some(deadline))
    }

    fn acquire(&self, deadline: Option<Instant>) -> io::Result<PooledConn> {
        let connection_deadline = deadline.unwrap_or(crate::tcp::governance::deadline_after(
            crate::tcp::network_policy().connect_timeout,
        )?);
        {
            let candidate;
            {
                let mut state = self.inner.state.lock().unwrap();
                loop {
                    crate::tcp::governance::remaining(connection_deadline)?;
                    if state.closed {
                        return Err(io::Error::new(io::ErrorKind::NotConnected, "连接池已关闭"));
                    }
                    state.idle.retain(|c| {
                        c.created_at.elapsed() < Duration::from_secs(self.config.max_lifetime)
                            && c.returned_at.elapsed() < Duration::from_secs(self.config.max_idle)
                            && c.tcp.is_reusable()
                    });
                    if let Some(conn) = state.idle.pop_front() {
                        candidate = Some(conn);
                        state.active += 1;
                        break;
                    }
                    if state.active < self.config.max_size {
                        candidate = None;
                        state.active += 1; // 先预留容量，随后在锁外建连。
                        break;
                    }
                    let Some(deadline) = deadline else {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "Connection pool exhausted",
                        ));
                    };
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "等待连接池超时"));
                    }
                    state = self
                        .inner
                        .available
                        .wait_timeout(state, remaining)
                        .unwrap()
                        .0;
                }
            }
            let created_at = candidate
                .as_ref()
                .map_or_else(Instant::now, |c| c.created_at);
            let connection = match candidate {
                Some(mut conn) => {
                    if self.config.health_check_timeout > 0 {
                        conn.tcp
                            .with_deadline(connection_deadline, |tcp| {
                                tcp.check_health(Duration::from_secs(
                                    self.config.health_check_timeout,
                                ))
                            })
                            .map(|()| conn.tcp)
                    } else {
                        Ok(conn.tcp)
                    }
                }
                None => Tcp::with_config_until(&self.tcp_config, connection_deadline),
            };
            match connection {
                Ok(tcp) => {
                    let mut conn = PooledConn {
                        pool: self.clone(),
                        tcp: Some(tcp),
                        created_at,
                    };
                    if self.inner.state.lock().unwrap().closed {
                        conn.invalidate();
                        return Err(io::Error::new(io::ErrorKind::NotConnected, "连接池已关闭"));
                    }
                    Ok(conn)
                }
                Err(err) => {
                    self.release_reservation();
                    Err(err)
                }
            }
        }
    }

    fn release_reservation(&self) {
        let mut state = self.inner.state.lock().unwrap();
        state.active = state.active.saturating_sub(1);
        self.inner.available.notify_one();
    }

    pub fn stats(&self) -> PoolStats {
        let state = self.inner.state.lock().unwrap();
        let active = if state.closed { 0 } else { state.active };
        PoolStats {
            total: active + state.idle.len(),
            active,
            idle: state.idle.len(),
            max_size: self.config.max_size,
        }
    }

    /// 永久关闭池；已经借出的连接在归还时丢弃。
    pub fn close(&self) {
        let mut state = self.inner.state.lock().unwrap();
        state.closed = true;
        state.idle.clear();
        self.inner.available.notify_all();
    }
}

pub struct PooledConn {
    pool: ConnectionPool,
    tcp: Option<Tcp>,
    created_at: Instant,
}

impl PooledConn {
    pub fn get_mut(&mut self) -> &mut Tcp {
        self.tcp.as_mut().expect("连接已被标记为不可复用")
    }

    pub fn execute<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Tcp) -> R,
    {
        f(self.get_mut())
    }

    /// 请求失败后调用；归还时释放容量而不保存失效连接。
    pub fn invalidate(&mut self) {
        self.tcp = None;
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        let mut state = self.pool.inner.state.lock().unwrap();
        state.active = state.active.saturating_sub(1);
        if !state.closed
            && self.pool.config.max_idle > 0
            && self.created_at.elapsed() < Duration::from_secs(self.pool.config.max_lifetime)
            && let Some(tcp) = self.tcp.take()
            && tcp.is_reusable()
        {
            state.idle.push_back(IdleConnection {
                tcp,
                created_at: self.created_at,
                returned_at: Instant::now(),
            });
        }
        self.pool.inner.available.notify_one();
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total: usize,
    pub active: usize,
    pub idle: usize,
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tcp::tests::mock_server;

    #[test]
    fn cleanup_does_not_move_borrowed_connections() {
        let (config, server) = mock_server(3, |_| vec![0, 0]);
        let pool = ConnectionPool::with_tcp_config(
            PoolConfig {
                max_size: 2,
                max_idle: 0,
                health_check_timeout: 0,
                ..Default::default()
            },
            config,
        )
        .unwrap();
        let first = pool.get_connection().unwrap();
        let second = pool.get_connection().unwrap();
        assert_eq!(
            pool.get_connection().err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(first);
        let third = pool.get_connection().unwrap();
        drop(second);
        drop(third);
        assert_eq!(pool.stats().active, 0);
        assert_eq!(pool.stats().total, 0);
        pool.close();
        server.join().unwrap();
    }

    #[test]
    fn reuse_wait_and_close() {
        let (config, server) = mock_server(1, |_| vec![0, 0]);
        let pool = ConnectionPool::with_tcp_config(
            PoolConfig {
                max_size: 1,
                health_check_timeout: 0,
                ..Default::default()
            },
            config,
        )
        .unwrap();
        let conn = pool.get_connection().unwrap();
        assert_eq!(
            pool.get_connection_timeout(Duration::from_millis(5))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        drop(conn);
        assert_eq!(pool.stats().idle, 1);
        let conn = pool.get_connection().unwrap();
        pool.close();
        assert_eq!(
            pool.get_connection().err().unwrap().kind(),
            io::ErrorKind::NotConnected
        );
        drop(conn);
        assert_eq!(pool.stats().total, 0);
        server.join().unwrap();
    }

    #[test]
    fn invalid_configuration() {
        assert!(ConnectionPool::new(0).is_err());
    }
}
