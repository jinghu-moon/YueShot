# 另一台 Windows 电脑验收

最近一轮用户已确认登录自启、Explorer 重启托盘恢复及睡眠唤醒后的截图/保存通过，见 [2026-11-09 人工结果](acceptance/2026-11-09-beta-lifecycle-manual.json)。下表仍是每轮验收模板，不把上一轮结果自动应用到新构建或其他机器。

将验收包 ZIP 复制到另一台 Windows x64 电脑，先解压到可写目录。使用普通用户、非管理员 PowerShell，目标机无需 Rust、Python、Node 或 Visual Studio。请使用本渠道尚未安装 YueShot 的干净机器或新用户；脚本发现已有安装/进程会停止，不覆盖已有产品或强杀进程。记录电脑是否曾安装 VC++ 运行库，已有运行库的环境不算“干净运行库”资格。

在解压目录执行（仅对此进程使用执行策略参数，不修改系统设置）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\accept.ps1 -Mode Auto
```

验证安装包哈希 → 隔离目录安装 → 已安装负载哈希和渠道登记 → 运行库实际加载/配置及数据库 smoke → 同版本重装保留配置 → 卸载保留测试数据。结果为 `results/Auto.json`；失败时保留现场，不自动递归删除数据。

接着启动交互验收：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\accept.ps1 -Mode StartManual
```

程序数据只写入验收包 `results/data`。人工检查下表，并记录结果、系统版本、缩放、显卡及显示器数量；不要提供截图内容、剪贴板正文、账号或完整个人路径。

| 项目 | 操作及预期 | 状态 |
| --- | --- | --- |
| 普通截图 | F5 → 框选 → Enter 复制；Ctrl+S 打开保存面板并实际保存 | 待实测 |
| 长截图 | F7 → 框选松开 → 滚动采集，右侧预览 → Ctrl+S 实际保存；取消也正常 | 待实测 |
| 历史与贴图 | 保存内容出现在历史，原图打开、复制、贴图正常 | 待实测 |
| 关闭与重复启动 | 关闭主窗口后托盘/热键仍可用；从测试 EXE 使用相同 `--data-dir` 再启动，只唤醒原实例 | 待实测 |
| DPI | 记录可用缩放；安装界面、保存面板、截图边界无裁切，跨屏/负坐标正常 | 待实测 |
| 睡眠恢复 | 正常睡眠/唤醒后热键、截图及保存可用 | 待实测 |
| 自启/Explorer | 使用单独的临时用户或虚拟机验证真实登录自启及 Explorer 重启后的托盘恢复，不能用单元测试替代 | 待实测 |
| 保存失败 | 选择无写权限的目录或被占用目标，显示失败且不丢失已有文件；磁盘不足另用隔离环境验证 | 待实测 |

完成后通过程序“退出”按钮正常退出，再执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\accept.ps1 -Mode FinishManual
```

它检查安装负载、逐项记录人工会话数据哈希，再卸载测试副本，验证数据字节保留。若曾在隔离用户开启自启，应先在产品设置关闭；卸载也会清理该安装位置拥有的启动项。只返回 `Auto.json`、`StartManual.json`、`FinishManual.json` 与人工检查文字结果；不要发送整个 `results/data`。人工项目默认仍为 pending，自动脚本通过不代表全部设备验收通过。

代码签名按用户决策不启用。若 Windows 显示未知发布者，先核对发布来源；不要关闭系统安全设置。当前只是测试候选，不等于正式稳定发行。
