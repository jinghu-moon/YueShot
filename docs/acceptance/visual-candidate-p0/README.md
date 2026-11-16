# 视觉候选 P0 执行记录

**事实，2026-11-16**：从 `2b88e591ca97c980ed976a4d2a7b64c713c1e335` 开始执行。当前批次只增加特征化测试、测试注入入口、证据采集脚本与文档；不改变产品行为。P0**基线阶段通过**（[阶段门记录](stage-gate.json)），未进入P1，未实现视觉检测器。不能将本批次的测试通过解释为旧缺陷已修复或视觉精度已达标。

## 已执行与证据

| 检查 | 实际结果 | 原始证据 |
| --- | --- | --- |
| 改动前捕获库／media／geometry | 560／8／10 通过；捕获库 88 ignored；退出码 0 | [改动前日志](pre-change-tests.txt)、[版本与环境](pre-change-environment.json) |
| 增补后捕获库／media／geometry | 571／8／10 通过；捕获库 90 ignored | [本轮定向批次摘要](stage-gate-run/summary.json)，每次命令、匹配数、退出码、被测源码与日志 SHA-256 分别记录；qualified-run/final-run/verified-run/html-fixtures-run为之前的成功记录，不覆盖历史计数 |
| 新增 native_baseline | 11 项常规测试＋1 项真实图形控制器回放；12 项回放均实际通过；另1项资源探针单列 | 定向批次中 `native-baseline` 与 `native_baseline_controller_preview_replacement_and_manual_path_trace`；其中含下表明确标识的缺陷特征化 |
| 定向原生／图形资格 | 9 个指定测试各匹配并通过 1 项；无全局热键／剪贴板写入 | 同批次独立日志；含上行图形控制器测试，不重复计算新增测试数量 |
| 完整工程门禁 | `cargo xtask check` 退出码 0；格式、工具／许可、相关测试、Clippy、feature 编译和依赖方向检查通过 | [本轮日志](engineering-check-stage-gate.txt)、[命令／退出码](engineering-check-stage-gate.json)；engineering-check-html-fixtures、engineering-check-final及之前记录保留，不冒充本轮日志 |
| 首次工程检查失败 | 新增测试 `chunks_exact(4)` 被 Clippy 拒绝，改用 `as_chunks::<4>()` 后重跑通过；未压制 lint | [首次日志](engineering-check-before-clippy-fix.txt)及同名 JSON；保留失败而非覆盖 |
| Demo 4.0.0 | 60 顶层合同、99 对照变体、119 资产哈希；23/23 合同／几何 oracle 自检通过；算法状态仍为 `not_run` | [历史合同摘要](demo-contracts.json)；本轮已导出全部PNG并独立解码，见下表；未运行算法 |
| 固定HTML开发图集 | 119/119张PNG文件及独立解码RGBA哈希一致；60顶层合同、99边界／相似变体；素材完整性反例6/6通过 | [图集说明](../../../apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate.md)、[解码报告](frozen-html-fixtures.json)、[反例测试日志](frozen-html-fixture-tests.txt)与[退出码](frozen-html-fixture-tests.json)；纯素材检查，算法not_run |
| 实捕／空闲基线 | 5次暖机＋100会话真实捕获／确认／取消；60s空闲，退出0；重复两批均通过生命周期断言 | [最新原始摘要](lifecycle-02/summary.json)、[逐会话事件](lifecycle-02/probe-events.json)、[进程采样](lifecycle-02/process-samples.json)、[派生统计与证据边界](lifecycle-analysis.json)；lifecycle-01保留首次基线 |
| 设备环境 | Windows 26100、i5-13600KF；WMI 枚举 RTX 4070 Ti SUPER 与 GameViewer 虚拟适配器 | [硬件环境](hardware-environment.json)；WMI 名称不证明每个 D3D11 测试实际选中了哪张卡，不能据此声称集显／独显成对资格 |

定向硬件测试为：上下文 owner 断言、上传纹理异步取色、FP16 区域原位读回、HDR 取色不改源、D2D 提示不导出、真实显示器捕获、300×200 区域读取、F5 worker 构建策略，以及真实图形控制器回放。SDR 构建中的 FP16 数值测试不等于 `hdr-capture` 实屏资格。常规运行的90个ignored中，本轮显式运行9个定向原生测试及1个生命周期探针，共10个不同测试；其余80个没有因此获得通过资格。完整工程门禁与定向批次的 feature 组合不同，不能直接相加测试计数。

