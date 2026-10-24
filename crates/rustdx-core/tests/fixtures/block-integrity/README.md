# 完整板块文件夹具

`block_zs.dat`（329,507字节）及`block_zs.metadata.bin`（38字节）为2026-10-24从115.238.56.198:7709取得的原字节复制，没有修改布局或字段。提取时的原始请求、压缩帧及逐块来源保存在[首次采样](../../../../../docs/testing/evidence/block-integrity-fix-20261024/initial/live-a)，02c5为frame-028，06b9为frame-029至039；另一节点和最终源码重采内容相同。

独立Python使用struct、zlib、hashlib和严格GBK解码完整原帧，结果见[独立核对](../../../../../docs/testing/evidence/block-integrity-fix-20261024/independent.json)。文件为117个2813字节槽位，11,554条成分；MD5为`deddfa997800581cf5cff4ea5cf200ca`，SHA-256为`fcc3befb6299f06af053f39bfc44a4ffe60d2112ba66c4161c571a5299b71151`；元数据SHA-256为`67395d913a12ae94f3c5ef20d919e4517ed1cb77abc9d7b72407e5261ad86737`。

测试中成分JSON的MD5 `235661e9cc49825c68de63e8bbfd7463`由独立Python生成：按槽位/成员顺序，每条键依次为blockname、block_type、code，UTF-8、ensure_ascii=False、无空白compact JSON。该摘要验证全部成分字段的回归一致性，不表达供应商类别或第7字节的业务权威性。
