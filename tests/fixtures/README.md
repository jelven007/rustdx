# 测试夹具与来源

本目录只保存可随源码分发的固定回归材料，运行下载包、全量CSV和本次任务清单不放入这里。

| 目录 | 内容 | 使用与证据边界 |
| --- | --- | --- |
| tdx/ | 原assets中的sz000001.day和gbbq二进制 | 本地解析/收益回归；保留原字节，不能作为当前证券事实或每日生产gbbq |
| kline/ | 股票日线、指数日线、股票5分钟各3条请求/响应/期望CSV | [manifest](kline/manifest.json)记录固定pytdx参考及摘要；9条样本不代表全市场 |
| protocol/ | 两证券快照、财务及当日/历史逐笔请求/响应/期望CSV | [manifest](protocol/manifest.json)记录来源/时点及全部摘要；仅所列记录 |
| rustdx-cmd/tests/fixtures/（仓库根） | CLI东财CSV与原始正文等材料 | [CLI夹具](../../rustdx-cmd/tests/fixtures/)就近用于CLI回归 |

独立参考期望CSV进入版本控制，`.gitignore`只对这两组`*.expected.csv`开例外；不要通过全局放开CSV把运行数据入库。protocol四份CSV已按既有manifest摘要核对后补入，解决干净检出缺少预期结果的问题。

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --test file-day --test fq --test properties --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --test kline_decoders --test protocol_decoders --locked
```

重新联网采样使用新目录并保存采集时间、服务器、参数和请求/正文/期望SHA；固定pytdx源码执行前先核对摘要。重解工具见[脚本说明](../../scripts/README.md)，语义与覆盖见[协议接口](../../docs/architecture/PROTOCOL_REQUESTS.md)。参考日历在[config/calendars](../../config/calendars/)，验收结果JSON在[docs/validation/evidence](../../docs/validation/evidence/)。
