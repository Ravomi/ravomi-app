# 移动节点的静谧反馈

2026-10-12。原实现每次移动发射两枚白色方粒，并随机向后飞散；连续拖动会形成明显散点轨迹。现改为每个节点一条贴近运动尾缘的短淡光，复用主题雾蓝紫强调色，同一节点的反馈直接更新，停止后约 0.17 秒淡出。具体尺寸和透明度见 [设计规范](../DESIGN.md#当前动效与迁移说明)。

## 验证

- `cargo fmt --all --check` 通过。
- `cargo test --workspace --release --locked` 通过。
- `cargo clippy --workspace --all-targets --release --locked -- -D warnings` 通过。
- 回归覆盖连续 200 次移动只保留一条反馈、反向移动更新位置、多节点分别合并、缩放后屏幕宽度与范围有界、低透明度及停止后清理。

使用已安装的 `~/.local/bin/graplu`，在独立 Xvfb `:93`、1280×800、1×、llvmpipe/Vulkan 窗口中，通过 XTest 创建 `test` 文本块，按相同路径从 `(640,400)` 拖至 `(900,400)`，在释放前截图，释放 0.5 秒后再次截图。此项验证实际窗口交互和绘制，不是 GPU 性能测试，也不代表所有系统缩放均已验收。

- [修改前：白色散点向后飞散](movement-feedback/before.png)
- [修改后：反馈贴近节点边缘](movement-feedback/after.png)
- [停止后：无残留](movement-feedback/idle.png)

修改后拖尾区域 `x=500..794, y=280..519` 没有脱离节点的非背景像素；旧版相同区域可见散点。测试未保存或修改用户项目。已按 Linux 安装脚本更新本机 Ravomi，安装文件与构建文件 SHA-256 一致：`08a7ccb46bfdfc01429339d8a0e28a0b49bf3a8eae90b0fab0bcce369c548f60`。
