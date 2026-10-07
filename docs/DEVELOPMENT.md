# 开发与构建

[← 文档导航](README.md) · [普通用户下载与使用](PREVIEW.md)

本页面向源码开发者。仓库自 **2026-10-07** 起公开，当前公开试用包为 [v0.1.0-preview.3](https://github.com/YingYveltal/Rhine-Music/releases/tag/v0.1.0-preview.3)。更新这里的文档不重新构建或替换已发布二进制。

## 工程与环境

- macOS、Apple Command Line Tools、Node.js 24 与 Rust。CI 使用 macOS 15、Node 24.14.0、Rust 1.93.1，具体见 [checks.yml](../.github/workflows/checks.yml)。
- `frontend/`：TypeScript / Three.js / Vite 界面，桌面版由系统 WebView 承载。
- `src-tauri/`：Rust / Tauri 核心及原生 MusicKit 桥接；`qq-connector/`：QQ 音乐连接模块。
- 两份 npm 锁文件、两个 Rust crate 的 Cargo.lock 均保留。安装包不包含私人曲库、凭据或生成缓存。

## 安装依赖与检查

从仓库根目录执行：

```sh
npm ci
npm --prefix frontend ci
bash scripts/check.sh
```

检查包括前端构建、内容和视口约束、音乐与动画契约、已有渲染算法检查、QQ connector、桌面默认及 Preview 配置测试，以及 Swift 资料库读取策略检查。需要真实会话或音频设备的 ignored 测试不会自动执行。

[Checks](https://github.com/YingYveltal/Rhine-Music/actions/workflows/checks.yml)在 PR 与 main 更新时运行。自动检查通过不代表扫码、钥匙串、用户曲库、实际声音或性能基准已验收；真实设备验证应使用隔离资料，注明对应提交与测试边界。

## 构建独立 Preview

公开试用入口使用 **Rhine Music Preview**：Apple Silicon、macOS 14+，应用标识为 `com.rhine.music.preview`，与旧 QQ 版的数据、钥匙串和 WebKit 存储分开。

在依赖已安装、源码干净且已提交的目录执行，输出路径必须尚不存在：

```sh
python3 scripts/build-preview.py --output "$HOME/Desktop/Rhine-Music-Preview-build"
```

脚本将 `preview` feature 与 `src-tauri/tauri.preview.conf.json` 成对使用，生成 `.app`、ZIP、签名记录、文件清单和 `manifest.json`。默认复用本工作目录的 `.local/target`；可用 `CARGO_TARGET_DIR` 指定构建缓存。脚本不会启动应用，也不会自动发布 Release。默认产物采用 ad-hoc 签名，不能当作已完成 Developer ID 签名或 Apple 公证。

普通 `npm run tauri -- build --bundles app` 仍沿用 **Rhine Music QQ** 的旧名称、`com.rhine.music.qq` 身份和 macOS 12 最低配置，不等同于当前 Preview；Apple Music 功能本身需要 macOS 14+。如需交付公开 Preview，使用上方脚本。

### 隔离验收

内部 QA 可用 `RHINE_PREVIEW_QA=1` 启动同一 Preview 二进制，使用 `com.rhine.music.preview.qa` 数据／钥匙串身份与独立 WebKit profile。正常 Finder 双击不需要环境变量或开发工具。运行记录 `preview-runtime-main.json` 或对应 QA 文件记录实际 WebKit store 结果，不包含凭据；不要把本机原始验收资料直接上传公开仓库。

独立持久 WebKit profile 的 macOS 14 要求见 [WebKit 官方说明](https://webkit.org/blog/14423/building-profiles-with-new-webkit-api/)。真实账户授权由使用者本人处理，不用改签名、换身份或凭据命令绕过授权。

## 任务与交付

[Issues](https://github.com/YingYveltal/Rhine-Music/issues)记录目标、范围和验收条件，PR 记录实现与结果，提交标识对应源码。播放器与音乐源、渲染与交互共用主线，每项独立需求使用短期分支和独立 worktree，避免多个任务同时修改同一目录。

当前由开发总控协调现有任务的主 agent；不把常规开发默认拆给 subagent。职责与权限以根 [AGENTS.md](../AGENTS.md) 及具体派工为准。设备性能测试、GUI、音频验证和重型构建串行安排，防止相互影响。

恢复任务时核对 Issue、PR、分支与工作区，不把旧对话摘要当作源码事实。提交与推送后创建关联 Issue 的 PR；尚未完成验收时保持草稿。总控核对当前提交、证据、与最新主线的组合后串行合并。代码合入、可以试用和正式发布分别记录，同一 GitHub 账号下的不同 agent 不算独立审批账号。

### 仓库状态

2026-10-05 仓库仍私有时，主线保护 API 返回 403；这是当时的历史限制。2026-10-07 仓库公开后，总控核对 main 保护接口返回 404，**目前未启用分支保护**。PR、检查和串行合并仍由维护者组织，不描述为服务器强制门禁。本轮文档整理不增设管理规则或修改仓库保护设置。

## 来源与历史文档

初始源码来源见 [baseline-source.json](baseline-source.json)，两层上游署名与许可见 [NOTICE](../NOTICE.md)。本仓库从 QQ 原生移植快照建立新历史，前端目录保留上游 v0.3.0 文档和资料；其中的运行方式、版本状态与验证结果只描述当时版本。

后续公开发布及新的产品取舍按照当前授权处理。当前 Release 的构建源码与主线提交关系、校验值及有界验证范围见 [Preview 说明](PREVIEW.md#版本与验证范围)，不能用历史测试代替新版本验收。
