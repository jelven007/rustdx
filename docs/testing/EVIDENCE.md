# 证据与固定样本管理

<!-- lifecycle:registry -->
读者：测试/审查；责任：测试维护者。源码核验：2026-10-27；每90日及证据schema、路径或保全规则变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

更新日期：2026-10-25。证据用于证明明确源码、时点、环境和输入范围下的结论；摘要验证字节完整性，不证明公告或第三方返回的真实性。

## 1. 位置与内容

| 位置 | 内容 |
| --- | --- |
| crates/rustdx-core/tests/fixtures | 核心就近二进制、报文、独立期望及manifest |
| apps各crate/tests | CLI/控制层回归及就近样本 |
| docs/testing/evidence | 固定原始采样、独立核验、源码参考快照和机器结果 |
| docs/archive | 对应轮次的分析、实施、验收报告 |
| 显式运行目录 | 本次运行清单、私有输入、输出、现场响应和备份；按部署保留期管理 |

## 2. 留存原则

原始请求/header/wire/payload、采样参数、节点、接收时间和字节摘要一起留存。期望来自独立解码或明确外部依据；合成数据注明synthetic，模拟证券事实不冒充真实市场覆盖。

新采样写入新目录，不能覆盖旧输入或更新旧SHA使历史测试“通过”。每份新结论记录源码、工具版本、命令、退出码、规模、字段范围、跳过项和限制。临时/tmp日志本身不是长期证据，必要部分复制进固定支持包。

上游源码快照和许可证是协议/参考来源，不能因为位于证据目录而作为冗余源码删除。运行数据不因Git目录清理而删去。

## 3. 路径迁移

本次docs/validation/evidence迁至docs/testing/evidence，tests/fixtures迁至crates/rustdx-core/tests/fixtures。原始JSON中的相对/绝对采样路径、源码摘要和历史日志保持原字节。

[迁移清单](evidence/project-restructure-20261024/migration.json)记录旧路径、新路径、原大小及SHA-256。核验时先按最长路径前缀解析，再比对原字节；实际采样时的绝对临时目录不自动改写成当前存在的现场。

历史固定源码摘要用于当时源码复现；当前源码因修复而不同，应明确报告差异。对新源码执行行为验证另存结果，不改写历史基线。

本次保全和工程检查结果见[verification.json](evidence/project-restructure-20261024/verification.json)。被删除文件记录原因，历史报告中的该链接可追溯至保留的实施结果和需求定义。

## 4. 工程与历史材料分层

`preflight.py`的scope1只打包工程输入，排除大型证据和旧project-review目录。
就近小夹具仍支持离线测试；[来源清单](../../config/release/offline-fixtures.json)记录
原始路径/摘要及新副本。98帧回放的大型日线JSON仅生成已断言字段的紧凑新夹具，
独立原期望和所有旧字节保留；金融发行人PDF复制到核心夹具并保留原路径。

```sh
python3 scripts/release/evidence_manifest.py inventory \
  --include-local-binaries --output /review/new-evidence.json
python3 scripts/release/evidence_manifest.py verify \
  --manifest /review/new-evidence.json --root /other/checkout \
  --output /review/new-verification.json
python3 scripts/release/evidence_manifest.py bundle \
  --manifest /review/new-evidence.json \
  --prefix docs/testing/evidence/system-review-20261025/ \
  --archive /review/new-bundle.tar.gz --output /review/new-bundle.json
python3 scripts/release/evidence_manifest.py restore \
  --manifest /review/new-bundle.json --archive /review/new-bundle.tar.gz \
  --root /review/new-restored-root --output /review/new-restore.json
```

`restore`要求目标根目录尚不存在、父目录已存在。它先核对归档大小/SHA，在私有暂存目录核对全部条目、原路径、大小和SHA，再生成新根目录。损坏/缺少/额外条目、重复、链接和路径穿越明确失败，不覆盖已有工作树。旧清单的路径原样恢复；迁移映射仍在使用阶段独立解析。按需取得归档后可离线恢复；远端不可变存储尚须部署方配置，工具不自动上传或删除历史证据。

库存逐文件流式SHA-256；已提交原件同时核对HEAD的Git blob身份/模式，改变即拒绝，
不刷新旧摘要。`verify`报告missing/changed和`ordinary_git/local_retained`可得性；
获取方按manifest路径恢复后再校验，路径逃逸、符号链接和重复条目拒绝。
`bundle`只生成新归档/分发清单，前后核对输入；没有删除、上传或对象存储迁移。

历史12份超限collector继续按[原清单](evidence/optimization-source-delivery-20261025/local-binaries.json)
在原路径本地保留。普通Git检出缺这些字节，须从保留方或完整历史包取得；
`--include-local-binaries`可核对它们，未获取时不能声称完整第六阶段重放。
远端对象存储地址、访问权限及退役日期尚未配置，不编造分发/保留承诺。

`.gitattributes`继续禁止历史字节换行转换；摘要/检查清单及参考Rust/Python/TOML源码
可审阅文本diff，大型原始材料仍按摘要审核。Git历史和冻结报告不裁剪。
