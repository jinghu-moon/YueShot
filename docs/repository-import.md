# 独立源码首次导入

2026-11-08。本仓库由 SnapClip 工作区中 YueShot 的已验证源码准备，保留 YueShot、真实共享能力、受控组件/GPUI 快照、测试/交付工具与原始许可材料。没有导入 SnapClip 应用及其 model/history/clipboard 私有 crate。

来源候选为 0.1.2-beta-lifecycle，其对应源码归档 SHA-256 为 dceb1195e886cd96e014484e1f5a1fa29e807e258f21e864cab4c59602eff436，源码清单 buildId 为 0d15c05b049ed9621d129e63e51a5184ad60d830a6db7e82905b9bff79dc4cb3。候选 EXE 的 SHA-256 为 2ce20c19835858b1c9cffbbfcd4795ffc6e242952d6a0f14c2bb8f01cf542f09。这些标识关联原候选，不冒充本仓库后来新增文档/配置后的构建标识。

从来源移除 4 个 SnapClip workspace 成员和 6 个专属外部依赖；剩余锁文件包记录不变。默认 YueShot 的 404 个 normal/build 依赖及 features 逐项一致，16 个显式成员保留。改用 YueShot README/dev 别名，检查脚本根据实际 workspace 识别现有消费者；原工作区的 SnapClip 回归仍运行，独立仓库不调用不存在的产品。

导入前独立源码完整 cargo xtask check 与原工作区回归均通过，有限凭据模式检查未发现结果。待提交 Git index 曾导出到另一空目录并逐文件核对，包含上游 ignore 但构建所需的文件；gitattributes 保留受控快照和许可原文字节。

用户已明确授权上传，并自行将 jinghu-moon/YueShot 转为公开仓库。导入不创建正式产品标签、GitHub Release 或生产更新密钥。远程 CI 与后续运行结果应绑定具体提交，不以本地缓存验证代替干净 runner。

受控快照来源、原始哈希、排除和本地适配位于 vendor/yue-ui/snapshot.json；codec 上游源码及补丁材料位于 vendor 和 tools/codec-bench。第三方权利不因移动仓库改变。
