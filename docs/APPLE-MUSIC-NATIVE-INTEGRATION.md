# Apple Music 原生接入（Issue #30）

本任务把 MusicKit 原型接到现有 Rust/Tauri 核心，前端由 #31 独立负责。当前是待组合验收的实现，不是已验收的 Preview 成品。

`src-tauri/native/AppleMusic.swift` 通过异步 C 回调向 Rust 返回 JSON，所有 MusicKit 对象与队列操作留在 MainActor。Rust 的调用线程等待回调时不阻塞应用主线程。构建脚本编译最低 macOS12 的静态桥，MusicKit 库/播放器入口受 macOS14 availability 检查保护；用途说明合入 Info.plist，不改 bundle identity、Keychain 或 WebKit 数据目录。低系统运行仍需实际机器验收。

## 数据与接口

- `apple_request({operation,body})`：status、authorize、sync、enable；进入界面只 status，authorize 仅在明确点击后请求，已拒绝或受限不反复弹窗。首次没有保存偏好时 enabled=true，但不自动授权或读库；明确关闭后保留false，避免授权同步成功却看不到卡片。
- 同步从个人 Playlist 分页读取 entries 和 tracks，核对数量和完整顺序，保存不同出现位置，不按曲名去重。原型10/250限制已移除。分页/顺序失败保留上次完整缓存，显示 job.error，不提交半份曲库。
- 缓存位于当前应用数据目录的 `apple/library.json`，封面单独保存在 `apple/covers`。关闭来源保留缓存与偏好；重新启动可显示缓存。点播时若原生对象尚未载入，重新读库恢复引用；快照发生变化则明确要求重新同步，不猜测匹配。
- `/api/library` 增加 apple 状态及 source=apple、kind=playlist 卡片，曲目ID是带快照版本和位置的不透明出现条目ID。同样的条目序列重复同步ID稳定；平台条目或顺序变化后生成新版本，旧ID需要重新同步后点播。browserPlayable=false，原生支持类型独立判断，缺少playParameters不阻断MusicKit真实尝试。不会伪造本地路径、编码器或音频下载地址；封面使用现有本地资源协议。
- 原生队列使用已验证的 typed Track 构造器。所选 Track 的平台ID在请求队列内重复时，目前显式返回“无法唯一定位”，避免 SDK startingAt 选错同曲位置。重复项仍完整展示，自动队列映射按全序条目和 queueGeneration 维护；重复项可用性需组合验收。

示例位于 [API样例](apple-native-api-examples.json)，均为人工构造的脱敏契约示例，不是真实账户或验收记录。

## 统一播放与来源互斥

`player_command` 保留单一递增receipt。`player_state.appliedCommand` 只有在对应操作完成、明确失败或被已完成的后续命令替代后推进；辅助设置的完成不能越过仍在途的播放命令。queueGeneration 只在本后端标识队列，前端跨来源以receipt确认。

切换到 Apple 时，Rust先使旧QQ准备任务失效，停止歌曲并挂起背景音，等待实际 sink 排空；同时等待MusicKit旧在途操作结束并确认停止，再启动新队列。切回Rust时先完成MusicKit停止屏障，再恢复Rust背景音策略并起播。挂起不改用户音量/背景音偏好。快速 Stop 可以使待启动的队列过期；旧回调不得恢复新状态。QQ退出只中止QQ当前/准备播放，不中断Apple；关闭Apple也会取消尚未起播的Apple请求。原生在途操作等待和真正停播等待均有期限，超时返回失败，不视为停止成功或放行新来源。

Apple 暂停后的 toggle 调用原生 resume；next/previous 使用原生队列，进度来自实际播放器。Stop 显示 idle，Pause 显示 paused，不照搬SDK停止后的paused枚举。原生异步完成后无条件重申当下终止意图，正常状态采样不反复发 Pause，以允许媒体键恢复；媒体键和失活来源上的外部操作仍需最终包实测。Apple能力为seek=true、volume=false、fade=false，不修改系统全局音量。

## 验证记录

首轮 Swift 静态编译及 Rust 测试链接通过，后端29通过、2个既有设备/账号测试忽略；这一阶段未打开播放设备。已覆盖来源切换双屏障、停止取代在途播放后迟到回调、全局确认不越过未完成命令、Rust真正排空前不得确认以及既有QQ/本地回归。

`bb76a5ba43aba980ecf0e40b5b85e6966ff7b9e7` 的远端 macos-checks 已通过（运行37513135034，包含普通及Preview测试配置），覆盖新增缓存和来源独立性回归。真实产品可执行文件的Swift链接、授权、个人歌单中段续播、跨来源切换、拒绝/撤销、媒体键和Preview身份仍待设备窗口及与#31组合验收。没有把上述测试当作最终包出声证据。构建/运行需由总控与性能任务串行安排，当前保留旧Preview与其登录数据。
