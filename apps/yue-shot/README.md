# YueShot

Windows x64 截图软件，使用 Rust、GPUI Base 与 Yue 组件库。当前版本 0.1.2，处于开发阶段，尚未正式发布。以下命令在仓库根目录执行。

## 开发与验证

需要 Windows、Rust 1.98.1（根 rust-toolchain.toml 固定）、微软官方 Visual Studio / Build Tools 的 C++ 桌面工具与 Windows SDK。Yue/GPUI 的受控源码位于 vendor/yue-ui，不依赖相邻的组件仓库。程序运行不需要 Node、Tauri 或 WebView2；Node 仅用于安装器工具。

```powershell
cargo run -p yue-shot --locked
cargo run -p yue-shot --locked -- --help
cargo run -p yue-shot --locked -- --product-info
cargo run -p yue-shot --locked -- --smoke-test --data-dir D:\Temp\YueShot-Smoke
```

默认 F5 普通截图，Enter 复制并归档，Ctrl+S 打开保存面板。F7 框选后松手进入长截图，显示侧边预览，滚动完成后 Ctrl+S 保存；F9 不再用于长截图保存。F6 消费 YueShot 自己的贴图来源，不依赖剪贴板管理软件。默认配置为 SDR；系统开启 HDR 不代表启用高精度捕获。`avif`、`hdr-capture` 是显式开发 feature，完整颜色和设备资格仍待验收。

同渠道/同数据根仅运行一个实例，重复启动转交命令；可用 `--show`、`--capture`、`--background`。默认数据根为 `%LOCALAPPDATA%\Yue\YueShot\<dev|beta|stable>`，`--data-dir` 接受绝对路径；同一根跨渠道也不允许多写。关闭主窗口是否保留托盘由设置控制，退出请使用托盘正常退出。

当前用户登录启动默认关闭，在设置中启用后使用当前 EXE 和数据根后台启动。移动程序后可更新启动项；开发测试结束请关闭自启。`--remove-startup` 清理指定实例的启动项，安装器的 `--uninstall-cleanup` 则按 EXE/渠道所有权清理多个可核实的数据根，不删除截图或配置。

## 检查与打包

```powershell
npm ci --prefix tools/delivery
cargo install cargo-deny --version 0.20.2 --locked
cargo install cargo-audit --version 0.22.1 --locked
cargo xtask check
cargo xtask package --profile release --channel beta
```

`check` 运行产品/公共库回归、格式、严格 Clippy、workspace 构建、字体/图标/SVG/许可/依赖边界检查；原多产品工作区同时验证 SnapClip 消费者。被 ignore 的桌面/GPU/性能用例不算通过，可用实机资格须另验。默认产品依赖按其真实 normal/build 图审计，不用 workspace 合并后的 features 代替产品合同。

打包依赖 Git 文件索引/HEAD，生成当前用户 NSIS 渠道候选，附 EXE、需要的官方 VC runtime、许可原文、对应源码、PDB 和源码清单构建标识。产物位于 `target/yueshot-delivery/<版本>-beta-release`，不自动上传或发布。stable 在签名/恢复流水线完成前直接拒绝；`cargo xtask release --check <候选目录>` 会拒绝当前未具备发行资格的候选。运行库审计不从 System32/PATH 猜测复制 DLL；可用 `YUESHOT_VC_REDIST_DIR` 指定官方 x64 CRT 目录。

实际安装验证前正常退出 YueShot，且不得有同渠道安装；脚本不强杀进程，试验数据保留在候选目录：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-yueshot-install.ps1 -Candidate <候选目录>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-yueshot-installer-policy.ps1 -Candidate <候选目录>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-yueshot-channel-install.ps1 -Candidate <候选目录>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-yueshot-upgrade.ps1 -From <旧版本候选> -To <新版本候选>
```

这些检查使用安装目录中的真实程序；升级需要两份真实递增版本候选。它们不替代干净普通用户 Windows、交互安装和截图设备验收。身份/目录/AUMID 与安装器说明见 [packaging/README.md](packaging/README.md)。在线认证更新、签名和密钥恢复、完整分发权利尚未完成。

## 离线备份、恢复与诊断

先正常退出对应实例。备份目录须在数据根外，恢复目标须为空且与备份记录的根一致；失败原数据应先保留再恢复，不盲目降级数据库：

```powershell
cargo run -p yue-shot --locked -- --backup-to D:\Backups\YueShot-Dev --data-dir "$env:LOCALAPPDATA\Yue\YueShot\dev"
cargo run -p yue-shot --locked -- --restore-from D:\Backups\YueShot-Dev --data-dir "$env:LOCALAPPDATA\Yue\YueShot\dev"
cargo run -p yue-shot --locked -- --export-diagnostics-to D:\Temp\yueshot-diagnostics.json --data-dir "$env:LOCALAPPDATA\Yue\YueShot\dev"
```

诊断目标父目录须存在，输出为数据根之外的新 JSON，不覆盖旧文件、不联网。扫描固定 4 个日志文件、每个至多 4 MiB，单条 8 KiB，保留最近 8,192 条，输出至多 4 MiB；拒绝超预算文件/reparse point。只导出白名单元数据，不复制原文、路径、截图、剪贴板、配置或内存转储。Rust panic 有同步标记；原生崩溃不由 panic hook 捕获，当前不自动生成 dump。

交付构建使用 SOURCE-MANIFEST.json 的 SHA-256 作为 buildId 关联 EXE/PDB/源码；普通开发构建为 null。该标识不是签名，也不承诺二进制可复现。默认卸载保留用户数据，自定义数据根不会被安装器递归删除。

## 所有权与授权

apps/yue-shot 持有截图业务、模型、捕获、覆盖层 UI 与存储私有 crate；crates/yue-* 是中立公共能力，产品之间没有私有 crate 依赖。组件快照的原始哈希、排除与本地适配保存在 vendor/yue-ui/snapshot.json。字体重建见 subfont/README.md。

产品代码为 AGPL-3.0-only；第三方依赖和素材保留各自许可，不能把清单收集或 cargo-deny 通过当作全部分发权利审查完成。
