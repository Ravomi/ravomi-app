# 已安装版本：空白文字块编辑验收

2026-10-12，Linux。本次使用 `cargo build --release --locked --bin project-graph-bevy-canvas` 构建，再通过 `bash packaging/linux/install-app.sh` 安装。应用菜单名称为 Ravomi，可执行文件保留兼容路径 `~/.local/bin/graplu`。构建与安装文件 SHA-256 相同：`a2f92d9663445c5c585e5644541d633087e7dda80877d71e63c8706f6956c87c`。

## 本机启动

在用户的 niri / 原生 Wayland 桌面启动已安装程序，Intel Iris Xe、Mesa 26.2.3、Vulkan。屏幕为 2880×1800，系统缩放 175%；首页正常显示。此项是实际 GPU 启动验证，不代表已完成此缩放下全部编辑交互验收。

## 独立窗口交互

为避免重复输入影响用户正在使用的桌面，在 Xvfb `:93`、1280×800、1× 缩放中运行同一安装文件。虚拟显示不支持 Intel Vulkan 的 DRI3 呈现，因此明确选择 llvmpipe 软件 Vulkan；不将此会话用于性能结论。通过 XTest 向该独立窗口发送鼠标和键盘事件，并显式设置窗口焦点。

1. 首页新建未命名画布，双击空白处创建文字块，不输入文字。
2. Enter 完成，再双击块内部进入编辑。两次鼠标按下相隔约 400 毫秒。
3. 连续重复完成、双击编辑 5 轮，点击点在块内部偏移 6 像素。
4. 输入 `test`，Ctrl+A、Backspace 清空，Enter 完成。

结果：每次进入编辑均出现编辑提示；完成后提示消失。上述 10 张检查截图中，节点表面纯色像素的包围盒均为 `(545, 310)–(685, 489)`，未发生宽高或位置跳变；这是表面像素范围，不是文档坐标或外描边尺寸。测试未保存或覆盖用户项目。

- [空白编辑中](empty-text-node/editing.png)
- [完成编辑](empty-text-node/finished.png)
- [再次双击编辑](empty-text-node/reopened.png)

本次未验证真实中文输入法、跨屏缩放、Wry/React 输入层，也未据此宣称所有双击丢失场景均已排除。此前本轮修复的格式检查、工作区测试和 Clippy 均通过；本文件记录的是随后执行的安装与窗口验收。
