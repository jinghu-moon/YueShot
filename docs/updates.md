# YueShot 在线更新

Windows x64，按 dev / beta / stable 分离。当前已经接入客户端与本地进程回归；用户在 2026-11-09 本机生成带密码的 beta 私钥，并提供公钥，beta 信任根已经配置。0.1.3 beta 与签名更新源已获用户确认并公开发布；真实客户端检查、下载与验签通过，用户已确认真实升级到 0.1.3、自动重启及设置/截图保留。安装包仍可不做 Authenticode 签名；这里的 Minisign 签名用于更新来源认证，二者独立。

## 用户流程与所有权

设置页的“软件更新”按需检查，不在启动时联网。更新视图持有检查/下载状态，产品持有退出决策，产品私有 update 模块负责网络、认证、暂存和安装辅助进程；没有新增公共平台 crate。

1. “检查更新”：读取本渠道固定地址的 latest.json 和 latest.json.minisig；先验签，再解析严格 schema，检查产品、渠道、架构、版本、有效期和 GitHub 仓库地址。旧版本清单不会触发降级，稳定渠道不接收预发布版本。
2. “下载更新”：写入独立暂存目录的 installer.partial，限制大小，检查签名清单绑定的精确长度和 SHA-256，完成后才重命名为 installer.exe。中断或字节不匹配会删除本次暂存内容。
3. “退出并安装更新”：检查截图/保存/快捷键写入是否完成，核对当前 EXE 是否为 HKCU 登记的此渠道安装版本。辅助程序从暂存副本运行，携带当前安装位置的 app-local VC runtime，不从 System32 复制 DLL。
4. 辅助进程重新验签，确认来源进程与安装位置，并保持安装包只读、禁止写入/删除的文件句柄。它先报告就绪，再等待主程序明确提交更新、正常退出。主程序在辅助进程就绪后再检查一次截图/保存状态；放弃提交不会在之后普通退出时意外安装。
5. 原进程完整结束后才执行现有安装器 `/S /NS /D=<原目录>`，不使用更新库的直接 exit、不强杀产品。安装器失败沿用已有文件事务；辅助进程检查退出码、新 EXE 哈希及 `--product-info` 身份。成功时重启新程序；失败时只允许在旧 EXE 哈希仍完整的情况下重启旧程序，均保留原数据目录。

结果保存在该次 handoff 的 result.json 和数据目录 updates/last-result.json，下一次设置页显示结果。记录结果失败不应阻止已经完成的安装正常重启。每次准备分配独立 handoff 目录，失败后允许重新尝试。

网络使用系统 WinHTTP 代理和证书验证，不放宽证书检查，不允许 HTTPS 降级为 HTTP。清单最多 32 KiB、签名最多 4 KiB、安装包最多 512 MiB；连接/读取设有超时和总时限。下载目前不实现断点续传和后台定时检查。

## 信任配置与发布位置

`apps/yue-shot/packaging/update-keys.json` 随编译固化，分别列出各渠道允许的 Minisign 公钥。beta 已配置用户提供的公钥 `RWRPrtKhF/f6lRgLLGrdWRDJ7nekzqDkFWKoD8QPOJrsqWoRcMNhR7ZJ`；dev / stable 数组为空，其客户端在联网前明确报告尚未配置。绝不信任网络提供的公钥，也不信任测试签名密钥。用户私钥位于仓库之外，由用户本机生成、加密并保管；助手没有读取私钥或获得密码。

固定清单地址：

```text
https://github.com/jinghu-moon/YueShot/releases/download/yueshot-updater-<channel>/latest.json
https://github.com/jinghu-moon/YueShot/releases/download/yueshot-updater-<channel>/latest.json.minisig
```

安装器来自同一仓库的版本标签，例如 `yueshot-v0.1.3/YueShot_0.1.3_x64-setup.exe`。公钥是本地信任根，签名同时绑定清单内所有字段，不能仅用同站下载的 SHA-256 代替认证。清单有有效期；发布维护者须在到期前重新签署并更新滚动清单。

## 发布者操作

使用官方 Minisign。首次在本地生成带密码保护的私钥，私钥放在仓库和 target 之外，密码只在本机 Minisign 提示中输入，不发送给助手、不写入参数或 CI 日志：

本次已从官方 GitHub 发布 `jedisct1/minisign` 的 0.12 版本下载 Windows 归档，并验证工具报告 `minisign 0.12`。本机工具位于 `target/update-tools/minisign-win64/x86_64/minisign.exe`，归档 SHA-256 为 `37b600344e20c19314b2e82813db2bfdcc408b77b876f7727889dbd46d539479`；该值是本次下载记录，不冒充上游单独发布的签名验证。使用时可将下文 minisign 替换为这个完整路径。

