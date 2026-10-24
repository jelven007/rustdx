use argh::FromArgs;
use eyre::Result;

mod data_check;
mod data_status;
mod day;

#[derive(FromArgs, PartialEq, Debug)]
/// Explicit small TCP daily candidate; runner supplies frozen request and facts.
#[argh(subcommand, name = "tcp-day")]
struct TcpDayCmd {
    /// private candidate directory containing tcp-request.json and facts.
    #[argh(option)]
    directory: std::path::PathBuf,
}

pub use self::day::{DayCmd, auto_prefix};

const VERSION: &str = env!("RUSTDX_VERSION");

#[derive(FromArgs, PartialEq, Debug)]
/// rustdx
pub struct TopLevel {
    /// 整命令总时限（秒），默认10800，覆盖下载到数据库验收。
    #[argh(option, default = "10800")]
    pub timeout: u64,
    /// 此路径出现时协作取消；Ctrl-C/SIGTERM也取消当前任务。
    #[argh(option)]
    pub cancel_file: Option<std::path::PathBuf>,
    /// 同机TCP/HTTP共享连接上限，默认8；同目录须使用一致的上限。
    #[argh(option, default = "8")]
    pub network_limit: usize,
    /// 共享连接许可目录，默认系统临时目录/rustdx_network_permits。
    #[argh(option)]
    pub network_dir: Option<std::path::PathBuf>,
    /// 显式跳过进程启动TCP测速与预热，供离线文件操作使用；默认启用。
    #[argh(switch)]
    pub no_tcp_startup: bool,
    #[argh(subcommand)]
    sub: SubCommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum SubCommand {
    Day(DayCmd),
    TcpDay(TcpDayCmd),
    DataCheck(data_check::DataCheckCmd),
    DataStatus(data_status::DataStatusCmd),
    Help(Show),
}

/// rustdx 版本号、调试
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "show")]
struct Show {
    /// 版本号。
    #[argh(switch, short = 'v')]
    version: bool,

    /// 可选。打印 TopLevel（及子命令） 结构体。比如 `rustdx -p day`。
    #[argh(switch, short = 'p', long = "print-struct")]
    pub print_struct: bool,
}

impl TopLevel {
    pub fn match_subcmd(&self) -> Result<()> {
        use SubCommand::*;
        match &self.sub {
            Day(cmd) => cmd.run(),
            TcpDay(cmd) => {
                let request =
                    rustdx_cli::tcp_day::Request::load(&cmd.directory.join("tcp-request.json"))?;
                rustdx_cli::tcp_day::collect(&cmd.directory, &request)?;
                Ok(())
            }
            DataCheck(cmd) => cmd.run(),
            DataStatus(cmd) => cmd.run(),
            Help(help) => {
                if help.version {
                    println!("当前版本号：{VERSION}");
                }
                if help.print_struct {
                    println!("{self:#?}");
                }
                Ok(())
            }
        }
    }
}
