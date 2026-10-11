# 画布缩放与节点拖动手感调研

> 历史调研与实现证据：以下保留原调研日期、来源、参数和测试结果，不代表 Ravomi 新 UI 已完成验收。当前交互遵循 [DESIGN](../DESIGN.md) 与 [AI_UI_RULES](../AI_UI_RULES.md)；迁移到新界面后增量验证准确落点、连线同步、可中断反馈、键盘焦点和并行任务视图稳定性。产品流程范围见 [PRD](../PRD.md)，本报告未验证 Agent 执行或交接。

查阅日期：2026-10-09。下文区分公开事实与 Graplu 的设计建议；Figma、Miro 的操作说明不能证明其内部缓动算法或时间常数。

## 已公开的实现

| 产品 | 可验证的行为 | 对 Graplu 的启发 |
| --- | --- | --- |
| Figma | 官方说明 Ctrl/Cmd + 滚轮缩放、触控板双指捏合缩放；提供适应画布、适应选择等独立操作。未公布滚轮缓动参数。 | 保持手势和快捷操作一致；不能把主观手感归因于某个未公开弹簧模型。[官方说明](https://help.figma.com/hc/en-us/articles/360041065034-Adjust-your-zoom-and-view-options) |
| Miro | 鼠标模式滚轮缩放，触控板模式双指平移、捏合缩放；允许自动切换及手动选择输入设备。 | 鼠标刻度与连续触控输入应分开处理。[官方说明](https://help.miro.com/hc/en-us/articles/360017731053-Using-Miro-with-a-mouse-trackpad-or-touchscreen) |
| tldraw | 相机配置区分平移/缩放行为及灵敏度。当前源码收到 wheel 会停止已有相机动画，按当前倍率比例计算增量，保持指针锚点并立即设置相机；不能将按钮/程序导航动画视为滚轮缓动。 | 平滑并不等于必须增加惯性；响应速度、增量归一化和锚点稳定同样重要。[相机文档](https://tldraw.dev/sdk-features/camera)、[wheel 分支源码](https://github.com/tldraw/tldraw/blob/035b741dc331bf76a4cb9af27c346be10bba2a88/packages/editor/src/lib/editor/Editor.ts#L10450) |
| Excalidraw | `App.wheel.ts` 对大滚轮增量限幅，在高倍率下调整步长，对小触控增量减弱放大效果；保留事件当时的指针位置，通过函数式状态更新累积事件。该路径调用 viewport 更新，未启动普通逐刻度缓动。 | 避免突跳和丢失同帧连续输入；缩放应始终围绕事件锚点。[滚轮源码](https://github.com/excalidraw/excalidraw/blob/4c00f31ddcc20086bac1020428819fe0b9a30bce/packages/excalidraw/components/App.wheel.ts#L112)、[viewport 更新](https://github.com/excalidraw/excalidraw/blob/4c00f31ddcc20086bac1020428819fe0b9a30bce/packages/excalidraw/components/App.viewport.ts) |
| MapLibre GL JS | 鼠标 wheel 累计目标 zoom，使用 200 ms 缓动；重复输入重新从当前状态向新目标收敛并调整曲线连续性。trackpad 直接应用目标值；返回 `noInertia: true`，不会再叠加另一层惯性。默认围绕指针缩放。 | 为离散滚轮补连续帧是明确可借鉴的实现；地图的 200 ms 只是参考，不能直接认定适合节点编辑。[源码](https://github.com/maplibre/maplibre-gl-js/blob/379b3673a4f0982472ae91d774db5bd42931583d/src/ui/handler/scroll_zoom.ts#L265) |

## Graplu 的问题与建议

调研开始时，[navigation.rs](../src/navigation.rs) 的 Line 事件立即执行 `scale *= exp(-y * 0.12)`，一格约改变 12.75% 的视觉倍率。指数映射已经保证比例缩放，`zoom_at` 也已有指针锚点补偿；缺的是两次刻度之间的时间过渡。降低 0.12 只改变灵敏度，不能消除跳变。

建议只给 Line 滚轮加入短暂、可打断的目标收敛，在 `ln(scale)` 空间处理；连续输入累加到目标，不能每帧从当前倍率重新算全部目标而丢掉尚未完成的量。本次实现初始选择 45 ms 的一阶时间常数，约 135 ms 完成 95% 的剩余量；首次响应限制使用的帧间隔，避免应用空闲后首帧直接跳到终点。这是 Graplu 自选参数，并非上述产品的参数。

每帧重新保持同一锚点，不单独缓动相机平移；Pixel 平移、Ctrl/Cmd + Pixel 缩放及原生捏合继续直接响应。反向滚动应及时制动，UI 捕获、失焦、新拖动、适应画布或外部相机修改应取消或重设待完成目标。缓动期间持续重绘，收敛后回到空闲刷新；时间衰减按秒计算，避免 60/144 Hz 手感不同。

## 节点的轻微质量感

准确落点应优先。Excalidraw 的节点移动根据按下时的原始位置加共同偏移，统一处理多选、容器成员和绑定边；tldraw 的形状平移随 pointer move 更新，pointer up 完成交互。两者所查路径都没有把节点作为释放后继续飞行的物体。[Excalidraw 拖动实现](https://github.com/excalidraw/excalidraw/blob/4c00f31ddcc20086bac1020428819fe0b9a30bce/packages/element/src/dragElements.ts#L35)、[tldraw Translating](https://github.com/tldraw/tldraw/blob/035b741dc331bf76a4cb9af27c346be10bba2a88/packages/tldraw/src/lib/tools/SelectTool/childStates/Translating.ts#L120)。

针对用户要求，建议把质量感实现为很轻的阻尼跟随：快速拉动时略微滞后，慢速精调时接近直接跟手；限制屏幕空间滞后距离，避免放大后脱离鼠标。本次实现初始选择 28 ms 时间常数、至多 12 个逻辑像素的跟随残差，并沿用原有物理速度上限。释放时固定最后指针对应的目标，短促收尾到该落点，禁止额外抛掷位移。这些参数和行为是 Graplu 的设计取舍，不是声称上述产品已有同款效果。

多选与嵌套组必须共享一个位移和同一时间进度；背景、文字、边端点、选择框一起移动。释放、取消、删除、撤销以及物理布局接管时清理状态，一次拖动只产生一条历史记录。建议验证不同刷新率、倍率和 DPI 下的跟随、精确落点、松手即撤销、多选相对位置及失焦恢复。

## 复用范围

这里需要的是现有导航/拖动系统的目标状态与少量插值集成；标准库 `f32::exp_m1`、Bevy 已有时间/向量设施以及项目已经接入的 Avian 物理系统足够。上述 JavaScript 画布/地图库不适合作为 Rust 桌面输入依赖；不移植整套引擎，不新增通用动画库/物理库，也不复制第三方源码。先保留当前事件分流和文档事务，再加入必要的时间状态。本文记录依据与初始参数，不代替实现的测试结果。

## 验证

2026-10-09，release 模式下 navigation 的 13 项测试通过：含多 DPI 锚点、连续刻度、反向、30/60/144 Hz 相同时间的收敛、空闲唤醒、输入接管与停止持续重绘。physics 的 13 项测试通过（包含 ignored 教程整组 fixture）：含不同倍率的屏幕空间跟随、松手准确收尾、方向反转、锁定障碍物、嵌套组和一次撤销/重做。拖动阻尼目前随默认开启的推挤模式生效；关闭推挤仍直接拖动。手感参数仍需实际鼠标体验反馈，自动测试验证的是运动与事务行为。
