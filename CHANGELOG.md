# 变更记录

## 未发布新crate版本（2026-10-23，目录治理与需求2.0）

- 文档按需求/架构/开发/验证/运营运维/发布/历史归类；脚本按开发/运维/基准/验收/发布归类，更新全部调用路径。
- 二进制样本归tests/fixtures/tdx，SQL归deploy/clickhouse/sql，版本化参考日历归config/calendars；补运行配置、部署、脚本、夹具及数据说明。
- 重写需求文档，保留FR-01至11，新增缓存、来源重放与阶段观测，补角色/对象/状态、异常、非功能约束、验收追踪和生产待补事实。
- 补入四份被通用CSV忽略规则排除的协议期望；修正核心打包根data排除范围及Excel示例的缺失assets依赖。
- Rust1.89全workspace格式/Clippy/离线测试/文档、干净检出相关回归、独立7组协议重解及包清单/源码编译通过；CLI联合上传包离线准备仍缺核心注册表索引，范围见[验证记录](docs/validation/VALIDATION.md)。

当前目录见[项目结构](docs/architecture/PROJECT_STRUCTURE.md)，唯一需求主文档为[requirements/REQUIREMENTS.md](docs/requirements/REQUIREMENTS.md)，旧需求路径保留跳转。仅本地提交，版本与Cargo.lock不变，未推送或发布。

## 未发布新crate版本（2026-10-23，第三轮优化）

- 覆盖巡检、东财原始正文留证/离线重放、默认4096键可配回读与长驻MongoDB、快照/财务/逐笔checked请求、schema6阶段/临时字节/新鲜度观测。
- 779,200行真实官方历史子集独立逐行核对与两库重复全键/数值回读、四项真实服务故障恢复通过；修正ClickHouse查询格式化配置并保留线程池耗尽失败证据。
- 固定Linux数据库镜像及CI配置已保存，尚未在该平台执行；实际验收使用macOS实验ClickHouse26.11.1.548/MongoDB8.0.16，目标生产/全量与未来年度数据另验收。

详细实现、测量及限制见[第三轮记录](docs/history/OPTIMIZATION_ROUND3.md)。

## 未发布新crate版本（2026-10-23，六步优化与交叉验证）

- data-check更新前逐日比较独立日历，SecurityHistory保留公告摘要及已核实区间，未知证券状态返回None。
- 数据库每批512键独立精确回读全部导出字段，schema3记录database_verification，verified才允许清理CSV；同机同目标写入协调和MongoDB唯一索引检查。
- CLI整任务--timeout（默认10800秒）、--cancel-file及Ctrl-C/SIGTERM协作取消；TCP/HTTP共用进程/同机OS锁许可，清单记录实际资源及窗口P95。
- day以4096条分块，质量和累计收益跨块承接，非A股磁盘归并，目录/文件/日期输出确定；大样本4线程RSS219.5→10.9MiB，新旧字段摘要一致。
- EastCanonicalBar校验市场、源时间、单位和原价/收益语义，CSV/JSON格式兼容；KlineRequest私有参数、股票/指数独立checked解码，Client迁移并保留旧类型。
- 固定pytdx参考真实报文、独立跨源比较、当前TDX/东财与隔离数据库小表验收通过；Rust1.89完整检查及627,152次ASan/覆盖率fuzz通过。

本轮仅本地提交，Cargo版本保持1.12.2/1.8.2，未推送或发布。2027/证券历史、目标生产配置和全量负载及长期P95边界见[六步记录](docs/history/NEXT_OPTIMIZATION.md)，当前架构见[架构评估](docs/architecture/PROJECT_ANALYSIS.md)。

## 未发布新crate版本（2026-10-22，仓库源码优化与文档交付）

- 日历提供版本、来源与覆盖范围，严格导航支持外部休市数据；历史涨跌幅及主板ST制度切换按日期处理。
- 进程共享连接预算、端点轮换/冷却、连接/请求/分页/批次总体时限及网络统计。
- 标准行情显式来源、周期、价格口径和单位；股票名单HTTPS、响应大小和时限限制。
- day/east持久化运行清单、SHA-256、行数/逻辑键/日期边界、ClickHouse query_id和不确定部分写入状态；数据库导入超时，失败保留CSV。
- 布林带双栈滚动方差及KDJ单调队列，正常数据O(n)；保留预热、缺口及平滑口径。
- Client分页/复权与CLI数据库sink/schema拆分；新增checked day/lc/gbbq入口，移除解密unsafe及偏移加法溢出。
- 属性测试、模糊测试目标、窗口性能观测和all-features/MSRV CI。行为边界见[优化记录](docs/history/OPTIMIZATION.md)及[导入说明](docs/operations/IMPORTS.md)。
- 补齐需求、方案、开发、测试、运营、运维和版本交付文档，更新旧架构/迁移/验收描述并建立变更对应表，见[文档索引](docs/README.md)。
- 文档分支合入main，保留两个历史快照：`history/source-20261022`（1f9f739）和 `history/release-1.12.2-1.8.2`（904b33e）；完整提交链继续保留，见[版本交付](docs/release/RELEASES.md)。

