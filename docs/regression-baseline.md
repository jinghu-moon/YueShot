# YueShot 生命周期回归基线

2026-11-09。用途是确定每次修改应运行的回归及证据范围，不是把历史通过自动转成新候选资格。

| 变更触及 | 必须复用的回归 | 证据要求 |
| --- | --- | --- |
| 产品/共享 Rust 代码 | `cargo xtask check` | 记录 commit 与日志；ignored 不算通过，静态 HDR/AVIF 不算实屏资格 |
| 工具与许可清单 | `python -m unittest discover -s tools/tests -p 'test_yueshot_*.py'` 及对应实际工具 | 不只测试 mock；候选需 `verify` 与来源校验 |
| 安装器/资源 | Release 打包，三个 `test-yueshot-{install,installer-policy,channel-install}.ps1` | 新隔离候选；安装负载/登记/快捷方式、锁/回滚、卸载数据保留 |
| 事务/提取路径 | 上述策略回归 + `test-yueshot-low-space.ps1`；下载暂存路径另加 `-UpdateDownload` | 实际 NSIS/WinHTTP、独立 VHD、真实磁盘满错误、失败不改旧文件/数据/登记、释放空间可重试 |
| 主窗口/退出 | `test-yueshot-window-close.py` 及 X/托盘人工回归 | 关闭到托盘保留进程可唤醒；明确退出返回 0；不得只测 Rust 状态 |
| 在线更新 | `cargo test -p yue-shot --features test-support --locked --lib update:: -- --test-threads=1` | 验签负面/中断/哈希/锁，独立父子进程退出/重启；fixture 与真实 NSIS 分开记 |
| 网络实现或发布源 | 显式 HTTPS 资格、真实客户端检查/下载/验签 | 不用单元测试冒充真实断网；区分“检查”与“下载”的故障 |
| 自启/托盘/恢复 | 登录、Explorer 重启、睡眠唤醒后的截图/保存 | 操作者明确报告；记录已知环境，不借用其他轮次元数据 |
| 运行库/工具链/最低系统 | 干净普通用户安装、实际已安装 EXE 与图形路径 | 开发机和 hosted runner 不替代无预装运行库机器 |
| 更新信任根 | 原客户端信任关系、备份恢复签署/验签，轮换专项 | 公钥文件存在不等于私钥可恢复；不能自动替换生产密钥 |

## 已建立的人工基线

- [普通用户 Build 19044 的 X/托盘定向复测](acceptance/2026-11-09-windows-19044-close-to-tray-retest.json)。
- [0.1.2 → 0.1.3 Beta 升级、重启、设置/截图保留](acceptance/2026-11-09-beta-online-upgrade.json)。
- [登录自启、Explorer 重启、睡眠恢复](acceptance/2026-11-09-beta-lifecycle-manual.json)。
- [独立介质密钥恢复与原公钥验签](acceptance/beta-key-recovery-20261109/result.json)。

原生安装故障、隔离 VHD 磁盘不足和相关回归的最新本地范围见 [失败验证](upgrade-failure-qualification.md)。

## 结果记录规则

每次记录测试命令、实际候选版本/buildId/安装器哈希、环境、成功/失败与未覆盖项。
只将执行过的断言记为通过；失败的原始报告保留，修复后用新输出目录重测。
人工确认独立存放，不改写自动脚本的 `manualQualification=pending`。

真实断网、断电、注册表写失败、重复恢复循环和资源稳定性仍需独立资格，不能因为上表已有相关测试名称而视作完成。
性能结论必须有前后指标；本轮工程收尾不作没有测量依据的性能承诺。

## 视觉候选实施 P0（2026-11-16，基线阶段通过）

从 `2b88e591ca97c980ed976a4d2a7b64c713c1e335` 开始；此次只增补测试与证据，不改变产品行为。完整记录及B01～B13缺口见[本轮执行记录](acceptance/visual-candidate-p0/README.md)。

| 范围 | 改动前 | 增补后实际执行 |
| --- | --- | --- |
| capture lib | 560通过／88 ignored | 571通过／90 ignored |
| media／geometry | 8／10通过 | 8／10通过 |
| 新增特征化 | 不存在 | 常规11项、真实图形控制器1项均执行通过；另1项生命周期资源探针通过。含旧缺陷特征化，不表示缺陷已修复 |
| 定向原生／GPU | 本批次尚未运行 | 9个指定测试分别匹配1项并通过，包含上行图形控制器；原始日志持久保存，不重复相加 |
| Demo合同自检 | 历史研究结果另记 | 23/23通过，60顶层合同／99变体／119资产哈希；算法`not_run` |
| 固定HTML图集 | 仅页面生成／导出合同 | 119张PNG逐张独立解码与RGBA哈希通过；素材完整性反例6/6通过，全部仅作开发／回归输入 |
| 实捕与空闲基线 | 本批次尚未采集 | 两批各5次暖机＋100会话／60s空闲通过生命周期断言；最新4K捕获P95 39.952ms，隐藏控制器测量，非可见UI消息泵资格 |
| 工程门禁 | 本批次尚未运行 | 本轮`cargo xtask check`退出0，见`engineering-check-stage-gate.*`；早期失败与成功日志均保留 |

