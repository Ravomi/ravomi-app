# 画布回答正文与折叠碰撞修复

日期：2026-10-12。环境：Linux x86_64，Bevy 0.19.1。

## 变更

- 物理碰撞使用与绘制一致的折叠投影尺寸；文字测量变化也更新碰撞体，不修改内部成员位置。
- Agent 内容节点使用解析后的完整回答，规划使用任务步骤和完成条件。取消原始运行日志预览和 2500 字截断。正文固定宽度，高度随排版增长。
- 新完成结果展开一次；绑定保存已展示 Run ID，后续刷新尊重手动折叠。已有绑定在下次连接同步时兼容升级，不自动接管本机运行。
- 所有新 Run 使用中文表达规则，覆盖规划、任务与 Skill；重试不会重复叠加同一策略。旧回答不重写。

## 表达参考与真实调用

参考 [ASD-STE100 官方介绍](https://asd-ste100.org/about_STE.html) 的明确表达原则，以及 [i-have-adhd](https://github.com/ayghri/i-have-adhd/blob/main/skills/i-have-adhd/SKILL.md) 的答案优先、编号步骤和有限分组。中文规则不是 ASD-STE100 受控英语认证；没有安装上游插件或 Skill。

使用本机已配置的 Pi / DeepSeek 对“什么是 Git 工作流”进行真实调用，不写入用户画布。模型返回可解析的 `summary` / `files` JSON，正文为中文，给出了定义和实例。首轮列表过长，收紧提示词后第二轮每组不超过五项，但仍有额外分类和不必要的“未验证”说明。提示词是行为约束，不代表逐句合规保证；未实现完整语言验证器。

## 验证范围

回归用例覆盖折叠尺寸变化与展开恢复、规划/正文投影、长正文不截断、手动折叠保存重开、项目运行隔离、重试提示词幂等。

- `cargo fmt --all --check` 通过。
- `cargo test --release --workspace --locked`：310 项通过、27 项忽略。
- `cargo clippy --release --workspace --all-targets --locked -- -D warnings` 通过。
- `cargo build --release --locked` 通过。
- Linux Xvfb / lavapipe 图形检查：测试画布中的已完成节点直接显示模拟回答，正文高度从初始 240 缩到内容高度，分组随之收紧；手动折叠的规划节点保持折叠。截图见 [节点正文](ai-content-after.png)。截图使用测试执行器，不作为真实模型完成代码任务的证据；真实模型调用另见上节。

本轮不代表 macOS 或实体触控板验收。
