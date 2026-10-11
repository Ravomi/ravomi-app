# Ravomi 应用层 · React / Tauri

这是 Ravomi 实际存在的应用层迁移入口。Ravomi 是 Arcraven 的多 Agent 画布工作空间：目标是在无限画布上连接代码、素材、任务和子代理，让任务拆解、并行分支、工作流与上下文交接可见，并允许人随时介入。

React + TypeScript 负责界面，Tauri 2 负责原生宿主，现有 Rust `Document` 负责文档与历史。这里提供对象列表、文字编辑与内置 Pi 的代码协作 MVP，完整空间画布尚未接入。项目目标覆盖桌面三端、Android、iOS 和浏览器；此入口不代表六端已验证。

## 设计与产品契约

开始界面工作前阅读根目录的 [PRD](../PRD.md)、[DESIGN](../DESIGN.md) 与 [AI_UI_RULES](../AI_UI_RULES.md)。[AI_DESIGN](../AI_DESIGN.md) 负责 Agent 运行架构，不能替代给 AI 实现 UI 的规范；[README](../README.md) 是项目总入口。

新界面采用 Arc 式素净层次：低饱和侧栏、内嵌暖白工作区、清晰模块和按需出现的详情。迷幻、神秘而克制的气质集中在小猫：一体团子轮廓、两耳两眼，无嘴、鼻、尾或脚；石墨黑主体配局部雾蓝紫细颗粒柔光。正文、连线、表单和状态不使用模糊或颗粒，状态也不能只靠色彩或猫的表情表达。品牌形态与资源见 [BRAND](../assets/branding/BRAND.md)。

实现画布与 Agent 流程时，须让用户看清任务目标、当前状态、上下文来源、产物与下一步，并按执行器的真实能力提供暂停派发、请求停止、调整、重试和审核的明确入口。暂停派发只阻止新任务启动，已运行任务继续；停止请求获得确认后才显示已停止，运行暂停与恢复仅在执行器实际支持并验证后提供。未接入能力明确显示不可用或阶段说明，不能用静态进度、演示产物或按钮外观暗示真实执行。以下“已接通”仅描述当前代码；新规范落地后需单独验收。

## 运行

