# 板块文件完整性修复

2026-10-24。当前未提交工作区已补齐02c5/06b9板块完整文件校验，并关闭高层畸形文件返回空表或部分成分的问题。最终macOS工作区回归558项通过、27项ignored，新增10项板块专项全部通过，fmt及全工作区Clippy `-D warnings`通过。两个真实节点的概念/指数文件经独立原帧重解、MD5及全部105,826条成分字段核对一致。此结论限定于板块传输完整性和所列文件布局。

## 修复行为与原因

旧下载仅按02c5的size读取06b9块，没有验算服务器声明的MD5；size=0直接返回空。旧解析器按向上取整分槽，遇到截断会提前结束，成员可能越过本槽，GBK及非法代码可能被容错处理。因此长度相同的损坏文件及格式异常可能作为成功结果交付。

`BlockMetadata::verify_content`现在要求原始MD5字段为32位十六进制ASCII，核对完整字节数与RustCrypto `md-5 0.10.6`计算值，接受大小写声明，保留原字段及未知元数据。空文件也必须匹配空字节MD5。

`download_block_file/Client::block_file`每块读取30000字节，最多64MiB，共享分页总时限；每块必须精确匹配请求长度，与元数据保持同端点、连接及北京时间观察日。中途重连返回错误，调用方从元数据重新开始。`BlockFile`保存完整content、metadata、computed_md5以及每块start/length/header_raw/source；content及raw_payload不进入serde JSON，归档必须显式保存字节。

`get_block_info/Client::block`经下载校验后使用严格`decode_block_content`：固定槽位须整除且至少13字节，成分数不得超出本槽容量，名称须合法GBK，代码须为六位ASCII数字；错误不交付部分结果。空字节文件或386字节零板块头可以返回空。旧`parse_block_content`保持历史兼容行为，`Client::block`保留底层IO错误kind。

实现见[下载与解析](../../../crates/rustdx-core/src/tcp/stock/block_info.rs)、[MD5验算](../../../crates/rustdx-core/src/tcp/stock/resource_metadata.rs)、[专项回归](../../../crates/rustdx-core/tests/block_integrity.rs)。需求追踪为FR-04、FR-07、FR-13，消费规则统一见[应用接入](../../development/APPLICATION_INTEGRATION.md)。

## 两节点完整文件核对

最终源码重新构建采样程序后，分别从115.238.56.198:7709及180.153.18.170:7709下载。每节点先经独立IpPool三轮验证，再使用受管Tcp；关闭自动重连和空闲心跳。每节点整任务180秒、帧128个/归档字节16MiB上限；每文件共享网络策略的120秒分页预算。

| 文件（每节点） | 文件字节 | 块数 | 板块槽数 | 槽位字节 | 成分条数 | 声明/实际MD5 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| block_gn.dat | 757,083 | 26 | 269 | 2,813 | 41,359 | 135dd66cf5b2e52abcdd967633465a4f |
| block_zs.dat | 329,507 | 11 | 117 | 2,813 | 11,554 | deddfa997800581cf5cff4ea5cf200ca |

共4个文件、80帧，其中76个压缩帧。独立Python仅使用struct/zlib/hashlib/严格GBK，校验响应前缀、opcode、消息号、解压长度、文件名与分块偏移、全部原字节、声明MD5、每块原始前4字节以及Rust输出的全部blockname/block_type/code。两个节点对应文件SHA-256及全部成分一致，不能仅由两节点相同推导分类权威性。

[独立核对JSON](../../testing/evidence/block-integrity-fix-20261024/independent.json)与[最终原始支持包](../../testing/evidence/block-integrity-fix-20261024)保留请求、header、wire、payload、端点/时间、完整文件和输出。`initial/`保存最终格式整理前的首次采样，最终重采写入独立的live-a/live-b；就近指数夹具从首次原字节复制，最终内容一致。旧02c5元数据证据及此前0feb/TCP日线冻结包没有修改。

离线重核示例（输出到新的结果文件）：

```sh
python3 scripts/validation/verify_block_integrity.py \
  docs/testing/evidence/block-integrity-fix-20261024 \
  --output /tmp/rustdx-board-recheck.json
```

## 检查、源码身份与剩余边界

环境为macOS arm64、Rust1.89.0、edition2024、Python3.9.6。最终执行：

```sh
cargo +1.89.0 fmt --all --check
cargo +1.89.0 clippy --workspace --all-targets --all-features -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace
```

工作区Cargo汇总558 passed、0 failed、27 ignored；其中10项新增专项覆盖已知MD5、同长内容变更、长度错误、非法hash、空文件、64MiB上限、正常多块、短块、畸形槽/名称/代码、高层错误kind、中途重连及总deadline。完整指数夹具还校验独立Python生成的全部成分JSON摘要。端点/连接/跨北京时间自然日比较沿用`check_scan_source`既有回归。

初次Clippy发现测试NUL字面量的歧义写法，初次工作区测试发现macOS超时可返回WouldBlock；修正字面量及平台断言后重跑最终完整检查，原失败日志也保留。558是Cargo汇总，环境变量保护的真实网络/数据库检查未计为实际业务执行；27项ignored没有运行。本轮没有重跑Linux、fuzz、数据库原生恢复或盘中0547主动推送。

所选249项源码/清单/锁文件/验证工具/新夹具的身份为`7631261ad483205e77b1d165f428aa696bcfb13ccdf38adff431070dd45e67de`，详见[源码清单](../../testing/evidence/block-integrity-fix-20261024/source-manifest.json)；它不是Git HEAD，也不是独立源码归档。采样程序另保留源码和独立Cargo.lock，二进制SHA-256为`a7d5b0f0f72bff82aaf6fb33a18b296796aa2e26c5faea826dcbc82974c69885`。检查命令、退出码、工具身份及支持包摘要见[机器摘要](../../testing/evidence/block-integrity-fix-20261024.json)。

06b9正文前4字节、代码第7字节及未用槽位字节仍保留原样，不赋予市场或状态语义；BlockRecord.code缺少market，板块类型不是交易所官方行业分类。未采样block_fg.dat/block.dat，未扩展eltdx的300字节通用资源或压缩统计文件合同。0feb通用日期资格、完整保留窗口、真实全市场及目标生产门禁继续见[验收矩阵](../../testing/PRODUCTION_ACCEPTANCE.md)。未提交、推送或发布。
