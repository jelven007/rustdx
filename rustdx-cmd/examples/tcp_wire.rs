//! cargo run -p jelven-rustdx-cli --example tcp_wire -- capture /new/archive 127.0.0.1:7709
//! cargo run -p jelven-rustdx-cli --example tcp_wire -- replay /archive 3
use rustdx_cli::tcp_evidence::{FrameArchive, load_response, replay};
use rustdx_complete::{
    operation::OperationContext,
    tcp::{SecurityCount, Tcp, TcpConfig, Tdx},
};
use std::{path::Path, sync::Arc, time::Duration};

fn main() -> eyre::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        eyre::bail!(
            "usage: tcp_wire capture DIRECTORY ENDPOINT | replay DIRECTORY RECORD_SEQUENCE"
        );
    }
    let operation = OperationContext::new(Duration::from_secs(15), None)?;
    let _scope = operation.enter();
    match args[0].as_str() {
        "capture" => {
            let archive = Arc::new(FrameArchive::create(
                Path::new(&args[1]),
                64 * 1024 * 1024,
                256,
            )?);
            let config = TcpConfig {
                ip: Some(args[2].parse()?),
                ..Default::default()
            };
            let result = Tcp::with_config_and_observer(&config, archive.clone())
                .and_then(|mut tcp| tcp.heartbeat());
            archive.finish()?;
            println!("security_count={}", result?);
        }
        "replay" => {
            let record = load_response(Path::new(&args[1]), args[2].parse()?)?;
            let mut parser = SecurityCount::new(0);
            replay(&record, &mut parser)?;
            println!("security_count={}", parser.result());
        }
        _ => eyre::bail!("unknown action"),
    }
    Ok(())
}
