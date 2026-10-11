# MIO v2 保存基础验证

日期：2026-10-12。格式规范见 [STORAGE.md](../STORAGE.md)。本轮在 Linux x86_64 共享开发工作区接入保存基础，工作区还包含其他会话的改动；测试总数不是本次新增测试数。本文不沿用 [v1 记录](storage-mio.md) 的通过结论。

## 实现范围

| 范围 | 实现与边界 |
| --- | --- |
| 原生格式 | 新输出使用容器 `schemaVersion: 2`、画布版本 1 与必需能力 `portable-snapshot-v1`；读取 v1 `.mio` 和 ZIP + MessagePack `.prg` |
| 正文对象 | `collaboration/records/history.msgpack` 保存 History v1 结构，正文引用 `objects/sha256/<哈希>`；相同字节复用对象，引用位置与运行身份不合并 |
| 保存清单 | `collaboration/snapshot.json` 固定本次保存的 UUID、项目身份、文档修订、所选流程与除自身外全部条目的哈希/大小；不是 Runtime 事务提交水位或身份签名 |
| 旧绑定迁移 | `canvas/bindings.json` 以有版本封装引用原 `ravomi-code-agents.json` 字节；不生成不存在的对话检查点，不在包内保留第二份可写绑定事实 |
| 校验与发布 | 校验路径/数量/大小、完整条目集合、正文引用闭包、对象哈希和结构关系；同目录临时文件同步后重读核对，再原子发布；失败保留原文件和未保存状态 |
| Bevy 归档 | 保存与恢复前固定 Runtime 视图，只同步当前打开文档已连接的流程；绑定、日志和归档变更进入未保存与恢复状态 |
| 打开与控制 | 打开文件只显示固定归档；Bevy 显式连接本机流程后才接收更新和控制运行，当前进程内禁止副本接管其他打开文档的连接 |
| Tauri 归档 | 保留显式“保存到当前项目”和独立只读查看；异步操作校验 `projectId` 与本次打开令牌，普通保存不打包全部活动流程 |
| Linux 缩略图 | 兼容 v1/v2 已知版本与功能；只读画布/图片，不解析协作历史，也不承担编辑器的完整包校验 |

实现依据：[project.rs](../src/project.rs)、[project_package.rs](../src/project_package.rs)、[project_history.rs](../src/project_history.rs)、[document.rs](../src/document.rs)、[画布协作](../src/canvas_ai.rs)、[绑定同步](../src/collaboration/canvas.rs) 与 [Tauri 归档命令](../app/src-tauri/src/collaboration.rs)。

打开 v1 不写磁盘；下一次明确保存 `.mio` 才生成 v2。另存保留旧原件，在原路径保存会在验证成功后替换。旧字段、资产和受支持的不透明扩展继续保留；扩展占用新格式保留路径时拒绝覆盖。`objects/sha256/` 是当前历史与绑定引用的封闭集合，不接收孤立对象；扩展自己的字节应保存在 `extensions/`。

## 本轮执行

