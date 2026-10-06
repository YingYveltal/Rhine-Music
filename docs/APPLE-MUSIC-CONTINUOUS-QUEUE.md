# Issue #28：任意点歌的系统队列对照

本轮**未解决任意点歌后的连续播放，不能作为该能力的产品交付**。六个公开接口候选均未建立所需队列；当前证据仅排除这些具体调用在本机样本上的行为，不能推出所有公开接口均不可行。现有整张歌单播放在同一构建的正向对照中仍可自动接续。没有修改 Rhine 产品代码或替用户选择降级体验。

## 测试绑定

2026-10-07（本机 UTC+8），macOS 26.3.1 / Music 1.6.3。一个已有、25 首订阅歌曲的歌单；原型列表与实际选择身份结合持久 ID、时长及 Music.app UI 核对，原始名称和 ID 留在本机。

| 构建源码 | 可执行文件 SHA-256 | 范围 |
| --- | --- | --- |
| `099768b25ea5536a4ebe2e412609a983321c3e48` | `c730f9dcd7c3f02460f6fa2238fbe8d92e28804b932d16ba69f3455993a86777` | 原始嵌套序号事件、即时 Scripting Bridge 引用 |
| `3125cf479190789b09724132d8822c2b4565cd03` | `a654c9955d4db0eb867d9e69c18c539c43404b3f4658d827fb43ab48e6671c46` | 补充候选、曲尾反例、整张歌单对照 |

两次构建源码均干净，clang 编译无警告，ad-hoc 签名严格验证通过。固定 bundle ID `com.rhine.music.appleprobe`，正常权限预检均为 0；未新增或重置授权。旧实例正常退出后才重建，没有两个探针同时运行。报告后的文档提交不代表重新验证另一个可执行文件。

只读记录：`fixedIndexing=false`、`shuffleEnabled=false`、`songRepeat=kRpO`；Music.app 显示不随机、不重复、自动过渡开启。没有改变这些偏好、音量或静音，没有创建或修改歌单。测试期间 QQ Preview 已停止且未操作其登录和曲库。

## 实测结果

| 候选 | 操作与观察 | 结论 |
| --- | --- | --- |
| 原始 Apple Event 嵌套序号 | `hook/Play` 直接参数为 `track n of playlist m`，`once=false`。第 2、10 首身份和进度正确，UI 显示队列中无音乐 | 不能建立后续队列 |
| 即时 Scripting Bridge 引用 | 重新取得 `music.playlists[p].tracks[n]`，不先读取该新对象的 metadata，不调用其 `get`，立即 `play`。第 10 首进度正常；下一首命令成功但仍为原曲，UI 队列空 | 不能归因于旧曲目对象或显式 `get` 才失败 |
| 省略 `once` | 原始事件省略可选参数，从停止状态点播第 10 首，进度增加，UI 队列空 | 不解决 |
| 歌单 `subject` 属性 | 用公开 `keySubjectAttr` 明确歌单；从正向对照已建立的队列和第 2 首切到第 10 首，UI 队列被清空 | 不解决，已有队列也未保留 |
| 曲目对象 `playOnce:NO` | 当前运行时 `respondsToSelector` 为真，实际调用成功；从第 10 首切到第 2 首且进度增加，UI 队列空 | 对象接收器也不解决 |
| 曲目范围 specifier | 公共 `formRange` 引用第 10 至 25 首；回复含 16 个 null 结果，实际落在第 25 首，UI 队列空 | 明确不符合点播目标；仅作为实验保留，不能集成 |

原始嵌套事件另做了两组曲尾反例：第 2 首暂停后定位到 326/336 秒，恢复并观察到 334.85 秒后停止；第 10 首定位到 301/311.406 秒，恢复并观察到 310.21 秒后停止。两组随后均连续观察至少 16 秒，没有下一首出现，Music UI 队列为空。没有发送测试员手工 next 来代替自动续播。

