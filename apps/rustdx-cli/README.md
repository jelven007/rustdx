# rustdx CLI

包名 `jelven-rustdx-cli 2.0.0`（已发布crates.io），库名 `rustdx_cli`，二进制 `rustdx`。Rust 2024 / MSRV 1.89。处理通达信日线目录、官网 ZIP、指定 URL 或本地 ZIP，输出 CSV、质量清单，或导入 ClickHouse/MongoDB 并精确回读。

```sh
cargo install --locked jelven-rustdx-cli --version 2.0.0
rustdx day /path/to/lday -o /path/to/stocks.csv \
  --report /path/to/collection.json
rustdx day --help
```

源码安装从仓库根执行 `cargo install --path apps/rustdx-cli --locked`。所有全局参数放在子命令前；本地命令默认不初始化行情TCP，`--tcp-warmup`显式测速和预热。HTTP或数据库按实际需要使用网络。

成功须同时满足退出码、清单、产物与适用的数据库精确验收；`acknowledged` 仍须继续回读。失败、确认丢失或不一致时保留 CSV，恢复前核实目标及较新数据。

CSV 量为手，factor 为累计收益指数，原始 OHLC 不被该因子缩放。配置由命令行和进程环境显式传入，CLI 不自动读取 runtime.env。

新增`tcp-day --directory <private-candidate>`供runner或受控调用者使用：
目录包含严格`tcp-request.json`、单位材料`facts/<sha256>`和空`raw`。
默认不执行通用预热；采集器对显式端点执行三轮协议探测。
支持沪深A股已结束日期；新`Request.schedule`最多10000只、8个有预算预留的采集线程，
窗口全部结束后按输入顺序重放写出。旧无schedule请求仍为串行、最多32只。
该命令schema8 CSV量为股，报告明确来源合同与`business_coverage=unverified`；
旧`day` CSV单位保持手。最终业务覆盖、封存与修订由runner完成。
库接口为`tcp_day::{Source, Schedule, Request, collect, replay_security, verify_archive_budget}`。

仓库中的[导入状态与恢复约定](https://github.com/jelven007/rustdx/blob/main/docs/operations/IMPORTS.md)、[运行配置](https://github.com/jelven007/rustdx/blob/main/config/README.md)和[运维手册](https://github.com/jelven007/rustdx/blob/main/docs/runbooks/RUNBOOK.md)随源码维护。发布版本与当前源码须按提交区分；本包保留 LICENSE-MIT。
