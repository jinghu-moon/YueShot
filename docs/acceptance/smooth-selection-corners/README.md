# 截图连续圆角

日期：2026-11-17。基线：`7555dcf`。用户要求更柔和的截图圆角，并确认采用四阶超椭圆方案。

## 形状与范围

四个角在原半径方块内采用 `x⁴+y⁴=r⁴` 的四分之一轮廓，中间保留直边。对角中点从旧圆弧的内移 `r·(1−2^(-1/2))` 改为 `r·(1−2^(-1/4))`。这是形状有意变化，不能要求非零圆角与旧图逐字节一致；半径为零必须继续逐字节一致。半径仍以物理像素计，沿用 `geometry::radius_cap` 的 `min(128, 短边/4)` 上限和现有圆角设置。

| 消费方 | 实现与验证重点 |
| --- | --- |
| 选框描边 | `windows/win/d2d/selection_path.rs::geometry` 将统一控制点旋转到四角；沿用内缩描边。只改截图选框，不替换工具栏、放大镜或按钮的圆角 |
| 普通／模糊遮罩 | `d2d.rs::rounded_hole_geometry` 以同一超椭圆路径挖洞；保留原几何缓存和模糊路径 |
| 保存、剪贴板、贴图的选区像素 | `artifact.rs::visit_corners` 使用同一四阶不等式；8×8 子像素覆盖只作用于边界。直通 alpha 不改 RGB，预乘 alpha 同步缩放关联颜色，FP16 保持源精度 |
| 角部手柄 | `geometry.rs::corner_handle_anchor` 使用同一形状的对角中点；已有角区域命中逻辑继续复用 |
| 倒计时窗口／独立暗层 | `windows/win/window.rs::selection_region` 按统一解析边界生成 GDI 像素中心覆盖区域；`ExtCreateRegion` 一次提交合并后的扫描行矩形。GDI 本身只支持二值区域，不伪称其带有 alpha 抗锯齿 |
| 倒计时半径上限 | `overlay/session.rs::countdown_window_pieces` 先按实际选区限制半径，再生成外扩窗口带。新增测试复现旧逻辑在 100×70 选区仍使用 128，而绘制仅使用 17 的不一致 |

## 曲线依据与开销

`selection_shape.rs` 是唯一形状定义。每个角共 8 段三次贝塞尔，前半 4 段的端点取解析曲线 `y/r=0,0.25,0.5,0.67,2^(-1/4)`；控制点位于端点切线上，通过两个端点的曲率方程求出控制距离。另一半交换坐标并反向。直边连接处的曲率为零，各段匹配位置、切线和曲率（G2；非参数速度意义的 C2）。固化控制点免去运行时求解，不引入新依赖。D2D默认展平产生的边缘偏差由真实渲染回放复现，故在几何缓存重建时用0.005px容差将贝塞尔显式展平；重绘直接复用该精细路径，不每帧重算。数值检验包含 80008 个参数点，并以实际 f32 控制点校验最大半径下的坐标误差 <0.01 px。

导出仍只处理四角，复杂度 `O(r²)`，不随整图面积扫描。旧路径为整个半径方块做 4×4 采样并申请 `r²` 字节覆盖数组；新路径先用单调边界判定完整内外像素，只对穿边像素做 8×8 采样，随算随应用四角，删除临时数组。代价用实际导出函数与测试专用旧圆弧公式配对测量；旧公式的输出哈希必须匹配改动前 golden，避免把不同工作量当基准。测试专用参考不进入产品，不是兼容路径。

描边新增一个按矩形／半径缓存的路径：原生圆角 API 不需要每帧创建路径，贝塞尔替换后如果去掉缓存，会在相同选框的每次重绘中创建 COM 几何。缓存只保存当前一条路径，随渲染目标释放。原遮罩几何缓存继续复用。

新增实体的必要性：统一形状模块防止导出／绘制／手柄各自偏离；D2D 适配函数负责将纯控制点交给平台；GDI 构造函数替代只会圆弧的原生区域入口，防止倒计时仍呈现旧形状。未新增用户参数、依赖、兼容层或后台任务。

## 基线、失败与修正纪律

