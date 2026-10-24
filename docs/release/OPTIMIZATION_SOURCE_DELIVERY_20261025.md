# 17项优化源码提交范围

本次提交包含17项工程优化、其依赖的工作区目录迁移、2.0版本、测试、配置、CI、当前文档及固定验证资料。之前本地`main`中14个尚未推送提交是本次源码的祖先，一并通过快进推送交付；不重写历史。

最终代码检查与限定范围见[收尾报告](../archive/development/OPTIMIZATION_FINAL_20261025.md)，原始结果见[支持包](../testing/evidence/optimization-final-20261025)。提交准备阶段只增加Git忽略/属性规则和交付记录；生产Rust代码保持最终检查的字节。

历史第六阶段演练包含12份`collector.bin`，每份138,315,760字节，超过GitHub普通Git的100MiB单文件限制。原文件仍在本机原路径，字节及原历史摘要不变；本次Git提交以[排除文件清单](../testing/evidence/optimization-source-delivery-20261025/local-binaries.json)记录路径、大小、SHA-256和本地保留状态。原始演练的完整重放仍需这些历史二进制，普通Git检出不具备其字节；按该清单从保留的本地材料或完整预检归档取得，再核对摘要。源码、就近测试夹具、独立期望、有限日志和实际审核crate随提交保留。

`.gitignore`仅排除该历史目录内的`collector.bin`。对已在固定证据清单中登记、被通用CSV/日志规则忽略的材料，以明确文件清单入库，避免迁移后漏交验证输入。旧冻结报告不改写为最新提交身份；最终代码摘要与本次交付摘要分别记录。

`.gitattributes`禁止冻结证据和固定夹具的换行转换；证据按摘要审阅，不参与源码行差异或空白检查。源码/许可证空白检查的三处提示均来自原已跟踪内容，保留原字节。最终检查清单中的两份`.DS_Store`是Finder元数据，继续按既有规则忽略，不属于生产源码；具体范围见[提交准备核验](../testing/evidence/optimization-source-delivery-20261025/source-verification.json)。

提交身份延续本仓库之前的作者`张路 <jelven@126.com>`和提交者`trae-solo-agent <solo-agent@trae.ai>`，使用单次Git参数，不更改全局用户配置。远程为`https://github.com/jelven007/rustdx.git`，目标分支为`main`。远端CI结果须在推送后实际查询，既有本机测试不替代Actions结果。
