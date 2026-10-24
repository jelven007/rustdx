# 通达信2026年10月公开协议研究资料

保存日期：2026-10-23。说明及结论见[研究报告](../../../architecture/TDX_PROTOCOL_RESEARCH_20261023.md)，版本、命令差异、采样时间及边界见[研究证据](../tdx-protocol-research-20261023.json)。

本目录保存公开资料，作为协议研究附件，不进入Cargo依赖或运行路径。原始源码包只下载、校验和静态阅读；没有安装SDK、执行示例、加载原生动态库或访问行情服务器。资料中的行情示例来自原作者，不属于本次独立采集。2026-10-24按通达信采集范围裁剪三份附件，均明确标记为filtered_derivative，保留原摘要与转换说明。

## 直接阅读

| 内容 | 固定资料 |
| --- | --- |
| 官方最新更新 | [官网原始页面](official/official-versions.html)，10月23日1.3.1注明“仅内测版支持” |
| 官方1.3.1具体变化 | [tdxaidata CHANGELOG](tdxaidata-1.3.1/CHANGELOG.md) |
| 官方接口完整字段及用法 | [tdxaidata包内使用文档](tdxaidata-1.3.1/src/tdxaidata/docs/tdxaidata使用文档.md) |
| 官方本地17709 HTTP | [官网说明快照](official/official-http17709.html) |
| 官方后台SDK与Key | [官网说明快照](official/official-tdxaidata.html) |
| 官方K线及量额单位 | [官网说明快照](official/official-market-data.html) |
| 社区最新26个7709命令 | [eltdx COMMANDS_7709](eltdx-3.3.0/docs/COMMANDS_7709.md) |
| 社区逐接口说明 | [eltdx methods目录](eltdx-3.3.0/docs/methods/README.md)、[字段字典](eltdx-3.3.0/docs/FIELD_REFERENCE.md) |
| 社区近期变化 | [eltdx CHANGELOG](eltdx-3.3.0/docs/CHANGELOG.md)、[3.3.0说明](eltdx-3.3.0/docs/releases/v3.3.0.md)、[3.2.3说明](eltdx-3.3.0/docs/releases/v3.2.3.md) |
| “21个命令”历史来源 | [eltdx 3.0.0发布说明](eltdx-3.3.0/docs/releases/v3.0.0.md) |
| 命令登记与请求编码 | [eltdx request.rs](eltdx-3.3.0/crates/eltdx-protocol/src/request.rs) |
| K线编码与解码 | [eltdx klines.rs](eltdx-3.3.0/crates/eltdx-protocol/src/commands/klines.rs)、[周期及复权定义](eltdx-3.3.0/crates/eltdx-protocol/src/unit.rs) |
| 10月Go库实现对照 | [injoyai命令常量](injoyai-tdx/protocol/const.go)、[逐笔时间解析](injoyai-tdx/protocol/model_history_trade.go) |
| 发布及故障证据 | [PyPI元数据](registries/pypi-tdxaidata.json)、[eltdx发布记录](github/eltdx-latest-release.json)、[特定环境空响应issue摘录](github/empty-response-report.json) |

## 原始安装包

- [tdxaidata-1.3.1.tar.gz](archives/tdxaidata-1.3.1.tar.gz)：官方后台SDK最新公开上传包，4,516,507字节；功能支持以官网内测说明为准。
- eltdx 3.3.0原始源码包已于2026-10-24移除；保留展开后的通达信协议源码、文档和许可，以及[分发记录摘录](registries/pypi-eltdx.json)。原包身份、摘要、源URL及移除原因见清单omitted_source_archives，不再作为本地附件。
- [tdx_cli-0.1.7.tar.gz](archives/tdx_cli-0.1.7.tar.gz)：9月27日背景资料，50,637字节，不是10月新协议证据。

最初取得的三个包SHA-256均与对应PyPI分发记录一致，目前本地保留其中两个原包。[SOURCE_MANIFEST.json](SOURCE_MANIFEST.json)记录205份附件的相对路径、字节数、SHA-256及来源，其中202份保持原字节、3份为显式裁剪衍生件；本说明文件及清单自身不计入205份附件。当前附件合计6,827,427字节。origin.archive表示当时用于提取的上游包身份，已移除原包单独记录。

从安装包中展开的文档及协议源码保持原字节，没有修复原作者文字、示例或相对链接。三份衍生件为eltdx PyPI元数据（只保留相关sdist和包身份）、空响应issue（只保留身份/标题/时点）和Go快照源码（三处跨来源比较注释裁剪，解码语句保留）；衍生件当前摘要与原摘要分别记录。部分文档面向其在线文档站，可能包含网站宏、样式标记或已过期的文档名；应结合本报告及实际文件阅读。官网HTML是来源快照，其页面资源仍使用官网地址，不是重新制作的离线网站。

## 许可与证据口径

eltdx使用[ELTDX Non-Commercial Research License](eltdx-3.3.0/LICENSE)，允许个人学习、复制及协议研究，禁止商业或生产服务使用。injoyai/tdx的[MIT许可](injoyai-tdx/LICENSE)和tdxaidata的[包内MIT许可](tdxaidata-1.3.1/LICENSE)亦保留。代码许可不代替行情数据服务的使用条件。

GitHub快照、安装包及官网页面分别固定各自观察时点。GitHub主分支SHA不被当作PyPI整个安装包的等价摘要。社区接口规则是该版本作者的协议重建和测试描述，本次未独立验证全部服务器、品种、数量单位或最新线上报文。
