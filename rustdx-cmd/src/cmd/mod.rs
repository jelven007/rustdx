use argh::FromArgs;
use eyre::Result;

mod data_check;
mod day;
mod east;

pub use self::{
    day::{DayCmd, auto_prefix},
    east::EastCmd,
};

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
    #[argh(subcommand)]
    sub: SubCommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum SubCommand {
    Day(DayCmd),
    EastMoney(EastCmd),
    DataCheck(data_check::DataCheckCmd),
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
            EastMoney(cmd) => cmd.run(),
            DataCheck(cmd) => cmd.run(),
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
