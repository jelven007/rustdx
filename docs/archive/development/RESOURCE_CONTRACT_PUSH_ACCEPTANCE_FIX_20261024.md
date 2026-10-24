# 通用资源合同与0547验收门禁修复

日期：2026-10-24。通用300字节06b9读取、预算/末尾合同和已知统计ZIP解析已补齐，并取得两节点完整字节及全部CFG槽的独立重放证据。0547工程门禁已加强；当天为周六，本轮没有盘中主动推送事实，实盘项保持待验。

## 变更与消费边界

新增`Client::file_content/resource_file/resource_stats`及对应自由函数、请求/DTO。通用06b9路径字段为300字节，ASCII、无NUL、trim及反斜线规范化；块大小1–60000，offset+size不得溢出。响应前4字节按独立通用协议配置解释为声明块长，拒绝超请求/截断；声明以外的尾部保留，不拼入文件。旧100字节板块请求和已冻结板块头部的未知状态保持原合同。

完整下载有文件、块数、响应和总时限预算，并核对节点/连接/UTC+8观察日连续性。只有短块或空块能置`complete=true`；字节预算恰好填满、块数或响应预算耗尽都交付显式不完整前缀，`require_complete()`拒绝消费。通用下载不带02c5声明MD5，也不保证资源在扫描期间保持原子版本。

统计入口要求完整文件后才解析ZIP；检查原始中央目录和成员数量/重复名称，避免ZIP库按名称建立索引后隐藏重复记录。最多32MiB压缩包、128成员、单成员32MiB、累计解压64MiB，拒绝加密、多卷/ZIP64及缺少必需CFG。校验必需成员CRC和GBK、唯一市场代码键；每CFG最多100000行、单行16384 UTF-8字节。原始列、原始日期文本、未识别行和压缩包/CFG字节保留。命名数值全部放在`candidate_values`；非法/非有限数值为None，整数槽不截断小数或饱和溢出。

协议与槽位依据为已固定的eltdx3.3.0源码；[参考副本及来源](../../testing/evidence/resource-contract-push-fix-20261024/reference/provenance.json)记录原始tar及成员摘要。统计单位、日期资格、供应商分类和行情市场编号对应没有因结构重放通过而升级。

0547严格验收新增两项必要条件：

1. 最终快照必须仍为Synchronized，连接/代次不变，全部订阅键有对应完整报价；独立核验拒绝缺键、错身份和错代次。
2. 结束时使用实际精确时间判断同一交易会话，避免对耗时取整后漏掉刚越过午休/收盘的情况。观察完成还检查单调耗时及UTC时钟持续时长，证据核验检查阶段顺序、事件时钟、接收时点和请求轨迹。

