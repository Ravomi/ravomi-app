# Linux 窗口与 GPU 诊断

此参考服务于 Ravomi 的 Bevy 基线与相关输入适配；先按 [开发指南](../../../../DEVELOPMENT.md) 区分实际入口，不能把 Vulkan 结果套用于 Tauri WebView。UI 清晰度与交互期望以 [DESIGN](../../../../DESIGN.md)、[AI_UI_RULES](../../../../AI_UI_RULES.md) 为准，产品状态以 [PRD](../../../../PRD.md) 为准。以下诊断命令和历史环境只提供取证方法，不表示新 UI 或 Agent 已验收。

## 最小环境记录

只采集与故障有关的环境变量，不输出完整环境。以下命令只读取状态：

```sh
printenv XDG_SESSION_TYPE XDG_CURRENT_DESKTOP WAYLAND_DISPLAY WAYLAND_SOCKET DISPLAY
printenv WGPU_BACKEND WINIT_X11_SCALE_FACTOR VK_DRIVER_FILES VK_ICD_FILENAMES
uname -r
command -v vulkaninfo wayland-info
```

`printenv` 遇到未设置的变量、`command -v` 遇到未安装的工具可能返回非零；这是缺失信息，不是应用错误。工具存在时按需运行：

```sh
vulkaninfo --summary
wayland-info
```

`vulkaninfo` 说明系统可枚举的 Vulkan 设备；实际应用使用哪一块 GPU，仍以应用日志或 Bevy `RenderAdapterInfo` 为准。记录适配器名称、backend、device type 和驱动信息。软件适配器可以用于隔离问题，其结果不可与硬件 GPU 混作性能结论。

`wayland-info` 用于核对合成器公开的协议和输出信息；工具缺失时可使用合成器已有诊断能力，不为完成信息表自动安装整个 SDK。

## 后端对照

先读程序已有的 winit 配置。winit 0.30.13 在没有显式强制后端时优先选择非空 `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` 所指的 Wayland，然后才是 `DISPLAY` 所指的 X11；相应后端必须已编译启用。不要直接照搬旧版 `WINIT_UNIX_BACKEND` 环境变量。

以下为独立启动命令模板：把 `./existing-app` 和参数换成已核对的二进制与同一份测试数据，串行运行，关闭前一次测试窗口。仅在有对应桌面连接时使用。

原生 Wayland + Vulkan（保留当前有效 Wayland 连接，移除 X11 候选）：

```sh
env -u DISPLAY WGPU_BACKEND=vulkan ./existing-app
```

X11 + Vulkan（保留 `DISPLAY`，移除两个 Wayland 连接变量）：

```sh
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET WGPU_BACKEND=vulkan ./existing-app
```

第二条命令在 Wayland 桌面中通常经 XWayland，在 X11 桌面中则是原生 X11。程序显式选择后端时，环境变量不能覆盖该选择；应核对项目代码和启动日志，必要时在允许修改代码的任务中记录实际 raw display handle 的类型。

`WGPU_BACKEND` 经 Bevy 默认 `WgpuSettings` 读取；若应用自行覆盖 `backends`，以配置和实际适配器为准。环境变量值不是验证结果。保持所有调整仅对当前进程生效，不默认写入 shell 配置、桌面启动器或合成器全局设置。

需要固定 X11 比较的缩放时，可在第二条命令上单独增加 `WINIT_X11_SCALE_FACTOR=1`；这不是原生 Wayland 的缩放控制，也不能代替记录实际物理尺寸。先做同尺寸后端比较，再单独研究缩放，避免同时改变多个变量。

项目已有产物的候选路径（从仓库根目录解析，使用前检查存在且与待测代码对应）：

```text
target/release/project-graph-bevy-canvas
fixtures/tutorial-shortcut-keys-3.1-original.prg
```

读取项目当前的参数解析和 README 后再追加 `--benchmark`、`--preview`、`--output` 等选项；不要假定所有 Bevy 程序支持这些选项。用新的输出路径保留旧基线。

## 黑屏、设备和呈现错误

按故障发生阶段缩小范围：

