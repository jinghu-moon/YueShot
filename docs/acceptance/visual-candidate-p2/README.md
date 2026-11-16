# P2 CPU算法对照与传输参考

状态：P2开发阶段门通过，提交／推送以Git记录为准；不是视觉产品已交付。起点`3990eae1375dfd5bb3b94ef56796424c5d580231`。所有算法／读回探针均为`cfg(test)`，没有修改默认自动目标。

## 根因、实现与候选决策

| 类别 | 结论与出处 |
| --- | --- |
| 事实 | 算法只接八位sRGB像素、读取范围、指针；不接DOM、案例ID、标注、原生语义。`src/visual_detection/mod.rs::Image`、`proposals.rs::seed/components_with_global` |
| 事实 | 原A的四侧峰笛卡尔积制造大量无关矩形并耗尽验证预算；已删除，改为沿连续边追踪角点；四边重新验证。每侧48峰来源为24层×两描边响应，完整验证请求仍64。不是简单扩大笛卡尔积预算 |
| 事实 | 同符号RGB残差抵消仿射渐变；描边四边联合归一到外沿，精修后重新验证。相邻填充间隙不能只凭单边反向跃迁否决；角部延伸要求连续越过角点 |
| 事实 | 纹理过滤看边界的切向跃迁变化，内部纹理不是硬否决条件；Q15固定内部、只改外框对照。阈值5/255，支持率0.65、最大断裂0.20、每端去角ceil(10%)、最多64个不同采样、至少8个；粗验证16点阈值保守放宽 |
| 事实 | B独立执行准确面积缩图、前景8/背景4邻接、方向线、原图重验证和512ROI补检，没有调用A补齐。`coarse()`成本每源单列，不能只看每查询耗时 |
| 推断 | 选择A：同集合成硬查询没有发现B独有的必需几何收益，B有预算溢出和更多实体；冻结本轮开发profile，真实资格在P5检验 |
| 事实／边界 | 首版固定SDR。共享`PixelBuffer`已校验stride/storage、NaN/Inf；负scRGB是合法输入颜色，但未取得分析映射资格时返回UnsupportedSource，不把FP16字节当八位色。透明样本不猜背景 |
| 推断 | 后续P3只优化有界传输：实际同步小Map已表现出高等待成本，纯CPU颜色计算没有证据要求新增shader。P3仍需证明批量、异步、UI争用／取消均达标，不能借正确性测试宣布性能达标 |

### 与原假设冲突及处理

| 原假设 | 实际处理 |
| --- | --- |
| 逐层组合4峰 | 开发实验改为追踪角点与48峰，设计§4.4/§4.7/附录E和HTML4.1同步，旧4峰实验结果不能当新合同通过 |
| P2需要真实宿主图才能开发 | 按用户明确要求，以HTML固定像素完成算法开发与对照；控件类型不传给检测器。真实图分布、owner/原生语义和完整操作链仍不由合成输入证明；P4/P5资格另记，不阻止本阶段算法开发 |
| 读取少量字节就必然很快 | 按需块证明字节可控，但数千次Map比CPU更贵；必须保留真实调用数与端到端成本，进入传输优化 |
| 框内方差小作为接受门 | 不采用。会误拒文字／图片／渐变填充；保留边界证据和纹理负例，不增加全图方差实体 |

## 新增实体必要性

全部为测试专用，未选择的B不进入产品。

| 实体 | 去掉会发生的具体问题 |
| --- | --- |
| `Image`、`Completion`、`SideEvidence`、`Candidate` | 缺少显式读取覆盖／失败状态与可观测边证据，无法拒绝ROI伪边、核查数值阶段或区分不完整结果 |
| `proposals.rs` A/B实验与有界几何链 | 无法在同输入比较路线，或让无关峰组合耗尽预算；链为几何参考，原生融合不在这里伪造 |
| `reads.rs::DemandPixels`（512ROI＋4×4追加块）与`Image::from_reader` | 只有完整CPU图时无法证明实际访问量；也看不到每个小Map的成本。4px为测量粒度，非产品最佳参数／持久瓦片缓存，不直接进入产品 |
| PNG dev dependency与固定输入锁 | Rust无法独立解码浏览器导出的PNG，输入或合同会随环境漂移；不是产品图像依赖 |
| GPU读回／CPU worker探针 | 无法区分算法计算和owner等待，无法验证局部几何原点、单上下文拥有者及源不变 |
| P2证据脚本、4.1与8K开发输入 | 无法固定命令／计数／源码／日志身份，无法测试保持物理小尺寸的8K；历史v4仍由P0引用，不是兼容产品路径 |

## 合同覆盖与不计通过项

