# P4 产品视觉接入执行记录

状态：**软件收尾；用户已授权真机延期，自动化通过后可提交推进，最终资格待用户验收**。起点为已推送的 P3 提交 `3103590`。本目录同时保留通过和失败记录，软件回放、隐藏 GPU 回放、可见实屏及统计资格分别记录。

## 当前实现事实

| 范围 | 实际实现与出处 | 去掉会发生什么 |
| --- | --- | --- |
| 产品检测 | `src/visual_detection/progressive.rs::Query` 接入已选 A；`proposals.rs` 的 B／全局提案仍仅 `cfg(test)` | 无法根据实际已读像素继续 A 查询；产品不运行 B 后备路线 |
| CPU worker | `src/windows/visual_worker.rs::VisualWorker`；预创建一个 CPU 线程，2MiB 栈，容量1命令及结果单槽；只有拥有所有权的字节与几何跨线程 | 计算回到 owner 消息处理路径，或出现无界排队；GPU／COM 不跨此边界 |
| owner 调度 | `src/windows/visual_pipeline.rs::VisualPipeline`；一项 active、一项 latest、一份4MiB atlas；缓存只复用同源覆盖字节，新光标重新计算 | 源、在途图形资源和下一次请求无法统一失效／安全排空；不新增独立候选池 |
| 图形读回 | `src/windows/win/d3d11/visual_transfer.rs::BatchReadback` 从 P3 测试专用代码移入产品；每事件≤64 copy、一次 Flush、Map DO_NOT_WAIT | 同步读回阻塞 owner；取消只排空已提交命令，不再补交未提交 copy |
| 归属融合 | `src/window_detection/visual.rs::VisualSnapshot::{current,fuse}`；同 source／capture／owner／epoch／point，冻结范围及保守前窗遮挡；语义政策见设计§4.6 | 几何接近的错源／错 owner 结果会进入显示与确认；像素不提供原生语义 |
| 控制器接入 | `src/windows/overlay/visual.rs`；`TargetOrigin::Visual`，统一显示链与预览／选层／确认，标签为“区域” | 视觉与原生显示会各自决定目标，提示与导出可能不一致 |
| 分配失败 | `src/visual_detection/allocation.rs`；直接 Vec／表增长使用 fallible reserve，测试按命名 site 注入 | 检测／融合分配失败可能发布部分结果；此处不是对 Rust／驱动内部所有分配的全局拦截 |
| 诊断复用 | `src/diagnostics.rs::WindowDetectionMetrics` 新增 visual 计数、延迟及缓冲峰值，沿用会话摘要／reset | 无法区分没有提交、空结果、失败和取消；不另建遥测模块 |

本表的 `src/` 相对 `apps/yue-shot/crates/yue-shot-capture/`。新增 Rust 文件是实现职责划分，不代表每个文件增加运行时线程、缓存或依赖。未新增第三方依赖、shader、兼容层或迁移分支。

## 有意改变的旧行为

| 合同 | P0 实际行为 | 当前实现及验证边界 |
| --- | --- | --- |
| K01 同 owner 确认竞态 | 晚到预览矩形可替换请求时矩形 | `overlay/hover.rs::{confirm_snap_preview,apply_confirmation}` 保存 `confirming_preview`，消费请求时目标／矩形／source；旧日志保留。`native_baseline` 对应断言改为请求时矩形；视觉控制器回放另比较导出字节 |
| K02 手动选层 | 祖先插入可使按下标保持的层漂移 | 保存 `(WindowIdentity, PathLevel)`；在该 owner／几何内保持，移出／换 owner／Shift／取消解除；视觉与原生晚到回放分别验证 |
| K03 Shift 清理 | 两个 `take()` 的逻辑或短路，可能残留层链 | 分别执行两个 `take()`；Shift 清除手动锁、混合链与视觉请求 |
| T23／Q07 近邻真实层 | 原生滚轮的3px跳过规则不保证视觉1px层可达 | `window_detection/deep.rs::next_visible_stop`：涉及 Visual 的不同几何保留；原生旧规则保持。Q07 阶段向量验证真实层、重复、交叉；不引入面积比阈值 |
| 缓存原生路径上的新 seed | 原生缓存命中不会再提交原生深入，缺少视觉新点入口 | `overlay/hover.rs::{drive_refinement,on_refinement_tick}` 复用现有80ms计时器；不伪造原生 pending。已通过真实 `WM_TIMER` → controller handler → 新视觉矩形回放 |
| 取消时 CPU 字节释放 | 初期 P4 中，取消发生在 Pace 等待期间可能留下 worker 内 Query | `visual_worker.rs` 在失效的 Pace 上释放 Query／source；专门回放核对 Query 存活计数与 CPU 线程退出 |

