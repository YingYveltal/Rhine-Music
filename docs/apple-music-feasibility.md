# Apple Music 接入可行性

关联 [Issue #5](https://github.com/YingYveltal/Rhine-Music/issues/5)。资料核对：**2026-10-05**；代码基线：`7d1b9304aa4bbe4bc09fccfd5d3fa7c46d5de50d`。本文交付路线判断，**尚未接通账号或验证播放**。依据限 Apple 官方文档、随系统提供的接口定义和本仓库源码；官网未标发布日期的页面以核对日期为准。

## 给产品经理的结论

**能做，推荐保留 Rhine 界面，由一小段原生 Swift MusicKit 代码负责 Apple Music 授权、资料库访问和播放，Rust 继续处理现有业务。** 用户可在同一套专辑卡片中浏览、选歌和控制播放；无需把整个产品改成 Swift。Apple 官方提供应用内独立播放器 `ApplicationMusicPlayer`，其 macOS 支持从 **14.0** 开始。推荐仅让 Apple Music 功能要求 macOS 14+，保留现有 macOS 12/13 的 QQ 与本地音乐能力；这是工程建议，尚未验证条件编译及打包。[官方播放器](https://developer.apple.com/documentation/musickit/applicationmusicplayer)、[MusicKit](https://developer.apple.com/documentation/musickit)

必要前提有两组：开发方具备 Apple Developer Program 资格及配置好的应用身份；用户允许访问音乐资料，播放订阅曲目还需有效 Apple Music 权益。开发者会员当前标准价为 **99 美元/年或当地币价**，不是每名用户都要交的费用；用户音乐订阅另计，按其地区与计划收费。当前项目是否已有开发者资格、可用签名和用户订阅均未核查；本研究不购买或注册。[开发者令牌要求](https://developer.apple.com/documentation/applemusicapi/generating-developer-tokens)、[会员费用](https://developer.apple.com/programs/whats-included/)、[播放权益](https://developer.apple.com/documentation/musickit/musicsubscription/canplaycatalogcontent)

**获取歌单和歌曲信息不等于取得音频文件。** Apple Music 歌曲的播放参数、预览地址不能当作完整音频下载地址；订阅音乐应交给官方播放器。首版不做“下载成普通本地文件”、音频提取或交给 Rust 解码，也不承诺离线、无损、空间音频或音效处理。Apple 的 MusicKit 条款要求使用官方渲染通道，限制下载、修改等用途；用户自行持有的无保护音乐文件仍走现有本地导入。[歌曲属性](https://developer.apple.com/documentation/applemusicapi/songs/attributes-data.dictionary)、[MusicKit 条款 §3.3.6 D](https://developer.apple.com/support/terms/apple-developer-program-license-agreement/)

## 哪些能力有依据

以下“支持”指官方接口能力，不代表 Rhine 已实测。

| 能力 | 官方支持与首版边界 |
| --- | --- |
| 用户授权 | `MusicAuthorization.request()` 请求本应用的音乐访问许可；需要 `NSAppleMusicUsageDescription` 用途说明。系统许可、Apple 账户登录、订阅权益是不同状态，不能把一个成功当作全部可用。Rhine 不收集 Apple 密码。[授权说明](https://developer.apple.com/documentation/musickit/musicauthorization/request%28%29) |
| 用户资料库与歌单 | `MusicLibraryRequest` 在 macOS 14+ 可读取用户音乐资料库；云端 REST 也提供个人歌单/歌曲接口，并要求 Music User Token。首版读取歌单、曲目及封面，分页加载、保留顺序和不可播条目；仅存在于某台设备的文件与云端同步条目不能假定完全相同。[原生资料库](https://developer.apple.com/documentation/musickit/musiclibraryrequest)、[云端歌单](https://developer.apple.com/documentation/applemusicapi/get-all-library-playlists)、[用户令牌](https://developer.apple.com/documentation/applemusicapi/user-authentication-for-musickit) |
| 搜索 | 可搜索 Apple Music 目录中的歌曲、专辑等；目录请求需开发者身份，个人资料请求另需用户授权。结果受 storefront（商店地区）及内容可用性影响，搜到名称不保证当前用户能播放。[目录搜索](https://developer.apple.com/documentation/musickit/musiccatalogsearchrequest)、[地区说明](https://developer.apple.com/documentation/applemusicapi/storefronts-and-localization) |
| 完整播放 | macOS 14+ 的 `ApplicationMusicPlayer` 可在应用内播放；订阅内容先检查 `canPlayCatalogContent`，每次播放仍需处理下架、地区或账户限制。没有订阅不等于 MusicKit 整体不可用，Apple 说明仍可访问已购买或同步的音乐；具体条目可播性需实测。[播放器](https://developer.apple.com/documentation/musickit/applicationmusicplayer)、[订阅能力](https://developer.apple.com/documentation/musickit/musicsubscription)、[WWDC26 授权与订阅说明](https://developer.apple.com/videos/play/wwdc2026/254/) |
| 播放控制 | 官方播放器提供播放、暂停、停止、上一首/下一首、队列及播放位置。Rhine 可以保留原控制栏，把操作和状态交给它；拖动进度、切歌时序和系统媒体键需分别验收，不能凭方法存在就宣称体验通过。[MusicPlayer](https://developer.apple.com/documentation/musickit/musicplayer)、[播放位置](https://developer.apple.com/documentation/musickit/musicplayer/playbacktime) |

## 三条接入路线的实际差别

| 路线 | 用户体验与必要条件 | 判断 |
| --- | --- | --- |
| **原生 MusicKit + Swift 桥接** | Rhine 自己管理 Apple Music 播放队列；保留 Tauri/Three.js 界面。需要开发者资格、匹配的 App ID、用户许可及相应播放权益；本方案功能最低 macOS 14。 | **推荐。** 符合播放器体验，避免把 WebView 的流媒体兼容性作为首版前提。桥接与签名包仍须实测。 |
| **控制本机“音乐”App** | Rhine 显示它的资料库，向它发送播放命令；音乐 App 承担播放并共享当前歌曲/队列，用户在两边操作会互相影响。需要安装并配置好音乐 App，用户授予自动化许可。 | 若接受“Rhine 是遥控界面”，值得独立验证。此路线本身不调用 MusicKit 云 API、不需该 API 的开发者令牌；正式分发的签名/公证成本另算。不能把脚本的资料库搜索当作全站目录搜索。 |
| **MusicKit JS 放进 WebView** | Apple 支持在网页内授权、搜索和播放，需要开发者令牌；前端复用程度高。 | 普通浏览器支持不能证明 Tauri 的 WKWebView 和 `tauri://localhost` 环境可用。登录窗口、会话、origin、受保护播放和 CSP 都待验证，不选作本轮推荐首版。[官方 Web 入口](https://developer.apple.com/musickit/)、[Web v3 文档](https://js-cdn.music.apple.com/musickit/v3/docs/index.html?path=/docs/get-started--page) |

本机遥控的依据是 Apple 的 [Scripting Bridge 说明](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ScriptingBridgeConcepts/Introduction/Introduction.html)与当前 Music 1.6.3 随包脚本字典：可枚举歌单/曲目，读取当前曲目、播放状态和位置，调用播放、暂停、停止、前后切歌。该指南已归档（最后更新 2008-03-11），字典只证明接口存在；本次没有启动 App 或发出命令。自动化需要用途说明；Hardened Runtime 下还需 Apple Events entitlement，若将来采用 App Sandbox，须另核对发送目标权限。[用途说明](https://developer.apple.com/documentation/bundleresources/information-property-list/nsappleeventsusagedescription)、[Apple Events 权限](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.automation.apple-events)

**原生 macOS 不能直接使用 `SystemMusicPlayer`。** 当前官方页面不列 macOS，本机 SDK 明确标记 unavailable；不要把 iOS 或 Mac Catalyst 的示例套到 Tauri Mac 应用。遥控音乐 App 是上表第二条路线，不是这个类的 macOS 版本。[平台列表](https://developer.apple.com/documentation/musickit/systemmusicplayer)

如果只想导入歌单目录，音乐 App 可导出 XML；其中只有歌曲信息，不含实际歌曲。这是手动导入的候选方式，不是同步或完整播放方案。[Apple 导出说明](https://support.apple.com/en-ie/guide/music/mus27cd5060f/mac)

## 推荐首版与当前项目的关系

建议下一项实现只做一个闭环：**连接 Apple Music → 浏览个人歌单 → 搜索目录歌曲 → 点播 → 暂停、切歌、拖动进度 → 显示授权/订阅/不可播原因**。先只读，不写回或合并平台歌单，不做下载。切换到 QQ 或本地音乐时先停止 Apple Music，反向同理；首版不做跨来源混合队列、自动匹配下载或音乐驱动的实时音频分析。

现有接入点足够开始，不需要先建大型音乐平台框架：

- [Tauri 配置](../src-tauri/tauri.conf.json)当前最低系统 12.0、bundle ID 为 `com.rhine.music.qq`、签名为 ad-hoc；CSP 不允许直接加载 Apple 托管的 JS/媒体。不能把当前 QQ QA 包当作已具备 MusicKit 身份的测试包。
- [命令入口](../src-tauri/src/main.rs)的 `player_command` / `player_state` 目前只服务 Rust 播放器，曲目解析只识别 QQ 和本地路径。建议在此增加小范围 Apple Music 路由，向原界面返回统一的当前曲目、进度及错误；平台 ID 带来源标记，不能冒充本地文件路径。
- [音频模块](../src-tauri/src/audio.rs)使用 rodio 解码文件，QQ 经 resolver 准备音频；Apple Music 应委派 MusicKit，而非塞进该 resolver。已有音量、淡入淡出、背景音等行为不能默认套用；Apple 曲目首版只承诺验收过的官方控制。Three.js 场景不需要因音源更换而重写。

原生令牌可由 MusicKit 自动管理，但前提是开发者门户中的 **Explicit App ID 开通 MusicKit App Service，且与应用 bundle ID 一致**；自动管理不等于免开发者资格。Web/手动 REST 路线则需签发开发者 JWT，私钥不能打包进前端。现有 ad-hoc 身份不足以证明这条正式链路可用；正式签名方式及站外分发包上的授权/播放仍列入下一步验证，不武断宣称必须上架或无需上架验证。[自动令牌配置](https://developer.apple.com/documentation/musickit/using-automatic-token-generation-for-apple-music-api)、[开发者令牌](https://developer.apple.com/documentation/applemusicapi/generating-developer-tokens)

## 验证状态与下一步

本轮已完成官方资料与源码只读核对；本机 SDK **26.2** 的 `MusicKit.swiftinterface` 确认：`MusicAuthorization`、`MusicCatalogSearchRequest` 从 macOS 12 可用，`MusicLibraryRequest`、`ApplicationMusicPlayer` 从 macOS 14 可用，`SystemMusicPlayer` 在 macOS unavailable。核对文件位于 SDK 的 `System/Library/Frameworks/MusicKit.framework/Versions/A/Modules/MusicKit.swiftmodule/arm64e-apple-macos.swiftinterface`；遥控接口来自 Music App 的 `Contents/Resources/com.apple.Music.sdef`。仅读官方安装文件，未读用户资料库。

**未验证**：开发者资格与签名身份、自动令牌、用户授权/拒绝/撤销、真实歌单覆盖、账户地区和订阅、完整音频输出、状态同步、跨来源互斥、重启及实际分发包。没有账户操作、凭据读取、GUI 或集成代码；无需为完成本研究提供账号。

下一步最小验证条件是已有或之后经用户批准取得的开发者资格、配置好的独立测试 App ID/签名、macOS 14+、用户同意授权且具有可播放歌曲的测试账户。先用小型原生样例验证授权、读取一个歌单、搜索并播放一首完整曲目及暂停/跳转；再桥接进隔离 Rhine 包，验证与 QQ/本地播放切换、拒绝/撤销后的界面。输出成功与失败证据后，才把文档可行性升级为产品已接通；若样例在目标签名/分发方式上失败，先解决身份或平台条件，不继续扩大实现。

**唯一需要产品决定的问题：首版是否接受“macOS 14+ 的 Apple Music 功能 + 开发者会员前提”，以获得 Rhine 内独立播放？** 推荐接受；若希望先避免这项资格投入，则把首版明确改为“控制已登录的音乐 App”，并接受它共享播放状态。这个选择交总控用于确定 Issue #6 范围，本研究不代替购买或账户授权。
