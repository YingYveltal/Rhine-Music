# Rhine Music Preview 试用说明

适用 Apple Silicon Mac、macOS 14 或更新版本。这是本地试用包，界面仍由系统 WebView 与 Three.js 渲染，Rust 负责曲库与播放。

1. 解压 ZIP，双击 **Rhine Music Preview.app**；也可以把它拖到“应用程序”后打开。它与原 Rhine Music QQ 并存。
2. 首次没有自己的曲库，可以先查看演示封面。导入时，在 Finder 选中音乐文件夹，按 **⌥⌘C** 复制路径；打开应用的“音乐库”面板，将路径粘贴到音乐文件夹框，点击“保存目录并扫描”。扫描后选择唱片查看曲目和播放。
3. QQ 音乐需要在此应用内重新连接，不会读取或迁移原应用的登录状态。仅在需要持久登录时选择保存连接；权限和曲目可播状态仍取决于 QQ 音乐。

数据保存在 `~/Library/Application Support/com.rhine.music.preview/`，QQ 钥匙串服务与 WebKit 存储也独立。退出再打开会保留你在 Preview 中添加的曲库和设置。QA 测试资料使用另一份身份，不混入正常试用资料。

本包为本机 ad-hoc 签名，未公证、未公开发行。若 macOS 提示无法验证开发者，请从 Finder 右键应用选择“打开”；系统仍阻止时可在“系统设置 → 隐私与安全性”查看本次应用的“仍要打开”。不要全局关闭系统安全检查。

当前来源与文件哈希见同目录 `manifest.json`，实际测试范围见随包的验收摘要。未合入的渲染实验及 Apple Music 不包含在此包中。

## 开发者重建

安装两份 npm 锁定依赖后，在干净、已提交的源码目录执行：

```sh
python3 scripts/build-preview.py --output /绝对路径/新的交付目录
```

脚本使用 `preview` feature 与 `tauri.preview.conf.json` 成对构建并检查身份，生成 app、ZIP、签名记录与 manifest。普通 `npm run tauri -- build --bundles app` 仍沿用原 QQ 名称、存储和 macOS 12 最低要求。

仅内部验收时以 `RHINE_PREVIEW_QA=1` 启动同一二进制，使用 `com.rhine.music.preview.qa` 数据/钥匙串身份与独立 WebKit UUID；正常 Finder 双击不需要环境变量或开发工具。启动后的 `preview-runtime-main.json` 记录实际 WK store getter 结果，不包含凭据。

独立持久 WebKit profile 的 macOS 14 要求已对照锁定 Tauri/Wry 源码与 [WebKit 官方说明](https://webkit.org/blog/14423/building-profiles-with-new-webkit-api/) 核对。
