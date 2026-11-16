# 视觉候选固定像素图集

**事实，2026-11-16**：`visual-candidate-v4/` 来自仓库 `docs/design/visual-candidate-test-cases.html` 的“导出全部素材 ZIP”。共 60 个顶层合同、99 个边界／相似变体、119 张 PNG；不是 119 个独立真实场景。导出浏览器／DPR见 `manifest.json::generator`，HTML 源文件、ZIP、manifest 的 SHA-256 见 `dataset-lock.json`。仓库 `.gitattributes` 的 `* -text` 保持 JSON／PNG 的原始字节。

## 输入与真值

| 文件／内容 | 用法 |
| --- | --- |
| `*.png` | 纯图像或明确标记的阶段掩码；不含页面布局、查询标记或真值叠加。定位以图像固有尺寸为准，不读取页面显示尺寸 |
| `manifest.json::assets` | PNG 文件哈希、独立解码后的紧排 straight RGBA8 哈希、尺寸与颜色解释；必须同时核对文件与像素 |
| `manifest.json::cases` | 输入上下文、查询、先验几何真值、数值／事件／阶段合同；只给执行器，不传给检测算法 |
| `dataset-lock.json` | 固定输入身份及划分：本版全部为开发／确定性回归集，独立验收集为空；同源变体不得拆成独立置信样本 |

**推断／执行要求**：P2 算法函数仅接收 PNG 解码后的像素、读取范围和查询参数，不接收 DOM、标注、案例 ID 或 `expected_*`。执行器独立比较结果。涉及 owner、原生来源、异步确认的合同由相应阶段执行器读取显式上下文；这些条件不能从颜色推断。

**事实／覆盖重点**：Q01～Q16 固定其余条件、分别改变色差、渐变／真实边、细线、独立采样数、ROI读取范围、纹理、嵌套／交叉／重复、AA定位、原生融合、透明／数值、容量或时序。以 manifest 中各组 `title`、`comparison_member`、`queries` 和 `stage_vectors` 为准确合同；硬断言、待标定、阶段注入分开记账，后两者不能冒充端到端检测通过。

## 复验与重新生成

校验工具需要 Python 与 Pillow；本轮实际使用 Pillow 11.1.0。Pillow 只用于测试输入的独立解码，不是产品依赖；去掉独立解码便无法发现“PNG文件哈希正确而RGBA真值错误”的素材。HTML 生成器沿用现有 Canvas，不新增 UI 库或生成器。

```powershell
python tools/yueshot_visual_fixtures.py
python -m unittest discover -s tools/tests -p test_visual_candidate_fixtures.py -v
```

如需安装工具依赖：`python -m pip install Pillow==11.1.0`。上述第一条逐张解码／核对，第二条验证篡改、缺图、错误RGBA哈希、额外文件、重复冻结、非法导出包不会被误认通过；不执行检测算法。

重新生成时通过本地 HTTP 打开 HTML，等待页面输入自检通过，导出全部 ZIP，再冻结到**新版本目录**：

```powershell
python tools/yueshot_visual_fixtures.py --import-zip <导出的ZIP> --directory <新目录> --report <新证据JSON>
```

工具拒绝覆盖既有目录或证据；全部资产验证通过后才发布新目录。冻结工具解决浏览器字体／阴影环境变化导致回放输入漂移的问题；锁文件解决输入身份与开发／验收用途不清的问题。旧版本是否保留由其实际回归引用决定，不增加产品兼容路径。

**边界**：HTML 能生成算法开发所需的可控像素，且不要求真实控件参与；它不能证明真实宿主截图分布、UIA/MSAA 暴露或实际系统交互已通过。本版真实宿主样本为 0，独立验收为 0，算法运行状态为 `not_run`。图集可立即用于开发与阶段测试；设计 §8.4 的真实分层统计资格仍需另行采集。
