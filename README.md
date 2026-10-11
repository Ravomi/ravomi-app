<div align="center">

**English** | [中文](README-ZH.md)

<img src="assets/branding/ravomi-logo.png" alt="Ravomi: a graphite-black cat beside the wordmark" width="640" />

**A shared canvas for people and AI agents.**

Arcraven · A visible, connected, controllable multi-agent workspace

[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0--only-535762?style=flat-square)](LICENSE)
[![Status: Prototype](https://img.shields.io/badge/Status-Prototype-838BA3?style=flat-square)](#current-progress)

[Overview](#overview) · [Preview](#preview) · [Quick start](#quick-start) · [Roadmap](#roadmap) · [Contributing](#contributing)

</div>

<a id="产品概述"></a>

## Overview

**Ravomi is an infinite-canvas workspace for multi-agent collaboration, being built by Arcraven.** It brings agents, code, assets, tasks, and subagents into connectable modules, so you can understand the whole project and step into any branch to move the work forward.

The intended workflow starts with a goal, a reference, or a task. AI breaks it into TODOs, assigns work, coordinates the workflow, and hands the necessary context and results between agents. You can follow progress, adjust plans, add input, and review results while other branches continue independently.

| Start with | Work together | Work toward |
| --- | --- | --- |
| Goals, tasks, code, references, and assets | Organize modules on a canvas; let agents divide work, run in parallel, hand off, and bring results together | Traceable results and a process you can understand and influence throughout |

Ravomi is designed around three qualities:

- **Visible**: See who is doing what, which inputs they depend on, which version they produced, and where your decision is needed.
- **Connected**: Make tasks, code, and assets reusable modules with clear relationships, dependencies, and context sources.
- **Controllable**: Change assignments, revise plans, request a stop, or review results while keeping room to work on other independent branches.

> **Current stage: a native canvas with an embedded Pi code collaboration MVP.** Select text/group blocks and press Space to plan and execute tasks; each planned Agent gets a collapsible canvas node with run records. Canvas text tasks require no external code folder, and their records are included when saving `.mio`. Configure the model and optional source attachments in “画布协作”. Ready Agents run without a fixed concurrency or task-count cap. Select Agent nodes to summarize fixed runs into an editable, project-local Skill. Image workflows, autonomous dynamic delegation and automatic Skill reuse remain planned. The [React/Tauri entry](app/README.md) is a separate prototype, not the full canvas. See [canvas workflow validation](validation/canvas-ai-workflows.md) for tested scope.

<a id="界面预览"></a>

## Preview

<div align="center">

<img src="assets/branding/ravomi-workspace-concept.png" alt="Ravomi workspace concept: a quiet canvas connecting planning, research, development, and visual agents, with task details on the right" width="100%" />

<sub>Target UI concept, including planned agent collaboration features. This is not a screenshot of the current application.</sub>

</div>

The interface stays quiet: a muted gray-violet shell, a warm-white workspace, clear text and connections, and details revealed when needed. A graphite-black cat carries a restrained sense of mystery through local misty blue-violet light and fine grain. The work itself remains clear and readable.

See the [UI/UX specification](DESIGN.md) for layout, states, keyboard interaction, and accessibility, and the [brand guidelines](assets/branding/BRAND.md) for the character and wordmark. Actual prototype screenshots and their test environments are documented in the [validation records](VALIDATION.md).

<a id="从目标到成果"></a>

## From a goal to a result

For a goal such as “prepare a product launch,” the full collaboration experience is designed to work like this:

1. **Add the goal and references**: Place the launch brief, product notes, code, and reference assets on the canvas.
2. **Plan and assign**: A planning agent creates TODOs with completion criteria and connects research, development, and visual tasks.
3. **Move forward in parallel**: Agents handle their assignments and delegate to subagents when needed. You can keep organizing references or work on another branch.
4. **Hand off and combine**: Downstream tasks receive specific versions of results and the necessary context, preserving sources and dependencies.
5. **Review and adjust**: Inspect results, add feedback, retry, or reassign work until the deliverable is ready.

The canvas shows relationships; task scope and selected inputs determine what actually runs. Each run records one attempt, and results retain their versions. Connecting modules does not automatically execute commands or publish content. See the [AI execution design](AI_DESIGN.md) for run and handoff contracts.

<a id="当前进展"></a>

## Current progress

The repository contains three entry points built on a shared Rust document foundation:

| Entry point | Available today | Work remaining |
| --- | --- | --- |
| **Rust / Bevy canvas** | Text nodes, connections, nested groups, image import, search, zoom and pan, undo and redo, `.prg` saving and recovery copies, PNG export | New UI specification, everyday editing refinements, and cross-platform validation |
| **React / Tauri prototype** | Object list, text editing, new/open/save, undo and redo; reuses the Rust document core | Spatial canvas and full interaction migration; the browser entry point is a UI preview only |
| **React input embedded in Bevy** | Node text editing in the native canvas through a Wry child WebView | A separate experimental entry point, with Linux X11 / XWayland validation recorded |

Editing and rendering have been validated on Linux. Native startup and DMG packaging have received limited validation on Apple Silicon macOS; see the [macOS notes](packaging/macos/README.md). Windows and mobile platforms have not completed full runtime validation. See the [validation records](VALIDATION.md) for format compatibility and known issues.

<a id="快速开始"></a>

## Quick start

<a id="1-准备环境"></a>

### 1. Prepare your environment

| Dependency | Requirement |
| --- | --- |
| Rust | Version 1.95 or later, including Cargo |
| Desktop build environment | C / C++ build tools and platform-specific windowing and graphics dependencies |
| Linux runtime | X11 / Wayland, Vulkan drivers, Fontconfig, and CJK fonts |

See the [development guide](DEVELOPMENT.md) for platform dependencies and runtime options. The current canvas does not require a model API key.

<a id="2-获取源码并启动画布"></a>

### 2. Get the source and launch the canvas

```sh
git clone https://github.com/Ravomi/ravomi-app.git
cd ravomi-app
./run.sh
```

The launcher builds the application as needed and opens the home screen. Choose “新建草稿” (New draft) to start editing, or open an existing `.prg` document.

To open a specific file and use a local CJK font:

```sh
./run.sh /absolute/path/example.prg --font /path/to/your-cjk-font.ttf
```

On Linux, the default font path is `/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc`. Use `--font` if the font is installed elsewhere.

<a id="3-创建第一张画布"></a>

### 3. Create your first canvas

| Action | Control |
| --- | --- |
| Create / edit a text node | Double-click an empty area / a text node |
| Finish editing / insert a line break | Enter / Shift+Enter |
| Zoom / pan | Scroll wheel / middle-button drag or Space+left-button drag |
| Select multiple nodes / group | Shift+click / Ctrl+G |
| Connect nodes | Right-button drag from one node to another |
| Undo / redo / save | Ctrl+Z / Ctrl+Shift+Z / Ctrl+S |
| Fit the canvas in view | F |

These are the default desktop controls. See the [development guide](DEVELOPMENT.md) for trackpad behavior, macOS modifier keys, and input focus rules. Saving writes to the original file; recovery copies remain separate files.

<details>
<summary><strong>Developer entry points: React / Tauri and embedded React input</strong></summary>

The React / Tauri prototype requires Node.js 24, npm, Rust, and platform build dependencies. From the repository root, run:

```sh
cd app
npm ci
npm run tauri dev
```

`npm run dev` previews the browser UI only, with document operations disabled. The Tauri prototype currently offers an object list and text editing; it does not yet provide the full spatial canvas.

After installing the `app` dependencies, you can try React text editing embedded in Bevy through a separate entry point. Run this from the repository root:

```sh
./app/run-canvas.sh
```

On Linux, the embedded entry point requires GTK 3, WebKitGTK 4.1, and X11 / XWayland. See [app/README](app/README.md) for full environment requirements, differences between entry points, and validation commands.

</details>

<a id="路线图"></a>

## Roadmap

| Stage | Focus | Status |
| --- | --- | --- |
| Canvas foundation | Local documents, module connections, editing, saving, and export | Prototype available; being refined |
| Everyday workspace | New UI, canvas migration, input and zoom, recovery, and platform validation | In progress |
| Single-task execution | Selected inputs, executor calls, results, retries, and reopening tasks | Code MVP in React/Tauri |
| Multi-agent collaboration | TODO orchestration, parallel branches, subagents, versioned handoffs, and human review | Code MVP; spatial integration pending |
| Reusable modules and extensions | Multiple executors, composed modules, and research, coding, and visual workflows | Planned |

The first complete collaboration workflow starts with one goal, two parallel branches, one subagent delegation, one merge of versioned results, and one human review. See the [PRD](PRD.md) for scope, dependencies, and acceptance criteria.

<a id="文档与项目结构"></a>

## Documentation and project structure

| Document | Contents |
| --- | --- |
| [Product definition](PRD.md) | Scope, user journeys, stages, and acceptance criteria |
| [UI/UX specification](DESIGN.md) | Layout, visual hierarchy, components, states, and accessibility |
| [AI UI rules](AI_UI_RULES.md) | Constraints for AI-assisted interface design and implementation |
| [AI execution design](AI_DESIGN.md) | Contracts for agents, tasks, context, artifacts, and scheduling |
| [Development guide](DEVELOPMENT.md) · [Development conventions](AGENTS.md) | Entry points, environment, checks, and collaboration practices |
| [Validation records](VALIDATION.md) | Completed checks, environments, and known limitations |

```text
src/          Rust document core, history and recovery, Bevy canvas
app/          React / Tauri prototype and embedded text input
assets/       Brand assets, icons, and fonts
packaging/    Platform packaging and integration
validation/   Validation records and screenshots
vendor/       Dependencies with preserved source and change records
```

Most development and design documents are currently written in Chinese. The [Chinese README](README-ZH.md) covers the same product overview and setup instructions.

<a id="参与开发"></a>

## Contributing

Use [Issues](https://github.com/Ravomi/ravomi-app/issues) to report problems, suggest use cases, or discuss design. Contributions to code, documentation, and tests are welcome. For interaction issues, include reproduction steps, the entry point used, your operating system, and display scaling. For file issues, provide a minimal example with sensitive content removed.

Read the [development conventions](AGENTS.md) before contributing. Keep independent changes in separate, reversible commits and run the relevant checks. For interface changes, verify both the [design specification](DESIGN.md) and the actual interaction. Install new skills only in the project's `.agents/skills/` directory.

<a id="致谢与许可"></a>

## Acknowledgments and license

Ravomi is developed by Arcraven and builds on parts of the Project Graph / Graplu prototypes and their `.prg` compatibility conventions. We thank the upstream authors, open-source dependency maintainers, and everyone contributing code, design, documentation, and tests. See [MIGRATION](MIGRATION.md) for source and migration records, and [CREDITS](CREDITS.md) for the full acknowledgments index.

The project is licensed under [GPL-3.0-only](LICENSE). Third-party components retain their own copyright notices and licenses. See [BRAND](assets/branding/BRAND.md) for brand asset information.

## Native project files

New Ravomi projects use `.mio`: a versioned ZIP package containing the canvas, assets, and explicitly selected collaboration history. Schema 2 stores shared evidence bodies by SHA-256 and seals a checked snapshot before atomically publishing a save. Existing `.mio` v1 and `.prg` drawings remain readable; saving as `.mio` upgrades the container. Opening a project never resumes archived runs; local execution records remain separate. See the [storage specification](STORAGE.md) for the package layout, conversion rules, and current limits.
