# Ravomi 应用图标

从已确认的 [团子猫头像](../ravomi-avatar.png) 等比导出，保留暖白底、双耳双眼和局部蓝紫材质；没有重新绘制造型。使用项目锁定的 Tauri CLI 2.12.1：

```sh
cd app
npm exec tauri icon -- ../assets/branding/ravomi-avatar.png --output ../assets/branding/app-icons --png 32 --png 128 --png 256 --png 512 --png 1024
```

32 px 用于网页图标；128 px 用于原生窗口及 UI 标记；256 px 用于原生首页；Linux 安装器分发 32 / 128 / 256 / 512 px；macOS 打包脚本以 1024 px 生成 ICNS。Tauri 配置引用这些 PNG，不表示 Windows/macOS 安装包已验收。

已检查 PNG 尺寸和 32 px 缩小识别：轮廓与双眼仍可分辨，颗粒随缩小弱化。保持不透明暖白底；透明、单色和 16 px 专用简化版仍待制作。来源与使用约束继承 [BRAND](../BRAND.md)，不替换原始 Logo 或头像。
