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

dev/beta 候选允许明确未签名；stable 目前直接拒绝，等待正式签名及恢复流水线，不能降级为 unsigned stable。
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

此记录确认用户指定的产品素材和内容一致性，不推定第三方许可或作者身份。
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
