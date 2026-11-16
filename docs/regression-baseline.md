# YueShot 生命周期回归基线

2026-11-09。用途是确定每次修改应运行的回归及证据范围，不是把历史通过自动转成新候选资格。

| 变更触及 | 必须复用的回归 | 证据要求 |
| --- | --- | --- |
| 产品/共享 Rust 代码 | `cargo xtask check` | 记录 commit 与日志；ignored 不算通过，静态 HDR/AVIF 不算实屏资格 |
| 工具与许可清单 | `python -m unittest discover -s tools/tests -p 'test_yueshot_*.py'` 及对应实际工具 | 不只测试 mock；候选需 `verify` 与来源校验 |
| 安装器/资源 | Release 打包，三个 `test-yueshot-{install,installer-policy,channel-install}.ps1` | 新隔离候选；安装负载/登记/快捷方式、锁/回滚、卸载数据保留 |
| 事务/提取路径 | 上述策略回归 + `test-yueshot-low-space.ps1`；下载暂存路径另加 `-UpdateDownload` | 实际 NSIS/WinHTTP、独立 VHD、真实磁盘满错误、失败不改旧文件/数据/登记、释放空间可重试 |
| 主窗口/退出 | `test-yueshot-window-close.py` 及 X/托盘人工回归 | 关闭到托盘保留进程可唤醒；明确退出返回 0；不得只测 Rust 状态 |
| 在线更新 | `cargo test -p yue-shot --features test-support --locked --lib update:: -- --test-threads=1` | 验签负面/中断/哈希/锁，独立父子进程退出/重启；fixture 与真实 NSIS 分开记 |
| 网络实现或发布源 | 显式 HTTPS 资格、真实客户端检查/下载/验签 | 不用单元测试冒充真实断网；区分“检查”与“下载”的故障 |
| 自启/托盘/恢复 | 登录、Explorer 重启、睡眠唤醒后的截图/保存 | 操作者明确报告；记录已知环境，不借用其他轮次元数据 |
| 运行库/工具链/最低系统 | 干净普通用户安装、实际已安装 EXE 与图形路径 | 开发机和 hosted runner 不替代无预装运行库机器 |
| 更新信任根 | 原客户端信任关系、备份恢复签署/验签，轮换专项 | 公钥文件存在不等于私钥可恢复；不能自动替换生产密钥 |

## 已建立的人工基线

- [普通用户 Build 19044 的 X/托盘定向复测](acceptance/2026-11-09-windows-19044-close-to-tray-retest.json)。
- [0.1.2 → 0.1.3 Beta 升级、重启、设置/截图保留](acceptance/2026-11-09-beta-online-upgrade.json)。
- [登录自启、Explorer 重启、睡眠恢复](acceptance/2026-11-09-beta-lifecycle-manual.json)。
- [独立介质密钥恢复与原公钥验签](acceptance/beta-key-recovery-20261109/result.json)。

原生安装故障、隔离 VHD 磁盘不足和相关回归的最新本地范围见 [失败验证](upgrade-failure-qualification.md)。

## 结果记录规则

每次记录测试命令、实际候选版本/buildId/安装器哈希、环境、成功/失败与未覆盖项。
只将执行过的断言记为通过；失败的原始报告保留，修复后用新输出目录重测。
人工确认独立存放，不改写自动脚本的 `manualQualification=pending`。

真实断网、断电、注册表写失败、重复恢复循环和资源稳定性仍需独立资格，不能因为上表已有相关测试名称而视作完成。
性能结论必须有前后指标；本轮工程收尾不作没有测量依据的性能承诺。

## 视觉候选设计研究快照（2026-11-16）

源码 HEAD：`7938581`。以下是设计初稿研究时实际执行、在评审修订时重新核对原始日志的计数摘要，**不是视觉算法已实现、P0 基线已补齐或完整升级已通过**。本次评审修订没有重跑代码测试。

| 范围 | 通过 | 失败 | ignored |
| --- | --- | --- | --- |
| capture：window_detection | 113 | 0 | 0 |
| capture：refinement_worker | 6 | 0 | 0 |
| capture：overlay | 31 | 0 | 1 |
| capture：capture_worker | 5 | 0 | 1 |
| capture：providers | 14 | 0 | 2 |
| capture：session / artifact / sampler / geometry | 24 / 8 / 16 / 51 | 0 | 0 |
| yue-media / yue-geometry | 8 / 10 | 0 | 0 |
| 本项目合计 | 286 | 0 | 4 |
| mark-shot 区域模块临时独立 harness（另计） | 5 | 0 | 0 |

命令、参考版本哈希及证据边界见[设计文档附录 A](visual-candidate-pipeline.md#附录-a初稿研究执行记录)。原始日志仍位于临时目录，摘要持久化不等于原始日志已归档；ignored、完整 `cargo xtask check`、新 golden、新算法和正式性能对照未在该轮执行。实施时须将新批次的原始日志、环境、退出码及未覆盖项持久保存到 `docs/acceptance/`，不能借用本摘要作为新候选资格。
