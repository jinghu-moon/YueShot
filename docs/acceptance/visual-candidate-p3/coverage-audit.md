# P3 覆盖审核

本表是阶段职责核对，不把测试原型解释为产品接入。最终当前源码批次为 `full-monitor-qualification-run/summary.json`；历史失败保留在本目录。所有报告中的 revision 是原型开始前的 P2 HEAD，实际受测代码以各报告的 sourceSha256 为准。

| 合同／风险 | 实际执行与结果 | 适用边界 |
| --- | --- | --- |
| P2 算法输出不得漂移 | reference 20 项、137 条渐进读取回放通过；p2-behavior-equivalence.json 四份冻结报告行为字段逐项全等 | 仅两项实测时间不要求相等；阈值、定位、几何、完成状态未放宽 |
| E03/E04 格式 | 既有像素格式／数值测试通过；faults 真实纹理拒绝 FP16，精确 BGRA 读回 | 首版 SDR；未取得 HDR 分析资格 |
| E07 读取权限／E09/E10 容量 | Query 未读像素显式未知；137 条全源等价；faults 分别执行 16384 区域及 1MiB 精确边界和越界拒绝 | 不以 ROI 裁边造边；B 的 CC 上限仍为测试对照，不进入产品 |
| owner 与有界提交 | 仅 owner 使用 context；单 worker、单 batch；每事件最多 64 copy，最终一次 Flush，非阻塞 Map | 原型拓扑已测，产品 controller 的所有事件组合属于 P4 |
| 取消不可提前复用 | faults 在部分提交、StillDrawing、取消后验证仍 pending，真实完成后才释放；过早 Map／重入被拒绝 | 尚未实现产品超时停用／源代际 gate |
| Map 故障 | StillDrawing、DeviceRemoved 注入真实 complete_mapping 分支；前者保持在途，后者无结果，错误被设备丢失分类器识别 | 不是物理设备移除／睡眠恢复资格，不声称已覆盖驱动所有故障 |
| 源不变／取色交错 | 105 次 offscreen D2D 与既有取色交错；所有输出逐字节核对，源再次读取不变 | 不扩大取色块，不使用第二个即时上下文 |
| 资格负载及渲染 | 3840×2160 全显示器 D2D/DComp/Present；四输入各 1000×3 查询全正确且 Complete；实际连续显示间隔 P95 增量约 −0.1µs | RTX 4070 Ti SUPER／Windows 26100 本机；8K 是输入尺寸，不是 8K 物理显示器；不外推其他厂商、iGPU、GPU 满载 |
| 延迟 | 四组查询最高会话 P95=19.156ms；owner 最高会话 P95=0.231ms；P50/P95/P99/max 及原始样本齐全 | 通过本机原型 25ms／1ms 门；不包含尚未实现的产品归属／融合，P4 必须重测 |
| CPU／GPU 内存 | 原型 Vec capacity、哈希增长／控制字节／对齐、临时区、2MiB 默认 worker 栈和 owner atlas 元数据的保守上界最高 15096688 bytes；显式 GPU atlas 4MiB | 已测输入的容量核算，不是所有输入下的产品分配预算管理器；P4 要在实际分配点守卫及失败注入 |
| 冷资源／静止 | 105 次 staging/Query 生命周期；100 次正式冷分配 P95=6.213ms；自身 Drop 计数归零，61 点／60 秒静止末20秒平台通过；进程 private、GPU 峰值和句柄类型原样保存 | 不是完整产品进程冷启动或完整捕获会话 E15；5 次既有读回预热匹配无分析对照的驱动初始化，严格平台断言未放宽 |
| 既有功能回归 | unlocked-regression-run：capture593、media8、geometry10、native-baseline11、selection-target3、三项真实窗口及九项原生／GPU定向测试全通过 | capture 另99项 ignored 不算通过；此前屏保造成的三项错误5失败保留 |

产品完整 E08～E15、源／owner／request 代际、latest-only、设备忙碌停用、实际分配失败、确认冻结、手动层保持、F5/F7/保存/贴图等由原计划 P4 实施并验收；P5 承担独立真实宿主统计和扩展设备资格。这里没有把缺失项删掉或改成通过。
