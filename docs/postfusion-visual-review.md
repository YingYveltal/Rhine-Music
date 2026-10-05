# Issue #18：第一关画面采集（部分验收）

2026-10-05 15:55–16:03 UTC，在固定 QA 包取得 13 份 A1/A2/B/A3 报告。所有报告 A/A 与恢复最大误差为 0，A/B 最大通道差为 1/255，无超过 2/255 的通道。**尚未完整通过第一关**：快速换片中旧、新封面同时可辨的重叠状态未明确捕到。未执行任何计时性能区块，也没有帧率收益结论；正式 postFusion 默认仍关闭。

## 固定包与隔离

- 源码 `8f09303ee767ff9eeb7e206a438745f3fa7bab0f`，基线 main `ff30a9a6bd82445d926b26c0ed6ac59a9d0e6d35`。后续主线推进未混入本包。
- QA-only 构建差异为已审 `postfusion-qa-harness.patch` 和独立配置。总控逐一核对 tracked blob，仅 main.rs 不同，并从原 main 独立应用补丁，确认与构建副本逐字节相同。
- QA main SHA-256：`905190355ebe0752ebe3f1db28e7db88230fe535a2b8738960fb3b40ae1aecde`。
- 最终 ZIP SHA-256：`5973873a9aba168532deaa4aeaa3876be2fff4466423347120ea44745c5dee3a`；可执行文件 SHA-256：`318267244e59b38a67bbcf5f062dd68a5f20d05244111a6cf92e193c282babb9`。codesign 严格校验通过，未公证/发布/替换正式应用。
- 身份 `com.rhine.music.issue18-postfusion-qa`；启动日志实际回读 store `D075F93A-B745-4856-B372-9B81E7C54173`。PID 37981，独立 `data-final`，启动移除 RHINE_QQ_SESSION/QQMUSIC_API_KEY；只扫描新合成 fixture，16 专辑/16 静音曲目，按专辑名两列，未连接真实 QQ。
- 原始画质、smoothMotion/Metal/透射深度关闭，音效/BGM 关闭。每份报告均恢复 postFusion=false。

原图、原始 JSON、包、启动日志及无效首包都留在本 worktree `.local/postfusion-qa/`。仓库只保存本说明和 [证据索引](postfusion-visual-evidence.json)，其中绑定报告、四张 PNG 及关键本地材料的 hash，不上传原图或原始报告。

## 实际条件与覆盖边界

所有保存报告的 viewport 与实际 canvas 都为 1920×1080，物理 DPR=1，诊断 DPR 覆盖=null。Mac mini / M4 / 16 GB，macOS 26.3.1(a)，采集后系统读数为 AC 电源。系统列出 DELL P2715Q（物理 3840×2160，逻辑 1920×1080@60 Hz）和 VG248（1920×1080@144 Hz）；应用报告未绑定物理 display ID，不能断言窗口在哪块屏幕或把任一刷新率当作测量条件。

这与原性能协议 DPR2/1280×788/buffer1920×1182 不同。**仅冻结为本轮画面条件，未修改性能协议。** 是否补画面或重新制定统一性能条件，由总控在任何计时运行前决定。

下表以原始报告文件中的 UTC 时分秒作简短索引；完整文件名、phase、pose 与四帧 hash 见证据索引。

| 主题 | 报告 | 实际内容与局限 |
| --- | --- | --- |
| 暖昼 | 155542 | archive，detail=0，QA01 架子、玻璃与前后景 |
| 暖昼 | 155642、160224 | placing，detail≈0.000200 / 0.147832；后者明确为 QA11 打开中的镜头与抬升姿态 |
| 暖昼 | 155656 | presented，detail=1，QA01 详情 |
| 暖昼 | 155717 | 与 155656 同 pose/误差，不额外算新姿态 |
| 暖昼 | 155815、155904 | 快速换片后 placing、detail=1，分别可辨 QA04/QA08；未证明旧、新封面同时重叠可辨 |
| 暖昼 | 160252 | returning-array，detail≈0.983322，QA11 已开始回落，属于返回初段，非完整返回轨迹 |
| 深夜 | 160018 | archive，detail=0，QA08 架子 |
| 深夜 | 160041 | placing，detail≈0.125844，QA08 打开中的镜头与抬升姿态 |
| 深夜 | 160106 | presented，detail=1，QA08 详情 |
| 深夜 | 160124 | 快速换片后 placing、detail=1，可辨 QA11；未证明旧、新封面同时重叠可辨 |
| 深夜 | 160156 | returning-array，detail≈0.993103，QA11 返回初段 |

