# 验证记录

本文件是按日期保留的工程证据索引。历史段落中的“当前”、Graplu / H&M 名称、配色、界面和测试结论均指对应测试当时的版本；它们不自动证明 Ravomi 新 UI 或 Agent 产品能力已经验收。原有日期、环境、结果、截图和测试数保持原样。

当前产品契约见 [PRD](PRD.md)，UI/UX 验收依据见 [DESIGN](DESIGN.md)，AI 生成与实现界面的执行规则见 [AI_UI_RULES](AI_UI_RULES.md)，品牌资源状态见 [BRAND](assets/branding/BRAND.md)。实际工程入口见 [DEVELOPMENT](DEVELOPMENT.md) 与 [app/README](app/README.md)。AI 执行与编排尚未接入；对象列表、静态设计图或模拟 IPC 不能作为多 Agent 协作已完成的证据。

## 新规范的增量验收要求

以下是后续实现的验收要求，本次文档统一没有运行这些测试，也不新增通过结论。

- 区分被测入口：Bevy 原生画布、React/Tauri 应用、Wry/React 编辑层分别记录产物、版本、环境及范围，不能互借平台结果。
- 检查素净界面层次、正文与控件清晰度、状态可辨识性、缩远后的模块和连线，以及键盘、焦点、窄窗口和深浅主题。小猫局部柔光与颗粒不能影响任务内容或命中区域；猫保持一体团子、双耳双眼，无嘴、鼻、尾和脚。
- Agent 接入后按真实流程验证：创建任务、拆解 TODO、查看输入与产物、连接模块、并行推进、携带明确上下文交接，以及暂停、调整、失败重试和人工审核。每项标注真实执行或模拟范围；未实现项写明未实现，不记为通过。
- 保存同场景截图和实际操作结果；新的设计规范、视觉概念图、自动截图成功或历史测试数均不能替代新版本的交互验收。

## 2026-10-11：Ravomi 产品重命名

产品显示名称从 H&M 统一为 Ravomi：Bevy 首页、关于页、菜单、窗口标题、剪贴板提示，以及 React / Tauri 标题和 Linux / macOS 打包显示名称同步更新。复用现有主题、字号和布局；README 使用用户提供的 `assets/branding/ravomi-logo.png`，与附件逐字节一致（1774 × 887）。原双鸦和 Graplu 图标保留历史来源，系统小尺寸图标尚未替换。远端仓库地址、应用 ID、内部 crate / 二进制名称、用户数据目录及 `.prg` 格式保持兼容。

Linux 上前端主应用和画布输入组件生产构建通过，17 项 Playwright 测试通过。检查 Chromium 浏览器中深浅主题各 100% / 150% / 200% 截图，1200 像素宽及 390 像素窄窗口无横向溢出；新品牌文字可见，长中文文件名沿用截断，窄窗口隐藏副标题。代表证据：[深色 100%](validation/ravomi-brand/react-dark-100.png)、[浅色窄窗口 200%](validation/ravomi-brand/react-light-narrow-200.png)。这些截图使用模拟 IPC，不代表原生 Tauri 验收；上轮截图见 `validation/react-tauri/`，内容与窗口条件不同，不作逐像素比较。

新编译的 Bevy release 程序在 Linux / Intel Iris Xe / Mesa 26.2.3 / Vulkan 下以 `--preview --home --native-scale` 检查深浅主题 100% / 150% / 200%，Ravomi 品牌及副标题可读；[关于页](validation/ravomi-brand/bevy-about.png)文字与项目链接名称一致。代表首页：[深色 100%](validation/ravomi-brand/bevy-dark-1.png)、[浅色 200%](validation/ravomi-brand/bevy-light-2.png)。200% 时既有首页底部导航超出面板，不能将本次名称检查视为完整布局验收；本次未改变导航布局。预览退出前偶有截图未落盘，缺失项单独重跑。

常规 release workspace 测试 253 项、Clippy 全 targets（拒绝 warning）、fmt 均通过；25 项 Linux 缩略图测试、打包脚本语法和 desktop 文件校验通过。前端和 Rust 检查在保留其他会话未提交改动的工作区执行，本次只提交重命名相关文件或代码块。未重新执行 macOS DMG 构建、Windows 运行或全量 ignored GPU / 字体测试。

## 2026-10-10：React / Tauri 迁移入口

新增对象编辑原型，复用 Rust 文档、撤销和 `.prg` 保存。生产前端构建、Tauri 调试打包（嵌入生产前端、不生成安装包）、250 项 workspace 测试、Clippy 与 9 项前端测试通过。真实 Linux WebKit 窗口中新建、中文编辑、Rust 保存及重开临时文档通过；浏览器预览不冒充可编辑应用。环境、截图、复现命令与未验证范围见 [迁移入口验收](validation/react-tauri-bootstrap.md)。

## 2026-10-10：H&M 品牌与顶部入口清理

当前界面使用 H&M / Huginn & Muninn 文字标识；首页、关于页、窗口标题、文件筛选与剪贴板提示已统一，项目链接指向 Wang-Yang-source/H-M。左上角 Dock 已移除，文件、编辑、视图、设置与窗口功能从右上角“⋯”进入。启动器不再下载或自动加载上游教程。上游来源与许可保留，crate、文档格式、旧系统 ID 和用户数据目录保持兼容。

使用本次源码生成的 release 程序，在 Linux / 原生 Wayland / Intel Iris Xe / Mesa 26.2.3 / Vulkan 上生成并检查深浅主题、100% / 150% / 200% 的启动页截图。代表证据：[深色启动页](validation/hm-home-dark.png)、[深色 200% 文件菜单](validation/hm-menu-dark-200.png)、[浅色 200% 启动页](validation/hm-home-light-200.png)、[关于页](validation/hm-about-dark.png)。截图确认左上角不再有 Dock、标签与右侧操作不重叠；浅色截图中菜单已收起，不作为浅色展开菜单的验收证据。历史截图内容不同，不作逐像素前后比较。

常规 workspace 测试 248 项、Linux 缩略图测试 25 项通过；CJK 标签裁剪 ignored 测试单线程通过。fmt、release Clippy 全 targets、脚本语法、desktop 文件校验通过。窄窗口回归覆盖右侧文件菜单与分类菜单，以及无左侧 Dock 时的标签可用区域。未在本轮重新执行 macOS 打包、Windows 运行、所有交互状态或全量 GPU ignored 测试；多平台统一打包仍是后续方案，尚未引入 Tauri 或 cargo-packager。

## 2026-10-10：桌面画布半透明

读取本机 Ghostty 当前配置 `background-opacity = 0.82`，窗口开启预乘 alpha 合成，背景颜色单独使用 82% 不透明度；节点、文字、菜单和导出背景仍用原主题色。本机原生 Wayland / niri / Intel Iris Xe 的合成器截图确认桌面壁纸透过画布，菜单与文字保持实色。透明 PNG 导出 GPU 回归通过，200 项常规测试、fmt、Clippy 全 targets 通过。

## 2026-10-10：分组参与外部推挤

完整分组使用一个碰撞体，位置回写及渲染插值应用到全部递归成员；单独拖动成员时拆开祖先碰撞体，避免与自己的分组边框互相推挤。锁定节点及锁定组后代不能启动拖动，包含锁定成员的完整分组作为固定障碍物。

200 项常规测试通过，18 项物理测试（含环境测试）通过，fmt、Clippy 全 targets 通过。覆盖文本推组、组推文本、嵌套成员和连接点保持队形、锁定阻挡、组内调整、取消及撤销重做。桌面 3.1 / 3.2 在内存中解锁后验证整图归组与移动，原文件不写入；本地快捷键教程另存临时副本再打开，节点、连线、成员及附件一致。3.2 原生 Wayland / Intel Iris Xe 运行完整平移、缩放、物理拖动轨迹正常退出；该轨迹用于运行验证，不作为性能对比结论。

## 2026-10-10：切割删除后越界闪退

回归测试经真实右键切割输入删除最后一个节点，旧版本在 `fit_groups` 访问 `nodes[0]` 时复现 `len is 0` panic。输入阶段改为仅刷新文档分组边界，图形尺寸刷新留在 `sync_nodes` 清理旧实体之后，避免混用新文档与旧图形编号。

196 项常规测试及 fmt、Clippy 全 targets 通过。用桌面 `tutorial-main-3.1.prg` 在隔离 X11 窗口执行右键拖切：636 节点 / 391 连线变为 631 / 384；Ctrl+Z 恢复 636 / 391，Ctrl+Shift+Z 重做至 631 / 384，全程窗口存活。未保存测试修改至原文件。

## 2026-10-10：修复系统 175% 缩放下的概览错字

用户复测指出上轮仍有严重错字。此前预览强制 1×，未覆盖普通窗口的系统 1.75×。本轮在本机 Intel Iris Xe / Mesa 26.2.3 / Vulkan / 原生 Wayland、2240×1400 物理分辨率、无缩放 override 下复现：3.1 文件在 20 秒交互轨迹中记录 563 次 `epaint texture atlas overflowed!`；[修复前截图](validation/render-native-175-before.png) 可见标题部分笔画被其他字形覆盖。

原因是固定 100 逻辑字号仍会乘系统 DPI 与 UI 缩放，高 DPI 下同帧标题就能占满 egui 图集。现在固定 64 物理像素生成缓存字形，并缩放几何保持标题的画面尺寸。未提高图集上限。新增环境回归同时排版教程全部分组标题，旧实现 150% 时占用达到 0.9707，修正后 100%、150%、175%、200%、350% 均低于重置阈值，后续缩放帧不再重建图集；桌面 3.1 / 3.2 各运行一次通过。

相同 175% 本机窗口轨迹在 3.1 深色、3.2 浅色下均无溢出日志，缩放阶段截图中的标题完整：[深色](validation/render-native-175-after.png)、[浅色](validation/render-native-175-light.png)。195 项常规测试及 fmt、Clippy 全 targets 通过，20 项环境测试默认跳过。复现入口为 `FILE.prg --benchmark --native-scale --screenshot PATH --output REPORT`，截图在连续缩放阶段抓取；测试只改内存，不保存用户文件。这轮验证针对字形覆盖，不据此声称所有字号都具有相同锐度或全部界面问题已经解决。

## 2026-10-10：首页卡片高度减半

卡片高度从 `clamp(width × 1.25, 260, 420)` 改为 `clamp(width × 0.625, 130, 210)`；四列规则不变，压缩摘要区和预览留白，让最小高度下仍能显示缩略图、文件名和状态。195 项常规测试、fmt 和 Clippy 通过。Xvfb / llvmpipe 的深浅主题、100% / 150% / 200% 截图见 `validation/home-half-{dark,light}-{100,150,200}.png`。100% 时六张最近文件卡片可完整展示；200% 窄窗口仍有此前记录的顶部标签重叠和首页侧栏溢出问题。

## 2026-10-10：文字图集与相机同步

同一字体重新加载时按字体内容复用 MSDF 字形键，标签切换沿用已有字体资源；不同字体和字面索引保持区分。使用桌面 `tutorial-main-3.1.prg` 的全部节点文字连续加载字体八次，生成字形数和图集页数保持首次加载后的数值。64 MiB 限额未提高，本次修复重复缓存增长，不代表任意多种不同字形均不会达到上限。

分组概览标题固定字号排版后缩放几何，连续缩放 120 帧不再追加字号对应的字体纹理。标签切换、导航及输入先更新相机，再绘制概览；同一帧的 egui 重复布局不会重复应用画布输入。195 项常规测试通过，19 项环境测试默认跳过；字体重新加载环境测试单独通过，fmt、Clippy 全 targets 和 diff 检查通过。

桌面 3.1 文件为 636 节点、391 连线、24 附件；3.2 为 614 节点、375 连线、24 附件。隔离 Xvfb / llvmpipe 窗口检查 3.1 的缩放、平移、新建标签及返回原标签；本机 Intel / Vulkan / Wayland 检查 3.2，见 [本机截图](validation/render-native-after.png) 和 [平移缩放后截图](validation/render-zoom-pan-after.png)。本轮未观察到报告中的大块破碎字形，但静态截图不能覆盖所有连续交互帧。3.1 的 1280 像素宽 PNG 导出成功。

深浅主题和 100% / 150% / 200% 界面缩放截图保存在 `validation/render-{dark,light}-{100,150,200}.png`。概览标题均显示；全图缩至 4–5% 时细小正文仍有采样锯齿，不把这种极小字号的清晰度计为已解决。

## 2026-10-10：首页四列卡片

最近文件网格的最大列数由三列改为四列，保留 240 的列宽阈值和 16 间距，窄内容区仍自动降列。192 项常规测试、fmt、Clippy 通过。Xvfb 1280×800 / llvmpipe 检查 `validation/home-four-{dark,light}-{100,150,200}.png`：100% 为四列，150% 两列，200% 一列；文件名截断及悬停路径提示可见。200% 下现有顶部标签与首页标题重叠，未在本次列数调整中解决。

## 2026-10-10：搜索展开隐藏节点

搜索定位在移动相机和选择节点前展开其折叠祖先，只改动目标所在的祖先链；多层展开形成一次撤销。已可见目标不生成历史，锁定祖先、无效节点及未完成事务返回可见错误，保持原来的相机与选择。

