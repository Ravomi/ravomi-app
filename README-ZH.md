<div align="center">

[English](README.md) | **中文**

<img src="assets/branding/ravomi-logo.png" alt="Ravomi：石墨黑团子猫与字标" width="640" />

**让人和 Agent，在同一张画布上协作。**

<em>A shared canvas for people and AI agents.</em>

Arcraven · 可见、可连接、可干预的多 Agent 工作空间

[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0--only-535762?style=flat-square)](LICENSE)
[![Status: Prototype](https://img.shields.io/badge/Status-Prototype-838BA3?style=flat-square)](#当前进展)

[产品概述](#产品概述) · [界面预览](#界面预览) · [快速开始](#快速开始) · [路线图](#路线图) · [参与开发](#参与开发)

</div>

## 产品概述

**Ravomi 是 Arcraven 正在构建的无限画布多 Agent 协作工作空间。** 它将 Agent、代码、素材、任务与子代理组织成可连接的模块，让人既能看清整个项目，也能随时进入一个分支推进具体工作。

从一句目标、一份资料或一个待办开始，AI 拆解 TODO、分配任务、编排流程，并在 Agent 之间交接所需上下文与成果。你可以查看进度、调整计划、补充输入和审阅结果；一个分支运行时，其他工作仍可继续。

| 从什么开始 | 如何协作 | 希望得到什么 |
| --- | --- | --- |
| 目标、任务、代码、参考资料与素材 | 在画布上组织模块，让 Agent 分工、并行执行、交接与汇合 | 可追溯的成果，以及始终能理解和介入的工作过程 |

我们围绕三个体验设计 Ravomi：

- **可见**：看清谁在做什么、依赖哪些输入、产出了哪个版本，以及哪里需要你作出决定。
- **可连接**：让任务、代码与素材成为可复用模块，明确它们之间的关联、依赖和上下文来源。
- **可干预**：调整分工、修订计划、请求停止或审阅成果，同时保留其他独立分支的工作空间。

> **当前阶段：本地画布原型与 React/Tauri 代码协作 MVP。** [Tauri 入口](app/README.md) 已内置 Pi Agent Core，提供任务规划、并行代码提案、固定版本交接与人工审阅。默认 Bevy 画布尚未承载 Agent；空间模块、图像工作流和 Skill 提炼仍待实现。详细范围见 [产品定义](PRD.md)。

## 界面预览

<div align="center">

<img src="assets/branding/ravomi-workspace-concept.png" alt="Ravomi 目标界面：素净画布上连接规划、研究、开发与视觉 Agent，右侧展开任务详情" width="100%" />

<sub>目标 UI 设计概念，包含规划中的 Agent 协作能力；并非当前应用截图。</sub>

</div>

界面保持素净：低饱和雾灰紫外壳、暖白工作区、清晰的文字与连接关系，细节按需展开。石墨黑团子猫以局部雾蓝紫细颗粒柔光呈现“迷幻、神秘而克制”的气质，工作内容始终清晰可读。

布局、状态、键盘操作与可访问性见 [UI/UX 规范](DESIGN.md)；角色与字标见 [品牌规范](assets/branding/BRAND.md)。当前原型的实际截图与检查环境收录在 [验证记录](VALIDATION.md)。

## 从目标到成果

以“准备一次产品发布”为例，完整的协作体验将按以下流程展开：

1. **放入目标与资料**：把发布目标、产品说明、代码和参考素材放到画布上。
2. **拆解与分工**：规划 Agent 生成有完成条件的 TODO，连接研究、开发与视觉任务。
3. **并行推进**：不同 Agent 处理各自任务，必要时委派子代理；你可以继续整理资料或推进另一条分支。
4. **交接与汇合**：下游任务接收指定版本的成果及必要上下文，保留来源与依赖关系。
5. **审阅与调整**：查看结果、补充反馈、重试或重新分配任务，逐步完成交付。

画布负责呈现关系，任务范围与所选输入决定实际执行。一次运行记录一次尝试，成果按版本保留；连接模块本身不会自动执行命令或发布内容。运行与交接契约见 [AI 运行设计](AI_DESIGN.md)。

## 当前进展

仓库包含三个共享 Rust 文档基础的实现入口：

| 入口 | 当前可用 | 正在补齐 |
| --- | --- | --- |
| **Rust / Bevy 画布** | 文字节点、连线、嵌套分组、图片导入、搜索、缩放平移、撤销重做、`.prg` 保存与恢复副本、PNG 导出 | 新 UI 规范、日常编辑细节与跨平台验证 |
| **React / Tauri 原型** | 对象列表、文字编辑、新建、打开、保存、撤销重做；复用 Rust 文档核心 | 空间画布与完整交互迁移；浏览器入口仅作界面预览 |
| **Bevy 内嵌 React 输入** | 通过 Wry 子 WebView 在原生画布中编辑节点文字 | 独立实验入口，已有 Linux X11 / XWayland 验证 |

Linux 已有编辑与渲染验证记录；Apple Silicon macOS 完成过原生启动及 DMG 打包的有限验证，见 [macOS 说明](packaging/macos/README.md)。Windows 和移动端尚无完整运行验收。格式兼容范围与已知问题见 [验证记录](VALIDATION.md)。

## 快速开始

### 1. 准备环境

| 依赖 | 要求 |
| --- | --- |
| Rust | 1.95 或更高版本，包含 Cargo |
| 桌面构建环境 | C / C++ 构建工具及对应平台的窗口、图形依赖 |
| Linux 运行环境 | X11 / Wayland、Vulkan 驱动、Fontconfig 与中文字体 |

平台依赖与运行参数见 [开发指南](DEVELOPMENT.md)。当前画布运行无需配置模型 API 密钥。

### 2. 获取源码并启动画布

```sh
git clone https://github.com/Ravomi/ravomi-app.git
cd ravomi-app
./run.sh
```

启动器按需编译并进入首页。选择“新建草稿”，即可开始编辑；也可以打开已有 `.prg` 文档。

打开指定文件并使用本机中文字体：

```sh
./run.sh /absolute/path/example.prg --font /path/to/your-cjk-font.ttf
```

Linux 默认字体路径为 `/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc`；本机路径不同时使用 `--font` 指定。

### 3. 创建第一张画布

| 操作 | 方式 |
| --- | --- |
| 创建 / 编辑文字节点 | 双击空白处 / 双击文字节点 |
| 完成编辑 / 插入换行 | Enter / Shift+Enter |
| 缩放 / 平移 | 滚轮 / 中键拖动或空格＋左键拖动 |
| 多选 / 分组 | Shift＋点击 / Ctrl+G |
| 连接节点 | 从节点右键拖向另一节点 |
| 撤销 / 重做 / 保存 | Ctrl+Z / Ctrl+Shift+Z / Ctrl+S |
| 查看全图 | F |

以上为默认桌面操作；触控板、macOS 修饰键及输入焦点规则见 [开发指南](DEVELOPMENT.md)。保存写入原文件，恢复副本作为独立文件保留。

<details>
<summary><strong>开发者入口：React / Tauri 与内嵌 React 输入</strong></summary>

React / Tauri 原型需要 Node.js 24、npm、Rust 与平台构建依赖。在仓库根目录执行：

```sh
cd app
npm ci
npm run tauri dev
```

`npm run dev` 仅预览浏览器界面，文档操作禁用。Tauri 原型目前提供对象列表与文字编辑，尚未承载完整空间画布。

在完成 `app` 依赖安装后，可从仓库根目录使用独立入口体验 Bevy 内嵌 React 文字编辑：

```sh
./app/run-canvas.sh
```

Linux 内嵌入口需要 GTK 3、WebKitGTK 4.1 与 X11 / XWayland。完整环境要求、入口区别与验证命令见 [app/README](app/README.md)。

</details>

## 路线图

| 阶段 | 重点 | 状态 |
| --- | --- | --- |
| 画布基础 | 本地文档、模块连接、编辑、保存与导出 | 已有原型，持续完善 |
| 日常工作空间 | 新界面、画布迁移、输入与缩放、恢复及平台验收 | 进行中 |
| 单任务执行 | 选定输入、调用执行器、接收成果、重试与重新打开 | React/Tauri 代码 MVP |
| 多 Agent 协作 | TODO 编排、并行分支、子代理、版本交接与人工审阅 | React/Tauri 代码 MVP；完整空间协作待实现 |
| 模块复用与扩展 | 多执行器协作、组合模块，以及研究、编码和视觉工作流 | 规划 |

核心协作闭环从“一份目标、两条并行分支、一次子代理委派、一个版本汇合、一次人工审阅”开始。具体范围、依赖与验收标准见 [PRD](PRD.md)。

## 文档与项目结构

| 文档 | 内容 |
| --- | --- |
| [产品定义](PRD.md) | 产品范围、用户旅程、阶段与验收 |
| [UI/UX 规范](DESIGN.md) | 布局、视觉层级、组件、状态与可访问性 |
| [AI UI 规则](AI_UI_RULES.md) | 给 AI 设计和实现界面的执行约束 |
| [AI 运行设计](AI_DESIGN.md) | Agent、任务、上下文、工件与调度契约 |
| [开发指南](DEVELOPMENT.md) · [开发约定](AGENTS.md) | 实现入口、环境、检查与协作方式 |
| [验证记录](VALIDATION.md) | 已执行检查、运行环境与已知限制 |

```text
src/          Rust 文档核心、历史与恢复、Bevy 画布
app/          React / Tauri 原型与内嵌文字输入
assets/       品牌、图标与字体资源
packaging/    平台打包与集成
validation/   验证记录与截图
vendor/       保留来源与修改记录的依赖
```

## 参与开发

欢迎通过 [Issues](https://github.com/Ravomi/ravomi-app/issues) 反馈问题、提出场景或讨论设计，也欢迎提交代码、文档与测试改进。交互问题请附复现步骤、运行入口、系统及缩放比例；文件问题请提供移除敏感内容后的最小示例。

提交前阅读 [开发约定](AGENTS.md)：独立变更保持单独、可回滚的提交，并运行相关检查；界面变更同时核对 [设计规范](DESIGN.md) 与实际交互。新增 skill 仅安装到项目的 `.agents/skills/`。

## 致谢与许可

Ravomi 由 Arcraven 开发，沿用 Project Graph / Graplu 原型的部分基础与 `.prg` 兼容约定。感谢上游作者、开源依赖维护者，以及参与代码、设计、文档和测试的贡献者。来源与迁移记录见 [MIGRATION](MIGRATION.md)，完整致谢索引见 [CREDITS](CREDITS.md)。

项目采用 [GPL-3.0-only](LICENSE)。第三方组件保留各自版权与许可证；品牌资源说明见 [BRAND](assets/branding/BRAND.md)。

## 原生项目文件

新建 Ravomi 项目使用 `.mio`：以版本化 ZIP 包保存画布、资产和明确选定的协作历史。既有 `.prg` 画布保留兼容读写；打开项目不恢复归档中的任务，本机活动运行记录独立保存。包内结构、转换规则和当前限制见 [存储规范](STORAGE.md)。
