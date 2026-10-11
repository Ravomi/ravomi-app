# Ravomi Linux 安装与文档缩略图

本目录提供 Bevy 原生画布及内置 Pi 代码协作引擎的安装入口，以及 `.mio` 项目和 `.prg` 兼容画布的离线预览；不包含完整 React/Tauri 发布。产品与阶段状态见 [README](../../README.md)、[PRD](../../PRD.md)；React/Tauri 入口见 [app/README](../../app/README.md)。

品牌显示与资源替换遵循 [BRAND](../../assets/branding/BRAND.md)，UI/UX 及 AI 实现规则见 [DESIGN](../../DESIGN.md)、[AI_UI_RULES](../../AI_UI_RULES.md)。发布前单独验收桌面图标的小尺寸识别、窗口文字与清晰度；现有兼容图标和旧式缩略图不会因规范更新自动符合新设计。保留既有程序 ID、用户数据路径和 MIME 的兼容性。

## Linux 安装

从仓库根目录编译并安装当前用户的桌面应用：

```sh
cargo build --release --locked
npm --prefix app run build:agent
bash packaging/linux/install-app.sh
```

需要 `desktop-file-utils` 和 `shared-mime-info`。安装后在应用菜单搜索 Ravomi。为兼容已有安装，程序路径暂保留 `~/.local/bin/graplu`，启动项 ID 与旧图标资源保留在 `${XDG_DATA_HOME:-~/.local/share}`，后续由统一打包配置迁移。安装会注册 `.mio` 与兼容 `.prg` 类型，不需要 sudo，不更改默认打开应用。安装脚本也接受已编译二进制的路径作为第一个参数。

安装入口始终启动完整原生画布。Pi 可执行文件放在 `~/.local/bin/ravomi-agent`，随附许可与引擎锁文件放在用户数据目录 `ravomi/engine/`；不依赖外部全局 Pi。可将引擎二进制路径作为安装脚本的第二个参数。引擎构建环境与版本见 [agent-engine](../../app/agent-engine/README.md)。原生协作使用方法见 [画布协作验收](../../validation/canvas-ai-workflows.md)。

## Linux 文件管理器缩略图

为 `.mio` 和 `.prg` 文件生成离线 PNG 预览，复用并维护 Godot 版的 Python/Cairo/Pango 工具。不启动 Bevy/Godot，不执行文档代码，不修改源文件；这不是画布内的节点或分组缩略图。

支持 Ravomi `.mio` 的 ZIP `manifest.json`、`canvas/stage.msgpack` 和 `assets/`；校验项目格式版本及必需能力，不读取或执行协作历史。Linux MIME 新增 `application/x-ravomi`，保留 `.prg` 的 `application/vnd.project-graph` 及别名。

兼容预览 Bevy/master 的 ZIP `stage.msgpack`、Godot 的 ZIP `stage.json`、原生 `PGDOC4` JSON 预览块，以及仅含 `thumbnail.png` 的旧文件。PGDOC4 会校验块范围及 SHA-256，不解码 Godot Variant 记录。缩略图支持范围不等同于编辑器支持导入的格式；编辑器当前只支持 `.mio` 与 ZIP + MessagePack `.prg`。

Bevy 保存编辑后可能保留旧 `thumbnail.png`；本工具优先读取当前 stage。损坏的 stage 会报错，不回退到可能过期的图片。PNG 以临时文件原子替换，失败时保留已有输出；拒绝将源文件或其硬链接作为输出。

## 依赖

使用系统 Python 3、PyGObject、Pycairo、Pango、GdkPixbuf、librsvg 和 shared-mime-info，MessagePack 解码复用 `msgpack`。没有新增 Rust 依赖或构建步骤。

验证环境：Fedora 44 / Python 3.14.7 / Pycairo 1.28.0 / PyGObject 3.56.3 / msgpack 1.1.2 / Pango 1.57.1 / librsvg 2.62.3 / GdkPixbuf 2.44.6（发行版回退至 2.44.4）/ shared-mime-info 2.4。源码要求 Python 3.9+；其他发行版尚未实测。原生库沿用发行版包的版本、签名及安全更新。

Fedora 安装依赖：

```sh
sudo dnf install python3-cairo python3-gobject python3-msgpack pango librsvg2 gdk-pixbuf2 shared-mime-info
```

若需单独安装 msgpack，可在包含系统图形库的 Python 环境中使用锁定的源码包及 SHA-256：

```sh
python3 -m pip install --require-hashes --no-binary=msgpack -r requirements-thumbnailer.txt
```

系统已有 msgpack 1.1.2 时无需运行。中文字体沿用系统配置。

## 运行与安装

以下命令从本目录运行。直接生成图片：

```sh
COLOR_SCHEME=dark /usr/bin/python3 project-graph-thumbnailer.py '/path/中文 文档.prg' /tmp/project-graph-preview.png 256
```

参数依次为输入、输出 PNG、请求尺寸，第四个可选参数是矩形预览高度。为高密度屏幕输出请求尺寸的两倍，边长限制为 64–2048 像素。可设 `COLOR_SCHEME=light`，无主题信息时使用深色。

