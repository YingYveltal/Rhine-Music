# Issue #7：默认 QQ 路径补验

状态（2026-10-05）：Q1、Q2、Q3、Q2b 已在下述隔离 harness 中执行，观察样本未发现封面回归；Q4 部分执行，Q5/Q6 未完成。Mac 锁屏中断 GUI，测试 app 已退出并释放时段，不能据此批准整体验收或合入。生产源码为 PR #10 的 `0d3e913ea82a689fc68dbe6b5454f9d08d4b5586`；后续仅增加本方案和合成 fixture 生成器。本补验只覆盖总控指出的封面缓存、事件目标及普通设置变化，不开启新优化工作。

## 无需设备的证据及其边界

| 改动 | 当前代码证据 | 仍需观察 |
| --- | --- | --- |
| `cover-atlas.ts` 的绘制封面缓存 | `select` 用记录键复用 CanvasTexture，`snapshot` 共享纹理并增加引用；淘汰保护出场纹理；异步图片完成有 generation/disposed 检查。原 3 个缓存测试覆盖复用、容量淘汰/重复释放、曲库重置 | 这些测试使用模拟 Canvas，没有证明首次加载、快速反向切换和出场动画的封面像素正确；不能用版本号/引用计数代替视觉验收 |
| `scene.ts` 事件目标 canvas→container | `.three-scene` 为绝对定位容器，唯一绘制 canvas 的 CSS 为 100% 宽高；原 music CSS 已设置 canvas `touch-action:none`，本次向容器增加同值；按钮、设置面板为容器兄弟节点。左键过滤、6 px 点击阈值、取消/失去捕获处理逻辑保留 | CSS 和节点结构不能证明实际命中、capture、鼠标移出、滚动或触摸表现；应观察边界和遮挡情况下的输入 |
| 普通设置增加实验项 | `preferences.smoothMotion=false` 后合并旧偏好；没有该字段的旧设置保持 false。设置的 change handler 持久化布尔值，启动时传入 `setSmoothMotion`。默认 Metal 未注册、透射深度/后处理开关为 false | 新安装与旧偏好兼容、开关保存/重启/恢复原分辨率、设置面板操作不透传、可见标签及文字均需 GUI 验证 |
| 测试数据边界 | 内置 demo-sun/demo-night/demo-mountain 仅引用 public/demo-covers 的方/竖/横图，曲目数组为空，不写真实索引。`Qq::new` 仅存在 remember-connection 标记时读取钥匙串 | 新建空测试目录必须无此标记；启动时显式移除 QQ 会话/密钥环境变量；不点击登录、导入、保存连接或真实音乐目录 |

已有 MotionResolution 3 项单测、封面缓存 3 项单测及来源哈希只作为前置证据。它们不覆盖下面的 GUI 验收。

## 原定构建与环境隔离（实际差异见结果）

仅在总控交接时段后执行。默认 QQ 源码直接使用 `npm run tauri -- build --bundles app --config <测试配置>` 构建，不经过 build-metal.py，不注入 Metal 模块、透明窗口或额外渲染依赖。测试配置仅覆盖应用 identifier 和主窗口 dataStoreIdentifier，其他 QQ 窗口/画质配置保留。记录两项测试隔离差异，不能将测试应用身份说成正式 QQ 身份。

`MUSIC_NATIVE_DATA_DIR` 指向本 worktree 新建的 `.local/default-qq-qa/data`；launch 环境移除 RHINE_QQ_SESSION/QQMUSIC_API_KEY；不复制旧数据。独立 WKWebView dataStoreIdentifier 用于 fresh localStorage，避免误用真实 QQ 偏好。本机 Tauri 配置源码表明 macOS 14+ 支持 16 字节 datastore 标识，本机 26.3.1 可用；dataDirectory 在 macOS 不生效，因此不用它代替。

Cargo target/cache 均在本 worktree 内。只启动自己的测试 app，不关闭其他用户应用。先观察空库和默认设置，再关闭测试 app 内的 BGM/音效用于后续静音检查；这两项声音偏好不影响渲染验收。不播放真实音频。先使用内置演示卡片执行 Q1–Q3，再从普通音乐库设置导入下述 16 专辑的合成 fixture，补做容量淘汰及 Q4–Q6。

## 具体复验矩阵

