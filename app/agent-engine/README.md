# 内置 Pi 引擎

Ravomi 将 Pi Agent Core 与 Pi AI 1.0.4 的发布源码模块编译进私有原生可执行文件，随 Tauri sidecar 分发。应用不查找或启动全局 `pi`，用户无需安装 Node、Bun 或 Pi CLI。Rust 持有调度、输入快照、工件与审阅；Pi 承担单个 Agent 的模型调用和流式输出。

开发构建需要 Node 24、npm 与 **Bun 1.3.14**：在 `app` 执行 `npm run build:agent`。使用 lockfile 的 integrity 固定依赖。产物位于忽略的 `app/src-tauri/binaries/`；`npm run tauri dev/build` 自动构建。可用 `RAVOMI_AGENT_TARGET=darwin-arm64` 等目标交叉编译引擎，但这不替代相应平台的应用构建与验收。

支持 DeepSeek、OpenAI、Anthropic API Key。优先使用应用当前会话设置，其次读取相应 `*_API_KEY` 环境变量，最后兼容已有 `~/.pi/agent/auth.json` 的 `api_key` 记录。不会加载 Pi 扩展、全局 Skill、MCP、项目配置或历史会话。OAuth、自定义端点与外部会话续聊尚未接入。

当前代码 MVP 使用选定源码与依赖产物生成完整文件提案，Pi 工具列表为空。用户另行启动测试时，由 Rust 在独立临时目录执行指定 argv；目录隔离不是权限沙箱。只有人工通过的版本可以导出到新目录；不自动修改源项目、不安装依赖、不提交代码。

## 来源与分发

- [Pi](https://github.com/earendil-works/pi/tree/7c10bd4337495ee613f2224843ecdf349b80d1df)，MIT，Mario Zechner；npm 1.0.4 的 `gitHead` 为该提交。上游未修改，适配代码位于本目录 `src/main.ts`。
- [Bun 1.3.14](https://github.com/oven-sh/bun/tree/bun-v1.3.14)；运行时及其链接组件的声明见 [Bun-LICENSE.md](licenses/Bun-LICENSE.md)，Pi 声明见 [Pi-LICENSE](licenses/Pi-LICENSE)。Bun 的 JavaScriptCore/WebKit 重链接说明按原许可证保留。
- 构建收集锁文件内生产依赖附带的 LICENSE/NOTICE 与本目录许可证，生成 `THIRD-PARTY-NOTICES.txt`，与 `engine-lock.json` 一并进入 Tauri bundle resources。发布时同时提供本仓库对应源码、锁文件和构建说明；当前尚未验证正式安装包。

私有协议为 stdin/stdout JSONL：`get_state`、`prompt`、`abort`；返回文本增量、最终消息、用量与完成标志。该协议不对外承诺 Pi CLI 兼容性。