```powershell
minisign -G -p D:\YueShot-Release\beta.pub -s D:\YueShot-Release\beta.key
python tools/yueshot-update.py trust --channel beta --public-key D:\YueShot-Release\beta.pub
```

目录需预先创建。不要加 `-W` 取消密码保护。公钥改动审查后重新构建客户端，已经安装的旧客户端不会动态读取仓库文件。生产私钥由用户创建并保管；独立 USB 备份、恢复和拔插后复读的实际结果见 [恢复演练](update-key-recovery.md)，密钥轮换仍待验证。

验证候选后，生成待签署材料：

```powershell
python tools/yueshot-update.py prepare --candidate target/yueshot-delivery/<候选目录> --tag yueshot-v0.1.3 --output target/yueshot-updates/0.1.3-beta
python tools/yueshot-update.py sign target/yueshot-updates/0.1.3-beta --secret-key D:\YueShot-Release\beta.key
```

prepare 先执行现有候选字节校验，标签必须匹配候选实际版本，绑定该候选安装器和 EXE 的哈希；不覆盖已有输出。sign 调用官方 Minisign，并验证签名匹配客户端信任的渠道公钥。脚本不上传材料、不创建 Release，也不把候选的发行门禁自动改为通过。更新发布还应附带该版本的对应源码与许可材料。

## 验证范围

```powershell
cargo test -p yue-shot --features test-support --locked --lib update:: -- --test-threads=1
cargo test -p yue-shot --features test-support --locked --lib update::tests::https_transport_uses_windows_networking_and_reads_the_public_repository -- --ignored --exact
cargo xtask check
```

签名 fixture 覆盖篡改、未知信任根、产品/渠道/架构错误、过期、降级、重复版本、稳定渠道预发布、非法地址和大小。下载回归覆盖中断、损坏、清理与文件锁；GPUI 测试实际点击按钮并确认失败后可以重试。

原生回归从独立临时目录启动真实父/辅助进程，动态签署测试清单，验证提交前及父进程退出前都不运行安装器、安装后校验与重启、失败保留旧 EXE 和用户数据。它使用专门编译的安装器/负载 fixture，不触碰 HKCU 的真实安装登记，也不冒充真实 NSIS 在线升级验收。两个 ignored 子进程入口由该回归显式执行；网络资格用例需显式运行，ignored 不计作通过。

真实签名清单 → HTTPS 下载 → 已安装旧版 → 新版 NSIS → 重启及设置/截图保留已获用户实测确认，范围见文末；生产 beta 密钥已通过独立 USB 备份、恢复、实际签署/验签及拔插后复读，轮换资格另计。已完成的自动安装/回滚/渠道回归继续保留。

2026-11-09 本地证据：更新定向回归 10 passed / 3 ignored；两个子进程入口在原生回归中显式调用，HTTPS 用例另行实际执行 1 passed / 0 ignored。`cargo xtask check` 已通过，含 51 项 Python、1,096 次 Rust 通过与 107 次 ignored 出现（当时未包含随后新增的注册表读取用例和 HTTPS ignored 入口，新增用例另行定向通过）。安全审计 0 blocking / 0 maintenance notices，15 条产品图外记录保留。

完整 dev 渠道 Release 候选 `target/yueshot-delivery/0.1.2-dev-authenticated-updater-release` 的来源/字节/运行库检查、安装 smoke、文件事务与回滚策略、渠道/快捷方式检查全部通过，未覆盖现有 beta 安装。它构建于配置 beta 公钥之前，不能用作 beta 更新客户端。首次 dev profile 打包被既有运行库门禁拒绝 `d3dcompiler_47.dll` 静态导入，失败记录保留在 target/update-package.log；本次未把该 DLL 加入白名单，正式 Release 配置正常通过。

实际 GUI 使用隔离数据目录验证设置页更新区域、按钮焦点、鼠标及 Enter 操作、缺少公钥时的可重试提示，并通过“退出”正常结束。签名测试数据还使用官方 Minisign 0.12 验证通过，不只依赖 Rust 验证器自身。

带用户 beta 公钥的初版引导候选 `target/yueshot-delivery/0.1.2-beta-update-bootstrap` 曾完成 Release 打包及候选校验，buildId `188b2b4f6464be6d5e52759bac04bfd060cf72cc5074545f03d5ce6df43d8711`；没有安装它或覆盖现有 Beta。随后发现下述 NSIS 路径交接缺陷，此包不再作为验收入口，须使用修复后重建的引导包。dev 渠道 Release 的原生关闭窗口回归两个分支均通过，结果在 `target/update-window-close-native/report.json`。引导版与新版之间的真实升级尚待签署和发行验收。

