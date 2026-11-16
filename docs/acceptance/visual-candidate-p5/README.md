# P5 软件审核与用户验收交接

**结论：开发和自动化审核已完成，完整验收仍待用户。** 用户明确真机实测后续自行检测；本阶段不再次索要解锁，也不把真机、独立真实样本或未执行硬件测试记为通过。P4产品提交为 `531bc91`；本阶段新增审核工具和证据索引，不再改产品算法或阈值。

## 本轮可复核结果

| 项目 | 事实与证据 | 结论边界 |
| --- | --- | --- |
| 当前源码与锁文件 | `software-evidence-audit-final.json`核对P4源文件清单，包含新增／删除检测，以及当前源码、锁文件、测试HTML和执行脚本哈希 | 版本相同才复用已执行测试；文档索引修改不冒充代码变化 |
| 新功能与故障回放 | P4 `lifetime-qualified/summary.json`：33常规视觉、29产品回放、5生命周期API、1传输故障；原始日志的退出码、最后结果计数和SHA256重新核对 | 31 ignored未运行者不计通过；产品回放使用固定像素和实际GPU／CPU worker，原生答案按测试注入 |
| 既有功能 | P4 `lifetime-regression/summary.json`：capture611／122 ignored、media8、geometry10、native-baseline11；`lifetime-selection-target.txt`另3项通过 | 不是122项ignored全部执行；真实窗口／宿主可见操作由用户后验收 |
| P2行为等价 | 从当前原始`visual-reference.txt`重新解析四份报告，与P2冻结报告全字段比较；仅COMPARISON中的两项实测时间排除 | 不仅相信旧的“等价=true”；失败状态、坐标、候选、证据及校准行为都不能被当时间忽略 |
| 输入图集完整性 | 三套版本化图集各自锁定：v4、v4.1、8k-v4；独立PNG解码并核对RGBA哈希、尺寸、案例数和资产清单 | 都是HTML开发／回归数据；独立真实宿主样本仍为0，不增加精度统计分母 |
| 生命周期与工程 | P4两项各60秒idle、cargo xtask check及新程序构建退出0；本轮核对摘要和原始日志SHA256、构建exe哈希 | 不借历史成功批次认证当前源码实屏性能，不把消息注入当物理恢复 |
| 新审核工具自身 | `tool-regression.json/txt`，66项工具测试通过，含6项审核器反例测试；`fixture-tool-regression.json/txt`另验证6项图集损坏反例 | 拒绝0匹配、失败、重复suite、计数变化、日志篡改、源码增删／变化、越界路径和行为变化；不签发真实效果资格 |

所有P4路径相对 `../visual-candidate-p4/`。同名编号的不同资格分开记录，测试数量不相加冒充独立样本数。完整适用合同映射沿用[P2执行记录](../visual-candidate-p2/README.md)、[P3覆盖审核](../visual-candidate-p3/coverage-audit.md)、[P4覆盖审核](../visual-candidate-p4/coverage-audit.md)；校准项保留原始观察，不改成硬断言通过。

## 清理与唯一产品路线审核

本表为**源码事实**，路径相对 `apps/yue-shot/crates/yue-shot-capture/src/`。没有为了制造清理成果删除有效回归实验，也没有新增产品实体。

| 检查项 | 现状／出处 | 处理及“去掉会怎样” |
| --- | --- | --- |
| 产品算法 | `visual_detection/progressive.rs::Query`调用A的分阶段提案／验证；`windows/visual_worker.rs::VisualWorker`单线程执行 | 保留唯一A／CPU路线；删Query无法用已读覆盖继续查询 |
| 落选B／全局池 | `visual_detection/proposals.rs`的`B_LIMITS`、`components`、`directional_rectangles`、`components_with_global`均为`cfg(test)` | 不进产品，不分配生产全局池。保留对照回归，删掉会失去P2选择证据及算法对照 |
| 图形后端 | `windows/visual_pipeline.rs`拥有单`BatchReadback`；`win/d3d11/visual_transfer.rs`为实际有界atlas | 没有同步Map后备视觉算法、计算shader或第二产品后端；测试原型在子测试模块 |
| 目标合同 | `window_detection/model.rs::SelectionTarget`统一Window／Accessibility／Visual；旧WindowTarget定义已在P1删除 | 不新增adapter／转发／迁移标记；统一确认矩形与来源 |
| 生命周期标签 | `windows/top_level_provider/lifetime.rs`；`WindowIdentity.creation_token`贯穿请求和确认 | 必需，删除会恢复同PID同class复用缺口；权限／容量失败为零标签，禁视觉并保留原生／手动 |
| 新P5工具 | `tools/review-visual-candidate-acceptance.py`仅离线检查已执行证据；复用`yueshot_visual_fixtures.py`和Pillow | 去掉会失去可重跑的源码／日志／图集交叉审核；没有新产品依赖、线程、缓存或配置开关 |

