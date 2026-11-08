# 57 · YueShot 默认发行配置授权核验

日期：2026-11-08。范围为 Windows x64、默认 SDR 配置。此文记录工程证据和剩余义务，
不把许可声明收集、静态导入检查或 cargo-deny 通过等同于完成所有发行资格。

| 项目 | 当前核验 | 证据与边界 |
| --- | --- | --- |
| 产品代码 | 用户确认 AGPL-3.0-only | 根 LICENSE；zenjpeg / zenwebp 选择 AGPL 分支，不选择商业 LicenseRef |
| 默认 Rust 图 | 404 包、许可原文材料 0 缺项，cargo-deny 通过 | 逐包发行源码 checksum、原文、下游证据分类和许可选择门禁；未据此声明 native 漏洞全覆盖 |
| BLAKE3 1.8.7 | 默认 x64 MSVC 路径有 SSE/AVX 汇编，选择 Apache-2.0 | 原始归档含 Apache/CC0/LLVM 多许可材料；build.rs 和全部 native 源文件哈希见 native-review.json |
| SQLite | libsqlite3-sys 0.30.1 的 MIT wrapper，bundled SQLite 3.46.0 为公有领域声明 | 当前不启用 SQLCipher/OpenSSL；现在同时提取 sqlite3.c 中逐字节的原始声明，而非只保留 wrapper LICENSE |
| VS 查找辅助 | vswhom-sys 0.1.3 为 MIT，C++ 原作者 Jonathan Blow 另有署名 | 原始 ext/vswhom.cpp 开头已加入随包 notices；此代码供构建主机查找 VS，不能误写为单独运行时 DLL |
| Windows import libraries | windows_x86_64_msvc 三个锁定版本按 MIT/Apache 原文提供 | 包含的是 import library，不是微软系统 DLL 的再分发权限；不从开发机复制系统 DLL |
| GPU API 声明 | GPUI driver-version FFI 代码注明 NVIDIA/AMD 的固定上游提交 | 补充对应头文件的 NVIDIA MIT 原文和 AMD MIT；以当前本地实现 hash 绑定审查范围，不声称本地实现来自同一个上游源码树；不随包部署 nvapi/AMD AGS DLL |
| 覆盖层字体 | Noto Sans SC Regular / Fira Code 数字子集，OFL | 源字体、衍生哈希、版权/OFL 原文与重建步骤受控 |
| 不用的组件字体 | 六个原文件与加载分支已移除 | 原始 SHA-256 与排除原因保留；源码分发不再包含它们，不推定其修改权利 |
| 控件图标 | Tabler MIT、Lucide/Feather ISC/MIT | 原文随包；用户此次提供的新应用 ICO 不按旧 Tabler 临时应用图标认领来源 |
| 应用 ICO | 用户指定 refer/yue-shot.ico，原样复制；2026-11-08 确认为自行创作、持有权利并允许公开源码/安装包分发 | 产品路径 apps/yue-shot/packaging/YueShot.ico；SHA-256 da28e6ec295182ffb0e5759fff15af6760fd85bec06d837afa888ef0ddee1323；记录用户权利声明，不冒充第三方许可或独立权利鉴定 |
| 微软运行库 | 用户确认使用微软官方 VS/Build Tools 并遵守其条款；本机 Community 17.14.36202.13 | 当前仅额外携带 vcruntime140.dll，来自 Redist 14.44.35112/x64/Microsoft.VC143.CRT，未修改且微软签名有效；资格确认与分发条款分开记录 |

原生审查材料位于 `tools/delivery/licenses/native-review.json`。它锁定审查过的版本、
features、构建脚本和 native 源码 hash，不宣称扫描出所有内嵌第三方代码。
源文件中的声明由 `tools/yueshot_licenses.py` 直接从 Cargo.lock checksum 相符的 `.crate`
读取；不能用 registry 解包目录中的可修改文件冒充原始材料。

微软条款原始来源：

- [Visual Studio Community 2022 EULA](https://visualstudio.microsoft.com/license-terms/vs2022-ga-community/)
- [原始协议文档](https://visualstudio.microsoft.com/wp-content/uploads/2021/11/Visual-Studio-2022-Community-License-EN.docx)
- [VS 2022 可再分发清单](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution)

原始 DOCX、文本抽取、官方清单与哈希见 `tools/delivery/licenses/microsoft/review.json`。
官方清单允许持有效许可者分发 VC/redist 下未修改文件，排除 debug_nonredist。
EULA 第 4 节同时要求相应的下游保护条款，并禁止使微软分发代码源码受 Excluded License 约束。
记录用户的合法构建环境确认，不等于豁免上述要求；微软 DLL 不改授权为 AGPL。
正式发行前仍需明确运行库的下游条款呈现/同意路径，以及 AGPL System Libraries 对该组合的适用。
本轮没有把整个 VS EULA 当作 YueShot 的产品许可强加给用户。

剩余发行边界：微软下游分发条款、最终资产权利范围、可选 AVIF/HDR 的独立闭包、
动态加载/驱动/最低 Windows 资格，以及干净普通用户机器验证。签名、在线更新真实性和
公开发布授权属于另外的生命周期门禁。本次不据此勾选完整 S6.3。