后四项是 P4 开发中发现并修正的接入问题，不回写成 P0 已有能力。另修复两项：查询局部BudgetExceeded不再停用整幅冻结源；视觉失败后撤销旧视觉预览，恢复原生／窗口。T13补24种到达排列后实证发现刷新会无条件清掉确认：现在同身份／同owner边界且无保守遮挡的枚举更新保留请求时意图，owner消失／移动／身份／遮挡／换源仍拒绝，7个负向量验证；视觉候选epoch门不变。P0 golden 与历史失败原始证据不修改。

## 已执行记录与边界

| 批次 | 实际结论 | 资格边界 |
| --- | --- | --- |
| `initial-product-replays/`、`product-replays-run2/` | 26常规＋11/12产品回放＋1传输故障通过，各sourceUnchanged=true | 历史源码，不认证后续改动 |
| `software-regression-run3/` | 31常规／16 ignored，16产品、1故障通过；四行为报告等同P2 | 未覆盖后增合同 |
| `software-regression-run4/` | 33常规／30 ignored，28产品、1故障通过；四行为报告等同P2 | T13当时只跑原事件流，没有24排列 |
| `software-regression-run5/` | 33常规和1故障通过；产品27通过、1失败 | 同owner重新枚举的预期修正见覆盖审核；失败不抹去 |
| **`software-regression-run6/`** | **33常规／30 ignored，28产品、1故障全过，sourceUnchanged=true；四行为报告精确等同P2（只排除2项实测时间）** | 当前软件资格批次；含T13 24排列＋7拒绝、T15 8独立情境、T19/T21/Q13 16条真实产品查询、T22分9块真实读回；不等于完整Demo解释器或实屏资格 |
| `final-regression-run/` | capture606／121 ignored、media8、geometry10、native-baseline11、selection-target3、真实窗口3及前6项原生/GPU定向门通过；最后3项实捕均GetCursorPos错误5失败，runner退出1 | 该批退出1，失败保留；`final-desktop-observation.json`确认Screen-saver。随后同源补跑3项全部通过，完整B汇总见`consolidated-baseline.json`；仍不代表可见产品操作 |
| `final-idle-probes/` | 缓存第二点零新增Map＋60秒静止通过，E11初稿输入前提错误失败，整体退出101 | 详见覆盖审核；不靠提高产品截止解决 |
| `final-idle-probes-run2/` | 2项各60秒通过，exit=0、sourceUnchanged=true；E11完成0／Unavailable 1／迟到发布0，T18覆盖换点零新增Map | 真实GPU＋worker的pipeline子合同，不替代可见Selecting操作 |
| `final-engineering-check.json/txt` | 完整`cargo xtask check`退出0、sourceUnchanged=true | 旧`engineering-check.*`不认证最新源码；完整工程门不包含当前未过的实屏门 |

### 实捕、冷会话与资源（历史通过，须辨认源码批次）

