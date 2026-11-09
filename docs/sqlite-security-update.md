# 0.1.4 SQLite 安全更新

2026-11-09。0.1.4 首次 Beta 打包被产品安全审计拦截；原失败候选与报告保留，没有生成可发布清单。

## 根因与处理

`rusqlite 0.32.1` 的 bundled 依赖 `libsqlite3-sys 0.30.1` 内含 SQLite 3.46.0，命中
[RUSTSEC-2026-0459](https://github.com/RustSec/advisory-db/blob/ba4d1245f77987d28e7a7f119ed21ad844c11f57/crates/libsqlite3-sys/RUSTSEC-2026-0459.md)。
公告描述：进程打开由较不可信方构造的数据库和同名 WAL 文件，且两者 page size 不一致时，查询可能返回未初始化缓存中的进程内存。
这是需要本地文件被构造/替换并被产品打开的向量；不将其写成远程网络攻击或已观察到的数据泄露。

产品入口与私有存储 crate 同时升级至上游 `rusqlite 0.41.1`，解析到 `libsqlite3-sys 0.39.1` / SQLite 3.54.1，
符合公告 `>=0.39.0` 修复范围。继续使用 bundled SQLite，不改用开发机系统 DLL，不引入本地源码补丁或忽略公告。
Rust 1.98.1 满足新依赖的最低编译器要求。数据库 schema 仍为 2，没有添加兼容层或迁移分支。

锁文件只接受该升级所需变化：三个库版本更新，新增的 WASM 专用锁定包保留在锁文件而不进入 Windows 产品图。
其他原有依赖的版本及 Windows 依赖边保留，已经由 Cargo `--locked` metadata/tree 验证。
默认 Windows normal/build 图从 405 包变为 403 包：旧 ahash/hashbrown 分支移除，已存在的 hashbrown 0.17.1 启用 default-hasher。

## 授权与原生材料

新 wrapper 仍为 MIT，SQLite 仍为公有领域声明；hashlink 仍为 MIT/Apache-2.0。
新 `.crate` 归档逐一与 Cargo.lock checksum 校验，SQLite 实际头部确认 3.54.1。
原始 SQLite 声明与旧版本逐字节相同，在新 sqlite3.c 中的偏移为 `[1601, 2027)`；
重新绑定完整源文件哈希及字节范围，没有自行编造声明或只保留 wrapper LICENSE。
新 native 源文件和 build.rs 哈希更新到 `tools/delivery/licenses/native-review.json`；Windows 产品不启用 SQLCipher、OpenSSL 或 WASM。
403 包许可清单已复核，原文缺项为 0。

## 验证与证据

- 修改前：`target/update-014-beta-package.log` 与失败候选 `target/yueshot-delivery/0.1.4-beta-lifecycle-fix/security-audit.json`
  显示 1 blocking，明确为该 SQLite 公告。源码提交 c12b3f9 的远程 [CI 42600499131](https://github.com/jinghu-moon/YueShot/actions/runs/42600499131) 亦在同一门禁失败，未放宽审计。
- 修改后：`python tools/yueshot_security.py --output target/sqlite-security-audit-fixed` 为 0 blocking、0 maintenance，
  15 条产品图外记录保留；公告数据库 commit 为 `ba4d1245f77987d28e7a7f119ed21ad844c11f57`。
- `cargo test -p yue-shot-storage --locked` 为 19 passed / 0 failed / 0 ignored，覆盖历史持久化/分页/删除、迁移/异常输入及像素原件。
- 另由已归档旧 Release EXE 创建独立数据根，确认数据库 writer version 为 SQLite 3.46.0、schema 2、完整性正常；
  保留设置/数据库哈希作为新 EXE 读回的基线。该实际 EXE 基线为空历史，不冒充真实截图历史升级验收。

升级后的完整 `cargo xtask check` 已通过：51 项 Python、1,097 次 Rust passed、109 次 ignored 出现，
包括严格 AVIF/HDR Clippy、workspace 与依赖边界；ignored 不计通过。日志为 `target/sqlite-security-full-check.log`。
修复版候选的构建与旧数据读回结果在候选验收中另记。测试使用独立目录，不打开或修改用户现有 Beta 数据。
本更新不证明所有未知原生漏洞已排除，不自动完成微软运行库分发条款或最低系统资格。

## 修复版 Release 候选与签署

干净源码提交 `25ab0c56ad25ec10970f37ce6b743c528ec7a018` 已构建
`target/yueshot-delivery/0.1.4-beta-lifecycle-sqlite-fixed`，buildId 为
`1c8fce7c9865f18f86977c2f92dbd5cb2863f28534c85ffb536dbf06cbc80b6d`。
Release 构建、来源/负载/运行库/许可/安全门禁及最终候选字节复核通过，`readyForRelease=false` 保留。
新 Release EXE 对旧 SQLite 3.46.0 EXE 创建的隔离空历史数据库 smoke 成功，设置与数据库字节哈希不变；
实际原生关闭窗口的驻留托盘和正常退出两分支通过。该证据不代替非空真实用户升级或干净普通用户资格。

用户已本地签署 0.1.4 Beta 清单。官方 Minisign 与客户端同一库验签成功；提取未改动的生产认证/版本判断代码执行探针，
接受 beta 0.1.3 → 0.1.4、拒绝篡改字节、渠道不符、过期及重复版本。签名绑定的安装器/EXE 哈希与候选实际文件一致。
公开结构化范围见 [验收记录](acceptance/2026-11-09-sqlite-security-update.json)，
本地日志见 `target/update-014-signature-qualification/`。目前未公开 0.1.4，现有公开 Beta 源仍为 0.1.3；
签署不等于发布，也不等于目标电脑真实安装升级已经通过。

该候选源码的 [Windows CI 42602126904](https://github.com/jinghu-moon/YueShot/actions/runs/42602126904)
已全部成功，含完整检查、安全审计和缓存收尾；普通 push 的 delivery 按设计跳过，本地实际 Release 打包证据另计。
