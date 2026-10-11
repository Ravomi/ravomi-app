# 内置 Pi 代码协作 MVP 验证

环境：2026-10-11，Linux x86_64，Node 24.21.0，Bun 1.3.14，Pi Agent Core / Pi AI 1.0.4。原生 UI 使用 Xvfb、WebKitGTK 与真实 Tauri IPC。本记录不代表 macOS/Windows 或 GPU 性能验收。

## 已实现范围

React/Tauri 的“代码协作”入口选择 UTF-8 源码 → Pi 生成待编辑计划 → 用户保存并启动 → Rust 最多两路并行 → 计划内子 Agent 检查与固定版本汇合 → 单独运行生成代码测试 → 人工审阅版本 → 导出新目录。每个任务一个 Agent，子 Agent 的父任务明确记录，但不是模型自主创建的动态后代。模型本身没有终端/文件工具；源码变更是完整文件提案。

Run 输入和上游版本启动前保存；失败隔离、暂停派发、停止分支、新 Run 固定输入重试、重启核对、保存失败恢复均有实现。原项目不直接写入；同名分支冲突必须显式解决，原基线变化时拒绝导出。

## 本次执行

| 检查 | 结果 |
| --- | --- |
| `cargo test --release --workspace --locked` | 265 通过，27 忽略；忽略项包含需显式环境的字体/GPU/教程及远端模型测试 |
| `cargo clippy --release --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cargo fmt --all --check` | 通过 |
| `cd app && npm run build` | TypeScript 与生产 bundle 通过 |
| `cd app && npx playwright test` | 22 通过；模拟 IPC 仅验证前端契约，不能代替真实 Agent |
| `cd app/agent-engine && npm test` | 2 通过：无外部运行时启动、私有协议与显式模型错误 |
| `npm run tauri build -- --no-bundle` / `--debug --no-bundle`（app 内） | 两种构建通过，内置引擎和许可证资源接通；未生成正式安装包 |
| `live_pi_code_workflow`（显式 ignored 测试） | DeepSeek 真调用：初次 6 任务/8 个 Run，增加拓扑断言后复验 5 任务/7 个 Run（含规划和代码测试），生成的 Node 测试通过 |
| `app/tests/collaboration_native_smoke.py` | 最终真实窗口 + IPC + 编译内置 Pi：4 任务、6 个 Run，包含独立文件并行分支和父子检查；Node 测试、审阅及导出通过，原文件未改写；运行期间可创建并编辑独立文字节点 |
| `app/tests/native_smoke.py` | 真实中文编辑 → Tauri IPC → Rust 保存/重开通过 |

Rust 可控执行器测试覆盖 DAG/路径校验、并发上限、幂等启动、固定版本交接、失败分支、取消、重试、未知运行恢复、损坏记录保留、存储失败后 Flush、分支文件冲突、暂停撤销排队中的显式派发与测试进程失败/取消。

首次远端使用旧环境凭据被 DeepSeek 拒绝（401），更新本次测试进程凭据后通过；密钥未写入源码、工件或本记录。原生验收迭代时一次模型任务失败（初版脚本未保留具体错误）、一次“必须有两个根任务”的测试假设失败。后一项已改为验证两项不存在相互依赖的任务，允许共同前置任务；后续原生完整流程通过。另一次补验中测试 Agent 未返回冲突文件的合并版本，被宿主拒绝；现已在任务输入中列明必须合并的路径。最终原生用独立文件分工完成并行验收，冲突拒绝另有确定性单元测试。模型生成并不保证每次成功，生产入口保留明确失败与固定输入重试。

## 复现

先在 app 构建内置引擎与桌面应用，按本机方式配置 `DEEPSEEK_API_KEY`，不要把密钥放入测试源码或提交文件：

```sh
cargo test --release -p project-graph-bevy-canvas --lib --no-default-features --locked collaboration::tests::live_pi_code_workflow -- --ignored --nocapture
xvfb-run -a python3 app/tests/collaboration_native_smoke.py target/debug/hm-app /tmp/ravomi-code-native.png
```

第二条需要 `cd app && npm run tauri build -- --debug --no-bundle`。测试只使用临时源码与 app data，关闭后删除；实际窗口截图见 [原生代码协作](code-collaboration-native.png)。

## 未覆盖和限制

- 默认 `./run.sh` 仍为 Bevy 画布；Agent 尚未接入空间模块、框选和连线。
- 本轮不做图像相关功能；W16 总结/Skill 提炼已入 PRD/TODO，尚未实现。
- OpenAI/Anthropic API-key 适配已编译，远端验收未执行；OAuth、自定义端点和全局 Pi 扩展未接入。
- 代码测试使用宿主权限，临时目录不是安全沙箱；没有自动 Git 合并、源项目写回、删除文件或依赖安装。
- 存储为单后端版本化 JSON，最多 30 流程/64 MiB；审阅只保留每个 Run 的最新决定。完整数据库事件模型、独立 Artifact、归档入口与动态委派待后续。
- macOS/Windows 引擎目标已定义，未在对应机器构建、打包或验收；运行中取消在提供方服务端是否计费不能由本地进程终止保证。