新增两项回归覆盖嵌套展开、无关分组保持折叠、节点坐标/成员/连线不变、重复定位不改版本、一次撤销/重做，以及锁定/非法索引/编辑中拒绝的原子性。192 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。正式折叠菜单及物理推挤仍在集成，尚未替换安装版。

## 2026-10-10：新建节点排除隐藏分组

提取共享的分组代表索引计算，新建文字或导入图片选择归属时排除折叠分组和隐藏子分组。折叠内层分组时新内容归入仍展开的外层；外层整体折叠时新内容放到根层，不会创建后立即隐藏。

新增回归覆盖两种嵌套折叠状态、新节点可见、成员归属、保存重开和撤销/重做。190 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。正式折叠功能仍在集成，安装版尚未替换。

## 2026-10-10：折叠分组跳跃归组

Alt 跳跃按可见卡片命中起点、目的地并绘制预览，隐藏成员不成为放置目标。混合选择折叠分组与普通节点时，以可见选区中心计算落点，再换算为原始文档整体位移；成员仍通过文档事务一起移动，保持身份、归属与连线。

新增回归覆盖隐藏成员位置、折叠卡片目标、混合选区预览尺寸、所有成员的相同位移、连线保持以及撤销/重做。189 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。折叠仍限预览，正式菜单、搜索定位和物理推挤等集成尚未完成。

## 2026-10-10：标签切换与折叠输入缓存

切换文档时清空折叠显示缓存及连线路径，保留折叠显示是否启用。文档版本变化后的显示投影在鼠标输入之前刷新，避免首次点击沿用上一文档或撤销前的几何；文字测量与布局后的渲染同步继续更新卡片尺寸。

标签切换回归加入旧显示缓存与旧路径，验证切换后全部清除；新增输入前准备回归覆盖折叠、撤销展开、重做折叠，已有 ECS 回归改为运行实际准备/同步顺序。188 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。折叠正式入口仍未启用，安装版尚未替换。

## 2026-10-10：折叠卡片切割

切割起点占用检查、候选节点、接触特效、预警描边和删除碎片使用可见卡片边界。隐藏成员不独立参与命中；实际删除仍通过原始文档的删除计划展开整组，并保留锁定保护。空白双击的占用检查复用同一显示边界。

新增折叠分组回归验证隐藏成员原位置不占用、不被切中，卡片可以切中，整组删除后外部节点保留，一次撤销恢复全部成员和连线、重做再次删除。187 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。正式折叠入口与其他交互仍在集成，尚未替换安装版。

## 2026-10-10：折叠卡片的基础命中与连线预览

点选、右键选择、框选、键盘全选与选择外框使用显示几何并排除隐藏成员；折叠卡片按实心节点参与命中，展开分组保留标题和边框命中规则。连线开始、结束、端口吸附及预览共享显示卡片边界。ECS 回归覆盖卡片中心、隐藏成员原位置、外部节点、小范围框选和右侧端口。

186 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。切割、跳跃归组等其他操作仍需接入，折叠仍仅在内部预览入口启用，尚未替换桌面安装版。

## 2026-10-10：折叠分组渲染预览

新增 `--preview-tool collapsed` 内部验证入口，空白启动生成三个节点、一个分组和一条外部关系，或折叠输入文档中的首个非空分组。显示缓存使用文字测量尺寸生成紧凑卡片，通过根实体可见性隐藏成员；连线使用投影端点和卡片边界。全部原始节点和连线仍保留在文档中。此入口不会自动保存文档。

修正分组边界整理重新写入展开外框、而折叠缓存未重算时外框残留的问题：缓存几何计算，持续应用显示尺寸。ECS 回归模拟原始布局覆盖，验证卡片尺寸、成员显隐、未修改的原始边界、空闲不置脏，以及展开后的恢复。186 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。

Xvfb 1280×800、llvmpipe/Vulkan 下检查深色 [100%](validation/collapsed-dark-100.png)、[150%](validation/collapsed-dark-150.png)、[200%](validation/collapsed-dark-200.png) 及浅色 [100%](validation/collapsed-light-100.png)、[150%](validation/collapsed-light-150.png)、[200%](validation/collapsed-light-200.png)：内部节点隐藏，标题在卡片内，外部关系连到卡片边缘。现有默认连线标签使用边框色，对比度仍需完善。

折叠渲染目前仅在该预览入口启用。正式画布的命中、切割、跳跃归组、选择外框、搜索定位和菜单入口尚需统一使用显示投影；当前不能视为可交付的折叠交互，B04 保持未完成。

## 2026-10-09：折叠分组的显示投影

新增只读 `Document::section_view`，保留文档索引，计算每个成员对应的最外层可见折叠分组、折叠卡片边界及显示连线。卡片尺寸由调用方提供文字测量结果，在原分组中心定位；内部连线隐藏，外部端点投射到折叠分组中心，多目标连线合并落到同一折叠分组的端点，未受折叠影响的端口保持原值。成员身份、坐标、原始连线和文档版本均不修改。重复 ID、缺失成员、多父分组、循环引用、无效尺寸和失效端点返回错误。

三项回归覆盖嵌套折叠/逐层展开、外部关系与原有自环、多目标端点合并、原始数据不变及非法层级/尺寸拒绝。185 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。当前仅完成共享计算接口，渲染、鼠标命中及操作菜单尚未接入，B04 仍未完成。

## 2026-10-09：分组折叠状态纳入节点模型

参考原版 `Section.tsx` 的持久化 `isCollapsed`，将折叠状态从打开文件时的独立 ID 集合迁入 `GraphNode`。现有节点历史、图形复制、全图/选区快照因此保留折叠状态；新建分组默认展开，新增及已有分组均写回兼容字段。分组概览和删除成员规则读取实时节点状态，快照也保留原有概览排除元数据。

新增 `set_sections_collapsed` 文档操作，批量校验分组类型、索引及锁定关系，再生成独立撤销记录；已有编辑未结束时拒绝，不吞并当前事务。相同状态不产生新历史。三项回归覆盖保存重开、撤销重做与保存点、成员坐标及边关系保持、快照和跨文档复制、复制后删除成员、混合非法选择与锁定拒绝。

182 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。此提交仅完成数据与历史基础，折叠外观、成员显隐、边端点投影、命中排除和操作菜单尚未实现，不能视为 B04 完成。

## 2026-10-09：导出取消回收与选区桌面验收

导出任务在复制临时图片和检查目标文件后、最终发布前再次检查取消标志，取消不创建或替换目标，临时文件随作用域清理。统一渲染子进程等待与停止流程，取消/超时先停止并等待回收；处理进程在检查与终止之间已经退出的情况。新增真实 `/bin/sleep` 子进程回归验证取消与超时退出及回收，`/bin/true`、`/bin/false` 验证退出状态和已退出进程的取消；发布回归验证取消后新目标不存在、原目标字节不变且无临时文件残留。

179 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。子进程测试在 Unix 下运行，不等同于 Windows 验收。

在 Xvfb / llvmpipe / 独立 D-Bus GTK portal 下，从教程画布选中“快捷键的分类与说明”单节点，通过文件菜单 → 导出 PNG → 所选内容 → 透明背景 → 原生保存，生成 [2048×1271 透明图片](validation/export-selection-transparent.png)。目视确认仅有该节点、没有相邻连线或工具栏，角像素为 `(0,0,0,0)`；[成功反馈](validation/export-selection-success.png) 保留已选 1、已保存状态。源 PRG 的 SHA-256 仍为 `e61ea94d505cab5eeca7c5b236b8afaf049d9d34211342f71a5f2934bcd4c49d`。原生 Wayland 仍未验收。

## 2026-10-09：PNG 导出设置与后台任务

文件菜单“导出 PNG…”接入全图/选区、图片宽度与透明背景设置，复用 `tool_frame`、`tool_style`、`tool_header` 和 `primary_button`。保存时捕获独立快照与当前字体、主题，后台启动隐藏渲染进程；关闭设置窗口可继续编辑，状态栏可重新查看进度和结果。支持取消、150 秒任务超时与可见错误。PNG 先生成到临时目录，再同步写入目标目录中的临时文件并发布；已存在目标需原生对话框确认，导出期间变化则拒绝替换，新目标不覆盖落盘。

176 项 workspace 常规测试通过、18 项环境测试跳过，fmt / Clippy 全 targets 通过。新增发布回归覆盖新文件、确认替换、目标并发变化与拒绝时保留原内容。

Xvfb 1280×800、llvmpipe/Vulkan 与隔离 D-Bus/GTK portal 下，实际点击设置主按钮、原生保存对话框，导出 393 节点/230 连线教程为 [2048×1148 PNG](validation/export-live-tutorial.png)，已检查没有工具栏。[成功状态](validation/export-live-success.png) 可通过滚动查看完整路径；再次选择已有目标出现 [覆盖确认](validation/export-overwrite-confirm.png)，取消后显示“已取消导出”，目标 SHA-256 保持 `7a1960a980d27341407db9d152961ab0a2fa620f1c1541654cb9118273676707`。源教程副本 SHA-256 仍为 `e61ea94d505cab5eeca7c5b236b8afaf049d9d34211342f71a5f2934bcd4c49d`。

已目视检查深色 [100%](validation/export-dark-100.png)、[150%](validation/export-dark-150.png)、[200%](validation/export-dark-200.png)，浅色 [100%](validation/export-light-100.png)、[150%](validation/export-light-150.png)、[200%](validation/export-light-200.png) 以及 [640×500 窄窗口](validation/export-narrow.png)。选区为空时对应选项禁用，长说明换行，高度不足可滚动。选区数据往返由上一节测试覆盖，选区界面端到端、后台取消时子进程回收、原生 Wayland 保存仍待进一步验收。

## 2026-10-09：导出选区快照

新增 `Document::capture_selection_snapshot`，复用图形复制规则，包含所选节点、递归分组成员、所选连线端点，以及这些节点间的内部连线。保留节点坐标与 ID，重新映射连线索引，仅携带选区所需附件；空选区、失效索引和未结束编辑返回错误。快照用于后续后台导出，不修改源文档、选择或撤销历史；文件菜单导出入口仍未接入。

新增两项回归，验证嵌套分组与图片附件的 PRG 保存重开、排除外部节点与连线、单独选择连线、非法选区拒绝，以及源文件字节、保存点和撤销重做保持。175 项 workspace 常规测试通过，18 项环境测试跳过，fmt / Clippy 全 targets 通过。

## 2026-10-09：PNG 实际绘制边界

自动导出区域改为汇总可见 Sprite、Mesh2d 和 Text2d 的局部包围盒，经 GlobalTransform 转为世界边界后增加 32 世界单位边距，覆盖越出节点矩形的曲线、箭头和文字。边界变化时同步纹理尺寸并重新等待布局稳定；显式 `--export-rect` 保持用户指定区域。仍需接入选区过滤与产品导出入口。

173 项 workspace 常规测试通过，18 项环境测试跳过，fmt / Clippy 全 targets 通过。新增纯逻辑回归验证带缩放和平移的包围盒与世界 Y 轴转换；另在 Xvfb / llvmpipe 实际执行 `cargo test --release --locked --test png_export -- --ignored --test-threads=1`，构造两个节点与偏移 600 的大圆弧，导出 600×617 透明 PNG。测试检查红色曲线像素数量、输出扩展高度、四边全部透明；[输出图](validation/png-export-large-arc.png) 已目视检查标签与曲线完整。可用 `GRAPLU_EXPORT_ARTIFACT` 指定该测试的 PNG 留存路径。

## 2026-10-09：离屏 PNG 渲染基础

新增内部验证入口 `FILE.prg --render-png OUTPUT.png --export-width 1600`，支持 `--light`、`--transparent` 和明确世界区域 `--export-rect x,y,width,height`。在隐藏窗口应用中复用现有 Bevy 节点、连线、MSDF 与彩色字体渲染，摄像机输出独立纹理并经 Screenshot 图像读回，不包含 egui 工具栏。等待 MSDF 就绪与渲染稳定后写出，120 秒超时退出。PNG 使用临时文件同步后不覆盖落盘，失败返回非零进程状态。

这仍是导出底层，文件菜单导出保持禁用。默认边界目前仅节点包围盒加 32 世界单位边距，尚未包含越界曲线和所有标签；选区过滤、用户设置与原生保存入口待后续实现，不算 B07 完成。

Xvfb / llvmpipe 下，393 节点、230 连线的教程副本生成 1600×897 [深色 PNG](validation/png-export-dark.png)、[浅色 PNG](validation/png-export-light.png)、[透明 PNG](validation/png-export-transparent.png)。透明空白像素为 `(0,0,0,0)`，半透明分组样本经线性空间去预乘后为 `(49,50,68,115)`，避免 PNG 再次合成时变暗。额外验证 777×389 的非对齐行宽读回；已有目标导出退出码 1。UI 缩放 1/2 输出尺寸和构图一致，但逐像素比较仍有约 0.01% 差异，尚未证明像素级确定性。

