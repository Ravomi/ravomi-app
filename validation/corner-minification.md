# 圆角边框缩小采样

2026-10-12。用户圈出文本节点边框上的断线、亮点与锯齿。

复现环境：Linux x86_64、Xvfb / X11、Mesa lavapipe Vulkan，1280 × 800 窗口，`tutorial-main-3.2.mio` 的“批量连线”区域，约 37% 画布缩放。旧版同一位置小节点轮廓表现为零散亮点。

原因：连续圆角使用 4× 光栅纹理与九宫格拉伸。纹理只有原始层，双线性采样在缩小时无法覆盖整个像素足迹，薄线随采样位置丢失。修复为缓存纹理生成完整 mip 链，沿用线性 mip 过滤。白色 RGB 固定，过滤 alpha 覆盖率；不每帧光栅化，不增加纹理句柄数量，不改变文档尺寸及命中区域。

扩展现有纹理测试，验证各半径的 mip 层覆盖率非零、数据长度完整，同时保留轮廓宽度、纹理复用及空闲帧不改变 Sprite 的检查。

验证命令：`cargo fmt --all --check`、`cargo test --release --workspace --locked`、`cargo clippy --release --workspace --all-targets --locked -- -D warnings`、`cargo build --release --locked`。实际前后截图保存在本机 `results/corner-antialias/`。没有测量帧耗时，也未进行 macOS 实机验收。

结果：308 项通过、27 项忽略、0 项失败，格式、Clippy 和发布构建通过。实际 GPU 截图确认原先零散亮点的小节点边框恢复连续，无新增九宫格接缝；滤波后边缘更柔和。对比：[修复前](corner-minification-before.png) / [修复后](corner-minification-after.png)。

本机 Linux 安装已更新，并核对二进制与发布构建一致；重启后生效。测试在共享工作区执行，其他会话的改动不纳入本功能提交。