测试出处均相对`apps/yue-shot/crates/yue-shot-capture/`。

| 合同／风险 | 可执行验证 | 边界 |
| --- | --- | --- |
| T01～T10、T18/T19/T21、Q组适用像素查询、8K小目标 | `visual_comparison_fixed_inputs_report`：191查询，逐默认／required一对一／forbidden／层级判定；A的适用查询必须Complete | 适用硬查询136，其余是校准、阶段或观察；不把191都记硬通过 |
| Q01/Q02/Q03/Q04/Q11/Q14/Q15边证据 | `visual_detection_rectangle_validation_fixed_stage_vectors` | 硬断言与校准分别报告，不把未设期望的校准记通过 |
| Q06独立采样 | `visual_detection_sampling_stage_contracts_and_single_output_matching` | 五个真实阶段向量；一个输出不能满足两个独立目标 |
| Q07几何包含、重复、交叉、顺序 | `visual_detection_injected_geometry_chains_and_arrival_permutations`与像素查询 | 原生融合Q07/Q09、owner与滚轮在P4 |
| Q08整数／亚像素外沿定位 | `visual_detection_boundary_location_exact_coverage_vectors` | 七个相位、四边误差各自0px；不以端到端1px容差吞掉内沿误差 |
| Q05/E07有限读取 | `visual_detection_read_coverage_and_extreme_dimensions`、`visual_detection_demand_transfer_matches_owned_input_and_enforces_bytes` | 裁片不可造边；按需获取原图后结果与整图参考一致，真实GPU探针另列 |
| M01～M04 | `visual_detection_metamorphic_frozen_pairs` | 12条配对查询，平移／反相／整数缩放／亮度；M05真实DPI在P4/P5 |
| E01～E04 | 输入限制、奇数大图、既有PixelBuffer/格式与通道置换测试 | NaN/Inf、短stride/storage拒绝；负scRGB合法但未映射不支持；不宣称HDR分析资格 |
| E06 | `visual_detection_refinement_rejects_inversion_boundary_peaks_and_stale_evidence` | 倒置、±16边缘峰、不重新验证质量的反例 |
| E09 | `visual_detection_isolated_mask_uses_the_real_component_guard` | 两种邻接均76800原始组件，实际CC在65536处停；非只比常量 |
| E10 | `visual_detection_isolated_capacity_stage_vectors`、`visual_detection_demand_reads_fail_closed_before_overallocation` | 32个隔离／组合向量；真实追加块在分配前守卫；计数注入不冒充所有产品容量已集成 |
| E16单设备重复稳定 | 100次CPU重复几何与通道置换、Q08 | 厂商／多设备交叉资格未测 |
| T11～T17/T20/T22/T23、Q09/Q13融合、E08/E11～E15 | P4/P5与条件P3 | 算法像素能测的部分单列，其余时序／source／owner／取消／分配失败／UI／资源未在P2记通过 |

**安全门**：`DemandPixels`失败是显式状态，不能发布已积累候选。现阶段是测试参考而非产品发布器；P4必须在快照发布处处理所有失败、超时、预算溢出。局部worker参考只证明给定ROI覆盖的等价，不证明ROI外层级，后者由按需整源参考及真实按需GPU测试单独证明。

## 证据批次与复跑

| 批次 | 用途／状态 |
| --- | --- |
| `reference-run/` | 早期源码／v4实验，保留历史结果，不覆盖最终代码 |
| `regression-run/` | 早期常规与原生回归，之后实验源码变化，不能代替最后门 |
| `final-reference-run/` | 名称含final不代表通过。该批次有一条新增测试错误地要求小于1Mi像素的图也必须生成B粗图，实际约束是无需粗图；保留失败日志，修正测试只匹配既有阈值合同，不改算法或降低精度预期 |
| `qualified-reference-run/` | 22通过、0失败、0忽略；表中性能测量出自该批次；Rust算法与最终批次相同 |
| `stage-reference-run/` | 整理HTML／Cargo新增行的换行格式后再次22/22实跑；当前源码／脚本／输入锁精确哈希匹配，阶段门引用本批次 |

```powershell
pwsh -NoProfile -File tools/test-visual-candidate-p2.ps1 -EvidenceDirectory <新目录> -Measure
pwsh -NoProfile -File tools/test-visual-candidate-baseline.ps1 -EvidenceDirectory <新目录> -Native -SelectionTarget
cargo xtask check
```

P2开发计时每场景首次单列、暖机5、正式100，报告P50/P95/最大值，不据100次声称稳定P99。按需GPU探针为单次冷查询成本诊断；不拿它给P95资格。所有参考固定源只读，读回每次逐行验证且最终GPU源字节不变。