工程门包含产品编译、Clippy拒绝警告和依赖方向检查；这些验证比用文本搜索猜测“所有函数都被使用”更可靠。源码审核的定位结果保存在`cleanup-audit.txt`，不将单次rg结果当运行时证明。

## 关键行为前后对照

| 项目 | 升级前的已记录行为 | 当前结果与证据 |
| --- | --- | --- |
| 原生／窗口 | 两个来源，既有命中／标签／默认策略 | 相关回归通过，具体Text／Image等优先，Custom可细化；真实宿主暴露与可见操作待用户 |
| 视觉补盲 | 没有视觉来源 | A／CPU＋有界异步传输已接入；T19/T21/Q13固定像素经实际产品路径回放 |
| K01确认 | 同owner换preview时可确认错误矩形，P0有失败证据 | 请求冻结source／owner／rect；T13的24排列及8种不安全刷新拒绝 |
| K02手动选层 | 新path沿用数组索引导致几何漂移 | 按owner＋几何保持，移出／Shift／取消等明确解除；相关controller回放通过 |
| K03 Shift | 短路或使第二状态未清理 | 两份状态分别清理；独立phase×Shift向量和既有基线通过 |
| E08身份 | 只有HWND／PID／class，无法识别同类同进程复用 | 增独立私有生命周期属性，0标签不发视觉；标签失效、9维晚到、确认换代拒绝通过；真实宿主权限与复用待验收 |
| 性能／资源 | 原生基线与P2/P3测量已封存 | 当前软件缓冲／调度／取消／idle合同通过；当前源码真实首显／全交互／设备负载指标未验收，不借旧哈希数字声称通过 |

## 用户后续执行入口

1. 从当前提交构建并登记exe哈希：`cargo build -p yue-shot --locked`。按[P4可见验收表](../visual-candidate-p4/visible-acceptance-checklist.md)逐条执行V01～V07，重点核验相似框、边界、1px层、确认和第二会话。
2. 需要当前源码实捕／成对资源／4K测量时，用新目录执行下列实屏runner；它会核对源码，失败立即记录并停止。它不替代真实F5/F7／保存／贴图。
3. 独立真实宿主、权限受限窗口及硬件矩阵按设计§8.2／§8.4采集，先冻结图、查询、真值、复核人与名单，再执行。每宿主的统计门与置信修正不变；HTML变体不能充独立真实样本。

```powershell
pwsh -NoProfile -File tools/test-visual-candidate-p4-desktop.ps1 `
  -EvidenceDirectory docs/acceptance/visual-candidate-p5/user-desktop-01 `
  -SourceManifest docs/acceptance/visual-candidate-p4/lifetime-qualified/summary.json

# 只审核软件证据；不操作桌面，不重跑检测，不覆盖已有输出。
$env:PYTHONUTF8='1'
python tools/review-visual-candidate-acceptance.py `
  --p4-directory docs/acceptance/visual-candidate-p4 `
  --output docs/acceptance/visual-candidate-p5/review-next.json
```

如果产品源码变化，先在新目录重跑相应测试／工程检查，登记新来源，再更新阶段索引；审核器拒绝旧源码证据。不能修改历史摘要的哈希来伪造复测。用户实测结果须新建批次，保留失败及后续复测，不回写旧日志。当前没有可读的独立真实图集，真实准确率、召回及误吸附置信均为**待确认／未取得**。
