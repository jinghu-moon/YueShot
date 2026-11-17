# 智能吸附整素材问题：运行程序与源码核对

## 已取得的事实

| 检查 | 实际结果 | 限制 |
| --- | --- | --- |
| 用户反馈 | T01 内外多个位置均截得同一张 401×200 素材，预览标签为“元素” | 用户操作记录；没有本次会话的视觉细节日志 |
| 用户保存 PNG 的离线复放 | 左、右查询分别得到 `[43,31,100,70]`、`[242,40,100,70]`；两个背景点没有内部候选；均 Complete | 输入为用户本地 PNG，不代表 GPU／交互链已通过 |
| 浏览器整页 PNG 的离线复放 | 左右内部查询各得到对应 100×70 框，背景查询得到素材外框 | 自动化浏览器的画面，不是用户当时的冻结屏幕 |
| 原运行 EXE | SHA256 `83974ef8efea06988949e415e80193272aa439c3459c4f8ccc83096c326b63b0`；不包含 `visual atlas`、`visual query unavailable`、新增诊断文字 | 字符串只是辅助证据 |
| Release 捕获库的编译依赖文件 | 清理前各非测试 `.d` 均没有 `visual_pipeline.rs`／`visual_worker.rs`；测试 `.d` 有；普通构建仍报告 Fresh，EXE 哈希未变 | 可以确认旧的构建产物与当前源码不一致；缓存为何被判为 Fresh，具体机制待确认，不能据此断言 Cargo 本身有缺陷 |
| 定向清理后 | `cargo clean -p yue-shot-capture --release` 后重新编译捕获库及应用；新的非测试 `.d` 包含视觉模块 | 最终 EXE、启动信息另存 `build-verification.json`；可见吸附仍须复测 |

**推断**：旧 Release 捕获库未包含当前视觉链路，足以解释为什么软件只显示原生“元素”。因此先修正运行产物，再判断是否还存在算法或融合错误；不因本次症状改变 Image 保护、边界阈值、资源上限或超时。

**新运行事实／尚未修复**：清理后 EXE SHA256 为 `5390d6c3d85663a9b56003685c58a5a21150b5e7b64cbff18bb9a92ad0589792`。用户复测仍选中素材，并观察到内部框短暂出现后跳到整图。`runtime-events.json` 证明视觉请求已提交，但出现 `BudgetExceeded` 及100ms stall，故不能把缓存修正认作问题全部解决。当前 `progressive.rs::Query::step` 的 Peaks／Rectangles 阶段使用全源 Image coverage；`proposals.rs::scan_peaks`、`line_span` 会读取全宽／全高及远处边界。外围复杂度是否耗尽本次读预算，需要进一步用失败回放验证；不能把这一推断写成已通过的修复。

## 本次改动与验证

- 增加默认日志 `capture.visual`：初始化失败、提交、归属未验证、源被禁用、格式、读取失败、卡住、预算、完成数量与融合来源。只记录状态、请求序号、数量、耗时及枚举，不记录图像、路径、窗口标题或无障碍文字。沿用现有有界滚动日志。
- 增加普通页面及 4K 画面中嵌入 T01 的回归：两个目标、两个背景点分别查询，核对坐标和随种子变化；不向算法传入 DOM、标注或案例 ID。
- 增加显式提供 PNG／查询点的离线诊断入口。诊断打印结果，不把程序无异常退出当作检测正确或人工验收通过。
- `summary.json`／各原始日志记录 34 项视觉测试、29 项实际 GPU／控制器回放、5 项生命周期测试与1项传输异常测试。相比前版，普通测试新增1项、需显式参数的忽略诊断新增1项；同步更新运行器／审核器的严格计数，未减断言或跳过原测试。
- `capture-regression.txt` 为定向清理后的捕获库完整常规回归；实际计数以日志末尾为准。忽略测试不计为通过，真机 V01～V07 也不因此通过。

用户复测时按 F5，在左右两个内部矩形分别停留后确认。视觉预览应标“区域”，目标尺寸100×70；背景点不要求命中内部矩形。若仍失败，读取 `%LOCALAPPDATA%/Yue/YueShot/beta/logs/yueshot.jsonl` 的 `capture.visual` 记录定位，不再只凭最终裁图推测。

## 离线诊断复现

```powershell
$env:YUESHOT_VISUAL_PROBE_PNG = '已明确提供的PNG绝对路径'
$env:YUESHOT_VISUAL_PROBE_POINTS = '91,66;290,75;20,20;210,170'
cargo test -p yue-shot-capture --release --locked --lib visual_diagnose_supplied_png -- --ignored --nocapture --test-threads=1
```

查询点必须是该 PNG 内的实际像素坐标。该入口只读取显式文件，不采集桌面、不操作截图或剪贴板。