未覆盖：产品异步传输／取消与呈现竞争、原生融合／手势和K01～K03修复、真实宿主统计与独立人工标注、多厂商／iGPU／HDR／远程及电源组合。P2通过不代表P3～P5通过；后续仍需满足完整时延、UI、资源与独立验收门。

## 最终结果与关键行为对比

| 项目 | 修改前 | P2实际结果 | 判断范围 |
| --- | --- | --- | --- |
| 原生行为 | P1统一两来源，无视觉 | 591项捕获库、media8、geometry10通过；原生L1为11项、统一目标3项、真实窗口3项、既有9项定向资格全过，P0 golden字节不变 | `final-regression-run/summary.json`；旧K01～K03仍未修复 |
| 视觉参考 | 无算法回放 | 22项视觉测试全执行通过（含4项显式ignored探针/计时）；191查询中136条适用硬查询，A为136正确且Complete；B几何136正确，134 Complete、2 BudgetExceeded | `qualified-reference-run/`；109正／27负查询，全是开发集 |
| A/B复杂度收益 | 无同集测量 | 单次配对CPU P50/P95/max：A 0.521/2.304/3.589ms；B 4.164/37.741/43.391ms，B另含每源粗图准备 | 开发诊断，不对相关查询计算独立置信区间；没有观察到B独有硬几何收益 |
| 8K／ROI外 | 只有整图像素参考无法证明传输 | 137条按需回放与整图几何/状态一致，追加字节最大541888，初始ROI≤1048576 | 每查询清空参考cache；实际计入重复边块字节；不是产品瓦片缓存 |
| 精确定位 | 设计假设 | Q08七相位四边误差全0px；M01～M04共12查询配对通过，E10共32阶段向量通过 | Q校准结果单列，不记硬通过 |
| 输入／源位 | 无该算法入口 | 非法stride/storage、NaN/Inf拒绝；未映射HDR不支持；RGB/BGR置换证据完全相同；GPU读回每次逐行核对，最终源不变 | 不修改共享PixelBuffer实现；只复用其校验 |
| 完整工程 | P1门通过 | 本轮`cargo xtask check`退出0，clippy含all-targets与HDR/AVIF组合、workspace检查均由同一门运行 | `engineering-check.json/txt`；ignored资格按上方独立执行 |

### 性能与下一阶段门

单位ms，P95为5次暖机后100次正式测量的最近秩。首次另在原始报告中；100次不作稳定P99保证。CPU计时为已拥有像素，不能替代传输全成本。

| 参考测量 | P95 | 结论 |
| --- | --- | --- |
| A纯CPU：T01/T03/T06/T07/T09 | 0.215/0.613/0.767/0.517/0.664 | 小场景计算可控 |
| A纯CPU：4K T10／文字种子T21／E09／8K T10 | 0.277/2.478/0.759/1.376 | 文字种子比空白背景贵，仍无计算shader必要性证据 |
| 4K全图／512ROI／交叉条带／四边条带 | 13.275/0.624/0.442/0.744 | 全图默认物化会放大内存与等待 |
| 8K全图／512ROI／交叉条带／四边条带 | 61.244/1.737/1.559/2.896 | 单ROI同步Map已超过UI一次事件1ms目标 |
| ROI→单CPU worker：T01／4K T10／T21 | 0.560/0.746/1.235 | 仅给定ROI覆盖与几何原点等价；不证明ROI外目标 |
| 按需完整源：T21／4K T10／8K T10（单次冷查询） | 248.391/153.247/302.445，**非P95** | 分别6660/3756/8432次Map；源与完整参考一致但性能失败，不能进入产品 |

**推断／决策**：P3必需，范围仅为批量有界异步传输，同一CPU算法继续使用；不能用GPU计算掩盖传输瓶颈。P3必须减少实际copy/Map/唤醒数量，计入CPU拼装与所有字节；当前4×4需求oracle留测试，禁止直接投入产品。P3尚未通过，产品局部25ms／UI1ms／渲染回归／16MiBCPU与32MiBGPU资格都不据本表宣称。

本轮与完整工程检查并行的机器负载可能影响计时；这是保留的开发成本诊断，P3/P5须按固定资格负载、≥1000暖样本和跨会话规则重新测量，不能把本表作为最终性能保证。

阶段门为`stage-gate.json`，核对17个回归suite、原始日志完整性、当前Rust源码与固定输入、22项参考实跑、A完整硬合同与工程退出码。参考脚本自身一直标记`in_progress`，因为它只验证参考范围；最终阶段状态由综合阶段门确认，不能单独将脚本当P2全部验收。HTML导出锁保存生成时原始源码身份；最终源码仅整理未改动行的换行风格，渲染代码和119张像素均未改变。
