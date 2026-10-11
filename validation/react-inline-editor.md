# 同窗口 React 文字编辑验收

> 历史证据：本文保留 2026-10-10 的 Wry/React 输入适配环境、测试数和截图；不是 Ravomi 新 UI 或多 Agent 流程验收。新版本按 [DESIGN](../DESIGN.md)、[AI_UI_RULES](../AI_UI_RULES.md) 增量检查输入清晰度、焦点隔离、键盘退出、缩放与真实 IME；产品范围见 [PRD](../PRD.md)。这组编辑层测试不证明完整 React/Tauri 画布或 Agent 执行可用。

2026-10-10，Linux x86_64；Rust 1.98.1、Node 24.21.0、Bevy 0.19.1、Wry 0.57.0。原生截图使用 Xvfb 1600×1000，应用窗口 1280×800、系统比例 1×，Mesa 26.2.3 lavapipe Vulkan。软件渲染仅用于功能验收，不作为性能数据。

## 改动与职责

圆角渲染原先用已提交的文档尺寸覆盖编辑预览，造成文字/编辑轮廓与背景不一致。现在由临时 `NodePreviewSize` 向渲染系统提供尺寸，结束编辑清除；回归测试覆盖相机比例 0.25、1、4，确认不改变文档或历史。

`react-editor` 可选 feature 通过 Wry 把 React textarea 嵌入原生 Bevy 窗口。Rust 保留文档、撤销历史、字体排版与 GPU 画布；React 处理选择、光标、输入法与草稿测量。消息携带会话编号，旧会话被忽略，预编辑未结束时不接受提交。该实现没有迁移整个界面，Tauri 对象列表原型仍是独立入口，菜单和连线标题仍由 egui 处理。

Linux 使用 X11/XWayland；原生 Wayland 嵌入不支持。X11 子窗口透明合成不可靠，编辑区域使用节点的不透明底色。WebKit 释放 GLX 子窗口的特定错误按 Wry 官方 wgpu 示例处理并记录日志，避免错误归因到 winit 的下一次窗口操作。其他错误保留原有处理。

## 验证结果

- `cargo fmt --all --check`：通过。
- `cargo test --release --workspace --locked --features react-editor`：252 通过、26 忽略、0 失败。忽略项包含单独要求桌面/GPU、字体及 fixtures 的历史测试，不声称本次全部执行。
- `cargo clippy --release --workspace --all-targets --locked --features react-editor -- -D warnings`：通过。
- `cargo test --release --locked --no-default-features --lib`：67 通过、5 忽略，确认文档库及输入协议不要求 WebView/Bevy。
- `npm run build:canvas-editor` 与 `npx playwright test tests/canvas-editor.spec.ts`：8 项通过；覆盖组合输入期间 Enter、结束后的单次提交、缩放更新保留草稿和光标、Shift+Enter，以及深浅主题 × 1/1.5/2 倍文字缩放。
- `canvas_native_smoke.py`：深浅主题 × 100/150/200% UI 缩放共 6 次全部通过。真实 WebKit IPC 发送合成的 composition/input/keydown，Rust 拒绝提前提交并确认最终提交。截图检查文字与背景位置一致、无黑色编辑面板及超出节点的提示条。

原生复现命令（先按 app/README 构建）：

```sh
xvfb-run -a -s '-screen 0 1600x1000x24' env \
  -u WAYLAND_DISPLAY -u WAYLAND_SOCKET GDK_BACKEND=x11 \
  VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
  python3 app/tests/canvas_native_smoke.py \
  target/release/project-graph-bevy-canvas --ui-scale 1.5 --light
```

Vulkan ICD 路径按系统调整。Xvfb 无 DRI3，直接选择 Intel 硬件 ICD 会缺失可呈现 surface；本次显式选择 lavapipe。测试使用合成图，不写用户文档。Python 仅驱动测试，不参与画布运行。

| UI 缩放 | 深色 | 浅色 |
| --- | --- | --- |
| 100% | [截图](react-inline-editor/native-dark-1.png) | [截图](react-inline-editor/native-light-1.png) |
| 150% | [截图](react-inline-editor/native-dark-1.5.png) | [截图](react-inline-editor/native-light-1.5.png) |
| 200% | [截图](react-inline-editor/native-dark-2.png) | [截图](react-inline-editor/native-light-2.png) |

没有同一环境与同一文档的旧版截图，不能把用户原图与合成测试图当作严格前后对照。这里的 composition 事件由测试合成，尚未人工验收真实 Fcitx/IBus 候选窗、多显示器系统 DPI、Windows/macOS 或移动端。没有测量性能提升。
