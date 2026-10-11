---
name: bevy-wayland-vulkan
description: 开发和排查 Linux 上 Bevy 应用的 Wayland/XWayland 窗口与 Vulkan 渲染问题，包括 winit 后端选择、分数缩放、中文 IME、窗口重绘、wgpu 设备选择和帧耗时。适用于 Bevy 画布与桌面应用；不用于合成器协议实现或通用 Vulkan 计算内核开发。
---

# Bevy + Wayland + Vulkan

以可复现的窗口与渲染证据定位问题，优先使用 Bevy、winit、wgpu 和现有 UI 集成提供的能力。

## 版本与层次

先读取目标项目的 `Cargo.toml`、`Cargo.lock` 和适用的 `AGENTS.md`。本技能仅安装于 Ravomi 项目；编写本技能时锁定 Bevy 0.19.1、winit 0.30.13、wgpu 29.0.4、bevy_egui 0.42.0。它们是核对起点，实际实现以当前锁文件为准；不要为了套用示例升级依赖。

当前仓库同时存在 Bevy 画布、React/Tauri 迁移入口与 Wry/React 文字输入层，先按 [开发指南](../../../DEVELOPMENT.md) 确认被测入口。产品和规划边界见 [PRD](../../../PRD.md)；涉及界面的修复按 [DESIGN](../../../DESIGN.md) 与 [AI_UI_RULES](../../../AI_UI_RULES.md) 验收。猫形象的局部柔光不构成正文、连线或控件模糊的理由，历史 Catppuccin 截图也不能替代新规范。

区分两个独立选择：

- **窗口路径**：Bevy → winit → 原生 Wayland，或 X11 → XWayland → 合成器。
- **渲染路径**：Bevy → wgpu → Vulkan → GPU 驱动。

`WGPU_BACKEND=vulkan` 不会选择 Wayland。`XDG_SESSION_TYPE=wayland` 只说明桌面会话，不能证明应用窗口使用原生 Wayland。Canvas2D、Godot 也可能使用 GPU，不以语言或引擎名称推断性能瓶颈。

## 按问题选择检查

| 现象 | 先收集的证据 | 下一步 |
| --- | --- | --- |
| 无窗口、黑屏、启动失败 | 首个错误、实际窗口后端、GPU/驱动、surface 初始化 | 对照原生 Wayland 与 XWayland；保留 Vulkan 后端不变 |
| 模糊、点击偏移、缩放中心漂移 | 物理/逻辑窗口尺寸、系统 scale factor、应用 override、egui 缩放、相机缩放 | 追踪坐标转换，检查是否重复乘除缩放因子 |
| 中文不能输入、候选框错位 | 文本焦点、IME 预编辑/提交事件、候选框位置、UI 输入捕获 | 检查 bevy_egui/winit 桥接，用真实输入法验收 |
| 拖动或缩放卡顿 | 帧间隔分布、CPU/GPU 时间、文字布局次数、物理活动量 | 先定位每帧新增工作，再做单变量对照 |
| 静止时耗电、动画不更新 | `WinitSettings`、窗口焦点、重绘请求、活动动画/物理 | 区分按需刷新与持续更新，检查停止和唤醒条件 |

需要具体采集命令、后端对照、驱动或性能诊断时，读取 [Linux 诊断参考](references/linux-diagnostics.md)。只执行当前问题需要的部分。

## 窗口、缩放与输入

- 分别记录窗口物理像素、逻辑像素、系统报告的基础缩放和应用覆盖后的缩放。`physical = logical × effective scale factor`；界面缩放与画布世界缩放另行记录。
- Bevy `Window::cursor_position()` 返回逻辑坐标；`physical_cursor_position()` 返回物理坐标。相机转换和 UI 布局必须使用各自要求的坐标单位，不凭视觉偏移添加固定补偿。
- 有自定义 viewport 时，核对 viewport 原点和尺寸；优先使用相机的 viewport/world 转换 API，先查当前版本的输入单位和返回类型。
- 分数缩放问题应覆盖窗口调整尺寸、跨不同缩放显示器移动、鼠标锚点缩放和拖动。不能用强制 1× 缩放替代高 DPI 修复；1× 只作为诊断对照。
- 候选框位置应跟随文字插入点，转换到窗口客户区坐标；不直接传世界坐标。若 bevy_egui 已管理 IME，不再添加竞争的窗口设置系统。
- 预编辑文本和最终提交文本分开处理。确认、取消、撤销和保存快捷键要考虑 IME 消费事件；组合输入期间的 Enter/Escape 不应误触画布命令。
- 用真实中文输入法检查预编辑、选词、提交、取消、失焦和重新聚焦。直接注入字符串只能测试文本更新，不能证明输入法集成正确。

## 渲染与调度

- 平移和缩放优先改变相机或对象变换，保留可复用的文字布局和字形资源。检查文字是否在每帧被重新赋值、换字号或布局，不把 `Changed<T>` 计数直接解释为 GPU 上传字节数。
- 根据实际清晰度需求评估字形分辨率；固定字形图集的缩放速度不等于任意倍率下的文字质量。
- GPU 调试沿 wgpu 抽象层进行，先看适配器信息、validation 错误和已有诊断。只有证据指向底层互操作需求时才引入直接 Vulkan 调用。
- 依赖更新顺序时显式声明系统顺序。休眠时减少刷新，交互、动画和物理运行时保证有持续更新；检查鼠标释放、失焦、取消事务后的状态收束。
- Wayland 窗口需要绘制并呈现后才会显示；首帧未提交时，先查相机、surface、资源准备和重绘，不立即归因于窗口管理器。

## 本项目的执行边界

遵守当前仓库 AGENTS.md 与用户授权。先读启动器，当前 `./run.sh` 会执行 `cargo run --release`，会按需编译。运行已有二进制前核对路径和产物来源；缺少匹配产物时明确说明未完成运行验证，不把旧产物当成新代码测试。

已有 `viewer.rs`、`editor.rs`、`physics.rs`、`stress.rs` 分别承载画布、编辑、物理和测量逻辑；优先在现有边界定位问题。不要为诊断重写窗口层、渲染器或物理求解器。

## 验证与交付

记录复现操作、版本/产物来源、实际窗口和渲染后端、修复前后数据及尚未验证的范围。性能结果应附场景规模、实际物理分辨率、测量阶段与分位数。截图验证画面，计时验证性能，两者不能互相替代。

只修改支撑已确认原因的配置或代码；运行适用的检查，不执行项目禁止的构建。按项目要求将独立修复单独提交。

## API 核对入口

- [Bevy 0.19.1 窗口与坐标实现](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_window/src/window.rs)
- [Bevy 0.19.1 WgpuSettings](https://docs.rs/bevy/0.19.1/bevy/render/settings/struct.WgpuSettings.html)
- [winit 0.30.13 Wayland 支持](https://docs.rs/winit/0.30.13/winit/platform/wayland/index.html)
- [bevy_egui 0.42.0](https://docs.rs/bevy_egui/0.42.0/bevy_egui/)

锁文件变化后，改读对应版本的官方文档。其他 Bevy 技能可以补充 ECS 或渲染知识，本技能不依赖它们的安装位置。
