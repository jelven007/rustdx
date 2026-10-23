# 东财原始响应与离线重放

CLI `east`在解析每页之前，将完整UTF-8 HTTP正文写入`<manifest>.evidence/<run_id>/east-00001.json`等文件。包括JSONP包装、市场f13、源时间戳f124和未识别字段；这是HTTP解压后的正文，不是TLS报文抓包。每页还记录完整请求URL、页号、页大小、max、UTC采集时点、正文SHA-256和解析器版本`eastmoney-clist/2`。

单页正文上限16MiB，一次采集正文合计64MiB，超过上限失败。JSON转义后的磁盘字节数可能更大；每次只持有一页原始响应，已解析的证券集合仍受上游u16 total限制。证据文件自身的SHA-256、字节数和绝对路径保存在schema4清单`raw_pages`中；逐页持久化，即使随后解析或日期校验失败也保留已经接收的证据。原先的规范化数据摘要继续保存在`sources`，两种摘要含义不同。

无网络重放：

```sh
cargo run -p jelven-rustdx-cli --example east_replay -- eastmoney.csv.manifest.json replay.csv
# 使用原采集的外部日历时，显式传入同一文件
cargo run -p jelven-rustdx-cli --example east_replay -- run.json replay.csv calendar.json
```

重放依次检查证据文件摘要、正文摘要、解析器版本、URL/请求参数和页序，然后经过相同解析、规范化及标准日线校验，原子输出CSV。不进行“今天”新鲜度检查，以便重放历史证据。回放保留源收盘、量额和前收；未重新提供前因子，factor为0。因此未使用previous的成功采集可与回放逐字节比较；使用previous的累计收益需要另行重放已摘要留存的前因子输入。源无收盘的数据与日常CLI一样不写入CSV。

库入口`eastmoney::replay`接收逐页迭代器；不必把全部响应载入内存。恶意摘要、参数、版本、缺页及多页在离线回归中拒绝，CSV字段和手单位维持兼容。摘要用于完整性检测，不证明第三方数据真实或为证据签名。归档时需一并保存manifest、原始页及实际日历；当前绝对路径在搬迁后需按归档位置重新定位。
