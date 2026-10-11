# Ravomi 迁移沿革与兼容边界

Ravomi 是 Arcraven 的多 Agent 画布产品，产品定义与阶段状态以 [PRD](PRD.md) 为准。本文记录代码来源、技术迁移和兼容性，不承担当前 UI/UX 规范；界面实现读取 [DESIGN](DESIGN.md) 与 [AI_UI_RULES](AI_UI_RULES.md)，资源来源与使用状态读取 [品牌规范](assets/branding/BRAND.md)。

## 当前入口

产品已于 2026-10-11 从 H&M（Huginn & Muninn）更名为 Ravomi。公开产品仓库为 [Ravomi/ravomi-app](https://github.com/Ravomi/ravomi-app)，此前开发仓库为 `Wang-Yang-source/H-M`。首次公开发布使用已提交内容的完整快照，本地未提交的开发改动不纳入该快照；原有开发分支与历史保留在原工作区。crate、程序内部标识、用户数据目录与 `.prg` 格式继续兼容。

仓库现在同时包含 Rust/Bevy 原生画布基线、实际存在的 React/Tauri 迁移入口，以及 Wry/React 同窗口文字输入适配。各自的命令、能力和平台验证边界见 [开发指南](DEVELOPMENT.md) 与 [app/README.md](app/README.md)。React/Tauri 尚未替代完整 Bevy 画布；AI 执行、Agent 编排与上下文交接仍属产品规划。规范中的团子猫、素净工作区和多 Agent 工作流不会因文档更新自动成为已实现能力。

## 2026-10-09：Bevy 原型迁入 Graplu

以下为当时迁移的历史记录；其中 Graplu 名称、仓库和“没有迁入”的目录范围仅适用于该次提取，不描述当前仓库的全部内容。

2026-10-09，按用户选择，将 Bevy 原型提取为独立 Rust 项目，放到 Graplu 根目录。

- 来源仓库：`graphif/project-graph`。
- 来源分支：本地 `bevy`；原 GitHub 远端分支已按用户要求删除，不会由本次迁移重建。
- 稳定基线：`63020f03be6717fbb6d09a1f30ee7e6c273afa79`。
- 提取目录：`prototypes/bevy-canvas/` → 本仓库根目录。
- 公共仓库：<https://github.com/graphif/Graplu>，默认发布分支 `main`。

本仓库以基线代码快照开始独立提交历史。原分支历史仍保留在原本地仓库，未删除、重写或混入这里。React/Tauri 的 `app/`、`packages/`、其他实验目录和工作区未跟踪文件没有迁入。

Graplu 目录内原有 PRD 保留，仅更新开头的迁移状态与代码路径边界。原型 README、运行脚本、测试、验证记录、Linux 缩略图工具、Kayak MSDF 字体子库一并迁入。补充 GPL-3.0-only 的根 LICENSE，并保留第三方 MIT/Apache 等原始许可和来源说明。

迁移保持 Rust crate、程序二进制、embedded shader 路径及 PRG MIME 标识不变，避免把品牌重命名和行为修改混在迁移中。产品名称使用 Graplu；内部标识若要修改，应在独立提交中同步测试与打包入口。

已保存的性能数字与 `validation/` 图片来自原型的历史验证，不代表新目录重新测速。迁移后在 Graplu 根目录完成 43 项 Rust 测试（含全部 ignored、GPU/字体与教程文件回归）和 25 项 Python 缩略图测试；Cargo fmt、Clippy、Ruff 及 shell 语法检查通过。未重新进行帧耗时基准。

源工作区尚未完成的分组与 Wayland 触控板改动，在基线发布后复制为新工作区的未提交改动，由负责会话继续实现、验证并分别提交。这些改动不包含在首次迁移提交中。编译缓存和教程 fixtures 只保留在本地，不上传 GitHub。
