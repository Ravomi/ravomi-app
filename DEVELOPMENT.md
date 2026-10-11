# Ravomi 开发与实现说明

Ravomi 是 Arcraven 的多 Agent 画布工作空间。产品目标是在无限画布上将代码、素材、任务与子代理组织为可连接模块，让任务拆解、并行分支、工作流编排和上下文交接可见、可连接、可干预。本文说明支撑这一目标的现有工程入口与技术约束，不将规划中的 Agent 能力列为已实现功能。

开发前按职责阅读：[README](README.md) 提供产品与仓库入口，[PRD](PRD.md) 定义产品契约与阶段状态，[DESIGN](DESIGN.md) 定义 UI/UX，[AI_UI_RULES](AI_UI_RULES.md) 约束 AI 生成和实现界面，[AI_DESIGN](AI_DESIGN.md) 描述 AI 运行架构。品牌资产及使用边界见 [品牌规范](assets/branding/BRAND.md)。

## 工程入口与实现状态

全平台应用的迁移入口已经位于 `app/`（React + Tauri + Rust），运行与验证见 [app/README.md](app/README.md)；“全平台”是目标，并非各平台均已验收。下面的 Bevy 说明仍适用于已实现的原生画布基线；`./run.sh` 保持启动该基线。新 Tauri crate 通过 `default-features = false` 复用 Rust 文档库，不在新窗口中加载 Bevy。

同窗口 React 文字编辑使用独立入口 `./app/run-canvas.sh 文件.prg`：原生 Bevy 画布通过 Wry 子 WebView 承载输入层，Linux 目前要求 X11/XWayland。先执行 `cd app && npm ci`。手动构建 `--features react-editor` 前必须运行 `npm run build:canvas-editor`，生成的 bundle 不入库。详细边界和测试见 [app/README.md](app/README.md)。

工具浮层截图可在预览命令中追加 `--preview-tool search`、`--preview-tool properties`、`--preview-tool context` 或 `--preview-tool exit`，与 `--light`、`--ui-scale`、`--screenshot` 组合。属性与右键预览选中前四个节点，退出预览在内存中新建节点以触发未保存提示，不写入文件。这些入口仅用于复现控件布局，不替代真实输入操作验收。

面向使用者的介绍见 [README](README.md)，功能状态与交付要求见 [PRD](PRD.md)。本文保留运行、交互细节、渲染、依赖与性能验证方法；历史测量的环境和限制以 [验证记录](VALIDATION.md) 为准。

当前代码使用 Ravomi 名称，共享主题、原生首页及 React 工作区已落实获准风格卡的暖白／石墨方向。图标从已确认头像导出，供网页、原生窗口和 Linux/macOS 打包配置复用；各平台实际安装结果需分别验收。新设计采用低饱和侧栏和清晰工作区。石墨黑 `#202126`、局部雾蓝紫细颗粒柔光主要用于团子猫形象；小猫为一体轮廓、双耳双眼，无嘴、鼻、尾巴或脚。文字、输入框、连线和操作状态保持清晰，不对整层 UI 施加模糊、颗粒或发光。

Ravomi 的 Rust/Bevy 画布架构以本地文档、ECS 场景、GPU 渲染和 egui 编辑界面为基础。源提交、目录对应和许可见 [迁移说明](MIGRATION.md)。AI 执行、TODO 拆解、Agent 编排与交接尚未接入；实现这些能力时必须同时落实真实状态、上下文来源、结果查看与人工介入路径，不用装饰性 Agent 卡片代替执行模型。

