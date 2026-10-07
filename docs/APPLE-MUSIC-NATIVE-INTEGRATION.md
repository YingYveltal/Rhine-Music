# Apple Music 原生接入（Issue #30）

本任务把 MusicKit 原型接到现有 Rust/Tauri 核心，前端由 #31 独立负责。与 #31 的组合 Preview 已完成有界实机自测，待总控独立抽查及串行合并。

`src-tauri/native/AppleMusic.swift` 通过异步 C 回调向 Rust 返回 JSON，所有 MusicKit 对象与队列操作留在 MainActor。Rust 的调用线程等待回调时不阻塞应用主线程。构建脚本编译最低 macOS12 的静态桥，MusicKit 库/播放器入口受 macOS14 availability 检查保护；用途说明合入 Info.plist，不改 bundle identity、Keychain 或 WebKit 数据目录。低系统运行仍需实际机器验收。

## 数据与接口

- `apple_request({operation,body})`：status、authorize、sync、enable；进入界面只 status，authorize 仅在明确点击后请求，已拒绝或受限不反复弹窗。首次没有保存偏好时 enabled=true，但不自动授权或读库；明确关闭后保留false，避免授权同步成功却看不到卡片。
- 同步从个人 Playlist 分页读取 entries 和 tracks，核对数量和完整顺序，保存不同出现位置，不按曲名去重。原型10/250限制已移除。分页/顺序失败保留上次完整缓存，显示 job.error，不提交半份曲库。
- 缓存位于当前应用数据目录的 `apple/library.json`，封面单独保存在 `apple/covers`。关闭来源保留缓存与偏好；重新启动可显示缓存。点播时若原生对象尚未载入，重新读库恢复引用；快照发生变化则明确要求重新同步，不猜测匹配。
- `/api/library` 增加 apple 状态及 source=apple、kind=playlist 卡片，曲目ID是带快照版本和位置的不透明出现条目ID。同样的条目序列重复同步ID稳定；平台条目或顺序变化后生成新版本，旧ID需要重新同步后点播。browserPlayable=false，原生支持类型独立判断，缺少playParameters不阻断MusicKit真实尝试。不会伪造本地路径、编码器或音频下载地址；封面使用现有本地资源协议。
- 原生队列仅包含当前支持的音频 typed Track；不支持的视频仍保留在资料库展示，直接点选会明确报错。筛选队列保留原始出现位置，状态返回原列表索引。先 prepareToPlay，再核对命令未过期、完整顺序/标题、runtime ID 唯一性与所选位置，通过后才 play；失败或未绑定的队列不能直接恢复/切歌。所选 Track 的平台ID在请求队列内重复时，目前显式返回“无法唯一定位”，避免 SDK startingAt 选错同曲位置。重复项仍完整展示，自动队列映射按全序条目和 queueGeneration 维护；重复项可用性需组合验收。

示例位于 [API样例](apple-native-api-examples.json)，均为人工构造的脱敏契约示例，不是真实账户或验收记录。

## 统一播放与来源互斥

`player_command` 保留单一递增receipt。`player_state.appliedCommand` 只有在对应操作完成、明确失败或被已完成的后续命令替代后推进；辅助设置的完成不能越过仍在途的播放命令。queueGeneration 只在本后端标识队列，前端跨来源以receipt确认。

切换到 Apple 时，Rust先使旧QQ准备任务失效，停止歌曲并挂起背景音，等待实际 sink 排空；同时等待MusicKit旧在途操作结束并确认停止，再启动新队列。切回Rust时先完成MusicKit停止屏障，再恢复Rust背景音策略并起播。挂起不改用户音量/背景音偏好。快速 Stop 可以使待启动的队列过期；旧回调不得恢复新状态。QQ退出只中止QQ当前/准备播放，不中断Apple；关闭Apple也会取消尚未起播的Apple请求。原生在途操作等待和真正停播等待均有期限，超时返回失败，不视为停止成功或放行新来源。

Apple 暂停后的 toggle 调用原生 resume；next/previous 使用原生队列，进度来自实际播放器。Stop 显示 idle，Pause 显示 paused，不照搬SDK停止后的paused枚举。原生异步完成后无条件重申当下终止意图，正常状态采样不反复发 Pause，以允许媒体键恢复；媒体键和失活来源上的外部操作仍需最终包实测。Apple能力为seek=true、volume=false、fade=false，不修改系统全局音量。

