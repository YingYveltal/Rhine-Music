# Rhine Music 文档

[← 项目首页](../README.md) · [下载当前 Preview](https://github.com/YingYveltal/Rhine-Music/releases/tag/v0.1.0-preview.2) · [反馈问题](https://github.com/YingYveltal/Rhine-Music/issues)

## 下载与使用

| 想了解什么 | 从这里开始 |
| --- | --- |
| 兼容性、安装、连接三种音乐来源、操作与常见问题 | [Preview 使用说明](PREVIEW.md) |
| 本次下载的改动、校验值与验证范围 | [v0.1.0-preview.2 Release](https://github.com/YingYveltal/Rhine-Music/releases/tag/v0.1.0-preview.2) |
| 项目来源、作者署名及资源权利 | [NOTICE](../NOTICE.md) · [代码许可证](../LICENSE) |

## 开发与维护

[构建、检查与协作入口](DEVELOPMENT.md)包含源码开发步骤。普通试用者直接下载应用，无需执行构建命令。

当前试用版是 Rust / Tauri 桌面应用，前端仍使用 WebView / Three.js，Apple Music 使用原生 MusicKit。实验性渲染文档不代表已成为默认渲染路线。

## 历史与研究资料

这些文件保留各阶段的事实与当时结论，其中“尚未接通”“待验收”“未公开”等表述属于该阶段，不能代替当前 Release 状态。

| 资料 | 所属阶段 |
| --- | --- |
| [上游音乐版 README](../frontend/README.md) · [上游原版 README](../frontend/README.original.md) | RonaldDeng v0.3.0 音乐适配，以及 LBEILC 三维档案界面；不是当前 `.app` 使用说明 |
| [工程来源快照](baseline-source.json) · [初始基线](BASELINE-2026-10-05.md) | 2026-10-05 建立本仓库时的源码与检查记录 |
| [Apple Music 路线研究](apple-music-feasibility.md) · [原生接入记录](APPLE-MUSIC-NATIVE-INTEGRATION.md) | 专辑接入之前的可行性研究与歌单实现；当前专辑支持见 [Issue #43](https://github.com/YingYveltal/Rhine-Music/issues/43) |
| [渲染迁移研究](rendering-migration.md) · [默认渲染验收](rendering-default-qa.md) | 特定提交上的实验和有界验证，不能视作所有设备的性能承诺 |
| [首页素材说明](media/README.md) | 品牌示意图与明确标注的上游历史界面图 |

本仓库基于 Rhine-Music-Demo **v0.3.0** 移植快照继续开发，没有自动合入上游后续版本的全部功能。当前状态以本仓库的 Release、Issue 和 PR 为准。
