mod cmd;
mod download;
mod io;
mod paths;
mod quality;

#[macro_use]
extern crate log;

use eyre::Result;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let cmd: cmd::TopLevel = argh::from_env();
    let context = rustdx_complete::operation::OperationContext::new(
        std::time::Duration::from_secs(cmd.timeout),
        cmd.cancel_file.clone(),
    )?;
    let signal_context = context.clone();
    ctrlc::set_handler(move || signal_context.cancel())?;
    let _guard = context.enter();
    rustdx_complete::tcp::configure_network(rustdx_complete::tcp::NetworkPolicy {
        max_connections: cmd.network_limit,
        shared: Some(rustdx_complete::tcp::SharedNetworkPolicy {
            directory: cmd
                .network_dir
                .clone()
                .or_else(|| std::env::var_os("RUSTDX_NETWORK_DIR").map(Into::into))
                .unwrap_or_else(|| std::env::temp_dir().join("rustdx_network_permits")),
            max_connections: cmd.network_limit,
        }),
        ..Default::default()
    })?;
    if !cmd.no_tcp_startup {
        rustdx_complete::tcp::startup::initialize_from_env()?;
    }
    cmd.match_subcmd()?;
    rustdx_complete::operation::check()?;
    Ok(())
}
