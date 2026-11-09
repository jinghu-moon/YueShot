# 生命周期与交付状态

2026-11-09。开发版本 0.1.3，Windows x64，默认 SDR。产品源码已从 SnapClip 工作区独立，当前阶段允许完善生命周期机制，禁止新增截图功能。

## 已有验证

- 独立副本完整 cargo xtask check 通过，包含产品与公共库回归、格式、严格 Clippy、workspace、AVIF/HDR 静态检查、许可、SVG 和依赖边界。首次导入本地检查为 1,087 次 Rust 通过（含显式筛选重测）、105 次 ignored 出现、43 项 Python 通过；40e98b7 干净远程检查为同样 1,087 次 Rust 通过及 45 项 Python 通过，ignored 不算通过。
- 默认产品 normal/build 图 404 个包与拆分前版本、来源、许可、features 一致，所有本地依赖位于仓库内。原工作区仍验证 SnapClip 消费者，共享能力不依赖产品私有 crate。
- 本机实际安装目录的 0.1.1→0.1.2 升级保留自定义设置、真实历史记录和原始 PNG；降级拒绝、未来数据库版本拒写、恢复 A 版备份、卸载保留数据和重装读回通过。两版本 schema 相同，不声称验证了结构迁移。
- 当前用户渠道身份、安装登记、快捷方式目标/参数/AUMID、运行中拒绝安装/卸载且不强杀、损坏版本登记拒绝、同版本重装均有自动门禁。
- 字体/图标、对应源码、许可原文、VC runtime 静态导入闭包、构建标识与符号材料纳入候选流程。完整权利审查见 [分发权利](distribution-rights.md)。
- beta 0.1.2 → 0.1.3 真实在线升级、自动重启、设置和截图保留已获用户明确确认；[验收证据](acceptance/2026-11-09-beta-online-upgrade.json) 区分用户报告与代理独立执行，并保留此前未确定根因的 12029 记录。正常升级通过不代表失败恢复或密钥恢复通过。
- 用户按三项人工步骤确认登录自启、Explorer 重启后的托盘恢复、睡眠唤醒后的截图及保存均正常；见 [生命周期人工验收](acceptance/2026-11-09-beta-lifecycle-manual.json)。本次未提供环境元数据或重复循环/资源指标，不扩展为所有设备与稳定性资格。

## 仍待完成

首次公开 CI 已发现并修复干净环境的问题：依赖须在离线审计前按锁文件下载；Windows 长短路径统一 canonical 根；path-patched codec 的原始发行归档纳入 checksum 控制；程序解析的 cargo tree 强制关闭终端颜色。原始失败均保留，不通过更新清单或跳过检查掩盖。审计工具缓存只保存版本复验的可执行文件，不缓存检查结论。

干净 runner 还暴露出 GDI 内存字体不属于 DirectWrite 系统字体集合的问题。原生文字改为使用显式的两份嵌入字体集合，factory/loader 与静态字体具有进程生命周期，加载错误传播；GDI 控件保留私有注册。完整生产字符串通过 cmap 和 DirectWrite face 双重缺字检查，排除系统已安装完整字体掩盖错误。字体文件、字重、字号和排版规格未改；最低系统对所需 DirectWrite 接口的资格仍须独立确认。

认证更新客户端与优雅退出/重启已接入并完成隔离进程回归，beta 公钥已配置，0.1.3 清单已由用户本机签署并通过双验证；0.1.3 beta 及签名更新源已授权公开，实际客户端检查/下载/验签通过，用户已确认真实升级到 0.1.3、自动重启、设置及截图保留。beta 密钥独立 USB 备份、恢复副本签署、原公钥验签及拔插后复读已通过；真实登录自启、Explorer 重启托盘恢复和睡眠唤醒亦获用户确认。本地 dev 候选的实际磁盘不足故障清理/重试已通过，公开安装器尚未交付此修复。仍待密钥轮换/丢失补救、完整分发权利与最低系统/动态加载资格、普通用户剩余设备资格、多屏/DPI、重复睡眠循环及原生崩溃/资源稳定性、真实断网/中途断电/注册表写失败等故障资格。本机管理员验证不能替代普通用户验收，各轮证据边界见后文。

