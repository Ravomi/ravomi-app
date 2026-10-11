# 内置 Pi 动态子 Agent 首轮

2026-10-12，Linux x86_64，Pi Agent Core / Pi AI 1.0.4，Bun 1.3.14。本轮为一层动态委派实现，不代表完整多 Agent 产品验收。

## 实现

- 普通任务的 Pi 会话通过 `spawn_agent` 请求 Rust 创建独立子任务与 Run；持久化后才派发，不依赖 tmux 或全局 Pi。相同父 Run / 调用 ID 重试返回原 Run，不同参数拒绝。
- `wait_agent` 仅访问本次父运行创建的子任务，返回已保存的固定版本成果。运行中新增的已读取成果与启动时固定输入分开，防止改写输入和父子等待环。
- 独立子任务可以并行，沿用父任务模型与固定输入。子任务没有委派、文件或终端工具；主任务总时限包含等待。模型调用用量按每个 Run 内多轮累加。
- 暂停阻止新委派；停止父分支取消子运行，父任务结束也清理未结束子运行，所有 Run 和成果保留。父任务遗漏成功子成果或留下在途子任务时不能标记成功。
- 原生画布复用折叠 Agent 组及实际状态，父子连接标记“委派”；绑定的流程保存为 `.mio` SQLite v3 内的协作归档 v2。打开不执行，旧归档继续读取。

## 验证记录

- `cargo fmt --all --check`、`cargo clippy --release --workspace --all-targets --locked -- -D warnings` 与 `git diff --check` 通过；修改文档的本地链接目标存在。委派、运行与 Pi 模块的 similarity-rs 检查未发现重复函数。

- `cargo test --release --workspace --locked`：329 passed，28 ignored；其中核心 131 passed、7 ignored。包含双子任务并行、调用幂等及参数冲突、跨父查询拒绝、分支停止、父任务提前结束、子文件汇总、运行中归档刷新、画布组投影、`.mio` 保存重开与运行库重开。结果来自共享工作区，不将其他会话已有测试算作本功能新增测试。
- 内置引擎构建成功；`npm test`：5 passed，覆盖请求/回复按调用 ID 配对、错误返回、取消后迟到回复，以及使用可控模型事件流驱动真实 Pi Agent 的 spawn → wait → 最终输出循环。这是本地协议验证，不是远端模型验收。
- DeepSeek 真实冒烟已尝试；现有本机凭据返回 HTTP 401，未进入工具调用阶段，**未通过远端验收**。未读取用户工程，未将凭据保存到仓库。可在本机修复凭据后运行下方命令。

```sh
cd app/agent-engine
npm run build
npm test
cd ../..
cargo test --release --lib --locked --no-default-features real_pi_dynamic_delegation -- --ignored --nocapture
```

## 边界

尚未实现 steer/follow-up、递归委派、独立子模型选择、共享预算账本、完整计划修订历史、多 Agent 共用任务组和按状态汇总的独立总览。活动事实仍位于本机 `workflows.json`，归档进 `.mio` 不等于活动数据库已统一。未进行本轮 GPU 截图或 macOS/Windows 实机验收。

参考 [Pi Subagent](https://github.com/badlogic/pi-subagent) 的机制；实现独立编写，未复制许可尚未确认的源码。
