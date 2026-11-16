# P3 有界批量传输

起点为已推送的 P2 提交 `36e62ba`。P3 仍为 `cfg(test)` 原型，产品没有视觉来源。用户解锁后，2026-11-17 01:46 的只读探针确认输入桌面恢复 Default，随后完整显示器资格与既有回归全通过。以 [stage-gate.json](stage-gate.json) 为当前阶段状态，最终覆盖见[审核表](coverage-audit.md)。

## 实现与必要性

| 问题／事实 | 处理、实体及去掉它的后果 |
| --- | --- |
| P2逐4×4同步Map单查询153～302ms | BatchReadback／Batch／Piece合并需求区域并打包到单个1024×1024 BGRA staging，一批≤1MiB、单在途、一次Flush、Map(DO_NOT_WAIT)；去掉会恢复数千次小Map，或失去取消后的资源占用管理 |
| 整边预取超追加1MiB预算 | 未读像素显式未知；沿边以256起步几何倍增，找到真实断边即停。去掉会把缺像素当无边／伪角点或突破预算 |
| 每轮从头计算浪费CPU | Query／Stage／Step按寻峰、角点提案、验证继续，沿用同一A判定；去掉会重复全算法或发布部分结果 |
| 小块哈希／重复request开销 | 有界u16直接索引和紧排块，8K索引约3.96MiB，索引>8MiB明确BudgetExceeded；去掉会恢复昂贵查找和碎片读回，不新增全图像素副本 |
| 四邻域循环处理轴向采样 | 像素中心／单轴快速分支保持原权重和加法次序；几何、逐边证据、状态和判定与P2冻结结果一致，未放宽阈值或采样数 |
| Sleep(1ms)实际等待数毫秒 | 单CPU worker用高分辨率waitable timer，在GPU仍在途时一次性延迟250µs通知owner；无在途不运行，不改全局timer resolution；去掉会忙轮询或同步等待 |
| DXGI事件／D3D fence早于Map可读 | 失败日志保留，未胜出代码删除；Map为真值，无事件／fence产品后备路径 |
| 基线忙等、分析侧超时等待不公平 | 测试统一原生消息／帧计时等待，等待CPU时继续泵消息，64copy切片间渲染先行；PresentProbe用于实屏证据，不进产品 |
| 进程总句柄变化不能归因给Query | 新增构造／Drop配平和句柄类型诊断，去掉无法区分自身资源存活与运行时背景变化；只在测试使用 |
| 测试匹配数、源码及预算易漂移 | P3脚本冻结命令、计数、源码哈希、日志、逐会话门槛；去掉无法重放审计 |

源码位于capture crate的 `src/visual_detection/{mod,proposals,progressive}.rs` 和 `src/windows/win/d3d11/visual_transfer.rs`。4px块、1024atlas、64copy切片、16轮、65536request、256扩张和250µs间隔已经本机原型资格锁定，产品接入后须重测完整链路。不同历史源码的日志不能借用为当前通过；未胜出路径不进产品。

## 历史实验

`experiment-history.json`及同名日志保留整边预取、两像素块、分阶段读取、同步轮询、事件／fence等实验，不代表当前源码通过。

| 批次 | 结果及限制 |
| --- | --- |
| idle-1000x3 | 几何／取消通过，T21三会话P95=30.509／31.956／31.940ms，未达25ms |
| validation-vector-1000x3 | 预取与扫描halo改进后的隔离实验，不是UI／负载资格 |
| interleave | 105次真实offscreen D2D／取色交错、源不变；不是显示间隔门 |
| native-pump-1000x3 | 20项视觉及137条渐进回放通过；T21第二会话P95=27.536ms失败；提交间隔差39µs，但显示基线无有效统计 |
| displayed-baseline-failed | 2628次Present均为0x087A0001（DXGI_STATUS_OCCLUDED），连续有效显示间隔0；JSON／原始日志保留 |
| isolated-fastpath-1000x3 | 空环境值被旧var_os().is_some()误作开启Present，屏保中基线失败，未完成千次查询；已修正为值必须等于1并修正脚本环境清理 |

## 资源断言修正

事实：`resource-failed.txt`、`resource-control-failed.txt`及 `native-pump-1000x3/resource.txt`保留旧断言失败；`resource-types.json/txt`也保留一次旧断言通过的运行，不能只选后者。

固定失败批次中，无分析60秒对照句柄371→374，类型为Timer+2、Semaphore+2、Thread+1、Event+1、ALPC Port−3。105次冷staging／Query释放后的句柄数全部一致；随后静止期378→380，仅Timer和Semaphore各+1，18秒后稳定，最后20秒private bytes恒定343445504。

推断：进程总句柄首尾相等会误归因，不能充当分析资源泄漏的充分判据。运行时／驱动内部创建栈未定位，不声称证明某厂商有泄漏或无泄漏。

修正保留逐会话释放后的句柄精确相等，并新增Query／BatchReadback构造及Drop真实计数，释放后存活数必须为0。静止期保留全部61个样本；末20秒句柄类型一致、GPU占用不增长，private bytes噪声上限取同进程无分析对照范围，不设任意MiB容差。该观察窗口不能证明无限稳定，也不是完整产品worker／退出／显示器切换的E15。

## 先前源码的隔离结果

