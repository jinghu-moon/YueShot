# YueShot 默认构建依赖的许可证据

`sources.json` 固定发布包没有携带的许可证据。上游原文以发布包 VCS 提交和许可文件哈希核验；发行维护者整理的版权文件则明确标为 `downstream-curated`，同时锁定发行源码包 SHA-256，并逐文件对比对应 crates.io 包。标准 MIT/Apache 文本单独锚定 SPDX license-list-data 固定提交和 SHA-256。下游整理材料不伪称上游原件，也不单独构成授权兼容结论。

完整清单在 `../dependency-licenses.json`，当前范围是 Windows x64 默认 YueShot 的 normal/build 图。它由产品级 `cargo tree` 解析，避免 workspace `cargo metadata` 的 feature 合并混入其他产品和工具的可选依赖；metadata 仅提供包身份和来源路径。相同包的 host/target feature 采用并集，包含构建依赖，属于保守归档，不声称这些 crate 都会链接到最终 EXE。

```powershell
# 安装固定版本；CI 同样固定该版本
cargo install cargo-audit --version 0.22.1 --locked
cargo install cargo-deny --version 0.20.2 --locked

# 日常检查清单，发现未经审查的依赖/来源/feature/许可变化则失败
python tools/yueshot_licenses.py

# 检查默认产品依赖是否有可追溯的许可文本；其他发布审查仍独立执行
python tools/yueshot_licenses.py --strict

# Check the confirmed AGPL-3.0-only policy against the default Windows product graph
cargo deny --manifest-path apps/yue-shot/Cargo.toml --config tools/delivery/yueshot-deny.toml --target x86_64-pc-windows-msvc --exclude-dev check licenses

# 刷新 RustSec 后按相同产品图审查，保留 workspace 原始结果和产品范围报告
python tools/yueshot_security.py --output target/yueshot-security

# 清单变化必须先审查，不以更新哈希绕过失败
python tools/yueshot_licenses.py --update
```

源码校验依据 Cargo.lock 中完整 registry 身份和 checksum。许可文本从校验后的 `.crate` 压缩包读取，包含嵌套 native NOTICE/LICENSE；不信任可被修改的 registry 解包目录。原始 `.crate` 随候选的 `dependency-sources.zip` 保存，本地源码在 `corresponding-source.zip`，补充许可原文也随本地源码归档。`THIRD-PARTY-DEPENDENCY-NOTICES.txt` 装入程序目录，候选内的 JSON 记录原文来源、哈希、缺项和审查范围。

安全审查没有忽略公告。当前产品内 vulnerability/unsound/yanked 会阻止候选；停止维护公告保留为正式发布待审项；其他 workspace 包的结果单独列出并保留原始报告，不能借此声称整个 workspace 安全。审查依赖在线刷新 RustSec 和 crates.io 索引；网络或数据库元数据异常不能作为“通过”。

许可表达式及原文收集不等于完成授权兼容性审查，也不覆盖系统 DLL、微软再分发条款、应用自有图片、所有 native 子组件及未公开漏洞。`mac 0.1.1` 与 `seahash 4.1.0` 的 crates.io 包没有上游 LICENSE 文件；当前材料由 Debian/Ubuntu copyright 记录确认版权与许可表达式，并用发行源码包和 locked `.crate` 的文件比对确认其对应关系。报告会把该证据单独标注为下游整理材料。其余 SPDX 选项兼容、停止维护公告、native 依赖及微软条款审查仍是独立发布门槛。

GPUI Base 的 `instant` 引用已在受控 Yue UI snapshot adaptation 中迁到其既有 `web-time` 依赖，paste 消费者迁到 pastey。当前产品的 resvg/usvg 0.49 仅启用图形与 raster-images 支持，关闭 SVG text/system-fonts/memmap-fonts；不再加载字体数据库，fontdb 和对应内嵌 ttf-parser 子集已退出默认产品图。具体适配哈希和上游哈希见 `vendor/yue-ui/snapshot.json`。`tools/yueshot_svg.py` 检查产品 normal/build 图与 SVG 资产，含动态文字的 SVG 须在制作时转为路径；普通 UI 的 DirectWrite 字体路径不受影响。安全工具仍保留内嵌 parser 检测规则：若未来重新引入该依赖，审查项会恢复；本次不是忽略或豁免公告。

产品受控 Yue 快照已移除 6 个未使用的组件字体文件及其加载 feature/API；原始路径、哈希与排除原因保留在 snapshot.json，不推定这些排除字体的修改或分发权利。覆盖层的 Noto/Fira 合法来源与子集继续单独核对。对应源码归档从实际 workspace members 与受控 snapshot 清单收集文件，不能仅以 Git 未忽略文件为依据；候选核验应同时检查解压后的 snapshot、Cargo workspace 与逐文件哈希。独立组件仓库不受此源码裁剪影响。

2026-11-09 更新认证接入增加 `minisign-verify 0.3.0`（MIT，无传递依赖、无 build.rs），锁定发行归档 SHA-256 为 `871285dc19d8d0ebe0eef3d0e99a205f2a71363b122632cbbfa4a6c370a960ce`，Frank Denis 版权和 LICENSE 原文随包。SHA-256、URL、版本比较复用已锁定的 sha2 0.11 / url / semver；WinHTTP 属于系统 DLL，不随包复制。产品 normal/build 清单从 404 增至 405 包，既有包版本及来源保持不变，windows-sys 0.59 仅增加 WinHTTP feature。测试专用 blake2 / ring / base64 用来签署原生回归 fixture，不进入产品 normal/build 图。新清单经逐项对照后更新，原文仍为 0 缺项；这不代表生产更新密钥或完整线上升级资格已完成。
