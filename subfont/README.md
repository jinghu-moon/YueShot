# YueShot 覆盖层字体

GPUI 窗口沿用系统字体。原生覆盖层嵌入两份字体子集，不要求用户安装字体：

| 字体 | 来源 | 派生处理 | 许可 |
| --- | --- | --- | --- |
| Noto Sans SC | Google Fonts 固定提交，`assets/font/NotoSansSC[wght].ttf` | 保留现有界面字符，固定 Regular（wght=400），调整静态字体名称 | SIL OFL 1.1；保留名为 `Source`，派生家族名不使用该名称 |
| Fira Code 6.002 | 官方 6.2 release，`assets/font/FiraCode-Regular.ttf` | 仅保留 `number-chars.txt` 的数字、单位和格式字符 | SIL OFL 1.1 |

许可原文、来源 URL、源文件与子集 SHA-256 位于 `tools/delivery/licenses/`。两份子集都保留版权、许可说明及许可 URL，派生字体继续使用 OFL。原来的 HarmonyOS 子集已移除。

重建命令（仓库根目录）：

```powershell
python -m pip install -r subfont/requirements.txt
powershell -NoProfile -ExecutionPolicy Bypass -File subfont/subset.ps1
python subfont/build_number_font.py
python tools/yueshot_assets.py
cargo test -p yue-shot-capture --lib subset
cargo test -p yue-shot-overlay-ui --lib
```

`subset.ps1` 先运行明确指定的 Rust codegen 测试，从现有绘制字符串生成 `drawn-text.txt`。生成器拒绝过期字符串清单、源文件哈希变化及丢失字符/许可元数据；固定 fontTools 版本并保留源时间戳，重复生成结果应一致。实际嵌入的文件位于 `apps/yue-shot/crates/yue-shot-overlay-ui/assets/`，`subfont/` 保留相同生成产物供审查。

源字体或界面字形变化时，应先审查来源、许可及生成差异，再更新 `sources.json` 中的受审查哈希，不能为绕过失败直接替换哈希。普通构建和 CI 使用已生成子集，无需联网获取字体或安装 fontTools。发布工具额外导出字体/图标许可文本，但该检查不代表完整应用许可审查已完成；禁用的 vendor 字体源码分发仍需单独核实。
