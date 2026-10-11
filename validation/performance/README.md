# 本机性能初测：2026-10-09

> 历史性能证据：本目录保留 Graplu 基线的原环境、样本、名称与测量结果，不证明 Ravomi 新 UI 或多 Agent 工作流已经实现或达标。产品与阶段边界见 [PRD](../../PRD.md)，新界面的可读性、状态与交互要求见 [DESIGN](../../DESIGN.md) 和 [AI_UI_RULES](../../AI_UI_RULES.md)。设计落地后须以同产物、同场景、同窗口条件新增测量，并检查品牌柔光不引入全局模糊或持续重绘；下文历史数字不改写为新结果。

这是短时诊断记录，不是性能倍数或完整视觉等价性的证明。每份文件保留一次有效的四阶段测试，每阶段约 4 秒，未做多轮统计。Graplu 的缩放尾延迟仍需优化。

## 环境与样本

- i9-13900H，20 逻辑线程，接通电源；Intel Iris Xe / Vulkan / Mesa 26.2.3。
- Graplu：Bevy 0.19.1，发布模式；基于 `36f4395`，另含本目录的 PSS 采样改动。
- 原版：桌面安装的 `/usr/bin/project-graph`，RPM `project-graph-4.2.4-1.x86_64`，CEF / Canvas2D。上游 master 仅用于实现参考，并非所测二进制版本。
- 两边均保持焦点、持续绘制；Graplu 开启诊断和 UI，关闭物理，X11，1279×799，缩放因子 1，请求 AutoNoVsync。
- 原版窗口约 1279×800 物理像素，DPR 1.75，画布约 1277×700；菜单、视口、LOD 与 Graplu 不完全一致。截图只用于视觉检查，不能逐像素比较。
- 没有同时编译或运行其他主动测试负载，但未完全隔离桌面后台负载。

| Desktop 文件 | 节点 | 连线 | 附件 | 输入 SHA-256 |
| --- | ---: | ---: | ---: | --- |
| tutorial-main-3.1.prg | 636 | 391 | 24 | `4056f132f322cfce0f29fdb7e8035543212ba0785e409aa72f273fefbbe1fd05` |
| tutorial-main-3.2.prg | 614 | 375 | 24 | `8c37cdfe29cdd7ed9766189c8ce63508a1c3b4c06fd720894af30da06d67f694` |

原版打开的是 `/tmp` 副本：3.1 要求从文件格式 2.4.0 升级到 2.7.0；3.2 也发生自动保存。不要把原版运行后的副本哈希当成输入哈希。桌面源文件未由测试保存覆盖。

## 实际绘制间隔

单位 ms，表格为平均 / P95，越低越好；不是输入延迟或单独 GPU 时间。

| 场景 | Graplu 3.1 | 原版 3.1 | Graplu 3.2 | 原版 3.2 |
| --- | ---: | ---: | ---: | ---: |
| 静止持续绘制 | 3.47 / 4.34 | 16.70 / 23.90 | 3.24 / 4.06 | 16.66 / 22.70 |
| 平移 | 3.57 / 4.50 | 16.67 / 23.10 | 3.29 / 4.09 | 16.69 / 22.70 |
| 缩放 | 18.62 / 33.05 | 16.68 / 21.80 | 17.33 / 31.81 | 16.68 / 23.00 |
| 批量移动 | 6.49 / 9.44 | 16.72 / 23.20 | 5.36 / 8.26 | 17.30 / 22.30 |

原版实际绘制约受限在 60 FPS，而浏览器 rAF 约 120 Hz。因此不能用此表宣称 Graplu 快 5 倍。有效测量包装 `renderer.tick_`，记录真正绘制的开始时间及函数 CPU 时长；`raf` 单独保留。原始 JSON 的顶层 `timing` 文案仍是早期的 rAF 描述，**应以 `phases` 的实际 tick_ 测量及本说明为准**；复测脚本已修正文案。

曾观察到失焦时 rAF 继续但渲染暂停，相关试跑已排除。原版 3.2 第一次聚焦试跑的静止/平移异常低频，也未作为稳定结果；这里采用后续持续聚焦的一轮，不能替代重复测量。

原版 `renderer_cpu_ms.mean_ms`：3.1 各阶段约 6.34、6.34、5.57、4.67；3.2 约 5.45、5.15、4.89、4.11。这只覆盖渲染函数，不能直接与 Graplu 全部应用调度耗时相比。

