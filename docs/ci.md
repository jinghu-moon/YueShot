# Windows CI 与交付验证

项目只面向 Windows x64，继续使用 windows-2022、Rust 1.98.1、NASM 3.01 和已审查的微软 VC143 Redist 合同。

## 触发方式

| 事件 | verify | delivery |
| --- | --- | --- |
| main 普通提交 / PR | 完整检查与安全审计 | 不运行 |
| 手动运行，delivery=false（默认） | 完整检查与安全审计 | 不运行 |
| 手动运行，delivery=true | 完整检查与安全审计 | 同一提交 verify 成功后运行 |
| yueshot-v* 标签 | 完整检查与安全审计 | 同一提交 verify 成功后运行 |

`verify` 保持原有 cargo xtask check 与产品安全审计，不删除测试、Clippy、AVIF/HDR 静态检查、工作区检查、依赖方向、来源/资产/许可校验或 npm 审计。`delivery` 保持原有 Release 打包、运行库/来源校验、安装/回滚/渠道/关闭到托盘回归、artifact 上传再下载字节核验。不创建 GitHub Release，不给未完成发行资格的候选冒充正式发布。

在 GitHub Actions → YueShot Windows → Run workflow，勾选 delivery 即可运行完整交付验证。CLI 等价入口：

```powershell
gh workflow run windows.yml --ref main -f delivery=true -f cold_build=false
```

完整的无编译缓存验证：

```powershell
gh workflow run windows.yml --ref main -f delivery=true -f cold_build=true
```

cold_build=true 跳过 Rust registry / target 缓存的恢复及写入，仍可复用固定版本的审计工具和 npm 下载缓存；每项审计、编译、测试与字节核验仍实际执行。它不是干净普通用户设备验收，也不自动证明可重现二进制。

## 缓存边界

复用官方 actions/cache restore/save，不新增第三方缓存服务。共同准备逻辑位于 `.github/actions/windows-setup/action.yml`，防止检查与交付任务的工具版本漂移。

- verify 缓存 Cargo registry、git 源和 target/debug；delivery 缓存相同来源目录和 target/release。两者按 profile 隔离，不缓存整个 target。
- key 包含 runner 的 ImageOS / ImageVersion、rustc -Vv、锁文件、根/产品/公共库/快照 Cargo.toml、组件快照来源哈希、rust-toolchain 与 Cargo 配置。编译缓存失效时，可在相同镜像/编译器/profile 前缀下恢复旧依赖，由 Cargo 指纹重新判断需要构建的内容。
- CARGO_INCREMENTAL=0 避免保存体积较大的增量状态；Release 的 opt-level、LTO、codegen-units、符号及运行行为不变。
- 只在 main 且整个任务成功后写入编译缓存；PR 和标签可读取允许范围内的缓存，不写入 main 的编译缓存。审计工具复用后仍核验精确版本；锁定依赖仍执行 fetch。
- 缓存从不替代测试或审计。不缓存 candidate、源码归档、delivery.json、安全审计结论、验收记录、上传下载验证结果，也不直接发布缓存内 EXE。packager 每次重建候选并验证当前源码 buildId；build.rs 追踪渠道和 buildId 环境变量。
- 缓存为不可变条目，同一 key 命中后不重复上传；依赖/配置变化产生新 key。首次建立缓存或 runner 镜像更新时仍须完整编译，不能承诺固定分钟数。

同一分支/PR 的新运行取消被取代的旧运行，其他分支和版本标签不受影响。verify / delivery 各设置 45 分钟超时，避免异常无限消耗 runner。

## 性能基线与验证

优化前，生命周期修复提交 661b878 的 Windows CI 42565942191 总耗时 26 分 18 秒（2026-11-09 05:12:29–05:38:47 UTC）。cargo xtask check 为 12 分 02 秒，Release 打包为 12 分 14 秒，安装回归 18 秒。该运行全部通过。

本次不更改产品代码或降低检查标准。workflow 通过 actionlint 1.7.11，workflow/action YAML 和全部 PowerShell 脚本解析通过，48 项 Python 工具回归通过。原有 `cargo xtask check` 脚本和交付验证步骤保持不变。

2026-11-09 对照实测（优化后两轮均为提交 b93e511）：

| 运行 | 范围 / 缓存 | 总耗时 | cargo xtask check | 结果 |
| --- | --- | --- | --- | --- |
| [42565942191](https://github.com/jinghu-moon/YueShot/actions/runs/42565942191) | 优化前，检查与交付串行 | 26 分 18 秒 | 12 分 02 秒 | 全部通过 |
| [42570052468](https://github.com/jinghu-moon/YueShot/actions/runs/42570052468) | 普通 push，首次无 Rust 编译缓存 | verify 15 分 01 秒 | 12 分 14 秒 | 检查/审计通过；delivery 按设计跳过；缓存保存成功 |
| [42571676485](https://github.com/jinghu-moon/YueShot/actions/runs/42571676485) | 同一提交，手动完整交付，debug 精确命中，release 首次无缓存 | verify 7 分 27 秒；delivery 14 分 01 秒；全程 21 分 30 秒 | 5 分 13 秒 | 检查、审计、完整交付全部通过 |

第二轮相较第一轮无编译缓存的 verify 缩短约 50%，计入环境准备和缓存下载/解压时间。debug 缓存压缩后约 1.63 GB，恢复约 80 秒；工作区及受控快照中的本地 crate 仍按 Cargo 指纹重新编译。每轮均实际执行 48 项 Python 测试、1,087 次 Rust 通过（含显式筛选重测）、105 次 ignored 出现及完整审计；ignored 不计作通过。此次为相同源码的单轮对照，不承诺后续源码变动、依赖变动或镜像更新后的固定耗时，也不将省略日常打包的收益归为缓存收益。

手动完整交付的 Release 打包为 11 分 42 秒，实际安装/回滚/渠道与原生关闭到托盘回归为 17 秒；artifact 上传、下载及候选逐项字节校验全部通过。上传与下载整包 SHA-256 均为 `3d12f25b019d0d860b995dc6993d9edb0357630e92312b8a717115007941971c`。安全审计为 0 blocking、0 maintenance notices，15 条产品图外记录保留。候选的 release readiness 仍为 false，没有创建 GitHub Release。

本轮还成功保存了独立的 release 编译缓存，压缩后约 0.83 GB；尚未实测 release 缓存恢复后的交付耗时，不据此承诺交付速度。现有证据只证明 debug 缓存恢复有效，以及首次建立 release 缓存时完整交付流程通过。
