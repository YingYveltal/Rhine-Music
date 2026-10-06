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