本机 Windows Sandbox 未启用，未更改系统功能或重启系统。ARM64 缺少设备，单独排期，不阻塞 x64 流程。

## 已通过的公开 CI

提交 40e98b7 的 [Windows CI 42488706977](https://github.com/jinghu-moon/YueShot/actions/runs/42488706977) 于 2026-11-08 完整成功：干净 checkout → 完整 check（含 AVIF/HDR 严格静态检查）→ 产品安全审计 → Release beta 打包 → 三项实际安装验证 → artifact 上传 → 下载 → 整包严格校验。1,087 次 Rust 通过、105 ignored 不算通过、45 项 Python 通过；安全报告 0 blocking、0 review，15 条产品图外记录保留。安装验证覆盖已安装 EXE、负载哈希、数据保留、运行中保护、渠道登记、快捷方式和 AUMID。

21a59fa 曾在下载复验时缺少 3 个 NSIS 隐藏文件；40e98b7 明确保留该候选范围的隐藏文件并增加回读门禁，不删除清单项来掩盖问题。新 artifact 上传/下载的整体 SHA-256 一致，23 项清单文件全部校验通过。本地另读取清单、诊断和这 3 个文件，哈希相符；构建标识与本地重算的 7,553 文件源码清单一致。本地补充检查没有重复下载整份新候选，整包回读由 CI 完整执行。

候选为 [artifact 13621258927](https://github.com/jinghu-moon/YueShot/actions/runs/42488706977/artifacts/13621258927)，保留 14 天；未签名、readyForRelease=false，未创建正式 Release。源码 buildId 为 88f92499f3fd4e3b42e92261c8cba1f3b2fa6414b54a85e02b4edfe204ef99a2；归档 SHA-256 为 0546cabf61cff0147472abfb33635ade5b7ef89ebd2a752869515bdd525a527c。这些校验不替代更新真实性或正式发行资格。

## CI 与发行边界

main 推送/PR 触发 Windows CI 的完整检查与安全审计；手动勾选 delivery 或推送 yueshot-v* 标签，才在同一提交检查通过后生成未签名 beta 候选并执行实际安装验证。公开仓库中的 Actions artifact 可被下载。它不是 GitHub Release，也不代表产品已经正式发布。缓存、无缓存入口与实测耗时见 [CI 说明](ci.md)。

构建镜像固定 windows-2022，与当前审查的 VS 2022 / VC143 Redist 合同一致；编译前验证官方 x64 Redist 目录。windows-latest 曾漂移到 VS 2026，其 Release 编译虽通过，却不符合该打包合同。更新编译器/运行库须一起核对工具、打包来源和权利材料，不从开发机 System32 猜测补 DLL。

windows-2022 还需显式安装 NASM：缺失时现有 AVIF 严格 Clippy 在 zenrav1e 汇编构建失败。CI 固定官网 nasm-3.01-win64.zip，使用实际下载计算并记录的 SHA-256 校验归档，再确认版本并加入后续步骤 PATH；不关闭汇编或跳过 AVIF 门禁。此工具仅用于构建，不进入产品安装负载。

正式 release --check 必须在未满足更新、权利、更新密钥恢复、干净用户等门槛时拒绝。用户于 2026-11-09 选择不做 Authenticode 代码签名；所有渠道允许未签名候选，代码签名不再阻塞发行。不通过忽略公告、放宽断言或强杀进程制造验收通过，不将实验 HDR 自动放入默认发行配置。


## NSIS 安装界面与语言（2026-11-09）

本轮仅修改交付界面及其打包配置，没有新增截图功能。安装器使用原生 NSIS 控件，不加载 GPUI、WebView 或附带字体。界面、语言和参数合同见 [打包资源说明](../apps/yue-shot/packaging/README.md#安装界面与语言)。

| 行为 | 修改前 | 修改后与实际验证 |
| --- | --- | --- |
| 安装流程 | 默认多页向导 | 单页安装路径和快捷方式选项 → 真实安装进度 → 自动进入完成页；完成页可取消启动 |
| 语言 | 仅英文 | 当前用户 Windows 界面语言为中文时显示简体中文，其余回退英文；本机默认中文与 `/LANG=en` 英文实窗均验证 |
| 路径 | 原生目录向导 | 自定义页规范化完整路径；中文且含空格的新目录安装成功；相对路径、盘符相对路径、磁盘根和现有文件明确拒绝 |
| 快捷方式 | 统一创建或 `/NS` | 桌面和开始菜单独立选择；仅开始菜单、均不创建两种交互结果检查通过 |
| 卸载 | 旧 NSIS 界面 | 同样自动检测语言，英文覆盖实窗验证；默认不勾选删除数据 |

交互测试曾发现 NSIS `GetFullPathName` 对尚不存在的安装目录返回空字符串，已换为 `GetFullPathNameW` 并校验错误/长度，未通过预建目录绕过。另拒绝 `D:folder` 这类盘符相对路径。相对路径、根目录、文件拒绝均在真实安装页验证，中文新目录安装后逐项核对负载哈希、执行已安装 EXE smoke 并正常卸载。

工具测试修改前后均为 45 项通过。实窗检查采用本机当前 150% 缩放；未验证所有系统语言和 DPI，也不代替干净普通用户验收。现有用户 Beta 安装未覆盖或卸载，实际安装回归使用隔离 dev 渠道。

本轮另外运行的 npm 工具链审计发现 3 项记录（1 low、2 high，含传递影响），当前锁定版本没有自动可用修复；结果保存在本机 `target/installer-npm-audit.json`。这是构建工具链待处理项，不等同于 Rust 产品图审计结论；未修改锁文件或放宽正式发行门槛。

最终 dev 候选 `target/yueshot-delivery/0.1.2-dev-bilingual-setup` 已重新打包，通过 `test-yueshot-install.ps1`、`test-yueshot-installer-policy.ps1`、`test-yueshot-channel-install.ps1` 三项门禁，日志分别位于 `target/installer-runtime-final.log`、`target/installer-policy-final.log`、`target/installer-channel-final.log`。最终安装包交互安装到中文空格目录，完成按钮成功启动该目录中的 EXE，通过产品“退出”按钮正常退出后正常卸载，默认 dev 数据保留。45 项 Python 回归日志为 `target/installer-python-tests-final.log`。以上为本机证据，本轮没有重新运行公开 CI。

供人工查看的最终 Beta 安装包：`target/yueshot-delivery/0.1.2-beta-bilingual-setup/nsis/YueShot_0.1.2_x64-setup.exe`。完整 Release 打包和候选字节校验通过，源码 buildId 与上述 dev 候选一致；未安装覆盖已有 Beta，未推送仓库或创建 Release。


## 安装器 DPI 与系统字体（2026-11-09）

旧 Beta EXE 的 manifest 未声明 DPI 感知，在 150% 下由 Windows 位图缩放；现在使用 NSIS `ManifestDPIAware true`，保持 system DPI 感知，不宣称跨 DPI 显示器动态重排。新增 `tools/yueshot_installer.py` 读取实际 PE manifest（仅映射资源、不执行 EXE），打包校验安装器，安装 smoke 同时校验实际卸载器。旧候选被新检查明确拒绝，新预览通过。

字体探针来自临时生成的 NSIS 预览，直接读取控件所持 HFONT 的 LOGFONT，不进入交付源码模板或产品运行流程。修改前实际正文确为 Microsoft YaHei UI 9pt，未回退宋体。修改后中文为 Microsoft YaHei UI、英文为 Segoe UI；正文、路径框与 MUI 页眉实际高度均为 -20px、字重 400，主标题 -26px、字重 400，对应当前 144 DPI 下的 10pt/13pt。实测记录在 `target/installer-system-font-probe/fonts-zh.txt` 和 `fonts-en.txt`。

| 项目 | 修改前 | 修改后 |
| --- | --- | --- |
| DPI | 无声明，Windows 位图缩放 | system-DPI-aware，原生绘制 |
| 正文/控件 | 9pt | 统一 10pt 常规字重 |
| MUI 页眉 | 默认 700 粗体 | 与正文同字体、字号及 400 字重 |
| 页面主标题 | 12pt、600 字重 | 当前语言字体 13pt、400 字重 |
| 卸载数据选项 | 固定像素位置和高度 | 锚定在原生目录框下方，边距/高度随 dialog units 缩放 |

中英文选项页及中文卸载页在本机当前 150% 缩放下实窗检查，无文字裁切或控件重叠。未变更系统字体、ClearType、缩放或语言设置；没有验证 100%/200% 的实际设备显示，也没有引入字体文件。

最终 dev 候选 `target/yueshot-delivery/0.1.2-dev-system-fonts` 完整 Release 打包通过；47 项工具测试通过（`target/installer-system-font-tests.log`）。三项安装门禁全部通过，日志为 `target/installer-system-font-runtime.log`、`installer-system-font-policy.log`、`installer-system-font-channel.log`，其中安装 smoke 读取安装器和实际卸载器的 PE manifest，均确认为 system DPI 感知。另使用完整候选交互安装到 `target/字体排版 QA`，检查实际进度与完成页，核对所有负载哈希、运行已安装 EXE smoke、取消安装后启动并正常卸载。现有用户 Beta 未覆盖或卸载。

供人工验收的 Beta 安装包为 `target/yueshot-delivery/0.1.2-beta-system-fonts/nsis/YueShot_0.1.2_x64-setup.exe`，完整 Release 打包和字节校验通过，源码 buildId 与 dev 回归候选一致。此次未推送仓库、未创建公开 Release。


## 发行收尾任务（2026-11-09）

按用户“逐项做”执行，继续禁止新增截图功能。1 未签名策略 → 2 提交/远程 CI → 3 干净普通用户 → 4 生命周期/故障 → 5 更新闭环 → 6 发行审查与测试版。

用户明确选择不做 Authenticode，已移除 unsigned stable 的特殊拒绝；所有渠道执行相同构建与检查，候选记录 `codeSigning.authenticode=unsigned`、`required=false`。不以取消代码签名为由取消更新包真实性、源码/许可、干净普通用户等独立验收。旧版拒绝测试按新政策替换为三渠道都进入同一构建检查、未知渠道和已存在输出仍拒绝；48 项工具测试通过，基线 47 项通过。安装提示改为渠道无关的未签名说明。

本机再次查询 `Containers-DisposableClientVM` 为 Disabled，尚未变更系统功能或重启。已询问可用的干净环境；没有把管理员会话冒充普通用户证据。


### 收尾执行证据

未签名 stable 完整 Release 候选 `target/yueshot-delivery/0.1.2-stable-unsigned-policy` 已打包并通过字节校验；隔离安装后逐项验证 payload、执行已安装 EXE smoke、检查安装器/卸载器实际 DPI manifest，再正常卸载并确认设置保留。日志 `target/unsigned-stable-package.log` 与 `unsigned-stable-smoke.log`。`release --check` 仍正确拒绝未完成的其他发行资格；没有创建正式 Release。

提交 `2baabc8` 为安装界面/字体/DPI，`9ca1a2d` 为未签名策略；对应的 [Windows CI 42560905403](https://github.com/jinghu-moon/YueShot/actions/runs/42560905403) 已成功完成（2026-11-09 补查）。

用户选择另一台 Windows 电脑做干净普通用户验收。新增可离线运行的 PowerShell 验收脚本和打包工具，目标机无需开发工具；脚本拒绝提升的管理员会话、已有同渠道安装及运行中的产品，不强杀进程。宿主只验证管理员拒绝路径和 PowerShell AST 语法；另一台电脑的正向运行与人工截图/设备验收仍等待实际结果。验收说明见 [干净用户验收](clean-user-acceptance.md)。

补充运行 `cargo test -p yue-shell --locked -- --include-ignored --test-threads=1`：6 项全部通过、0 ignored，包含真实测试图标创建/销毁和隔离组合键占用/恢复；这不是重启真实 Explorer 或真实登录/睡眠证明。

先前 npm 审计的 3 条工具链记录已从依赖根因处理：直接依赖相同版本 `@crabnebula/packager-win32-x64-msvc@0.11.2`，由本仓库极薄 Node 入口传入完整 JSON，移除 Electron 探测、extract-zip 和 deepmerge。打包版本、NSIS 模板及产品图不变。npm ci 和 npm audit 现为 0 条漏洞记录；新入口真实编译 NSIS 后实际 DPI 和图标校验通过，日志 `target/native-packager-validation.log` 与 `delivery-native-audit.json`。CI 新增 npm audit 门禁。此结论不代表 native 打包器所有漏洞均已穷尽。


### 文件部署故障回归（2026-11-09）

故障基线：以纯文本 EXE fixture 持有 `FileShare.None` 句柄，旧安装器静默返回 0、保留该文件，却仍创建卸载登记。证据 `target/installer-file-lock-evidence.json`。根因是 NSIS 文件写入失败后未检查错误，继续执行成功登记；不是截图或快捷键问题。隔离故障现场已在核对注册路径和 fixture 字节后修复并正常卸载，用户 Beta 安装未动。

现在安装先在目标卷暂存全部负载、备份全部原文件，再发布新文件；文件失败时回滚并返回 14，回滚失败返回 15 且保留恢复文件。卸载同样先备份全部受管文件，避免后面的资源锁导致前面已删除。所有渠道的本机安装脚本使用相同策略，仍拒绝覆盖已有用户安装。

| 行为 | 修改前实测 | 修改后实测 |
| --- | --- | --- |
| 新安装 EXE 被锁 | 返回 0，文件未替换但创建成功登记 | 返回 14，字节不变，不创建登记 |
| 重装 EXE / 后面的 DLL / 卸载器被锁 | 原实现没有完整文件事务 | 均返回 14，所有原文件及登记逐项一致 |
| 卸载后面的通知文件被锁 | 原实现直接逐个删除，没有回滚边界 | 返回 14，已移走文件恢复，登记保留 |
| 文件路径上已有用户目录 | 未建立明确保护 | 拒绝替换，目录及其文件保留 |
| 存在中断事务目录 | 没有恢复材料保护策略 | 返回 14，恢复证据字节不变 |
| 正常安装 / 重装 / 卸载 | 正常 | 三项实际安装门禁全部通过，数据保留 |

完整未签名候选 `target/yueshot-delivery/0.1.2-stable-file-transaction` 已通过 Release 构建、负载字节及审计校验。48 项 Python 工具回归通过，PowerShell 验收脚本 AST 解析通过；原生产品热键冲突测试 `tests::a_conflicting_product_hotkey_does_not_disable_the_capture_runtime` 实际执行 1 项通过、0 ignored。日志分别为 `target/installer-transaction-package.log`、`installer-transaction-python-final.log`、`installer-transaction-policy.log`、`installer-transaction-runtime.log`、`installer-transaction-channel.log`、`lifecycle-product-hotkey.log`。

事务是针对当前受管文件集合的失败处理；断电后的自动恢复、磁盘不足、注册表写入故障与真实设备生命周期尚未全部验证，不据此声称完整故障资格。在线更新库 Windows 默认安装路径会直接退出进程，后续整合须走产品保存/退出策略及安装结果检查，不能直接套用 `download_and_install`。尚未接入自动更新，也未创建公开 Release。


### 普通用户自动验收回报（2026-11-09）

用户提供另一台 Windows x64 电脑的 `Auto` 控制台结果：OS Build `10.0.19044.0`、`elevated=false`，候选 `0.1.2 stable`，buildId `f7a4f6a9b5dc9369cb4d28aa709f3d03043323fb98a7886ed765b54b03077ca6`，状态 `passed-automated-checks`。安装器及已安装负载哈希、渠道登记、已安装 EXE 运行库加载及配置/存储 smoke、同版本重装保留配置、卸载移除程序和登记并保留测试数据均通过。

[结构化回报](acceptance/2026-11-09-windows-19044-auto.json) 根据用户提供的 JSON 记录，非代理独立执行；未确认该电脑此前是否安装过 VC++ 运行库，因此不据此声称干净运行库或所有最低系统资格。`manualQualification=pending`，截图/长截图、保存、托盘、睡眠、登录自启及 Explorer 恢复仍需人工结果；下一步执行验收脚本 `StartManual`，完成后正常退出，再执行 `FinishManual`。


### 人工会话收尾回报与主窗口关闭缺陷（2026-11-09）

用户提供相同 Windows Build 19044、相同 buildId 的 `FinishManual` 成功结果，已安装负载/登记校验、卸载及人工会话所有数据字节保留均通过。[收尾回报](acceptance/2026-11-09-windows-19044-finish-manual.json) 是用户控制台证据，并不表示各人工项目通过；用户报告 X 导致整个进程退出及托盘消失，人工资格仍为 pending。

已用原生独立进程回归复现：`closeToTray=true`、真实托盘可用时，最后一个主窗口的 `WM_CLOSE` 仍导致进程退出。根因是产品未配置 GPUI Windows 默认的 `QuitMode::LastWindowClosed`；现在入口使用 `QuitMode::Explicit`，进程退出由既有 `request_quit` 策略控制。保存中的退出阻止和托盘不可用时的退出策略保留。

回归同时暴露退出分支的原生异常：Windows display DispatcherQueue 在 Platform Drop 中清理，其嵌套消息泵执行延迟的 `DestroyWindow`，此时 AppCell 已释放，窗口关闭回调访问 AsyncApp 引发 panic。调试符号栈确认调用链，修复为 quit observers 完成后、`Application::run` 仍持有 AppCell 时清理该队列；Drop 仅保留未运行平台的平衡释放。仍注销颜色事件和等待 WinRT shutdown，不通过忽略回调、捕获 panic 或泄漏队列绕过。组件快照以 localAdaptations 记录原始哈希、修改哈希及理由。

| 行为 | 修复前实测 | 修复后目标 |
| --- | --- | --- |
| 默认 X / 最后窗口关闭 | 进程及托盘一起退出 | 关闭主窗口，进程/托盘保留，可唤醒原实例 |
| 取消关闭到托盘后 X | 额外回归揭示退出时 AsyncApp panic | 正常退出 0，托盘及窗口移除 |
| 保存/复制进行中退出 | 产品已有阻止策略 | 保留，相关保存用例回归 |

新增 `tools/test-yueshot-window-close.py` 在真实进程中发送平台 `WM_CLOSE`、检查进程与原生托盘窗口并通过同数据目录 Show 命令唤醒；另启动关闭到托盘为 false 的独立进程检查正常退出。它只终止本脚本创建的存活测试子进程以清场，不能冒充托盘菜单优雅退出验收，也不能代替另一台电脑的 X 按钮人工复测。旧包失败证据 `target/window-close-before`，仅修复退出模式后暴露的异常证据 `target/window-close-debug`；完整修复的调试版原生回归 `target/window-close-debug-fixed` 两个分支均通过。CI 安装门禁后增加此进程测试。


最终修复候选 `target/yueshot-delivery/0.1.2-stable-close-to-tray-fixed` 的 Release 构建、负载字节及产品审计通过，buildId `7078e44cdc9ab4893b413e472d0f0271a76658f3d037887eb5c5ced0abc9e2b3`。`test-yueshot-window-close.py` 针对实际 Release EXE 的两个分支均通过（`target/window-close-release-fixed/report.json`）；安装、文件锁/回滚策略与渠道快捷方式三项门禁通过（`target/window-close-final-{runtime,policy,channel}.log`）。

`cargo test -p yue-shot --features test-support --locked --lib -- --test-threads=1` 为 254 passed / 4 ignored / 0 failed；另显式运行既有窗口关闭/托盘健康测试 1 passed / 0 ignored。48 项 Python 测试通过；`cargo clippy -p yue-shot --all-targets --features test-support,avif,hdr-capture --locked -- -D warnings` 通过；产品既有范围 fmt 与修改的 Windows platform 文件 rustfmt 通过。一次扩大到 `cargo fmt --all` 的检查发现历史未格式化的 `yue-image-codec` 和快照 `image_preview` 文件，未将这些无关格式修改混入修复，也不宣称全仓 fmt 通过。

新验收 ZIP 为 `target/yueshot-acceptance/0.1.2-x64-close-to-tray-fixed.zip`，候选字节校验和 ZIP CRC/无 results 的初始清单检查通过。需用户在另一台电脑解压到新目录，用 `StartManual` 重点复测 X 后托盘/热键和重新打开，再正常退出并执行 `FinishManual`；不能复用上轮 buildId 的人工资格。上一轮安装器提交 d674951 的 [Windows CI 42563037473](https://github.com/jinghu-moon/YueShot/actions/runs/42563037473) 已全部通过，本次生命周期修复的 CI 结果另计。


### 主窗口关闭修复的普通用户复测通过（2026-11-09）

用户在另一台 Windows Build `10.0.19044.0`、`elevated=false` 环境，提供修复包 buildId `7078e44cdc9ab4893b413e472d0f0271a76658f3d037887eb5c5ced0abc9e2b3` 的 StartManual / FinishManual 控制台结果，并明确反馈“测试没有问题”。依据紧接其前的定向复测步骤，将 X 关闭保留托盘、F5 仍可截图、托盘重新打开窗口、正常退出及卸载保留人工数据记为通过。见 [复测证据](acceptance/2026-11-09-windows-19044-close-to-tray-retest.json)。旧构建的失败记录保留。

脚本原始 `manualQualification=pending` 不改写；它不自动判断人工操作，人工结论单独记录在 manualConfirmation 中。此次结论限于上述定向复测，没有据此补签睡眠恢复、真实登录自启、Explorer 重启、所有 DPI/多屏、磁盘不足、在线更新或干净运行库资格。生命周期修复提交 661b878 的 [Windows CI 42565942191](https://github.com/jinghu-moon/YueShot/actions/runs/42565942191) 已全部通过，包含检查、安全审计、Release 打包、安装/回滚/渠道/原生关闭到托盘回归及 artifact 回读；耗时 26 分 18 秒。在线更新和首次公开测试发行仍未完成。

### CI 缓存与交付分离验收通过（2026-11-09）

提交 b93e511 将普通 push/PR 的完整检查、安全审计与手动/标签触发的完整交付拆成两个任务，保留所有原有门禁；增加分离的 debug/release 编译缓存、同 ref 旧运行取消及无编译缓存入口。普通 push [42570052468](https://github.com/jinghu-moon/YueShot/actions/runs/42570052468) 在首次无编译缓存下通过，delivery 按设计跳过。随后同一提交的手动完整运行 [42571676485](https://github.com/jinghu-moon/YueShot/actions/runs/42571676485) 全部成功，debug 精确命中缓存后 verify 为 7 分 27 秒，首次无 release 缓存的 delivery 为 14 分 01 秒；安装/回滚/渠道/关闭到托盘与 artifact 回读全部通过。实际范围、耗时和缓存限制见 [CI 说明](ci.md)。本轮没有发布 Release，也没有据此补签任何尚未完成的产品发行资格。

### 更新认证与正常退出交接（2026-11-09）

设置页已接入按需检查、认证下载和退出安装，网络、签名与安装辅助进程均归产品所有；没有新增截图功能或公共平台 crate。先以渠道内置公钥验证 Minisign 签名，再解析产品/版本/渠道/架构/有效期和安装器哈希；下载失败不留下可执行暂存包。辅助程序重新认证并锁定安装包，等待产品明确提交及原进程完整退出后才安装，校验新 EXE 哈希与产品身份，再以原数据目录重启。截图或保存未结束时不交接安装。详见 [在线更新协议和验证范围](updates.md)。

用户自行生成并加密 beta 密钥，已将其公钥固化到 beta 客户端。私钥在仓库外，助手未读取私钥或获得密码。初版 `target/yueshot-delivery/0.1.2-beta-update-bootstrap` 曾完成 Release 打包、来源归档、负载及运行库校验，buildId 为 `188b2b4f6464be6d5e52759bac04bfd060cf72cc5074545f03d5ce6df43d8711`。随后实测发现 NSIS 含空格路径参数的引号问题，该包不再作为验收入口，需修复重建；根因、前后探针及原生回归证据见 updates.md。没有安装此包或覆盖用户现有 Beta。

| 项目 | 修改前 | 本轮验证结果 |
| --- | --- | --- |
| 检查和下载更新 | 没有客户端链路 | 签名、身份、版本、期限、大小及哈希负面回归通过 |
| 退出安装 | 尚无产品交接 | 原生 fixture 验证明确提交及父进程退出前不安装，成功后重启、失败保留旧 EXE，用户数据保留 |
| X 与关闭到托盘 | 已修复 | 新 dev 渠道 Release 的真实进程两个分支均通过，见 `target/update-window-close-native/report.json` |
| 普通安装/重装/卸载与锁冲突 | 已有完整门禁 | 新 dev 渠道 Release 三项门禁通过，未覆盖 Beta 安装 |
| 在线真实版本升级 | 未完成 | 仍待生产签名、受审发行材料与真实 HTTPS → NSIS → 重启验收 |

定向更新回归 10 passed / 3 ignored（两个入口由原生回归显式执行，HTTPS 资格测试单独运行 1 passed / 0 ignored）；完整 `cargo xtask check`、严格 Clippy 和安全审计通过，日志及范围见 updates.md。不能把测试密钥/fixture 安装器的通过计作生产密钥和真实 NSIS 在线升级通过；公钥配置也不代表已备份私钥或授权公开发布。


### Beta 签名密钥备份与恢复（2026-11-09）

用户指定 U 盘并授权本机不透明复制后，已识别为 USB 可移动介质，完成加密私钥/公钥备份、刷新写盘、从 USB 恢复到新目录及原文件/备份/恢复副本字节哈希一致性校验；没有输出私钥内容、私钥哈希或密码。用户随后在本机用恢复副本路径输入密码并签署公开 challenge，助手使用客户端内置的原 beta 公钥验证签名及 comment 签名通过。见 [恢复演练与公开证据](update-key-recovery.md)。

据此将备份、恢复及恢复副本实际签署/验签记为通过。随后用户明确确认安全弹出并重新插入 U 盘，助手重新识别设备并复读，原文件/USB 备份/已验签恢复副本哈希一致，从 USB 读取的公开消息再次验签通过；物理拔插后复读亦记为通过。密钥轮换和丢失/泄露补救仍待验证；没有生成新生产密钥、修改信任根、改动公开更新清单或删除任何原密钥/备份/恢复副本。

### 登录自启、Explorer 与睡眠人工验收（2026-11-09）

向用户提供升级后 0.1.3 Beta 在测试电脑上的三个步骤：开启自启后真实注销/重新登录，不手动启动而检查托盘、F5 与 Ctrl+S；任务管理器重新启动 Windows 资源管理器，检查托盘重新出现、主窗口可打开以及截图保存；正常睡眠唤醒后检查主窗口、托盘及 F5/F7 的 Ctrl+S 保存。用户明确回复“三个测试都没有问题”。据此将上述范围记为用户实测通过，见 [结构化证据](acceptance/2026-11-09-beta-lifecycle-manual.json)。

这不是代理在目标机独立执行，没有据此沿用旧轮次的 OS build、账户权限或 buildId，也不将单轮通过扩展为重复恢复循环、资源稳定性或全部硬件资格。旧人工报告与故障证据保持不变。本轮仅更新文档和验收记录，没有修改产品、安装器、密钥或公开更新源。

接下来依次收尾：隔离环境中的升级故障验证（断网、文件占用、磁盘不足，断电恢复另计）、分发条款/许可与已知限制、发布操作手册和回归基线。现有文件锁/事务回滚与网络中断 fixture 证据优先复用，缺失场景补实测；继续禁止新增截图功能，不把完整生命周期或稳定发行资格标为全部完成。

### 安装失败清理与发布基线（2026-11-09）

隔离 VHD 实际磁盘满发现 NSIS `File` 可在写失败时直接终止，跳过后续 `IfErrors`，返回 2 并留下不完整事务。
已将显式失败与 NSIS 失败回调统一到受当前调用所有权保护的回滚/清理路径，已有事务继续保留。
新版 dev Release 通过真实磁盘满返回 14、旧文件/数据/登记完整、旧 EXE 可启动、释放空间可重试；
安装 smoke、文件锁/回滚策略、渠道/快捷方式及关闭到托盘两个分支均通过，另有 51 项 Python 与 10 项更新回归通过。
候选、失败基线和未覆盖项见 [安装与升级失败验证](upgrade-failure-qualification.md)。没有覆盖现有 Beta 安装，公开 0.1.3 资产及更新源未变。

已补齐 [发布操作手册](release-runbook.md)、[回归基线](regression-baseline.md) 与 [已知限制](known-limitations.md)。
[分发权利](distribution-rights.md) 已明确微软运行库的剩余下游条款问题，并移除已完成的图标来源/更新真实性/密钥恢复待办。
本轮没有批准或自行添加新的微软下游协议，也没有勾选稳定发行资格；实际断网、断电、注册表写失败及其余环境/稳定性资格仍单列。
