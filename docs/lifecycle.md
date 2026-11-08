# 生命周期与交付状态

2026-11-08。开发版本 0.1.2，Windows x64，默认 SDR。产品源码已从 SnapClip 工作区独立，当前阶段允许完善生命周期机制，禁止新增截图功能。

## 已有验证

- 独立副本完整 cargo xtask check 通过，包含产品与公共库回归、格式、严格 Clippy、workspace、AVIF/HDR 静态检查、许可、SVG 和依赖边界。首次导入本地检查为 1,087 次 Rust 通过（含显式筛选重测）、105 次 ignored 出现、43 项 Python 通过；40e98b7 干净远程检查为同样 1,087 次 Rust 通过及 45 项 Python 通过，ignored 不算通过。
- 默认产品 normal/build 图 404 个包与拆分前版本、来源、许可、features 一致，所有本地依赖位于仓库内。原工作区仍验证 SnapClip 消费者，共享能力不依赖产品私有 crate。
- 本机实际安装目录的 0.1.1→0.1.2 升级保留自定义设置、真实历史记录和原始 PNG；降级拒绝、未来数据库版本拒写、恢复 A 版备份、卸载保留数据和重装读回通过。两版本 schema 相同，不声称验证了结构迁移。
- 当前用户渠道身份、安装登记、快捷方式目标/参数/AUMID、运行中拒绝安装/卸载且不强杀、损坏版本登记拒绝、同版本重装均有自动门禁。
- 字体/图标、对应源码、许可原文、VC runtime 静态导入闭包、构建标识与符号材料纳入候选流程。完整权利审查见 [分发权利](distribution-rights.md)。

## 仍待完成

首次公开 CI 已发现并修复干净环境的问题：依赖须在离线审计前按锁文件下载；Windows 长短路径统一 canonical 根；path-patched codec 的原始发行归档纳入 checksum 控制；程序解析的 cargo tree 强制关闭终端颜色。原始失败均保留，不通过更新清单或跳过检查掩盖。审计工具缓存只保存版本复验的可执行文件，不缓存检查结论。

干净 runner 还暴露出 GDI 内存字体不属于 DirectWrite 系统字体集合的问题。原生文字改为使用显式的两份嵌入字体集合，factory/loader 与静态字体具有进程生命周期，加载错误传播；GDI 控件保留私有注册。完整生产字符串通过 cmap 和 DirectWrite face 双重缺字检查，排除系统已安装完整字体掩盖错误。字体文件、字重、字号和排版规格未改；最低系统对所需 DirectWrite 接口的资格仍须独立确认。

认证在线更新及优雅退出/重启；签名和密钥备份/轮换/丢失恢复；完整分发权利与最低系统/动态加载资格；干净普通用户 Windows；真实登录自启、Explorer 重启、多屏/DPI/睡眠恢复、原生崩溃与资源稳定性；中途断电/磁盘不足等升级故障。当前本机验证来自管理员会话，不能替代普通用户验收。

本机 Windows Sandbox 未启用，未更改系统功能或重启系统。ARM64 缺少设备，单独排期，不阻塞 x64 流程。

## 已通过的公开 CI

提交 40e98b7 的 [Windows CI 42488706977](https://github.com/jinghu-moon/YueShot/actions/runs/42488706977) 于 2026-11-08 完整成功：干净 checkout → 完整 check（含 AVIF/HDR 严格静态检查）→ 产品安全审计 → Release beta 打包 → 三项实际安装验证 → artifact 上传 → 下载 → 整包严格校验。1,087 次 Rust 通过、105 ignored 不算通过、45 项 Python 通过；安全报告 0 blocking、0 review，15 条产品图外记录保留。安装验证覆盖已安装 EXE、负载哈希、数据保留、运行中保护、渠道登记、快捷方式和 AUMID。

21a59fa 曾在下载复验时缺少 3 个 NSIS 隐藏文件；40e98b7 明确保留该候选范围的隐藏文件并增加回读门禁，不删除清单项来掩盖问题。新 artifact 上传/下载的整体 SHA-256 一致，23 项清单文件全部校验通过。本地另读取清单、诊断和这 3 个文件，哈希相符；构建标识与本地重算的 7,553 文件源码清单一致。本地补充检查没有重复下载整份新候选，整包回读由 CI 完整执行。

候选为 [artifact 13621258927](https://github.com/jinghu-moon/YueShot/actions/runs/42488706977/artifacts/13621258927)，保留 14 天；未签名、readyForRelease=false，未创建正式 Release。源码 buildId 为 88f92499f3fd4e3b42e92261c8cba1f3b2fa6414b54a85e02b4edfe204ef99a2；归档 SHA-256 为 0546cabf61cff0147472abfb33635ade5b7ef89ebd2a752869515bdd525a527c。这些校验不替代更新真实性或正式发行资格。

## CI 与发行边界

main 推送/PR 触发 Windows CI，只生成未签名 beta 候选和实际安装验证，公开仓库中的 Actions artifact 可被下载。它不是 GitHub Release，也不代表产品已经正式发布。

构建镜像固定 windows-2022，与当前审查的 VS 2022 / VC143 Redist 合同一致；编译前验证官方 x64 Redist 目录。windows-latest 曾漂移到 VS 2026，其 Release 编译虽通过，却不符合该打包合同。更新编译器/运行库须一起核对工具、打包来源和权利材料，不从开发机 System32 猜测补 DLL。

windows-2022 还需显式安装 NASM：缺失时现有 AVIF 严格 Clippy 在 zenrav1e 汇编构建失败。CI 固定官网 nasm-3.01-win64.zip，使用实际下载计算并记录的 SHA-256 校验归档，再确认版本并加入后续步骤 PATH；不关闭汇编或跳过 AVIF 门禁。此工具仅用于构建，不进入产品安装负载。

正式 release --check 必须在未满足更新、权利、签名/恢复、干净用户等门槛时拒绝；未签名 stable 打包直接拒绝。不通过忽略公告、放宽断言或强杀进程制造验收通过，不将实验 HDR 自动放入默认发行配置。
