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
    let count = tcp.heartbeat()?;
    if count == 0 {
        return Err("心跳返回证券数量为0，无法确认行情服务可用".into());
    }
    println!("通达信握手和心跳成功，深市证券数量: {count}");
    Ok(())
}