172 项 workspace 常规测试通过、17 项环境测试跳过，fmt / Clippy 全 targets 通过。新增回归检查宽高比、非有限值、零尺寸、分配上限及线性空间去预乘。实现参考 [Bevy Screenshot](https://docs.rs/bevy/latest/bevy/render/view/window/screenshot/struct.Screenshot.html) 与 [ScreenshotCaptured](https://docs.rs/bevy/latest/bevy/render/view/window/screenshot/struct.ScreenshotCaptured.html)。

## 2026-10-09：恢复副本清理与隔离桌面点击验证

恢复列表增加逐份删除，点击后在同一记录内显示确认和取消。确认后后台删除并刷新；处理中禁用记录操作，错误可见。删除函数只接受恢复根目录下的“会话 UUID / 数字标签 ID / 代次 UUID”三层目录，拒绝符号链接及含其他文件的目录，不读取元数据中的原路径作为删除目标。重复删除已不存在的代次可以完成。跨会话累计副本现可手动清理；不自动删除旧会话的最后一份草稿。

170 项 workspace 常规测试通过，17 项环境测试跳过，fmt / Clippy 全 targets 通过。新增回归覆盖其他会话保留、根目录/父目录/越界路径拒绝、额外文件保护、重复删除及 Unix 符号链接拒绝。

Xvfb 隔离桌面上实际执行删除按钮→取消→再次删除→确认：取消后 PRG 存在，确认后代次目录消失、列表变为空；用于创建副本的测试源 SHA-256 始终为 `e61ea94d505cab5eeca7c5b236b8afaf049d9d34211342f71a5f2934bcd4c49d`。工件：[确认](validation/recovery-delete-confirm.png)、[删除后空列表](validation/recovery-delete-empty.png)。随后重新放入测试副本，通过刷新与恢复按钮打开出 393 节点 / 230 连线的新草稿，标签及状态栏显示未保存：[实际恢复结果](validation/recovery-live-draft.png)。未操作用户恢复目录；此验证不包含原生 Wayland 的另存对话框。

已刷新并检查六组 [深色 100%](validation/recovery-dark-100.png)、[150%](validation/recovery-dark-150.png)、[200%](validation/recovery-dark-200.png) 和 [浅色 100%](validation/recovery-light-100.png)、[150%](validation/recovery-light-150.png)、[200%](validation/recovery-light-200.png) 截图，使用 Xvfb 1280×800 与 llvmpipe/Vulkan。

## 2026-10-09：持久化备份设置

恢复窗口中的“备份设置”可启用/关闭自动恢复副本、设置 15–3600 秒间隔和每个会话标签 1–100 份保留数量。设置写入恢复根目录 `settings.json`，临时文件同步后替换；保存成功后才更新内存策略。重新启动读取设置，损坏或越界配置显示原因并暂停自动备份，允许用户重新保存有效设置。关闭不删除副本，已开始任务完成，数量策略于下次成功写入时应用。

新增配置回归验证缺省值、持久化往返、非法配置拒绝且旧文件保持、两代保留限制；调度回归验证禁用不启动任务、重新启用重新计时、无效设置不影响当前策略。168 项 workspace 常规测试通过，17 项环境测试跳过；fmt / Clippy 全 targets 通过。跨会话累计存储管理未完成。

下列截图使用 Xvfb 1280×800、llvmpipe/Vulkan；设置页复用现有主题、弹窗、主按钮和占位滚动条。高缩放下超出内容可滚动访问。

| UI 缩放 | 深色 | 浅色 |
| --- | --- | --- |
| 100% | [截图](validation/backup-settings-dark-100.png) | [截图](validation/backup-settings-light-100.png) |
| 150% | [截图](validation/backup-settings-dark-150.png) | [截图](validation/backup-settings-light-150.png) |
| 200% | [截图](validation/backup-settings-dark-200.png) | [截图](validation/backup-settings-light-200.png) |

## 2026-10-09：恢复列表与新草稿

文件菜单“恢复副本…”后台读取各会话的有效副本，按时间排序，展示来源、相对时间和恢复按钮。损坏元数据或缺少 PRG 的代次列出具体错误，不隐藏其他有效项。加载 PRG 使用已有后台打开任务，成功后新增未命名、未保存的草稿；保存走另存对话框。恢复不会绑定原文件或备份路径，不改变备份文件。新增回归覆盖混合有效/损坏目录、恢复后的保存点、编辑撤销后仍未保存以及另存后备份字节不变。

166 项 workspace 常规测试通过、17 项环境测试跳过；fmt / Clippy 全 targets 通过。六组 Xvfb 1280×800、llvmpipe/Vulkan 截图采用隔离恢复目录中的同一条测试记录，已检查恢复按钮不被裁切。最初 Intel Vulkan 在 Xvfb 缺少 DRI3 导致呈现失败，切换 llvmpipe 后重跑；未据此宣称真实桌面点击恢复和原生另存对话框全流程已验收。

| UI 缩放 | 深色 | 浅色 |
| --- | --- | --- |
| 100% | [截图](validation/recovery-dark-100.png) | [截图](validation/recovery-light-100.png) |
| 150% | [截图](validation/recovery-dark-150.png) | [截图](validation/recovery-light-150.png) |
| 200% | [截图](validation/recovery-dark-200.png) | [截图](validation/recovery-light-200.png) |

## 2026-10-09：后台恢复副本调度

每 60 秒检查所有打开的标签页，仅捕获已提交且有新修改的文档修订。快照捕获不复制撤销历史；压缩、往返校验和落盘由后台线程完成，完成结果按标签 ID 与捕获修订号记录，不把后续编辑误记为已备份。后台失败进入现有错误提示，下个周期可以重试。文字编辑及未结束事务暂缓备份，不强制提交用户输入。

恢复目录按进程会话、标签 ID、随机代次分隔；每代含 `document.prg` 与来源路径、时间、修订元数据。两者完成后才发布该目录，之后保留当前代与最近九份旧代；未完成临时目录不参与恢复列表。不修改原文件和文档保存点。目前设置与恢复界面尚未接入，也尚未限制跨会话累计存储；不宣称完整自动保存/恢复流程已验收。

新增后台调度测试验证活动和非活动标签都写出、任务启动后的新增节点不混入旧快照、完成后文档仍为未保存。存储测试验证副本可重开、快照独立、临时目录被忽略及十代保留上限。

workspace 165 项常规测试通过、17 项环境测试跳过，fmt 与 Clippy 全 targets 通过。显式引入已锁定的 serde 1.0.229，锁文件仅增加根包依赖声明，没有升级传递依赖。

## 2026-10-09：恢复副本写入基础

`Document::save_snapshot` 复用正式保存的 PRG 序列化、附件保留、往返校验与临时文件落盘，使用不覆盖目标的提交方式写入独立恢复代次。它不改变文档路径、历史保存点和原文件冲突检测基准；未结束的编辑事务拒绝写出。此处仅完成存储基础，尚未接入定时备份、保留策略或恢复界面。

新增两项回归覆盖：未命名草稿恢复副本、保存重开、已有目标拒绝覆盖、失败后未保存状态、撤销/重做保存点、外部改写原文件后恢复副本仍可写出而正式保存拒绝覆盖。workspace 163 项通过、17 项环境测试跳过，fmt 与 Clippy 全 targets 通过。

## 2026-10-09：系统剪贴板图片

文件菜单新增“粘贴剪贴板图片”，与文件导入共享后台任务、图片大小限制、文档事务及纹理上传。沿用图对象 Ctrl+V 和文字输入的剪贴板规则，避免已有图对象快照与系统图片之间的优先级歧义。后台只读取系统剪贴板，不替换其内容。空剪贴板、非图片内容或读取错误通过现有模态提示反馈。

复用依赖树已有的 arboard 3.6.1，显式启用 `wayland-data-control`；依赖锁未新增包或升级版本。读取的是系统提供的 RGBA 位图，将其无损编码为 PNG 附件并标记来源 `clipboard.png`，不宣称恢复源应用的原始压缩文件或文件名。

常规 workspace 161 项测试通过、17 项环境测试跳过，fmt / Clippy 全 targets 通过。隔离 Xvfb `:98` 上另行执行 `system_clipboard_image_is_imported_and_survives_save_reopen`：独立 clipboard owner 写入半透明图片，后台线程读取，检查精确 RGBA、撤销/重做、保存重开后的 PNG 像素，再写入纯文本验证可见错误内容。该测试要求 `GRAPLU_ISOLATED_CLIPBOARD=1` 且移除 `WAYLAND_DISPLAY`，未修改用户主桌面的剪贴板；不据此声称原生 Wayland 或所有源应用的剪贴板格式均已验收。

六档菜单截图使用同一四节点场景、Xvfb 1280×800、llvmpipe / Vulkan、系统缩放 1。200% 下通过滚动访问后续菜单项。

| UI 缩放 | 深色 | 浅色 |
| --- | --- | --- |
| 100% | [截图](validation/clipboard-menu-dark-100.png) | [截图](validation/clipboard-menu-light-100.png) |
| 150% | [截图](validation/clipboard-menu-dark-150.png) | [截图](validation/clipboard-menu-light-150.png) |
| 200% | [截图](validation/clipboard-menu-dark-200.png) | [截图](validation/clipboard-menu-light-200.png) |

## 2026-10-09：本地图片导入与粘贴纹理

文件菜单“导入图片…”使用原生文件对话框，后台读取和解码 PNG/JPEG/SVG，取消不修改文档。导入期间阻止画布输入、保存和标签切换；图片居中放入当前视野，保持比例，最长边最多 512 逻辑屏幕单位且不放大原始尺寸。沿用新节点的分组归属和锁定检查，一次导入对应一次历史记录。原始字节保留；附件使用原版要求的 `UUID.扩展名`，原文件名写入 `grapluSourceName`，不暴露完整本机路径。该扩展字段由 Graplu 保留，未承诺原版应用再次保存时会保留它。

修复跨文档粘贴图片时只复制附件、没有上传目标画布纹理的问题。回归检查图片上传的真实 RGBA 数据、Sprite 引用、撤销/重做后的纹理复用，以及文档归组、锁定拒绝、原字节/中文文件名保存重开、重复导入不覆盖。常规 workspace 160 项通过，16 项环境测试跳过，fmt / Clippy 全 targets 通过。原生对话框的选择/取消与完整桌面保存操作仍需人工闭环；系统剪贴板位图尚未接入，不能把图对象粘贴当作系统图片粘贴。

菜单截图使用同一四节点场景，Xvfb 1280×800、系统缩放 1、llvmpipe / Vulkan。深浅主题各检查 100%、150%、200% UI 缩放；200% 内容超高，菜单显示滚动条，不把下方入口未出现在首屏描述成全部可见。

| UI 缩放 | 深色 | 浅色 |
| --- | --- | --- |
| 100% | [截图](validation/import-menu-dark-100.png) | [截图](validation/import-menu-light-100.png) |
| 150% | [截图](validation/import-menu-dark-150.png) | [截图](validation/import-menu-light-150.png) |
| 200% | [截图](validation/import-menu-dark-200.png) | [截图](validation/import-menu-light-200.png) |

## 2026-10-09：损坏附件的可见错误与占位

移除附件解码路径中“已验证图片/SVG”的 `expect` 假设：文件加载实际上只确认附件存在，没有验证其图像内容。图片解码失败现在汇总为现有模态错误，标明附件名和原因；对应节点显示“图片无法显示”，其他节点正常加载，原始文档附件不被删除或替换。占位文字由显示层生成，文档标题保留；文档修订同步时也不会将占位抹掉。复用主题文字和现有错误窗口，没有新增调色板或确认流程。

输出图像在分配像素前检查单边不超过 8192、总计不超过 16777216 像素；超限会提示缩小后重试。SVG 扩展名识别不区分大小写，半透明 SVG 继续转换为直通 alpha。回归覆盖损坏 PNG/SVG、不支持的数据、超宽 PNG、超大 SVG、正常 PNG 像素、半透明 SVG 颜色，以及占位经过两次文档同步仍存在且不改写文档文字。常规 workspace 测试 157 项通过、16 项环境测试跳过；fmt 与 Clippy 全 targets 检查通过。该回归未替代真实桌面错误窗口的截图验收。

## 2026-10-09：SVG 附件的缺失字体回退

教程中的 SVG 使用 serif、Verdana 等字体名。本机 fontdb 的默认通用字体映射指向未安装字体，usvg 找不到起始字体时会整段省略文字。附件解析现在先加载画布配置的字体，再加载系统字体；仅在通用字体无法解析时将其映射到已加载字体，并设置 SVG 默认字体。存在的字体映射保留，附件字节及文档内容不变。同一文档的 SVG 共用一份字体库，避免逐附件扫描系统字体。

ignored 栅格测试分别绘制不存在的字体名、serif、sans-serif、monospace、cursive、fantasy 六行中文和英文，逐行检查实际覆盖像素。[结果截图](validation/svg-font-fallback.png) 使用透明底白字，每行 24 像素；这是字体回退验证，不代表与原版字体或完整 UI 逐像素相同。系统字体：Noto Sans CJK，resvg/usvg 0.48.1。常规 workspace 测试 154 项通过，16 项环境测试跳过；该 SVG 环境测试另行通过，fmt 和 Clippy 全 targets 检查通过。

上一轮的彩色 emoji 修复及 GPU 截图见 [字体说明](assets/fonts/README.md) 和 [彩色渲染截图](validation/msdf-color-emoji.png)。这些修复解决已确认的缺字原因，不将下面历史记录中的未完成 UI、输入法、恢复或导出验收自动标为完成。

## 2026-10-09：紧凑浮层、节点右键和分组概览

搜索、属性、右键与退出确认复用 `theme::tool_frame/tool_style`；首页关闭按钮不再使用突兀矩形边框。标签轨道移除矩形滚动渐隐，新建和关闭固定在滚动列表之外；窄窗口空间不足时标签另起一行。退出确认的“保存”使用主题主色按钮。

节点菜单采用图标操作、色条与文字操作。色条最左项写入透明色 `[0,0,0,0]`，恢复主题填充。测试通过实际 egui 按下/释放点击该色块，检查所选 ID、透明色和撤销恢复；对齐与自动布局仍明确禁用。内联编辑居中及输入法空通知两项回归先在旧实现失败，再修复通过；真正的组词/提交帧仍屏蔽结束快捷键，Shift+Enter 保留换行。未据此声称所有桌面输入法均已人工验收。

概览依据 master `1fb1b5cc` 的 0.45 相机阈值、15% 视野比例、0.5 覆盖透明度、90% 拟合与 80% 最大字号，结合 Godot 工作区版本的 1.12 退出防抖、编辑恢复与标题避让。自动测试覆盖阈值两侧、字号拟合、编辑子节点时恢复所属分组、覆盖分组选取不修改文档，以及导入的虚线/无边框/折叠状态。覆盖层使用 egui 绘制，普通画布文字仍用 MSDF；未移植 Godot 的截图缓存与聚合连线系统，也没有对这项改动宣称性能收益。

`cargo test --workspace --release --locked`、`cargo clippy --workspace --all-targets --release --locked -- -D warnings`、`cargo fmt --all --check` 均通过。代表性截图如下，覆盖深浅主题及 100%/150%/200% UI 缩放，但不是所有控件、主题、缩放与状态的完整笛卡尔积验收。

| 截图 | 环境与范围 |
| --- | --- |
| [搜索深色](validation/tools-search-dark.png)、[搜索浅色](validation/tools-search-light.png) | 原生 Wayland、Intel Iris Xe / Mesa 26.2.3 / Vulkan，100% |
| [属性深色](validation/tools-properties-dark.png)、[属性浅色](validation/tools-properties-light.png) | 同上，150% |
| [右键深色](validation/tools-context-dark.png)、[右键浅色](validation/tools-context-light.png) | 隔离 Xvfb 1280×800、Mesa llvmpipe / Vulkan，150% / 200%；浅色图验证窄窗口标签另起一行 |
| [退出深色](validation/tools-exit-dark.png)、[退出浅色](validation/tools-exit-light.png) | 同一隔离环境，150% / 200% |
| [首页浅色](validation/tools-home-light.png) | 原生 Wayland，150%，设置菜单打开；检查无常驻边框的关闭按钮 |
| [概览深色](validation/overview-dark.png)、[概览浅色](validation/overview-light.png) | 隔离环境，快捷键教程，100% / 150% |

隔离截图使用软件 Vulkan，不能作为性能基准。主教程 `tutorial-main-legacy-benchmark.prg` 的预览遇到 `MSDF text could not be prepared completely`（1 个失败标签），因此没有把主教程截图记为通过；快捷键教程完成了概览截图。旧主教程的字形兼容性仍需单独处理。

## 2026-10-09：关闭确认与错误提示的模态隔离

关闭确认及错误提示改为 egui `Modal`，带遮罩并阻止背景控件接收点击；错误未处理时阻止文档切换，关闭/错误/文件对话框期间不响应后台 Ctrl+S。错误提示存在时暂不绘制关闭提示，避免同帧出现两个模态层而导致层级不确定。

常规 workspace release 测试 141 项通过、14 项环境测试跳过，Clippy 全 targets `-D warnings` 与格式检查通过。原生 Wayland 下无实际修改的编辑会话关闭后正常退出。XWayland 实际窗口已观察到 [保存确认及遮罩](validation/close-modal-dark.png)，点击“取消”后回到画布并保留选区。该图为 1419×1772，X11 scale override 1、应用 UI 100%，Intel Iris Xe / Mesa 26.2.3 / Vulkan。未用这一截图推断真实文件保存或多个脏标签逐个放弃已完成桌面验收。

## 2026-10-09：新节点放射光伪影与初始尺寸修正

截图反馈对应三层最大 100 世界单位的宽描边模糊，宽度超过小节点自身尺寸，造成轮廓内外交叠。创建效果改为单层连续圆角描边，1.5 屏幕像素、0.2 秒淡出；移除跨分组射线。新节点原硬编码字号 32、高 76，现按示例画布 24 号字基线设为高 57（36 行高与上下各 10.5 留白）。空编辑草稿保留初始宽度，避免创建效果与输入轮廓宽度不一致。

`cargo test --release --workspace --locked`：141 项通过、14 项环境测试跳过；相关 ignored GPU 测试另行通过，fmt 与 Clippy 全 targets `-D warnings` 通过。几何回归覆盖世界单位/屏幕像素比例 0.1、1、10，确认创建描边不会形成远离节点的尖刺；GPU 回归确认轮廓外及内部中心无额外光斑。实际 GPU 为 Intel Iris Xe / Mesa 26.2.3 / Vulkan。

[深色强调色](validation/creation-outline-dark.png) 与 [浅色强调色](validation/creation-outline-light.png) 是 512×128 离屏截图，冻结动画首帧，黑底用于检查透明区域。测试对象仍使用 60×76 的旧小节点尺寸以回归原始伪影；不将其视为新建节点高度或完整桌面布局截图。完整桌面新增交互及主题/缩放矩阵仍沿用下方待验收项。

## 2026-10-09：基础编辑闭环与共享主题验收进度

基础入口已接入：节点创建即编辑；节点和边删除；边标题编辑与反转；批量颜色、字号与宽度；当前图搜索；上下文菜单；跨标签页剪贴板；各标签独立选区、相机与历史；关闭标签和退出时逐个处理未保存文档。结构操作撤销/重做按稳定 ID 恢复选区。功能分别提交在 `feat/core-editing`，不包含旧文件兼容工作。

自动化检查：`cargo test --release --workspace --locked -- --include-ignored --test-threads=1` 共 **154 项通过**，包括 GPU、CJK 字体与教程文件；随后共享窗口主题改动的常规测试 **140 项通过、14 项环境测试跳过**。`cargo fmt --all --check`、Clippy 全 targets `-D warnings` 通过；Linux 缩略图 **25 项通过**。这批检查覆盖保存重开、剪贴板独立历史、边编辑/删除、选区恢复、关闭取消与多文档退出状态；不替代系统输入法验收。

截图环境：Intel Iris Xe，Mesa 26.2.3，Vulkan；Bevy 0.19.1、egui 0.36.2。六张画布截图使用同一 `--synthetic 4` 内容，原生 Wayland，实际图片 **2240×1400**；`--ui-scale` 为应用 UI 缩放，不能当作系统 scale factor。画布自动适配会随 UI 占位调整，因此这些图用于控件布局检查，不用于比较文字性能。

| UI 缩放 | 深色 | 浅色 |
| --- | --- | --- |
| 100% | [截图](validation/core-dark-100.png) | [截图](validation/core-light-100.png) |
| 150% | [截图](validation/core-dark-150.png) | [截图](validation/core-light-150.png) |
| 200% | [截图](validation/core-dark-200.png) | [截图](validation/core-light-200.png) |

已目视检查这些图的 Dock、标签、文字和文件菜单。浅色 200% 的最终截图菜单已收起，不能作为该档菜单展开证据。除浅色 200% 外，其余五张早于共享窗口控件颜色修正；最终颜色修正后还需补齐一致状态的矩阵。

桌面 Ctrl+A → Alt+Enter 已打开四节点属性。对比 [修正前](validation/core-properties-dark-150.png) 与 [修正后](validation/core-properties-themed-dark-150.png)，标题栏、输入控件与说明文字改为共享主题配色。两张经 XWayland 捕获，分别为 **2850×1772、系统缩放 1.75** 与 **1419×1772、显式系统缩放 1**，应用 UI 均为 150%；窗口尺寸不同，仅证明控件配色变化，不是等尺寸布局对照。Ctrl+F 实际打开搜索窗口。

颜色按 sRGB 相对亮度计算：深色正文/面板 8.69:1、次要文字/面板 5.65:1，浅色正文/面板 6.57:1；浅色次要文字原 4.06:1，调整为 `#626579` 后 4.72:1，强调色上的浅色文字 4.79:1。此计算针对语义实色，不代表所有透明度及禁用状态均达到该值。

仍待完成：最后颜色修正后的完整截图矩阵、窄窗口与长中文文件名、相关焦点/悬停/中断状态及关闭提示桌面流程。真实 fcitx5/Rime 预编辑未取得可靠证据，不能声称 IME 通过；已恢复测试前 `keyboard-us` 输入法。继续保留此项待验收。

## 2026-10-09：跨标签页图对象剪贴板

Ctrl+C / Ctrl+X / Ctrl+V 使用应用级独立快照，复制分组时递归收集成员并去重，仅保留内部连线；粘贴生成新 ID、重映射成员与附件，按鼠标对应世界位置居中，重复粘贴偏移 24 世界单位。源文档关闭后仍可粘贴，剪切与目标粘贴分别撤销。锁定选区拒绝剪切并给出可见错误；文字编辑捕获输入时不运行图对象快捷键。帮助窗口补充快捷键说明，沿用共享主题。

新增三个文档测试及一个 Bevy 输入测试，覆盖源关闭、分组、内外连线、附件、多目标边样式、独立历史、锁定剪切、保存重开和 150% DPI / 4 倍画布尺度的粘贴定位。`cargo test --release --workspace --locked`：132 项通过、14 项环境测试跳过；`cargo clippy --release --workspace --all-targets --locked -- -D warnings` 通过。实际桌面文字剪贴板、IME 与最终主题/缩放截图仍待整体验收。

## 2026-10-09：整图分组后拖动遗留空分组修复

截图对应的可复现原因在选择范围：Ctrl+A、工具栏全选和框选排除 `Section`。随后 Ctrl+G 将所选叶子节点从原组抽出，旧分组框留在新组外；拖动新组时旧框及其端点不随行。master `StageManager.selectAll` 包含全部舞台对象。现全选包含旧分组，完整包住分组的框选也包含其框；组内或部分覆盖框选仍只选相交普通节点，避免意外移动整个父组。

新增教程端到端回归：Ctrl+A→Ctrl+G 保留全部旧组 children 和边；393 个原节点加新组整体平移，保存重开、连续撤销移动与分组。实际 Avian headless 回归默认开启推挤，每帧重算分组边界，覆盖重叠节点、嵌套组和 ConnectPoint；双向拖动、松手停稳、一次撤销/重做均保持相对位置，教程保存重开核对全部节点、边、附件及 counts。物理系统本身不需修改。

完整 release workspace `--include-ignored --test-threads=1` **101 项通过**（5 字形库、27 文档、69 应用），Clippy 全 targets `-D warnings`、fmt 与 diff 检查通过。日志 `/tmp/graplu-group-final-tests.log`、`/tmp/graplu-group-final-clippy.log`。测试返回值的 must-use 警告已在源码修正并经后续 Clippy 确认。另以独立回归移除双击编辑时强制居中和缩放，保持原位编辑的当前视角。

本地独立提交 `yko` 分组选择修复、`kkm` 编辑视角修复。已有错误分组可先撤销到分组前，再重新全选和分组；修复不会根据屏幕位置猜测并重写已经丢失的旧成员关系。未推送。

最终 release 二进制 SHA-256 `51ab854a6c342eb031c981afd0b52ba3d9e0e00e1e960a892c6bd9c6c1629ff3`。原生 Wayland/Vulkan 教程窗口已启动，PID 3311167 / niri 747，日志 `/tmp/graplu-final-run.log` 未见初始化错误，加载 393 节点、230 连线、14 附件。使用 `--example`，保存需另选路径；未关闭用户已有窗口。

## 2026-10-09：原位编辑与 MSDF 截图修正

和独立文字渲染会话协作完成。编辑框改为透明原位输入，沿用节点字号、内边距和自动/手动换行；Enter 完成、Shift+Enter 换行，Escape/点击外部/滚轮保留内容退出，组合输入与系统失焦不触发退出。草稿边框随内容变化，绿色轮廓与节点共享 Godot 连续圆角路径，修复普通圆角叠在连续圆角上的错位。

原位快捷键、字形库、文档及应用完整 release workspace 测试 **96 项通过**（含 ignored GPU、教程、字体回归）；Clippy 全 targets `-D warnings`、格式与 diff 空白检查通过。日志 `/tmp/graplu-text-final-tests.log`、`/tmp/graplu-text-final-clippy.log`。此结果对应分组全选范围修复之前的版本。

[Z 节点原位编辑截图](validation/inline-edit-z.png) 使用教程 `--edit-node 350`，原生 Wayland/Vulkan、Intel Iris Xe/Mesa 26.2.3，已目视确认透明输入和绿色连续轮廓贴合；该图不是分组拖动或真实 IME 的验收证据。MSDF 修复距离符号和对角插值，消除已复现的外部碎片及明显笔画缺口；4 倍子像素轮廓测试排除距轮廓 0.6 texel 的边缘带，尖角仍存在亚 texel 近似，不声称逐像素一致。详见 vendored `UPSTREAM.md`。

## 2026-10-09：master 切割与交互反馈

参考 `Arcraven/project-graph` 本地 master `1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0` 的 `ControllerCutting`、`utilsControl`、`inputElement` 与具体效果类。旧仓库仅只读，新代码均在 Graplu。

- 空白右键从固定起点拉出直线，松手使用最新端点和当前曲线缓存执行一次删除事务；节点上右键仍创建连线。Lyon 已提供曲线展开和线段相交，沿用现有锁定版本，没有新增几何库。
- 删除处理关联边、受影响的孤立连接点、分组成员提升及索引重排。UUID 保证文字、物理和连线仍对应原对象；旧文字测量不会写到删除后的后继节点。PRG 保存重建引用并保留未知字段、二进制/扩展类型和附件。
- 反馈复用 Bevy `ColorMaterial`、Lyon 与标准库随机种子，统一瞬态 mesh、512 效果上限。编辑绿框 0.25 秒、移动粒子 0.5 秒、切割碎片约 0.83 秒；通过真实时间更新避免效果寿命随刷新率变化。先推进旧效果，再处理本帧交互，避免从休眠唤醒时吞掉新效果；全部结束后恢复按需刷新。
- 明确差异：未移植声音；刀光阴影用多层透明轮廓近似 Canvas 模糊；指针离开窗口取消未完成切割。未实现折叠操作界面及全部 master 效果，不将本次结果称为完整编辑器复刻。

手动验收：在教程空白处按右键跨过节点和曲线，检查红色预览后松手删除；Ctrl+Z 应一次还原，另存副本再打开保留其余节点、分组与附件。节点上右拖仍建立连线。拖动普通节点观察后沿白色粒子，停稳后应消失；进入文字编辑以及 Enter 完成时出现短暂绿色扩散框。真实中文输入法组合输入、不同显示器缩放及触控板仍需交互验收；自动化输入不代替这些验证。

## 2026-10-09：默认推挤、Alt 跳跃与 Godot 连续节点形状

参考 `/home/waya/Projects/Software/project-graph` 的 `src/entity_layer_mover.gd`、`src/main/continuous_corners.gd` 和 `src/stage_object/entity/text_node/text_node.gd`。本轮无 Godot MCP，用户明确允许例外只读源码；未修改或运行 Godot 项目。

- 交互默认开启 Avian 推挤；加载/切换开关保持布局、物理时钟暂停和历史干净。保留 `--no-physics` 与基准显式 `--physics`，并修复文档成员增删后的碰撞体索引同步。
- Alt 手势复用文档历史与 Lyon 弧线：落点预览、松开提交、递归成员整体移动、已有 Section 归组/移出；锁定、自包含、非有限坐标拒绝。普通节点尚不自动转换容器。拖动切入 Alt 保持一次撤销，取消、失焦、空格与撤销/分组快捷键结束手势。
- 普通节点和分组按 Godot 当前源码的两段三次曲线绘制，每个角半径为 `floor(min(24,max(0,min(width,height)/2-2)))`；共享 resvg 纹理和 Bevy 九宫格，尺寸变化切换缓存。填充、边框、选择共用轮廓，图片选择保持矩形。边框为 1 世界单位，纹理固定 4 倍精度；尚未移植 Godot 的层级描边和动态 DPI 档位。

完整 workspace release 测试（`--include-ignored --test-threads=1`）**71 项通过**：4 字形库、21 文档、46 应用；覆盖 GPU 文字与教程文件，新增推挤空闲/增删、跳跃/历史/保存/非法目标、圆角栅格和缓存回归。Clippy 全 targets `-D warnings` 通过。日志 `/tmp/graplu-node-interaction-tests.log` 与 `/tmp/graplu-node-interaction-clippy.log`。没有新增依赖；复用锁文件已有 Avian、Bevy、Lyon、resvg、undo。

Release 产物 SHA-256 `e6d2d4849ba54f088b3909abcc2d379b6b565d202fa676a8a0c823ebbbd5d0fc`。`env -u DISPLAY WGPU_BACKEND=vulkan` 原生 Wayland/Vulkan 启动通过，Intel Iris Xe / Mesa 26.2.3。六节点合成图深浅主题截图为 [深色](validation/node-shapes-godot.png)、[浅色](validation/node-shapes-godot-light.png)，已目视检查约 269% 画布缩放下连续四角、完整边框与默认推挤状态；未据截图推断性能。教程交互窗口 PID 3215630 / niri 737 已打开（`--example`，保存需另选位置），旧窗口未关闭。真实 Alt 输入、推挤手感及其他倍率由下列步骤继续人工验收。

实现独立本地提交：`c09778a` 默认推挤、`99a6e1b` 连续形状、`b147f06` Alt 跳跃，分支 `feat/node-interactions`，没有推送。

手动验收：启动教程图，确认“推挤：开”，打开后节点不自动移动；拖动邻近节点检查推开与撤销。选中节点后 Alt+点击空白或分组内部，检查弧线颜色、整体位移、分组成员和一次 Ctrl+Z；按住 Alt 时切换空格/撤销/失焦，松鼠标不应意外落点。放大短节点、多行节点与分组，编辑文字改变尺寸，检查四角/直边接缝、描边完整及选择贴合；保存副本后重新打开确认成员结构。

## 2026-10-09：同时捏合平移与滚轮事件分类修复

用户确认需要同一次双指手势同时缩放和平移，并报告新版鼠标滚轮抖动。读取当前聚焦窗口 PID 3128694 的可执行文件 SHA-256，确认它确为上一节的 `0968f645…` 产物；没有将问题归因于旧窗口。窗口存在未保存修改，保留原进程。

- Wayland 输入回调原本忽略 `axis_source`，任何没有 discrete 步数的事件都变成 PixelDelta。画布把 Pixel 当作双指平移，所以明确标识为 Wheel 的部分刻度也可能夹杂平移。修复后 Wheel/WheelTilt 仅发离散 LineDelta，等待完整步数；Finger/Continuous 保持 Pixel，不猜测 absolute 到刻度的换算。四项底层测试覆盖片段与整格交错、DPI、双轴、方向、停止和未知来源。源码缺陷已验证，但没有采集用户硬件当时的原始事件，不能据此断言解释了所有抖动。
- Wayland pinch Update 同时转发缩放和中心位移。画布改用 Bevy 的 RawWinitWindowEvent 扩展接口按顺序处理，保留窗口与 phase；开始时建立中心，Pan 更新中心和相机，Pinch 围绕该中心缩放。多帧平移→缩放、缩放→平移、平移后不抬手继续捏合均保持原世界锚点跟随累计手指位移。UI 捕获、失焦、取消后清除状态，残留 Moved 不再重新激活旧手势。

完整 workspace release 测试（include-ignored）**57 项通过**，含 GPU 字体、教程分组保存及 8 项画布导航回归；vendored winit 的 pointer 测试 **8 项通过**。所有 targets 的 Clippy 拒绝 warning、rustfmt、diff 空白检查通过。日志：`/tmp/graplu-combined-tests.log`、`/tmp/graplu-wheel-backend-tests.log`、`/tmp/graplu-combined-clippy.log`。滚轮分类与组合手势各自独立提交，均未推送。

新产物 SHA-256：`5ebe8b658c6f3db38f44a5aa10da8e3569eee781019e9d86650b7b1d255c761a`；原生 Wayland/Vulkan/Iris Xe 启动通过，无运行时错误，交互窗口 PID 3152713（niri window 733），日志 `/tmp/graplu-combined-interactive.log`。旧窗口 PID 3128694 保留未保存内容。

实际触控板手感和硬件滚轮复测仍由用户验收。当前滚轮保留整格缩放；高精度 fractional wheel 需要窗口库提供 axis_value120，不用经验比例替代协议。下一节关于高层 PinchGesture 丢失 phase 的限制已由本轮原始事件接口解决；取消仍选择保留当前视图。

## 2026-10-09：Graplu 分组与触控板交互

本轮在迁移后的 Graplu 根目录完成，分组与手势分别保留为本地原子提交，没有推送本轮功能。

- Ctrl+G / 工具栏创建 Section，成员去重、嵌套父组关系、整体移动、撤销/重做与 PRG 保存重开通过回归。分组边界复用 master 的成员外包矩形、30 单位边距和非空标题 50 单位留白；成员移动或文字尺寸变化时刷新边框。新增和撤销分组同步创建/销毁画布实体。
- 用户教程的“上/下/左/右对齐”四节点分组回归通过，验证全部节点和连线按 UUID 一致，保存与撤销输出的非 stage ZIP 内容逐字节保留。原版 Section.children 的内嵌对象提升到舞台数组时同步重定位路径引用。
- 双指 Pixel 滚动转换为逻辑像素后平移，Line 鼠标滚轮继续缩放，Ctrl+Pixel 作为替代缩放入口。逐条处理避免混合单位；真实 Camera 缓存矩阵回归验证 DPI 1/1.5/1.75/2 下的平移距离与缩放锚点，覆盖 UI 捕获、失焦、无光标、其他窗口滚轮与非法数值。
- Bevy 0.19.1 的 PinchGesture 复用现有消息桥。固定 winit 0.30.13 的本地补丁回移上游 Wayland pointer-gestures，未升级引擎或引入新运行时依赖。来源与限制见 [winit 补丁说明](vendor/winit/UPSTREAM-PATCH.md)。

`cargo test -j6 --release --locked --workspace -- --include-ignored --test-threads=1` **54 项通过**（4 字体库、18 文档/导入、32 画布），包括 GPU 字体测试。vendored winit 的 `pointer_gesture` **3 项单测通过**（独立 manifest、其自带锁文件、release），覆盖增量、重置、非法值及生命周期事件。workspace 全 targets Clippy 拒绝 warning、rustfmt 和 diff 空白检查通过。测试日志分别为 `/tmp/graplu-interactions-tests.log`、`/tmp/bevy-winit-pinch-tests.log` 和 `/tmp/graplu-interactions-clippy.log`。

匹配源码的运行产物 SHA-256：`0968f645416f98c6d43c02ab69322bda39687c9bb13e336c300f3186da0d24be`。Fedora/niri、原生 Wayland、Vulkan / Iris Xe / Mesa 26.2.3，使用原版 393 节点、230 连线教程，未出现运行时或 shader 错误。`WAYLAND_DEBUG=client` 启动预览确认：合成器公布 `zwp_pointer_gestures_v1` v3，应用以 v1 绑定，并对当前 `wl_pointer` 调用 `get_pinch_gesture`；这证明后端已接通，不等同于真实手指输入验收。见 [教程与跟随成员的分组边框](validation/groups-gestures-wayland.png)。

尚需用户真实触控板验收方向、速度、捏合连续性与跨屏 DPI。XWayland 不提供本补丁的原生捏合，使用 Ctrl+双指滑动或鼠标滚轮；macOS 复用上游 PinchGesture，但本轮未在 macOS 运行。当前为指数缩放响应，Bevy 消息不含 phase，因此取消时保留当前视图，不回滚之前的缩放。本轮没有重新测量帧耗时，不宣称新的性能提升。

## 2026-10-09：截图反馈修复

按用户实际截图分别修复四个问题，各自独立提交：

- 分组外框不可见（`39fcfbd9`）：分组位于 z=-5，而相机 z=1000、far=1000 将它裁掉；相机恢复 2D 默认深度中心，回归覆盖分组、连线和节点的裁剪范围。
- 滚轮缩放锚点来回偏移（`7770d2a8`）：输入系统显式先于 `CameraUpdateSystems`，同帧提交匹配的缩放矩阵与相机位置。纯 CPU 测试调用真实 Bevy `camera_system` 和 `viewport_to_world_2d`，在 DPI 1/1.5/1.75/2 下连续注入 Line/Pixel 滚轮及空帧，误差小于 0.005 世界单位。负向测试强制旧顺序，复现大于 100 世界单位的单帧偏移及下一帧回正。
- Option 符号缺字（`f97b8e2c`）：启用系统字体发现，并复用 Fontique 的公开 fallback API 追加已安装符号字体。测试验证中文与 `⌥⌘` 混排选到有效字形和不同字体 face，16 倍变换不重新生成字形/网格。缺字不再伪装就绪，修正文案后可恢复。
- 文字软换行及外框溢出（`48c02b8e`）：保留原版 auto/manual 模式，自动节点只按原换行符分行，手动节点按宽度分行；实际 TextLayoutInfo 驱动外框尺寸。教程 345 个文字节点通过高度包含检查，自动节点通过原换行数量和宽度包含检查。派生测量不标脏或增加历史，拖动撤销保留最新排版尺寸，保存保留节点模式。

`cargo test -j6 --release --locked --workspace -- --include-ignored --test-threads=1` **43 项通过**（4 个字体库、13 个文档/导入、26 个画布测试）；workspace Clippy 全 targets 拒绝 warning、rustfmt、diff 空白检查通过。首次混排 GPU 回归因新字体改变行基线，旧采样点落在“中”的横笔上而失败；检查实际图像后调整空洞采样并增加另一侧空洞断言，保留笔画、边缘及覆盖率检查后通过。见 [中文与 Option 的 1×/4×/16× 图](validation/msdf-symbols.png)。

运行产物 SHA-256：`249788d19388de599b9ac51835f4d61e90d1619deaeb951da843dfd8a8929144`，Rust 1.98.1，Bevy 0.19.1，Fedora/niri，原生 Wayland + Vulkan / Iris Xe / Mesa 26.2.3。匹配源码的 release 应用已运行预览，未出现 shader 或 schedule 错误。见 [完整教程与分组](validation/layout-groups-wayland.png) 和 [键盘说明区域](validation/layout-keyboard-wayland.png)：分组可见，文字留在外框内，Option 字符可见。

本轮重点是正确性；字体测量后的节点尺寸与分组可见性都发生变化，且用户的旧测试窗口仍运行，因此不重报性能加速比。真实鼠标手感、跨屏拖动与中文 IME 仍需用户验收；自动相机回归不替代硬件交互测试。

## 2026-10-09：master 连线与创建交互

参考 `master` 的 `1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0`，接入 Lyon 1.0.20 路径细分、Mesh2d 曲线缓存和原版端口/箭头规则。匹配本轮源码的 release 二进制 SHA-256 为 `de2ebd0a38212f708f3c8a5c5894b69d00f91aaa3e1e0fa7044782fee46f2776`。没有更换 MSDF 文字实现或隐藏内容。

- `cargo test -j6 --release --locked --workspace -- --include-ignored --test-threads=1`：34 项全部通过，包含中文 GPU、原版教程、原版 ZIP 保留、连线增量保存、撤销重做和实际输入系统。
- 输入回归覆盖右键拖拽只创建一次、短点击不创建、UI 捕获释放与窗口失焦取消。几何测试覆盖六种箭头设置、三种线型、并行边、自环、多目标 line/convex/circle、非中心 rectRates 及有限坐标。
- 缓存回归覆盖整体平移不重建网格，单端移动只重建相关边，撤销新边移除对应实体。解析回归覆盖端口与 centerRate 的 `$` 引用。
- Workspace Clippy（所有 targets，拒绝 warning）、rustfmt 和 `git diff --check` 通过。

本机环境沿用下节，接通电源，窗口和基准串行运行；原生 Wayland + Vulkan 教程为 393 节点、230 连线、2240×1400，UI 开启、推挤关闭。加载到文字准备完成 5347 ms；缩放均值 / p95 为 **9.97 / 12.39 ms**，多节点移动为 **10.06 / 12.28 ms**。见 [原始报告](validation/edges-tutorial-wayland.json) 和 [GPU 画布截图](validation/edges-tutorial-wayland.png)。截图确认曲线、中文标签与箭头可显示，未出现 shader validation 错误；不是所有线型的逐像素原版对照。

教程平移/缩放阶段曲线及文字网格重建均为 0。拖动阶段只移动非分组节点，26 条连接到静止分组等端点的边每帧需要重建，397 帧累计 10322 次；这是端点相对位置变化，并非整图重建。其余边复用 mesh，仅更新 Transform，文字网格仍为 0 次重建。

5000 节点、4929 连线，XWayland + Vulkan、1279×799，UI 开启、推挤关闭，两次独立进程：缩放均值 **27.75～37.12 ms**、p95 **34.34～46.79 ms**；整组拖动均值 **24.50～24.79 ms**、p95 **28.76～28.98 ms**。加载到文字就绪为 1057～1977 ms。所有稳态阶段曲线网格、文字布局、MSDF 网格重建和补字次数均为 **0**。见 [第一次](validation/edges-5000-x11.json) 与 [第二次](validation/edges-5000-x11-repeat.json)。两次缩放波动较大，不能从这些样本宣称恢复曲线后整体提速；5000 节点仍未达到稳定 60 FPS。

Linux 文件预览的独立移植由 MSDF 会话协作完成，25 项 Python 测试与 Ruff 检查通过，具体格式、安装和桌面验收范围见 [缩略图说明](packaging/linux/README.md)。尚未向系统安装或验证真实文件管理器调用。原版所有全局连线设置、Arc/CatmullRom、连线属性编辑/删除及鼠标手工验收仍未完成。

## 2026-10-09：与 MSDF 会话联合验证

MSDF agent 负责 Kayak 字形生成、shader、文字组件及编辑衔接；本会话负责加载调度、就绪门禁和应用测量。合并为原子提交 `a2984653f841a7073834552e7a8ef981eafe7fa1`。匹配该源码的 release 应用 SHA-256 为 `dfd1ba26d653c2f36920837e410f6719cde739c985e630cb9a873768f8493728`。保留全部文字，不启用缩小隐藏或 LOD。

27 项 release 测试通过，包括全部 ignored 测试、中文 GPU 渲染、编辑后的缓存更新、失败回退和基准超时拒绝；workspace Clippy（所有 targets、拒绝 warning）及 rustfmt 检查通过。首次 GPU 测试虽然通过像素数量断言，截图仍发现反色与尖刺；修正 CFF 距离符号及 Kayak 三次曲线移植错误、加强像素断言后重新通过。[1× / 4× / 16× 截图](validation/msdf-smoke.png) 已由双方检查；它只证明所测字形与倍率，不能保证所有字体和极端倍率。

测量环境沿用下节 Linux 环境，本轮接通电源、电池满电，窗口结束时均未聚焦。构建与各次窗口测试串行运行。先等待全部 MSDF 标签/字形就绪，再预热 4 秒，然后四个阶段各测 4 秒。`load_ms` 从场景建立计到文字就绪，不含进程完整启动，也不等于首帧 GPU 呈现时间。失败回退、120 秒未就绪或采样中重新补字会输出 `valid:false` 并退出失败；未完成文字不能作为有效性能样本。补字期间按 16 ms 间隔刷新，完成后普通编辑窗口恢复按需刷新。

### 教程与 5000 节点

教程为原版 393 节点、230 连线，Wayland + Vulkan，2240×1400：501 个字形、3 页图集、400 个标签网格重建，首次文字准备 **5154 ms**，失败标签为 0。图集纹理 CPU、GPU 各 **12 MiB**，不含其他内存。缩放均值 / p95 为 **9.26 / 12.51 ms**，批量拖动 **8.25 / 10.86 ms**。相比下节 Text2d 的单次教程结果略慢，不能声称教程提速。见 [原始报告](validation/msdf-tutorial-wayland.json) 和 [完整画布截图](validation/msdf-tutorial-wayland.png)。

5000 节点、4929 连线，XWayland + Vulkan，1279×799，UI 开启、推挤关闭。两次独立进程均有效，文字准备 **933～991 ms**，18 个共享字形、1 页图集、5000 个标签网格重建。合成短文本的字形种类远少于教程，因此加载更快，不能将其加载速度外推到大量不同中文。

| 阶段 | Text2d 均值 / p95 ms（此前两次） | MSDF 均值 / p95 ms（本轮两次） |
|---|---:|---:|
| 缩放 | 30.57～34.43 / 51.62～54.04 | 32.47～33.23 / 41.51～42.01 |
| 全部节点移动 | 25.99～27.22 / 36.52～44.24 | 25.64～27.08 / 29.52～32.11 |

见 [本轮第一次](validation/msdf-5000-x11.json)、[本轮第二次](validation/msdf-5000-x11-repeat.json)。本机这两次的尾部帧间隔较低，均值范围重叠；样本有限且非隔离环境，不能归纳为普遍提速，仍未达到稳定 60 FPS。所有稳态阶段文字排版、MSDF 字形生成及网格重建均为 **0**；零重建不等于零渲染准备或 GPU 上传开销。

额外一次 [5000 节点诊断](validation/msdf-5000-profile.json) 中，主应用 First→Last 墙钟跨度均值为静止 11.28、平移 11.06、缩放 11.08、拖动 16.57 ms，GPU 2D 通道均值约 3.10～3.93 ms。此跨度不是纯 CPU 时间，通道计时也不是全部渲染过程；不能据此直接断言 draw call 是唯一瓶颈。每标签独立 mesh 的渲染准备与合批仍是后续定位对象，尚未实现跨标签实例合批或持久磁盘字形缓存。

[编辑预览](validation/msdf-editor-wayland.png) 已检查：egui 编辑框与周边放大的 MSDF 中文同时可见，编辑标签的浏览文字由编辑框接替。真实中文 IME、跨屏 DPI、完整原版功能迁移仍未完成验收。彩色/位图/SVG 字形可能回退 Text2d，相应基准会判无效；本轮未执行含 Emoji 的完整压力套件。

## 2026-10-09：完整显示下的 Linux 优化

按项目 `bevy-wayland-vulkan` 技能定位；保留全部文字、九宫格圆角、边框和连线，无缩小隐藏或细节降级。该轮 MSDF 接入前代码为 `51a2cfbe1c0d4c9861941245008005fb94eac2d3`，release 产物 SHA-256：`728cd58f5d37fde3dc74b5af5f2308fcc6e6b0948b8c4703308325c34b7e1224`。下方旧版 Canvas2D 对照属于历史原型，不能直接用作当前编辑器的提速倍数。

环境：Fedora 44 / niri，i9-13900H，Iris Xe RPL-P，Mesa 26.2.3；桌面 2880×1800、120.001 Hz、175% 缩放。Bevy 0.19.1 / winit 0.30.13 / wgpu 29.0.4 / bevy_egui 0.42.0。其他桌面应用仍在运行；电源状态未采集。测量串行执行且不同时编译。请求 AutoNoVsync，实际呈现模式未知，报告为 null。基准窗口在结束时均未聚焦，测试强制连续更新。

已完成：

- 16 项 release 测试通过；Clippy `--all-targets -- -D warnings`、rustfmt、基准脚本 Bash 语法检查通过。
- Avian 刚体由 Position/速度单向驱动，关闭每个固定步长不必要的全画布变换传播、Transform 反向位置同步和缩放同步。刚体独立于显示层；Collider 尺寸由文档显式更新。
- 推挤关闭时不创建刚体，关闭后释放，再开启按当前文档重建。生命周期、尺寸/位置重建、锁定障碍、队形、取消、撤销及远处休眠节点测试通过。
- 拖动时只写实际改变的 Transform。新增测试在旧实现中得到 6 个变化对象而失败；修复后仅移动节点和相关连线两个对象变化，空更新不标记变化。
- 普通窗口遵循系统 DPI；平移使用逻辑光标坐标。输入集成测试覆盖 175% DPI，缩放锚点覆盖 100% / 150% / 175% / 200%。[原生窗口截图](validation/linux-native-dpi.png) 确认完整图与中文界面可见。真实中文 IME、跨显示器拖动及候选框位置尚未人工验收。

### 定位用计时

5000 节点、4929 连线，XWayland + Vulkan，1279×799，UI 开启、推挤关闭，预热 4 秒后各阶段测 4 秒。增加 `--diagnostics` 复用 Bevy RenderDiagnosticsPlugin；它有额外开销，下表只用于定位。First→Last 是主应用调度的墙钟跨度，包含调度等待，不是纯 CPU 时间，也不包含后续提取及呈现。

| 阶段 | 优化前主调度均值 ms | 优化后主调度均值 ms |
|---|---:|---:|
| 静止连续刷新 | 23.39 | 8.35 |
| 平移 | 21.05 | 8.01 |
| 缩放 | 22.10 | 9.30 |
| 全部节点移动 | 29.01 | 13.70 |

这是各一次诊断对照；原始样本数、p95/p99、GPU 通道及环境在 [优化前](validation/linux-profile-before.json) / [优化后](validation/linux-profile-after.json)。GPU 2D 通道通常只需约 2～3 ms，无法解释全部整帧延迟。临时移除整个物理插件的定位开关已删除，正式产物保留推挤功能。所有稳态阶段文字重排计数为 0；这不等于没有 GPU 上传。

### 不启用诊断的结果

原版教程：393 节点、230 连线，原生 Wayland + Vulkan，实际 **2240×1400**，完整显示、UI 开启、推挤关闭。缩放均值 **7.77 ms**、p95 **9.62 ms**；多节点移动均值 **7.49 ms**、p95 **9.70 ms**。这是一次运行，见 [完整数据](validation/linux-tutorial-wayland.json)。该尺寸不同于 XWayland 的 1279×799，因此不作窗口后端优劣比较。

5000 节点的 XWayland 同尺寸结果：

| 阶段 | 优化前均值 / p95 ms（一次） | 优化后均值 / p95 ms（两次范围） |
|---|---:|---:|
| 缩放 | 37.14 / 89.23 | 30.57～34.43 / 51.62～54.04 |
| 全部节点移动 | 33.32 / 40.13 | 25.99～27.22 / 36.52～44.24 |

见 [优化前](validation/linux-5000-before.json)、[优化后](validation/linux-5000-after.json)、[重复运行](validation/linux-5000-after-repeat.json)。大图缩放长帧仍存在；拖动 p95 与旧值重叠，不能声称所有长帧都改善，也尚未达到稳定 60 FPS。原版全部编辑功能仍按根目录 TODO 推进，不能把本轮性能修改当成功能迁移完成。

## 历史：初始原型基线

2026-10-09，Fedora 44 x86_64 / niri，Intel Core i9-13900H（20 线程），Intel Iris Xe RPL-P 集成显卡，Mesa 26.2.3，Bevy 使用 Vulkan。Rust 1.98.1、Node 24.21.0、Chrome 154.0.8037.97。桌面仍有其他应用运行，这不是隔离的实验室测试；测量期间没有编译本原型。

测试文件为 graphif/assets 提交 `d9d945e549c9cd8a593ba3079d983e805d9c837c` 的 `tutorials/tutorial-shortcut-keys-3.1.prg`，SHA-256 为 `e61ea94d505cab5eeca7c5b236b8afaf049d9d34211342f71a5f2934bcd4c49d`。完整解析 393 节点、230 连线、14 附件；拖动阶段同时移动 366 个非分组节点。

## 正确性

- `cargo test --release --locked -- --include-ignored`：6 项通过，包括原版教程完整性、嵌套对象去重/引用解析、分组边界、缺失引用报错、缩放锚点和实际 Bevy 输入系统的多选拖动/相机平移。
- `cargo clippy --locked --all-targets -- -D warnings`、`cargo fmt --check` 通过。
- `node --check benchmark/canvas.mjs`、`bash -n run.sh` 通过。
- 优化二进制在本机 GPU 上运行完成全部阶段，截图确认节点、中文、分组和连线可显示。见 [全图截图](validation/bevy.png)。全图缩小时文字很小，当前原型尚未做原版逐像素排版还原。

## 测量方法

先预热 4 秒，再按静止、平移、连续缩放、多节点拖动顺序各测 4 秒。两种渲染器复用相同模型、世界范围和运动轨迹；各自保留文字换行结果。Bevy 的测速模式采用连续更新，避免默认的后台 60 Hz 限制；请求关闭垂直同步，测量的是墙上时钟帧间隔，仍包含系统调度、驱动和桌面合成影响。

本机桌面缩放为 1.75，直接启动会改变物理分辨率。测试采用 XWayland 和 `WINIT_X11_SCALE_FACTOR=1`；窗口管理器将请求的 1280×800 调整为 **1279×799**，已在 Bevy 报告和 PNG 尺寸中核对，Canvas 也设置为 1279×799。之前窗口尺寸不一致、受编译负载影响、或后台限帧的试跑均未用于下面的结果。

```bash
env -u WAYLAND_DISPLAY WINIT_X11_SCALE_FACTOR=1 ./run.sh --benchmark --output results/bevy.json --screenshot results/bevy.png
./run.sh --export results/scene.json
BENCHMARK_WIDTH=1279 BENCHMARK_HEIGHT=799 node benchmark/canvas.mjs results/scene.json results/canvas.json
```

每种渲染器运行两次，均依次单独测量。原始结果保存在 `validation/`。

## 结果

下表是第一次运行的均值与 p95，单位毫秒。FPS 为 `1000 / mean_ms`，不是显示器实际呈现帧率，也不是输入延迟。

| 操作 | Bevy 均值 | Bevy p95 | Canvas2D 均值 | Canvas2D p95 |
|---|---:|---:|---:|---:|
| 静止 | 7.27 | 8.29 | 8.56 | 10.90 |
| 画布平移 | 7.30 | 8.62 | 8.52 | 11.90 |
| 连续缩放 | 7.28 | 9.92 | 34.49 | 57.40 |
| 366 节点一起移动 | 6.08 | 7.06 | 7.77 | 39.60 |

Bevy 第二次运行：缩放均值 6.69 ms、p95 8.12 ms；多节点移动均值 6.63 ms、p95 7.83 ms。其他阶段见 [第二次 Bevy 结果](validation/bevy-repeat.json)。

Canvas2D 第二次运行：缩放均值 31.30 ms、p95 57.50 ms；多节点移动均值 7.57 ms、p95 22.50 ms。画布平移均值 6.44 ms，比该次 Bevy 的 7.08 ms 更低，不能声称 Bevy 在所有操作上都更快。见 [第二次 Canvas2D 结果](validation/canvas-repeat.json)。

Canvas2D 是独立的同场景对照，**并非原版 React/Tauri 应用**。字体栅格化、文字布局、箭头和部分视觉效果不同；不能把结果解释为整个应用重写后的加速倍数。这个样本支持继续验证 Bevy 的保留式 GPU 画布，尤其连续缩放与帧间隔稳定性；尚不能保证完整编辑器、巨大图、图片密集图、其他系统或其他硬件的性能。大量图片并未在本次场景中显示。原版曲线、编辑、保存、撤销、分组行为仍需后续迁移与测试。

## 2026-10-10：编辑交互集中修复

节点颜色的真实 ECS 回归先复现“文档已改色、Sprite 仍是主题色”，修复后修改、撤销、重做立即更新。分组标题通过双击、F2 和右键编辑，保留尺寸与成员；标题保存重开及锁定祖先保护通过。锁定/解锁入口支持 Ctrl+L，验证混合选择、继承锁、成员独立锁、保存与撤销。

拖拽回归先复现 PostUpdate 指针移动 `(-24,-12)` 时可见位移仍为 `(0,0)`。输入后同帧执行 Avian，并刷新本帧扫掠碰撞范围；移除 28ms 跟随、速度限制及旧位置插值。17 项物理测试通过，覆盖 30/60/120/240Hz、2400×1200 快速位移、邻居推挤、嵌套分组与锁定障碍。这是确定性帧序和碰撞验证，未进行桌面端到端输入延迟测量。

空心箭头覆盖线体的网格回归先失败后通过；线型变更更新现有材质，虚线/双线边界使用屏幕导数抗锯齿。10 项边线测试及现有离屏 PNG GPU 回归通过。圆弧弦中点的绘制与命中测试在修复前也通过，因此未声称已复现或修复用户历史截图中的额外直线；该截图在本轮仅有占位符。

画布粘贴识别图片和 SVG 源码，文件拖入共用受限尺寸解码；SVG 原始附件、保存重开、撤销重做与无效数据测试通过。图片角点测试驱动真实 egui 多帧输入，检查累计拖动、停住、释放和撤销重做；Ctrl+滚轮按中心等比缩放。网址节点保留独立的 title/url，保存、复制与撤销通过；目标解析及异步打开失败回传均有测试，不自动打开真实外部网页。

顶部布局在 Intel Iris Xe、Vulkan、原生系统缩放 1.75× 下检查深浅主题的 100%/150%/200% UI 缩放，标签与两侧操作保持单排；另检查固定 1× 预览。代表截图：[深色100](validation/graplu-layout-native-dark-100.png)、[深色150](validation/graplu-layout-native-dark-150.png)、[深色200](validation/graplu-layout-native-dark-200.png)、[浅色100](validation/graplu-layout-native-light-100.png)、[浅色150](validation/graplu-layout-native-light-150.png)、[浅色200](validation/graplu-layout-native-light-200.png)。

普通 niri 窗口在 UI 200% 下仅有 405.5 逻辑内容宽，首次验证暴露标签覆盖两侧控件；增加最窄档收纳后检查通过，见[窄窗口长中文标题](validation/graplu-layout-narrow-405-fixed.png)。另检查[深色右键](validation/graplu-review-context-dark.png)、[浅色右键](validation/graplu-review-context-light.png)、[深色属性](validation/graplu-review-properties-dark.png)、[浅色属性](validation/graplu-review-properties-light.png)，确认颜色、锁定与分组命名入口。

常规工作区测试 219 项通过，21 项需额外环境的测试默认跳过；fmt 与 Clippy 全 targets 通过。原历史中的公式节点及其他仅有图片的功能描述尚未恢复，未将这些条目记为完成。

## 2026-10-10：右键空白、概览对比度与缩远连线

切换文档时 `Tools::default()` 清掉图标，真实 `tabs::apply` 回归先出现 `icons None`，改为保留纹理后通过。常用六项合并单排，缺失图标有文字兜底。在 Xvfb / llvmpipe 中打开两个临时文档、切换第二个标签再右键，验证图标保留与菜单布局，见[切换文档后的菜单](validation/menu-switched-dark-100.png)。未修改用户文档。

概览按分组实际底色选择标题深浅，增加中性遮罩及标题衬底，覆盖深浅分组嵌套。对比度测试遍历两种主题、RGB 三档和三档透明度，黑白底层合成均达到 4.5:1；高 DPI 教程标题图集测试通过。

桌面连线按相机缩放保留至少 1.2 逻辑像素，虚线段和双线间隙一并补偿，箭头限制屏幕尺寸与短边占比。新增四项回归验证实际网格厚度、箭头范围、曲线路径及字号不变、同档缩放和平移不重建、标签不重复更新。离屏 PNG GPU 回归通过，导出保留原文档尺寸。

用户补充的绿色直线截图已定位为 Alt 跳跃预览中的显式起终点辅助线；删除该绘制调用，保留弧线、箭头及范围框。五项既有跳跃交互测试通过。

workspace 常规测试、fmt 和 Clippy 全 targets 通过；最终概览调整后重跑四项概览测试及 Clippy。本机 Intel Iris Xe / Vulkan / Wayland 深浅主题、100%/150%/200% UI 缩放画面见 `validation/context-final-{dark,light}-{100,150,200}.png`：[深色100](validation/context-final-dark-100.png)、[浅色150](validation/context-final-light-150.png)、[深色200](validation/context-final-dark-200.png)。菜单无顶部空白，图标保持单排，缩远连线可见，深色分组使用浅色标题。发布构建安装到 `~/.local/bin/graplu` 后与产物逐字节比对；已打开的旧进程需用户保存后重启。

## 2026-10-10：概览大标题复用 MSDF

概览原先将 64 物理像素的 egui 字形贴图放大，导致“键盘”等大标题边缘模糊。回归 `overview_does_not_magnify_egui_bitmap_text` 在旧路径失败；现在概览使用正文的 `MsdfText`、字体、字形图集和着色器，固定字号排版后变换几何。遮罩和标题衬底迁入同一画布绘制层，保留嵌套顺序、较大标题优先避让、编辑时恢复细节及首页清理。按实际文字布局约束宽高，短标题仍采用原来的自适应大字号。

画布使用线性光 alpha 混合，与 egui 的 sRGB 混合不同；旧遮罩在极端底色下对比度回归降至 4.47:1。遮罩不透明度从 230/255 调至 238/255 后，深浅主题、分组 RGB/透明度组合与黑白底层矩阵均达到 4.5:1。

228 项 workspace 常规测试通过，22 项环境测试默认跳过；fmt 与 Clippy 全 targets 通过。另运行真实教程字体测试：预先生成正文 MSDF 后，100%、150%、175%、200%、350% DPI，三档 UI 缩放与深浅主题均不新增字形或图集页。连续缩放 120 帧复用标题实体、字形和网格；嵌套遮罩/标题深度、重叠避让及清理回归通过。

视觉对照使用快捷键 3.1 fixture、原生 Wayland / Intel Iris Xe / Mesa 26.2.3 / Vulkan、系统 175%、2240×1400 物理窗口：[修复前](validation/overview-msdf-before.png)、[修复后深色100%](validation/overview-msdf-dark-100.png)。完整矩阵见 `validation/overview-msdf-{dark,light}-{100,150,200}.png`；未写入教程文件。截图证明标题边缘与层次，不用于推断加载时间或帧耗时。

## 2026-10-10：首次打开文件的 MSDF 等待

用户确认延迟发生在首次打开文件。原调度器每个后台任务只生成一个字形，最多四个任务，必须等主线程下一帧轮询后才能继续派发。回归 `glyph_workers_continue_a_batch_without_waiting_for_more_frames` 在旧实现仅得到 4 个字形而失败。现在每个任务连续处理最多 8 个字形，仍最多 4 个并发任务，按实际字形数统计未完成工作；图集容量、距离场精度、彩色字体回退及错误提示路径不变。每轮最多接收 32 个字形，避免无界工作与上传。

为排除其他会话同时修改分组样式的影响，最终构建使用隔离源码：`8a62631` 加本次 MSDF 调度/测试，以及任务开始前已有的窗口透明度配置。隔离 workspace 的 229 项常规测试、fmt、Clippy 全 targets 通过；22 项环境测试默认跳过。真实教程的 MSDF 复用/DPI 矩阵另行通过。[最终文字截图](validation/overview-msdf-batch-verified.png) 保持大标题清晰，未降低字形生成精度。

相同 `tutorial-main-3.1.prg`（636 节点、391 连线）、原生 Wayland / Vulkan / Iris Xe、2240×1400、系统 175%，每次重新启动进程且字形缓存为空。基线 `--benchmark --native-scale` 的场景文字就绪 `load_ms` 为 [2750.04 ms](validation/msdf-load-before.json)；固定隔离二进制两次为 [1921.91 ms](validation/msdf-load-after.json)、[1914.23 ms](validation/msdf-load-after-repeat.json)，均值 1918.07 ms，较该基线缩短约 30%。均为 643 个字形、3 页图集、0 个失败标签。计时期间未执行编译或本任务的其他负载；这不是操作系统磁盘冷缓存测试，也不等同于从点击启动器到呈现的总时间，不能据此声称所有文档都低于两秒。

基线二进制 SHA-256 为 `8ba48aa2643ba34a360068a0dd017517e3a0c385601903dab37dc899300d4eee`，最终隔离二进制为 `80bac5092c367ce1a5cce1b7fc66e2699fe173d89b3b28a2ba4e21b434c42538`。共享工作区后续构建曾混入另一组圆角修改，那组中间截图和复测不作为本次对照证据。

## 2026-10-10：分组圆角随视距调整

展开分组采用约 24 个窗口逻辑像素的连续圆角，世界半径随相机靠近而减小，上限为短边四分之一；底色、边框、选中轮廓与概览遮罩共用半径。复用已有九宫格纹理，额外缓存一张概览边线纹理，不在缩放过程中生成纹理。回归验证相机变化后的半径、分组尺寸与文档版本不变、普通节点不受影响、折叠/导出恢复原轮廓，以及静止状态不重复标记 Sprite 变化。概览回归同时检查底色与边框九宫格一致、嵌套顺序与标题优先级。

`cargo test --release --workspace --locked` 的 232 项常规测试、`cargo fmt --all --check` 和 release Clippy 全 targets 通过；23 项环境测试默认跳过。桌面 GPU 回归覆盖近远视距 × 深浅主题 × 100%/150%/200% UI 缩放，逐图检查四角没有被填成直角，两项既有嵌套分组与越界连线 PNG 导出回归通过。实际 DPI 首轮在第九张预览退出后没有截图文件，未计为通过；补充输出日志与缺失文件断言后重跑，完整十二格通过，未通过重试忽略图像断言。预览定时退出与异步截图之间仍有竞态风险，此处没有修改截图生命周期。

视觉环境为原生 Wayland / niri、Intel Iris Xe / Mesa 26.2.3 / Vulkan、2240×1400 物理窗口。先验收强制 1× 预览，再用 `--preview --native-scale` 验收实际 175% 系统缩放；另检查有标题的概览六格矩阵。[近景](validation/group-corners-near.png)、[远景](validation/group-corners-far.png)、[概览深色100%](validation/group-corners-overview-dark.png)、[概览浅色150%](validation/group-corners-overview-light.png) 保留代表截图。使用临时合成文档，未修改用户文件；这些截图不作为性能测量。Mac 安装包尚未包含本次圆角改动。

## 2026-10-10：概览标题层级与半透明内容

标题深度原先随可见概览数量和顺序变化，而 Bevy 0.19.1 的 Mesh2d 透明阶段保留初次入队的排序深度；标题会被后来移动的衬底盖住。成员变化回归在旧实现得到 246.66667 / 250.0 深度不一致，修复后标题保持固定深度并高于全部遮罩与衬底。标题重叠时仍保留较大标题优先。

按用户确认的“淡化组内内容，但小标题仍可见”，整组遮罩使用约 50% 不透明度，仅标题局部衬底保留约 93% 不透明度。未隐藏整个分组的节点文字或子分组标题。回归检查三档视距下内部节点仍可见、文档版本不变；对比度矩阵检查局部衬底达到 4.5:1，同时锁定整组半透明参数。

234 项 workspace 常规测试通过，fmt 与 release Clippy 全 targets 通过。原生 Wayland / Vulkan / Iris Xe、175% 系统缩放，使用教程副本与远处的临时节点检查远景，深浅主题 × 100%/150%/200% UI 缩放均生成截图。[深色](validation/overview-translucent-dark.png)、[浅色](validation/overview-translucent-light.png) 展示内容透出与小标题保留；各图启动视距可能不同，不用于像素差或性能比较。用户的原始教程文件未改写。

## 2026-10-10：缩放后的圆角切片更新顺序

画布视觉更新原先没有声明与 Bevy `ComputeSlices` 的顺序。切片可能先读取旧半径，随后相机更新，使本帧显示旧角和旧线宽；反应式窗口停止重绘时尤其明显。现在统一将画布视觉系统放入 `CanvasVisuals`，在切片计算前完成尺寸、半径和概览遮罩更新。

回归复用实际调度配置，在切片计算阶段检查 0.25→10→1→4→0.25 的往返变化及折叠恢复，确认接收到当前帧的半径/线宽，文档版本不变。234 项常规测试、fmt、Clippy 通过。原生桌面截图回归有一张分组超出视口左边界，未将那次运行计为通过；改在 Xvfb / llvmpipe / Vulkan 隔离窗口运行，近远 × 深浅 × 三档 UI 缩放十二格全部通过。真实系统 175% 的教程矩阵另见上一节；隔离截图不用于性能比较。

## 2026-10-10：文件标签下伸字母被裁切

用户指出 `group-near` 首字母 `g` 下半部分不完整。默认 egui 字体未复现，换用实际界面的 Noto Sans CJK 后，175% DPI 的字形底部为 55.1、裁切底部为 54.0，回归失败。原实现按逻辑行高居中，而字体实际笔画向下偏出；现在按 `Galley::mesh_bounds` 的绘制范围垂直居中，保留既有标签尺寸和横向省略行为。

默认字体与实际 CJK 字体分别覆盖三种名称（下伸字母、中英文及重音字符）、四档 DPI、四档可用宽度。两项回归通过；235 项 workspace 常规测试、fmt、release Clippy 全 targets 通过。系统字体测试单独用 `--include-ignored --test-threads=1` 执行，可通过 `GRAPLU_TEST_FONT` 指定字体文件。

Xvfb / llvmpipe / Vulkan 隔离截图采用 175% 窗口缩放，深浅主题 × 100%/150%/200% UI 缩放六格；[修复前](validation/tab-descenders-before.png)、[修复后深色](validation/tab-descenders-dark.png)、[修复后浅色](validation/tab-descenders-light.png) 保留对照，`g` 下半圈完整。截图验证字形裁切，不作为性能证据。

## 2026-10-10：最近文件自动清理

启动及重新进入首页时清理已不存在或已变成目录的路径，原子保存最近列表；真实存在的损坏文件保留错误提示，不删除用户文件或关闭内存中的文档。新增回归覆盖启动清理、首页重入、持久化结果及损坏文件保留，三项首页测试通过。预览与性能模式跳过个人最近文件记录，避免临时 QA 文件进入日常列表。

## 2026-10-10：最近文件右键菜单

卡片按文件路径保持稳定交互 ID，右键打开“打开文件所在位置 / 删除…”菜单，不打开文档。删除对话框默认仅移除最近记录，永久删除本机文件需显式勾选；已打开的文件禁用磁盘删除，后台再次校验。目录打开使用独立进程参数，不经过 shell，在后台等待并将错误交给现有提示。最近记录原子保存，失败保留可重试的记录。

六项首页测试覆盖最近文件持久化、失效清理、默认保留文件、显式删除、删除/保存失败、包含中文与 shell 字符的目录，以及深浅主题 × 三档 DPI 的真实 egui 右键、确认框和按钮点击。239 项 workspace 常规测试、fmt、release Clippy 全 targets 通过。Xvfb / llvmpipe / Vulkan、175% 窗口缩放的[深色菜单](validation/recent-menu-dark.png)与[浅色菜单](validation/recent-menu-light.png)使用隔离的最近文件目录和临时文档，未对用户文件执行删除。Mac/Windows 的文件管理器启动分支未在本轮实机运行。

## 2026-10-10：普通节点的视距圆角

用户指出黄色“从这里开始放大浏览”普通文字节点缩远后仍近似直角。普通文字、网址和公式节点现在复用展开分组的视距圆角，同步底色、边框、选中与编辑轮廓。往返缩放回归同时检查文字节点与分组的九宫格输入，不改文档版本；239 项 workspace 常规测试、fmt、release Clippy 全 targets 通过。隔离 Xvfb / llvmpipe / Vulkan、175% DPI 下的[黄色远景节点](validation/text-node-corners-far.png)验证实际输出。

## 2026-10-10：标签基线、宽度与间距

用户指出两个展开标签的字位与宽度不协调，间距过大。移除宽度的重复插值和邻近状态对展开高度的叠加；文字使用统一参考字形基线，展开高度 32、左右留白 12、间距 4。下伸字母、中文、重音、数字与未保存圆点在四档 DPI、四档窄宽度下不裁切且基线一致，默认与真实 Noto CJK 字体测试均通过。深浅主题 × 100%/150%/200% UI 截图使用隔离 Xvfb / llvmpipe / Vulkan、175% 系统缩放，[两标签同时展开](validation/tabs-baseline-spacing.png)保留实际输出；244 项 workspace 常规测试及 fmt、Clippy 全 targets 通过。

## 2026-10-10：工具栏悬停高光

复用 `dock_style`，悬停底色改为正文色 8%、选中/按下 13%，圆角 20 且不外扩。文件按钮显式圆角与共享样式一致；普通面板仍保留自己的 8 圆角。相同隔离环境完成深浅主题 × 三档 UI 缩放，保留[浅色](validation/dock-hover-light.png)和[深色](validation/dock-hover-dark.png)实际悬停截图；fmt、244 项常规测试、Clippy 通过。

## 2026-10-10：嵌套标题露角

小标题的高不透明度衬底原先只限制在自身矩形，可能越过祖先的圆角。现在按实际父子关系计算祖先连续圆角的内接区域，并在交集内拟合标题与衬底；不更改分组大小、半透明遮罩或隐藏组内内容。回归验证右上角小标题及衬底的实际位置均位于安全区域，扣除九宫格纹理的透明边距。[隔离截图](validation/nested-title-corner.png)保留“术语介绍 / UI层”嵌套场景；244 项常规测试、fmt 与 Clippy 通过。

## 2026-10-10：Rust 原生 LaTeX 与 MSDF

`LatexNode` 使用固定版本 RaTeX 0.1.14 原生解析、排版，KaTeX 数学字体与已加载的 CJK 字体进入画布现有 MSDF 缓存；分式横线、根号等路径使用 Lyon 矢量网格，无 JavaScript、外部 TeX 进程或运行时下载。保留原节点尺寸并居中，允许编辑源码；语法错误进入现有错误提示并显示源码。保留字体 OFL 原文与来源。

用户的 `equation`、`mathbf`、`frac`、中文 `text` 公式[实际渲染](validation/native-msdf-equation.png)通过；覆盖根号、上下标、分数线、错误回退、主题重着色。连续 120 次变换缩放不生成字形、不增图集、不重建网格；系统字体用串行 `--include-ignored` 测试。245 项常规测试、fmt、Clippy 通过。

本机 release 独立进程测用户公式的解析与排版，首次 1.299 ms，随后 1000 次平均 111.4 μs；测量期间未编译或运行截图任务。该数值不含首次 MSDF 字形生成、GPU 上传及呈现，不代表整帧耗时。后续相机缩放直接复用已生成几何。截图使用 Xvfb/llvmpipe，仅用于视觉验收。

## 2026-10-10：连线预览与成功反馈

参考本地 Project Graph master `1fb1b5cc` 的连接控制器和曲线渲染器。预览加入屏幕尺寸受限的方向箭头和目标轮廓提示，修正目标端点计算使用源端口而非节点中心；连接成功后从正式曲线路径生成 0.4 秒的强调色流动。复用现有效果队列与时间步长，切换标签及失焦清理，不延迟真实拖动。

真实输入回归检查新增连线、撤销/重做、UI 捕获及失焦取消；路径测试核对预览与正式端点一致，动画测试覆盖 30/60/144 Hz 下沿路径绘制、淡出和停止。245 项常规测试、fmt、Clippy 通过。[实际拖动预览](validation/connection-preview.png)在有显式输入焦点的隔离 Xvfb/Vulkan 窗口核对箭头与目标轮廓，测试文档未保存。

## 2026-10-10：组内内容随视距裁剪到祖先圆角

上一轮只约束概览标题衬底，普通节点底色、文字和嵌套分组仍独立绘制，因此缩远后仍会越过父分组圆角。本次按文档父子关系建立共享 GPU 裁剪链，所有节点绘制层使用祖先本帧的位置、尺寸及圆角；遮罩采样外框原有的连续圆角纹理。保留半透明遮盖和角内小标题，跨组连线保持原路径。

像素回归创建绿色父分组与贴角红色子分组：旧安装版在 `(286,673)` 检出角外红色像素，[修复前](validation/group-content-clip-before.png)；新版本通过四角 × 深浅主题八格，覆盖 100%/150%/200% UI 缩放，[修复后](validation/group-content-clip-after.png)。颜色定位边缘时预留两物理像素的抗锯齿误差，同一断言对旧版仍失败，同时要求组内红色区域保留。[嵌套标题实景](validation/group-content-clip-titles.png)检查“术语介绍 / UI层”及亮色节点的叠加。

248 项 workspace 常规测试、fmt、release Clippy 全 targets 通过。新增单元回归覆盖 120 次连续视距/位置变化、祖先链、解除分组恢复及畸形循环的 GPU 终止条件；另连续修改 120 次尺寸，断言 Sprite 替代实体、网格句柄及 MSDF 字形网格均复用。原有近远圆角十二格、嵌套层级四种导出尺寸和越界弧线 PNG 导出亦通过。视觉回归使用 Xvfb / llvmpipe / Vulkan、175% 窗口缩放，不作为 GPU 性能测量；用户文档及最近文件列表未修改。

## 同窗口 React 编辑

2026-10-10：节点编辑预览几何修复与 Wry/React 同窗口输入适配，检查结果及深浅主题 × 三档缩放截图见 [验收记录](validation/react-inline-editor.md)。