正向对照完全由原型 `play(playlist)` 建队列，第一首实际进度增加；暂停、定位到 230/239.627 秒后恢复，自动出现第二首且继续到 22 秒。原型未在曲尾发送补播命令。此结果限定于现有自动过渡设置和定位后的尾段测试，不宣称整首自然播放或主观听感通过。

最后发送停止，原型稳定为 `kPSS`，Music UI 播放按钮禁用，随后正常退出探针。

## 原始证据与测量边界

本轮工作目录中 ignored `.local/apple-probe/evidence/run-3125cf4/` 保存 `manifest.json`、501 行 `events.jsonl`、`report.json`、`evidence-index.json`。不上传个人曲目原始日志或账户数据。

- 原始日志 SHA-256：`4818ed81030d1d8daf38130b3cc4c260b6cf3a7f95bba91bee85a32bcb25dced`。
- 本地报告 SHA-256：`2a22cc599ac99e73942249de273c5ecbb580762bfd151346ba37962cca2d93c7`。
- 行 1–158 属于 `099768b`；159–501 属于 `3125cf4`。早期运行另保留 `run-099768b` 快照。
- 关键行：22/23 原始嵌套请求；69/70 即时 SB 引用；88 下一首；161 偏好；200/201 省略 once；250/251 范围请求和回复；309–353 靠前曲尾；355–397 中段曲尾；399–451 整张歌单正向对照；453–466 显式 subject 从已有队列切歌；468–489 对象方法从另一首切歌；490/492 停止及状态。
- UI 证据来自当前任务的 CUA 返回文本；本地报告中的 UI 结论是人工整理的观察，未伪称为导出的原始 AX 文件。

状态、曲目和进度仍由顺序属性读取组成，**不是原子快照**。无当前曲目和停止过渡曾出现 `-1728`；成功命令后的旧标签也可能暂留。停止分支不读当前曲目，日志里的空 ID 不证明系统已遗忘身份。本轮没有读取 `currentPlaylist` 作为额外证据，也没有验证重复歌曲、库重排时的行号映射。`fixedIndexing=false` 时，不能把所测行序推广为所有 UI 排序。

没有任何点播队列方案通过前置门槛，因此不运行或冒称通过其完整外部暂停、停止、外部改播及快速指令矩阵。没有实现“接近曲尾且停止便猜 EOF”的自有补播；也没有用连续 next、临时歌单、静音或更改随机偏好模拟点播。实际出声仍待用户确认。

## 最小集成边界与后续选择

本轮没有新增可启用的 `playFromPlaylistIndex` 能力。后续集成应继续区分 `playPlaylist` 的系统队列与 `playTrack` 的单曲结果；发出命令成功不能设置 `continuousQueue=true`。这是接口事实记录，不是已经替用户接受双模式降级方案。

任何后续候选至少要先在靠前和中段完成正确身份、进度、系统队列与自动下一首验证，再运行外部暂停/停止/换曲和快速操作反例。不要用当前非原子轮询日志驱动补播。

剩余产品路线需由总控与用户选择：继续寻找有明确公开依据的新上下文入口；或改变“无需开发者会员”的前提并单独核实 MusicKit 所需能力与成本；或接受整张歌单/单曲的能力边界后再做 Rhine 集成。本轮不购买、不申请新账户、不写用户资料库，也不把实验分支合并为完成品。

接口依据是本机 Music.sdef 与 SDK 的 SBObject、NSAppleEventDescriptor、Apple Event 对象 specifier、ASRegistry 声明。Apple 的 [Scripting Bridge 使用指南](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ScriptingBridgeConcepts/UsingScriptingBridge/UsingScriptingBridge.html)说明对象接收器和空引用风险；[性能指南](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ScriptingBridgeConcepts/ImproveScriptingBridgePerf/ImproveScriptingBridgePerf.html)说明引用与 get。旧示例只作候选来源，结果以本机运行证据为准。
