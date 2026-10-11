# 切割反馈修正

> 历史证据：本文保留 Bevy 切割反馈的原始参数、环境与结果，不将其中红色光晕、粒子或碎片作为 Ravomi 新界面的默认动效。当前规范见 [DESIGN](../DESIGN.md) 与 [AI_UI_RULES](../AI_UI_RULES.md)。新界面需要另验收清晰的删除范围、取消与单次撤销、短暂可中断反馈和静止无残留；下列 GPU 结果不能替代新版 UI 验收。

对照 Project Graph master `1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0` 的 `ControllerCutting`、`LineCuttingEffect`、`RectangleSplitTwoPartEffect`、`CollisionBoxRenderer` 与 `WorldRenderUtils`。按用户截图反馈修正刀光过粗、颜色实心和碎片无法淡出。

根因：`ColorMaterial::from(Color::WHITE)` 在当前 Bevy 0.19 中选择 `AlphaMode2d::Opaque`，使共享特效 mesh 的顶点透明度失效。材质显式使用 `Blend` 后，多层光晕、接触粒子和碎片按各自顶点 alpha 混合。

- 拖动刀光为固定 2 个逻辑屏幕像素的白芯、红色透明渐变光晕。
- 释放刀光向终点收束，白芯不超过 2.5 个逻辑屏幕像素并逐渐变窄；红晕外延最多 15 像素。此处按用户反馈覆盖原版 20 世界单位宽度上限，防止放大后变成粗刀条。
- 使用圆角连接及有界刀尖几何，避免锐角描边产生长红刺；光晕复用现有 Lyon mesh，以多层轮廓近似 Canvas 模糊。
- 节点警告框按原版外扩 7.5、描边 8、圆角 16、红色 alpha 0.5；正常缩放采用世界单位，极端缩放采用屏幕单位。释放删除、碎片运动和单次撤销沿用已有逻辑。

没有新增依赖：标准库管理寿命与随机视觉变化，Lyon 负责几何，Bevy `ColorMaterial` 负责透明混合。瞬态效果保留 512 个上限；静止预览不会主动要求连续刷新，瞬态效果结束后恢复按需刷新。

测试包括材质混合模式、多个缩放下白芯与光晕宽度、刀形不自交、警告框尺寸和生命周期。`export_cutting_visual_reference` 导出实际 tessellation 的软件参考图；软件图不作为 GPU 透明混合已生效的证据。

真实 GPU 回归 `gpu_blends_cut_halos_and_fading_debris_at_multiple_zooms` 在 Intel Iris Xe / Mesa 26.2.3 / Vulkan 通过：25%、100%、400% 缩放下白芯不超过 3 个栅格像素、红晕保持半透明且不越界，半寿命碎片正确混合，清空后无残留。100% 捕获保存为 `cutting-preview-gpu.png`、`cutting-release-gpu.png`、`cutting-debris-gpu.png`，黑底用于隔离 alpha 混合行为。

极尖刀形回归还检出 Lyon 圆角连接保留内侧 miter 的边界：宽光晕包住极短刀形时会产生小尖刺。仅在光晕描边输出中将异常内侧法线限制为半描边宽度，并将曲线容差收紧至 0.01；原有 15.02 屏幕像素边界断言保持不变并通过。
