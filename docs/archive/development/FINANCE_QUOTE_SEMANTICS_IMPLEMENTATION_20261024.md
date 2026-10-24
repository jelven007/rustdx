# 财务语义与报价精度实施记录

日期：2026-10-24。用户批准[修复方案](FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)后，本轮已实现默认业务入口、回归、示例和迁移文档。核心Cargo仍1.12.2、CLI仍1.8.2；本轮没有提交、推送、发包或部署，保留原工作区已有改动。Rust2024/MSRV1.89，同步I/O与有界线程设计不变。

## 实际接口与行为

财务请求首先通过[FinanceRaw](../../../crates/rustdx-core/src/tcp/stock/finance_raw.rs)检查身份、数量、日期、全部浮点有限性和完整长度，保存136字节主体。原始位模式、更新/上市日、地区/行业可以回放；未知日期0保留为None。负利润、现金流、EPS、权益不当作坏报文。

[FinanceSnapshot](../../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs)包含31项原值/单位/状态/证据，金额基线候选×1000、股份×10000、EPS/每股指标/人数原值，7个未确定槽只保留raw。先f32扩宽为f64再换算。`field.value()`仅返回逐字段已验证值，`candidate_value()`要求显式选择候选，未知单位返回None。

内置证据严格匹配深圳000001、更新日20260815及10项各自raw bits；对应官方2026-H1报告期20260630。每个EPS/利润/权益口径只在对应字段获证时标注，不因其它字段获证而传播。其它证券/日期/字段保留候选或未知；可用`FinanceProfile::new(...).verify(field, raw_bits)`注入调用者自备独立资料。人工profile只确认调用者选择的字段，未自动获得银行口径。

默认`Client::finance/finance_snapshot`、`FinanceRequest::recv`和`FinanceInfo::recv_parsed`返回新Snapshot，live记录获取时间/节点。历史`FinanceInfoData`通过`finance_legacy/recv_legacy/decode_finance_legacy`显式使用，仍复现旧pytdx单位和名称。[validate_finance_snapshot](../../../crates/rustdx-core/src/tcp/stock/validator.rs)对候选字段、未知报告期、经营风险及可疑口径给Warning，协议坏记录由解码器报错；不自动计算PE、年化/TTM或强制EPS/权益等式。

报价在[quotes.rs](../../../crates/rustdx-core/src/tcp/stock/quotes.rs)中拆成raw读槽与精度还原，base/delta先i64相加，现价、昨收、OHLC及买卖五档使用同一证券目录尺度。[PriceScale](../../../crates/rustdx-core/src/tcp/stock/price_scale.rs)支持decimal2/3/4对应100/1000/10000；0/1及其它值返回Unsupported，缺失返回MissingPrecision。金额独立编码、各类量、涨速和时间保持原规则。合法零价/零档保留。

`decode_quotes(request, bytes, &PrecisionSnapshot)`及`decode_quotes_with_metadata`只解码，不联网；离线显式历史快照可以重放。`SecurityQuotes`离线必须先`with_precision(Arc<...>)`，live可加载目录。历史固定/100使用`decode_quotes_legacy/recv_legacy/try_parse_legacy`。

[Client](../../../crates/rustdx-core/src/tcp/stock/client.rs)保存默认独立的`QuoteMetadataStore`，显式`with_quote_metadata`才共享。完整0450目录锁外加载并检查，成功后一次替换不可变Arc及generation；多市场refresh后半失败不发布前半，原快照继续保留。单目录页严格拒绝超1000条及多余字节，不单独发布缓存。

`refresh_quote_metadata`使用默认120秒分页deadline；第一次`quotes`冷加载、报价与可选空帧重查共用默认15秒总体请求deadline，并提示可先预热。`Client::stocks`取得完整目录也可预热。缓存按北京时间采集自然日失效，目录加载跨日、live传输前后发现跨日/未来时间均拒绝；没有默认stale模式。同generation查不到证券不会每次整市场重拉。`quotes_with_metadata`返回请求/返回/缺失身份、每证券decimal/source、metadata generation及获取时间。

## 回归与工程检查

新增[finance_semantics](../../../crates/rustdx-core/tests/finance_semantics.rs)5项与[quote_precision](../../../crates/rustdx-core/tests/quote_precision.rs)10项。财务逐槽检查位模式/单位、官方10字段容差、证据的证券/日期/字段范围、负值、未知日期、全部非有限槽和状态清空；其它沪深证券用明确合成记录验证可扩展profile，没有冒充其它发行人财报核验。

