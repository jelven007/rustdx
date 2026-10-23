# 运行配置与参考数据

本目录保存无凭据示例及版本化参考日历。CLI读取进程环境和命令行，**不会自动加载runtime.env或Job JSON**。新增runner通过`run --config`显式读取严格JSON，配置职责相互独立。

## 环境变量

复制[runtime.env.example](runtime.env.example)为runtime.env，在本地修改后由shell/部署平台导出：

```sh
cp config/runtime.env.example config/runtime.env
# 编辑本地值后，在仓库根执行；文件内容须可信
set -a
. config/runtime.env
set +a
rustdx --timeout 10800 --network-limit 8 day ./lday -o stocks.csv
```

runtime.env被Git忽略。部署环境可直接注入变量，不必复制文件；ClickHouse Compose自己的`.env`见[部署说明](../deploy/README.md)，两者不会自动相互读取。

| 变量 | 用途 | 未设置时 |
| --- | --- | --- |
| CLICKHOUSE_URL | CLI的HTTP地址 | http://127.0.0.1:8123 |
| CLICKHOUSE_USER / CLICKHOUSE_PASSWORD | CLI数据库认证 | default / 空密码 |
| MONGODB_URI | mongoimport/mongosh连接及认证 | mongodb://127.0.0.1:27017 |
| RUSTDX_VERIFY_BATCH_ROWS | 精确回读每批键数，1–16384 | 4096；非法值拒绝 |
| RUSTDX_NETWORK_DIR | 同机共享许可文件目录 | 系统临时目录/rustdx_network_permits |
| RUSTDX_EASTMONEY_IPS | 东财备用节点IP，逗号分隔 | 内置候选；通常无需修改 |
| NODE_OPTIONS | mongosh的V8选项 | CLI回读子进程默认--max-old-space-size=128；已有值不覆盖 |
| RUST_LOG | env_logger日志过滤 | Info |
| RUSTDX_REPORT_DIR | scripts/dev测试日志目录 | TMPDIR或/tmp下的rustdx-reports |
| RUSTDX_SKIP_INTEGRATION_TESTS | 离线检查跳过外部源，检查存在性 | 未设置启用外部案例；设0也会跳过 |
| RUSTDX_DB_ACCEPTANCE | 独立数据库验收测试开关 | 默认不运行真实数据库验收 |

全局CLI参数须放在子命令前：`--timeout`默认10800秒、`--cancel-file`可指定取消标记、`--network-limit`默认8、`--network-dir`优先于RUSTDX_NETWORK_DIR。同目录不同进程必须使用相同上限。线程数、报告、外部日历等为子命令参数，以`rustdx day --help`为准。

## 日历与规则

[calendars/sse-2026.json](calendars/sse-2026.json)和[calendars/szse-2026.json](calendars/szse-2026.json)分别来自两交易所，覆盖2026全年，各242交易日，用于维护与独立逐日回归。内置日历覆盖2024–2026；这些外部文件不会默认加载。

```sh
rustdx data-check calendar config/calendars/sse-2026.json \
  --reference config/calendars/szse-2026.json
rustdx day ./lday --calendar config/calendars/sse-2026.json -o stocks.csv
rustdx data-status --calendar config/calendars/sse-2026.json --report coverage.json
```

2027完整数据未取得，不能复制本年度文件并延长日期；规则核实截止与日历覆盖分别管理。生产当前日历和证券事实由部署存储管理，经data-check验证后显式传入。格式及更新流程见[数据维护](../docs/operations/DATA_MAINTENANCE.md)，资源边界见[网络预算](../docs/architecture/NETWORK.md)。

## 单机生产Job

[runner/job.example.json](runner/job.example.json)是当前本地A股原价日线契约的严格配置，字段包含根目录/CLI、来源与策略修订、日期、完整事实、源/工作空间/行数/期限预算及可选数据库目标。所有路径为绝对路径；全零公告摘要必须替换为实际SHA-256，不能直接作为真实事实执行。`database_targets: []`只发布文件，多目标须全部精确verified。

[runner/publisher.env.example](runner/publisher.env.example)只给出变量名，实际凭据由密钥管理注入。JSON不含凭据值，worker清空继承环境；清空环境不构成同UID权限隔离。Job文件须由维护流程原子更新，定时器反复执行相同身份只去重，不自动制造次日Job/事实。详细格式/命令见[runner](../rustdx-runner/README.md)，DBA、systemd和恢复见[部署说明](../deploy/runner/README.md)。
