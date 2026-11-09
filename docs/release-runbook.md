# YueShot 发布操作手册

适用：Windows x64、默认 SDR、独立渠道安装。2026-11-09。
当前公开版本为 0.1.3 Beta；本手册不表示稳定发行资格已经完成。
不新增截图功能，不替换已经公开的版本资产，不将 Authenticode 与更新真实性签名混为一谈。

## 1. 固定版本与源码

1. 更新 `apps/yue-shot/Cargo.toml` 中的产品版本，并核对锁文件及发布说明。不要升级无关依赖。
2. 提交后记录完整 commit、渠道、目标架构。工作区必须干净；tag 使用 `yueshot-v<实际版本>`。
3. 核对 [已知限制](known-limitations.md)、[回归基线](regression-baseline.md) 和 [分发权利](distribution-rights.md)。
4. 更新渠道公钥必须先单独审查。旧客户端只信任其构建时内置的公钥，不会自动读取仓库里的新配置。

产品版本也进入许可清单。版本提升触发 inventory 门禁时，先比较 `target/yueshot-license-review/dependency-licenses.json`
与已审清单，确认是产品自身版本变化，核对第三方版本、features、来源和许可原文哈希；审查后才运行
`python tools/yueshot_licenses.py --update` 并提交差异。不得用整体刷新清单掩盖无关依赖变化。

## 2. 检查与构建候选

```powershell
cargo xtask check
cargo xtask package --profile release --channel beta --output target/yueshot-delivery/<新的候选名>
python tools/yueshot-delivery.py verify target/yueshot-delivery/<新的候选名>
```