NSIS 路径回归：官方文档要求 `/D=安装目录` 是最后一项、不能加引号，即使目录含空格。独立真实 NSIS 探针 `target/update-nsis-path-probe/result.log` 确认 `Command::arg` 会让 NSIS 忽略含空格的自定义目录，而 Windows `raw_arg` 按原目录安装。原生辅助进程 fixture 现使用含空格的目录，并检查实际原始命令行末尾；旧实现失败证据为 `target/update-nsis-spaces-before.log`。修复不放宽安装位置登记或负载哈希校验。

修复后的定向回归 `target/update-nsis-spaces-after.log` 为 10 passed / 3 ignored。原生 fixture 验证成功后重启与失败后旧 EXE/数据保留；失败后重启真实旧产品仍包含在后续真实安装器验收中，不以测试 harness EXE 的启动代替。

修复版引导包为 `target/yueshot-delivery/0.1.2-beta-update-bootstrap-fixed`，从干净提交 `1f8b2b8` 构建，buildId `d2c3fd12d0a68d85c8ca5a98e6b46e48892ae245743f6439cb3b0dca9b191960`；Release 打包和候选字节/来源/运行库/许可/安全审计通过。修复后的完整 `cargo xtask check` 亦通过，日志 `target/update-path-fixed-full-check.log`，含 51 项 Python、1,097 次 Rust passed 和 108 次 ignored 出现。没有覆盖现有 beta 安装。0.1.3 仅提升产品版本以准备真实升级目标，禁止新增截图功能的限制继续有效。

## 0.1.3 beta 签署与待审发布（2026-11-09）