- `development-observations/yueshot-p4-real-lifecycle.txt`：Screen-saver中首次GetCursorPos错误5；`desktop-observation.json`保留观察。
- `unlocked-real-lifecycle.*`、`unlocked-lifecycle-with-reasons.*`：解锁后105轮真实复杂桌面捕获与真实owner、60秒静止；视觉全部BudgetExceeded。它证明失败隔离与清理，**不证明成功视觉检测**，预算未调大。
- `controlled-positive-lifecycle.*`：初稿可控host只有绘制chrome、没有正目标，默认Window断言失败。`controlled-positive-lifecycle-run2.*`修正fixture后，独立可见D2D宿主、真实捕获／枚举owner的105轮全部Complete且Visual默认框精确，61个静止事件。
- `final-controlled-control/`：PowerShell将cargo正常stderr按Stop处理，测试未启动；`final-controlled-control-run2/`：测试自身100正式捕获＋61静止断言通过，但runner丢失exitCode，gate=false。修正采集脚本持有进程handle后，`final-controlled-control-run3/summary.json`与`final-controlled-positive/summary.json`均exit=0、gatePassed=true、sourceUnchanged=true。
- 上述配对批次均5轮暖机＋100正式独立捕获会话。分析组100条全Complete：冷源查询P95=9.570ms、controller交付P95=10.166ms；检测缓冲保守上界10,002,576 bytes（含2MiB worker栈和保守owner核算）。两组末20秒private/working/GPU均为平台。CPU/线程/句柄外部500ms采样在process-samples.json；分析组静止CPU共0.046875秒，包含测试探针。**之后T13确认处理与测试又有修改，因此这组是历史产品链资格，不能写成当前完整阶段已过。**
- 冷源指每轮新捕获／新source、首用Query／atlas，CPU worker在controller初始化时预建；不是100次进程冷启动。GPU每进程usage包含独立可见D2D宿主、捕获与驱动，不能当纯视觉显存增量；显式视觉atlas只有一份4MiB。500ms观测峰值不能伪称连续时间的精确峰值，新的采集脚本另记录OS进程peakWorkingSet／peakPagedMemory。

### 真实4K呈现竞争（历史通过）

`product-4k-present-qualification.txt/json`退出0：同device真实产品controller／transfer／CPU worker，3840×2160 D2D/DComp/Present可见独立测试面竞争。T01/T21/T10/T10-8k各至少1000暖查询、3会话；为收满连续DXGI显示间隔实际样本更多，原始数组保留。各场景最高会话查询P95依次0.6379／4.0960／0.8411／1.1389ms，最高owner P95=0.0069ms、hot P95=0.0009ms；配对显示间隔P95增量为−0.2／−0.1／−0.2／0µs。没有把CPU提交间隔当显示间隔。

该测量复用同源像素，是暖路径，不替代上述冷源会话；测后又有T13等修改，当前源码须重测。`VISUAL_PRESENT`行描述测试显示面本身，`PRODUCT_PRESENT`行才声明其与产品controller同device竞争；前者“not the product controller”不表示未执行后者。8K是输入纹理，不是8K物理显示器；本机单卡／SDR结果不外推iGPU／其他厂商／HDR／满载／远程／电池。

## 当前稳定源码的最终实屏批次

`final-desktop-requalification/summary.json`：三项实捕回归全部通过；`qualified-controlled-control/`与`qualified-controlled-positive/`各5暖机＋100正式捕获、61个静止事件通过；4K竞争四场景×三会话通过。整个批次sourceUnchanged=true，结束后对软件及B源哈希再次核对一致。`consolidated-baseline.json`汇总原606等既有回归及同源3项补跑，旧屏保失败仍保留。

用`python tools/analyze-visual-candidate-p4-evidence.py docs/acceptance/visual-candidate-p4 docs/acceptance/visual-candidate-p4/qualified-measurements-reviewed.json`复算原始数组，得到：

