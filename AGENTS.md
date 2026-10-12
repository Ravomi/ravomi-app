# Ravomi 开发约定

Ravomi 是 Arcraven 的无限画布多 Agent 协作产品。核心定义：召唤并协作管理多个 Agent，将代码、素材、任务与子代理组织为可连接模块；AI 拆解 TODO、编排流程、按需交接任务和上下文；人能查看进度、介入调整，同时并行推进其他分支。核心体验为可见、可连接、可干预，并保持对项目全局的掌握。

本仓库包含 `src/` Rust/Bevy 画布基线、`app/` React/Tauri 界面迁移原型，以及 Wry 内嵌 React 文字输入入口；根目录 `Cargo.toml` 是 workspace 入口。两个入口均已接入编译内置 Pi 的代码协作 MVP；原生画布支持 Space 提交、折叠 Agent 节点和选区 Skill 提炼，验证范围见 `validation/canvas-ai-workflows.md`。图像工作流、动态自主委派和 Skill 自动复用仍待实现。不能将设计概念、可视化连线或模拟测试写成真实外部调用已通过。`app/` 是实际本地目录；迁移前上游的 `app/src/core/` 等路径只在明确标记的历史资料中保留。

## 文档与设计依据

开始工作先读 [README](README.md) 与 [PRD](PRD.md)；界面工作继续读 [DESIGN](DESIGN.md)、[AI_UI_RULES](AI_UI_RULES.md) 和 [品牌规范](assets/branding/BRAND.md)；运行编排工作读 [AI_DESIGN](AI_DESIGN.md)。PRD 决定产品范围，DESIGN 决定当前目标体验，AI_UI_RULES 约束 AI 如何落实设计，AI_DESIGN 定义尚待实现的运行契约。源码及有环境记录的验证决定哪些能力已实现。

- UI 保持 Arc 参考后的素净与清晰：低饱和外壳、暖白工作区、清晰文字和连接关系，渐进展开细节。角色氛围不扩散为整张画布的模糊、颗粒或强光。
- 小猫为一体团子轮廓、双耳双眼，无嘴、鼻、须、尾巴、脚和四肢；石墨黑主体，局部雾蓝紫细颗粒柔光。复用已确认资产，不因任务角色生成额外五官、装备或不同物种。
- 连线、移动或分组不自动授权执行、发送全部会话或发布成果；区分任务修订、一次 Run、固定工件版本和人工审阅。多个分支运行时仍须允许人在其他区域操作。
- 当前实现与目标规范不一致时，记录差距并在当前任务范围内处理；历史截图、旧主题和概念图不覆盖最新文字规范。不要把文档变更报告成代码已完成。

## 开发与交付

产品名称为 Ravomi，品牌主图见 `assets/branding/ravomi-logo.png`，独立头像见 `assets/branding/ravomi-avatar.png`。公开产品仓库为 `https://github.com/Ravomi/ravomi-app.git`，默认分支为 `main`；开发工作区可能仍连接历史仓库 `Wang-Yang-source/H-M`，推送前核对实际目标，不能假定本地 remote 已切换。首次公开发布采用已提交内容的完整快照，旧项目历史不作为新仓库的提交历史发布，来源与许可证仍保留在 `CREDITS.md` 和 `MIGRATION.md`。

- 每个独立功能或修复用一个 Conventional Commit；提交前运行相关测试，避免混入其他会话的未完成改动。使用 GitButler `but` 管理提交和分支。
- 新增或修改界面优先复用语义 token、主题与组件；旧主题迁移须显式完成，不能新增一套互相冲突的硬编码样式。全局视觉或交互规则变更时同步规范，按其中的主题、缩放、输入与状态矩阵验收。
- 新增 skill 仅安装在本项目 `.agents/skills/`，不得安装到用户级或全局技能目录；保留技能来源与许可证。
- 贡献者与上游致谢统一维护在 `CREDITS.md`，具体代码贡献沿用提交与 PR 记录，不逐函数追加个人署名。保留第三方原作者及许可证，迁移前的贡献保留来源索引。不自动添加个人头衔、邮箱或修改 Git 身份。
- 优先标准库及已有社区库，依赖固定版本并维护 `Cargo.lock`、许可证及 vendored 修改来源。
- 验证使用 `cargo fmt --all --check`、`cargo test --workspace --locked` 和 `cargo clippy --workspace --all-targets --locked -- -D warnings`，可选 `--release` 复用发布模式缓存。
- React/Tauri 及内嵌输入修改同时遵循 [app 验证说明](app/README.md)。仅修改文档时检查本地链接、命令/路径、实现状态与规范一致性，不把历史测试结果记成当次重新执行；不为纯文档变更启动无关 GPU 基准。
- GPU、系统字体及教程文件测试用 `-- --include-ignored --test-threads=1`；确认字体、fixtures 与 Vulkan 可用。桌面运行及性能测试遵循用户授权，启动器 `./run.sh` 会按需编译。不要在测量帧耗时的同时编译或运行其他负载。
- Linux `.mio` 缩略图验证：`python3 -m unittest discover -s packaging/linux -p test_thumbnailer.py`。Python 不参与实时 Bevy 画布渲染。
- `target/`、`results/`、`fixtures/` 不纳入版本控制；不要提交令牌、环境文件或用户文档。`validation/` 保留有说明的验证工件。
- 项目文件仅支持 `.mio`，不再兼容 `.prg` 读写、恢复或系统关联。当前 crate 名称暂保留 `project-graph-bevy-canvas`，shader 的 embedded 路径依赖该名称。
- 修改文件使用直接编辑工具，不用 Python/Node/内联脚本批量改写。不要静默吞掉错误；运行时错误应进入现有 UI 提示流程。
- Bevy 版本以当前锁文件为准；Linux 窗口/字体/输入/帧耗时排查参考 `.agents/skills/bevy-wayland-vulkan/SKILL.md`。