Demo 浏览器唯一控制台错误为开发服务器未提供 `favicon.ico` 的 404，已随 `demo-contracts.json` 登记。页面脚本自检无错误。早期批次曾关闭浏览器／删除临时文件；本轮重新打开并从同一HTML导出纯图，浏览器取回的源码SHA-256与仓库文件一致。Playwright下载ZIP与页面快照仅作临时输出，不作为图集或提交输入；定向批次工作树快照保留当时事实。

`native-baseline-tests.*` 是增补第10项回放前的中间记录，不用于最终计数。原始summary保留执行当时的源码哈希；其后只更新了设计文档的现状／R04文字，当时的源码与证据完整性见`evidence-integrity.json`，它是20:23的历史快照，不证明此后改动；最新批次另有`stage-gate-run/summary.json`及`lifecycle-02/summary.json`的源码哈希，不回写历史证据。

## B01～B13 逐项状态

路径基准为 `apps/yue-shot/crates/yue-shot-capture/src/`。模块／函数名称可在原始日志定位。**“已覆盖”只针对所列断言，不代表同一编号的所有实屏要求完成。**

| 基线 | 本轮复用与增补 | 结论／缺口 |
| --- | --- | --- |
| B01 L2 | `geometry::tests`、yue-geometry；`window_detection/native_baseline.rs::native_baseline_hit_mapping_and_epoch_trace` | 已覆盖负原点、96/120/144/192 DPI 输入不二次缩放、半开共享边、24px/25px邻近边界和局部坐标 golden；真实混合 DPI 切屏待确认 |
| B02 L2 | `windows/providers.rs::tests`、media；真实区域读取／FP16 定向测试；控制器确认后的逐像素源贯穿 | 已覆盖源描述、generation、区域字节和既有 padding／FP16 用例；没有新增全图副本 |
| B03 L1 | 既有 capture_worker、refinement_worker、request gate 测试；`native_baseline_confirmation_gate_cancel_restart_and_closed_owner` | 已覆盖容量1邮箱、协作取消、第二会话、重复确认、旧请求和取消后换源；新增`capture_worker::native_baseline_cancel_restart_completion_permutations`，枚举重启后旧完成／新完成／drain的6种顺序×成功／失败2种载荷，共12组合。另以`native_baseline_restart_cross_worker_completion_permutations`枚举取消／重启后旧snapshot、hover、confirm、refinement与新confirm的5!＝120种controller到达顺序；每步检查第二源generation=99、选区／会话身份不被污染。这是明示事件模型的穷举，不能泛称任意OS调度全排列 |
| B04 L1 | 既有 snapshot/hit_test/model；规范化 `native-baseline.json::hit_mapping` | 已覆盖 Z／身份／epoch 的既有单测与新返回值 trace；golden 不包含 Rust 类型名，供 P1 对比 |
| B05 L1＋L3 | `windows/refinement_worker.rs::native_baseline_transport_adoption_matrix` 的 11 个组合；既有 uia/provider 单测 | 已覆盖具体／宽泛／不可用输入合并、MSAA更细／同框／超时、Text开关与按钮文字排除、祖先顺序；输入在 transport 边界注入，真实宿主 COM 采纳 trace 待采集，不能把 UIA 具体分支的注入当作实际调用选择已验证 |
| B06 L1 | `preview_layers` golden；真实 `OverlayController::refresh_preview_for_cursor/apply_refinement_result` 回放 | 已覆盖首次等待不闪窗口、无结果选窗、横向等待保持、横向替换、向父级退回的二次 dwell；使用实际 renderer layout，未复制控制器状态机 |
| B07 L1 | 既有 gesture/session；新按下／抖动／3-4-5阈值／松手／晚到 dwell 与控制器确认；实际 Shift 清理 | 已覆盖所列时序；Shift 遗留索引为确认的旧缺陷 K03，未修复 |
| B08 L1 | 控制器 `confirm_snap_preview → worker mailbox → on_detection_ready → apply_confirmation → publish_selection` | 已覆盖请求门、窗口关闭、不同 owner、Shift 后迟到、取消换源、输出字节；同 owner 换矩形竞态 K01 未修复 |
| B09 L1 | 既有 wheel／label／rings；`preview_layers` golden；控制器选层后插入／删除祖先路径 | 已记录可见停靠／计数／环、标签与矩形；手动几何漂移 K02 未修复，不能将当前索引保持视作未来正确合同 |
| B10 L2＋L3 | sampler 既有测试、真实 owner／异步取色／FP16源不变 | 本机定向资格通过；新视觉传输交错不存在，P3前另补；尚无该路径性能结论 |
| B11 L1＋L3 | 既有 `overlay/input.rs::tests` 中 Ctrl+S／Enter／F7 与保存像素测试；新增确认区域逐字节断言；D2D提示不导出实测 | 已覆盖模块／控制器输出源；真实 F5→Enter→保存／F6贴图／F7长截图完整桌面操作链待执行 |
| B12 L3 | 上述9项定向测试及硬件元数据 | 当前桌面定向通过；集显、不同DPI／主题宿主、设备移除、开发HDR实屏未获得资格 |
| B13 L1＋L3 | diagnostics reset／共享语义与既有worker shutdown单测；新增fixture显式停timer／worker，先释放renderer再销毁HWND | 语义和测试资源清理覆盖；100次真实捕获／60s空闲已实测两批，含private bytes／线程／DXGI实际适配器显存；隐藏控制器不派发可见窗口消息泵，不能当作完整UI空闲或升级后资源资格 |