已复现K01同owner确认矩形竞态、K02祖先变化后的手动选层漂移、K03 Shift清理短路。已补捕获重启后12组合排列与controller跨worker的120种到达顺序、100会话与60s空闲隐藏控制器基线、固定HTML开发图集；真实宿主COM／完整操作链、独立真实验收图集划分仍有缺口；P0基线门通过、P1未开始，不宣称已达到视觉功能或精度验收。

复跑入口为 `pwsh -NoProfile -File tools/test-visual-candidate-baseline.ps1 -EvidenceDirectory <新证据目录> -Native`；脚本拒绝覆盖既有证据，每个指定原生测试须准确匹配1项。版本、命令、计数、退出码和原始日志哈希写入该目录的summary.json。

## 视觉候选实施 P1（2026-11-16，通过）

从`ad07e2c`开始原位统一目标，删除WindowTarget／TargetKind与无生产者ClientArea，无兼容别名。实际选区、owner、来源与冻结源身份分开表达；不接视觉，不夹带K01～K03修复。捕获库573通过／91 ignored，media／geometry为8／10通过；selection_target_replay含真实隐藏图形控制器3/3通过；新增3项真实窗口枚举／epoch／悬停资格各1/1，既有9项定向资格全部重测通过。P0规范化golden字节不变，原生trace保持；`cargo xtask check`退出0。

[阶段记录](acceptance/visual-candidate-p1/README.md)、[门结果／源码与日志完整性](acceptance/visual-candidate-p1/stage-gate.json)、[完整定向批次](acceptance/visual-candidate-p1/complete-run/summary.json)已持久保存。真实宿主COM／完整操作链等后续资格仍按P0清单，未测不计通过。P2未开始。

## 视觉候选设计研究快照（2026-11-16）

源码 HEAD：`7938581`。以下是设计初稿研究时实际执行、在评审修订时重新核对原始日志的计数摘要，**不是视觉算法已实现、P0 基线已补齐或完整升级已通过**。本次评审修订没有重跑代码测试。

| 范围 | 通过 | 失败 | ignored |
| --- | --- | --- | --- |
| capture：window_detection | 113 | 0 | 0 |
| capture：refinement_worker | 6 | 0 | 0 |
| capture：overlay | 31 | 0 | 1 |
| capture：capture_worker | 5 | 0 | 1 |
| capture：providers | 14 | 0 | 2 |
| capture：session / artifact / sampler / geometry | 24 / 8 / 16 / 51 | 0 | 0 |
| yue-media / yue-geometry | 8 / 10 | 0 | 0 |
| 本项目合计 | 286 | 0 | 4 |
| mark-shot 区域模块临时独立 harness（另计） | 5 | 0 | 0 |

