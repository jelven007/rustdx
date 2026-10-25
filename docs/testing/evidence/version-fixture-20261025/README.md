# 主线版本递增后的测试夹具回归

采集日期：2026-10-25。源提交为`fd20eecad6420ee061eaca2e031a3a8525ce5859`；
项目实际版本为2.0.1。本目录为新增记录，既有冻结证据保持原字节。

远端原始失败日志来自[Main version 40164217231](https://github.com/jelven007/rustdx/actions/runs/40164217231)，
job 121081887288（Quality / Behavior / Consumers / Packages）。
通过`gh api --allow-escape-sequences repos/jelven007/rustdx/actions/jobs/121081887288/logs`
取得，保留原时间戳和终端控制字节。Python35项中3项断言失败，退出码1；
当时的2.0.1版本事件记录已成功创建，质量失败不授予版本标签。

本机环境为macOS、Python3.9，仅使用临时Git仓库。新增回归命令：

```sh
python3 -m unittest discover -s scripts/validation/tests -p test_versioning.py -k fixture_bootstrap -v
python3 -m unittest discover -s scripts/validation/tests -v
```

修复前连续3次返回1，每次在模拟源2.0.1和7.8.9下各失败一次，无测试错误。
修复后新增回归通过；36项Python检查全部OK。反向证明从上述源提交提取原始
`fixture`函数，在测试进程内替换修复函数，同一新回归再次出现两个预期失败；
没有修改工作目录文件，恢复进程后新增回归再次通过。

| 文件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| [main-contracts.log](main-contracts.log) | 77029 | 755118596452c5a9988b245236321fe796c45f3bd359e78e5917d496434f6e7f |
| [regression-red-1.log](regression-red-1.log) | 1430 | 52333489604a80fd3fc2a6961f9ad838e23794dba79a962d2a950b74f8034e83 |
| [regression-red-2.log](regression-red-2.log) | 1430 | 9367f6b8c6bb65d4b75508c07d6925869054ebc9e6ccd9b8cc17ed5b77a8d9df |
| [regression-red-3.log](regression-red-3.log) | 1430 | 929e529d622e5d4dc31342bc1af1d4aa5e19fdc70d7ddcbbb62be42fd7ff2220 |
| [reverse-proof.log](reverse-proof.log) | 1430 | 4a9135b0a1beee02f6e747ad039ff632afe8f5697c5b296e65f69f8b6dd21a40 |
| [suite-green.log](suite-green.log) | 3735 | 81d202ceccdee837b6ca4b6ebfd956336bf47d854bc619135d1c7afc6aab13ba |

根因、修复与模式检查见[夹具修复记录](../../../archive/development/VERSION_FIXTURE_20261025.md)。
本机结果不替代修复PR或后续主线CI的实际结果；版本和history标签须另行核实。
