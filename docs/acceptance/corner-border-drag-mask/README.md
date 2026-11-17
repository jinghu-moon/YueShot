# 贴图圆角边框与拖动遮罩修正

本批次针对用户在 `dbd70e0` 后反馈的两处现象：F6 贴图的直边有边框、圆角弧段缺边框；F5 拖选时圆角外未模糊，松开后才出现模糊。原始图片在用户本机，不作为仓库可回放像素资产。

## 根因与修改范围

| 行为 | 修改前 | 修改后及验证依据 |
| --- | --- | --- |
| 贴图外形 | `crates/yue-pin/src/screenshot.rs::render` 绘制圆弧 quad 边框，同时向图片 shader 传入圆形半径；`platform.rs::round_screenshot` 再用 `CreateRoundRectRgn` 裁剪。与导出的四阶超椭圆不一致 | 绘制共享超椭圆外轮廓减内轮廓的闭合填充环；图片保留源 alpha，取消第二次圆形裁剪；HWND 使用保守的超椭圆扫描线区域，不切掉部分覆盖的边框像素 |
| 拖动期间的遮罩洞 | `GestureState::move_cursor` 清掉逻辑 preview，`OverlayController::on_mouse_move` 没有同步清掉 `preview_rect` 和动画目标，`RenderView::mask_hole` 因而仍选择旧的方形预览洞 | 在进入手动拖动的同一状态转换中调用 `sync_preview_rect`；直接抓取已有选区/手柄也清掉逻辑与动画预览；阈值以内的点击继续保留预览用于确认 |
| F6 像素契约 | `publish_selection` 发布未遮圆角的像素，最终保存确认路径却发布已遮圆角像素，贴图无法仅靠半径知道能否再次应用遮罩 | 发布保留选区时也在其独立读回缓冲中应用一次圆角。保存确认从冻结原图重新读取，不重复处理已发布缓冲。两条路径均向贴图交付含圆角 alpha 的源格式像素 |
| 形状定义 | 私有于 capture，贴图无法复用 | 将实际实现移至已有纯几何层 `crates/yue-geometry/src/selection_shape.rs`；删除原文件。控制点、解析边界、覆盖率定义共用，不增加依赖、配置或兼容分支 |
| 验收来源校验 | P4 清单只列捕获源码 | 同时记录、检查共享几何源码；新增/删除/修改共享形状均会使过期证据失效；同步实际测试计数门 |

新增私有函数 `screenshot_border` 负责 GPUI 闭合环的适配；没有它就只能保留不匹配的圆形 quad 或把曲线构造塞进图片绘制回调。`screenshot_region` 分离区域构造与 HWND 所有权，以便直接用 `PtInRegion` 验证保守覆盖；没有它就只能依赖可见窗口来测试该裁剪边界。没有新增后台任务、缓存或逐帧图像复制。

## 证据解读

- `baseline-pin.txt`：修改前完整贴图回归，75 通过、2 项显式忽略。
- `before-drag.txt`：新增真实控制器回放在旧实现失败，明确定位残留的 `preview_rect`。
- `before-pin.txt`：新增生产 scene 断言在旧实现失败，缺少匹配的闭合边框路径。
- 根目录其余日志保留初次编译和测试失败，不能用作最终通过依据。
- `pin-mesh-diagnostic.txt`～`pin-mesh-4.txt` 记录测试夹具修正：headless 平台不会自动派发 `SetWindowPos` 后的 Windows resize 通知；测试补发真实窗口应有的尺寸通知后，继续用同一套独立解析边界断言，没有降低边框覆盖或透明内部的断言。
- 首轮 `final-source/xtask-check.txt` 的编辑选区测试失败也属于夹具问题：44×32 的小框中心落在手柄命中区域，无法作为移动手势输入；最终测试先断言左上角命中，再拖动该手柄，并精确断言调整后的矩形，保留预览同步与动画清除断言。
- `final-source-3/` 是最终验证批次（`final-source/`、`final-source-2/` 分别保留夹具与 Clippy 失败）；命令、实际退出码、计数及源码/日志/程序哈希见本目录 `verification.json`。早期日志中的通过只代表当时源码。