| 当前实测 | 结果与边界 |
| --- | --- |
| 新源／新捕获100会话 | 查询P95=10.958ms，controller交付P95=11.508ms；100条全部Complete、精确Visual框。预建worker，不是进程冷启动 |
| 暖查询最高会话P95 | T01 0.4984ms；T21 4.3853ms；T10 0.6872ms；T10-8k 1.0904ms；每场景三会话，每会话至少1000条，原始样本更多 |
| owner／hot最高会话P95 | 7.8µs／1.0µs，分别低于1ms／0.1ms门；不含未测的真实指针移动→停留计时抖动 |
| 实际连续显示间隔P95增量 | 四组依次−0.2／0／−0.2／−0.2µs；完整4K D2D/DComp/Present，独立测试显示面与产品controller共享device |
| 缓冲／显式GPU | CPU保守容量上界10,002,576 bytes（含2MiB栈及保守owner）；单份4MiB atlas；16MiB／32MiB显式预算内 |
| 实际进程峰值 | 分析组／对照组观测private峰174,559,232／157,495,296 bytes；OS peakWorkingSet 102,842,368／97,972,224 bytes；OS peakPagedMemory 177,741,824／173,731,840 bytes。**观测private差17,063,936 bytes不是纯视觉缓冲开销**；它包含已有捕获／驱动／测试端口与分配器，500ms采样还会错过短峰，不能用相减代替分配归因或宣称精确private峰 |
| 实际GPU进程用量观测峰 | 分析组local/nonlocal=184,406,016/21,082,112 bytes；对照组184,406,016/12,201,984 bytes。包含可见D2D host／捕获／驱动，不标作纯视觉资源 |
| 静止与生命周期 | 两组末20秒private／working／GPU范围均0；各静止段进程CPU 0.078125秒（含测试探针）。每轮取消Query／transfer归零，worker退出平衡；末20会话private范围分析139,264 bytes、对照262,144 bytes，无累积上升证据，原始全部峰保留 |

首次衍生`qualified-measurements.json`只在部分内存峰值聚合中排除了暖机；自评后保留旧文件和旧分析器快照，改为**所有事件含暖机与idle均纳入资源峰值**，只有时间分位排除5条暖机。正式引用`qualified-measurements-reviewed.json`，其analyzer_sha256可复核；原始日志未改。该修正没有删峰、降低门或把整个进程差值强行归因。

新增分析脚本只用Python标准库；去掉它就要手工重算12组时间分位、4组连续呈现和两组资源，并容易混淆源版本／暖机峰。静止作业及Map计数只在`cfg(test)`存在；去掉会使“OS内存平台”无法证明没有空转。`confirmation_owner_is_current`复用一个实际owner／遮挡谓词；去掉会重新出现按枚举epoch误删确认或两处条件漂移。

## 尚未通过的阶段门

| 门 | 当前原因与下一步 |
| --- | --- |
| 完整Demo合同 | [覆盖审核](coverage-audit.md)：Q05已补产品Query＋实际GPU三变体权限runner（见下文）；E08没有owner_creation_token，现有HWND/PID/class无法保证同PID同类复用；其余具名子合同不代替未知／未实现操作 |
| 可见全时延 | 当前提交→查询／呈现／资源子门已通过；真实指针移动→80ms停留→结果及可见产品首显还未测。不能把80ms加查询分位冒充实际完整分布 |
| 可见产品操作 | `desktop-control-observation.json`：native pipe unavailable、browser access host unavailable；解锁不修复缺失控制组件。F5/F7/保存/贴图未执行，controller API回放不代替 |
| 图形异常／扩展硬件 | Map结果／显示与电源消息注入已测；不等于物理设备移除／睡眠恢复／热插拔。E14扩展设备与负载矩阵未取得 |
| P5独立统计 | 实际独立宿主图集／复核／分层置信仍无，HTML只用于开发与确定性回归 |

**推断／阶段结论**：P4仍进行中，不提交为已完成阶段、不推进P5、不称升级成功；按用户“通过该阶段全部测试才提交推送”执行。旧失败不删除，源码与证据不加兼容分支。状态与具体开放门以`stage-gate.json`为准。

## Q05后续合同修正与重新资格

`Query::new`接收明确的实际初始region；worker从真实读回结果传该几何，不再推导ROI。tile分配前验证源范围、point包含、区域容量与精确长度；非法／超大／零区域有负例。`visual_replay_demo_q05_actual_read_coverage_and_bounded_requery`三图相同，copy前强制权限，Query只收授权字节：full检出原框、crop只读初始ROI后拒绝缺失证据且无结果、recover补查后检出真框，不造ROI边。本次是核心输入合同变化，**上文run6、工程与最终实屏批次是修改前事实**；当前通过须引用新哈希批次。未新增产品权限开关或兼容构造器。

## 当前region合同后的验证索引