## 已确认旧缺陷与后续正确合同

| ID | 事实：改动前实际行为、源码、复现 | 推断：后续修正目标 |
| --- | --- | --- |
| K01 / R04 | `windows/overlay/hover.rs::confirm_snap_preview/apply_confirmation` 只保留请求ID，回包从可变preview读取rect；同owner从 `[10,8,30,20]` 改为 `[4,3,50,40]` 后确认了后者。`native_baseline_known_defect_confirmation_uses_replaced_same_owner_rectangle` 经过真实邮箱／请求门复现 | 请求保存完整确认快照；迟到结果不得改变用户发起确认的矩形、source和owner。独立修复与前后差异，不能夹进P1原生等价结论 |
| K02 / R04 | `hover.rs::apply_refinement_result/selected_deep_level` 发布新path后沿用旧index。选中parent的index=1，删除该祖先后index=1指向inserted，实际预览从parent变为inserted；图形控制器回放已复现 | 若声明手动几何保持，按原几何重新定位；无法保持时明确合同，不能用索引巧合冒充保持。按设计P4的手动保持门独立验证 |
| K03 | `hover.rs::clear_snap_preview_for_modifier` 中 `deep_target.take().is_some() || deep_levels.take().is_some()` 短路，前者存在时后者不执行。图形控制器回放实测 Shift 后仍有index=1 | 清理两份状态均实际执行；另列行为修复和反向断言，不保留兼容分支 |

首次按“Shift 后无索引”的正确合同运行曾失败：[失败日志](known-defect-shift-before.txt)、[退出码101](known-defect-shift-before.json)。当前特征化测试明确断言旧缺陷仍可复现，目的是记录现状，**不是将错误行为认定为正确或把产品门降为现状**。修复时应替换对应旧缺陷断言、增加正确合同断言、保留此处旧日志；禁止用批量更新golden掩盖行为差异。

## 数据采集与划分协议 v1（规则冻结；真实资产尚未采集）

本批次已将HTML Demo导出并冻结为`tests/fixtures/visual-candidate-v4/`：119张PNG全部通过独立解码，身份见`dataset-lock.json`。用户要求优先用HTML绘制，算法阶段无需真实控件；所有导出案例均明确分配为开发／确定性回归集。真实开发集与独立真实验收集仍为0，真实复核人／图集／补充设备资格**待确认**。以下冻结真实采集与划分规则；实际资产名单随采集形成，在执行器首次读取该侧结果前封存，不能把规则冻结冒充资产已经采集。