升级候选为 `target/yueshot-delivery/0.1.3-beta-authenticated-updater-fixed`，从干净提交 `7f45f34` 构建，buildId `1cfc5cf5db5a822fd7ff43a417d8964af1548493bcff9389e0811db6bef37c31`。Release 打包和完整候选字节校验通过，0.1.3 更新定向回归为 10 passed / 3 ignored。该提交的 [Windows CI 42583080733](https://github.com/jinghu-moon/YueShot/actions/runs/42583080733) 已全部成功，含完整检查与安全审计；普通 push 按设计不执行 delivery，本地 Release 证据另计。

用户在自己的终端完成了密码交互并签署 `target/yueshot-updates/0.1.3-beta/latest.json`。官方 Minisign 0.12 验证签名与 comment 签名通过；以客户端同一 `minisign-verify 0.3.0` 库执行独立探针，接受原始生产清单、拒绝篡改字节，见 `target/update-production-signature-probe/result.log`。此次未读取私钥或获得密码。

| 材料 | 字节数 / SHA-256 |
| --- | --- |
| 安装器 | 7,174,510 bytes；`75e8afb1a118628f6eb33562472956696f348fad0876dc57c69223524bf72fbc` |
| 清单 | `30eb5671f41190758d2a584eb4b953e94e20c23c6dbe3426f2d18e95fb888527` |
| 新 EXE | `755569a140928e026c8bb8794fe16c0f457435d5a49f7ea4409613413b6690dd` |

本机 `target/yueshot-updates/0.1.3-beta/release-plan.md` 与 `publication-plan.json` 列出待审的两个公开预发布：版本 `yueshot-v0.1.3` 固定上述提交，附安装器、对应源码、依赖源码、许可材料、签名清单与校验和；滚动源 `yueshot-updater-beta` 仅附同一份清单与签名。发布备注已准备，未创建标签、Release 或上传资产。公开预发布会让 beta 客户端发现更新，需要用户确认该发布范围。`readyForRelease=false`、生产密钥恢复、分发条款和剩余设备资格继续保留；签署成功不代表真实在线升级已通过。

## 公开 beta 与真实下载验收（2026-11-09）

用户明确回复“可以”授权上述公开 beta 方案。已先上传并核对版本预发布全部 9 个资产的远端大小、SHA-256、固定提交和预发布状态，再公开 [YueShot 0.1.3 Beta](https://github.com/jinghu-moon/YueShot/releases/tag/yueshot-v0.1.3)。通过不带 GitHub 认证的公开 HTTPS 地址回读安装器及签名材料，字节和签名均一致后，才公开 [Beta Update Feed](https://github.com/jinghu-moon/YueShot/releases/tag/yueshot-updater-beta)。两者均为 prerelease，不设为稳定 latest；没有将候选 readyForRelease 改为 true。

产品 WinHTTP/清单/暂存代码的独立网络探针使用内置 beta 公钥和 0.1.2 版本下限，实际访问滚动源，认证清单并下载真实 NSIS 安装包，检查精确大小、哈希，以及 0.1.3 客户端不接受重复升级；结果 `target/update-live-network-probe/result.log`，来源哈希及探针范围在同目录 source-evidence.json。它不执行安装器，不代替安装重启验收。

实际引导版 EXE 使用隔离数据目录 `target/update-published-ui-visible`，通过 computer-use 实际点击“设置 → 检查更新”，显示“发现新版本 0.1.3”；点击“下载更新”，最终显示“版本 0.1.3 已下载并通过验证”和“退出并安装更新”。随后通过产品“退出”结束，窗口和进程已消失，没有点击安装、修改 HKCU 安装登记或覆盖用户现有 Beta。

另一台普通用户电脑的验收包为 `target/yueshot-acceptance/0.1.2-to-0.1.3-beta-online.zip`，ZIP CRC 通过；含修复后的 0.1.2 引导安装器、对应/依赖源码、公开签名材料、公钥、expected.json 和 READ-ME.md，不含私钥。使用该电脑尚未安装 Beta 的普通用户安装引导版，建立测试设置与截图，再执行检查/下载/退出安装，核对自动重启到 0.1.3、自定义目录、设置和原图保留。尚未收到该真实升级结果，不能据发布成功勾选完整更新资格。

## 用户确认在线升级通过（2026-11-09）

在明确询问“自动升级到 0.1.3、自动重启、设置和截图保留”后，用户回复“这三项都没问题”。据此将 beta 0.1.2 → 0.1.3 正常在线升级闭环记为用户实测通过，见 [结构化验收记录](acceptance/2026-11-09-beta-online-upgrade.json)。这不是助手在目标电脑独立执行的结果，没有据此推断该轮 OS build、账户权限、干净运行库或自定义目录保留，也没有将未收到的 result.json/数据哈希写成实际证据。

此前用户在“检查更新”时报告 WinHTTP 12029，随后确认测试通过；本机同期重测公开源的检查、下载和验签通过。目标电脑的网络/代理根因未建立，期间没有修改产品网络代码，不能记为已经修复某个网络实现缺陷。

后续 [更新密钥备份与恢复演练](update-key-recovery.md) 已通过独立 USB 备份、恢复及恢复副本实际签署/原公钥验签，公开证据随该文记录。正常升级通过不自动覆盖断网/磁盘不足/断电等失败资格、密钥轮换或正式稳定发行门禁；旧候选字节和公开签名材料保持原样。

本地新 dev 候选补测实际磁盘不足，发现并修复 NSIS 提取失败绕过事务清理的问题；旧程序/数据/登记保留及释放空间后重试通过，
文件锁/回滚、渠道安装与原生关闭回归亦通过。具体范围见 [失败验证](upgrade-failure-qualification.md)。
这不是公开 0.1.3 包的追溯修复；此处记录的是当时安装卷资格。随后实际下载卷写满及释放空间重试也已通过，见 [失败验证](upgrade-failure-qualification.md)。
真实断网、断电或注册表写失败仍未完成；新版本须按 [发布手册](release-runbook.md) 独立交付。

## 0.1.4 Beta 已签署待发布（2026-11-09）

修复版候选 `target/yueshot-delivery/0.1.4-beta-lifecycle-sqlite-fixed` 来自干净提交 `25ab0c5`，
包括安装提取失败清理修复和 [SQLite 上游安全更新](sqlite-security-update.md)，没有新增截图功能。
Release 打包和最终候选字节校验通过；用户本地签署后，官方 Minisign 以及客户端同一 `minisign-verify 0.3.0` 验签通过。
进一步用未改动的生产认证与 offer 判断逻辑执行本地探针，接受 beta 0.1.3 → 0.1.4，
拒绝篡改、错误渠道、过期和重复版本；签名清单绑定的实际安装器大小/哈希及 EXE 哈希均与候选一致。

清单 SHA-256 为 `51df2f0314c5480753fa7a0c2d1660917581800c1efeacd329dd5c75c69e28c4`，
安装器为 `b63e2943b58ed2c6d3a781835936dcd70d9bedea084450ff4e79f2b2e10f161f`。
`target/yueshot-updates/0.1.4-beta/` 已准备发布资产（含签名及最终校验和）、发布备注和完整计划。
按维护者确认，项目源码及默认发行依赖已改为单个 `YueShot_0.1.4_source.zip`，版本发布附件由 10 个减为 9 个；
原候选两个内部源码包保留为证据，安装器和已签署清单不变。合并工具从原候选取材并核对 buildId、逐文件与锁定依赖哈希，
不会将后来改动的工作区代码当作该安装器对应源码。
合并工具的 57 项 Python 回归通过，实际验收 ZIP 的 CRC 和单源码附件一致性通过；从该源码包解压后的 Release
重编译与配置/存储 smoke 通过，构建后原始源码哈希不变。该次使用本机依赖缓存，不代表干净机或完整离线资格。
范围与哈希见 [单源码包验收](acceptance/2026-11-09-single-source-bundle.json)。
目前没有创建 0.1.4 标签/Release、上传文件或替换公开 Beta 源；需要明确授权后才公开该版本并最后更新滚动源。
`readyForRelease=false` 保留，真实已安装 0.1.3 → 0.1.4 升级另行验收。