## 验证记录

原生运行时代码提交 `2e8951a86c18f20c5937d269451522150d7e2784` 的本地 Preview 测试为34通过、2个既有设备/账号测试忽略；同提交的远端 macos-checks 通过（运行37513812058，包含普通及Preview配置）。覆盖来源切换双屏障、停止取代在途播放后的迟到回调、全局确认不越过未完成命令、Rust排空确认、失败同步保留完整缓存、出现条目顺序持久化、来源独立关闭及既有QQ/本地回归。

实际组合提交 `04a8baa1189774cab709bce20756ed50849edac4` = 上述原生提交 + #32 前端 `528710f8dbb1ed082f9c4b00ff111730c17b1c76`，树 `40070b6666bd993c42ec81c6a4bc03f1c2c7bb41`。正常 Preview 发布构建成功，严格签名校验通过，MusicKit/Swift 链接已核对。该包为 ad-hoc 签名，未公证；存在既有测试构造器未使用警告，不宣称零警告。以下验收绑定这个包，后续仅文档提交不改变其运行时代码。

- 可执行文件 SHA256：`40c9e6548212627c5a0941514a900e17973dd4e3a9bff7761111affa77699a78`。
- ZIP SHA256：`308edb592ac6a6c52b145c1cc5e535588b43c9a2175952cd1002d209e64afc6b`。
- Info.plist SHA256：`3c0d8fe653ca1f1461d972230ca35bf887aa93815017e92619eaf04ada8de04f`。
- 实际运行进程路径指向该组合包；`com.rhine.music.preview`、普通数据目录、原 Keychain 服务及隔离 WebKit 身份匹配。最低系统版本为14.0，低版本系统未经运行验收。

所有操作通过正常应用 GUI 完成。实际 Preview 点击允许访问后状态转为 authorized；操作者未观察或点击系统授权按钮，不能由此推断没有系统弹窗或用户动作。同步得到2份歌单、35个出现条目（25+10），两份完整，均 browserPlayable=false；QQ原24份/562条及已有连接保留。缓存文件正常写入。此处仅记录计数，不提交账户库或凭据。

| 检查 | 实际观察 |
| --- | --- |
| 个人歌单中段起播 | 从第10首开始，进度到20.8秒 |
| 暂停与继续 | 暂停29.9秒保持，恢复后30.4秒，没有回到开头 |
| 曲尾自动续播 | 暂停定位至299.7/311.4秒后继续，无手动Next，自动到第11首并到22.7秒 |
| 上一首/下一首 | 超过3秒时上一首重置当前；起点再次上一首回第10首；Next到第11首 |
| Apple→QQ | QQ播放进度到79.9秒 |
| QQ→Apple | Apple播放进度到16.5秒 |
| Stop与快速Play→Stop | 停止后off/0；快速操作后多次后读仍off/0 |
| 能力与偏好 | Apple音量/淡入淡出禁用且有解释；原歌曲、背景音和音效偏好保留 |

组合本地证据位于 integration worktree 的忽略目录 `.local/acceptance-04a8baa/`，包含包manifest、运行身份、带限制说明的报告和哈希索引。报告 SHA256 `5431c3a21ac3f4618c399bea98763e813f2ee2f8bcfe55304310ac4093efd38e`。报告来自 CUA 对话观察的整理，**不是原生原始时序遥测**。

边界：本轮没有主观听音确认或波形/亚帧重叠采样，不能声称任何瞬间绝无重叠；曲尾使用定位，不能写成完整整首无缝播放。GUI快停未证明恰好截中原生在途窗口，相关确定性单元测试与原型证据独立记录。媒体键、拒绝/撤销授权、重复目标实机路径、其他账户和系统未测。本轮未做本地文件夹GUI切换；QQ使用相同Rust播放引擎，屏障另有单元测试。重启后缓存起播留给总控独立抽查，当前不宣称已覆盖。来源实机自测结束时应用保持打开且歌曲停止在0，设备窗口已交还总控和性能任务。
