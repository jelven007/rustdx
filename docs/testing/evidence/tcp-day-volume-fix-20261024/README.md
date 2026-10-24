# TCP日线整数股数修复支持包

本包记录052b新采集路径、金额解码修复及两个实际节点的字段核对。范围为sh600000、sh600519、sz000001的2026-10-19至22日线。实现及检查结论见[修复报告](../../../archive/development/TCP_DAILY_INTEGER_SHARES_FIX_20261024.md)。

- `probe-*`：修复前的052d category9/category4及052b诊断原帧；其052b JSON仍保留当时的错误金额算法结果，不改写历史。
- `precision-analysis.json`：直接读诊断原始正文，确认category9差−94至+1股、category4有16/24条量化差、052b整数股数24/24精确。
- `live-*`、`independent.json`：首轮新CLI采集及24条原帧/收据/CSV/官网day精确核对。
- `final-live-*`、`final-independent.json`：完成工程检查后的固定二进制复采及24条核对。其CSV与首轮相同，每证券原始一页12条、过滤后4条，下界停止。
- `acquisition-binary.json`：最终实际采集二进制摘要、Rust及平台；临时执行路径仅为现场记录。
- `source-basis.json`及各候选`facts`：新来源合同、整数股数/金额编码和样本边界材料；SHA一致不代表全市场权威认证。
- `source-manifest.json`：本轮当前Rust源码、Cargo清单/锁及核验脚本的逐文件摘要，未提交工作区不使用Git HEAD替代源码身份。
- `checks.json`及`checks/`：实际命令、退出码、统计和日志；工作区使用`RUSTDX_SKIP_INTEGRATION_TESTS=1`，数据库/联网守卫和ignored项明确保留，两个runner TCP专项另显式执行。
- `legacy-preservation.json`：旧052d归档与既有摘要的只读核对；旧差异及原历史期望继续保留。
- `evidence-manifest.json`、`verification.json`：本包逐文件摘要与封存检查。库存不包含自身和最终验证标记，避免循环摘要。

六组原始响应及字节相同的四条官网记录放在[所属crate夹具](../../../../crates/rustdx-core/tests/fixtures/daily-shares/manifest.json)。官网参考来自[第五阶段已留存day文件](../acceptance-stage5-20261024)，各文件摘要保存在独立结果与夹具manifest中；原URL、下载时点和ZIP摘要继续由第五阶段来源材料解释。本轮没有重新下载官网包。

从仓库根只读复验，输出选择新文件：

```sh
python3 scripts/validation/verify_tcp_daily_shares.py \
  --official-root docs/testing/evidence/acceptance-stage5-20261024 \
  --output /tmp/rustdx-daily-shares-review.json \
  docs/testing/evidence/tcp-day-volume-fix-20261024/final-live-115.238.56.198 \
  docs/testing/evidence/tcp-day-volume-fix-20261024/final-live-180.153.18.170
python3 scripts/validation/analyze_tcp_daily_precision.py \
  --evidence-root docs/testing/evidence/tcp-day-volume-fix-20261024 \
  --official-root docs/testing/evidence/acceptance-stage5-20261024 \
  --output /tmp/rustdx-daily-precision-review.json
```

重新联网采集须新建私有候选目录、冻结请求及facts，并以实际采集日填写`acquired_on`；本包请求不用于其他日期。全市场、当天bar、五分钟、市场2、完整交易所历史及数据库生产部署不属于本包结论。
