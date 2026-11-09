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

本次不更改产品代码或降低检查标准。验证分为 workflow/action 语法和 PowerShell 解析、本地工具回归、远程普通提交确认 delivery 跳过、再次运行确认缓存恢复，以及手动完整交付验证。优化后的实际耗时和缓存命中需以运行结果补记，不能把去掉普通提交的打包时间直接当成编译缓存收益。
