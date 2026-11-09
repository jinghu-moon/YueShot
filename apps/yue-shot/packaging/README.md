# YueShot 打包资源

渠道身份由产品 `--product-info` 输出，交付工具严格校验后用于模板；不再使用 Delivery Test 安装身份。

| 渠道 | 显示名 | 安装目录（LOCALAPPDATA 下） | 数据目录（LOCALAPPDATA 下） |
| --- | --- | --- | --- |
| dev | YueShot Dev | Programs/Yue/YueShot/dev | Yue/YueShot/dev |
| beta | YueShot Beta | Programs/Yue/YueShot/beta | Yue/YueShot/beta |
| stable | YueShot | Programs/Yue/YueShot/stable | Yue/YueShot/stable |

安装登记按 `com.seeyuer.yueshot.<channel>.x64` 隔离；AUMID 为 `com.seeyuer.YueShot.<channel>`。
架构进入安装身份，数据目录按渠道共享；本轮仅 x64。快捷方式不覆写 `--data-dir`，使用产品默认渠道根。
用户的自定义数据根仍由 CLI 管理。卸载仅清理本 EXE 位置与渠道拥有的启动命令（包括仍存在的自定义根），
不创建默认数据目录、不删除其他安装/渠道的启动项、不修改 Windows-owned StartupApproved。
默认保留数据；交互勾选清理只对应本渠道默认数据根，自定义根不会被安装器递归删除。

```powershell
cargo xtask package --channel beta --profile release --output target/yueshot-delivery/<candidate>
```

用户于 2026-11-09 选择不做 Windows Authenticode 代码签名，dev/beta/stable 均允许明确未签名的候选；交付清单记录该策略。源码、许可、安全、运行库、安装检查继续执行，生成 stable 候选不等于已通过发行验收。自动更新包的真实性签名与密钥恢复仍需完成。
本地安装验收仍使用 `/D` 指定候选目录，并在已有同渠道安装或同名进程存在时停止。
`test-yueshot-channel-install.ps1` 另验证真实快捷方式/AUMID 和安装登记，运行前必须确保没有同名用户快捷方式。

`YueShot.ico` 为用户于 2026-11-08 指定的 `refer/yue-shot.ico` 原始文件，逐字节复制，不重新缩放或编码。
包含 16、24、32、48、64、128、256 px 图层；SHA-256 为
`da28e6ec295182ffb0e5759fff15af6760fd85bec06d837afa888ef0ddee1323`。

构建和安装器统一使用本目录下的副本，干净构建不依赖被 Git 忽略的 `refer/`。
`build.rs` 将其嵌入 `YueShot.exe`；交付工具将其提供给 NSIS。
资产来源与校验记录位于 `tools/delivery/licenses/sources.json`，check/package 会校验哈希。
NSIS 必须显式设置 `installerIcon`，模板同时设置 `MUI_ICON` / `MUI_UNICON`；
仅设置通用 `icons` 不会替换 NSIS 的默认安装器图标。打包后和安装验收时，
`tools/yueshot_icons.py` 逐图层比对 exe、安装器与卸载器中的资源原文。

用户于 2026-11-08 确认为自行创作、持有权利且允许随公开源码和安装包分发；记录该声明，不冒充独立权利鉴定。
原有 Tabler 图标继续用于相关界面控件，其 MIT 版权材料保留；旧临时应用图标的 Tabler 来源
不适用于此次提供的新 ICO。

安装器使用独立的安装前版本判断，静默、被动和交互模式均不能降级；损坏或缺少的已安装版本登记明确拒绝。
相同版本重装及正常向前安装允许。存在同名 YueShot 进程时，安装与卸载都要求先正常退出，绝不强杀；
同名检测目前是保守的全机进程策略，不代表已经支持多安装实例并行更新。

实际策略回归入口（使用新的隔离候选路径并先退出 YueShot）：

```powershell
./tools/test-yueshot-install.ps1 -Candidate target/yueshot-delivery/<candidate>
./tools/test-yueshot-installer-policy.ps1 -Candidate target/yueshot-delivery/<candidate>
```

