# React / Tauri 迁移入口验收

> 历史证据：本文保留 2026-10-10 的迁移初始版本、测试数和截图，不作为 Ravomi 新设计的完成证明。后续按 [DESIGN](../DESIGN.md)、[AI_UI_RULES](../AI_UI_RULES.md) 验收清晰的画布、状态与人工介入入口，并依据 [PRD](../PRD.md) 区分已实现和规划能力。空间画布、Agent 并行执行和上下文交接必须补充各自的真实端到端证据，不能沿用本次模拟 IPC 结果。

日期：2026-10-10。新入口 `app/`，Rust 命令接口 `src/session.rs`。这是新对象视图的初始基线，没有可直接比较的同页面改动前截图；不与历史 Bevy 画布做逐像素对比。

## 环境与结果

- Linux 7.2.9-200.fc44.x86_64，Rust 1.98.1，Node.js 24.21.0。
- Tauri 2.12.2，WebKitGTK 2.54.1，GTK 3.24.52。
- 原生冒烟测试使用 Xvfb / X11 + WebKitWebDriver，1200 × 800；出现 DRI3 不可用提示，因此只作为功能验证，不作为 GPU 性能或原生 Wayland 的证据。
- 前端使用 Playwright Chromium 156，1200 × 800 和 390 × 844 视口；深浅主题 × 100% / 150% / 200% UI 缩放共 12 组布局截图。模拟 IPC 只用于前端草稿、错误和布局检查。
- 已人工查看代表截图：文字可读，长中文文件名截断，窄窗口属性区下移；200% 窄窗口顶部操作换行，无水平页面溢出。缩放使用 CSS zoom，不能当作操作系统分数 DPI 验证。

| 验证 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --workspace --locked` | 250 通过，26 个原有依赖 GPU/字体/fixture 的测试保持忽略 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cd app && npm run build` | TypeScript 与生产构建通过 |
| `cd app && npm test` | 9 通过；保存先应用草稿、磁盘错误保留编辑、取消替换保留文字、浏览器操作禁用和布局矩阵 |
| `cd app && npm run tauri build -- --debug --no-bundle` | 通过，生成 `target/debug/hm-app`，嵌入生产前端 |
| 原生冒烟 | React 按钮创建节点、文字输入与应用 → 真实 Tauri IPC → Rust 保存 `.prg` → 新建后重新打开，中文一致 |

首次 debug workspace 回归发现 4 个原 egui 测试丢弃未处理 `TexturesDelta` 的断言失败。单独提交修复测试清理，并同步处理同模式的一个 ignored 测试；不修改生产渲染。重跑上述 workspace 检查全部通过，未通过关闭 debug 断言绕过问题。

## 原生冒烟复现

系统需安装 `WebKitWebDriver`、`Xvfb` 和 `xauth`。仓库根目录执行：

```sh
cd app
npm ci
npm run tauri build -- --debug --no-bundle
cd ..
xvfb-run -a python3 app/tests/native_smoke.py target/debug/hm-app results/hm-tauri-native.png
```

脚本使用临时目录创建文档并清理，只进行测试自动化，不参与应用运行或渲染。原生对话框选择文件未自动化；保存/重开通过真实 IPC 指向临时文档。

## 截图

- [原生 WebKit，深色 100%，真实中文编辑](react-tauri/native-dark.png)
- [Chromium，浅色 100%，长中文文件名](react-tauri/light-desktop.png)
- [Chromium，窄窗口深色 200%](react-tauri/dark-narrow-200.png)

## 边界

此版本是对象列表与普通文字编辑原型。空间画布、图片/公式/连线绘制、触控、自动恢复、多文档与 Agent 执行未迁移；不能用这组测试宣称原 Bevy 功能对等或性能改进。原生窗口关闭确认与文件对话框仍需桌面人工验收，中文内容测试不替代真实 IME 组合输入测试。

Windows、macOS、Android、iOS 尚未构建验收。浏览器仅预览 UI，Rust WASM/服务端和浏览器存储未接入。未生成安装包，也未发布。