命令、参考版本哈希及证据边界见[设计文档附录 A](visual-candidate-pipeline.md#附录-a初稿研究执行记录)。原始日志仍位于临时目录，摘要持久化不等于原始日志已归档；ignored、完整 `cargo xtask check`、新 golden、新算法和正式性能对照未在该轮执行。实施时须将新批次的原始日志、环境、退出码及未覆盖项持久保存到 `docs/acceptance/`，不能借用本摘要作为新候选资格。

## 视觉候选实施 P2（2026-11-16，开发阶段通过）

从`3990eae`开始，算法与图形探针仅`cfg(test)`，产品仍为Window／Accessibility两来源。HTML4.1的119张PNG与v4字节相同，另固定8K小目标图。A/B同集191查询（136适用硬查询）：A全部正确且Complete；B几何全部正确但2项预算溢出，选择A＋CPU，不保留B产品路线。22项视觉测试含4项显式ignored探针均实际执行通过；137条按需读取与完整源一致，追加最大541888字节。

最后回归：capture591／95 ignored，media8、geometry10、native-baseline11、selection-target3、真实窗口3以及既有9项原生／GPU资格全部实跑通过；ignored未执行者不算通过。P0 golden字节不变。`cargo xtask check`退出0。源位不变、数值／定位／采样／容量／等变合同均有独立结果；[记录与覆盖表](acceptance/visual-candidate-p2/README.md)、[阶段门](acceptance/visual-candidate-p2/stage-gate.json)。

真实按需GPU小Map约153～302ms，性能失败；CPU计算P95最高约2.478ms，8K ROI同步读回P95约1.737ms。因此进入P3有界批量异步传输，不引入计算shader；P3/P4/P5尚未完成，不能称局部25ms／UI1ms或真实宿主统计资格已通过。原生K01～K03仍按基线复现，未在算法阶段夹带修复。

## 视觉候选实施 P3（2026-11-17，本机原型资格）

有界批量读回与分阶段CPU查询仍仅`cfg(test)`。当前20项视觉常规测试、137条渐进读取与完整源等价；与P2冻结输出的全部行为字段一致，仅实际耗时不作相等要求。最终完整显示器3840×2160 D2D/DComp/Present竞争下，四场景×1000暖查询×三会话全部正确：查询P95最高19.156ms，owner事件P95最高0.231ms；实际连续显示间隔P95增量约−0.1µs。105次offscreen D2D／取色交错、格式／容量／取消／Map错误分支、105次冷staging／Query与60s静止资源平台通过；不等于产品冷启动或完整会话E15已通过。CPU缓冲保守容量上界最高15096688 bytes，显式GPU atlas 4MiB；产品分配守卫仍在P4。

解锁后完整回归capture593／99 ignored、media8、geometry10、native-baseline11、selection-target3、真实窗口3及9项原生／GPU定向测试全部通过。P0 golden未改，工程检查原始命令／退出码见final-engineering-check.json。此前Screen-saver桌面中的三项错误5及OCCLUDED失败记录保留，未把CPU提交间隔冒充显示间隔。本机单卡资格不外推其他厂商／HDR，P4完整产品链和P5独立统计尚未实施。原始失败及通过证据见[P3记录](acceptance/visual-candidate-p3/README.md)、[覆盖审核](acceptance/visual-candidate-p3/coverage-audit.md)与[阶段状态](acceptance/visual-candidate-p3/stage-gate.json)。

## 视觉候选实施 P4（2026-11-17，进行中）

当前产品接入A／CPU单线程与4MiB有界异步atlas读回，新增Visual来源；B仍仅测试。K01请求时矩形冻结、K02显式手动几何保持、K03 Shift分别清理已修改，对应旧日志不改。涉及Visual的不同几何保留1px滚轮层，纯原生跳过规则保持。详细源码、理由、前后合同和未过门项目见[P4记录](acceptance/visual-candidate-p4/README.md)。

当前region合同后的稳定源码批次：`coverage-qualified-run8`33常规／31 ignored、29产品、1故障全过，四份P2行为报告精确相等；`coverage-final-regression`606 capture／122 ignored、media8、geometry10、native-baseline11、target3、真实窗口3及9项原生/GPU门全过；`coverage-final-engineering`cargo xtask check退出0；`coverage-final-idle`两个各60秒子合同全过；均有源码哈希／退出码及未变检查。Q05真实GPU权限三变体已补齐；T13含24排列和7不安全刷新拒绝。详见[P4记录](acceptance/visual-candidate-p4/README.md)、[覆盖审核](acceptance/visual-candidate-p4/coverage-audit.md)。

修改region合同前的真实105轮配对／4K四场景三会话成功测量保留：冷源P95=10.958ms、暖路径最高4.3853ms，静止末20秒资源范围0；它们不认证后续region合同源码。最新实屏重测第一项GetCursorPos错误5失败，原始记录保存且runner停止。可见F5/F7/保存/贴图因native管道缺失未执行；同PID同类HWND复用没有创建token保证；完整交互时延、物理恢复与扩展硬件仍缺。P4未获全部资格、按要求未提交为完成阶段，P5未开始。[阶段门](acceptance/visual-candidate-p4/stage-gate.json)区分当前与历史证据。


### P4开发门收尾／真机责任调整（2026-11-17）

用户明确真机测试留待后续自行检测，因此自动化通过可提交推进；前述“未提交、不推进”是该指示前事实，不再是当前执行策略。E08新增provider私有窗口属性生命周期标签；零token不发视觉，旧token拒绝，未改名借用epoch。5项隐藏Win32 API测试、9维迟到键、T13的24种到达排列及8种不安全刷新均有覆盖。

最终批次`lifetime-qualified`为33视觉常规／31 ignored、29产品、5生命周期、1传输故障通过；`lifetime-regression`为capture611／122 ignored、media8／geometry10／native-baseline11；另3项selection-target回放通过。两项各60秒idle、cargo xtask check和新exe构建通过；当前源码／日志／哈希见[P4阶段记录](acceptance/visual-candidate-p4/stage-gate.json)。四份P2行为报告精确等价。真实全操作、当前源码完整实屏性能、扩展硬件和P5独立真实统计仍待用户，不称最终升级成功。


## 视觉候选实施 P5（2026-11-17，软件审核完成／用户验收待执行）

P4产品提交`531bc91`已推送。P5不改Rust／算法／阈值，新增离线证据审核器，实际核对100个当前源码／锁文件／可执行输入、suite退出码／原始计数／SHA256、3项统一目标回放、两项各60秒idle、工程／exe、三套图集独立解码及四份P2行为等价。66项工具回归含6项审核器反例；另6项图集反例通过。当前软件结果可复用的依据是源码身份一致，而不是借用历史性能批次。

完整[交付记录](acceptance/visual-candidate-p5/README.md)与[阶段门](acceptance/visual-candidate-p5/stage-gate.json)区分开发完成和用户待验收。真实F5/F7／保存／贴图、当前源码实屏性能与全交互时延、物理恢复／硬件资格、独立真实宿主统计仍未通过；用户已明确后续自行检测。不把HTML图集当独立真实样本，不声称最终升级成功。