- `coverage-qualified-run8/summary.json`：33常规／31 ignored、29产品、1传输故障全过，sourceUnchanged=true；`p2-behavior-equivalence.json`四报告全部行为字段精确等同P2。`coverage-qualified-run7`测试本身通过但末尾sourceUnchanged=false（补充Q05实际传输字节计数时与批次重叠），不能作最终资格；已重新完整运行run8。
- `coverage-final-regression/summary.json`：606 capture／122 ignored，media8、geometry10、native-baseline11、target3、真实窗口3、9项原生/GPU定向门全部实际通过。`coverage-source-integrity.json`在最终文档索引更新前核对全部代码、锁文件与基线输入哈希。
- `coverage-final-engineering.json`：cargo xtask check退出0、sourceUnchanged=true。`coverage-product-build.json`：新构建成功，exe哈希明确；可见操作尚未验证。
- `coverage-final-idle/summary.json`：新region合同下两个各60秒静止子合同全过，exit=0、sourceUnchanged=true；缓存换点零新增Map，静止零新增job／poll；200ms延迟＋1000移动后无过期发布。
- `coverage-final-desktop/final-desktop-requalification/`：第一项GetCursorPos错误5失败，新的串行runner立即停止；不是跳过失败继续称全过。4K／冷源成功的`qualified-measurements-reviewed.json`是修改region合同前的历史批次，仍保留但不借用为当前源码全部性能资格。
- 新增`tools/test-visual-candidate-p4-desktop.ps1`把3项实捕＋两组资源＋4K竞争按固定顺序执行、每步检查非零计数及退出码，失败保留并停止；去掉它会使这组门只能依赖未入仓的长命令和人工串行顺序。两个测量脚本同时支持绝对／相对输出目录，不覆盖旧证据；输出脚本与被测源码各自记录hash。

当前未过门还包括E08创建身份、可见产品操作（[逐项清单](visible-acceptance-checklist.md)）、完整真实交互时延、物理恢复与扩展硬件。Q05读取权限门已由真实选中后端三变体补齐，不再留为“待实现”。没有提交推送P4，P5未开始。


## 最新执行政策与E08修正（覆盖此前“不可推进”结论）

用户明确“真机实测的任务可以先放在一边，后续我会来检测”。因此可见UI、当前源码实捕／4K性能、物理恢复／扩展硬件与独立真实图集作为用户后验收，不再阻塞开发提交；原阈值、失败日志和未通过状态保留。前文关于不可推进／P5未开始均为该指示前的执行记录，以本节和stage-gate最新状态为准。

E08根因是没有窗口生命周期数据；现新增`top_level_provider/lifetime.rs::LifetimeTags`，provider独立GUID属性名＋进程内不重用标量，销毁／属性丢失使旧token失效。挂标签后重读身份／边界并双核对属性；`WindowIdentity`构造器原位改为显式四字段，无旧接口。零token保留原生能力但禁止视觉。新增实体、4096记录上限、清理和权限副作用见设计§4.6.1。`lifetime-before/identity.txt`是修改前4项身份基线；新增5项隐藏Win32 API测试，已有pipeline迟到矩阵由8维扩为9维，T13不安全刷新由7种扩为8种，融合／请求边界补零token拒绝。实际HWND值由OS分配，不把测试标签丢失冒充强制复用了同一个HWND。


## P4开发门结论（生命周期修正后，当前源码）

`lifetime-qualified/summary.json`：33视觉常规、29产品回放、5生命周期API、1传输故障全过，31 ignored不算通过，源码未变；四份P2行为报告完全等价。`lifetime-regression/summary.json`：capture611／122 ignored、media8、geometry10、native-baseline11通过，`lifetime-selection-target.txt`另3项统一目标GPU回放通过。`lifetime-engineering.json`记录cargo xtask check退出0；`lifetime-idle/summary.json`两项各60秒通过；`lifetime-product-build.json`构建成功并保存exe哈希。代码／锁文件／测试输入未变检查见`lifetime-source-integrity.json`。

**结论**：P4实现和适用自动化已完成，可按用户最新指示提交推送并进入P5软件审核。真机、可见操作、权限覆盖、硬件矩阵和独立真实统计仍待用户验收；完整升级资格未通过。旧批次与失败原样保留，未将其计为当前源码性能通过。
