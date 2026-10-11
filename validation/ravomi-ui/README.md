# Ravomi 风格卡落地验收

日期：2026-10-11。范围：原生 Bevy / egui 共享主题与首页、React/Tauri 工作区、内嵌文字编辑颜色、应用名称与品牌图标入口。本次沿用已确认的 Logo 和头像，不重新生成角色。

## 实现与检查

- 浅色默认暖白 `#F8F7F4`，深色中性石墨；窗口背景不透明。主按钮与选中/焦点分别使用语义颜色，节点用户自定义颜色保留。
- 原生首页及 React 品牌区复用团子猫；共享菜单、文件标签、属性与输入表面同步主题。React 增加侧栏、Lucide 图标、胶囊主操作和清晰卡片。
- 原生首页在可用高度较小时提供侧栏滚动；高 UI 缩放下隐藏大幅品牌区，保留文件入口与内容区域。200% 截图确认导航不再绘制到面板外。
- React 覆盖深浅主题 × 100% / 150% / 200% × 1200 / 390 px 窗口。截图检查发现 CSS zoom 下装饰图挤压内容，已改为按实际容器宽度重排，增加主按钮和输入框不被容器裁切的回归断言。
- 键盘切换主题保留焦点，Tab 可到缩放控件；系统减少动态效果下 CSS 过渡关闭。真实桌面字体显示正常；IME 自动化仍是已有预编辑协议测试，不声称重新完成真实输入法人工验收。
- 图标导出 32 / 128 / 256 / 512 / 1024 px；检查 PNG 尺寸、引用路径及 32 px 双耳双眼识别。Linux 安装器、macOS 打包脚本和 Tauri 配置已引用新图标；未执行系统安装、macOS DMG 或 Windows 打包。旧应用 ID、状态存储位置、crate 名及 `.prg` 标识保留兼容。

## 本轮执行

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --release --workspace --locked` | 253 通过，26 个环境相关测试按原有标记忽略 |
| `cargo clippy --release --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cargo build --release --locked` | 通过，原生截图使用本轮构建 |
| `npm run build` / `npm run build:canvas-editor`（app 目录） | 通过 |
| `npm test`（app 目录） | 18 项通过；包含模拟 IPC 的草稿/取消/保存失败、预编辑协议与布局检查 |
| `npm exec tauri build -- --no-bundle`（app 目录） | Linux release 构建通过 |
| `xvfb-run -a env -u WAYLAND_DISPLAY python3 app/tests/native_smoke.py target/release/hm-app results/ravomi-ui/tauri-native.png` | 真实 WebKit → Tauri → Rust 中文编辑、临时 PRG 保存及重开通过 |
| 安装脚本 `bash -n`、`desktop-file-validate`、plist 解析、图标引用与文档本地路径 | 通过 |

测试在当前组合工作区执行；其他会话已有的原生画布探针和 MSDF 错误通路改动保留在未提交区，不纳入本次功能提交。没有新增依赖、修改用户文档或执行性能基准。

## 原生截图环境

原生 Bevy 检查使用 Linux、Bevy 0.19.1、Xvfb/X11、Mesa 26.2.3 llvmpipe Vulkan、1280 × 800，系统缩放 1；应用 UI 缩放分别为 1 / 1.5 / 2。通过 `--home --preview` 或 `--synthetic 8 --preview-tool properties --preview` 生成，状态文件使用隔离目录。初次 Xvfb 使用 Intel Vulkan 驱动时因缺少 DRI3 无法呈现；明确选用 `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json` 后通过。软件渲染截图只验证外观，不代表 GPU 性能或所有平台支持。

Tauri 使用本轮 release 二进制、真实 WebKitGTK 窗口与临时文档；Xvfb 报 DRI3 加速不可用警告，编辑/保存/重开正常。Chromium 截图中的“原始想法”为明确的模拟 IPC 测试数据，未模拟 Agent 执行。

## 截图

- [原生浅色首页](native-home-light.png)
- [原生深色首页，150%](native-home-dark-150.png)
- [原生浅色首页，200%](native-home-light-200.png)
- [原生画布与属性面板，200%](native-properties-200.png)
- [Tauri 真实原生编辑窗口](tauri-native.png)
- [React 浅色](react-light.png) / [深色](react-dark.png)
- [React 窄窗，200%](react-narrow-200.png)

本轮未接入 Agent 执行、未将对象列表变为空间画布、未实现全局 Bevy 减少动态效果设置；这些能力仍按 PRD 分阶段交付。
