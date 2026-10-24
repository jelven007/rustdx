# 九命令真实协议夹具

11组精选帧覆盖0452、051b的两种selector、0fd1、0feb、056a、0ffc、054c、054b的两种方向和0547。采样来源及每个文件SHA256见[manifest.json](manifest.json)，完整两节点132组证据见[证据目录](../../../../../docs/testing/evidence/nine-commands-20261024)。

每组保存request/header/wire/payload与expected JSON。期望来自已被[独立标准库脚本](../../../../../scripts/validation/verify_nine_commands.py)逐字段核对的真实DTO，Rust回归复核全部owned记录。传输/错误测试使用的人工报文放在tests/common，明确标为synthetic，不混入本目录。

0feb夹具是实测空响应，仅证明传输、日期selector和空头布局；非空解码的极值/均价合同由合成边界测试覆盖，未计为非空实盘验收。0547夹具为刷新首帧，不代表已取得交易时段主动推送。竞价请求日期没有响应回显，未证明历史覆盖。
