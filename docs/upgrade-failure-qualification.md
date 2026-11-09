# 安装与升级失败验证

2026-11-09。范围为 Windows x64、默认 SDR，不新增截图功能。
本轮在管理员开发机 Build `10.0.26100.0` 上使用 dev 渠道，未覆盖已有 Beta 安装。

## 实际磁盘不足暴露的问题

安装器先在目标卷暂存全部负载，再备份/发布旧文件。旧实现仅在 `File` 后写 `IfErrors`。
NSIS 3.09 的 `File` 在提取写入失败时可以直接返回 `EXEC_ERROR`，不继续执行后面的错误分支；
官方 [源码](https://github.com/kichik/nsis/blob/v309/Source/exehead/exec.c) 与
[失败回调文档](https://nsis.sourceforge.io/Docs/Chapter4.html#oninstfailed) 说明了这一执行路径。

独立 128 MiB VHD 实际写满后，旧 dev 安装器返回 2，留下包含 49,152 字节不完整 EXE 的事务目录。
旧已安装文件的 SHA-256 未变，释放空间后旧 EXE 的 smoke 通过；但残留事务阻止直接重试，因此原失败清理合同没有兑现。
保留原始失败、镜像和提取残留后，已正常卸载此隔离 dev 安装。
公开 0.1.3 Beta 对应源码中的安装模板与修复前 HEAD 完全相同，亦未包含该失败回调；不能视作公开包已修复。

修复将显式失败和 NSIS 失败回调接入同一事务回滚/清理逻辑。
只有独占创建事务成功后，当前调用才拥有清理权；已有中断事务继续原样保留。
恢复或清理失败返回 15 并保留现场，不从回调再次执行部分回滚。

| 行为 | 修改前实测 | 修改后实测 |
| --- | --- | --- |
| 目标卷空间不足，提取写入失败 | 返回 2，跳过自有清理 | 返回 14，清理本次事务 |
| 失败后的旧文件/数据 | 文件哈希未变、旧 EXE 可运行 | 文件/数据哈希与安装登记快照完全相同、旧 EXE 可运行 |
| 释放空间后重试 | 残留事务需保留并处理 | 同一安装包正常重试，配置保持一致 |
| 已有事务或锁冲突 | 旧策略已检测并保护 | 全部原生策略回归通过，未扩大清理范围 |

## 新候选与实际命令

候选：`target/yueshot-delivery/0.1.3-dev-low-space-fixed-final`，buildId
`7c2ed93f7016babfb0ac74b26be5c9806bbc39100dcd7a88c32a815539693369`。
从 HEAD `0fbc0fc` 加本轮未提交改动构建，`dirty=true`，仅作本地资格候选。
安装器 SHA-256：`bc1118a2389bc3225e636e8332630b77e5e407bd7563bc053e1180a72ca2de23`。
Release 构建、对应源码输入一致性、负载/运行库/图标/许可及安全审计通过；安全 0 blocking、0 maintenance，15 条图外记录保留。

```powershell
./tools/test-yueshot-low-space.ps1 -Candidate target/yueshot-delivery/0.1.3-dev-low-space-fixed-final -Output target/low-space/20261109-nsis-fixed
./tools/test-yueshot-install.ps1 -Candidate target/yueshot-delivery/0.1.3-dev-low-space-fixed-final
./tools/test-yueshot-installer-policy.ps1 -Candidate target/yueshot-delivery/0.1.3-dev-low-space-fixed-final
./tools/test-yueshot-channel-install.ps1 -Candidate target/yueshot-delivery/0.1.3-dev-low-space-fixed-final
python tools/test-yueshot-window-close.py --binary target/yueshot-delivery/0.1.3-dev-low-space-fixed-final/bin/YueShot.exe --output target/low-space-window-close
```

上述路径均已执行并通过，不能直接复用输出目录重跑。三个安装脚本及磁盘不足脚本均使用 Windows PowerShell 5.1 执行。
VHD 测试经过真实写入返回 Win32 112，再释放 256 KiB 供目录元数据和部分提取，提取前实际空闲 307,200 字节。
验证失败不改变文件/用户数据/登记、事务清理、旧 EXE 启动、释放空间后的重试及卸载保留数据。
镜像通过绝对路径反查磁盘对象并核对非 boot/system、128 MiB、RAW、文件虚拟总线后才初始化；测试结束已卸载镜像。
dev-only 保护另行实测：传入公开 Beta 候选被拒绝，未创建输出目录或 VHD。

日志分别为 `target/low-space-fixed-20261109.log`、`target/low-space-fixed-{runtime,policy,channel}.log`、
`target/low-space-window-close.log`。失败基线在 `target/low-space/20261109-nsis-native/`，
修复后镜像和结果在 `target/low-space/20261109-nsis-fixed/`；公开摘要见 [结构化证据](acceptance/2026-11-09-install-failure-recovery.json)。
一次重建因打包期间 README 变化被来源门禁拒绝，原日志 `target/low-space-package.log` 保留；
固定输入后使用上述新候选重新完整打包，通过日志为 `target/low-space-package-final.log`，未跳过来源检查。

## 更新与既有行为回归

`cargo test -p yue-shot --features test-support --locked --lib update:: -- --test-threads=1` 实际 10 passed / 3 ignored，
包括原生辅助进程回归；两个 ignored 是其显式调用的子进程入口，第三个 HTTPS 入口本轮未重跑，不算通过。
网络连接错误、写入中断和损坏负载通过确定性的 fetch 故障注入，证明错误传播和暂存清理；未操作用户网卡或代理，不宣称真实断网资格通过。
51 项 Python 工具测试通过，PowerShell AST 语法通过，原生 WM_CLOSE 两个分支通过。

文件锁回归验证新安装锁、重装 EXE/DLL/卸载器锁、卸载资源锁、目录碰撞、已有事务、降级/损坏登记拒绝，
以及正常安装/重装/卸载、快捷方式和 AUMID。所有路径继续保护数据、未知文件与其他渠道，不强杀用户产品。

本轮磁盘不足针对同版本重装的真实文件部署路径；不是新一轮真实在线跨版本升级或数据库迁移。
没有模拟断电、注册表写失败或下载目录所在卷写满；没有测量性能，不作性能提升结论。
公开 0.1.3 及更新源未改动，修复需后续新版本独立签署/发布后才能到达用户机器。