第二个脚本验证降级/损坏登记拒绝时文件、用户数据与登记均不变化；验证重装、最低合法 SemVer、
运行中安装/卸载拒绝及用户额外文件保留。运行中测试使用同名受控辅助进程，通过事件通知正常退出，
不杀死真实产品。版本登记 fixture 不等于真实跨版本数据库升级或在线更新验收。


## 安装界面与语言

使用 NSIS 原生自定义选项页 → 原生真实安装进度 → 自定义完成页，移除默认蓝色侧栏、重复欢迎/许可/目录/开始菜单向导和通用 WiX 维护流程。许可原文保留可打开入口；不引入 GPUI、WebView 或额外字体运行依赖。

安装和卸载均读取 `GetUserDefaultUILanguage`（当前 Windows 用户的界面语言）：中文各区域使用简体中文，英文及其他暂未支持的语言使用英文。不读取地区格式、代码页或上一轮安装语言，不修改系统语言设置。NSIS 标准页面与产品提示使用相同语言；系统文件夹选择器/MessageBox 的原生按钮仍由 Windows 本身提供。

可选命令行覆盖 `/LANG=en`、`/LANG=zh-CN` 用于人工切换和重复 QA；不弹语言选择向导。安装位置与桌面/开始菜单快捷方式在同一选项页设置。新安装、同版本、向前升级分别显示“安装 / 重新安装 / 更新”（英文 Install / Reinstall / Update）。完成页可选择启动程序。已有快捷方式不会因为本次未勾选“创建”而被删除。

`/S` 静默、`/P` 被动、`/NS` 默认不创建快捷方式、`/R` 静默/被动完成后启动、最后一个参数 `/D=<绝对安装路径>` 的合同保留；静默模式不展示选项/完成页。版本拒绝、运行中保护及默认数据保留与既有策略相同。

安装与卸载 EXE 显式声明 system-DPI-aware，由 NSIS 在系统 DPI 下创建原生字体和控件，避免高缩放下 Windows 位图拉伸。未声明 per-monitor 支持；跨不同 DPI 显示器移动仍可能被系统缩放。打包及安装 smoke 校验实际 EXE 的 manifest，防止声明丢失。

字体统一使用系统自带的 Microsoft YaHei UI（中文）与 Segoe UI（英文），正文、页眉、按钮和路径框为 10pt 常规字重，页面主标题为 13pt 常规字重。不附带字体文件；标题直接使用 NSIS 当前语言的字体名称，避免重复定义漂移。卸载数据复选框按原生目录控件的位置和 dialog units 布局，随字号/DPI 缩放。

Windows 构建工具仅安装锁定的 `@crabnebula/packager-win32-x64-msvc@0.11.2`，`tools/delivery/package.cjs` 直接调用原生 API，不自动扫描 Electron 或合并外部配置。CI 执行 npm audit，产品运行时不依赖 Node。

## 文件部署失败与恢复

安装先在目标目录内独占创建 `.yueshot-install`，暂存全部负载和卸载器；先将所有原文件移入备份，再发布新文件。任一文件失败时恢复已移动原文件，返回 14，不写成功登记。卸载也先移走全部受管文件；后面的资源被锁时恢复先前移动的文件并保留登记。用户配置和非受管文件不进入事务。路径上已有目录时拒绝将它当作负载文件替换。

中断后已有事务目录时安装/卸载返回 14 并保留现场，不自动覆盖或清理可能仅剩一份的原文件；回滚本身失败返回 15，保留备份，需检查恢复后再启动。这是检测并保留恢复材料，不是已经实现断电后的自动恢复。文件部署回归并不能替代磁盘不足、断电及注册表写入故障验收。

`test-yueshot-installer-policy.ps1` 实际持有互斥文件句柄，覆盖新安装 EXE 锁、重装 EXE/DLL/卸载器锁、卸载资源锁、目录碰撞及残留事务拒绝，并比较所有文件字节和安装登记。
