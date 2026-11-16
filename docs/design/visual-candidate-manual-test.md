# YueShot 手动测试台

打开 [visual-candidate-manual-test.html](visual-candidate-manual-test.html)。可在浏览器直接打开本地文件；页面按相对路径读取仓库已冻结的PNG，需保留原仓库目录结构，没有网络字体、CDN或接口依赖。

如需HTTP访问，在仓库根目录执行：

```powershell
python -m http.server 8878 --bind 127.0.0.1 --directory D:/100_Projects/110_Daily/YueShot
```

然后打开 <http://127.0.0.1:8878/docs/design/visual-candidate-manual-test.html>。

1. 启动当前构建的 YueShot。先选 T01／T06／T08／T21，再到“边界与相似对照”重点试 Q01／Q02／Q03／Q07／Q13～Q16。
2. 默认按DPR补偿显示尺寸，目标为一源像素对应一屏幕像素，并将画布起点对齐屏幕像素。浏览器缩放／DPI／实际光栅化仍须登记；“适应可见宽度”只用于体验，不用于细边精度声明。
3. 点击“专注测试”，隐藏页面工具与参考边；按 YueShot 的实际截图快捷键（默认 F5），停留、滚轮选层、确认，测试 Shift 手动选区、取消及第二会话。按 Esc 或右下按钮恢复测试台。参考边与查询点只用于核对，不应留在受测截图中。
4. 用 YueShot 实际复制／保存／桌面贴图。在“粘贴与保存核对”按 Ctrl+V，或拖入保存的文件，查看尺寸与内容。页面的图片预览不等于执行了桌面贴图，图片只在页面内存中保留，最多6张。
5. “真实网页控件”提供真实输入框、按钮、文字、Image及Canvas，观察原生与视觉的实际融合；不会按标签伪造原生结果。
6. “长截图测试”提供01～18段连续正文与结尾END，没有吸顶元素或正文内部滚动条；用默认 F7（以实际配置为准）手动框住正文列并滚动，检查缺段、重复、保存及取消。
7. 在“记录测试结果”填版本、环境、案例ID与失败复现，导出JSON。只有人工选择才变为通过，未执行项不会自动标绿；图片不进入报告。浏览器禁用本地存储时仍可导出本次记录。

素材共31组／114张，直接复用`apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate-v4.1/`。页面内嵌的选择索引来自该目录manifest，只保留ID、说明、尺寸、标注与查询点；如果修改素材版本，应同步核对索引与锁文件。没有修改原Demo、PNG、算法参数或产品代码。

Q02弱对比、Q03部分断边、Q11等校准变体不强行判通过／失败。Q04/Q12/E07的PNG边缘不是冻结屏幕源边缘；Q05的读权限、Q06内部采样、Q09注入的原生上下文也不能由本页复现。页面会显示这些限制，不能用素材标签替代测试输入。页面容器／整张素材被选中不等于内部目标命中。正式合同及可见验收步骤见[设计文档](../visual-candidate-pipeline.md)与[V01～V07清单](../acceptance/visual-candidate-p4/visible-acceptance-checklist.md)。

浏览器验证证据在`docs/acceptance/visual-candidate-manual-test/`；它只证明测试台可操作，不代表已替用户完成YueShot真机验收。
