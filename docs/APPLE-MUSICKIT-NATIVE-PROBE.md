# Issue #28：原生 MusicKit 本机验证

**原生 MusicKit 已在当前 ad-hoc 探针中完成有限曲库读取、个人本地 AAC 播放和云订阅歌曲从样本队列中段开始后的自动续播。** 无需本轮新增开发者会员、Team ID、App Service 配置或自供 token。这纠正了“任何原生 MusicKit 操作都先需要开发者会员”的过强推断，但不是所有部署方式的承诺，也不是 Rhine 已接通。

这次队列来自 `MusicLibraryRequest<Song>` 的最多 3 条样本，**尚不是用户指定歌单的真实顺序**。下一门槛是已有 Playlist 的原生关系读取、顺序和中段起播；不能将当前样本成功等同于歌单功能完成。

## 版本和运行条件

- 源码：`d5b3c55f6b1119def8845a7dd9efdaa6c9c40dee`，干净构建；后续报告修改不表示另一个包被测过。
- 可执行文件 SHA-256：`ee1f4686030194ffbf21bf555677ed97b70785a09a782452e9339338f53b957b`。
- Info.plist SHA-256：`f405a21c9cf0c5cf8452a882ee28a57d6e3aad15eb95d7f44aa768985ae550ff`。
- macOS 26.3.1 (a), build 25D771280a；SDK 26.2，原生 arm64 / macOS 14+ 目标；Swift 编译无警告、严格签名验证通过。
- 沿用 `com.rhine.music.appleprobe`，ad-hoc / hardened runtime，无自定义 entitlement、Team 或 provisioning profile。新增正常 `NSAppleMusicUsageDescription`。旧 Apple Events 探针先退出，保留其应用与证据，没有重置 TCC 或换身份绕过拒绝。
- 独立构建路径为 ignored `.local/musickit-probe/Rhine Apple Music Probe.app`，源码及构建入口见 [探针说明](../probes/apple-musickit/README.md)。

## 有限实测

| 能力 | 操作与证据 | 结论 |
| --- | --- | --- |
| 音乐资料库授权 | `MusicAuthorization.currentStatus=notDetermined`，正常 `request()` 后返回 authorized，原型日志行 1–6 | 成功。代理没有点击系统权限按钮；没有观测证据能断言用户是否操作过瞬时系统提示 |
| 已下载样本 | `MusicLibraryRequest<Song>`，`limit=3`、`includeOnlyDownloadedContent=true` 返回 2 首，行 8–9 | 成功。两个样本在 Music「歌曲信息 → 文件」均显示 AAC、已上传和既有本地 m4a 路径；没有为测试新增文件 |
| 现有库样本 | 同请求 downloaded=false、limit=3 返回 3 首，行 12–13 | 成功；不表示完整曲库或 Playlist 已验证 |
| 个人无保护 AAC | 两条原有本地样本从 index0 起播，进度增长；暂停、定位172/181秒、恢复，自动切换第2条，继续到21秒，行 20–84 | 有限队列通过。没有调用手工 next 或曲尾补播 |
| 云订阅中段播放 | 3条库样本的 `Queue(for:startingAt:)` 从 index1 起播，行164；该样本和后续 index2 均在 Music 文件页明确显示 Apple Music、流播放、云服务，标题/艺人/专辑匹配 | 订阅样本类型得到 GUI 独立核对，不以 hasPlayParameters 或 downloaded 标志代替订阅证据 |
| 云订阅自动下一首 | 中间曲目持续播放超过30秒，暂停、定位209/219.467秒、恢复；218.5秒后自动进入第3首（约15.867秒），条目及解析后 songID 改变，进度到11.94秒，行169–237 | 有限中段→相邻样本通过，尚未整首不定位播放 |
| 停止 | 本地及订阅两次 `stop()` 后均保持无播放进度增长；SDK 状态稳定为 paused，保留位置/当前条目，本地保持超过60秒，订阅超过40秒，之后正常退出 | 不能假设 stop 返回 stopped 枚举或清空队列。产品必须明确停止意图、是否归零及队列处理 |

Music.app 在这些原生播放期间保持未播放；QQ Preview 也停止。`ApplicationMusicPlayer` 维护独立队列，不需要 Music.app 手工建队列。此阶段没有对 Music 发送 Apple Events，也没有 AX 产品自动化代码。

**独立播放改变了外部控制契约。** Music.app 的暂停/停止不应被假设能遥控这个播放器；原先共享播放器的外部操作验收不能直接沿用。跨来源互斥、媒体键、进度与队列归属需要在后续集成中明确定义并验证。

## 证据与边界

开发者自测快照在本机 ignored `.local/musickit-probe/evidence/run-d5b3c55/`，日志共 298 行，最后为 quit。原始曲名、个人库 ID 和路径不提交到 GitHub。UI 来源核对是 CUA 工具返回内容的人工整理，未声称有导出的原始 AX 文件。

| 文件 | SHA-256 |
| --- | --- |
| events.jsonl | `55eac1ba0ead73049cb11f4b0bbfaee8b707bf182af03f41907cc6a66e8da17b` |
| report.json | `52ae777240ebdede8cf33d2debc043f8a2b4ce31d6186e0cbe5470b688da2f17` |
| manifest.json | `64d523fed836a913e64990267092e08afa73428bb06faf22f78b8a531c09114c` |

错误只记录 NSError domain/code 链，不读取或打印完整 userInfo、URL、凭据或 token。此轮没有运行期 API 错误。没有使用 MusicSubscription.current 或目录请求提前阻断本地查询；也没有显式发起 MusicCatalogSearchRequest、MusicDataRequest 或配置 token provider。

库 ID 与实际播放时解析出的目录 ID 可能不同。本地条目的 `entry.item` 为 nil，日志 songID 为空；本次本地身份依据有限且唯一的标题、队列顺序和不同 entryID，不能当作通用 ID 映射方案。云样本有解析后的 songID，亦须维护库条目和队列条目的映射。

未覆盖：用户指定 Playlist 及其真实顺序、重复歌曲、已下载的订阅内容、完整不定位曲尾、主观实际出声、拒绝/撤销授权、目录搜索/REST/自动 token、重启恢复、其他 OS/账户、正式签名分发、快速指令竞态、系统媒体键、Rhine UI 和自动跨来源互斥。只读已下载请求返回的两条样本均为个人 AAC，本轮没有下载订阅歌曲来补测试。

## 路线结论

可以继续验证 `ApplicationMusicPlayer`，无需先购买会员作为本机试验前置条件。先完成真实 Playlist 样本与独立复核，再决定桥接 Rhine。Apple Music API 的目录/REST 身份条件、正式签名及分发条件仍分别核实，不能从本次成功推广为全平台免费无门槛。

Apple [MusicLibraryRequest](https://developer.apple.com/documentation/musickit/musiclibraryrequest)和 [ApplicationMusicPlayer](https://developer.apple.com/documentation/musickit/applicationmusicplayer)提供这条原生能力；[DTS 说明](https://developer.apple.com/forums/thread/784114)区分 App Service 和 entitlement，但本轮可行结论来自真实运行，不仅来自这条说明。此前 Apple Events 失败和 GUI 正向对照仍见 [队列对照报告](APPLE-MUSIC-CONTINUOUS-QUEUE.md)，并不与此处独立播放器成功矛盾。
