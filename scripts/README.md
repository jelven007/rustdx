# 脚本入口

脚本按职责归类。shell脚本从自身位置定位仓库根，可在其他工作目录调用；Python脚本使用显式参数，示例均从仓库根执行。临时测量、日志和输入路径写入结果文件，不以本机/tmp存在作为长期交付。

| 目录/脚本 | 用途 | 依赖与外部动作 |
| --- | --- | --- |
| dev/run_comprehensive_test.sh | offline/quick/live/report检查 | Cargo；默认离线，live显式联网 |
| ops/diagnose_network.sh | 通达信握手与心跳诊断，可传IP:端口 | Cargo及网络；失败传播退出码 |
| bench/measure_day_memory.py | 新旧二进制内存/字段摘要核对 | Python3、不同SHA的两版二进制、系统计时工具 |
| bench/measure_database_readback.py | 65,536行首次/重复写入及完整回读测量 | Python3、CLI、真实隔离CH/Mongo及mongoimport/mongosh |
| validation/verify_kline_pytdx.py | K线独立期望重解 | Python3、six、指定固定pytdx源码 |
| validation/verify_protocol_pytdx.py | 快照/财务/逐笔重解，--capture显式采样 | 同上；参考源码先核对SHA，不下载可执行代码 |
| validation/verify_real_day_database.py | 官网ZIP子集独立解码及两库重复验收 | ZIP、CLI及两库/工具 |
| validation/verify_database_faults.py | 自有代理断连、SIGKILL及恢复 | 真实隔离两库；只影响自有任务，随机私有库结束删除 |
| release/publish.sh | 两crate发布门禁及实际发布 | Cargo凭据；脚本确认后先核心再CLI |

```sh
bash scripts/dev/run_comprehensive_test.sh offline
bash scripts/dev/run_comprehensive_test.sh report
bash scripts/ops/diagnose_network.sh
python3 scripts/validation/verify_kline_pytdx.py \
  --pytdx-source /path/to/pinned-pytdx --fixtures tests/fixtures/kline
python3 scripts/validation/verify_protocol_pytdx.py \
  --pytdx-source /path/to/pinned-pytdx --fixtures tests/fixtures/protocol
```

report默认保存到`${TMPDIR:-/tmp}/rustdx-reports`，可通过RUSTDX_REPORT_DIR指定；pipefail保留Cargo失败退出码。协议重解会更新指定夹具文件，复测已有期望可先复制夹具到临时目录，避免无意改变既有证据。

测量完整参数见各Python脚本`--help`。day内存对比必须分别构建到独立target目录，不能传相同二进制。数据库脚本以随机私有数据库运行并删除自有表，不接受生产表名；仍需显式选择隔离服务，完整前置条件见[数据库验收](../docs/validation/DATABASE_ACCEPTANCE.md)。

发布脚本只在正式发包任务使用，本次目录整理没有运行。全all-features/MSRV/fuzz门禁另见[开发说明](../docs/development/DEVELOPMENT.md)及[版本交付](../docs/release/RELEASES.md)。