| 编号 | 操作 | 合格条件与证据 |
| --- | --- | --- |
| Q1 全新启动 | 空测试目录+新 WebKit store 启动；查看普通设置，再进入演示封面 | 空库无私人信息；原版画质、减少动态效果关闭、流畅优先未选中；页脚无 METAL 状态。记录 app SHA、窗口/画布尺寸、设置截图和数据目录只含测试文件 |
| Q2 封面热/冷切换 | 暖昼、深夜分别观察三张方/竖/横封面；首次依次打开；详情中快速正向和反向切换各 20 次；包含 A→B→A 和未落稳时返回/重新打开 | 旧盒出场保留旧图，新盒/标题对应同一专辑；无错误占位、空白、拉伸、释放后黑块；最终卡片与标题匹配。保留切换前/中/后视觉证据；仅静态截图不足以排除瞬态闪烁时明确标注 |
| Q3 高频回放补充 | 默认开关全关，执行现有 Ctrl+Option+J 的 28 秒/111 次事件处理器回放；同步观察演示封面，检查最终 demo-sun/detail/无待处理选择 | 无运行错误、最终状态正确，记录实际执行数和无效/打断标记；这是合成处理器回放，不冒充物理键盘/鼠标 8 Hz 或性能提升证据 |
| Q2b 超容量视觉淘汰 | 通过普通音乐文件夹入口扫描 16 专辑 fixture；按数字顺序逐张进入详情并停稳 1→16→1，再逆序 16→1→16；最后快速执行 01→16→01、12→13→12，各重复 10 次，包含旧卡未退完时反向输入 | 标题数字、封面大数字、颜色/比例始终匹配；01 再次加载无黑图、错图、消失、翻转或裁切；出场卡片保留其自己的数字。分别记录单次冷读、容量淘汰后的重访及快速交叠，不把仅浏览架子算成访问完整封面缓存 |
| Q4 原生可用指针 | 用本机 UI 自动化产生鼠标移动/左键/拖拽/释放；在卡片中心和容器边界选择；详情左右拖动后移出再释放；右键不触发专辑选择；再次进入后能继续操作 | 命中专辑正确；拖动不误点击；释放/移出后不粘滞；hover 能清除；返回/设置/遮挡 UI 正常，点击设置控件不会透传到卡片。分别记录实际可执行的输入类型 |
| Q5 设置与恢复 | 打开/关闭设置、切换主题及音效；打开流畅优先，在大于 1.4M 像素的画布中运动→停稳；关闭后再次运动；退出并重启核对开关；再把渲染实验开关恢复关闭 | 默认关闭且文本明确为实验；开启只影响三维画布，文字布局不变；停稳恢复、关闭始终 scale=1，原画质配置值不被改写；保存/重启状态正确；无意外 Metal 激活 |
| Q6 旧偏好兼容 | 仅在测试 store 中保存无 smoothMotion 字段的正常旧偏好并重启；若没有可安全访问该 store 的工具，则改记未覆盖 | 缺字段时开关仍未选中、普通偏好保留。不能清理或修改正式 QQ WebKit 存储来制造此用例 |

本机自动化若只提供鼠标/键盘，不将 DOM 合成 PointerEvent 标为真实触摸。物理触摸屏、多指触摸/触控板手势、压力笔输入均按实际能力记录；不可执行的类型明确为未覆盖。16 专辑的 Q2b 必须执行；若普通扫描入口出现阻塞，记录实际错误和最小补验办法，不因内置 demo 仅 3 张而跳过。

## 合成 fixture 与静态自检

生成命令（输出必须是不存在的新目录）：

```sh
python3 scripts/create-rendering-fixture.py .local/default-qq-qa/fixture-16
```

生成器不联网、不读取用户媒体，不修改生产代码。输出 16 个 `QA 01 … QA 16` 专辑文件夹：各一张原创编号 PNG 和一秒全零 PCM WAV。封面使用大号 01–16 数字、16 种色相、方/竖/横三种比例、非对称角标和二进制编号条，便于辨认错图/翻转/裁切。WAV 只供既有扫描器识别专辑，不需要播放。生成的图片、音频、manifest 均留在忽略的 `.local`；只提交生成器。

使用普通“音乐库 → 音乐文件夹 → 保存目录并扫描”入口，填入本机 fixture 绝对路径。子文件夹自然分组，`cover.png` 由既有扫描器识别，专辑名从目录得出。标题 01–16 使用零填充；验证后端共 16 个本地专辑/曲目及 native asset 封面路径指向 fixture，再开始 Q2b。无需伪造索引、替换 renderer 或添加生产测试后门。