- `before-characterization.txt` 固化旧导出 alpha 哈希及角手柄坐标；`before-{geometry,artifact,mask,blurred-mask,gdi}.txt` 是原行为回归。非零圆角 golden 按明确的新数学合同更新，零半径保留旧哈希。独立 f64 四阶采样另行验证新 alpha，不能仅自比哈希。
- `curve-first.txt` 记录首次编译的 Direct2D 常量导入位置错误；修正后 `curve-final.txt` 验证控制点误差和 G2。
- `shape-first.txt` 保留两类早期失败：几何测试以水平距离取代法向距离，靠近直边时实际间隔小于 D2D 展平容差；改为沿解析法向 ±0.03 px 取样。另一项发现小圆角的 4×4 导出采样不足，升级为边界 8×8，保留严格的独立像素合同。
- `shape-second.txt`～`shape-fourth.txt` 保留后续渲染比较失败；比较模型按产品实际的黑色暗层＋主题色两次source-over计算，不能直接在最终内外颜色之间线性插值。修正模型后仍有最大约20个通道级的偏差，独立256×256积分也确认不是导出方程错误；针对D2D默认展平精度细化路径，显式展平后误差下降，但仍不能将D2D覆盖率等同于CPU方格积分。`shape-fifth.txt`保留原先任意设定“1/16覆盖量子”的新测试失败；最终改为同设备原生圆弧＋同一8×8独立积分＋同一两层叠色的对照上限（原生最大通道误差＋2级舍入）。这改变的是不成立的跨光栅器等同性假设，不放宽四阶方程／导出逐像素合同及±0.03px几何边界门。`GetBounds`对浮点展平的4e-6px舍入采用1e-4px数值容差；最终执行状态以最后一批原始日志及 `verification.json` 为准。
- `shape-third.txt` 同时保留倒计时 128/17 半径不一致的真实失败；修正上限后必须重跑同一断言。

## 验证及用户复测

最终命令、源码／日志哈希、通过／忽略计数、性能和新 Release 产物身份见 `verification.json`。所有早期失败保留，未执行的可见操作不计通过。真实 GPU 输出 PNG 用于检查曲线、描边、手柄和不同圆角大小；它们是隐藏渲染结果，不是用户实屏截图。

```powershell
cargo test -p yue-shot-capture --release --locked --lib smooth_corner -- --include-ignored --nocapture --test-threads=1
cargo test -p yue-shot-capture --release --locked --lib -- --test-threads=1
cargo test -p yue-shot-capture --release --locked --test source_contract -- --test-threads=1
pwsh -NoProfile -File tools/test-visual-candidate-p4.ps1 -EvidenceDirectory <新目录>
cargo xtask check
cargo build -p yue-shot --release --locked
```

用户可在截图界面调整圆角到非零值，检查浅色／深色背景上的直边过渡、四角对称及手柄拖动；再保存 PNG、复制和贴图，核对形状一致。另测小选区、大圆角、倒计时及模糊遮罩。半径 0 仍表示直角。可见交互和主观外观验收待用户执行；软件数学／渲染验证不能替代个人审美确认。

当前新增7项常规测试、3项opt-in图形／导出成本测试，审核器capture计数从616/126提高到623/129；native-baseline仍为11/19，视觉四套预期计数不变。数值以最终实际执行结果复核。`shape-sixth.txt`中r128原生圆弧对照的缓存key误用了请求128而非实际50，最终已修正为实际半径，不能引用该行79.1通道误差作对照结论。

## 最终源码的已执行结果

| 检查 | 实际结果／适用范围 |
| --- | --- |
| 捕获库常规回归 | `capture-regression.txt`：623通过，0失败，129 ignored；忽略项未计通过 |
| 圆角专项 | `shape-final.txt`：9通过，包含显式执行的GPU两项和配对成本探针；最大半径解析坐标误差0.001300px |
| 原遮罩／模糊／任务栏 | `mask-regression.txt` 2通过；`blurred-mask-regression.txt` 1通过；`taskbar-regression.txt` 2通过 |
| 源格式与HDR颜色合同 | `source-contract.txt`：6通过，含FP16颜色与关联alpha |
| 原视觉产品链 | `software/summary.json`：参考37、产品回放32、owner5、传输故障1通过；原生类型保护与100×70局部候选不回退 |
| 工具回归 | `tool-regression.txt`：66通过 |
| 导出成本 | 同进程交替顺序，10次暖机＋300对样本；r128／512×512：首批旧圆弧P95=32700ns，新形状20500ns；最终源码批次为46300ns／34600ns；仅本机此场景，不外推交互整帧时延 |
| D2D／导出栅格差异 | 新轮廓最大通道差5.87～10.26；同设备原生圆弧9.74～28.41。两者抗锯齿覆盖不保证逐字节等同；位置边界另以±0.03px独立检验，不拿通道差冒充几何误差 |

`render-final-verified/selection-r48.png`和`selection-r128.png`为已目视核对的真实GPU输出；后者请求128、在512×352选区实际限制为88，测试同时断言实际半径。较小半径0/1/2/16/48/128的遮罩图也保留。工程检查与构建状态以`verification.json`记载的退出码为准。

最终源码批次统一在`final-source/`：首次完整工程检查因新增GPU测试的嵌套if触发Clippy而退出1（根目录`engineering-check.*`保留），修正写法后重跑所有相关套件、工程检查和构建。以上根目录通过日志为中间批次事实；`final-source/software/summary.json`的源码清单及`verification.json`引用才是交付源码资格。最终GPU预览位于`final-source/render/`，所有旧渲染与失败均不覆盖。

源码、文档及JSON的暂存区空白检查通过（`whitespace-check.json`）。`raw-whitespace-check.txt`保留全量检查对原始libtest尾空格／末尾空行的提示；不改写任何原始日志消除这些提示。最终完整工程门`final-source/engineering.json`退出0，含产品／库／集成测试、Clippy、workspace检查和依赖方向检查。
