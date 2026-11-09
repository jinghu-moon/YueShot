# Beta 更新密钥备份与恢复演练

2026-11-09。状态：用户指定 U 盘并授权本机备份与恢复，文件复制、写盘刷新和字节一致性校验已通过；恢复副本签署/验签、物理拔插后复读仍待完成。该演练不修改客户端公钥、现有签名清单或公开更新源，不生成新生产密钥。

## 要保存什么

- 已加密的私钥 `D:\YueShot-Release\beta.key`。
- 对应公钥 `D:\YueShot-Release\beta.pub`，以及当前客户端信任的公钥字符串。
- 密码由用户单独保管，不写进备份说明、终端命令参数、仓库、聊天或 CI。

备份保存到与当前电脑独立的受控介质，例如离线移动存储；同一块 D 盘的另一个目录只是副本，不能作为设备损坏后的独立备份。保留私钥现有密码保护。用户实际备份介质路径仅记录于本机演练材料，不加入公开文档。

默认由用户操作私钥备份；用户明确授权后，助手可以在本机将加密文件作为不透明字节复制、刷新写盘并比较哈希，不输出内容或私钥哈希、不上传。密码仅由用户输入，助手不执行密码输入。本次用户已明确指定 U 盘并授权上述本机复制和校验。

## 本机材料

已准备 `target/yueshot-key-recovery/beta-20261109/`：

- `challenge.txt`：明确标为恢复演练的公开消息，不是更新清单，不能用于升级。
- `beta.pub`：当前 beta 客户端内置公钥的副本。
- `expected.json`：上述两份公开材料的 SHA-256。
- `READ-ME.md`：本操作说明的副本。

该目录没有私钥。用户授权复制后的设备类型、校验结果及本机路径记录于同目录 `usb-recovery-evidence.json`，不记录私钥内容或私钥哈希。预期公钥为：

```text
RWRPrtKhF/f6lRgLLGrdWRDJ7nekzqDkFWKoD8QPOJrsqWoRcMNhR7ZJ
```

## 操作

1. 用户自行将 `beta.key` 与 `beta.pub` 复制到独立备份介质，保持原文件不变。备份后断开介质，再重新接入，确认文件可读。
2. 从该介质把这两份备份恢复到本机的**新目录**，例如 `D:\YueShot-Release\recovery-20261109`。恢复目录必须在仓库和 target 之外；不要覆盖或删除当前在用的 `beta.key`。只从当前原文件签署不算恢复演练。
3. 在自己的 PowerShell 中执行以下命令；示例假定上一步恢复到上述目录。如使用其他目录，只改 `-s` 后的恢复私钥路径。密码仅在 Minisign 提示中输入。

```powershell
Set-Location D:\100_Projects\110_Daily\YueShot

& '.\target\update-tools\minisign-win64\x86_64\minisign.exe' -S -s 'D:\YueShot-Release\recovery-20261109\beta.key' -m '.\target\yueshot-key-recovery\beta-20261109\challenge.txt' -x '.\target\yueshot-key-recovery\beta-20261109\challenge.txt.minisig'
```

4. 上一条命令成功后，使用客户端信任的**原公钥**验证恢复私钥生成的签名，不能只使用恢复介质附带的公钥来证明身份。

```powershell
& '.\target\update-tools\minisign-win64\x86_64\minisign.exe' -V -P 'RWRPrtKhF/f6lRgLLGrdWRDJ7nekzqDkFWKoD8QPOJrsqWoRcMNhR7ZJ' -m '.\target\yueshot-key-recovery\beta-20261109\challenge.txt' -x '.\target\yueshot-key-recovery\beta-20261109\challenge.txt.minisig'
```

看到 `Signature and comment signature verified` 后，回复“已从独立介质恢复并验签通过”。密码错误或验签失败时保留现有材料并报告错误，不重新生成、替换生产密钥或改客户端信任根。

若材料目录已有一次成功签名，不重复覆盖它；需要重新演练时使用新的公开 challenge 和输出目录。本次恢复成功也不等于已经验证密钥轮换或密钥丢失后的发行补救。

## 完成证据

完成必须同时具有：使用独立介质中的备份恢复的证据、恢复副本成功解密并签署、签名通过客户端原公钥验证。助手可以读取公开的 challenge 与签名复核，不能根据文件存在就认定备份和恢复完成。物理拔插后的可读性另记，不能用刷新写盘或即时复读冒充实际拔插。

此演练不会上传或发布任何内容，也不需要重新签署 0.1.3 更新清单。原私钥和恢复副本均继续由用户保管，测试结束后由用户按其存储策略处理恢复副本。