| 项目 | 规则与待办 |
| --- | --- |
| 记录单位 | 每条保存场景簇ID、宿主及版本、页面／控件结构、主题、DPI、物理原点、分辨率、源格式／generation／RGBA或原格式哈希、指针、窗口Z／身份／原生结果、独立几何真值及可接受默认集合；保留负例与非目标标签 |
| 案例重点 | 同组只改一条件；优先文字压种子、ROI外大框、三层中层接边、无障碍不可用／具体原生对照、邻接与交叉、同色／微差色、渐变伪边／真实矩形、纹理伪矩形；不能只采集显眼卡片 |
| 分层 | H1 Chromium（原生可用与不可用分开）、H2 Electron、H3 Qt／自绘、H4原生；Mica／亚克力、深浅主题、125/150/200% DPI、负原点、4K/8K为交叉轴。按设计§8.2记录实际暴露，不以开关替代COM输出 |
| 划分（推断／冻结规则） | HTML现有全部案例只作开发／确定性回归，名单与manifest哈希已锁定。真实数据先分场景簇：稳定簇ID为`Hn/source_id/scene_key`，source_id与scene_key由采集清单在运行检测器前登记，同页面相邻点、主题、DPI、缩放、图像变换、派生版本沿用同簇ID。计算`SHA256(UTF8("yueshot-visual-real-split-v1/" + cluster_id))`，前8字节视为无符号大端整数，模5为0分独立验收，其余分开发（约20%/80%，不保证小样本比例）。跨宿主复用相同内容时使用同一内容簇并归同侧。禁止按算法结果改ID／换侧；每个声明宿主数量不足则继续按同规则采集，不重分现有数据。资产／标注哈希与名单在首次运行该侧结果前封存；P2不得读取验收资产及标注。实际真实簇ID／名单／封存哈希尚不存在，逐批如实补录 |
| 标注 | 检测器运行前标四边及容差、owner和资格、默认可接受集合、严格包含层级；合成整数0px、AA定位0px、AA端到端1px；真实清晰框初始≤1px，事先逐边标定。独立复核人和争议裁决记录待确认 |
| 数量与置信 | 遵守设计§8.4：每个声明总体的接受查询≥300、清晰正查询≥60、困难负查询≥300只是单项零失败下限；多重声明事前固定K／α，使用精确界。不能用同图重复或99个Q变体扩充独立样本量 |
| 事件合同 | T13/E08/E13等描述式变体在相应runner阶段转为显式可执行事件；未知操作报UnsupportedContract；P2纯阶段与P4融合交互分别记账；当前尚无算法runner，不宣称T/M/E/Q检测通过；输入校验入口为`python tools/yueshot_visual_fixtures.py`，算法只能接收像素与查询，标注由执行器独立消费 |

## 后续阶段门

**推断／阶段归属澄清（D13，设计§8.1、§9）**：P0验证拟改行为的L1/L2基线、本机已运行L3、资源参照与采集／划分规则；设计P0门明确要求“未运行硬件明确缺口”，并不要求P0提前取得P5的真实统计资格。B05 transport合并golden属于P0；真实COM暴露属于L3。B11输出／入口回放属于P0；真实F5/F7/保存/贴图全操作链按设计P4验收。真实独立复核、资产封存和分宿主样本数量按§8.2/§8.4在各自使用前过门，缺失时不得宣称P2真实效果或P5升级成功。此澄清不修改任何精度／资源阈值，不将未运行项改记通过。

**推断／执行约束**：P0的捕获邮箱12种排列、controller跨worker的120种到达顺序及隐藏控制器资源／空闲基线已补齐。B05真实宿主、B11完整操作链及真实采集／划分仍有缺口，按设计B矩阵记录其L3/P4/P5资格归属，不误称HTML纯算法输入必须依赖真实控件。[阶段门记录](stage-gate.json)核对当前源码、日志哈希、测试退出码及图集证据后判定P0基线门通过；这些后续资格缺口不是P0纯基线门失败；也没有证据支持跳过相应P4/P5门或提前开始默认视觉接入。K01～K03已定位，修复应独立于P1纯重构对比；不要求为了兼容旧缺陷保留分支。硬件缺口与独立真实真值不足分别记录，不以自动单测或合成图替代；P0不要求提前达到P5的统计样本量。

复跑命令（必须使用新目录，脚本拒绝覆盖历史证据）：

```powershell
pwsh -NoProfile -File tools/test-visual-candidate-baseline.ps1 -EvidenceDirectory docs/acceptance/visual-candidate-p0/next-run -Native
cargo xtask check
```

新增实体均为测试设施：规范化golden避免目标类型重构掩盖几何变化；控制器fixture／无外部IO端口确保经过真实路径且不写剪贴板／文件；`DetectionWorker::publish_for_replay`仅在`cfg(test)`编译，用于稳定地测试真实邮箱与请求门，避免旁造状态机；证据脚本防止0匹配假通过、覆盖失败日志或丢失命令／退出码。没有新增产品模块、依赖、兼容层或后端资源。

## 本轮资源观测的适用范围

**事实**：最新基线实际使用RTX 4070 Ti SUPER、3840×2160、144 DPI；100个暖样本捕获P50/P95为36.962/39.952ms，捕获＋确认＋取消P95为45.988ms（nearest-rank）。60s空闲期间进程CPU采样增量0.046875s，private bytes从59,527,168降至58,740,736，线程29降至25；DXGI local/nonlocal空闲首尾同为8,421,376/806,912字节。原始波动与局限保留在`lifecycle-analysis.json`，不隐藏首批更高的P95。

**推断／限制**：这些数字建立升级前参照，不证明资源无泄漏，也不是可见overlay首显／消息循环空闲时间。500ms进程采样会漏短峰，不能据此宣布精确峰值或空闲零CPU；该探针的`gatePassed`只表示生命周期断言与事件数通过。P4/P5须补真实消息循环与选定视觉后端的成对测量。