准备阶段已生成 fixture 并执行无设备自检：16 个 PNG 哈希互不相同，逐块 CRC/解压像素长度及尺寸正确，包含三种比例；16 个 WAV 均为单声道 16 bit/16 kHz/一秒且全部样本为零。静态读取确认子文件夹专辑命名、cover.png 识别与根目录边界符合当前 `library.rs` / `snapshot` 路径。随后已通过普通 UI 运行真实扫描器并完成 Q2b，结果见下文。

每次启动前检查专用数据目录的 `qq/remember-connection` 不存在，launch 显式移除 `RHINE_QQ_SESSION` 和 `QQMUSIC_API_KEY`。应用 identifier 不能隔离 QQ 的固定 Keychain service；不触发保存连接或本机导入。若标记意外存在，停止该次启动并调查，不读取钥匙串或删除未知标记后继续。

## 记录和通过判据

每项记录实际动作、结果、版本、证据文件名及未覆盖项；测试截图/捕获留在本 worktree 的忽略目录，不上传任何私人媒体。若工具不支持连续画面或输入观测，记录其限度，不能把“未看见”写成“完全没有”。最终只修复复验中确认的回归，重新跑受影响检查；主线合入仍交由总控决定。没有 GPU 性能比较、触摸设备验收或用户体验验收的替代结论。

## 2026-10-05 实际结果

这是默认 WebGL 的回归补验，不是渲染方案性能对比。未确认新的生产回归，因此没有为本次补验修改生产代码。PR 继续保持 draft。

### 构建绑定和有效隔离

- 源码提交：`11a6aa7d188a1d01a167377a5f13772f5bc44b01`；后续结果文档提交不改变被测生产实现。
- 环境：macOS 26.3.1 arm64、Node 24.14.0、Cargo 1.93.1。测试窗口 1280×820，三维画布 1280×788、DPR 1。
- 应用主程序 SHA-256：`55da51edade6d0875ca02a86afe2b6f786000438dae6f8ec4cbfca41f8f578de`。
- QA identifier：`com.rhine.music.issue7-default-qa`；实际 WKWebView store UUID 读回 `C886D1F6-8483-4DF9-9BD0-C8688AACEE05`，与请求值一致。
- QA main.rs SHA-256：`5515e6da48039e59a8f21a79adfd40d00d96dafe11585bf7924b7d8d4daf8c04`。有效源码差异见 [harness patch](rendering-qa-harness.patch)，仅应用于本地隔离副本，未应用于生产 main.rs。

原定 JSON dataStoreIdentifier 在本机 Tauri codegen 中产生 `Vec<u8>` / `[u8;16]` 类型错误；改为修改运行期配置后，又发现自动窗口构建路径未传递该字段。因此前一次默认 store 的初步 Q1 观察排除在验收之外。

最终 harness 关闭自动建窗，在 setup 开头通过原配置的 `WebviewWindowBuilder::from_config` 显式调用 `data_store_identifier` 建窗，并用只读原生 getter 验证实际 store。主窗口配置、页面、IPC、插件和渲染代码保留，窗口创建时点有上述差异；测试包不能说成与生产包完全相同。独立 release 打包及严格 codesign 验证通过。harness 不引入 Metal 模块或新依赖。

以下本机相对路径均位于 `.local/default-qq-qa/`，保留供续验、不上传：

- app：`target/release/bundle/macos/Rhine Music QQ.app`
- 后端数据：`data-final/`；证据：`evidence-final/`
- 元数据：`build-source-final.json`；构建日志：`build-final.log`；原始差异：`harness-final.diff`
- 运行日志：`runtime-final.log`，仅记录匹配的 `RHINE_QA_STORE`，未发现 runtime-error/runtime-rejection 报告。

最终启动前数据目录为空，启动环境移除 QQ 会话/密钥变量；结束后仍无 `qq/remember-connection`。未执行 QQ 连接、保存连接、账号导入或真实媒体播放。仅经普通扫描入口导入下述合成 fixture。前一次无效隔离尝试的 `data/`、`evidence/`、`runtime.log` 仍保留，但不作为本轮通过证据。

### 逐项结果与证据边界

证据文件名以下均相对于 `evidence-final/`，除另有说明。