生成临时打包目录，不改动系统：

```sh
bash install-thumbnailer.sh /tmp/project-graph-thumbnail-package
```

用户需要启用系统文件管理器预览时执行：

```sh
sudo bash install-thumbnailer.sh
```

安装文件为 `/usr/libexec/project-graph-bevy-thumbnailer`、`/usr/share/thumbnailers/project-graph-bevy.thumbnailer`、`/usr/share/mime/packages/project-graph-bevy.xml`。独立名称不会覆盖 Godot 版文件。

MIME 正式名称沿用 Tauri 的 `application/vnd.project-graph`，旧名 `application/x-project-graph` 注册为别名。直接安装会运行 `update-mime-database`；打包安装需由软件包钩子刷新。依据 [Shared MIME-info 规范](https://specifications.freedesktop.org/shared-mime-info/latest/ar01s02.html)和 [Thumbnail 规范](https://specifications.freedesktop.org/thumbnail/latest/)。

两套 thumbnailer 同时注册相同 MIME 时，选择取决于桌面。启用此版本前应通过相应软件包管理方式停用旧的 thumbnailer 注册；脚本不会擅自删除旧文件。GNOME 缩略图沙箱需要系统安装位置，不能直接引用个人目录里的脚本。

安装后重新打开文件管理器，以新文件名复制一份测试文档，避免已有成功/失败缓存影响验收，无需清空用户缓存。显示还受文件管理器的预览开关及大小策略影响。

## 渲染范围

支持中文文本、容器、图片附件、笔迹及连线。解析 master 的几何 `$` 引用；LineEdge 的 0.01/0.99 端口位于边缘中心，默认中心端口投向矩形边界。支持 Bézier 曲线、实线/虚线/双线、燕尾箭头和空心/实心三角、菱形。MultiTarget 的 line 模式从成员包围框的 `centerRate` 中心向各成员展开，标签只绘制一次。

这是文档概览，仍使用 Godot 版的节点样式与容器排版。ArcEdge、自环、同端点平行连线使用简化曲线；MultiTarget 的 convex/circle 暂按中心连线预览，不保留包围轮廓。未保存的编辑不会出现在预览里。

输入限制：ZIP 256 MiB、10000 条目、stage/单个预览块 16 MiB、5000 对象、引用树深度 100。CLI 有 15 秒处理预算；失败返回非零状态及简短错误。图片解码使用系统库，不访问文档中的远程资源。

## 验证

从仓库根目录执行：

```sh
/usr/bin/python3 -m unittest discover -s packaging/linux -p test_thumbnailer.py
/usr/bin/ruff check packaging/linux
/usr/bin/ruff format --check packaging/linux
bash -n packaging/linux/install-thumbnailer.sh
```

2026-10-09：25 项测试通过，涵盖临时 DESTDIR 安装、真实 MIME 数据库刷新、中文/空格路径 CLI、损坏输入与源文件保护、旧图片失效处理、几何引用、精确端口、多端中心、线型/箭头、字号换算和 Godot 格式回归。master 的基准字号为 32，连线宽度为源 TextNode 字号 / 32 × 2，标签字号为 32 × 线宽 / 2。

真实教程已生成 1024×1024 预览并目视检查。源文件 SHA-256 保持 `e61ea94d505cab5eeca7c5b236b8afaf049d9d34211342f71a5f2934bcd4c49d`。教程位于忽略目录 `fixtures/tutorial-shortcut-keys-3.1-original.prg`，不随此改动分发，缺少它时该项测试跳过；其余 24 项使用临时生成的夹具。

本次未向 `/usr` 安装，未验证真实文件管理器缓存调用。已完成离线生成及临时安装目录集成验证，系统显示需要安装后的桌面验收。

## 来源与许可

初始辅助程序、15 项测试及 msgpack 清单取自用户指定的本地 Godot 项目 `/home/waya/Projects/Software/project-graph/packaging/linux/`，参考提交 `30b534ad2ad18aa7b62adee39e8ec7f19c9373fd`。复制时内容指纹：

- `project-graph-thumbnailer.py`：`e5f1999c04b994d62f004df72c353e248d1e68d1fc92a82234c19509b40ad421`
- `test_thumbnailer.py`：`257281b9ea0645e25486d0683b76b7936b2c5440648d7430fe095efdb4b826cc`

来源文件没有独立许可证头，参考工作区没有顶层 LICENSE；这里记录用户明确要求的本地移植来源，不宣称来源代码获得了 MIT 等第三方授权。本项目声明 GPL-3.0-only，许可证见 [LICENSE](../../LICENSE)。独立打包分发前需要补齐来源许可声明。

系统包许可：Pycairo 为 LGPL-2.1-only 或 MPL-1.1，PyGObject 为 LGPL-2.1-or-later，Pango 为 LGPL-2.0-or-later，librsvg/GdkPixbuf 为 LGPL-2.1-or-later，msgpack 为 Apache-2.0。图形和排版库通过系统包使用，没有复制其源代码。复用这些库处理字体排版、图片解码、曲线和二进制格式，新增代码负责 Project Graph 格式及桌面集成。
