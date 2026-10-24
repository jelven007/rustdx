# 脚本入口

脚本按职责归类。shell脚本从自身位置定位仓库根，可在其他工作目录调用；Python脚本使用显式参数，示例均从仓库根执行。临时测量、日志和输入路径写入结果文件，不以本机/tmp存在作为长期交付。

| 目录/脚本 | 用途 | 依赖与外部动作 |
| --- | --- | --- |
| dev/run_comprehensive_test.sh | offline/quick/live/report检查 | Cargo；默认离线，live显式联网 |
| ops/diagnose_network.sh | 通达信握手与心跳诊断，可传IP:端口 | Cargo及网络；失败传播退出码 |
| bench/measure_day_memory.py | 新旧二进制内存/字段摘要核对 | Python3、不同SHA的两版二进制、系统计时工具 |
| bench/compare_indicators.py | 同环境指标基准JSON比较及回归阈值 | Python3；核心indicators bench两次实际结果，要求同硬件/工具链/负载 |
| bench/measure_database_readback.py | 65,536行首次/重复写入及完整回读测量 | Python3、CLI、真实隔离CH/Mongo及mongoimport/mongosh |
| validation/verify_database_recovery.py | 两个runner版本、两库原生全备与全新数据目录恢复、全字段/索引/约束核对及失败保护 | Linux、同源码CLI/runner、Docker及mongosh/mongodump/mongorestore；固定digest、自建loopback容器，保留原生备份和逐命令证据 |
| validation/verify_kline_pytdx.py | K线独立期望重解 | Python3、six、指定固定pytdx源码 |
| validation/verify_protocol_pytdx.py | 快照/财务/逐笔重解，--capture显式采样 | 同上；参考源码先核对SHA，不下载可执行代码 |
| validation/verify_nine_commands.py | 九个新增7709命令完整帧/字段、资金分档、竞价分页及多精度报价独立核对 | Python3 stdlib；离线核验固定SHA，不运行第三方SDK，输出核验结果与摘要 |
| validation/verify_eltdx_330_commands.py | 02c5/02cf/02d0/052b完整帧、F10字节范围及退市字段独立核对 | Python3 stdlib；对新采样目录核验，历史源摘要不改写 |
| validation/verify_modern_commands.py | 0004/044d/0537/0fc6替代命令的请求、布局与原始字段核验 | Python3 stdlib；固定样本或新目录，业务日期/单位仍按范围声明 |
| validation/evidence_paths.py | 解析固定记录中的旧工作区路径 | 只映射属于本工作区的已迁移路径；不修改原manifest或摘要 |
| validation/verify_recent_minutes_boundaries.py | 0feb 非空采样的日期 selector、完整帧、原始字段与均价/精度独立核验 | Python3 stdlib；配合 `capture_recent_minutes_boundaries`，首次 `--freeze`；未取到全部所测证券非空数据返回 2 |
| validation/freeze_recent_minutes_fixtures.py | 从已冻结实采支持包提取真实非空0feb帧，并独立生成字段期望；`--check`只读核对来源、SHA和全部记录 | Python3 stdlib；仅向新目录写夹具，原始采样不修改，不调用Rust生成期望，不证明响应交易日期 |
| validation/recent_minutes_selector.py | 按schema1旧ordinal注释/schema2负YYYYMMDD读取0feb日期；直接执行运行日期回归 | Python3 stdlib；未知schema与非法日期拒绝，旧材料缺省schema1 |
| validation/verify_recent_minutes_date_selector.py | 日期修复专项：同日0fb4/日线、官方day、旧错位正文、新旧冻结SHA与跨节点字段差异 | Python3 stdlib；读取已冻结支持包，显式`--output`写新报告，原字节不修改；配合`validate_recent_auction --date-selector-check` |
| validation/verify_quote_push.py | 0547单次基线后被动接收：完整帧、精度、五档、请求轨迹、游标和交易窗口独立核验 | Python3 stdlib；配合 `validate_quote_push` 示例，首次 `--freeze`，休市诊断和合成回归不算盘中验收 |
| validation/verify_enhanced_history_transaction.py | 0fc6实帧解压、独立逐字段重解；--freeze创建新期望 | Python3 stdlib；默认只读、无网络，拒绝覆盖期望 |
| validation/verify_enhanced_history_day.py | 0fc6全天实帧重放、不同分页/重叠/分钟覆盖及K线量价额核对 | Python3 stdlib；默认无网络，固定输入/源码摘要；写入结果与规范事件，--freeze只创建一次摘要 |
| validation/analyze_enhanced_history_amount.py | 0fc6/0fb5和分钟/日线金额差异归因，累计量前缀与整手量候选 | Python3 stdlib；默认无网络，固定输入摘要并写入机器结果，--freeze仅首次创建摘要 |
| validation/verify_real_day_database.py | 官网ZIP子集独立解码及两库重复验收 | ZIP、CLI及两库/工具 |
| validation/verify_database_faults.py | 自有代理断连、SIGKILL及恢复 | 真实隔离两库；只影响自有任务，随机私有库结束删除 |
| release/publish.sh | 两crate发布门禁及实际发布 | Cargo凭据；脚本确认后先核心再CLI |
| release/preflight.py | 源码归档/身份及解包下游预检 | Python3、Rust1.89/Cargo；默认干净树，--snapshot允许审核未提交源码；不上传或提交 |

```sh
bash scripts/dev/run_comprehensive_test.sh offline
bash scripts/dev/run_comprehensive_test.sh report
bash scripts/ops/diagnose_network.sh
python3 scripts/validation/verify_kline_pytdx.py \
  --pytdx-source /path/to/pinned-pytdx --fixtures crates/rustdx-core/tests/fixtures/kline
python3 scripts/validation/verify_protocol_pytdx.py \
  --pytdx-source /path/to/pinned-pytdx --fixtures crates/rustdx-core/tests/fixtures/protocol
```

report默认保存到`${TMPDIR:-/tmp}/rustdx-reports`，可通过RUSTDX_REPORT_DIR指定；pipefail保留Cargo失败退出码。协议重解会更新指定夹具文件，复测已有期望可先复制夹具到临时目录，避免无意改变既有证据。

核心示例从根调用时使用-p jelven-rustdx-complete，固定夹具位于crates/rustdx-core/tests/fixtures。历史源码固定摘要不随目录整理更新；源码本身已变化时检出当时提交复现，当前行为验证另存结果。依赖本机/tmp的一次性实施审计生成器已删除，原审计JSON保留在docs/testing/evidence。

测量完整参数见各Python脚本`--help`。day内存对比必须分别构建到独立target目录，不能传相同二进制。数据库脚本以随机私有数据库运行并删除自有表，不接受生产表名；仍需显式选择隔离服务，完整前置条件见[数据库验收](../docs/testing/DATABASE_ACCEPTANCE.md)。

发布脚本只在正式发包任务使用，本次目录整理没有运行。全all-features/MSRV/fuzz门禁另见[开发说明](../docs/development/DEVELOPMENT.md)及[版本交付](../docs/release/RELEASES.md)。