保留 check/package 日志、`delivery.json`、构建标识、对应源码、依赖源码、许可材料、PDB 和安全审计。
同名输出目录禁止复用。候选构建后若源码或安装器改变，应重新构建新候选，不改旧清单里的哈希。
首次准备打包工具及 CI 触发方式见 [README](../README.md#检查与交付) 和 [CI](ci.md)。

公开下载只提供一个完整源码附件，内部两个原始源码包仍作为不可变构建证据保留：

```powershell
python tools/yueshot_source_bundle.py target/yueshot-delivery/<候选> --output target/yueshot-updates/<目录>/YueShot_<版本>_source.zip
python tools/yueshot_source_bundle.py target/yueshot-delivery/<候选> --verify target/yueshot-updates/<目录>/YueShot_<版本>_source.zip
```

合并器只读取候选中已绑定哈希的源码、依赖和声明，不将当前工作区代码混入历史候选。
项目源码直接位于 ZIP 根目录，`third-party/crates/` 保存默认 Windows 产品 normal/build 图的原始 `.crate`，
`BUILDING-SOURCE.md` 提供工具链和构建说明；`SOURCE-BUNDLE.json` 关联原候选 commit/buildId 及逐文件校验。
可选 features、其他平台和 workspace 测试的额外依赖不宣称已全部包含。工具链与 Cargo 索引不包含在包中，
不宣称全离线开发环境或逐字节复现 EXE。验收工具也改为携带这一个合并源码附件。
仅合并源码发布材料不会修改安装器或已签署清单；更新最终 SHA256SUMS 和发布附件清单即可，无须重新签署未变的 JSON。

普通 push/PR 只执行 verify/security，不代表本次候选已经完成安装测试。
需在同一提交手动触发 `delivery` 或通过版本 tag 运行完整交付流程；Actions 不自动创建 GitHub Release。

## 3. 安装与失败回归

先正常退出 YueShot。安装回归使用没有既有安装的 dev 渠道候选；不要用脚本覆盖日常 Beta。

```powershell
./tools/test-yueshot-install.ps1 -Candidate target/yueshot-delivery/<dev候选>
./tools/test-yueshot-installer-policy.ps1 -Candidate target/yueshot-delivery/<dev候选>
./tools/test-yueshot-channel-install.ps1 -Candidate target/yueshot-delivery/<dev候选>
python tools/test-yueshot-window-close.py --binary target/yueshot-delivery/<dev候选>/bin/YueShot.exe --output target/window-close-<新名称>
```

原生关闭回归指向该候选的 Release EXE 和新的隔离输出目录。
需要磁盘不足回归时，在管理员 PowerShell 中执行下列独立工具。它只格式化自己新建并核对身份的 128 MiB VHD，
不会填满宿主磁盘；失败保留 VHD 和结果，禁止直接重跑覆盖现场。

```powershell
./tools/test-yueshot-low-space.ps1 -Candidate target/yueshot-delivery/<dev候选> -Output target/low-space/<新名称>
./tools/test-yueshot-low-space.ps1 -Candidate target/yueshot-delivery/<dev候选> -Output target/low-space/<另一个新名称> -UpdateDownload
```

`-UpdateDownload` 另用当前产品测试代码读取公开 Beta 签名源，实际 WinHTTP 下载到同一个受控 VHD。
它验证写满返回 112 后暂存清理，以及释放空间后相同清单/安装器的重新下载和哈希验证；不运行下载的安装器。
此资格依赖网络和生产公钥，必须显式运行，不能将默认 ignored 项计为通过。

普通用户验收按 [干净用户步骤](clean-user-acceptance.md) 使用全新解压目录执行，保存原始输出。
自动脚本返回 `manualQualification=pending` 时，人工结论另记；不能编辑成自动通过。
截图、长截图、Ctrl+S、X 关闭到托盘、再打开、正常退出、真实登录自启、Explorer 重启、睡眠恢复为固定回归集合。
旧版本或其他机器的通过只能作为历史基线；更新影响对应路径时，必须复测受影响项。

## 4. 准备并签署更新

仅在候选字节与验收范围确定后生成清单；tag 必须对应实际版本。

```powershell
python tools/yueshot-update.py prepare --candidate target/yueshot-delivery/<候选> --tag yueshot-v<版本> --output target/yueshot-updates/<新的目录>
python tools/yueshot-update.py sign target/yueshot-updates/<新的目录> --secret-key D:\YueShot-Release\beta.key --minisign target/update-tools/minisign-win64/x86_64/minisign.exe
```

私钥由维护者在仓库外保管，密码只在 Minisign 本机提示中输入。签署由维护者执行，不放进 CI secret、参数或日志。
签署后必须看到渠道公钥验证成功；密码错误、命令非零退出或签名文件存在，都不能当作成功。
还要以客户端同一验签路径复核原文接受、修改字节拒绝，并核对安装器大小、SHA-256、新 EXE SHA-256、版本、渠道和有效期。
`prepare/sign` 不上传、不发布，也不会签发候选发行资格。

清单默认有效期 180 天，以 `expires_unix` 为准。安排在到期前重新评估并签署，不能直接修改已签署 JSON。
当前版本相同的客户端会忽略该版本；有效期续签不能替代新版本发布。

## 5. 审阅并公开

先形成可审阅的发布清单：commit/tag、版本与渠道、资产名称/大小/哈希、签名验证结果、验收范围和未完成项。
本地准备不等于发布授权；新的公开发布须取得维护者明确授权。

采用两个版本附件、两个更新源附件的固定布局：

| 发布位置 | 附件 | 用途 |
| --- | --- | --- |
| `yueshot-v<版本>` | 安装器 EXE、`YueShot_<版本>_source.zip` | 用户下载；源码包内含构建说明、许可清单与第三方原始声明 |
| `yueshot-updater-beta` | `latest.json`、`latest.json.minisig` | Beta 客户端固定更新源 |
| `yueshot-updater-stable` | `latest.json`、`latest.json.minisig` | 正式客户端固定更新源，完成 stable 资格并配置公钥后才启用 |

SHA-256 写进版本发布说明，不再独立上传 SHA256SUMS、许可 JSON、声明文本或更新清单。
两项源码内部输入仍保留作本地证据；不要为减少公开附件删除安装器或源码包内的许可声明。
客户端直接下载 `/releases/download/yueshot-updater-<channel>/latest.json` 与 `.minisig`，不解析网页或依赖 GitHub API/latest。

候选、合并源码及维护者签名完成后，使用新输出目录生成可审阅方案：

```powershell
python tools/yueshot-update.py plan --candidate target/yueshot-delivery/<候选> --directory target/yueshot-updates/<签署目录> --source target/yueshot-updates/<目录>/YueShot_<版本>_source.zip --notes <发布说明.md> --output target/yueshot-updates/<新的发布方案目录> --minisign <官方minisign.exe>
```

此命令核验候选、源码包、安装器、清单身份/有效期，并以候选原源码中的渠道公钥验签，生成两附件列表、带哈希的说明和更新源说明。
它不上传或发布，也不读取私钥。dirty 候选、未取得正式资格的 stable 候选和没有匹配公钥签名的渠道均不能生成有效方案。

1. 先创建版本 draft，固定 commit，仅上传方案中的安装器和完整源码包，发布说明附两者 SHA-256。
2. 逐项核对远端资产，版本资产必须与本地已验证字节一致。Beta 使用 prerelease，不能设为 stable latest。
3. 授权后公开版本 Release；从**不带 GitHub 登录凭据**的公开 HTTPS 地址重新下载并验签、核对哈希。
4. 版本安装器和源码包公开且回读成功后，最后更新本次渠道的 `yueshot-updater-<channel>` 配对清单/签名。只更新该渠道，不能以 Beta 材料启用 stable。
5. 客户端会拒绝清单/签名不配对；更新两文件期间允许短暂可重试错误，不得先发布指向尚不可下载安装器的清单。
6. 从实际已安装旧版测试检查、下载、退出安装、自动重启和设置/截图保留，记录环境、两版本 buildId 和实际结果。

不要依赖仓库的 `releases/latest` 作为各渠道更新源；不要覆盖既有版本安装器或移动既有版本 tag。
当前 0.1.3 Beta 的授权、公开回读和用户升级结果见 [更新记录](updates.md)。

## 6. 失败处理与收尾

| 故障 | 维护者操作 |
| --- | --- |
| 签名/身份/哈希失败 | 停止发布，保留原始字节和错误；核对正确候选，不能关闭验签 |
| 下载中断或磁盘不足 | 保持旧程序；解决连接/空间后重试，记录是否清理了不可用暂存包 |
| 安装返回 10 | 正常退出占用的 YueShot，再重试；不强杀进程 |
| 返回 12/13 | 核对降级或损坏版本登记，不手工降低版本绕过保护 |
| 返回 14，残留 `.yueshot-install` | 先保留目录与日志；区分本次清理失败和既有中断事务，不能直接删除可能仅存的原文件 |
| 返回 15 | 回滚失败，保留备份和故障现场，先恢复并验证一致的安装再运行 |
| 已公开版本有缺陷 | 修复并发布更高版本；现有客户端拒绝降级，不能把旧版写回滚动源当作回滚方案 |
| 密钥丢失/泄露 | 暂停发布；现有轮换/带外补救尚未演练，不能生成新密钥后假设旧客户端会信任 |

将验收摘要与公开证据入库；本机日志、故障镜像及 PDB 保留在受控存储。不要提交私钥或用户截图/正文。
本次新安装器行为若未发布，必须明确说明当前公开 0.1.3 安装器仍是旧字节。

稳定发行前仍须完成分发条款、最低系统/运行库等剩余资格；`release --check` 保持失败，不能手改
`readyForRelease` 或清空 `outstanding` 制造通过。当前工具尚无基于审查记录自动签发正式发行资格的入口。
