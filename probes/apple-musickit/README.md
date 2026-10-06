# 原生 MusicKit 有界实验（Issue #28）

这是 Cocoa / Swift / MusicKit 探针，验证当前机器的 ad-hoc 签名条件下，本地授权、有限库读取与 ApplicationMusicPlayer 是否可用。不是 Rhine 产品功能，不承诺能够访问或播放订阅云音乐。

```sh
python3 probes/apple-musickit/build.py
```

输出 ignored `.local/musickit-probe/Rhine Apple Music Probe.app`。沿用已有探针身份 `com.rhine.music.appleprobe`，先退出 Apple Events 版本；两种模式不能并行运行。原来的应用包、manifest 和原始证据保留。新模式增加合法 `NSAppleMusicUsageDescription`，ad-hoc / hardened runtime，没有伪造 Team ID、App Service 或 MusicKit entitlement，没有 token provider。

正常请求 MusicAuthorization；系统授权由用户处理，拒绝不重置或换身份。然后分别执行 `MusicLibraryRequest<Song>`（`includeOnlyDownloadedContent=true` 及 false，每次 limit=3）。不调用订阅或目录预检来阻断本地查询。只记录有限歌曲元数据、错误 domain/code；不输出 token、请求 URL 或完整错误 userInfo。只读列表，不导入、下载或修改歌曲。

若有样本，先独立核对是否为个人无保护文件、已下载订阅或云订阅；下载筛选本身不证明授权/DRM 分类。缺失类别保留未覆盖。播放前确保 QQ、Music.app 停止。独立队列最多 3 首，可从第 2 首开始；不足 2 首时不冒充中段测试。播放器状态与 Music.app 相互独立，不能沿用 Apple Events 共享队列的外部控制承诺。

出现明确签名/App Service/token 拒绝时收束，不注册、付费、借 token 或调整平台限制。构建清单绑定源码/可执行/签名/OS；运行结果另记报告，构建成功不等于功能通过。

公开依据：本机 SDK 的 MusicKit.swiftinterface，Apple [MusicAuthorization](https://developer.apple.com/documentation/musickit/musicauthorization)、[MusicLibraryRequest](https://developer.apple.com/documentation/musickit/musiclibraryrequest)、[ApplicationMusicPlayer](https://developer.apple.com/documentation/musickit/applicationmusicplayer)，以及 [DTS 对 App Service 与 entitlement 的说明](https://developer.apple.com/forums/thread/784114)。该说明不能替代本机运行验证。

真实歌单入口：读取最多10个现有库 Playlist，选一个读取 `.entries`（`preferredSource: .library`）。保留返回顺序、重复条目及 `position`，最多250条，记录分页/截断，不排序或去重。选择表格行后，以完整已读取条目构建独立队列并从该行开始；这是显式 `Queue(entries, startingAt:)`，不是假定已测过 `Queue(playlist:startingAt:)`。首选 library 不保证 SDK 完全不联网，若返回平台拒绝照实记录。

原型播放、继续、前后切均带命令代次；暂停/停止使在途命令过期，取消 Task 并在 await 返回后重新应用最新意图。停止锁存独立的 stopped 意图，继续/前后切不能开启已停止队列，须重新点播；底层可能保留队列与位置，不宣称已经清空。定时状态读取仅重申明确的暂停/停止意图，不根据曲尾猜测补播。这是实验内的明确停止语义；不能从代码推出 SDK 内部绝对无瞬态晚起播，也不是已验收媒体键/产品交互。日志保存队列代次、构造序号/源 ID/运行时 entryID 映射，以及解析后歌曲 ID 和只读随机/重复模式；ID 别名变化不作为换曲触发器。

8e1301a 候选使用 SDK 专用 `Queue(playlist:startingAt:)`，同样返回 code6。该版反复赋空队列仅是尝试，实际条目仍在；后续版本删除这一无效清空循环。先前5e48d4a的条目包装队列在本机返回 MPMusicPlayerControllerErrorDomain/6，不能泛化为会员或 App Service 拒绝。关系日志补充 item 类型及有无播放参数；运行队列只有数量和完整标题序列与读取序列一致时才附同序号来源映射，否则保留原始观察，不强配条目。重复标题及别名的通用身份映射仍须产品实现。暂停/停止补偿前另记原生状态，1秒采样仍不能证明没有更短瞬态回弹；长期暂停意图还会压制外部媒体键恢复，本原型不用于该产品行为验收。

当前候选从同一库 Playlist 另取 `.tracks`，保持返回序列并与 entries 的标题/艺人/数量逐项对照；不按名称重搜曲库。通过后直接 `Queue(for: tracks, startingAt:)`。任何不一致显式失败，不静默去重或改排序。
