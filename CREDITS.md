# 贡献者与致谢

Ravomi 是 Arcraven 的多 Agent 画布产品，曾用名 H&M / Huginn & Muninn，在 Graplu 画布原型基础上继续开发。公开产品仓库为 [Ravomi/ravomi-app](https://github.com/Ravomi/ravomi-app)，此前开发仓库为 [Wang-Yang-source/H-M](https://github.com/Wang-Yang-source/H-M)。Graplu 阶段属于 graphif 项目体系，相关社区贡献及上游来源保留如下。

产品与能力状态见 [README](README.md) 和 [PRD](PRD.md)；UI/UX 与 AI 实现约束见 [DESIGN](DESIGN.md) 和 [AI_UI_RULES](AI_UI_RULES.md)。本文件记录贡献和来源，不将上游界面或历史配色列为 Ravomi 的现行设计要求。

## 社区贡献

React/Tauri 迁移入口使用 React / React DOM（MIT）、Vite（MIT）、TypeScript（Apache-2.0）、Tauri 与官方 dialog 插件（MIT 或 Apache-2.0），前端验收使用 Playwright（Apache-2.0）。直接依赖固定版本，完整解析版本见 `app/package-lock.json` 和根 `Cargo.lock`；分发时保留各依赖自带版权与许可。原有 Rust 核心及 vendored 来源不变。

感谢参与代码、设计、文档、翻译、测试和问题反馈的贡献者。Ravomi 后续贡献以 [提交记录](https://github.com/Ravomi/ravomi-app/commits/main/) 和 Pull Request 为依据。新仓库从完整项目快照开始，迁移前贡献仍可查阅 [Graplu 历史提交](https://github.com/graphif/Graplu/commits/main/) 和 [原仓库贡献者](https://github.com/graphif/Graplu/graphs/contributors)，不因重建历史而改变归属。

Graplu 从 Project Graph 的 Bevy 原型迁入，迁移前的贡献不能仅凭本仓库的提交数量判断。原型来源、迁移基线和历史边界见 [迁移说明](MIGRATION.md)。

欢迎通过 Pull Request 补充遗漏的贡献与来源；设计、文档、翻译、测试等非代码贡献同样值得记录。

## 上游项目与第三方致谢

| 项目或资源 | 在 Ravomi 及其基线中的用途 | 来源与说明 |
| --- | --- | --- |
| Project Graph 及其社区 | Bevy 原型来源、节点图交互、`.prg` 格式与教程参考 | [迁移说明](MIGRATION.md) |
| Bevy 与 bevy_egui / egui | 应用调度、渲染、输入与界面 | [开发指南](DEVELOPMENT.md)、[依赖清单](Cargo.toml) |
| Wry / gtk-rs / x11-dl | 同窗口 React 文字输入的原生 WebView 与 Linux GTK/X11 事件适配 | Wry 0.57.0（MIT OR Apache-2.0）、gtk 0.18.2（MIT）、x11-dl 2.21.0（MIT）；实现参考 Wry 的 wgpu 子窗口示例，依赖版本见 Cargo.lock |
| Kayak UI / kayak_font_msdf | MSDF 字形生成器的上游实现与本地适配 | [原作者、来源及本地修改](vendor/kayak_font_msdf/UPSTREAM.md) |
| Viktor Chlumský 的 msdfgen | MSDF 算法来源及修复参考 | [MSDF 来源说明](vendor/kayak_font_msdf/UPSTREAM.md) |
| Winit 及其贡献者 | 桌面窗口与输入，含本地 Wayland 补丁 | [补丁来源说明](vendor/winit/UPSTREAM-PATCH.md) |
| Lucide / Feather | 界面图标 | [图标来源与许可](assets/icons/README.md) |
| Catppuccin | 既有 Bevy/Graplu 界面与旧品牌配色参考 | [品牌资源说明](assets/branding/BRAND.md)；新界面规范另见 [DESIGN](DESIGN.md) |
| Google Noto Emoji | 保留彩色的 CBDT emoji 字体 | [版本、来源与 OFL 许可](assets/fonts/README.md) |
| RaTeX / KaTeX | Rust 原生公式解析与排版、MSDF 数学字形字体 | RaTeX crates 固定 0.1.14，MIT；[字体来源与 OFL 许可](assets/fonts/README.md) |
| low-level-dev-skills | 项目级 x86 汇编分析技能 | [固定来源与本地修正](.agents/skills/assembly-x86/UPSTREAM.md)，保留 MIT 许可 |

这是一份主要来源与致谢索引，不是完整依赖或许可证清单。直接依赖见 [Cargo.toml](Cargo.toml)，锁定的依赖版本见 [Cargo.lock](Cargo.lock)。

涂鸦的 `.prg` 字段、压力宽度与二次曲线规则参考 Project Graph master 的 [PenStroke](https://github.com/graphif/project-graph/blob/1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0/app/src/core/stage/stageObject/entity/PenStroke.tsx) 和 [curveRenderer](https://github.com/graphif/project-graph/blob/1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0/app/src/core/render/canvas2d/basicRenderer/curveRenderer.tsx)；Graplu 使用缓存的 Lyon 网格实现相同曲线路径。

弧线连线的偏移量、圆弧方向、矩形裁剪和文字位置语义参考同一 master 提交的 [ArcEdge](https://github.com/graphif/project-graph/blob/1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0/app/src/core/stage/stageObject/association/ArcEdge.tsx) 与 [renderArcEdge](https://github.com/graphif/project-graph/blob/1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0/app/src/core/render/canvas2d/entityRenderer/edge/renderArcEdge.tsx)。

## 许可与署名

代码协作 MVP 内置 [Pi Agent Core / Pi AI](https://github.com/earendil-works/pi/tree/7c10bd4337495ee613f2224843ecdf349b80d1df) 1.0.4，原作者 Mario Zechner，MIT。模块经 Bun 1.3.14 编译进入 Ravomi 的私有引擎，不依赖外部 Pi CLI。上游源码未修改；锁文件、适配边界、Bun 及 Pi 许可证与构建生成的第三方声明说明见 [内置引擎来源](app/agent-engine/README.md)。

连线预览方向与成功反馈参考 Project Graph master `1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0` 的 `ControllerNodeConnection.tsx` 和 `edge/concrete/SymmetryCurveEdgeRenderer.tsx`。Graplu 复用原生曲线与效果队列，使用主题强调色，并沿实际曲线播放短暂流动。

分组概览的触发比例、覆盖透明度与标题拟合规则参考 Project Graph master `1fb1b5cc` 的 `SectionMethods`、`SectionRenderer` 和 `textRenderer`；退出阈值防抖、编辑恢复与标题避让参考本地 `project-graph-godot/src/group_overview.gd`（2026-10-09 工作区版本）。Graplu 保留自适应大标题，不移植 Godot 的固定字号上限、预览截图队列或独立物理对象隐藏机制。

Ravomi 沿用 [GPL-3.0-only](LICENSE)，第三方组件保留各自的原作者、版权声明和许可证。产品重命名及新 UI/UX 方向不改变历史贡献归属或第三方许可。

个人贡献优先通过提交和 Pull Request 记录。对独立模块或第三方补丁需要额外说明时，在对应来源文档中注明实际贡献范围，不为每个函数重复添加个人署名。
