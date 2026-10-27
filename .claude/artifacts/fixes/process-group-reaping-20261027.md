# Bug: 取消后的进程组存活断言偶发失败

> Status: FIXED (本机macOS；Linux现场由CI继续验证)<br>
> Mode: --deep<br>
> Severity: functional / blocks complete runner verification<br>
> Last updated: 2026-10-27

## Symptom and expected

`cargo +1.89.0 test -p rustdx-runner --offline --locked -j2`在
`database_process::tests::cancellation_and_deadline_kill_blocked_pipe_and_descendants`
的`!process_group_alive(pid)`断言失败。本轮日志为
`/private/tmp/rustdx-comprehensive-optimization-20261027-artifacts/phase1-runner-tests.log`。
取消/超时应终止自有组、释放pipe线程且不留下可运行后代；未知状态仍阻断恢复和发布。
环境为本机macOS、Rust1.89.0、同步pipe和专属Unix进程组。

## Hypotheses

这些概率仅为诊断优先顺序的主观估计，不是测量结果。

| 假设 | 预测 | 优先级 / 先验 / 信心 | 状态 |
| --- | --- | --- | --- |
| H1：ready在后代创建前写入，首轮组信号未覆盖所有后代 | 失败组仍有非Z活动后代；改为fork完成后ready可避免该时序 | 1 / 35% / 高 | 确认：原时序13/40失败，全部残留S状态且PPID=1；补终止后0/100，反转后4/8 |
| H2：组只剩退出但尚未被系统reap的僵尸，signal0误作可运行 | 失败组全部Z，没有可运行成员，pipe join已返回 | 2 / 45% / 低 | 排除：所有捕获失败均为S，而非Z |
| H3：组身份异常或PID复用造成错误存活观测 | PGID/PPID或创建身份不符 | 3 / 20% / 低 | 失败成员仍属于被监督的原PGID，撤回/恢复一行补终止即可重现/消除；未直接跟踪内核fork及kill errno |

## Reproduction and evidence so far

- 原实现在全package回归中失败；此前候选也记录过同一断言偶发失败。
- 已加入仅测试采证：专属PGID的PID/PPID/PGID/stat、ready内容、取消模式和耗时。
- 更严格的“后代创建完成后ready”测试，在三轮完整lib和100轮4路并发精确测试均通过。
  这不解释原始失败，也不代表回收已修复。
- 第一组3轮diagnostic因测试中PathBuf移动导致编译失败，不计入运行压力或复现稳定性；
  修正克隆后才取得上述真实运行结果，原日志保留。
- 恢复原ready-before-fork脚本后，40轮4路完整lib负载中13轮失败，前3轮连续失败；
  所有失败成员均为S状态、PPID=1、原PGID。例：leader83180退出后，PID83244/PPID1/PGID83180仍为S。
- 没有改弱断言、将僵尸视为不可运行或自动重试失败样本。每组日志和全部退出码分别保留。

## Root cause and fix

`Worker::drop`只在reap leader之前发一次SIGKILL，之后仅等待组自行消失。
实测leader退出后仍有可运行成员；后代创建与首轮组信号交错时，一次组信号不保证收拢全组。
因此在现有200ms清理循环中继续对自有PGID发SIGKILL，直到组消失或预算结束。
不增加等待期限、不改变signal0存在性语义、不放行Unknown、不弱化恢复/备份准入。
只改`supervisor.rs`的该循环一行行为及解释注释；原回归保留，Unix CI新增100次显式取消回归。

## Verification

| 阶段 | 迭代 / 并发 | 失败 | 记录 |
| --- | --- | --- | --- |
| 原实现RED | 40 / 4 | 13 | B05-original-results.json |
| 修复GREEN（诊断保留） | 100 / 4 | 0 | B05-green-results.json |
| 临时反转RED | 8 / 4 | 4 | B05-reverse-results.json |
| 恢复修复、移除诊断 | 100 / 4 | 0 | B05-final-results.json |

上述均为本机完整lib工作负载，无自动重试替换失败。最终binary SHA为
`95cca0992f0d5ab6535802e926c1b3ab39e3bb89182ea1519de94039b432a76a`。
最终runner全部可执行测试36通过/9显式ignored；Clippy/all-targets/all-features退出0，
diff检查通过。生产源码和脚本中临时诊断标记搜索无匹配。
日志根目录均为`/private/tmp/rustdx-comprehensive-optimization-20261027-artifacts`，
最终支持包另保全。Linux及目标现场尚未执行，不由macOS样本推导。

## Pattern analysis and RCA

`process_group_alive`用于Worker::drop、collect取消、pipeline恢复和ops备份准入；
它们继续严格拒绝未收拢的组。数据库Process复用Worker，同步pipe在组终止后join。
单次kill与200ms等待实现源于`47045cd5`（2026-10-25）；未执行git bisect来断言更早来源。
原单轮/空闲通过容易掩盖并行负载下的fork交错；现加入重复取消回归及原始失败状态证据。
未直接跟踪内核调度；确认范围为“一次组终止后有活动遗留，而补终止在相同预算内收拢”。