执行者查看两主题架子、打开与返回 A2/B 原图及换片代表图，当前已查看样本未见颜色、玻璃高光、景深边缘或封面身份差异。总控另直接查看部分原图，仍须完成独立复核；不能以 13 个 pixelGate=true 替代重叠姿态缺项，也不能当作所有可能姿态均通过。

## 换片状态的只读源码核对

固定源码确实保留旧、新两个独立盒体，并非只把单个前景模型归位、换贴图再展示。`music-presentation.ts:160` 的 beginDetailSwitch 先等待菜单退出，再经 commitDetailSwitch（179）调用 switchDetail；`music-app.ts:363` 接至 commitSelection，702 行调用 `scene.switchMusicAlbum`，因而人工按键后的最初一段仍可能只是旧详情。

`scene.ts:872` 的 switchMusicAlbum 保持详情并进入 select；893 行在循环模式、切换实际 cell、已载入且旧 lift>0.0001 时 clone 旧模型，897 行保存旧封面快照，915 行加入场景、916 行加入 outgoing，随后清零新选择的 lift，并在954行切换当前封面。`cover-atlas.ts:390` 的 snapshot 克隆旧材质并保留旧的不可变封面 texture，不随新选择覆盖。普通详情模式在 `scene.ts:684` 开启 looping。

`scene.ts:1309` 起让新盒使用独立 lift 弹簧升起；1369 行起让旧 outgoing 盒独立下降，1397 行到接近零才移除。因此普通非减少动态效果的详情换片中存在新盒升、旧盒降的几何共存区间。例外也有明确条件：邻近旧盒尚在旋转对齐时新盒可能先等待；减少动态效果会直接结束旧盒回落；切回仍在 outgoing 中的同 cell 会复用其状态（934 行）。

**几何共存不等于当前相机下两个封面都清楚可见**：遮挡、景深、行列位移和采样时刻都会影响可辨性。本轮视觉报告只有主模型/相机 pose，没有 outgoing 的身份/位置快照；三个 placing 样本无法单靠该字段证明双前景可辨。原图中的普通架子重叠封面单独成立，但不替代快速换片重叠项。此次只读核对未新增工具、修改动画或再启动采样。

## 异常与资源释放

首个无效包：在受 Git 忽略的构建子目录中运行 git apply，命令成功退出但跳过了补丁。首次 PID 36011 未输出 store 回读，立即退出。其独立 `data` 中只有空曲库配置/索引/分类文件和空 QQ 缓存目录，roots 为空、onlineEnabled=false，无 remember-connection 或 QQ 文件；未导入 fixture、未采集画面或性能，未读生产目录。首包 ZIP/启动记录保留并排除。改用文件级 patch，完整核对真实源差异后重建，总控复核后才启动最终包。

最终包导入后普通窗口一度停留于显示音乐库阶段，切全屏后继续；原因未定位，未将其计入性能。一次 Escape 同时返回专辑架并退出 macOS 全屏，未产生报告；仅恢复原全屏后继续，后续改点产品返回按钮。未伪造 DPR 或搬窗口凑性能条件。

PID 37981 已正常退出，并于 16:04 UTC 用进程查询确认，不保留后台 QA。完整前端编译、相关渲染/QQ 检查、check:music/content/viewport 与 release 打包通过；这些结果不代表真实账户或性能验收。没有实现 PR、默认启用、合并或发布动作。
