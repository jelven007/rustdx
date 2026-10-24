//! Unpooled synchronous HTTP: every live connection owns the same TCP/HTTP
//! permit through the complete response body, including redirects and retries.
use rustdx_complete::{operation, tcp};
use std::{
    io::{self, Read},
    time::{Duration, Instant},
};
use ureq::{
    Agent, RequestBuilder,
    typestate::{WithBody, WithoutBody},
    unversioned::{
        resolver::DefaultResolver,
        transport::{
            Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
        },
    },
};

pub fn agent_config() -> ureq::config::Config {
    Agent::config_builder()
        .max_idle_connections(0)
        .timeout_resolve(Some(Duration::from_secs(1)))
        .timeout_connect(Some(Duration::from_secs(1)))
        .build()
}

pub fn agent(resolver: impl ureq::unversioned::resolver::Resolver) -> Agent {
    Agent::with_parts(
        agent_config(),
        DefaultConnector::default().chain(PollingConnector),
        resolver,
    )
}

pub fn get(url: &str) -> RequestBuilder<WithoutBody> {
    agent(DefaultResolver::default()).get(url)
}

pub fn head(url: &str) -> RequestBuilder<WithoutBody> {
    agent(DefaultResolver::default()).head(url)
}

pub fn post(url: &str) -> RequestBuilder<WithBody> {
    agent(DefaultResolver::default()).post(url)
}

#[derive(Debug)]
struct PollingConnector;

impl<T: Transport> Connector<T> for PollingConnector {
    type Out = PollingTransport<T>;
    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<T>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        operation::check()?;
        Ok(chained.map(PollingTransport))
    }
}

#[derive(Debug)]
struct PollingTransport<T>(T);

impl<T: Transport> Transport for PollingTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.0.buffers()
    }
    fn is_open(&mut self) -> bool {
        self.0.is_open()
    }
    fn is_tls(&self) -> bool {
        self.0.is_tls()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        let limit = operation::timeout((*timeout.after).min(Duration::from_secs(1)))?;
        self.0.transmit_output(
            amount,
            NextTimeout {
                after: limit.into(),
                reason: timeout.reason,
            },
        )?;
        operation::check()?;
        Ok(())
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let started = Instant::now();
        loop {
            operation::check()?;
            let remaining = timeout.after.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(timeout.reason));
            }
            let poll = operation::timeout(remaining.min(Duration::from_millis(200)))?;
            match self.0.await_input(NextTimeout {
                after: poll.into(),
                reason: timeout.reason,
            }) {
                Err(ureq::Error::Timeout(_)) => continue,
                result => {
                    operation::check()?;
                    return result;
                }
            }
        }
    }
}

pub struct HttpOperation {
    _permit: tcp::NetworkPermit,
    started: Instant,
    timeout: Duration,
    success: bool,
    empty: bool,
}

impl HttpOperation {
    pub fn new(limit: Duration) -> io::Result<Self> {
        let started = Instant::now();
        let permit = tcp::acquire_network(limit)?;
        let timeout = operation::timeout(limit.saturating_sub(started.elapsed()))?;
        if timeout.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(Self {
            _permit: permit,
            started,
            timeout,
            success: false,
            empty: false,
        })
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    pub fn complete(&mut self, empty: bool) -> io::Result<()> {
        operation::check()?;
        self.success = true;
        self.empty = empty;
        Ok(())
    }
}

impl Drop for HttpOperation {
    fn drop(&mut self) {
        tcp::observe_request(self.started.elapsed(), self.success, self.empty);
    }
}

pub fn text(body: &mut ureq::Body, limit: u64) -> eyre::Result<String> {
    let mut bytes = Vec::new();
    operation::CheckedIo(body.as_reader())
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        eyre::bail!("HTTP响应超过{limit}字节限制");
    }
    Ok(String::from_utf8(bytes)?)
}
