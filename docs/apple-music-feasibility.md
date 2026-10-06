# Apple Music 接入可行性

关联 [Issue #5](https://github.com/YingYveltal/Rhine-Music/issues/5)。资料核对：**2026-10-05**；代码基线：`7d1b9304aa4bbe4bc09fccfd5d3fa7c46d5de50d`。初稿交付路线判断；**2026-10-07 已追加原生本机实测，以下历史研究状态以本节更新为准，Rhine 产品尚未接通**。依据限 Apple 官方文档、随系统提供的接口定义和本仓库源码；官网未标发布日期的页面以核对日期为准。

## 2026-10-07 实测更新

`d5b3c55` 独立原生探针在本机 ad-hoc / 无 Team、App Service 或自供 token 的条件下，正常 MusicAuthorization 返回 authorized；MusicLibraryRequest 的已下载和现有库有限查询均成功。ApplicationMusicPlayer 已播放两条个人本地 AAC，并从三条库样本的中间云订阅歌曲起播、定位尾段后自动接到后一条云订阅歌曲。详细源码/包哈希、原始本地证据和限制见 [原生验证报告](APPLE-MUSICKIT-NATIVE-PROBE.md)。

后续 `f090853` 已读取真实25首 Playlist 的 entries/tracks 并核对顺序，验证第10→11及第2→3自动续播，同包 Song 对照和独立中段续播复核通过。快速 Play→Stop 暴露的晚完成收敛已在 `3057140` 修复并定向复测；这不等于通用零瞬态或产品来源互斥已验收。详细版本及证据仍以原生报告为准。

这排除了把“付费开发者会员”视作本次原生读库/播放试验必需前提的做法。它**不证明** Apple Music 目录搜索、REST API、自动 token 配置、正式分发或其他机器/账户均无身份要求。已下载订阅音乐、重复条目通用定位、媒体键、重启、主观出声和 Rhine 集成仍未验证。播放器独立于 Music.app；外部暂停/停止语义不能沿用共享遥控路线。本次 SDK 的 stop 后状态为 paused 并保留位置，需由产品定义停止行为。

## 给产品经理的结论

**能做，推荐保留 Rhine 界面，由一小段原生 Swift MusicKit 代码负责 Apple Music 授权、资料库访问和播放，Rust 继续处理现有业务。** 用户可在同一套专辑卡片中浏览、选歌和控制播放；无需把整个产品改成 Swift。Apple 官方提供应用内独立播放器 `ApplicationMusicPlayer`，其 macOS 支持从 **14.0** 开始。推荐仅让 Apple Music 功能要求 macOS 14+，保留现有 macOS 12/13 的 QQ 与本地音乐能力；这是工程建议，尚未验证条件编译及打包。[官方播放器](https://developer.apple.com/documentation/musickit/applicationmusicplayer)、[MusicKit](https://developer.apple.com/documentation/musickit)

前提应按接口分别核实：原生 MusicAuthorization / MusicLibraryRequest / ApplicationMusicPlayer 的本机实验已有上述成功证据，无需先要求购买开发者会员。用户授权、实际歌曲权益和可播性仍须验证。Apple Music API 的手动开发者令牌及自动令牌 App Service 配置是另一条条件链，不能无差别套到原生本地调用或所有播放操作；正式签名/分发仍待验证。本项目未购买或注册。[开发者令牌要求](https://developer.apple.com/documentation/applemusicapi/generating-developer-tokens)、[自动令牌配置](https://developer.apple.com/documentation/musickit/using-automatic-token-generation-for-apple-music-api)

**获取歌单和歌曲信息不等于取得音频文件。** Apple Music 歌曲的播放参数、预览地址不能当作完整音频下载地址；订阅音乐应交给官方播放器。首版不做“下载成普通本地文件”、音频提取或交给 Rust 解码，也不承诺离线、无损、空间音频或音效处理。Apple 的 MusicKit 条款要求使用官方渲染通道，限制下载、修改等用途；用户自行持有的无保护音乐文件仍走现有本地导入。[歌曲属性](https://developer.apple.com/documentation/applemusicapi/songs/attributes-data.dictionary)、[MusicKit 条款 §3.3.6 D](https://developer.apple.com/support/terms/apple-developer-program-license-agreement/)

## 哪些能力有依据

以下表格描述官方能力；本机已经实测的有限部分以上方更新和绑定版本报告为准，不能等同于 Rhine 已集成。

| 能力 | 官方支持与首版边界 |
| --- | --- |
| 用户授权 | `MusicAuthorization.request()` 请求本应用的音乐访问许可；需要 `NSAppleMusicUsageDescription` 用途说明。系统许可、Apple 账户登录、订阅权益是不同状态，不能把一个成功当作全部可用。Rhine 不收集 Apple 密码。[授权说明](https://developer.apple.com/documentation/musickit/musicauthorization/request%28%29) |
| 用户资料库与歌单 | `MusicLibraryRequest` 在 macOS 14+ 可读取用户音乐资料库；云端 REST 也提供个人歌单/歌曲接口，并要求 Music User Token。首版读取歌单、曲目及封面，分页加载、保留顺序和不可播条目；仅存在于某台设备的文件与云端同步条目不能假定完全相同。[原生资料库](https://developer.apple.com/documentation/musickit/musiclibraryrequest)、[云端歌单](https://developer.apple.com/documentation/applemusicapi/get-all-library-playlists)、[用户令牌](https://developer.apple.com/documentation/applemusicapi/user-authentication-for-musickit) |
| 搜索 | 可搜索 Apple Music 目录中的歌曲、专辑等；目录请求需开发者身份，个人资料请求另需用户授权。结果受 storefront（商店地区）及内容可用性影响，搜到名称不保证当前用户能播放。[目录搜索](https://developer.apple.com/documentation/musickit/musiccatalogsearchrequest)、[地区说明](https://developer.apple.com/documentation/applemusicapi/storefronts-and-localization) |
| 完整播放 | macOS 14+ 的 `ApplicationMusicPlayer` 可在应用内播放；每次播放须处理下架、地区或账户限制；不要用 `MusicSubscription.current` 预检失败短路独立的本地库查询。`canPlayCatalogContent` 用于需要该权益信息的产品流程，不能替代实际条目播放结果。没有订阅不等于 MusicKit 整体不可用，Apple 说明仍可访问已购买或同步的音乐；具体条目可播性需实测。[播放器](https://developer.apple.com/documentation/musickit/applicationmusicplayer)、[订阅能力](https://developer.apple.com/documentation/musickit/musicsubscription)、[WWDC26 授权与订阅说明](https://developer.apple.com/videos/play/wwdc2026/254/) |
| 播放控制 | 官方播放器提供播放、暂停、停止、上一首/下一首、队列及播放位置。Rhine 可以保留原控制栏，把操作和状态交给它；拖动进度、切歌时序和系统媒体键需分别验收，不能凭方法存在就宣称体验通过。[MusicPlayer](https://developer.apple.com/documentation/musickit/musicplayer)、[播放位置](https://developer.apple.com/documentation/musickit/musicplayer/playbacktime) |

## 三条接入路线的实际差别

| 路线 | 用户体验与必要条件 | 判断 |
| --- | --- | --- |
| **原生 MusicKit + Swift 桥接** | Rhine 自己管理 Apple Music 播放队列；保留 Tauri/Three.js 界面。本机 ad-hoc 有限读库/播放已通过，需用户许可和相应歌曲权益，正式身份/分发另验；功能最低 macOS 14。 | **推荐。** 符合播放器体验，避免把 WebView 的流媒体兼容性作为首版前提。桥接与签名包仍须实测。 |
| **控制本机“音乐”App** | Rhine 显示它的资料库，向它发送播放命令；音乐 App 承担播放并共享当前歌曲/队列，用户在两边操作会互相影响。需要安装并配置好音乐 App，用户授予自动化许可。 | 若接受“Rhine 是遥控界面”，值得独立验证。此路线本身不调用 MusicKit 云 API、不需该 API 的开发者令牌；正式分发的签名/公证成本另算。不能把脚本的资料库搜索当作全站目录搜索。 |
| **MusicKit JS 放进 WebView** | Apple 支持在网页内授权、搜索和播放，需要开发者令牌；前端复用程度高。 | 普通浏览器支持不能证明 Tauri 的 WKWebView 和 `tauri://localhost` 环境可用。登录窗口、会话、origin、受保护播放和 CSP 都待验证，不选作本轮推荐首版。[官方 Web 入口](https://developer.apple.com/musickit/)、[Web v3 文档](https://js-cdn.music.apple.com/musickit/v3/docs/index.html?path=/docs/get-started--page) |

本机遥控的依据是 Apple 的 [Scripting Bridge 说明](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ScriptingBridgeConcepts/Introduction/Introduction.html)与当前 Music 1.6.3 随包脚本字典：可枚举歌单/曲目，读取当前曲目、播放状态和位置，调用播放、暂停、停止、前后切歌。该指南已归档（最后更新 2008-03-11），字典只证明接口存在；初稿研究阶段没有启动 App 或发出命令；后续实际结果见 [Apple Events 报告](APPLE-MUSIC-PROBE.md)及[队列对照](APPLE-MUSIC-CONTINUOUS-QUEUE.md)。自动化需要用途说明；Hardened Runtime 下还需 Apple Events entitlement，若将来采用 App Sandbox，须另核对发送目标权限。[用途说明](https://developer.apple.com/documentation/bundleresources/information-property-list/nsappleeventsusagedescription)、[Apple Events 权限](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.automation.apple-events)

**原生 macOS 不能直接使用 `SystemMusicPlayer`。** 当前官方页面不列 macOS，本机 SDK 明确标记 unavailable；不要把 iOS 或 Mac Catalyst 的示例套到 Tauri Mac 应用。遥控音乐 App 是上表第二条路线，不是这个类的 macOS 版本。[平台列表](https://developer.apple.com/documentation/musickit/systemmusicplayer)

如果只想导入歌单目录，音乐 App 可导出 XML；其中只有歌曲信息，不含实际歌曲。这是手动导入的候选方式，不是同步或完整播放方案。[Apple 导出说明](https://support.apple.com/en-ie/guide/music/mus27cd5060f/mac)

## 推荐首版与当前项目的关系

建议下一项实现只做一个闭环：**连接 Apple Music → 浏览个人歌单 → 点播 → 暂停、切歌、拖动进度 → 显示授权/不可播原因**。先只读，不写回或合并平台歌单，不做下载。切换到 QQ 或本地音乐时先停止 Apple Music，反向同理；首版不做目录搜索、跨来源混合队列、自动匹配下载或音乐驱动的实时音频分析。

现有接入点足够开始，不需要先建大型音乐平台框架：

- [Tauri 配置](../src-tauri/tauri.conf.json)当前最低系统 12.0、bundle ID 为 `com.rhine.music.qq`、签名为 ad-hoc；CSP 不允许直接加载 Apple 托管的 JS/媒体。不能把当前 QQ QA 包当作已具备 MusicKit 身份的测试包。
- [命令入口](../src-tauri/src/main.rs)的 `player_command` / `player_state` 目前只服务 Rust 播放器，曲目解析只识别 QQ 和本地路径。建议在此增加小范围 Apple Music 路由，向原界面返回统一的当前曲目、进度及错误；平台 ID 带来源标记，不能冒充本地文件路径。
- [音频模块](../src-tauri/src/audio.rs)使用 rodio 解码文件，QQ 经 resolver 准备音频；Apple Music 应委派 MusicKit，而非塞进该 resolver。已有音量、淡入淡出、背景音等行为不能默认套用；Apple 曲目首版只承诺验收过的官方控制。Three.js 场景不需要因音源更换而重写。

针对 **Apple Music API 自动令牌生成**，官方文档要求开发者门户中的 **Explicit App ID 开通 MusicKit App Service，且与应用 bundle ID 一致**；这是该 API 身份链的条件，不是本次已通过的所有原生读库/播放操作的统一前提。Web/手动 REST 路线则需签发开发者 JWT，私钥不能打包进前端。现有 ad-hoc 身份不足以证明这条正式链路可用；正式签名方式及站外分发包上的授权/播放仍列入下一步验证，不武断宣称必须上架或无需上架验证。[自动令牌配置](https://developer.apple.com/documentation/musickit/using-automatic-token-generation-for-apple-music-api)、[开发者令牌](https://developer.apple.com/documentation/applemusicapi/generating-developer-tokens)

## 验证状态与下一步

初稿（2026-10-05）已完成官方资料与源码只读核对；本机 SDK **26.2** 的 `MusicKit.swiftinterface` 确认：`MusicAuthorization`、`MusicCatalogSearchRequest` 从 macOS 12 可用，`MusicLibraryRequest`、`ApplicationMusicPlayer` 从 macOS 14 可用，`SystemMusicPlayer` 在 macOS unavailable。核对文件位于 SDK 的 `System/Library/Frameworks/MusicKit.framework/Versions/A/Modules/MusicKit.swiftmodule/arm64e-apple-macos.swiftinterface`；遥控接口来自 Music App 的 `Contents/Resources/com.apple.Music.sdef`。初稿仅读官方安装文件；2026-10-07 后续正常授权的有限库读取和播放以上方实测更新为准。

**仍未验证**：重复条目通用定位、目录搜索与自动 token、拒绝/撤销授权、完整音频主观听感、自动跨来源互斥、重启及实际分发包。库读取、个人本地样本、云订阅样本及真实歌单中段自动续播已实测，不能再列为全部未知。

真实歌单独立复核及停止竞态定向修复已有上述结果，下一步按原生和界面拆分任务桥接进隔离 Rhine 包。最小接入契约见原生验证报告。目录搜索属于额外能力，需另验身份条件，不能作为本地库首版的前置门槛。若遇真实平台拒绝，记录错误再决定所需配置，不先购买或修改账户。

当前建议继续独立原生播放器路线；需要产品确认的是它与 Music.app 各自播放、以及与 QQ/本地来源切换的交互方式。会员、签名和正式分发问题按实际接口需求分别决策，不再沿用初稿“开发者会员前提或退回遥控”的二选一。
