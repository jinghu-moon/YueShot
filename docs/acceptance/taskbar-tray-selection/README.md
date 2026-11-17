# 任务栏／托盘选区修正

日期：2026-11-17。范围：原生窗口候选入口及其下游回放；真实任务栏／托盘的可见操作由用户复测，不计入自动通过。

## 根因与历史边界

**事实**：`window_detection/provider.rs::SHELL_SURFACE_CLASSES` 原来同时排除了桌面背景、任务栏、托盘弹窗和 UWP/XAML 宿主。`snapshot.rs::passes_cheap_policy → classify → WindowSnapshot::hit_test` 因此没有任务栏 owner；`windows/overlay/hover.rs::drive_refinement` 无法针对该 owner 调度 UIA/MSAA 子元素查询。此缺陷在视觉融合之前发生。

**事实**：初始提交 `5a3bd82` 中已经存在同一八项列表。不能据此归因于最近的像素识别改动。用户记忆中可用的具体版本、Windows Shell 结构及入口差异**待确认**。

**复现**：`before.txt` 是新测试在旧产品逻辑上的真实失败：鼠标位于 `Shell_TrayWnd` 内，命中后方 App（HWND 256），而非前方任务栏（HWND 512）。这是构造窗口数据的自动复现，不是用户桌面 UIA 实测。

**修正**：删除宽泛 Shell 排除名称及旧接口，改为 `DESKTOP_BACKGROUND_CLASSES`／`is_desktop_background_class`，仅排除 `Progman`、`WorkerW`。其余宿主使用普通窗口的 z 顺序、完整显示器坐标、原生细化及确认链，不新增 Shell 专用 provider、兼容层或额外线程。自有窗口／进程排除、cloaked／离屏过滤和 owner 身份复核保留。

**推断**：任务栏按钮和托盘图标优先需要原生控件范围；像素视觉无法仅凭图标颜色推断整个点击区域。恢复 owner 入口是必要修正，不等于所有 Windows 版本的 Shell 都会提供合格的无障碍矩形。UIA/MSAA 的实际返回、权限限制和自动隐藏状态仍需可见复测。

## 行为与覆盖

| 项目 | 修正前 | 修正后验证 |
| --- | --- | --- |
| 任务栏／托盘宿主命中 | 被黑名单过滤，可能穿透后方 App | 六类宿主 × 底部任务栏／托盘弹窗／负坐标副屏／侧边任务栏，均保留正确前方 owner 与边界 |
| 子元素选区／滚轮／确认 | 上述 owner 无法进入原生查询 | 六类宿主 × 两类宿主几何 × Button/Image，共 24 组隐藏控制器回放；分类、调度、注入原生结果、预览、切父层、切回子层、确认和导出范围逐项断言 |
| 无生命周期标签的原生 owner | 原生合同允许，但 Shell 类名先被过滤 | 回放使用 `creation_token=0`，原生子框仍可查询和确认；不放宽视觉来源的标签门 |
| 桌面背景／自有窗口／不可见宿主 | 不进入候选 | 更新后的精确类名、排除集合与 DWM 测试继续断言拒绝；tool/no-activate 样式本身不作为拒绝理由 |
| 局部 100×70 视觉框及具体原生类型保护 | 上轮已修正并验证 | 重跑现有视觉参考、真实 GPU 产品回放、owner 和传输故障套件；不改算法阈值、融合优先级或预算 |

新增控制器回放调用真实渲染与产品控制器，但无障碍结果和最终 owner 校验结论由测试注入。没有点击或打开真实托盘，没有验证真实 Shell COM 返回，不能把回放当作可见验收。

## 证据与复跑

- `before.txt`：修改前失败；`after-taskbar.txt`：首次两项定向通过。`capture-regression.txt` 是补副屏／侧边场景之前的完整回归；最终源码结果以带 `final` 的日志及 `software/summary.json` 为准。
- `capture-regression-final.txt`：最终捕获库常规测试，616 通过、126 ignored、0 失败。ignored 不算通过。
- `taskbar-final.txt`：最终两项任务栏定向测试，显式执行隐藏控制器回放。
- `software/`：现有 P4 runner 的四套回归、实际计数、退出码及源码／日志哈希。
- `engineering-check.*`：`cargo xtask check` 的原始输出和实际退出码；`build-verification.json` 记录最终 Release 构建及产物身份。最终执行状态见 `verification.json`。

复跑（使用新的输出目录，不覆盖本批证据）：

```powershell
cargo test -p yue-shot-capture --release --locked --lib taskbar -- --include-ignored --nocapture --test-threads=1
cargo test -p yue-shot-capture --release --locked --lib -- --test-threads=1
pwsh -NoProfile -File tools/test-visual-candidate-p4.ps1 -EvidenceDirectory <新目录>
cargo xtask check
cargo build -p yue-shot --release --locked
```

本轮新增 1 项常规分类测试与 1 项 opt-in 控制器测试，故审核器当前基线计数从 615/125 调整为 616/126，`native-baseline` 从 11/18 调整为 11/19；只提高计数，不改历史日志或历史资格结论。P4 视觉四套预期计数不变。

## 用户可见复测

1. 使用本轮构建，F5 后分别停留于任务栏应用按钮、开始／搜索按钮、通知区图标。期望能从宿主框细化至系统提供的子元素矩形，确认后不扩成后方应用。
2. 先展开隐藏图标托盘，再 F5，停留各图标，滚轮切父／子层并确认。核对导出与预览一致。
3. 有副屏或自动隐藏配置时，分别在副屏任务栏和已展开的任务栏复测；未展开时不应声称能选到隐藏控件。
4. 在 Demo T01 左右 100×70 矩形复测稳定的「区域」选择。

以上可见检查仍待用户执行。若仍仅选到宿主，下一步应核对当前系统的 UIA/MSAA 返回及 owner 归属，不通过提高像素阈值掩盖原生查询问题。本轮没有测量 Shell 查询性能，不作性能提升承诺。

原始libtest输出保留尾空格和末尾空行；暂存区全量空白检查的原始提示见`raw-output-whitespace.txt`，不改写日志来消除提示。源码／文档／JSON的空白检查退出0，见`staged-diff-check.json`。这不改变任何测试断言或验收门。
