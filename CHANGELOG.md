# 变更记录

## 2.0.0（未发布，2026-10-25，顺序优化与发布准备）

- 核心/CLI升为主版本2.0.0，CLI依赖同步；日历原名称全部返回Result，未知覆盖与越界导航不再推断或回退。
- 复权checked入口校验完整证券身份，日线字节及增量基准链传播明确错误；旧Option入口也拒绝错证券事件。
- 连接池等待按20毫秒重查取消/父期限，预留槽位用RAII释放，中毒清理关闭池。
- 新增checked缓存合并，等待/读取/fetch/返回共享预算，识别条带重入，超期成功不交付；旧入口等待契约单独保留。
- 本地CLI/runner默认离线，通用TCP预热改为`--tcp-warmup`显式选项；在线来源继续核验自己的节点。
- TCP日线及报文归档在所有平台同步文件内容，仅在Unix同步父目录，修复Windows采集返回拒绝访问；CI构建复用test profile并限制并行编译，避免保留重复的完整构建产物。
- 心跳后台I/O按20毫秒重查停止信号，修复Windows关闭克隆socket后阻塞读取仍延迟Drop；原始socket提取等待当前完整心跳，并恢复正常读写超时。
- Windows受管业务/心跳读写先按20毫秒就绪轮询重查取消和绝对期限，避免阻塞socket超时粒度延迟取消；所有平台保留取消/期限错误种类，读取超时设置到实际BufReader句柄，连接大小预算计入平台句柄和时钟布局。
- ClickHouse HTTP测试服务先完整读取定长或分块请求再返回响应，避免提前关闭连接触发BrokenPipe，使数据库确认/失败回读回归稳定覆盖预期行为。
- TCP日线及现代命令的本地测试对端关闭Nagle，避免分片响应叠加延迟确认；保留报文分片、主动延迟和原有期限/取消断言。
- 空闲心跳在复制socket前保存已连接端点，避免Windows克隆句柄的对端查询失败被误判为连接失效；当日节点撤销、实际报文I/O及期限检查仍生效。
- 启动策略与探测期限回归分为独立测试进程，避免并行测试提前初始化一次性网络策略；保留原测试函数及全部期限、取消和单许可断言。

逐项状态、检查和未验收范围见[优化执行记录](docs/development/OPTIMIZATION_PROGRESS.md)。

## 未发布新crate版本（2026-10-24，板块文件完整性修复）

- 板块高层下载验算02c5完整长度/32位MD5，06b9逐块检查精确长度与端点/连接/北京时间观察日连续性；空文件也校验MD5。
- 新增`BlockFile`、`Client::block_file/download_block_file`及严格`decode_block_content`；槽位、成分数、GBK或代码畸形返回错误，旧parse保留兼容行为。
- 两节点完整概念/指数4文件、80帧、105,826条成分独立MD5/全部字段一致，文件字节相同；旧02c5证据不改。

范围及检查见[修复报告](docs/archive/development/BLOCK_FILE_INTEGRITY_FIX_20261024.md)。

## 未发布新crate版本（2026-10-24，TCP日线整数股数修复）

- 新日线采集改用052b整数股数，新增`Daily*`通用别名及有界range入口；052b金额改为非负有限binary32，修复旧算法遗漏隐含首位。
- CLI/runner输入来源升级为`a-share/tcp-052b/day/raw/cny/integer-shares/v2`、请求及收据schema2；旧052d来源v1仅支持原字段离线重放。
- 两节点3只在市沪深A股、2026-10-19至22共24条日线，原帧/收据/CSV与官网day全部8字段精确一致。旧052d−94至+1股差异及冻结证据原样保留。

范围及检查见[修复报告](docs/archive/development/TCP_DAILY_INTEGER_SHARES_FIX_20261024.md)。

## 未发布新crate版本（2026-10-24，0feb日期编码修复）

- 近期分时selector改为有效YYYYMMDD整数的32位补码，日期schema从1升为2；旧ordinal公式将20260817/18错选为20260609/10。
- 两节点、六证券、六交易日共17,280分钟点与同日0fb4价量及日线开末价一致，周末/未来日期为空；原始错位证据与24组旧夹具不变，新增72组正确日期夹具。
- 核验/冻结工具按schema区分新旧材料；响应仍不回显日期，require_trading_date保持Unsupported，通用日期资格与保存窗口尚未验收。

