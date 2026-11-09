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
0.1.2 → 0.1.3 beta 在线升级、自动重启、设置和截图保留已获用户实测确认；beta 密钥独立 USB 备份、恢复、实际签署/验签及拔插后复读通过，轮换、完整分发权利及剩余环境/设备验收仍待完成。
用户选择不做 Windows Authenticode 代码签名，所有渠道均允许生成未签名候选；`release --check` 仍拒绝尚未满足其他发行验收的候选。更新包真实性签名独立于代码签名。
普通 push / PR 的 Windows CI 执行完整检查与安全审计，不再每次构建安装包。手动运行时勾选 `delivery`，或推送 `yueshot-v*` 标签，才在同一提交检查通过后执行完整 Release 构建、安装回归及 artifact 上传/下载校验；不创建 GitHub Release，也不代表正式发行资格。
Rust 编译缓存按 runner 镜像、编译器、debug/release、锁文件及配置区分，不缓存交付物或检查结论；手动勾选 `cold_build` 可绕过编译缓存，保留干净构建验证入口。详见 [CI 说明](docs/ci.md)。
生命周期验证状态及未完成项见 [交付状态](docs/lifecycle.md)。
用户已确认登录自启、Explorer 重启后托盘恢复和睡眠唤醒后的截图/保存正常，范围见 [人工验收](docs/acceptance/2026-11-09-beta-lifecycle-manual.json)；重复循环与资源稳定性另计。
设置页已接入按需检查、认证下载与退出安装更新；[0.1.3 beta](https://github.com/jinghu-moon/YueShot/releases/tag/yueshot-v0.1.3) 及签名更新源已公开，实际客户端检查/下载/验签通过，用户已确认升级重启与设置/截图保留；dev/stable 尚未配置公钥，会明确拒绝更新。协议、发布者命令及验收边界见 [在线更新](docs/updates.md)。

## 工程边界

`apps/yue-shot/` 持有截图业务和私有 crate；`crates/yue-*` 是真实共享能力；`vendor/` 保存审计来源与受控补丁。
`tools/codec-bench` 和 `tools/icon-bake` 是截图验证/资产工具，不进入产品运行依赖。
此仓库不包含 SnapClip 应用或其私有 crate。首次导入的来源和验证范围见 [导入记录](docs/repository-import.md)。

产品代码按 [AGPL-3.0-only](LICENSE) 授权，第三方依赖与素材保留各自许可；最终分发权利仍须单独核验。
