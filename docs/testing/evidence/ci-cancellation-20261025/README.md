# macOS 取消测试的调度竞态

采集日期2026-10-25；修复基线为`bd0dd006ad24d440a983b9ed88c481a65291c256`，
功能分支版本继承2.0.1。新增证据独立保存，历史日志保持原字节。

`pr3-macos-original.log`来自[PR #3 首次 CI](https://github.com/jelven007/rustdx/actions/runs/40164904981)，
macOS job `121083851469`，由
`gh api repos/jelven007/rustdx/actions/jobs/121083851469/logs --allow-escape-sequences`
下载，保留时间戳和终端控制字节。原始文件158057字节，SHA-256为
`984758c9973a786f7e1aa206ffc8b1c1d0c4fb16658b618fcd9ce1111de926fd`。
`rustdx-runner --lib`中11项通过、1项失败，错误为期望`cancelled`、实际`timeout`，
Cargo退出101；其他目标和doctest结果按原日志解释。

本机使用macOS、Rust1.89.0、同步shell及临时目录。回归命令：

```sh
cargo +1.89.0 test -p rustdx-runner --lib --all-features --locked --offline \
  database_process::tests::cancellation_and_deadline -- --nocapture
```

原测试只将取消线程等待从20毫秒受控延迟到200毫秒，连续三次得到同一错误，
每次1项失败、退出101。修复测试覆盖独立100毫秒截止、确认管道开始写入后的立即取消、
200毫秒延迟取消；首次修复回归1项通过、退出0。反向证明只将修复测试的取消预算
从10秒恢复为100毫秒，200毫秒延迟用例再次得到`timeout`而失败，退出101；
随后恢复10秒预算。

原始命令、退出码、耗时和日志SHA-256逐项记录在`red-checks.json`、
`green-checks.json`、`reverse-checks.json`、`backup-checks.json`及两次
`verify-checks.json`、`verify2-checks.json`。
固定延迟用于注入被验证的调度条件；管道就绪使用有上限的条件轮询确认。
临时文件、shell和进程组由测试创建并回收。运行环境配置由本机明确注入，
未记录凭据或环境变量全集。

首次全包检查在备份用例遇到锁冲突并退出101，保留`runner-suite.log`及首个检查JSON。
该用例独立三次通过，完整并行命令复验35通过、9忽略、0失败；
最终8项检查均退出0，结果见`verify2-checks.json`和`*-second.log`。
文件锁冲突根因未确认，备份代码未修改。新增报告后的导航复查另记在审查回执。
远端须重新核验新的PR head及主线生成版本。根因与边界见
[取消测试修复记录](../../../archive/development/CI_CANCELLATION_20261025.md)。
