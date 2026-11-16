# 视觉候选 P1：统一目标合同

实施起点：`ad07e2c`。本阶段只接窗口与原生深入查询，不执行视觉算法；不修复P0记录的K01～K03。

## 根因与最终结构

**事实／改动前**：`window_detection/model.rs::WindowTarget`只携带窗口候选，`screen_bounds()`必然返回窗口几何；`gesture.rs::SnapPreview::selection`独立保存实际选区。`TargetKind::ClientArea`没有生产者，`DeepTarget::kind`与路径深度重复。原生路径只携带控件类型，无法明确区分窗口来源与无障碍来源。

**推断／改造（设计D01、D09、D13）**：原位替换为`SelectionTarget`，显式保存选中虚拟桌面矩形、来源、可选控件类型、真实owner及源身份；`PathLevel`保存来源。窗口命中仍只读取快照；`hover.rs::preview_for_cursor`在最终预览处绑定源，沿用原有几何选择与坐标转换。owner由窗口提供者校验，不能拿控件选区冒充窗口边界。

| 实体／改动 | 去掉会怎样 |
| --- | --- |
| `SelectionTarget::rect_screen`与`owner`分离 | 控件几何仍隐含在预览旁路，后续视觉框无法统一确认／层级合同 |
| `TargetOrigin::{Window, Accessibility}`与`PathLevel::origin` | 相同控件类型Window可能来自窗口快照或无障碍节点，无法仅按类型推断来源；UIA/MSAA不应变成两套选择来源 |
| `SelectionSource` | capture取消代次与PixelSource格式／generation含义不同，合成一个整数会丢掉身份；字段不复制像素 |
| `source: Option<SelectionSource>` | 窗口快照由独立worker产生，不知道捕获帧；强制在窗口命中时提供源会扩大provider职责。实际可见预览在overlay绑定源，P4再执行视觉发布gate |
| 保留`DeepTarget::path`，删除`DeepTarget::kind` | 原生worker已经拥有有界路径；重复保存来源会产生不一致。最终目标复制所选节点，不克隆整条路径／像素 |
| 删除旧类型／枚举／ClientArea与旧provider命名 | 没有生产者的分支不能帮助现有功能；不保留别名、转发或双模型 |

当前保留的`SnapPreview::selection`是显示器局部且裁切后的几何，`SelectionTarget::rect_screen`是虚拟桌面几何，两者坐标域不同。确认冻结修复不夹入本阶段，K01旧行为仍由基线明确记录；P4前须独立修复。

## 验证与前后对比

| 范围 | P0 | P1实际结果／证据 |
| --- | --- | --- |
| 捕获库常规 | 571通过／90 ignored | 573通过／91 ignored，新增2项常规模型合同；[完整定向批次](complete-run/summary.json) |
| media／geometry | 8／10通过 | 8／10通过 |
| 原生L1／L2 trace | 已冻结`native-baseline.json` | 相同golden字节，所有原生回放通过；未批量更新预期 |
| 统一目标合同 | 无 | `selection_target_replay`实际3/3通过，含真实隐藏D3D11/D2D控制器；逐字节核对确认输出，窗口／控件来源与源generation均断言 |
| 真实窗口提供者 | 部分资格未测 | refresh／epoch／hover三项各1/1通过；此处读取真实桌面，不等于真实浏览器COM语义资格 |
| 既有定向原生／GPU | 9项通过 | 同9项逐项重测通过；[各日志](complete-run/summary.json)含精确匹配数与SHA-256 |
| 旧缺陷K01～K03 | 均已复现 | 均仍由特征化复现；未声称修复 |
| 完整门禁 | P0退出0 | 本阶段[命令／退出码](engineering-check.json)、[原始日志](engineering-check.txt)，以实际退出码为准 |

**事实／工具修正**：首次`qualified-run`脚本将`selection-target`追加在`suites`初始化之前，随后被覆盖，因此该批次没有运行新增合同，不拿它证明3项通过。发现后修正脚本，再用新目录`complete-run`重跑，精确要求3项命中；旧记录保留。这不改变任何产品断言或预期。

阶段门还检查源码没有`WindowTarget`／`TargetKind`／`ClientArea`／旧定义与转发，P0 golden哈希不变，定向日志与当前源码哈希一致；见本目录`stage-gate.json`。真实宿主COM／完整桌面操作链、混合DPI、iGPU/HDR／设备丢失仍按P0资格清单与设计P4/P5执行，未在本阶段补测的不算通过。

复跑：

```powershell
pwsh -NoProfile -File tools/test-visual-candidate-baseline.ps1 -EvidenceDirectory <新目录> -Native -SelectionTarget
cargo xtask check
```

不以API重构结论代替视觉算法精度结论。本阶段没有引入Visual来源、分析worker、GPU计算、额外传输或任何兼容代码。
