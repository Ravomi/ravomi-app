# MIO-only 与已安装节点反馈验收

2026-10-12，Linux。文件支持变更提交 `314d8dc`，JSON 浮点往返修复 `61630e1`。使用当前工作区构建（其他会话未提交改动保留，未纳入上述提交），执行 `cargo build --release --locked --bin project-graph-bevy-canvas` 与 `bash packaging/linux/install-app.sh`。

构建产物及 `~/.local/bin/graplu` 的 SHA-256 均为 `6e3d5f1078ea0d62c78c6926dd01d700fb08a7ef3336a0bbb70ab387759398ac`。桌面入口的 `MimeType` 为 `application/x-ravomi;`。已安装命令行以 `.prg` 路径启动返回退出码 1，并显示“仅支持 .mio 项目，不再支持 .prg 画布”。

## 当次检查

- `cargo fmt --all --check`、`cargo test --release --workspace --locked`、`cargo clippy --release --workspace --all-targets --locked -- -D warnings` 通过。工作区测试合计 336 通过、28 忽略；未执行 GPU/教程等可选用例。
- 前端 `npm run build`、`npm test` 通过，27 项 Playwright 用例通过；这些用例不代表原生 IPC 验收。
- Linux 缩略图 26 项测试：25 通过、1 个可选 MIO 教程夹具测试跳过；Ruff 检查与格式检查通过。几何绘制测试使用模拟 SQLite 投影，格式读取另有真实 ZIP/SQLite 夹具覆盖。
- Rust 回归覆盖拒绝 PRG、改名为 MIO 的 PRG、保存/恢复失败不覆盖原件与当前会话；原先基于 PRG 的编辑往返回归已使用 MIO。

## 已安装程序的独立窗口

用户截图中的额外轮廓对应已提交修复 `ecc8bf6` 删除的新建闪光与切割节点描边。检查发现旧安装文件时间为 14:37，早于该提交的 14:48。

更新安装后，在 Xvfb `:94`、1280×800、1× 缩放、X11 与 llvmpipe 软件 Vulkan 中启动同一安装文件，通过 XTest 操作。测试状态目录位于 `/tmp`，未打开或保存用户项目，不用于性能结论。

1. 首页新建画布，双击空白处创建空白节点并进入编辑：[新建编辑截图](mio-only-install/creation.png)。仅出现正常节点/编辑边框，没有向右扩出的闪光轮廓。
2. Enter 结束编辑，右键按住并跨过节点：[切割预览截图](mio-only-install/cutting.png)。保留切割线，没有额外胶囊形节点描边。

截图为本次安装程序实际窗口。未据此宣称完成原生 Wayland、高 DPI、真实中文输入法或跨平台矩阵验收。测试进程与虚拟显示已退出。
