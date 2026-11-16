# P4 覆盖审核

**事实口径**：具名测试通过只认证其执行的字段、场景与边界；不等于通用 Demo 解释器完成。最新原始批次和退出码见 README。以下 `src/` 均相对 `apps/yue-shot/crates/yue-shot-capture/`。事实、推断、缺口分别列出；未完成项不计通过、不从门中删除。

| 合同 | 已有可执行证据／出处（事实） | 尚未证明的范围／状态 |
| --- | --- | --- |
| T11/T20、Q09 原生政策 | `window_detection/visual/tests/contracts.rs`：38个政策向量，28种LevelKind，Text关闭采纳、Custom细化、无owner关闭；controller Text保护和缺worker手动测试 | 融合向量与固定原生答案；不代表真实宿主提供者返回分布 |
| T12 DPI／遮挡 | 同文件4个物理坐标／负原点向量；`VisualSnapshot::fuse`保守拒绝前窗相交 | 非125%/150%/200%真实宿主渲染与异形透明窗真值；仍须实屏资格 |
| T13 确认竞态 | `overlay/native_baseline/visual_replay/contracts.rs`：原事件流、24种视觉／原生／刷新／确认排列，另8种owner消失／移动／PID／class／遮挡／capture／pixel／creation_token变化拒绝 | `qualified_result`边界注入；不是原生提供者的真实时序分布，也不是创建token |
| T14 手动几何 | 同文件：选层、插入、checkpoint、移出；`visual_replay_controller_manual_choice_and_changed_owner_cannot_drift`补owner／epoch | 已覆盖声明的插入／移出合同；更广泛可见操作与完整原生宿主路径仍须B门 |
| T15 phase×Shift | `visual_replay_demo_t15_phase_and_shift_late_completion_matrix`：8个独立情境，实际像素产生结果，再执行controller门 | 不是仅给融合函数传phase；真实键盘操作另在可见门 |
| T16 分析／导出隔离 | `visual_replay_demo_t16_analysis_does_not_change_export_or_source`两路径实际GPU源／导出精确比较；既有提示不导出测试 | writer／clipboard为禁止外部IO的测试端口，不替代实际保存／贴图 |
| T17 来源故障 | `visual_replay_demo_t17_failures_preserve_native_and_manual_and_retire_old_preview`：4个独立故障、手动精确选区；`visual_replay_missing_visual_worker...`补缺worker | 注入故障；驱动／真实提供者故障分布未测。曾失败的旧预览残留已修复，旧日志保留 |
| T18 A缓存与静止 | `visual_pipeline::tests::visual_replay_cached_selection_idle_resource_probe`：同源第二点新矩形、提交增1、Map次数增0；保留完成结果60秒，CPU jobs／GPU polls均不增 | 只适用所选A；B不进入产品。pipeline保持完成cache不等同可见UI一直Selecting；执行结果以独立资源批次为准 |
| T19/T21、Q13 近似场景 | `visual_replay_demo_t19_and_q13_similar_pixels_reach_product_default`：16查询，真实产品检测／读回／融合。T21运行release，debug预算拒绝日志保留 | 合成开发输入；不增加P5精度独立样本数 |
| T22 分片接缝 | `visual_transfer/tests.rs::visual_replay_demo_t22_partitioned_raw_reads_preserve_pixels_evidence_and_d2d`：在T01四条真边分9块，像素／证据精确、源不变、D2D前后精确 | 所选CPU区域传输适用，无计算shader，不据此声称GPU计算分片资格 |
| T23、Q07 混合层 | 4个Q07阶段向量；T23 controller实际滚轮3步，去重、1px真层可达、严格包含、手动保持 | 阶段注入与像素发现分别验证；不把融合通过解释为所有嵌套都能发现 |
| Q05 读取权限 | `visual_transfer/tests.rs::visual_replay_demo_q05_actual_read_coverage_and_bounded_requery`读manifest三变体，copy前强制权限，只把已读owned bytes交产品Query；full检出、crop拒绝补查且不发布、recover有界补查检出真框，源逐字节不变 | `Query::new`显式接收实际初始region；worker沿用读回块几何，不再另推ROI。权限是测试源约束，未新增产品配置；本项后端读取／验证阶段三变体已覆盖，可见交互仍单独验收 |
| E08 迟到／第二会话 | `visual_pipeline`九维键变更；controller取消后第二逻辑会话可确认、旧视觉／原生结果拒绝；源fixture bytes有意复用并明确标注 | `WindowIdentity.creation_token`以provider私有窗口属性失效为证据；5项隐藏API测试覆盖销毁／标签丢失换代、隔离／清理／失败；0标签不发视觉。实际同值HWND重建无法强制，真实两次F5和权限覆盖由用户后验收 |
| E09/E10 容量／失败恢复 | P2限值−1／等于／＋1向量；实际Query／transfer守卫；`visual_replay_query_budget_failure_does_not_disable_other_points_in_same_frame` | 局部BudgetExceeded只停本查询；不宣称复杂桌面检测成功。原复杂实屏105次BudgetExceeded保留 |
| E11 延迟＋1000移动 | `visual_replay_delayed_graphics_latest_query_idle_resource_probe`：真实已提交atlas注入200ms StillDrawing，1000次替换只保留latest，15ms排空节拍，释放后只允许最新点完成或按原100ms截止拒绝，再60秒jobs／poll不增 | 注入实际Map处理分支，非物理GPU卡死。单CPU／单graphics由结构与状态检查保证；各counter执行值见独立资源日志 |
| E12 分配故障 | 算法／owner／fusion所有直接命名site均实际注入；controller故障后native／manual存活；worker创建失败有单独controller测试 | 不是Rust allocator、std channel、COM／驱动内部每次分配的全局拦截；如将“each_allocation_site”解释为这些内部点，则整体证据不足 |
| E13 图形／显示／电源 | 实际pending atlas上的StillDrawing／DeviceRemoved处理注入；取消后不得复用，未释放注入时Drop也不等GPU；真实handler注入display/device/DPI/power消息后清理 | 非物理设备移除、真实睡眠／恢复或显示器热插拔。完整硬件资格仍未取得 |
| E14 扩展设备／负载 | 本机单卡、SDR、4K实际呈现竞争，8K是输入纹理；没有双产品后端 | **未完成**：iGPU／其他厂商／qualified_busy／远程／电池未执行，不能外推 |
| E15 生命周期／资源 | 真实受控D2D可见宿主、真实捕获／owner、105轮分析、取消后Query/transfer归零，worker Drop平衡；配对无分析控制，各61个idle事件及外部进程采样 | 独立测试宿主不是产品可见UI。原始实测、平台／峰值分析和源码批次分别记录；测试端口不执行文件／剪贴板IO |
| B01～B13 既有行为 | `tools/test-visual-candidate-baseline.ps1 -Native -SelectionTarget`执行既有capture/media/geometry、native baseline、target replay、真实窗口、9项原生/GPU门 | 退出码／计数以最新持久摘要为准；ignored不算通过。F5/F7/保存/贴图可见B11仍未执行 |