覆盖范围包括：四角的实际生产三角网格；0/1/8 源半径，10%/50%/100%/200%/1000% 缩放，100%/125%/150%/200% DPI；真实 GDI 区域在最大放大半径下的保守像素覆盖；真实 D2D 连续变更选区边界时的四角模糊与松开前后一致性；贴图原生 GPU 读回的透明角、半透明内部、四角边框 alpha、源数据共享及资源释放。控制器回放另外检查点击预览、手动拖动和编辑既有选区，以及保留像素的 RGB 不变、独立 8×8 四阶覆盖率与只应用一次 alpha。

## 最终软件验证

| 验证 | 结果 | 证据 |
| --- | --- | --- |
| 工程门 | `cargo xtask check` 退出 0；包括捕获 624 通过 / 130 显式忽略、source contract 6、工具 66、格式、Clippy、workspace 检查和依赖方向 | `final-source-3/xtask-check.txt` |
| 贴图 headless 全量 | 78 通过 / 2 显式忽略 | `final-source-3/pin.txt` |
| 原生贴图 GPU | 1 通过，四个弧段最大 alpha 均为 1.0；透明角、0.5-alpha 内部和源共享/资源释放断言通过 | `final-source-3/pin-native-gpu.txt` |
| 几何层 | 12 通过；原有 10 条加搬入的 2 条曲线精度/G2 测试 | `final-source-3/geometry.txt` |
| D2D / 圆角回归 | 显式运行 8 条通过，包括连续变更边界及 GPU 对照 | `final-source-3/gpu-corners.txt`、`final-source-3/gpu-images/` |
| 模糊遮罩 | 显式运行 1 条通过 | `final-source-3/blurred-mask.txt` |
| 控制器回放 | 14 通过 / 20 显式忽略；新增的 3 条均执行 | `final-source-3/controller.txt` |
| 智能吸附 P4 | 参考 37 / 34 忽略、产品回放 32、owner 生命周期 5、传输故障 1；全部严格计数门通过 | `final-source-3/p4/summary.json` |
| 贴图与几何静态检查 | 含 headless 测试目标的 Clippy 退出 0 | `final-source-3/pin-clippy.txt` |

130/2/20 项默认忽略的计数不是“通过”，上述显式运行的 GPU/原生探针单独记录；其余需要人工/环境矩阵的任务保持原有待验收状态。原始 libtest 日志保留原样；源码空白检查允许仓库既有 CRLF，并排除原始 `.txt` 证据。

原生基线的忽略数按实际运行从旧审计常量 19 校正为 20（任务栏隐藏渲染探针也在默认忽略集内）；没有新增忽略或跳过既有测试。该计数常量在实测后修改，单独重跑证据校验工具测试；它不改变 Rust 产品源码。最终源哈希与起始快照的这一个差异在 `verification.json` 明确记录。

Release 首次产品构建失败时，verbose 记录显示 `Fresh yue-geometry`，但被引用的正常产品 profile 依赖仍是旧 `.rmeta`，不含 `selection_shape`；测试 profile 已重编并通过。保留 `release-fingerprint-diagnostic.txt` 和失败构建日志；只对 `yue-geometry`、`yue-shot-model` 执行 `cargo clean --release` 后重建，没有改 Cargo 配置、产品代码或兼容分支。最终采用 `final-source-3/release-build-rebuilt.txt`，不采用同目录首次失败的 `release-build.txt`；Cargo 为何未使该正常 profile 失效，尚未定位，不能据此断言工具链缺陷。

## 复测范围

自动化绘制/原生窗口探针不能代替用户的可见桌面体验验收。请在复杂背景上 F5 拖出大圆角选区，持续移动鼠标，检查四角外始终模糊；松开后应无模糊跳变。再 F6 贴图，检查四角边框连续，并滚轮缩放观察边框；分别从未保存的已选区域和保存后的截图创建贴图。任务栏/托盘智能吸附仍由既有原生来源回放及 P4 产品回放回归，真实桌面手工操作由用户后验收。

最终 Release：`target/release/YueShot.exe`，18,302,464 字节，SHA256 `279b8152064faa74dd25bf70db3718539b50795cf45649f3c81fd7427dbc0652`。已用 `--background` 启动，记录 PID 为 34632；这是存活核对，不等于用户可见验收通过。
