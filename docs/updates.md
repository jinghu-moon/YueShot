# YueShot 在线更新

Windows x64，按 dev / beta / stable 分离。当前已经接入客户端与本地进程回归；用户在 2026-11-09 本机生成带密码的 beta 私钥，并提供公钥，beta 信任根已经配置。尚未上传签名清单或完成真实版本之间的在线 NSIS 升级验收。安装包仍可不做 Authenticode 签名；这里的 Minisign 签名用于更新来源认证，二者独立。

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

目录需预先创建。不要加 `-W` 取消密码保护。公钥改动审查后重新构建客户端，已经安装的旧客户端不会动态读取仓库文件。正式发行前另行完成离线备份、恢复及轮换演练；本次没有自动创建生产私钥或声称其已备份。

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

尚待真实签名清单 → HTTPS 下载 → 已安装旧版 → 新版 NSIS → 重启的整条验收，以及生产密钥备份/恢复。已完成的自动安装/回滚/渠道回归仍继续保留。

2026-11-09 本地证据：更新定向回归 10 passed / 3 ignored；两个子进程入口在原生回归中显式调用，HTTPS 用例另行实际执行 1 passed / 0 ignored。`cargo xtask check` 已通过，含 51 项 Python、1,096 次 Rust 通过与 107 次 ignored 出现（当时未包含随后新增的注册表读取用例和 HTTPS ignored 入口，新增用例另行定向通过）。安全审计 0 blocking / 0 maintenance notices，15 条产品图外记录保留。

完整 dev 渠道 Release 候选 `target/yueshot-delivery/0.1.2-dev-authenticated-updater-release` 的来源/字节/运行库检查、安装 smoke、文件事务与回滚策略、渠道/快捷方式检查全部通过，未覆盖现有 beta 安装。它构建于配置 beta 公钥之前，不能用作 beta 更新客户端。首次 dev profile 打包被既有运行库门禁拒绝 `d3dcompiler_47.dll` 静态导入，失败记录保留在 target/update-package.log；本次未把该 DLL 加入白名单，正式 Release 配置正常通过。

实际 GUI 使用隔离数据目录验证设置页更新区域、按钮焦点、鼠标及 Enter 操作、缺少公钥时的可重试提示，并通过“退出”正常结束。签名测试数据还使用官方 Minisign 0.12 验证通过，不只依赖 Rust 验证器自身。

带用户 beta 公钥的引导候选 `target/yueshot-delivery/0.1.2-beta-update-bootstrap` 已完成 Release 打包及候选校验，buildId `188b2b4f6464be6d5e52759bac04bfd060cf72cc5074545f03d5ce6df43d8711`；没有安装它或覆盖现有 Beta。dev 渠道 Release 的原生关闭窗口回归两个分支均通过，结果在 `target/update-window-close-native/report.json`。引导版与新版之间的真实升级尚待签署和发行验收。
