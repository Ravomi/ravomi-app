# Ravomi 字体资源与来源

本目录记录实际使用的字体来源、校验和与许可。正文和控件排版遵循 [DESIGN](../../DESIGN.md) 与 [AI_UI_RULES](../../AI_UI_RULES.md)：UI 保持清晰，团子猫的雾蓝紫柔光和颗粒不扩展到文字；任务状态须有明确文字或图标，不能仅用 emoji 或形象颜色区分。品牌字标规则见 [BRAND](../branding/BRAND.md)。本说明不改变字体内部名称或第三方许可。

## 彩色 emoji

`NotoColorEmoji.ttf` 是 Google Noto Emoji **v2.051** 的原始 CBDT/CBLC 字体，未修改字体数据。

- 来源：https://github.com/googlefonts/noto-emoji/blob/v2.051/fonts/NotoColorEmoji.ttf
- SHA-256：`72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b`
- 许可证：[SIL Open Font License 1.1](OFL-NotoEmoji.txt)
- 大小约 10.2 MiB，编译时嵌入，不依赖运行时下载或安装系统字体。

本机系统 Noto Color Emoji 使用 COLRv1，当前 Bevy/Swash 路径生成空图块。应用将此 CBDT 字体以内部别名 `Graplu Color Emoji` 注册为 emoji 通用字体；不会修改系统字体配置，也不会将 emoji 换成单色。普通轮廓文字继续使用 MSDF；含彩色字形的整个标签交给 Bevy Text2d 栅格图集渲染。

这不是彩色 MSDF。位图 emoji 在极大倍率下仍会失去清晰度；本字体版本也不代表覆盖未来 Unicode 新增 emoji。

## 数学字体

公式使用 [RaTeX](https://github.com/erweixin/RaTeX) 的 Rust parser/layout/types 与 `ratex-katex-fonts` 0.1.14（MIT），后者编译时嵌入未修改的 KaTeX TTF 字体。字体遵循 [SIL Open Font License 1.1](OFL-KaTeX.txt)，保留原作者版权。运行时无需网络、JavaScript 或外部 TeX 程序；中文 `\\text{}` 复用已加载的画布字体。字形轮廓进入同一个 MSDF 图集，分式横线与可伸展符号路径使用矢量网格。