| 检查 | 实际结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --release --workspace --locked`，最终集成检查 | 303 通过、27 忽略：2 个 Tauri、4 个 vendored、110 个核心库和 187 个画布测试；含 7 项 package 与 5 项 canvas_ai 回归 |
| `cargo clippy --release --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cargo test --release --locked --lib project_package::tests::repeated_body_size_comparison_uses_the_same_zip_compression -- --nocapture` | 1 通过、115 过滤；具体体积见下文 |
| `/usr/bin/python3 -m unittest discover -s packaging/linux -p test_thumbnailer.py` | 27 通过；包含 v1/v2 画布和图片预览、源文件不变、未知版本/功能及非法类型拒绝，失败保留既有预览 |
| `/usr/bin/ruff check packaging/linux/project-graph-thumbnailer.py packaging/linux/test_thumbnailer.py` | 通过 |
| `/usr/bin/ruff format --check packaging/linux/project-graph-thumbnailer.py packaging/linux/test_thumbnailer.py` | 通过 |
| similarity-rs 相关 6 个 Rust 文件扫描 | 使用本机支持的 `--skip-test -t 0.90 -m 12`，未报告重复；该版本 CLI 不支持 `--cross-file`，未将该选项写成已执行 |
| 文档检查 | 本文、STORAGE、CREDITS 的本地文件链接与行末空白检查通过；STORAGE 第 2 节字段、闭包与大小边界经实现者交叉核对 |
| `git diff --check` | 通过 |

首轮无默认 feature 的库测试曾有 101 项通过、2 个旧格式夹具失败，该轮不计为通过；修正夹具后第一次 workspace 检查为 294 通过、27 忽略。随后补齐 package 安全与 Bevy 保存准备回归，最终结果为上表的 303 通过、27 忽略。Python 27 项由根代理再次运行确认。

本轮未运行真实模型请求，未执行用户文件迁移，也未向系统目录安装缩略图工具。缩略图测试包含临时安装目录及 MIME 数据库检查，不能据此声称已验收真实文件管理器调用。

## 重复正文体积样本

固定夹具包含 4 个流程，32 个正文位置复用同一份 64 KiB 文本；v1 的历史 JSON 与 v2 的记录/对象使用相同的 Deflated ZIP 压缩。两份文件均可读取回相同 History，得到：

| 容器 | 文件字节数 |
| --- | ---: |
| v1 | 1,759,429 |
| v2 | 57,498 |

此样本减少约 96.7%，只证明重复正文的存储去重效果。它不是一般项目压缩率、增量保存或性能基准；未测打开/保存耗时和峰值内存，也不代表图片素材或缺少重复正文的项目会有相同收益。

## 验证边界

- 当前归档是 Runtime 内存视图的固定采集，可以包含未完成状态或尚未持久化的流式进度；包内清单的一致性不等于运行数据库事务的一致性。
- 本机 Runtime 仍使用版本化 JSON 全量快照；SQLite、`storeEpoch/storeCommitSeq`、持久 `instanceId`、分支头 CAS、节点检查点 DAG 和通用上下文闭包均未实现。临时文档令牌与 Bevy 进程内连接不能替代这些契约。
- 既有 Run 证据可以携带，但不构成完整多轮会话；没有 Pi 会话恢复、Git 同步、跨机执行接管或任意历史分叉。本轮也没有新增归档驱动的 Skill 执行能力。
- 哈希用于内容寻址与一致性检查，不是签名。正文按原内容保存，仍可能含路径、源码或秘密；排除运行控制字段不等于自动脱敏。
- 除上述重复正文体积样本外，尚未完成容量矩阵、保存/打开耗时与峰值内存测量；对象去重不能据此宣称提速、减少模型费用或实现增量 ZIP 保存。
- 未验收原生桌面文件对话框、操作系统双击、Windows/macOS 文件关联或断电故障恢复；原子替换不等于所有文件系统上的断电安全。

## 参考核验

本轮联网读取用户指定的官方仓库 [Wood-Q/Git-Agent-Sync](https://github.com/Wood-Q/Git-Agent-Sync)，并以 `git ls-remote` 核验当时 HEAD 为 `131ded1f1b56f51275c23d45d51ca6bd68336633`。只读检查了该提交的 README、关键存储/导出源码和许可证；未安装、执行上游程序或复制代码。

| 固定提交的证据 | 本轮取舍 |
| --- | --- |
| [event-store.ts](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/src/event-store.ts#L12) 按整份会话文件的 SHA-256 保存对象、封存批次 JSONL 事件，再派生索引 | 参考对象与封存记录分层；Ravomi 使用自己的单文件保存清单，不引入 Git 作为保存依赖 |
| 同文件的 [事件回放](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/src/event-store.ts#L116) 跳过损坏行并按墙钟排序，[冲突判断](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/src/event-store.ts#L263) 将同 session 的不同哈希视为冲突 | 不作为事务日志、检查点 DAG 或正常追加/分叉的模型；保留本项目的严格校验，未来事务与分支协议单独实现 |
| [utils.ts](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/src/utils.ts#L10) 的临时写入和重命名 | 该辅助函数没有文件同步和重读校验；不替换 Ravomi 已有的临时文件同步、内容验证与原子发布链路 |
| [conversation-ir.ts](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/src/conversation-ir.ts#L7) 只含 Codex/Claude 适配，[导出模式](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/src/conversation-ir.ts#L50) 当前明确为 readable、resumable=false | 借鉴明确区分可读历史与可恢复执行；不据此承诺 Pi 续聊 |
| [MIT LICENSE](https://github.com/Wood-Q/Git-Agent-Sync/blob/131ded1f1b56f51275c23d45d51ca6bd68336633/LICENSE) | 原文版权行是 `Copyright (c) 2026`，未列署名主体；本轮仅为概念参考，来源记入 [CREDITS](../CREDITS.md) |