源码实现提交为7cd02e7、5fb5b70、401265c、1f9f739，四个提交已推送main。Cargo版本仍为1.12.2/1.8.2，crates.io同版本不含本轮新增行为。Rust1.89离线workspace/all-features、Clippy、格式通过；ASan/覆盖率fuzz544,705次无崩溃，未重跑外部行情或数据库验收，见[验证方法](docs/validation/VALIDATION.md)。

## jelven-rustdx-complete 1.12.2 / jelven-rustdx-cli 1.8.2（2026-10-11，已发布 crates.io）

- 修复行情前后复权：可用历史首日不重复应用旧事件，停牌期间连续事件逐次承接参考价；非法价格、事件或因子返回错误。
- 涨跌停使用整数分及整数百分比舍入，修正半分误差与主板代码范围；昨收要求有效分单位报价。
- 证券列表拒绝提前空页、重复代码、数量越界；证券数量解析失败清空旧值，旧解析入口不因短报文或乱码名称panic。
- 重连握手失败计入重试预算，尚有预算时继续；当日分时回退固定使用UTC+8日期。
- `day/east` 统一检查相对/绝对路径、`..`及符号链接冲突，保护输入文件、报告、保留CSV和factor.csv；同一输出使用跨进程文件锁。
- ZIP缓存锁延续至解析结束；HEAD最多30秒、下载建连15秒/响应头30秒/单次总时限2小时，本地备选ZIP先校验目录。
- 文件与ClickHouse前因子使用同一流式校验，拒绝空集/重复/非法值，factor.csv原子写入；避免无事件复权分支的额外计算。
- EMA缺口重置；MACD保留缺口位置；非法参数和非有限结果不再伪装有效指标；SMA补偿求和及溢出恢复，KDJ校验收盘区间并修正文档公式。
- 移除无来源的行业/省份映射，旧函数弃用并返回未知，财务接口继续保留原始编码。
- 删除6个仅测试pytdx的脚本、过期请求脚本、未采用快照；简化网络诊断，清理死代码、错误导入名和无依据的一致性声明。保留许可证与必要协议来源。

本版基于源码提交2fbbda6，发布提交9dfc024；已按核心库1.12.2、CLI1.8.2的顺序发布到crates.io，并核验版本、依赖和源码包校验和。行为变化见[MIGRATION.md](docs/release/MIGRATION.md)。

## jelven-rustdx-complete 1.12.1 / jelven-rustdx-cli 1.8.1（2026-10-10，已发布 crates.io）

- 同一 Client 的 `k_batch` / `bars_batch` 排队，避免同时借空池或关闭在用池；批次内保留并行。
- `k_adjusted` 提前验证日期区间；字母证券代码不再误判 A 股，非法行情不再判为未触涨停。
- ClickHouse 完整读取响应，传播正文截断、HTTP错误和200正文异常；等待查询完成后判断结果。
- 已有 ClickHouse 表校验引擎、排序键、分区键、字段精度及默认版本，拒绝不兼容表。
- 全空 day 采集失败并保留上次输出；质量报告在输出及数据库导入结束后定案。
- 东财页码/页大小0提前失败；逐页检查total和重复证券，数量上限大于实际总数时正常结束。
- 移除错误静态概念样例；旧查询函数弃用并返回无数据，示例改用 `Client::block`。
- 删除20份旧归档、旧设计稿、4个第三方样例生成脚本和重复示例；保留版权和协议对照资料。
- 测试脚本修复退出状态及日志保存，默认离线，在线测试使用显式 `live`。
- CLI 更名为 `jelven-rustdx-cli`（crates.io 上 `rustdx-cli` 归属他人；Rust 导入名仍为 `rustdx_cli`，二进制名仍为 `rustdx`），源码包包含 MIT 许可证；更新架构、迁移和验证文档。

发布顺序：先 `jelven-rustdx-complete 1.12.1`，索引可见后再 `jelven-rustdx-cli 1.8.1`。
行为迁移与采集并发建议见 [MIGRATION.md](docs/release/MIGRATION.md) 和 [README](README.md)。

## 核心库 1.12.0 / CLI 源码 1.8.0（2026-10-10）

核心库以 `jelven-rustdx-complete` 发布，Rust 导入名保持 `rustdx_complete`。

- 修复连接池身份、完整帧重试、解压和安全解码；统一单只及批量分页与配置。
- 标准市场/代码/单位/UTC+8语义，精确日历覆盖2024–2026，排除午休。
- 增量收益因子继承前状态，停牌除权事件连续应用，前因子文件只读。
- day质量报告、严格检查、有界线程和结果通道、文件大小限制及原子输出。
- ClickHouse版本表与FINAL查询语义；MongoDB upsert和进程错误传播。
- 缓存容量、稳定键、原子文件、显式I/O错误及同键请求合并。
- ZIP续传版本边界、来源隔离、文件锁及解压失败保护。
- SMA/RSI线性滚动，RollingSma，KDJ缺口处理与RSI横盘口径。
- 东财JSON、收盘/日期及前因子衔接校验；Excel前导零与输入校验。
- 单标准握手修复当前服务器K线错误；非A股源异常保留警告，输出排序去重。
- Rust1.89验证，Linux/macOS/Windows CI；发布收敛至crates.io源码库。
- 本地真实行情、官网全量包及2981万行ClickHouse验收，详见 [验收报告](docs/validation/PRODUCTION_ACCEPTANCE.md)。

更早的实现历史可通过 Git 查看。当前维护文档以本仓库源码行为为准。
