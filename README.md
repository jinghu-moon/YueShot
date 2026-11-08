# YueShot

Windows x64 截图软件，使用 Rust、GPUI Base 与 Yue 组件库。当前处于开发阶段，尚未正式发布。
默认发行配置为 SDR；`avif`、`hdr-capture` 为显式开发 feature，实屏资格未完成。

## 开发

需要 Windows、Rust 1.98.1（含 rustfmt / Clippy）、微软官方 Visual Studio / Build Tools 的 C++ 桌面工具与 Windows SDK。
所有产品路径依赖均在此仓库中，`vendor/yue-ui` 为带哈希与来源记录的受控组件快照，不需要相邻组件仓库。
程序运行不需要 Node、Tauri 或 WebView2。Node 仅用于打包工具。
完整检查还需要 NASM 3.01 位于 PATH，用于现有可选 AVIF 的汇编构建；CI 从 NASM 官网下载固定版本并校验归档 SHA-256。

```powershell
cargo run -p yue-shot --locked
cargo run -p yue-shot --locked -- --smoke-test --data-dir D:\Temp\YueShot-Smoke
cargo run -p yue-shot --locked -- --help
```

F5 普通截图；F7 框选松手进入长截图并显示侧边预览，滚动后 Ctrl+S 打开保存面板；F9 不再用于长截图保存。
普通截图 Enter 复制并归档，Ctrl+S 打开保存面板；F6 使用 YueShot 自己的贴图来源。
同渠道/同数据根仅允许一个实例，重复启动转交命令。默认数据在 `%LOCALAPPDATA%\Yue\YueShot\<channel>`。

## 检查与交付

```powershell
npm ci --prefix tools/delivery
cargo install cargo-deny --version 0.20.2 --locked
cargo install cargo-audit --version 0.22.1 --locked
cargo xtask check
cargo xtask package --profile release --channel beta
```

源码归档需要本仓库的 Git 文件索引和 HEAD。
检查包含截图产品、公共库、图标/字体/许可/SVG 与依赖方向；AVIF/HDR 的静态回归不等于实机验收。
安装器为当前用户安装，渠道身份、目录、已安装程序验证见 [打包说明](apps/yue-shot/packaging/README.md)。
实际安装与升级脚本位于 `tools/test-yueshot-*.ps1`，开始前正常退出 YueShot；不会强杀产品进程。

候选包含程序、安装器、VC runtime、许可、对应源码、PDB 与精确构建标识。默认保留卸载后的用户数据。
目前在线认证更新、签名与恢复、完整分发权利、干净普通用户 Windows 和设备验收仍未完成。
`release --check` 应拒绝尚未具备资格的候选，未签名 stable 打包直接拒绝。
公开仓库的 Windows CI 会生成可下载的未签名候选 artifact，不创建 GitHub Release，也不代表正式发行资格。
生命周期验证状态及未完成项见 [交付状态](docs/lifecycle.md)。

## 工程边界

`apps/yue-shot/` 持有截图业务和私有 crate；`crates/yue-*` 是真实共享能力；`vendor/` 保存审计来源与受控补丁。
`tools/codec-bench` 和 `tools/icon-bake` 是截图验证/资产工具，不进入产品运行依赖。
此仓库不包含 SnapClip 应用或其私有 crate。首次导入的来源和验证范围见 [导入记录](docs/repository-import.md)。

产品代码按 [AGPL-3.0-only](LICENSE) 授权，第三方依赖与素材保留各自许可；最终分发权利仍须单独核验。
