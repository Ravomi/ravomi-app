# SQLite `.mio` 保存验证

日期：2026-10-12；环境：Linux 本地工作区，Rust release 构建。规范见 [STORAGE](../STORAGE.md)，DDL 见 [project_sqlite.sql](../src/project_sqlite.sql)。

## 实现范围

- `.mio` schema 3 使用内置 SQLite 3.53.2 / rusqlite 0.40.2，无独立数据库服务。节点与边按稳定 ID 拆分，端点、分组和运行来源使用有序关系表。
- 按内容哈希共享正文、资产及记录；已有数据库在原文件上事务更新变化行。首次保存、旧 ZIP 升级、另存新路径和恢复代通过临时数据库发布。
- 保留 ZIP `.mio` v1/v2 和 `.prg` 读取；打开旧格式不修改原件，明确保存才升级。保留未知画布字段、旧附件、清单扩展和不透明条目。
- 共用 Document 的 Bevy、Session/Tauri、恢复副本和 ProjectFile 读取接入数据库；Linux 缩略图增加只读 SQLite 画布解析。

## 已执行验证

`cargo test --release --locked --lib --no-default-features project_sqlite`：8 项通过，其中一项为仅由父测试以环境变量激活的子进程入口。覆盖：

| 检查 | 证据 |
| --- | --- |
| 自环、平行边、分组及撤销 | 保存重开后关系与稳定 ID 一致；分组后保存、撤销再保存和恢复代均可重开 |
| 多端点顺序 | 三端点中首尾为同一节点；SQLite 查询顺序与原图一致，未被集合去重 |
| 共享正文及 Run 来源 | 输入/输出共用正文只存一份；Run 顺序、来源和 retryOf 往返一致 |
| 增量写入 | SQLite 测试触发器观测到只更新被编辑节点记录，不更新附件；Unix 文件 inode 保持不变 |
| 重复保存 | 同一已保存 Document 未修改再保存，数据库字节不变 |
| 事务回滚 | 写入后在 metadata 更新注入 SQLite ABORT，旧文件字节不变、文档仍为未保存 |
| 过期写入者 | 两个打开文档发生冲突时拒绝覆盖；直接绕过 Document 预检查仍被事务锁内的基线比较拒绝 |
| 中断恢复 | 子进程 BEGIN 后修改并 cache_flush，直接 process::exit 跳过析构与提交；打开时恢复热日志，回到原文件字节 |
| 拒绝损坏 | 错误内容哈希、未知 user_version/application_id/必需能力、缺失关系索引和无效布局路径均拒绝，原文件保留 |
| 迁移与降级 | ZIP v2 打开不改写，明确保存后为 SQLite；纯画布仍可导出 PRG；旧 ZIP v2 封存校验测试继续用旧格式 fixture |

`python3 -m unittest discover -s packaging/linux -p test_thumbnailer.py`：28 项通过。新增 SQLite 画布生成缩略图、源文件字节不变、内容损坏及未来版本拒绝；既有 ZIP/PRG 预览回归保留。

`ruff check packaging/linux/project-graph-thumbnailer.py packaging/linux/test_thumbnailer.py` 通过。`similarity-rs` 在新数据库模块与相关存储/文档模块之间未报告新增重复，仅报告原有 Document 内的两对相似函数。

最终执行：

- `cargo fmt --all --check`：通过。
- `cargo test --release --workspace --locked`：320 通过、27 忽略；包含 Tauri 2、字体库 4、核心库 123、画布 191 项通过。忽略项包含既有 GPU/字体/教程测试，没有当作本轮通过项。
- `cargo clippy --release --workspace --all-targets --locked -- -D warnings`：通过。
- `git diff --check` 及所改文档的本地链接目标检查：通过。

工作区同时存在其他会话的功能改动；以上是该共享工作区的执行结果，本提交只包含 SQLite 存储相关文件/补丁，不混入其他功能。

## 限制与未验证项

此轮验证是持久化基础，不是大图性能基准：差异准备、打开和往返校验仍全量读取，兼容画布编解码器仍使用内存 ZIP 基线；SQLite 表和索引不保证比压缩 ZIP 更小。增量测试证明主库只改变化行，不代表 CPU、临时表或内存开销随改动数量线性下降。

当前使用 DELETE 回滚日志与 FULL 同步，未启用 WAL；SQLite 打开可在发现热日志时通过写连接回滚未提交页。进程中断测试不能替代真实断电、硬件缓存故障、网络文件系统或各平台崩溃矩阵。未运行 GPU、跨平台 GUI 或真实模型调用验收。

活动 Runtime 仍使用原 JSON；项目数据库保存固定归档，不声称已统一实时运行事实。节点检查点、共享消息序列、分支 CAS、持久实例/租约、懒加载、全项目提交水位与 Git 同步仍待实现。未迁移任何用户项目文件，测试只使用临时目录。

## 选型依据

参考 SQLite 官方的 [应用文件格式](https://www.sqlite.org/appfileformat.html)、[事务](https://www.sqlite.org/lang_transaction.html)、[原子提交](https://www.sqlite.org/atomiccommit.html) 和 [WAL](https://www.sqlite.org/wal.html)，以及 [rusqlite 文档](https://docs.rs/rusqlite/0.40.2/rusqlite/)。本次采用记录级数据库保存，保留旧 ZIP 编解码为兼容入口；未引入 GraphQL 或外部图数据库服务。
