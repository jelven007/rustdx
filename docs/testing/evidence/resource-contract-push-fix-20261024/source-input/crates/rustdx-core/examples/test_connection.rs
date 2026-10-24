//! 验证通达信连接和协议心跳。可选参数：IP:端口。
use rustdx_complete::tcp::{Tcp, TcpConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TcpConfig {
        ip: std::env::args()
            .nth(1)
            .map(|addr| addr.parse())
            .transpose()?,
        timeout: std::time::Duration::from_secs(3),
        ..Default::default()
    };
    let mut tcp = Tcp::with_config(&config)?;
    let ack = tcp.heartbeat()?;
    println!(
        "通达信握手和0004心跳成功，服务端日期: {:?}",
        ack.server_date
    );
    Ok(())
}