开发需要 Node.js 24、npm、Bun 1.3.14、Rust 1.95+ 与 [Tauri 平台构建依赖](https://tauri.app/start/prerequisites/)。Bun 只用于编译内置引擎，终端用户无需安装 Pi、Node 或 Bun。Linux 需要 GTK 3、WebKitGTK 4.1 开发库，以及可用的桌面文件选择 portal。

```sh
cd app
npm ci
npm run tauri dev
```

`npm run dev` 仅运行浏览器界面预览，文档操作明确禁用。浏览器尚未实现 Rust WASM/服务端与存储适配，不使用虚假数据模拟可保存的文档。

构建原生应用（目前不生成安装包）：

```sh
npm run tauri build -- --no-bundle
```

根 workspace 默认成员仍是原 Bevy 应用，`./run.sh` 和原有 `cargo run` 行为不变。Tauri crate 独立构建时关闭核心库的默认 `viewer` feature。

## 已接通的操作

- 新建、打开 `.mio` / 兼容 `.prg`、保存、另存为；新项目默认 `.mio`，取消文件对话框不替换文档。
- 对象列表、普通文字节点创建与编辑、撤销和重做；锁定节点只读。
- 应用文字后可撤销；保存会先应用文字草稿；切换对象、替换文档、关闭窗口会确认未保存更改。
- 失败在界面显示，打开失败保留当前文档，保存沿用原核心的磁盘冲突检测与原子写入。
- 深浅主题、三档 UI 缩放和窄窗口布局。

文档区仅显示对象列表与连线数量，不绘制空间位置、图片、公式或连线。保存仍由原文档核心保留这些数据。完整画布、触控手势、多标签、自动恢复与各平台发布流程尚未迁移。当前文字编辑保留文档中已有几何尺寸；自动排版需由后续画布完成。

## 代码协作 MVP

打开侧栏“代码协作”，选择目录并逐项勾选源码，输入目标与 `provider/model`（默认 `deepseek/deepseek-flash`）。提供方凭据可在“模型连接设置”中应用到本次会话，或使用 API Key 环境变量。应用设置的密钥不写入项目和运行记录，关闭后清除。未填写时兼容已有 Pi 的 API-key 配置；OAuth 尚不支持。

生成计划会调用模型，但任务执行要在审阅计划、保存后点击“运行 / 恢复派发”。就绪任务不设固定任务数量、并发数或调用次数上限，每次最长 180 秒。子 Agent 是计划中的显式检查任务；动态自主派生仍未实现。暂停仅阻止新任务派发，停止会取消相应分支，失败只阻塞依赖它的下游。

每次 Run 固定源码及上游版本，产物是完整文件提案。模型没有文件/终端工具；指定测试命令须另行点击运行，以本机权限在临时目录执行，并不构成安全沙箱。查看代码、测试记录并人工通过后，可导出新目录；原项目基线变化时拒绝导出。不支持自动写回、删除文件、Git 合并或安装依赖。

运行记录由 Rust 保存于 Tauri app data 的 `code-collaboration/workflows.json`，不混入 `.prg`。重启不自动重发；在途任务先核对再重试，写入失败保留当前进程的待保存成果并阻止继续派发。输入与产物含所选源码，备份运行目录时按项目数据处理。

明确附加到项目的流程历史随 `.mio` 保存，保留附加时的固定证据；后续运行更新需再次附加。保存共用 Rust 的 schema 2 快照、正文去重与校验链路，打开归档不恢复执行。与 Bevy 的当前文档流程连接规则区别见 [存储规范](../STORAGE.md)。

引擎来源、版本、构建与许可见 [agent-engine](agent-engine/README.md)。此 React/Tauri 入口独立于 Bevy；框选 Agent、折叠执行节点和 Skill 提炼已在默认原生画布的“Pi · 画布协作”入口实现，见 [验收记录](../validation/canvas-ai-workflows.md)。两个入口的运行存储分开。图像生成和外部 Pi 会话续聊仍未实现。

## 同窗口 React 文字编辑（原生 Bevy 画布）

在项目根目录运行 `./app/run-canvas.sh 文件.prg`。首次先在 `app` 执行 `npm ci`。启动器构建独立 React 编辑 bundle，并启用 Rust 的 `react-editor` feature。默认 `./run.sh` 仍使用原编辑器。

此入口通过 Wry 子 WebView 在 Bevy 窗口内原位编辑节点文字；画布、文档历史、MSDF 和 GPU 渲染仍由 Rust/Bevy 持有，没有逐帧图像传输。React 负责 textarea、选择与系统输入法；`src/edit_protocol.rs` 定义带会话编号的草稿和提交消息。预编辑中的 Enter 不提交，结束输入后 Enter、Esc 或 Tab 完成，Shift+Enter 换行。

Linux 需要 GTK 3、WebKitGTK 4.1 与 X11/XWayland。启动器仅为自身进程选择 X11；原生 Wayland 子窗口嵌入暂不支持。Windows/macOS 的同窗口入口尚未验收。这是文字输入适配层，菜单和连线文字仍沿用 egui；上面的 Tauri 对象列表原型尚未成为 Bevy 画布宿主。

验证入口：`npm run build:canvas-editor`、`npx playwright test tests/canvas-editor.spec.ts`，以及根目录的 `cargo test --release --workspace --locked --features react-editor`。原生桥接测试见 `tests/canvas_native_smoke.py`。

## 文档接口

`src/bridge.ts` 是界面访问文档的入口，Tauri command 将请求交给根目录 `src/session.rs`，后者调用 `Document`。Rust 是文档事实和历史的唯一所有者；React 只保存显示快照和未应用的文字草稿。磁盘读写与解析在 Tauri 阻塞任务池执行，错误返回前端。

现在每次命令返回完整的轻量对象快照，适用于接口验证；大图迁移时应采用增量变更和可见区域渲染，不能以本原型宣称画布性能。节点附件字节不经此快照传输。

下一阶段先验证真实空间画布、中文输入、触控与缩放。桌面平台逐一检查；移动端还需 URI 文件访问、生命周期与后台恢复适配；浏览器不能执行本地命令，未来 Agent 执行需要明确的本机或远程宿主。

## 验证

```sh
npm run build
npx playwright install chromium
npm test
```

在仓库根目录运行：

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`src/session.rs` 的测试执行真实 Rust 文档创建、中文编辑、撤销、保存重开和失败保留。Playwright 测试用明确的模拟 IPC 检查前端草稿、取消及错误反馈，并截图深浅主题 × 100/150/200% × 宽窄窗口；它不替代原生 IPC 与各平台验收。截图写入忽略的 `test-results/`。

Linux 可在嵌入生产前端的调试构建后，从仓库根目录运行 `xvfb-run -a python3 app/tests/native_smoke.py target/debug/hm-app`，验证真实 WebKit 窗口与 Rust 文档往返。需要 WebKitWebDriver、Xvfb、xauth；详情见验收记录。

界面验收同时覆盖 [DESIGN](../DESIGN.md) 与 [AI_UI_RULES](../AI_UI_RULES.md) 的状态、键盘、窄窗口和可读性要求；Agent 接入后增加并行分支、上下文交接和人工介入流程，不把前端模拟 IPC 当成真实 Agent 执行证据。历史验证见 [React/Tauri 入口验收](../validation/react-tauri-bootstrap.md) 和 [同窗口编辑验收](../validation/react-inline-editor.md)。

界面已采用获准风格卡的暖白／石墨主题，默认浅色；含团子猫品牌区、桌面侧栏、胶囊主操作和清晰内容卡片。窄窗口与高 UI 缩放按实际容器宽度重排；减少动态效果偏好关闭 CSS 过渡。系统窗口图标与网页图标引用 [品牌图标导出](../assets/branding/app-icons/README.md)，功能图标复用项目已有 Lucide 资源。Tauri 仍未启用安装包构建，不将资源接通等同于跨平台安装验收。

## `.mio` 项目与协作归档

原生项目默认保存为 `.mio`，继续支持旧 `.prg` 画布；结构与转换规则见 [存储规范](../STORAGE.md)。代码协作可明确选择流程，将该次快照附加到当前项目后保存；普通保存不会打包全部本机运行记录。重新打开项目只展示归档证据，不恢复会话或派发任务，活动 `workflows.json` 仍独立保存在本机。归档排除本机工程绑定与会话句柄，但选定源码和文本证据可能含用户提供的路径或敏感内容。
