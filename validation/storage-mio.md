# MIO 项目格式验证

日期：2026-10-12。格式规范见 [STORAGE.md](../STORAGE.md)。本次在 Linux x86_64 共享开发工作区验证；工作区还包含其他会话的改动，以下测试总数不是本次新增测试数。

## 已实现与覆盖

- `.mio` 原生 ZIP 项目包：独立格式清单、容器/画布版本、稳定项目 ID、画布、资产、只读协作归档。
- 新建与无后缀保存、原生保存重开、另存、旧 `.prg` 转换、旧原件不变、未知字段/ZIP 条目/资产保留。
- 不支持版本、缺失必需条目、不安全条目名拒绝；失败不替换当前编辑文档或原文件。
- 同一磁盘快照解析；外部修改冲突检测；原生恢复副本与旧 PRG 恢复目录兼容。
- 流程显式选择、无关流程隔离、同流程更新拒绝历史倒退与固定成果冲突、画布撤销不抹去归档。
- 打开归档不恢复运行。项目 ID 与本次打开的 `documentToken` 分离，复制文件后拒绝旧会话请求。
- React/Tauri 的归档按钮、标脏、只读查看和错误重试；深浅主题、100/150/200% 缩放、窄窗与键盘展开。
- Linux 新 MIME、文件筛选和离线预览；旧 `.prg` MIME 保留。

## 本次执行

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --release --workspace --locked` | 287 通过，27 忽略；三个 crate 的 doctest 检查通过 |
| `cargo clippy --release --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cd app && npm run build` | 通过 |
| `cd app && npm test -- --workers=1` | 27 通过 |
| `python3 -m unittest discover -s packaging/linux -p test_thumbnailer.py` | 27 通过 |
| `git diff --check`、安装脚本语法、desktop 文件校验 | 通过 |
| similarity-rs 对新增存储模块及调用点检查 | 新模块未报重复；现有读写/撤销及序列化函数相似项人工保留 |

环境：Fedora Linux 7.2.9-200.fc44 x86_64；Rust 1.98.1、Node 24.21.0、npm 11.19.0。测试复用 release 缓存，没有运行 GPU 基准、真实模型调用或用户数据迁移。前两次 Rust 全量检查的 doctest 阶段遇到暂时的库工件路径错误，最终完整重跑通过；未改工具链或禁用 doctest，未确认其根因。

## 验证边界

前端测试使用明确的模拟 IPC；真实文件读写由 Rust 核心和 Tauri 会话身份测试覆盖。本次未验收桌面原生文件对话框、操作系统双击启动、Windows/macOS 文件关联或断电恢复。归档查看不等于跨机器继续执行；活动 Runtime 仍使用本机 JSON 快照，SQLite、事件日志和完整工作副本绑定尚未实现。