1. 连接桌面之前失败：核对桌面连接变量、进程会话、已启用窗口后端和动态库错误。
2. 设备创建失败：核对 Vulkan loader、可用驱动、实际适配器与请求的 features/limits。不要通过禁用校验或强制所有可选 features 掩盖失败。
3. 设备成功但没有画面：检查首帧呈现、相机激活、渲染目标、窗口尺寸和资源加载状态。截图是否含画面可帮助区分渲染与呈现，但要说明截图捕获的是哪一层。
4. 调整尺寸或恢复窗口后出错：记录是否出现零尺寸、surface lost/outdated/timeout 等信息。查当前 Bevy/wgpu 的恢复路径，再决定是否需要应用层处理，避免竞争性地重复配置 surface。

需更多日志时，单次运行可使用项目支持的 `RUST_LOG`，例如 `info,bevy_render=debug,wgpu_core=debug,wgpu_hal=debug`。诊断日志和 validation layer 会改变开销；定位结束后关闭再测性能。Wayland 协议日志只在必要时短时开启，可能包含输入文本，应避免公开未脱敏的原始记录。

## 公平测量

- 同一份数据、同一产物/优化配置、同一 GPU、同一实际物理窗口尺寸、同一操作轨迹。记录 UI、文字编辑和物理是否开启。
- 分离首次资源加载/字形准备与稳态采样；重复串行测量。测量期间不编译，不同时开启多个压力测试窗口。
- 分别测静止、平移、连续缩放、节点拖动；需要时再增加长文本、图片、大图和物理场景。按需刷新的静止窗口不能用帧率衡量交互吞吐。
- 记录均值、p95/p99、样本数和持续异常区间。记录窗口焦点、显示器刷新率、请求的 present mode、实际模式（能取得时）和电源状态；取不到实际模式就标为未知。
- `AutoNoVsync` 有回退行为；不要仅因为配置名或报告中的 `vsync: false` 就宣称实际呈现完全不受同步限制。
- 墙钟帧间隔包含调度、CPU、GPU 等待及呈现影响，不能称作纯 GPU 时间或输入延迟。CPU 渲染跨度也不等于整帧 CPU 耗时。
- Bevy 0.19.1 的 `RenderDiagnosticsPlugin` 可在 Vulkan/DX12 路径提供 GPU 时间；还需核对实际设备能力和启用情况。缺失 GPU 指标表示未测得，不填成 0。
- 检查文字布局变化次数、物理步数/耗时、活动刚体、可见对象和 RSS 等已有指标，按证据选择后续采样。帧时间下降但文字清晰度、曲线或特效减少时，同时报告功能/质量差异。
- RenderDoc 用于检查具体帧、资源和绘制调用。采集帧的速度不能直接作为正常运行的性能基线。

## 结果解释

后端 A/B 仅证明问题与某条路径相关，不能单凭“XWayland 正常”就认定是合成器 bug，也不能单凭“Vulkan 更快”断言 CPU 无瓶颈。下一步应在差异层定位，例如坐标转换、输入事件、surface 配置或驱动。

修复后复测原始失败操作以及相邻状态：调整尺寸、失焦/恢复、IME 开关、物理停止/唤醒。未有图形会话、真实输入法或匹配二进制时，分别说明哪些只做了静态检查。

## 官方依据

- [winit 0.30.13 Linux 后端选择](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/linux/mod.rs)
- [winit 0.30.13 X11 平台文档与缩放变量](https://docs.rs/winit/0.30.13/winit/platform/x11/index.html)
- [Bevy 0.19.1 渲染初始化设置](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_render/src/settings.rs)
- [Bevy 0.19.1 窗口、缩放及 PresentMode](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_window/src/window.rs)
- [Bevy 0.19.1 渲染诊断](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_render/src/diagnostic/mod.rs)
- [Khronos Vulkan Tools：vulkaninfo](https://github.com/KhronosGroup/Vulkan-Tools#vulkaninfo)
- [Wayland 工具：wayland-info](https://gitlab.freedesktop.org/wayland/wayland-utils)