[isolated-fastpath-final-1000x3/summary.json](isolated-fastpath-final-1000x3/summary.json)记录20项视觉常规测试、137条渐进读取等价、四场景×1000暖查询×三会话、105次D2D／取色交错、105次冷staging／Query和60秒资源静止，所有suite实跑通过。

该批次没有开启Present，脚本整体退出1、prototypeChecksPassed=false为正确结果，表示显示／负载资格缺失，不能记为阶段完成。[P2行为对照](isolated-fastpath-final-1000x3/p2-behavior-equivalence.json)核对四份冻结报告，所有行为字段精确一致，仅两项实际耗时不作相等要求。[性能摘要](isolated-fastpath-final-1000x3/performance-summary.json)保留P50／P95／P99、最大值及带IID假设的P99秩区间。

| 场景 | 三会话查询P95（ms） | 三会话owner事件P95（ms） |
| --- | --- | --- |
| T01 | 1.658 / 2.568 / 2.393 | 0.063 / 0.064 / 0.073 |
| T21 | 19.255 / 20.095 / 19.844 | 0.171 / 0.167 / 0.168 |
| 4K T10 | 12.507 / 12.628 / 12.219 | 0.118 / 0.123 / 0.110 |
| 8K T10 | 20.211 / 20.341 / 19.348 | 0.141 / 0.142 / 0.146 |

## 桌面状态与完整回归

[desktop-observation.json](desktop-observation.json)记录先前执行桌面为WinSta0\Default、输入桌面为Screen-saver，这解释了Present的OCCLUDED和GetCursorPos错误5。未修改锁屏／屏保策略，不以提交时间代替显示间隔。用户解锁后，[新探针](unlocked-desktop-observation.json)已确认输入桌面恢复Default。

`final-regression-run/summary.json`记录capture593／98 ignored、media8、geometry10、native-baseline11、selection-target3和三个真实窗口回放通过；定向原生／GPU九项中六项通过，以下三项在屏保中失败，需新目录重跑：

- providers_probe_and_capture_a_real_monitor_when_available
- a_300x200_selection_reads_back_only_its_own_bytes
- native_f5_worker_honors_capture_build_policy

`engineering-check.json/txt`记录cargo xtask check实跑退出0、日志哈希和源码核对，P0 golden未改；失败日志不删，测试不弱化。

## 最终完整显示器资格

```powershell
./tools/test-visual-candidate-p3.ps1 -EvidenceDirectory docs/acceptance/visual-candidate-p3/full-monitor-qualification-run -Samples 1000 -Sessions 3 -Present
./tools/test-visual-candidate-baseline.ps1 -EvidenceDirectory docs/acceptance/visual-candidate-p3/unlocked-regression-run -Native -SelectionTarget
```

[full-monitor-qualification-run](full-monitor-qualification-run/summary.json)所有suite及脚本整体退出0，prototypeChecksPassed=true。测试使用3840×2160完整显示器、真实D2D/DComp/Present与消息／帧等待，基线至少1000个连续有效显示间隔；实际呈现间隔P95相对基线增量约−0.1µs。不是以CPU提交间隔替代显示间隔。执行环境与全部原始统计一并保留；平衡电源方案是运行后采样，未声称全程锁定。

| 场景 | 三会话查询P95（ms） | 三会话owner事件P95（ms） |
| --- | --- | --- |
| T01 | 2.536 / 2.356 / 2.229 | 0.092 / 0.099 / 0.088 |
| T21 | 17.740 / 19.156 / 17.557 | 0.190 / 0.200 / 0.188 |
| 4K T10 | 11.551 / 11.408 / 12.095 | 0.142 / 0.138 / 0.164 |
| 8K T10 | 16.897 / 16.794 / 16.929 | 0.231 / 0.227 / 0.228 |

12,000次暖查询全部几何正确、Complete且源不变。137条渐进回放与完整源相同；P2四报告行为字段全等。105次D2D／取色交错及格式／容量／取消／Map错误分支通过。保守CPU缓冲核算最高15096688 bytes（约14.40MiB），显式GPU atlas 4MiB；进程／驱动实际峰值另保留于resource.json。该核算不是所有实际分配点的产品预算守卫。

`unlocked-qualification-run`在800×450测试面上仍出现静止末尾Event／IoCompletion增长，原日志保留，不算通过。最终批次控制采样前用既有read_back_region_bgra预热5次，使无分析对照同样经历首次驱动读回初始化；未更改末20秒严格平台断言。105次staging／Query冷生命周期通过，正式100次P95=6.213ms；释放后自身存活计数归零，静止61个样本完整保留，末20秒平台通过。完整产品worker、捕获会话与退出仍须P4/P5重测。

[unlocked-regression-run/summary.json](unlocked-regression-run/summary.json)记录capture593／99 ignored、media8、geometry10、native-baseline11、selection-target3、三个真实窗口和九项原生／GPU定向测试全部通过，解锁前的三项错误5失败没有删除。最终工程检查见final-engineering-check.json/txt；golden未改。ignored未执行者不计通过。

本机RTX 4070 Ti SUPER结果不外推其他厂商、iGPU、HDR、物理8K屏或GPU满载。本轮选定有界异步读回＋CPU，不新增计算shader；P4接入后重新验完整延迟／归属／融合及E08～E15，P5独立真实统计资格仍未取得。

本文件修复了上一轮shell转义导致的排版／控制字符损坏，原始测试日志及哈希未变。
