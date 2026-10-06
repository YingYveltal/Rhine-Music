# Music.app 本机控制探针（Issue #6）

独立 Cocoa / Scripting Bridge 应用，固定身份 `com.rhine.music.appleprobe`。不需要 MusicKit 开发者令牌；Music.app 负责实际播放，双方共享状态和队列。只读其可见资料库，不能当作 Apple Music 全站搜索。

在 macOS 14+、安装 Apple Command Line Tools 的机器运行：

```sh
python3 probes/apple-music/build.py
```

打开 `.local/apple-probe/Rhine Apple Music Probe.app`，点击「连接并读取歌单」。首次自动化确认交用户正常操作；拒绝时显示错误，不能绕过或重置权限。选择一个已有歌单、读取其曲目，选择曲目后使用播放控制。使用「播放所选歌单」通过公开 play(playlist) 建立 Music.app 共享队列；单曲点播是独立操作，不承诺保留待播列表。最多读取所选歌单的前 250 首，保留接口返回顺序；不修改随机播放、循环、系统音量或用户资料库。

读取 `cloud status=subscription (kSub)` 与 `kind` 用于区分订阅来源，不能仅凭曲目名称、命令成功就断言订阅播放或音频输出。观察实际进度、Music.app 状态，并由用户确认实际出声。`play` 用于恢复暂停；字典中的 `resume` 只退出快进/倒退，不能替代暂停恢复。

应用以 ad-hoc 签名及 Hardened Runtime/Apple Events entitlement 构建，不购买会员、不公证。证据和私人曲目 metadata 仅写入 ignored `.local/apple-probe/evidence/events.jsonl`，不上传仓库。manifest 记录源码提交/脏状态、源码和可执行哈希；验收仅绑定具体包。不能并行操作其他音乐源或在运行时重建覆盖此探针。

接口依据：本机 `/System/Applications/Music.app/Contents/Resources/com.apple.Music.sdef`；Apple [SBApplication](https://developer.apple.com/documentation/scriptingbridge/sbapplication)、[自动化用途说明](https://developer.apple.com/documentation/bundleresources/information-property-list/nsappleeventsusagedescription)和 [Apple Events entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.automation.apple-events)。手写声明只覆盖实际读取及播放控制的公开字典方法，没有外部脚本执行或 UI 自动化代码。

9b3839b 原型已在正常用户允许自动化后读到订阅曲目并推进播放，暂停/恢复/定位/停止通过。单曲播放不会自动建立共享歌单队列；新增加的歌单播放入口尚待本轮验证。实际出声仍须用户确认。完整结论以绑定包哈希的后续验收记录为准。
