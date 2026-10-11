# Ravomi 应用图标

从用户确认的 [透明启动器图标](../ravomi-launcher.png) 等比导出，保留透明底、双耳双眼和局部蓝紫材质。源稿为用户提供的 `ravomi-launcher-v1.png`（1254 × 1254），替换旧暖白底应用图标。使用项目锁定的 Tauri CLI 2.12.1：

```sh
cd app
npm exec tauri icon -- ../assets/branding/ravomi-launcher.png --output ../assets/branding/app-icons --png 32 --png 128 --png 256 --png 512 --png 1024
```

32 px 用于网页图标；128 px 用于原生窗口及 UI 标记；256 px 用于原生首页；Linux 安装器分发 32 / 128 / 256 / 512 px；macOS 打包脚本以 1024 px 生成 ICNS。Tauri 配置引用这些 PNG，不表示 Windows/macOS 安装包已验收。

PNG 导出保留 alpha 通道，四角透明；不再使用暖白底板。单色和 16 px 专用简化版仍待制作。来源与使用约束继承 [BRAND](../BRAND.md)，不替换原始 Logo 或头像。Linux 安装检查不代表 macOS / Windows 打包验收。