报价覆盖混合2/3/4全部价格、金额/量/时间不缩放、跨市场同代码、i64边界、合法零档、缺失/不支持精度、截断/尾随/重复身份、并发刷新下固定generation、北京时间午夜/未来时间、冷加载/热缓存、同版本缺证券不重拉、刷新失败原子性、整体deadline和实际多精度回放。现有protocol/reliability回归增加显式目录，保留旧CSV比较，另验证默认Client及FinanceInfo的新模型和live来源。

最终使用仓库本地Rust1.89工具链执行：

```sh
export CARGO_HOME=/Users/bytedance/Documents/rustdx/.cargo
export RUSTUP_HOME=/Users/bytedance/Documents/rustdx/.cargo/rustup
.cargo/bin/cargo +1.89.0 fmt --all -- --check
.cargo/bin/cargo +1.89.0 clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 .cargo/bin/cargo +1.89.0 test --workspace --all-features --offline --locked
```

fmt和Clippy均通过。最终Cargo报告41个测试组共359 passed、0 failed、23 ignored（含文档测试）；结果及日志摘要见[实施证据](../../testing/evidence/finance-quote-semantics-20261024/implementation-evidence.json)。设置外部测试skip、未启用数据库opt-in；框架中的提前返回和ignored不计为新增线上/数据库验收。所有本地协议与新语义回归实际执行。fuzz目标已适配新签名，本轮未执行fuzzer或release/跨平台构建。

`Cargo.toml/Cargo.lock`只为核心测试与留证示例增加已有锁定版本的serde_json dev依赖；未引入运行时依赖或async。原finance.response.hex/finance.expected.csv及两位报价response.hex不变。

## 当前实帧与独立价格

通过本库[有界留证示例](../../../crates/rustdx-core/examples/capture_quote_precision.rs)采集115.238.56.198:7709，先对显式单节点运行现有三轮当日IP检测，再复用受管Tcp加载两市场完整0450目录及053e。每次整体90秒、最多200个留证帧，不执行第三方SDK，完整请求/压缩帧/解压正文和SHA清单已保存。

两市场目录共52,171项：深圳24,460、上海27,711；decimal2有6,980项、3有2,631项、4有42,560项。目录分布只说明该节点返回的metadata，不代表这些品种都活跃或全字段已验证。采集日在周六2026-10-24，独立返回时间为2026-10-23闭市后的数据。

| 证券 | 目录decimal | TDX价格 | 独立腾讯价格 | 范围 |
| --- | ---: | ---: | ---: | --- |
| sz000001 | 2 | 11.20 | 11.20 | 现价/昨收/OHLC/十档共15价全部相同 |
| sz159915 | 3 | 2.702 | 2.702 | 同上 |
| sh510300 | 3 | 4.178 | 4.178 | 同上 |
| sh018003 | 4 | 111.305 | 111.305 | 同上，原量500、有实际交易 |
| sz101123 | 4 | 121.4613 | 121.461 | 参考展示3位，差0.0003在0.0005展示容差内；原量0 |

初次四位选择为无交易的100元债券，单独保留在[live初采](../../testing/evidence/finance-quote-semantics-20261024/live-115-238-56-198/manifest.json)，不靠该样本证明活跃报价。第二次选择包括活跃债券和非整数价格；[完整实帧](../../testing/evidence/finance-quote-semantics-20261024/live-active-115-238-56-198/manifest.json)保存15只证券，[独立原文](../../testing/evidence/finance-quote-semantics-20261024/tencent-independent.txt)来自腾讯公开查询，SHA及15价格比较在实施证据中。它是独立供应商佐证，不是交易所官方报价；四位尾数尚缺独立四位展示的直接核验。

另外发现深圳目录的`pre_close`对这两只股票/ETF与报价昨收不一致；本轮只使用目录`decimal_point`，没有用目录pre_close定标。部分无交易证券amount解码为极小正数，沿用原独立金额编码，本轮不将该金额或目录pre_close记为已核验。

## 兼容与剩余证据

财务默认返回类型、FinanceInfo的数据类型、低层报价签名有破坏性变化；完整调用迁移见[MIGRATION](../../release/MIGRATION.md)。旧CSV仅作为legacy证据，新[finance.semantic.expected.json](../../../crates/rustdx-core/tests/fixtures/protocol/finance.semantic.expected.json)来自官方披露，[quotes.precision.expected.json](../../../crates/rustdx-core/tests/fixtures/protocol/quotes.precision.expected.json)来自独立价格；不得覆盖旧夹具后宣称验证。

本轮已完成0010/053e实现及单主站闭市2/3/4尺度佐证。尚未扩展非银行、不同报告期的独立财务证据；未覆盖所有主站/盘中变更/四位独立末位展示、decimal0/1、其它协议价格尺度或品种量单位。CLI/runner日线主链未调用这两个业务API，本轮不重写已有数据库。历史结果仅在raw及schema/精度来源充分时重解，不统一除10。