Graplu 缩放的应用调度平均耗时为 15.87 ms（3.1）、14.77 ms（3.2），GPU 主绘制约 0.69 / 1.09 ms，瓶颈主要在 CPU。缩放阶段字形生成、MSDF 网格和边网格重建计数均为 0；尚未定位到具体函数。批量移动会引起分组标题重新布局，不能声称所有文字网格都复用。

## 冷字形生成

`msdf-3.2-cold.jsonl` 使用更早的 3.2 快照，SHA-256 `a1d87dbc83b9b431a4d8f6c4190b204299ade65f6336662a78fb845577a21bc8`，不是上表帧测试的快照。

NotoSansCJK-Regular.ttc 第 0 个 face，633 个不同非控制字符，632 个字形；单字体缺少 U+1F3C3。64 texels/em、距离范围 8。每次清空生成结果，以字形队列分配工作，每档 3 次。

| 工作线程 | 中位耗时 |
| --- | ---: |
| 1 | 5095.50 ms |
| 4 | 1393.23 ms |
| 8 | 857.63 ms |

8 线程比 4 线程减少约 38.4% 耗时。各轮输出均为 10,994,072 RGBA 字节，校验和 `740fbffba5ef8f69`。这是现有 Rust 生成器的并行测量，**不包含 AVX2/FMA 优化**，也不包含字体读取、排版、GPU 上传或完整应用启动。

Graplu JSON 的 `load_ms` 约 2346 / 2364 ms，只是 Bevy 时钟起算至 MSDF 就绪，不能当作完整冷启动；没有对应原版冷启动测量。

## 内存与正确性限制

3.1 Graplu 各阶段末 PSS 约 497、497、733、780 MiB；RSS 约 532、532、767、815 MiB。3.2 记录尚未包含 PSS。

`original-3.1-memory.txt` 是原版一次测试后的各进程 RSS，合计约 1683 MiB，但共享页会被重复计数，未获得对应 PSS，不能据此给出物理内存节省比例。原版 3.2 多轮之后的内存不用于比较。

两份文件的导入与另存副本检查成功，压力涂鸦和 3.1 弧线已实现。仍存在以下视觉差异：

- 这些性能数据采集时彩色字形已被识别为正常回退，但之后 GPU 截图发现本机 COLRv1 emoji 被 Bevy 栅格化为空图块。后续已使用内嵌 CBDT 字体修复，见 [彩色截图](../msdf-color-emoji.png)。**本表是修复之前的基线，不能冒充完整彩色渲染的新测量。**
- SVG 加载报告找不到 serif / Verdana 字体，可能缺失内嵌 SVG 文字。保存附件不等于完整视觉还原。
- 本地旧教程 fixture 哈希与固定下载版本不符，ignored 导入测试预期 623 个对象而读到 624 个；未覆盖该文件或放宽断言。正常工作区测试通过。

## 复测入口

先完成编译，再单独运行测量，保持窗口焦点。以下命令均从仓库根目录执行；文件及输出路径按实际选择。

```sh
cargo run --release --locked --example inspect_prg -- /tmp/input.prg /tmp/text.txt
cargo run --release --locked -p kayak_font_msdf --example cold_glyphs -- /tmp/text.txt /usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc
WINIT_UNIX_BACKEND=x11 target/release/project-graph-bevy-canvas /tmp/input.prg --benchmark --diagnostics --output /tmp/graplu.json
```

原版必须使用可丢弃的副本，因为升级和拖动会触发自动保存。Node 24 原生 WebSocket 执行 CDP 诊断脚本：

```sh
PROJECT_GRAPH_CEF_DEBUG_PORT=47183 project-graph /tmp/disposable-copy.prg
node validation/performance/cef-evaluate.mjs 47183 validation/performance/cef-select-project.js
node validation/performance/cef-evaluate.mjs 47183 validation/performance/cef-phases.js
```

`cef-select-project.js` 依赖该安装版 React 内部结构。先选中目标文档并聚焦窗口，脚本找不到项目就报错；确认每阶段 `unfocused_frames` 为 0。采样结束还原相机和节点位移，但自动保存仍可能在测量过程中发生。原版每 rAF 更新操作、Graplu 每帧更新操作，并非固定频率输入回放；两边绘制范围、交互调度和内容细节仍需统一后再做严格对比。