共同消费规则见[应用接入](../../development/APPLICATION_INTEGRATION.md)，公开签名、默认预算和完整字段见[06b9手册](../../reference/COMMAND_API.md#cmd-06b9)。需求追踪保持FR-04、FR-07、FR-10、FR-13；全字段资格仍按[字段状态](../../reference/RESPONSE_FIELD_SEMANTICS.md)判定。

## 最终源码的真实样本

两个端点分别通过独立当日IP池三轮探测，使用独立连接、auto_reconnect=0、心跳关闭、180秒总预算。块间隔25ms、文件间隔250ms。最终采样时间为北京时间2026-10-24 22:49；此时间只界定资源取得时点，不是统计行情日期，精确起止见节点summary.json。

| 项目 | 115.238.56.198:7709 | 180.153.18.170:7709 |
| --- | ---: | ---: |
| 完整响应帧 | 57 | 57 |
| 压缩传输帧 | 24 | 24 |
| zhb.zip字节/块数 | 1,338,014 / 45 | 1,338,014 / 45 |
| block_zs.dat字节/块数 | 329,507 / 11 | 329,507 / 11 |
| 文件终止状态 | 均ShortChunk | 均ShortChunk |
| ZIP成员CRC独立核验 | 47 | 47 |
| tdxstat.cfg记录/原始槽 | 8,101 / 283,535 | 8,101 / 283,535 |
| tdxstat2.cfg记录/原始槽 | 8,101 / 170,121 | 8,101 / 170,121 |
| 候选值映射独立核验 | 153,919 | 153,919 |
| 未识别CFG行 | 0 | 0 |

共114帧、32,404条CFG记录、907,312原始字段槽及307,838候选映射精确一致。Python核验只用标准库，独立重解消息身份、zlib、声明块长、拼接边界、源连续性、ZIP CRC、GBK和全部CFG槽；没有调用Rust解码器，也没有将槽位映射的一致性称为业务单位独立核验。

两节点文件字节相同。zhb.zip SHA-256为`fde08da903a18edf8edac2d1150d1ccac20a836cde08d14b61809c3c65abdba2`；block_zs.dat为`fcc3befb6299f06af053f39bfc44a4ffe60d2112ba66c4161c571a5299b71151`。本次通用接口计算的MD5仅是内容摘要，没有服务器声明hash资格；旧板块MD5验算证据另行保留。

入口：[杭州独立核验](../../testing/evidence/resource-contract-push-fix-20261024/live-hz/independent.json)、[上海独立核验](../../testing/evidence/resource-contract-push-fix-20261024/live-sh/independent.json)、[机器摘要](../../testing/evidence/resource-contract-push-fix-20261024/summary.json)。

## 0547本轮结果

新最终二进制执行默认30秒门禁，返回退出码2、`market_closed`，未建立连接，完整帧和被动请求均为0。独立核验结果`live_trading_push_verified=false`，见[门禁证据](../../testing/evidence/resource-contract-push-fix-20261024/closed-gate/summary.json)。

加强后的核验器只读重放此前两个休市诊断及默认门禁：76帧仍通过结构核验，业务结果仍为diagnostic_not_verified/market_closed。旧材料没有被改成盘中结果，也没有重复写入独立核验文件。本轮没有再次把相同休市等待当作新增盘中证据。

下一个内置日历交易日为2026-10-26，建议北京时间09:35运行下列命令；没有创建定时任务。两个节点分别使用新目录执行，输出目录不能预先存在：

```sh
env CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.cargo/rustup" \
  .cargo/bin/cargo +1.89.0 run --offline --locked \
  -p jelven-rustdx-complete --example validate_quote_push -- \
  docs/testing/evidence/quote-push-live-20261026-hz \
  115.238.56.198:7709 --seconds 180

python3 scripts/validation/verify_quote_push.py \
  docs/testing/evidence/quote-push-live-20261026-hz --freeze
```

上海端点为180.153.18.170:7709，输出目录另用`quote-push-live-20261026-sh`。仅盘中工具退出码0且独立结果明确`live_trading_push_verified=true`时，才能关闭所列端点和订阅证券的实盘项；无有效更新仍保持not_verified。

## 工程检查与身份

环境为macOS arm64、Rust1.89.0、Python3.9.6。新增ZIP依赖固定2.4.2，仅开启deflate；没有引入异步运行时。

| 检查 | 结果 |
| --- | --- |
| 新资源合同/ZIP故障回归 | 7项通过 |
| 既有板块完整性与订阅传输回归 | 10+21项通过 |
| 0547验收工具时间/最终状态/连续变化回归 | 6项通过 |
| 相关Rust合计 | 44项通过 |
| Python推送判定/快照/时段回归 | 3项通过 |
| 修改证据副本负向验证 | 8项全部拒绝 |
| 工作区fmt检查 | 通过 |
| 工作区全target/feature Clippy，-D warnings | 通过 |
| 工作区all-features测试及doctest | 565通过，0失败，27 ignored |

工作区命令设置`RUSTDX_SKIP_INTEGRATION_TESTS=1`；外部服务测试按既有环境门禁跳过，不能把通过统计解释为本轮执行了真实数据库/Linux/生产验收。相关44项Rust行为检查单独执行，日志记录命令和退出码。

负向验证修改的是临时副本：不完整资源、候选值篡改、缺少CFG行、损坏声明长度/正文、缺少最终订阅键、错连接、错代次、将休市结果改为passed，均拒绝。原始资源和旧证据未修改。旧板块支持包1,035个文件、旧推送包408个文件重新按原摘要核验一致，见[旧证据完整性](../../testing/evidence/resource-contract-push-fix-20261024/previous-evidence-integrity.json)。

固定源码清单包含858项Cargo输入、各包源码/测试/夹具/示例和实际使用的Python核验脚本，SHA-256为`c6567d9901ffb49939065ee8a93038474bdc7d17788c3797291db163296e0671`。源码为当前未提交工作区，版本号不充当身份。资源采样二进制SHA-256为`184eb4171c9447f732df40c84b007673277db74432c0bb1d29f596e13d2158af`；推送工具为`42677f30e040e2ee1896b7c3ef528db8f09683011cc83e9914d8e0fcbd743cd2`。实际使用workspace测试构建后的示例程序（test profile，opt-level=3），在所有构建结束后固定二进制摘要、采样并再次核对。初次采样运行前没有固定二进制摘要，不计入本表最终统计，原字节另保留在attempt1目录；其后采集的environment记录不能追认初次运行二进制。详见[源码](../../testing/evidence/resource-contract-push-fix-20261024/source-manifest.json)、[采样前身份](../../testing/evidence/resource-contract-push-fix-20261024/binary-before-capture.json)、[采样后环境](../../testing/evidence/resource-contract-push-fix-20261024/environment.json)及[检查日志](../../testing/evidence/resource-contract-push-fix-20261024/checks)。

通用资源传输及所采zhb布局的工程/真实样本验收已取得；其他资源格式、业务统计单位/日期、下载期间原子性、生产容量及实际盘中主动推送仍需各自证据。本轮没有提交或发布代码，没有执行Linux、数据库恢复或fuzz验收。
