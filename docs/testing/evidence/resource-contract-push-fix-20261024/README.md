# 2026-10-24通用资源与推送门禁证据

当前结论与实现见[报告](../../../archive/development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md)和[机器摘要](summary.json)。

- `live-hz/live-sh`：二进制摘要记录后重新取得的最终真实资源样本，分别57帧；完整请求、帧头、压缩/解压正文、文件、全部CFG原字节和DTO。
- `closed-gate`：同一固定推送工具的默认休市门禁；退出码2，网络帧和被动请求为0，不能证明盘中主动推送。
- `binary-before-capture.json/environment.json`：实际采样前后固定二进制、平台和工具身份；测试构建完成后不再构建。
- `source-input/source-manifest.json`：当前未提交源码及测试输入858项，版本号不能代替源码身份。
- `reference`：固定eltdx3.3.0资源协议/统计槽位源码和原始tar来源摘要。
- `checks/negative-verification.json`：执行命令、退出码和本地故障/证据副本负向检查。外部服务集成测试环境跳过，不计为生产数据库验收。
- `attempt1-* / initial-environment.json`：初次样本原字节保留。随后发现workspace测试构建重写了示例二进制，初次运行前没有固定其二进制摘要，因此不计入最终身份固定采样统计；原有文件SHA清单不改写。
- `previous-evidence-integrity.json`：旧板块1,035文件与旧推送408文件按原摘要核验一致。

核验最终节点：

```sh
python3 scripts/validation/verify_resource_contract.py \
  docs/testing/evidence/resource-contract-push-fix-20261024/live-hz
python3 scripts/validation/verify_quote_push.py \
  docs/testing/evidence/resource-contract-push-fix-20261024/closed-gate
```

脚本成功只表示留证结构一致。统计命名槽/单位为候选、资源末尾不带声明hash或原子性、推送休市门禁不等于实盘验收。完整文件集合及摘要见`manifest.json`。