## 明确未关闭的门

1. **事实／工具阻塞**：`desktop-control-observation.json`中`cua.getState()`返回native pipe unavailable、browser host unavailable。解锁已恢复实捕，但不创建缺失的控制管道。不得用API回放替代F5/F7/保存／贴图的可见验收。
2. **事实／实现已补**：E08独立生命周期属性及九维旧结果拒绝已实现；隐藏API与纯身份／融合测试不等于所有真实宿主权限均可用。Q05已补实际GPU权限runner。
3. **事实／硬件缺口**：E13物理故障／恢复和E14扩展资格未取得；P5独立真实宿主标注／复核为0，HTML只证明同像素算法合同。
4. **用户执行政策更新**：用户明确真机留待自行检测；软件和工程门通过后可提交P4开发成果并推进P5可自动化工作。未执行真机／独立真实统计仍不计通过，不声称最终升级成功。回退依靠Git。

## 开发失败与合同修正

- **事实**：T13新增24排列后，原代码在刷新先于确认时回到Selecting；`t13-snapshot-permutations-before-run2.txt`保存实际失败。修正后24排列＋7种不安全输入拒绝通过，见`development-observations/t13-snapshot-permutations-after-run3.txt`。
- **推断／预期的有据变化**：旧`visual_replay_controller_manual_choice_and_window_epoch_cannot_drift`只更换枚举epoch便要求取消确认，与Demo T13的same_identity_same_bounds相冲突。它现改为`...changed_owner...`并实际改变owner边界，继续要求拒绝；不是将原断言简单改为通过。新增24正例单独证明允许的稳定owner情形，7负例独立保护安全边界。视觉／原生候选的epoch gate没有取消。
- **事实／测试前提错误**：E11初稿的第一条移动与初始seed相同，产品正确地去重，因此“已经无active”的测试前提不成立；改用独立初始seed，保留1000次交替移动。`final-idle-probes/probes.txt`保存失败。
- **推断／不增设通过条件**：Demo E11要求有界在途、latest-only、无迟到与无空转，不要求一个已经排队200ms的查询绕过100ms stall截止来产生正框。最新意图允许正常完成或显式Unavailable；两者都须没有退休点发布，随后完整60秒没有新job／poll。100ms不是25ms性能门的替代；没有放宽任何产品预算。

- **事实／Q05根因修正**：Query原由point自行推导ROI，而owner已完成实际ROI读回，存在两个几何决定点。现在`Query::new(size, point, region, bytes)`仅接受合法、含point、≤512×512像素、长度精确的已读region，Work::Start从实际读回元组携带region。删除旧构造器，不加兼容路径；生产仍选原512ROI策略。Q05三图字节相同，full读1块、crop只读初始1块后权限拒绝、recover实际补查恢复；requested_extra_bytes是申请计数，actual_read_bytes是实际提交字节，拒绝时不可混报。