范围、官方day佐证、两节点均价差异及检查见[修复报告](docs/archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。

## 未发布新crate版本（2026-10-24，TCP日线runner闭环）

- 新增显式TCP来源、最多32只沪深A股已结束日期的同步采集；冻结单位材料与实际获取日，和本地day来源互斥。
- CLI新增tcp-day、schema8股单位候选、压缩报文/分页留证与全库存独立回读；短页继续、空页同连接确认，预算/错身份/冲突拒绝。
- runner精确回读全部字段后核对事实键，复用release、数据库、源修订CAS和固定版本消费；封存恢复不联网重采。
- 保留旧day JSON/单位/Job身份，新增Config字段的Rust字面量须迁移。本地模拟已覆盖篡改、缺键与恢复；真实市场、五分钟、全量及目标环境仍待验。

结果见[第四阶段记录](docs/archive/development/REMAINING_FIXES_STAGE4_20261024.md)，消费合同见[应用接入](docs/development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。

## 未发布新crate版本（2026-10-22会话，P0响应正确性）

- 将旧、新命令的响应前缀、opcode和消息号校验统一至受管收帧路径，覆盖握手、心跳与IP探测；错配返回InvalidData，连接失效且不自动重试。
- 错配完整帧可留原始证据，失败计入请求统计；新增decode_response用于请求匹配的离线重解，CLI归档重放在解析前校验身份。
- 本地服务测试使用真实协议回显格式，补充合法正文配错身份、压缩/非压缩、连接池、握手/探测、空闲心跳及离线重放回归；冻结证据保持不变。

## 未发布新crate版本（2026-10-24，生产工程目录与需求3.0）

- 根目录改为虚拟Cargo工作区；核心源码、就近测试/示例/基准移至crates/rustdx-core，CLI/runner移至apps，外部测试和fuzz分别移至tests/integration与tools/fuzz。
- 同步路径依赖、include/共享测试引用、CI、脚本及应用安装入口；发布包拥有独立README和许可证。
- 需求保留FR-01至18并升为3.0；重写当前架构、技术方案、开发、测试、验收矩阵、运营、运维与发布入口，保留六类集成规则和完整26命令参考。
- 历史分析/实施/验收归档，已实施重复计划、需求跳转页、一次性临时审计生成器及Finder元数据删除；固定证据、夹具、日历与许可证按摘要保全。
- 覆盖巡检、systemd指标及恢复前置显式使用离线启动开关；全局期限参数放在子命令前。

目录映射、删除依据及本轮工程检查见[迁移记录](docs/release/PROJECT_RESTRUCTURE.md)。本次未执行新实盘、数据库、远端CI、发布或部署。

## 未发布新crate版本（2026-10-24，26命令现代入口补齐）

- 对照eltdx3.3.0的2个会话及24个业务命令，补齐02c5元数据、02cf/02d0 F10现代请求/范围读取和052b退市日线单页/分页/批量，26个同码入口全部覆盖。
- F10按实际字节推进，退市短页继续到空页；显式保留完整性、来源、尾部和专属量额编码，市场2仅参数/解码支持，在线未验收。
- 两节点98帧及独立字段/分页证据保留。同码覆盖不等于全业务验收；0feb响应日期和0547真实盘中主动推送仍有边界。
- 成交事件估算与052d源成交额、财务已验证值与候选值、目录价格精度分别处理，并同步外部应用消费约定。

完整签名、字段和约束见[接口参考](docs/reference/COMMAND_API.md)，原始实施范围见[26命令报告](docs/archive/validation/ELTDX_3_3_0_26_COMMANDS_IMPLEMENTATION_20261024.md)。

## 未发布新crate版本（2026-10-24，九个7709命令接入）

- 新增0452/051b/0547/054b/054c/056a/0fd1/0feb/0ffc的独立请求、严格解码、owned模型和Client入口，补6项有界批量与3项分页扫描；22项基线现在全部同码。
- 0547独占受管连接，支持刷新、推送观察/轮询、半帧保留、五档状态、队列/字节预算、失效和重建，保留连接代次与游标，关闭释放许可。
- 报价使用目录精度，分类返回未知精度时保留raw并以None标识价格；竞价明确为过程快照，资金两套分档保留原始字段和负值。
- 两节点132组完整帧及104342条目录记录通过标准库独立核验；11组真实夹具加入回归。0feb实测为空、0547未取得交易时段主动推送，业务范围明确记录。

接口、预算、测试和实际验证范围见[九命令实施记录](docs/archive/development/NINE_COMMANDS_IMPLEMENTATION_20261024.md)。Cargo版本保持不变。

## 未发布新crate版本（2026-10-24，进程默认TCP启动策略）

- CLI和runner所有有效命令默认全池测速、发布当日IP池并建立客户端，完成PACK1/0004验证；启动失败阻止业务执行。
- 保留40种子及共享/独立池CRUD，三轮、8并发、阶段3秒，每天北京时间09:00刷新；库内新建/重连/池复用仅使用当日通过节点。
- 新增startup初始化/报告/绝对时限API、refresh_until及测速线程取消传播，初始化/刷新锁等待计入deadline，取消或超时不发布部分结果。
- 预热TCP交接给匹配客户端/连接池；不匹配或HTTP申请额度时释放，单许可不会被预热连接占住。runner采集子进程继承初始地址和共享额度。
- 新增显式离线开关--no-tcp-startup、runner配置同名字段和RUSTDX_TCP_INITIAL_IPS覆盖；补本地协议和二进制启动回归。

策略、预算、集成API及运行行为见[启动与当日IP池](docs/architecture/DAILY_IP_POOL.md)。Cargo版本保持不变。

## 未发布新crate版本（2026-10-24，四条替代命令迁移）

- 默认心跳改为0004和HeartbeatAck，受管连接空闲30秒自动保活；完整帧锁串行心跳/业务I/O，失败失效、Drop回收及macOS关闭后重连有回归验证。
- 默认目录改为044d：u32偏移、1–1600条/页、37字节记录、16字节名称、f32昨收/量比基准和完整原始槽；完整目录校验后更新精度缓存。
- 默认分时改为0537，新增minute_series保存均价、首点相对字段、合成时间和目录精度；服务端日期保持None，失效精度和未知布局报错。
- 历史单页/全量/TradeQuery::history默认0fc6，上限1800，保留笔数和混合事件；全量event_pages保存原始页/参考字节/来源，既有预算、连接与完整性保护沿用。
- 保留显式legacy入口。新增42组真实帧、52171条目录和720分时点的Python独立核对；三只证券分时末点与053e一致。

检查与实际范围见[四命令实施记录](docs/archive/development/FOUR_COMMANDS_MODERNIZATION_20261024.md)。heartbeat返回类型变化未发布，Cargo版本不变。

## 未发布新crate版本（2026-10-24，增强历史0fc6独立接入）

- 新增EnhancedHistoryTransactionRequest、独立解码及Client::enhanced_history_transaction，每页1–1800条；与旧0fb5入口并存。
- 保留参考价原字节、笔数槽、状态/尾字段、页内索引、请求日期及raw payload；提供SDK兼容的竞价、开盘撮合、盘后成交视图和checked下一偏移。
- 保存单证券四次真实响应，4876条事件全部字段与Python独立解码一致；10项专项及原协议兼容回归通过。全量覆盖、其他品种单位和CLI/runner接入另验。

参数、字段、调用频率和最终工程检查见[实施记录](docs/archive/development/ENHANCED_HISTORY_0FC6_IMPLEMENTATION_20261024.md)。Cargo版本不变，源码未发布。

## 未发布新crate版本（2026-10-24，成交事件模型）

- 新增0fc5/旧0fb5的TradeQuery、只读TradeRecord/TradePage和共享严格解码，保留状态、价格差分、尾字段及原始payload；旧成交API/CSV兼容。
- 事件类型、交易阶段、方向与成交资格分开；状态8优先排除，盘后状态5方向Unknown，未知状态/旧历史候选不静默计为已确认成交。
- 新增TradeCollection的终止原因、派生覆盖状态、分类计数和警告；原始条数推进偏移，保留重复记录，校验查询/连接一致性。
- 新增Client::trade_page/history_trade_page和进程内唯一Tcp连接ID，同地址重连可检测。边界、真实单页与本地传输测试见[实施记录](docs/archive/development/TRADE_EVENT_MODEL_IMPLEMENTATION_20261024.md)。

## 未发布新crate版本（2026-10-24，财务语义与报价精度）

- 默认finance/FinanceInfo切换FinanceSnapshot，保留136字节raw、逐字段单位/状态/证据；金额候选×1000、EPS保持元/股，未知槽不冒用股本名称。初始10字段证据严格限平安银行固定2026-H1记录。
- 默认quotes按完整0450目录的2/3/4位精度还原现价、昨收、OHLC及五档；base/delta先i64合并，金额/量/涨速/时间保持原规则。缺失或不支持精度报错。
- 新增Client独立目录Arc快照、显式共享/预热、完整市场原子更新、跨日失效、失败保留与QuotePage来源。冷加载/报价共用请求deadline。
- 旧财务与固定/100由显式legacy入口保留，原CSV夹具不改；增加官方语义期望、多精度实帧/独立价格回放，适配示例与迁移文档。

实际检查及主站/证据范围见[实施记录](docs/archive/development/FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)。财务返回类型与低层报价签名有破坏性变化；Cargo仍1.12.2/1.8.2，未提交、推送或发包。

## 未发布新crate版本（2026-10-24，通达信采集范围与需求2.2）

- 移除第二行情来源的命令、模块、适配、专用留证/重放、样例、测试夹具及相关配置/文档；保留通达信采集、解析、复权、CSV导出、JSON报告、数据库和runner。
- CLI清单升为schema7，runner兼容schema6/7；同步需求及迁移说明，历史裁剪和第三方资料衍生件保留来源记录。
- Rust1.89 fmt、Clippy、workspace全部目标/全部features、文档及release构建通过；当前CLI驱动3项runner回归通过。
- 4个官方day文件、27,132行与独立解码六字段精确一致，1/4线程CSV相同；固定928条K线证据离线重放通过。

实际命令、摘要及边界见[验证记录](docs/archive/validation/TDX_ONLY_VALIDATION_20261024.md)。本次没有提交、推送、发包或部署，真实生产数据库/全量与未来年度事实另验收。

## 未发布新crate版本（2026-10-23，生产组件实施与需求2.1）

- 新增不发布crate `rustdx-runner 0.1.0`：严格本地day Job、SQLite WAL/FULL账本、任务去重、私有worker/deadline、事实/应有键、封存release、完整日期目录CAS及固定版本消费。
- 新增CH/Mongo独立版本隔离目标，512行批次insert-only、传输前持久请求/intent、完整回执、全字段与release总范围验收；unknown不重写，verified后目录事务中断可恢复，保留原核验凭据身份。
- 新增TCP从握手起完整帧observer、CLI有界fsync/SHA库存与checked离线重放；未压缩正文保持移动路径。新增状态/Prometheus原子textfile、SQLite在线备份与完整归档/数据库证据隔离恢复。
- 交付Job/凭据示例、DBA schema、systemd任务/指标/告警及恢复说明；同步需求FR-15至18、架构/目录和实施证据。
- Rust1.89 workspace fmt、Clippy all-targets/all-features拒绝警告、离线测试/文档通过；runner16项全部执行，包括真实CLI、600只模拟证券/1,200行本机两库、目录失败恢复、确认丢失不重写、WAL恢复和TCP故障。

代码及证据见[实施记录](docs/archive/development/PRODUCTION_IMPLEMENTATION.md)。核心/CLI版本仍为1.12.2/1.8.2，Cargo.lock增加控制层依赖。本次仅本地提交，不推送/发布/部署。目标Linux、全市场事实、权限隔离、全量/长期SLO和G5 HA未验收；本机模拟回归不作为生产上线通过。

## 未发布新crate版本（2026-10-23，目录治理与需求2.0）

- 文档按需求/架构/开发/验证/运营运维/发布/历史归类；脚本按开发/运维/基准/验收/发布归类，更新全部调用路径。
- 二进制样本归tests/fixtures/tdx，SQL归deploy/clickhouse/sql，版本化参考日历归config/calendars；补运行配置、部署、脚本、夹具及数据说明。
- 重写需求文档，保留FR-01至11，新增缓存、来源重放与阶段观测，补角色/对象/状态、异常、非功能约束、验收追踪和生产待补事实。
- 补入四份被通用CSV忽略规则排除的协议期望；修正核心打包根data排除范围及Excel示例的缺失assets依赖。
- Rust1.89全workspace格式/Clippy/离线测试/文档、干净检出相关回归、独立7组协议重解及包清单/源码编译通过；CLI联合上传包离线准备仍缺核心注册表索引，范围见[验证记录](docs/testing/VALIDATION.md)。

当前目录见[项目结构](docs/architecture/PROJECT_STRUCTURE.md)，唯一需求主文档为[requirements/REQUIREMENTS.md](docs/requirements/REQUIREMENTS.md)，旧需求路径保留跳转。仅本地提交，版本与Cargo.lock不变，未推送或发布。

## 未发布新crate版本（2026-10-23，第三轮优化）

- 覆盖巡检、默认4096键可配回读与长驻MongoDB、快照/财务/逐笔checked请求、schema6阶段/临时字节/新鲜度观测。
- 779,200行真实官方历史子集独立逐行核对与两库重复全键/数值回读、四项真实服务故障恢复通过；修正ClickHouse查询格式化配置并保留线程池耗尽失败证据。
- 固定Linux数据库镜像及CI配置已保存，尚未在该平台执行；实际验收使用macOS实验ClickHouse26.11.1.548/MongoDB8.0.16，目标生产/全量与未来年度数据另验收。

详细实现、测量及限制见[第三轮记录](docs/archive/history/OPTIMIZATION_ROUND3.md)。

## 未发布新crate版本（2026-10-23，六步优化与交叉验证）

- data-check更新前逐日比较独立日历，SecurityHistory保留公告摘要及已核实区间，未知证券状态返回None。
- 数据库每批512键独立精确回读全部导出字段，schema3记录database_verification，verified才允许清理CSV；同机同目标写入协调和MongoDB唯一索引检查。
- CLI整任务--timeout（默认10800秒）、--cancel-file及Ctrl-C/SIGTERM协作取消；TCP/HTTP共用进程/同机OS锁许可，清单记录实际资源及窗口P95。
- day以4096条分块，质量和累计收益跨块承接，非A股磁盘归并，目录/文件/日期输出确定；大样本4线程RSS219.5→10.9MiB，新旧字段摘要一致。
- KlineRequest私有参数、股票/指数独立checked解码，Client迁移并保留旧类型。
- 固定pytdx参考真实报文、TDX与隔离数据库小表验收通过；Rust1.89完整检查及627,152次ASan/覆盖率fuzz通过。

本轮仅本地提交，Cargo版本保持1.12.2/1.8.2，未推送或发布。2027/证券历史、目标生产配置和全量负载及长期P95边界见[六步记录](docs/archive/history/NEXT_OPTIMIZATION.md)，当前架构见[架构评估](docs/architecture/PROJECT_ANALYSIS.md)。

## 未发布新crate版本（2026-10-22，仓库源码优化与文档交付）

- 日历提供版本、来源与覆盖范围，严格导航支持外部休市数据；历史涨跌幅及主板ST制度切换按日期处理。
- 进程共享连接预算、端点轮换/冷却、连接/请求/分页/批次总体时限及网络统计。
- 标准行情显式来源、周期、价格口径和单位；股票名单HTTPS、响应大小和时限限制。
- day持久化运行清单、SHA-256、行数/逻辑键/日期边界、ClickHouse query_id和不确定部分写入状态；数据库导入超时，失败保留CSV。
- 布林带双栈滚动方差及KDJ单调队列，正常数据O(n)；保留预热、缺口及平滑口径。
- Client分页/复权与CLI数据库sink/schema拆分；新增checked day/lc/gbbq入口，移除解密unsafe及偏移加法溢出。
- 属性测试、模糊测试目标、窗口性能观测和all-features/MSRV CI。行为边界见[优化记录](docs/archive/history/OPTIMIZATION.md)及[导入说明](docs/operations/IMPORTS.md)。
- 补齐需求、方案、开发、测试、运营、运维和版本交付文档，更新旧架构/迁移/验收描述并建立变更对应表，见[文档索引](docs/README.md)。
- 文档分支合入main，保留两个历史快照：`history/source-20261022`（1f9f739）和 `history/release-1.12.2-1.8.2`（904b33e）；完整提交链继续保留，见[版本交付](docs/release/RELEASES.md)。

源码实现提交为7cd02e7、5fb5b70、401265c、1f9f739，四个提交已推送main。Cargo版本仍为1.12.2/1.8.2，crates.io同版本不含本轮新增行为。Rust1.89离线workspace/all-features、Clippy、格式通过；ASan/覆盖率fuzz544,705次无崩溃，未重跑外部行情或数据库验收，见[验证方法](docs/testing/VALIDATION.md)。

## jelven-rustdx-complete 1.12.2 / jelven-rustdx-cli 1.8.2（2026-10-11，已发布 crates.io）

- 修复行情前后复权：可用历史首日不重复应用旧事件，停牌期间连续事件逐次承接参考价；非法价格、事件或因子返回错误。
- 涨跌停使用整数分及整数百分比舍入，修正半分误差与主板代码范围；昨收要求有效分单位报价。
- 证券列表拒绝提前空页、重复代码、数量越界；证券数量解析失败清空旧值，旧解析入口不因短报文或乱码名称panic。
- 重连握手失败计入重试预算，尚有预算时继续；当日分时回退固定使用UTC+8日期。
- `day` 统一检查相对/绝对路径、`..`及符号链接冲突，保护输入文件、报告、保留CSV和factor.csv；同一输出使用跨进程文件锁。
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
- Excel前导零与输入校验。
- 单标准握手修复当前服务器K线错误；非A股源异常保留警告，输出排序去重。
- Rust1.89验证，Linux/macOS/Windows CI；发布收敛至crates.io源码库。
- 本地真实行情、官网全量包及2981万行ClickHouse验收，详见 [验收报告](docs/testing/PRODUCTION_ACCEPTANCE.md)。

更早的实现历史可通过 Git 查看。当前维护文档以本仓库源码行为为准。