| 项目 | 实际结果 | 证据与限制 |
| --- | --- | --- |
| Q1 | 通过已执行观察：空库、原版画质、渲染 100%、DPR 上限 1.5、减少动态效果及流畅优先关闭、无 Metal 状态 | `q1-empty.png`、`q1-default-settings.txt`、`q1-default-controls.png`。实际 DPR 为 1；后续仅将 BGM/音效关闭，画质不变 |
| Q2 | 两主题各观察三种比例封面，分别执行 20 次 Down + 20 次 Up；抽查中间态及终态，未观察到错图、黑块或比例错误 | `q2-bursts-contact.png`、`q2-*-settled.png`、`q2-*-native-keys.json` 及原始逐段截图。输入为 CUA 原生键盘事件，并非真人操作或保证 8 Hz；截图抽样不能排除所有瞬态问题 |
| Q3 | 111 次处理器回放完成；`valid=true`、无 interruptions；最终 `demo-sun/detail/pending=false` | 报告 `data-final/benchmarks/20261005-123439-optimized-stress-night.json`；`q3-capture-times.json`、`q3-sampled-contact.png`、`q3-final.png`。719 张截图覆盖回放中后段及终态，不覆盖完整开头。截图采集干扰负载，不能作 FPS、GPU 提速或物理输入至显示延迟结论 |
| Q2b | 普通扫描器得到 16 专辑/16 曲目；1→16→1 与逆向 16→1→16 逐张访问；01→16→01 和 12→13→12 各快速反向 10 轮。抽查封面数字、比例及出场图未见错误 | `q2b-forward-contact.png`、`q2b-reverse-contact.png`、`q2b-transitions-contact.png` 及原始截图/AX、`q2b-observations.json`（38 条标题匹配记录）。超过配置的 12 张缓存容量进行重访，但未采集 16 张路径结束时的运行期缓存计数；不能宣称测得该路径命中率或内存收益。部分截图标题仍在正常字母进入动画中 |
| Q4 | 部分：QA12 详情内 `[330,430]→[530,440]` 拖动可旋转卡片，未误换专辑 | `q4-drag-before.png`、`q4-drag-right.png`。第二次拟拖到画布外 `[1225,16]` 时工具因 Mac 锁屏失败，不能计作执行成功或释放通过；右键、边界、移出释放恢复、面板透传仍未验 |
| Q5 | 未完成：仅观察默认关闭、设置文字，执行常规开关面板/主题/声音操作 | 未开启流畅优先，未在大于 1.4M 像素画布测试运动/停稳/关闭恢复，也未测试保存后重启；本次画布低于触发阈值 |
| Q6 | 未执行 | 未在隔离 store 中构造缺 smoothMotion 字段的旧偏好并重启；不得拿默认 fresh 状态代替 |

Q3 的实际记录还确认 `nativeMetal.active=false`、`postFusionEnabled=false`、`transmissionDepthEnabled=false`、`smoothMotion.enabled=false/endingScale=1`，仅出现 `1280x788` 采样分辨率。回放的 `valid` 字段只描述内部有效性检查，不证明物理输入、真实显示帧或性能比较有效。

纯 hover/leave 的无按键移动、多指触控板、触摸屏和压力笔均无本轮验证；没有用 DOM 合成事件代替硬件。局部未观察到回归不等于用户已接受流畅度。

### 阻塞与续验

Q4 第二次拖动工具明确返回 Mac 已锁定且自动解锁失败；这是真实 GUI 前置条件。已停止操作、核对自身进程后退出 QA app（原 PID 6532，后续确认不在运行），设备/构建时段交还总控。保留最终 app、store、后端数据及全部证据，不清除数据、不重建生产实现。

等待总控集中安排解锁并明确交回 GUI 时段后，继续 Q4 的画布外释放/边界/遮挡，再在大画布执行 Q5 的开关与重启、Q6 的隔离旧偏好兼容。当前偏好为深夜、声音关闭、流畅优先关闭、减少动态效果关闭；后端仅含合成 fixture。Q4–Q6 完成前不批准整体验收或合入。本轮不新增优化、不形成 Metal 优于 WebGL 或光追优于光栅化的结论。

选定本地证据的相对路径、SHA-256 及文件大小见 [evidence manifest](rendering-qa-evidence.json)。该清单只绑定留在本机的证据，不含截图、媒体、原始账户日志或生成二进制。