独立的 Rust/Bevy 画布性能原型，直接读取原版 ZIP + MessagePack `.prg`。使用用户指定的 [tutorial-shortcut-keys-3.1.prg](https://github.com/graphif/assets/blob/d9d945e549c9cd8a593ba3079d983e805d9c837c/tutorials/tutorial-shortcut-keys-3.1.prg)。完整解析嵌套对象和路径引用，按 UUID 去重：393 节点（345 文字、27 分组、21 连接点）、230 连线（227 有向、3 多端）、14 附件。附件不是全部都在画布中使用。

本机测试与性能结果见 [验证记录](VALIDATION.md)。

Linux 文件管理器的 `.prg` 预览图使用独立离线工具，安装与验证见 [缩略图说明](packaging/linux/README.md)。

## Linux 性能诊断

按项目 [Bevy/Wayland/Vulkan 技能](.agents/skills/bevy-wayland-vulkan/SKILL.md) 采集。`--benchmark --diagnostics` 复用 Bevy 的 `RenderDiagnosticsPlugin`，在 JSON 中记录可用的 GPU/CPU 渲染通道计时及 First 到 Last 的主应用调度跨度。诊断有额外开销，最终性能对照去掉 `--diagnostics`。通道计时不包含所有渲染准备工作，主应用跨度不包含之后的提取和呈现等待；两者不能相加解释为输入延迟。

报告通过窗口 raw handle 和实际 RenderAdapterInfo 记录后端、GPU、驱动、物理/逻辑尺寸、缩放及焦点。`AutoNoVsync` 仅是请求模式，实际呈现模式未取得时为 `null`；缺失 GPU 数据不填成 0。窗口信息复用已有 `raw-window-handle 0.6.2`，MSRV 1.64，MIT / Apache-2.0 / Zlib 三选一，锁文件保留 registry 校验和。

`benchmark/run-suite.sh RESULTS_DIR wayland` 或 `benchmark/run-suite.sh RESULTS_DIR x11` 串行运行测试；默认使用 Wayland + Vulkan。只有实际分辨率、场景、配置相同时才能比较结果。基准和 `--preview` 模式默认保留历史 1× override，追加 `--native-scale` 使用系统 DPI，以复现普通编辑窗口的分数缩放问题。原生 Wayland 的实际物理尺寸仍可能受合成器影响。基准截图在 MSDF 就绪后的第 14 秒、连续缩放阶段抓取；预览仍在第 3 秒抓取。静止基准强制连续刷新，只衡量吞吐，不代表日常空闲耗电。

## 运行

需要 Rust >=1.95、Linux 的 X11/Wayland 与 Vulkan 驱动、curl，以及中文字体。开发机已验证 Rust 1.98.1。Linux 默认中文字体路径为 `/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc`；其他平台/字体位置通过 `--font /path/to/font.ttf` 指定。

```bash
cd /path/to/Ravomi
./run.sh
```

启动器按需编译并进入空白工作区的启动页，不下载教程。显式传入文件路径时打开该文档；追加 `--example` 时首次保存要求另存为。兼容性测试所用的上游教程留在忽略的 `fixtures/`，不随日常启动加载。Cargo.lock 固定传递依赖与 registry 校验和。

直接打开其他原版文件：

```bash
cargo run --release --locked -- /absolute/path/example.prg --font /path/to/font.ttf
```

鼠标滚轮以鼠标位置为中心缩放；触控板双指滑动平移，双指捏合缩放（Linux 原生 Wayland 和 macOS）。Ctrl+双指滑动也可缩放；XWayland 暂不支持原生捏合，可使用这个替代操作。中键或空格+左键平移；空白处右键拖动切割；左键选中并拖动；Shift+点击增减选择；空白区域拖动框选，完整包住分组时包含分组框；Ctrl+A 和工具栏全选包含已有分组，整图再次分组保留嵌套成员；Escape 取消拖动；F 回到全图。

原生 Wayland 支持双指捏合时同时平移：同一次手势的中心随手指位移移动，不需要抬手切换。像素位移按系统 DPI 转回逻辑像素；鼠标滚轮按系统输入来源分类，未满一格的鼠标片段不再误触平移。相机更新仍先于 CameraUpdateSystems；编辑框、弹窗或失焦时丢弃画布手势，恢复后必须从新的手势开始。

鼠标滚轮采用短暂的对数倍率缓动，连续刻度累积目标，反向滚动立即转向；触控板和原生捏合直接响应。拖动、失焦、编辑框捕获或全图操作会中断尚未完成的滚轮动画。缓动期间持续重绘，结束后恢复按需刷新。[公开实现调研与参数说明](validation/canvas-motion-research.md)。

滚动复用 Bevy MouseWheel；组合手势使用 Bevy 提供的 RawWinitWindowEvent，保留原事件顺序、窗口和 phase，避免拆开的 PinchGesture / PanGesture 丢失关联。直接引用的 winit 仍为依赖树中已锁定的 0.30.13，不增加新的版本或运行时库。Wayland 后端回移上游 pointer-gestures 并转发捏合中心位移，沿用已有连接；没有升级到与 Bevy 0.19.1 不兼容的 winit 0.31。来源、哈希、许可证和平台范围见 [补丁说明](vendor/winit/UPSTREAM-PATCH.md)。画布采用指数缩放响应；结束或取消时清除手势状态并保留当前视图。

选中节点后按 Ctrl+G（macOS 为 Command+G），或点工具栏“分组”，创建包含所选节点的 Section。拖动标题或边框会连同成员一起移动，支持嵌套、撤销/重做、保存后重新打开。文字编辑和弹窗获得焦点时不触发画布分组快捷键。分组复用现有文档历史、UUID 和 PRG Section 数据格式，不增加通用分组依赖。

选中节点后按住 Alt，在目标位置按下并松开左键：弧线和外框预览落点，绿色可放置、红色拒绝；落到已有分组会移入，落到空白会移到最外层。无选择时可 Alt+拖动节点选定并放置。成员保持相对位置，移动及归组共用一次撤销；Esc 取消，松开 Alt、切换空格平移或失焦结束预览。普通文字节点可接收子节点并转换为 Section，最后子节点移出时恢复文字节点；锁定节点和将组放入自身/后代会被拒绝。实现复用现有文档历史与 Lyon 曲线，参考 Godot 的 `entity_layer_mover.gd`。

从节点上右键拖到另一节点创建有向连线；工具栏“连线”或按住 E+左键也可使用。边缘命中选择相应端口，中央命中自动裁切到节点边界。连线拖动不足 4 个逻辑像素、释放到空白、Esc 或窗口失焦会取消。每条新连线独立撤销/重做，保存时追加到原版舞台数据并保留附件、未知字段与原有引用。

空白处按住右键，从起点拉出直线切割：红色预览标出受影响节点与连线，释放后删除，可一次撤销/重做。切割检测复用实际 Lyon 曲线路径，包含自环、并行偏移和多目标边。展开分组只切边框或标题，删除框时保留并提升成员；锁定分组及其后代受保护。Esc、窗口失焦或指针离开窗口取消未完成切割。删除保存会重建 PRG 引用并保留存活对象的未知字段、MessagePack 类型和 ZIP 附件。

既有 Bevy 编辑和移动反馈参考同一 master：进入编辑及按 Enter 完成出现短暂绿色扩散框；移动节点从后沿发出白色渐隐粒子；切割有交点闪光、收束刀光和裂开碎片。这是当前基线的实现说明；新界面按 [DESIGN](DESIGN.md) 收敛为短暂、局部、可中断的操作反馈，不能直接把历史特效作为新规范。现实现复用 Bevy `ColorMaterial`、Lyon 和标准库，所有瞬态图形共享一个 mesh，最多保留 512 个效果，结束后恢复按需刷新。刀光阴影使用多层半透明轮廓近似 Canvas 模糊，不承诺逐像素相同。

双击未锁定的文字节点原位编辑：透明输入层沿用节点的位置、字号、内边距和换行模式，外框跟随草稿尺寸，不再弹出独立编辑框。Enter 完成（兼容 Ctrl+Enter），Shift+Enter 换行；Escape、点击别处或滚轮结束并保留修改。编辑时 Ctrl+Z 先作用于文字草稿，退出后作用于文档；一次编辑只产生一条撤销记录。输入法组合输入期间不触发退出，切换系统窗口保留草稿。工具栏提供保存、另存为、撤销/重做、全图、全选和快捷键帮助；Ctrl+S 保存。关闭未保存文档会询问保存、放弃或取消。

文字节点沿用原版 `sizeAdjust`：自动尺寸只在已有换行符处换行，外框根据实际文字宽高调整；手动宽度按字符折行并调整高度。尺寸由同一套 Bevy/Parley 排版结果计算，连线端点、选择区域和碰撞体随之同步。首次加载的字体测量不创建撤销记录或将文档标脏；编辑保存保留自动/手动模式。不同平台字体会改变测量尺寸，未承诺与原版逐像素相同。

Bevy 外观已使用 Ravomi 暖白／石墨共享主题，画布背景不透明；新的颜色、层级、状态和动效以 [DESIGN](DESIGN.md) 与 [AI_UI_RULES](AI_UI_RULES.md) 为准。外观菜单可切换深浅主题及 100% / 150% / 200% 界面缩放；默认浅色，命令行支持 `--dark --ui-scale 1.5`，`--light` 保留兼容。界面缩放独立于画布，显式节点颜色保留。节点与分组复用 Godot `continuous_corners.gd` 的双段三次 Bézier 连续轮廓，半径按 `text_node.gd` 的无描边填充规则取 `floor(min(24, max(0, min(width, height)/2-2)))`；填充、边框与选中轮廓一致。现有 resvg 为 0–24 整数半径生成共享填充/边框纹理，Bevy 九宫格在尺寸变化时切换缓存，固定 4 倍栅格精度；边框保留 1 世界单位，暂未复刻 Godot 按层级调整的屏幕像素描边或动态 DPI 纹理。没有新增依赖或着色器。

## 测速

```bash
./run.sh --benchmark --output results/bevy.json --screenshot results/bevy.png
```

1280×800 窗口、关闭垂直同步，先等待 MSDF 字形与标签 mesh 全部就绪，再预热 4 秒，然后分别测量静止、全图平移、连续缩放、全部未锁定非分组节点一起移动，各 4 秒。报告的 `load_ms` 单列场景建立到 MSDF 就绪的耗时，不是完整进程启动时间。字形失败、120 秒仍未就绪，或稳态采样期间出现新补字/mesh 任务时，输出 `valid:false` 并以失败状态退出，避免缺字造成虚假提速。

报告记录帧间隔均值、p95/p99、物理固定步耗时、活动/休眠刚体、候选碰撞对/接触数、RSS、文字布局与 MSDF mesh 重建次数、生成字形数及图集占用。GPU/驱动信息见运行日志。窗口必须保持可见，不要在测量时编译或运行其他负载。帧间隔不是单独 GPU 执行时间，也不是输入延迟。

`--no-ui` 关闭 egui 绘制；`--physics` 开启推挤并将最后阶段改为移动 8 个节点；`--edit-node INDEX` 聚焦指定文字节点并保持编辑框打开。`--synthetic 1000` / `5000` 生成内存测试图，可组合 `--long-text`、`--images`、`--dense`。合成图不支持 PRG 保存。完成 release 编译后，`bash benchmark/run-suite.sh` 串行执行十种配置、每种三次；不要同时运行多个测试窗口。`--preview --screenshot PATH` 在 MSDF 就绪后第 3 秒截图、第 4 秒退出，可配合 `--light --ui-scale 2` 做外观检查。

测速窗口禁止调整大小，但桌面的 HiDPI 缩放仍可能改变物理像素尺寸。报告的 `window` 字段记录实际物理尺寸，比较前必须核对。Linux X11/XWayland 下可使用 `env -u WAYLAND_DISPLAY WINIT_X11_SCALE_FACTOR=1 ./run.sh --benchmark --output results/bevy.json --screenshot results/bevy.png` 固定为 1 倍缩放。

同数据的 Canvas2D 对照：

```bash
./run.sh --export results/scene.json
node benchmark/canvas.mjs results/scene.json results/canvas.json
```

Node >=22，默认使用 `/usr/bin/google-chrome`，可设置 `CHROME`。复用 Node 标准 HTTP/WebSocket/子进程 API 驱动独立浏览器窗口，不引入自动化依赖。两者应依次运行，使用相同文件、窗口尺寸和轨迹；可通过 `BENCHMARK_WIDTH`、`BENCHMARK_HEIGHT` 环境变量将 Canvas 尺寸与 Bevy 实际报告对齐。Canvas2D 对照重绘同一场景、预先计算换行并裁剪离屏节点。**它不是原版 React/Tauri 应用，结果不能当作完整应用重写加速比。** 字体排版、文字栅格化和部分连线视觉仍有差异。

## 当前范围

支持教程中的节点位置、颜色、中文文本、分组外框、连接点、连线端点与文字。PNG/JPEG 解码复用 image，SVG 栅格化复用 resvg。未知节点/连线类型、缺失引用/附件会报错，避免静默遗漏。

空白画布左键双击新建文字节点，默认内容为 `...`，选中新节点并显示短暂的白绿闪光。在分组内部新建时自动加入最内层分组，并显示四角归组反馈；锁定分组不允许新建。第二次松开鼠标才执行创建，框选拖动、平移和界面操作不会误触发。创建可撤销、重做并保存到 `.prg`；随后双击节点原位编辑。

当前 Bevy 画布支持文字节点创建与编辑、PNG/JPEG/SVG 图片导入、嵌套分组与整组移动、新建连线、切割删除、文档撤销与保存，以及默认开启且可关闭的物理推挤。折叠已有文档模型、显示投影及部分交互回归；`src/viewer.rs` 目前仅在 `--preview-tool collapsed` 时启用折叠投影，正式入口与完整集成待完成，不能仅凭底层测试认定折叠功能已交付。完整能力边界见 [PRD 的 C03 与 B04](PRD.md)。当前仍未覆盖原版所有节点类型的创建及全部特效，也未接入 Agent 运行。节点边框和排版未追求原版逐像素还原。世界文字使用固定逻辑字号的 MSDF 图集，放大由 shader 重建轮廓。已读取全部附件，不代表它们都在场景中显示。原版教程不包含画布图片节点，图片压力测试另用合成图。

## 连线渲染

规则参考上游 `master` 提交 `1fb1b5cc77d5d5a3820cbae4f5d5777339af63a0`：普通 LineEdge 使用对称三次 Bézier，保留端口、并行边偏移、自环、实线/虚线/双线、箭头和标签；ArcEdge 已有圆弧裁剪、箭头与文字位置渲染；MultiTargetUndirectedEdge 使用中心星状、凸包或圆形，保留 rectRates、centerRate、padding 与内外箭头。标签继续共用 MSDF 字形缓存；背景遮罩避免曲线穿过文字。切割已支持删除连线；不能据此声称兼容原版全部曲线类型、全局设置或逐像素还原，文字描边及多目标标签避让仍有差异。实际支持范围以 `src/edges.rs` 与文档解析结果为准。

已评估 [bevy_prototype_lyon 0.17](https://github.com/rparrett/bevy_prototype_lyon#bevy-versions-supported)（支持 Bevy 0.19）与直接使用 [lyon_tessellation 1.0.20](https://docs.rs/lyon_tessellation/1.0.20/lyon_tessellation/)。采用后者处理路径填充、描边及曲线细分，项目只适配原版几何、线型 shader 与 Bevy Mesh2d 缓存。相机平移/缩放不重建曲线；端点整体同向平移仅更新 Transform，端点相对位置改变才重建相关边。凸包直接复用已有 Avian/Parry。基准 JSON 的 `edge_mesh_rebuilds` 记录各阶段重建数量。

Lyon 使用 MIT OR Apache-2.0，纯 Rust，未声明 MSRV；本机 Rust 1.98.1 实际编译验证。新连线 UUID 复用 `uuid =1.27.0` 的 v4，MIT OR Apache-2.0、MSRV 1.89。Cargo.lock 固定传递版本与 registry 校验和，未引入另一套 Bevy 或手写通用曲线细分器。

## Kayak MSDF 文字

节点、分组和连线标签接入本地维护的 Kayak UI 字体生成器，固定上游提交、归档 SHA-256、许可证与修改说明见 [UPSTREAM.md](vendor/kayak_font_msdf/UPSTREAM.md)。未引入旧版 Kayak widget 系统或第二套 Bevy。排版、换行和字体回退仍复用 Bevy 0.19.1 / Parley 0.9.0，编辑框及工具栏仍使用 egui。

启用 Bevy `system_font_discovery`。Fontique 0.9 的 Linux 默认脚本候选不足以覆盖所有符号，应用通过其 `append_fallbacks` 接口追加系统已发现的符号字体，保留原有候选顺序；MSDF 使用排版实际选中的字体和 face，不替换 `⌥` 等字符。系统没有可用字形时明确报告缺字，不把 `.notdef` 方框计为加载成功。Linux 需要 Fontconfig；新增传递依赖 `yeslogic-fontconfig-sys 6.0.1`（MIT，Rust 1.77）及 macOS 条件依赖 `objc2-core-text 0.3.2`（Zlib / Apache-2.0 / MIT，Rust 1.71）由 Cargo.lock 固定版本与校验和，未捆绑系统字体文件。运行验证仍限本机 Linux。

字形以 64 texel/em、8 texel 距离范围按需在后台生成；同一字体 face / glyph 跨字号与标签共享缓存。缩放、DPI 或移动不触发重新排版、生成或 mesh 重建。shader 使用屏幕导数恢复边缘覆盖率；MSDF 是有限分辨率的距离场，极细笔画、极端缩小或放大仍需视觉检查。

线性 RGBA8 图集每页 1024×1024，最多 16 页（纹理数据 CPU、GPU 各最多 64 MiB，不含字体、mesh 和驱动开销）。最多同时生成 4 个字形；本轮应用生命周期内保留缓存，尚无 LRU 淘汰。每个标签按 atlas 页生成 mesh，跨标签 draw call 合批仍需后续优化。非默认可变字体轴、彩色/位图/SVG 字形或生成失败会提示并将对应标签回退为 Bevy Text2d，避免文字丢失；回退标签不具备 MSDF 放大清晰度，性能基准会拒绝把失败状态当作就绪。

维护修复覆盖轮廓闭合、边界合并、归一化丢边、单角轮廓多余空边、三次曲线切线与最近点搜索，以及 CFF / TrueType 轮廓方向差异；误差修正从每行重复扫描全图改为全图生成完成后只执行一次。依赖全部使用纯 Rust 并固定版本，许可证均为 MIT / Apache-2.0；`ttf-parser 0.25.1` 要求 Rust 1.63，`num-derive 0.4.2` 要求 1.56，`Parley 0.9.0` 要求 1.88，均低于应用要求的 1.95。颜色枚举转换直接复用已有 num-traits，省去 num 聚合依赖。

```bash
cargo test --workspace --locked
cargo test --locked chinese_outlines_generate_and_share_atlas -- --ignored
cargo test --locked gpu_renders_chinese_at_multiple_scales -- --ignored --test-threads=1
```

后两项需要本机 Noto Sans CJK 及符号回退字体，GPU 测试保存 `results/msdf-smoke.png`，覆盖相同字号在 1× / 4× / 16× 下的中文与 Option 符号绘制和字形复用。

## 推挤规则

普通交互窗口的“推挤”默认开启，可用工具栏或 `--no-physics` 关闭。载入文档或切换开关不会自动整理、移动节点，也不会生成撤销记录；开始拖动时才运行物理。为保持测速配置可比，`--benchmark` 仍默认关闭推挤，需显式添加 `--physics`；同时传入 `--no-physics` 时以关闭为准。Avian2D 使用零重力、120 Hz 固定步、锁定旋转、线性阻尼 16、零摩擦/弹性。完整分组以单个碰撞体参与外部推挤，内部成员不与自身分组框互相碰撞；单独拖动成员时按现有层级规则拆分。移动分组递归覆盖成员并按 ID 去重，锁定节点及锁定组后代保持受保护。

选中节点通过运动学速度直接跟随鼠标，内部不相互碰撞；对锁定节点统一做 shape cast 裁剪整组位移。动态邻居由物理求解推开。松手后稳定 0.2 秒或最多 2 秒收束为一次文档事务；Esc 恢复所有受影响节点。新拖动、文字编辑、撤销和保存先结束上一物理事务。画布静止时暂停物理时钟，闲置窗口使用按需刷新。

推挤模式把指针位置作为准确落点，不对所拖对象增加弹簧、阻尼跟随或历史位置插值；同帧更新输入、碰撞与显示，快速拖动更新扫掠碰撞范围。锁定障碍限制整组选区可达的位置，多选保持共同位移；松手不额外抛掷，拖动及邻居收束共用一次撤销。关闭推挤后也保持直接拖动。[历史运动调研](validation/canvas-motion-research.md) 中的 28 ms / 12 像素方案只属于当时实验，不描述当前实现。

## 依赖选择

Rust 标准库处理文件、CLI、集合、计时和结果写入；社区库处理通用格式/绘制，代码仅做 PRG 与场景集成。

| 库 | 固定版本 | 许可证 | 要求/用途 |
|---|---|---|---|
| Bevy | 0.19.1 | MIT OR Apache-2.0 | Rust 1.95；2D/wgpu、ECS、窗口、输入和文字 |
| zip | 8.2.0 | MIT | Rust 1.88；ZIP，启用 deflate 解码 |
| rmp-serde | 1.3.1 | MIT | Rust 1.85；MessagePack → serde_json 值 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Rust 1.71；读取数据模型、输出报告 |
| resvg | 0.48.1 | MIT OR Apache-2.0 | Rust 1.85；SVG 解析与栅格化 |
| image | 0.25.10 | MIT OR Apache-2.0 | Rust 1.88；PNG/JPEG 解码 |
| bevy_egui | 0.42.0 | MIT | Rust 1.95；Bevy 0.19 集成、剪贴板、IME |
| egui | 0.36.2 | MIT OR Apache-2.0 | Rust 1.95；TextEdit 与菜单 |
| rfd | 0.17.2 | MIT | 原生保存对话框，Linux xdg portal |
| avian2d | 0.7.0 | MIT OR Apache-2.0 | 官方对应 Bevy 0.19；2D 碰撞、求解、查询与休眠 |

不实现自制 ZIP、MessagePack、字体引擎或 SVG 解码器。Godot Variant 解码不适用于原版测试文件；Web UI 框架不承担此 Bevy 基线的画布绘制。依赖许可证/MSRV 通过 `cargo info --registry crates-io` 检查；本项目沿用 GPL-3.0-only。主要验证环境为 Linux；Bevy 的 macOS 启动与 Metal 离屏渲染已有有限证据，见 [macOS 打包说明](packaging/macos/README.md)，不代表完整交互验收。React/Tauri 与同窗口 React 编辑的跨平台状态分别见 [app/README.md](app/README.md)，不能套用 Bevy 的平台结果。

```bash
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

只检查读取层可使用 `cargo test --locked --no-default-features`，无需 GPU。

## 文档底层

`document::Document` 已支持稳定 ID 的编辑/移动事务、取消、撤销/重做与保存点，历史栈复用 `undo =0.52.0`（MIT OR Apache-2.0）。原版文件通过 `rmpv =1.3.1`（MIT，Rust 1.85）保留 MessagePack 类型并仅修改对应节点字段；其他 ZIP 条目原样复制。`tempfile =3.27.0`（MIT OR Apache-2.0，Rust 1.63）负责同目录临时文件与原子替换，替换前重新读取结果校验。遇到共享碰撞形状等尚不支持的写入结构会拒绝保存，不修改原文件。

可使用 `./run.sh --save-copy /absolute/path/copy.prg` 验证未修改文件的另存为。原版教程的编辑、全图移动、撤销、保存往返、非舞台条目逐字节保留与外部修改冲突检测由 `cargo test --no-default-features -- --include-ignored` 验证。坐标校验容差 0.01 世界单位，与原版序列化精度一致。保存保留节点的自动/手动尺寸模式；原版仍会按自己的字体引擎重新测量，不能承诺两版视觉尺寸逐像素一致。

中文字体由运行时外部加载，不随代码仓库分发；本机 Noto Sans CJK 的许可为 SIL OFL 1.1（`/usr/share/licenses/google-noto-sans-cjk-fonts/LICENSE`）。egui 保留内置字体回退。Bevy 与 egui 使用不同排版引擎；同一字体并不保证完全相同的换行。真实输入法候选窗、组合输入及跨平台行为须人工验收，自动输入测试不能替代。
