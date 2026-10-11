# 原生画布仅深色

2026-10-12：用户确认画布也暂时只提供深色。原生启动配置固定深色，移除画布工具栏与设置中的主题切换；设置展示“外观：深色”。保留底层浅色 token，不改写文档颜色与素材。

本次验证环境：Linux x86_64，Xvfb / X11 / Mesa lavapipe，实际发布二进制。未进行 macOS 实机验收。

- 不传 `--dark`，直接打开已有 `.mio`：深色画布、浅色正文与连接，工具栏无明暗切换。
- 首页新建：空画布保持深色。
- 首页设置：深色设置面板，外观为“深色”，无浅色模式入口。
- 执行 `cargo fmt --all --check`、`cargo test --release --workspace --locked`、`cargo clippy --release --workspace --all-targets --locked -- -D warnings` 及发布构建。

结果：307 项测试通过、27 项忽略、0 项失败；格式、Clippy 与构建通过。测试在共享工作区执行，其他会话修改不纳入本次提交。截图与操作脚本位于本机 `results/home-dark-reference/dark-*`。

发布构建通过 Linux 安装脚本更新本机，安装二进制与构建产物一致。已有运行窗口不强制结束，重启后应用新默认主题。
