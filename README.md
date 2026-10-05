# Rhine Music

Rhine Music 的统一开发工程。初始源码来自已使用的 QQ 版构建快照，保留 Rust 播放与曲库核心、Tauri、Three.js 界面和 QQ 接入。后续在同一主线上持续推进「渲染与交互」及「播放器与音乐源」。

当前基础应用名称为 Rhine Music QQ，应用标识为 `com.rhine.music.qq`；迁移不更换既有用户数据目录。开发与自动测试使用隔离数据，真实账号登录和设备体验另行验收。

## 构建与检查

需要 macOS、Apple Command Line Tools、Node.js 24 和 Rust。两份 npm 锁文件和两个 Rust crate 的 Cargo.lock 均保留。

```sh
npm ci
npm --prefix frontend ci
npm run build
npm --prefix frontend run check:content
npm --prefix frontend run check:viewport
npm --prefix frontend run check:music
./scripts/cargo.sh test --locked --manifest-path qq-connector/Cargo.toml
./scripts/cargo.sh test --locked --manifest-path src-tauri/Cargo.toml
```

将 Rust 的 cargo/rustc 加入 PATH 后，可独立打包：

```sh
npm run tauri -- build --bundles app
```

产物位于 `src-tauri/target/release/bundle/macos/`。这里直接构建已纳入 Git 的源码，不调用会覆盖旧项目 QQ 快照的构建脚本。安装包只作本机测试，正式发布单独验收。

默认 Rust 测试不需要真实 QQ 会话；标记 ignored 的真实账号和音频设备测试不自动执行。自动检查通过不等同于全新扫码、钥匙串、真实试听、画面和性能已经验收。

## 开发入口

GitHub Issue 记录目标、范围和验收条件；每项需求使用独立分支与工作目录，经 PR 交付并合回 main。项目职责和验收规则见 [AGENTS.md](AGENTS.md) 与 [开发与验收入口](docs/DEVELOPMENT.md)。已验收的默认渲染与 QQ 修复现已纳入统一主线；未合并实验继续留在独立 Issue 分支。

独立 macOS 试用包的使用与构建见 [Preview 说明](docs/PREVIEW.md)。

## 来源

上游为 [RonaldDeng/Rhine-Music-Demo](https://github.com/RonaldDeng/Rhine-Music-Demo) v0.3.0（`e910899`）。本仓库从本地原生 QQ 移植快照开始记录新的提交历史，并保留原作者署名、LICENSE、NOTICE.md 与前端许可说明。前端历史文档和旧项目指令中的发行记录只描述上游版本，不代表本仓库已公开发布或当前版本已重新通过所有历史验证。

本仓库不包含用户曲库、登录状态、密钥、生成缓存或安装包。
