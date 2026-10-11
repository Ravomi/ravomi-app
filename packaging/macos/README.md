# Ravomi · Apple Silicon DMG

此流程打包现有 Bevy 原生画布，不代表 React/Tauri 入口、完整多 Agent 能力或所有目标平台已经发布。产品状态见 [README](../../README.md) 和 [PRD](../../PRD.md)，应用层迁移见 [app/README](../../app/README.md)。

发布资源按 [BRAND](../../assets/branding/BRAND.md) 核对；界面验收按 [DESIGN](../../DESIGN.md) 与 [AI_UI_RULES](../../AI_UI_RULES.md) 执行。新团子猫强调简单轮廓和局部柔光，安装图标需单独检查小尺寸识别；窗口文字、控件和连线保持清晰。以下已有构建和截图记录不是新视觉规范的验收结果。

在 Apple Silicon Mac 上运行，需要 Xcode 命令行工具和 Rust >=1.95：

```bash
bash packaging/macos/package.sh
```

构建目标固定为 `aarch64-apple-darwin`，不启用特定 CPU 的指令集；适用于包括 M5 在内的 Apple Silicon。最低部署版本设为 macOS 13。输出位于 `target/macos-package/`，包含 DMG 和 SHA-256 文件。

应用包包含原生可执行文件、图标、许可证与贡献者说明，DMG 提供拖入 Applications 的快捷方式。图标 PNG 当前从 `assets/branding/graplu-icon.svg` 导出，属于兼容资源；后续品牌替换必须同步源资源、PNG 和打包配置，并重新验证产物，不能仅改本文宣称已经采用新 Logo。

默认采用 ad-hoc 签名，脚本校验应用签名、Info.plist 和 DMG。未配置 Developer ID 签名或 Apple 公证。运行时使用 macOS 自带的 `Hiragino Sans GB.ttc`，也可通过 `--font` 指定其他字体。

2026-10-10 在 macOS 26.6.2 / Apple M4 / Metal 上验证原生启动和中文节点图离屏渲染；镜像内 `.app` 是 arm64 Mach-O，只链接系统动态库。测试机处于锁屏状态，窗口截图为空白，未据此声称交互界面验收通过。尚未在 M5 实机验证。

![Metal 离屏渲染](../../validation/macos-metal-offscreen.png)
