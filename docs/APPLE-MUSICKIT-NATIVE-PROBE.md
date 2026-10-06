# Issue #28：原生 MusicKit 本机验证

**原生 MusicKit 已在当前 ad-hoc 探针中完成有限曲库读取、个人本地 AAC 播放和云订阅歌曲从样本队列中段开始后的自动续播。** 无需本轮新增开发者会员、Team ID、App Service 配置或自供 token。这纠正了“任何原生 MusicKit 操作都先需要开发者会员”的过强推断，但不是所有部署方式的承诺，也不是 Rhine 已接通。

**后续 f090853 已通过真实25首歌单的顺序读取、中段选曲及自动续播，详见末尾追加结果。** 下述 d5b3c55 是初始三条 Song 样本阶段，保留其原始范围和证据。两阶段均为原型，Rhine 尚未集成。

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

## 初始样本阶段的路线结论

可以继续验证 `ApplicationMusicPlayer`，无需先购买会员作为本机试验前置条件。先完成真实 Playlist 样本与独立复核，再决定桥接 Rhine。Apple Music API 的目录/REST 身份条件、正式签名及分发条件仍分别核实，不能从本次成功推广为全平台免费无门槛。

Apple [MusicLibraryRequest](https://developer.apple.com/documentation/musickit/musiclibraryrequest)和 [ApplicationMusicPlayer](https://developer.apple.com/documentation/musickit/applicationmusicplayer)提供这条原生能力；[DTS 说明](https://developer.apple.com/forums/thread/784114)区分 App Service 和 entitlement，但本轮可行结论来自真实运行，不仅来自这条说明。此前 Apple Events 失败和 GUI 正向对照仍见 [队列对照报告](APPLE-MUSIC-CONTINUOUS-QUEUE.md)，并不与此处独立播放器成功矛盾。

## 真实 Playlist：f090853

干净源码 `f0908533ce6c538e17492f169c83d8479222f7cc`，可执行 SHA-256 `ecf3a69fc29ec452129881384bc62646d81d2fb1731e5094e36c8a1c27c1e409`，Info.plist SHA 与初始包相同。编译无警告、严格签名检查通过，OS/SDK/签名及身份沿用上述条件。后续文档提交不改变已测二进制。

正常授权返回 authorized；最多10个歌单请求返回2个，选定一个已有25首歌单，分别读取原生 `.entries` 和 `.tracks` 关系（preferredSource `.library`）。两者均一页完整返回、无剩余页，数量/标题/艺人逐项一致，entries.position 为0–24。未排序、搜索替代或去重。CUA 检查 Music.app 的“播放列表顺序 / 升序”勾选，并核对原始相邻第9–12行。运行时完整25条标题序列与输入对齐；这不是任意重复条目身份映射已解决的证据。

| 验证 | 实际观察 |
| --- | --- |
| 真实中段 | `Queue(for: [Track], startingAt:)` 从第10首开始，实际进度增长超过22秒；暂停定位301/311.406秒、恢复，自动进入第11首并继续超过19秒；未手工 Next |
| 近开头 | 同一歌单从第2首开始，进度超过19秒；暂停定位326/336秒、恢复，自动进入第3首并继续超过24秒；未手工 Next |
| 前后切与暂停 | 快速 Next→Pause 后第4首 paused / 0.128秒保持；暂停中 Previous 返回第3首 paused / 0秒；Next→Stop 后第4首 paused / 0.126秒保持。两次 Next 实际均先完成，不能宣称验证了在途 Next 的晚完成 |
| 真正在途 Play→Stop | queueGeneration3 的 Play ticket16 在 Stop ticket17 后完成，18:00:53 UTC 记录 staleCommandCompletion；18:00:54 仍需 reassertStop。之后原生 paused / 0.747143345秒稳定超过4分钟；停止后直接 Resume 不重启旧队列。存在晚起播再收敛，不能写成零回弹或队列已清空 |
| 同二进制 Song 对照 | `Queue(for: [Song], startingAt:)` 从三条库样本的第2首起播，定位209秒后自动进入第3首，观察进度超过9.9秒。最终 Stop 在队列结束后发生，原生停留首条 paused / 0秒；不作为末条停止保持的证据 |

只读随机/重复状态为 off/none，没有修改系统偏好。ID 解析可能由库 ID 转为目录 ID，不能用别名变化推断换曲。来源与运行条目仅在整个数量与标题序列匹配后附同序号映射；跨代次 entryID 可复用。重复标题/重复歌曲出现位置尚未专项验证。

### 失败候选

`5e48d4a` 的 `Queue.Entry` 包装序列构造，及 `8e1301a` 的 `Queue(playlist:startingAt:)` 均在本机返回 `MPMusicPlayerControllerErrorDomain / 6`。这不等于 MusicKit 所有路径失败，更没有证据可称为 token、会员或 App Service 拒绝。后者失败后 SDK 延迟暴露 paused 队列，反复赋空队列仍保留条目；f090853 删除该无效循环，改为停止意图锁定并保留 SDK 队列。源码/包与日志分别保留在 ignored `preserved-5e48d4a`、`preserved-8e1301a` 及对应 `evidence/run-*`。

### 本轮证据和边界

不可变本机目录 `.local/musickit-probe/evidence/run-f090853/` 保存本次独立日志1088行，来自累计日志735–1822行，最后为 quit；report 写明观察、范围和遗漏。个人元数据不提交仓库。

| 文件 | SHA-256 |
| --- | --- |
| events.jsonl | `afbcc356b129115346f2c13eaffb080d4e97f496f3f6f555d41ad2e787b0db5b` |
| report.json | `6b78ddbeaf12bf4d7604bc3d61a9e2ec60980c7bb4ea206925e3cefe1ff8260f` |
| manifest.json | `759211c0c6adfe34b2f2638b0d1a07479d1a53827dd28cbd0b16083cd5a130f7` |

原始 `playerBeforeIntent` 记录补偿之前的 SDK 状态，但1秒采样无法排除更短瞬态。探针持续重申 pause/stop 意图会压制媒体键 Resume，**不能照搬成产品暂停循环**。UI 各字段顺序读取，准备期并非原子快照。代理未操作系统权限按钮，不能据此断言无瞬时提示。

f090853 已交同包独立复核。已发现停止收敛需小修：异步操作完成后应无条件补发最新 Stop/Pause，不能因当时 SDK 状态仍为 paused 而跳过，让随后晚起播留给轮询补偿。此项定向修复及复测另记版本，不改写 f090853 结果。

尚未覆盖：重复出现条目的通用定位、完整不定位曲尾、主观实际出声、已下载订阅内容、拒绝/撤销授权、系统媒体键、跨来源自动互斥、重启与分发身份、其他OS/账户。当前成功属于本机有界原型，不是 Rhine 功能验收。

## 独立复核与停止定向修复

总控在同一 f090853 可执行文件独立复核：真实25行歌单第10行起播超过15秒，暂停定位301秒后恢复，自动到第11行、entryID改变且进度超过5.93秒，无 skipRequest；随后停止固定11.5758秒至正常退出。不可变 `evidence/coordinator-f090853/`：events SHA `d3d03d398f3bc7852b523cbbd784dc0de9482d5f7a5db85369ef36feb1cca249`，report SHA `08b67c7e577eb8f7abe58627dc77cbf05a62f693a0002bd6b0475238bd83f10c`，manifest SHA 同 f090853。该复核没有覆盖主观出声或产品媒体键。

最小修复源码 `30571401553693043b4a5e8c9b37d94fe450ae92`，干净构建，无编译警告且严格签名验证通过；exe SHA `8f846c60f923251618868c82a42b014926479cc3ad2f34eb8cd41b20f73c80cd`，Info.plist SHA不变。异步 Play/Resume/Next/Previous 完成的 settle 路径无条件补发最新 Stop/Pause，同时记录 completionIntent 的原生前态；不再以可能滞后的 paused 快照跳过最终命令。

定向重测真实 Play→Stop 捕获 ticket1/latest2：18:12:23 UTC 过期完成时 SDK 仍 paused，立即补发 Stop；18:12:21–44 的25个补偿前观察均 paused / 0秒，无轮询 reassertStop。另一次 Next→Stop 的 Next 实际先完成：Stop瞬间原生态仍 playing / 0.086秒，Stop调用路径立即重申，下一秒为 paused 且位置固定至18:13:27，后续 timer 未再补偿。这没有捕获在途 Next，更不是无瞬态播放的通用保证。未重复已通过的全套正向歌单测试；修复仅改变终止意图收敛。

`evidence/run-3057140/` 保存原累计日志2003–2154共152行，最后 quit；events SHA `ce039b3f795d7f8a9dc6b1331599141bb2d051f0c8b8e7dbe49666d7adc1319d`，report SHA `b3bb6afbad938ffef707186c2ba5b3187a4a75b0cae0894c97024db703a68083`，manifest SHA `04624e5e321dc277b4194690670409aa88e768558014f20e5fa80e64d4942246`。当前原型已退出，设备交回总控；最终文档提交不重建已测包。

## 推荐 Rhine 首版接入契约（提案，未实现）

保留现有 Rust/Tauri 命令入口与 QQ/本地功能，通过小型 Swift MusicKit 模块的异步 C ABI 桥接到 Rust；MusicKit 在 MainActor 管理真实 Song/Track 对象，不阻塞主线程等待异步回调。前端只访问 Tauri。

| 接口 | 推荐约定 |
| --- | --- |
| `apple_request` | `status / authorize / sync / enable({enabled})`；status 至少返回 supported、authorization、enabled、playlistCount、trackCount、updatedAt、job{running,completed,total,message,error}。拒绝/不可用有明确状态，不以目录或订阅预检阻断读库 |
| `/api/library` | 追加 Apple 状态及 source=`apple` 歌单卡片；同步个人歌单快照，保留顺序、重复项、不可播项、分页完成/部分失败信息。首版不含目录搜索 |
| 曲目身份 | UI id 为歌单快照内不透明的出现条目ID，连同源 position 与 snapshot revision 定位；不能用解析 songID 去重。Swift 保留原生 Track，运行队列映射绑定 queueGeneration；遇重复项不能唯一定位时显式报错，不默选第一个 |
| `player_command / player_state` | 保留 receipt 与 appliedCommand 屏障，只有原生结果确认后才推进；增加 source、currentIndex、queueGeneration 和 seek/volume/fade 能力字段。Apple 前后切走原生 skip；暂停后 toggle 真正 resume，不能沿用前端 paused→play(id) 的重建行为；自动换曲以原生状态为准 |
| 来源互斥 | Rust 单一命令序列先使旧准备任务过期，等待旧播放器真实停止且在途异步播放完成/被抑制，再启动新来源。QQ resolver、Rust 音频/BGM 和 Apple 都纳入切换；只设置UI stopped或取消Task不足以保证不串音。原型事后重申停止不能据此承诺零重叠 |
| 系统控制 | 独立 ApplicationMusicPlayer，不假设 Music.app 能控制它。暂停/停止应是明确命令及有限在途收敛；正常媒体键恢复需要更新权威状态，不用永久暂停循环压回去。SDK stop 保留队列/位置，产品显示和恢复语义须一致 |

Apple 功能运行门槛 macOS14+；保留低系统的 QQ/本地能力，在最终 Rhine 包以正常用途说明及真实身份重新验授权。音量、淡入淡出等未验证能力显式不可用，不能改全局系统设置代替实现。原生/构建/数据由音乐来源任务负责，前端由视觉任务负责，各独立 Issue 与 worktree，由总控串行整合。
