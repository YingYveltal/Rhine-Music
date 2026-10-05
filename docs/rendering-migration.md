# Rendering migration — Issue #7

本 PR 将冻结的既有渲染成果迁入 QQ 基线，供审查与后续复验；后续 Q5 补验增加了有证据的闲置分辨率恢复修复（见文末），没有将历史性能结果当作此提交的验收。起点为 `7ca1e5f653d646a0ef8cb068565771306509c805`，需求见 [Issue #7](https://github.com/YingYveltal/Rhine-Music/issues/7)。

## 来源与迁移边界

`rendering-source.json` 列出迁入的 32 个源文件及冻结快照 SHA-256（2026-10-05）。首次迁入除构建适配和 README 修订外保持快照内容；后续 Q5 对三个文件的有意修正单列于清单 intentionalPostMigrationChanges，冻结源哈希仍保留。QQ 基线已包含 v3/v4 的实例裁剪、图集局部上传、光照与景深优化；相同文件没有重复修改。

- 前端：12 张绘制封面 LRU 缓存、出场卡片纹理共享、Metal 控制器与增量资源录制、帧完成测量、可选运动分辨率、全背面深度实验及四组小测试。
- Rust：`src-tauri/src/metal_bridge.rs`，以及完整 `metal-lab` 源码、Cargo.lock、许可证、Python 翻译参考和 Rust 翻译入口。
- 构建：`scripts/build-metal.py` 只在本 worktree 的 `releases/<variant>/.build` 注入 Metal 命令、依赖、透明窗口和工具资源。target 位于同 variant 的 `.target`，Cargo/npm 缓存位于本 worktree `.local/rendering-build`，覆盖调用者共享缓存环境。依赖必须安装在当前 worktree，不借用旧工程。
- 保持 QQ 的 `main.rs`、Cargo.toml/lock、tauri.conf.json、native.ts、播放、曲库、QQ 连接与登录代码不变。`music-app.ts` 的增量仅为渲染设置/诊断/帧率显示。没有用生成的 `.build` 覆盖源代码。
- 未迁入旧 `.gitignore`、根 README、旧默认应用配置、`build-qq.py`、Metal build.txt/run.txt、生成着色器、捕获帧、个人音频/封面、私密日志及二进制。旧 QQ 构建脚本由基线工作负责；本任务只保留 Metal/v2 构建入口。

## 默认行为与实验入口

默认 QQ 应用继续使用 WKWebView + Three.js/WebGL，原画质设置不变；Rust 负责原有数据/播放任务。Metal 命令只在独立实验构建中注册。封面缓存默认启用；后处理合并、透射深度预计算、“流畅优先（实验）”均默认关闭。保存过的用户设置仍会读取。

| 入口 | 行为与限制 |
| --- | --- |
| `python3 scripts/build-metal.py` | 生成 Metal Preview；启动仍为 WebGL；不会启动应用或访问 QQ 会话 |
| `RHINE_PREVIEW_VARIANT=v2 python3 scripts/build-metal.py` | 同一渲染源码的独立 v2 应用名称/identifier，不代表另一套已验证算法 |
| Ctrl+Option+N | 实验构建中准备/切换 Metal，首轮会本地捕获当前场景、编译原 GLSL，完成前应等待；普通 QQ 构建无 Metal 命令 |
| 设置 → 流畅优先（实验） | 较大画布运动时三维长宽降为 2/3，停稳 850 ms 后恢复。文字仍为原分辨率；默认关闭，属于画质取舍，不能算同画质提速 |
| Ctrl+Option+T / F | 分别切换全背面深度实验/后处理合并；均无已证实的端到端收益 |
| Ctrl+Option+J | 28 秒、111 次交互回放；应同时看长帧、延迟、最终状态及有效性，而非只看平均 FPS |
| Ctrl+Option+H | Metal 中捕获下一张 GPU 长帧并进行扰动式分段诊断；当前测量会标记无效 |
| Ctrl+Option+G / K | 显式本地帧捕获/离屏实验导出；可能包含用户封面，不应提交或上传 |

构建需要 macOS、Rust、Node 和官方 Homebrew glslang/spirv-cross/spirv-tools。先在根目录及 frontend 分别 `npm ci`，使用当前 worktree 内 `.local` 作为 npm cache。脚本打包工具与许可证；预览运行时无需 Python/Homebrew。它使用 macOS private API，不属于 App Store 分发验收。

## 历史证据及解释（本 PR 未重测）

以下是迁移前开发记录的汇总，原始本地帧、图像及日志没有入库；因此不能作为此 PR 的可复现性能验收。已有“更优秀”的结论仅能支持继续保留 WebGL 默认、Metal 实验待验。

| 旧实验 | 观察 | 不能推出什么 |
| --- | --- | --- |
| M4、1920×1182、111 次互动 | 早期 Metal 帧完成约 37.2–37.6 FPS，WebGL RAF 约 33.8 FPS；Metal 完成间隔 P95 74–75 ms、输入至完成 P95 166–168 ms | 两者计时边界不同，不能据此宣称显示帧率或输入延迟优于 WebGL |
| 全背面深度 + MSAA resolve 合并候选 | WebGL 31.48 RAF FPS；Metal 35.27 完成 FPS，完成间隔 P95 90 ms、GPU P95 70.69 ms、输入至完成 P95 179 ms | 未解决卡顿；全背面深度没有默认开启 |
| 一张 26.41 ms GPU 长帧 | CPU 编码 5.04 ms；拆开诊断背面玻璃 10.96 ms、正面 7.15 ms、法线 2.12 ms、景深 0.44 ms | 拆 encoder 改变执行条件，分项不能相加还原真实帧；只是优先调查玻璃着色/重叠绘制的线索 |
| 合并 MSAA resolve | 离线参考图一致；完整 GPU 11.54→11.59 ms | 没有可宣称的性能收益 |
| 原生硬件光追第一表面查询 | 小实验比光栅化慢约 4.5–5.3 倍 | 未实现完整玻璃材质/折射/AO，不能代表所有光追方案；没有理由默认迁移到光追 |
| 动态场景 Metal 图像一致性 | 一次暖昼动态捕获 mean 0.7117/255、max 193，1.9844% 通道差异 >2 | 局部封面差异未解释，不能宣称像素等价 |
| 流畅优先 | 策略单测存在；长宽 2/3 在数学上减少约 56% 三维像素 | 最终 UI、帧率、分辨率切换分配开销尚未验证；没有稳定 60 FPS 结论 |

优先待验的是玻璃透射重叠绘制成本、动态资源与捕获一致性、长帧来源，以及应用完整呈现链路的延迟。封面缓存减少重复绘制/上传有明确算法依据，但仍需在迁移版中衡量总体收益。后续新算法与产品取舍应由总控另立任务，不在迁移 PR 中继续扩展。

## 本次验证

2026-10-05，在本分支独立 worktree 执行。已同步主线 `574c6f5`；完整 app 对应源码提交 `1a684dd`，后续仅补充本文验证记录。环境为 macOS 26.3.1 arm64、Node 24.14.0、Cargo 1.93.1。

- 根目录与前端 `npm ci` 成功；前端 `npm run build`（TypeScript/Vite/PWA）、`check:music`、`check:content`、`check:viewport` 全部通过。首次音乐检查早于依赖安装完成而缺包失败，安装完成后的正式检查通过。
- 在 frontend 执行 `node --experimental-transform-types --test scripts/check-motion-resolution.mjs scripts/check-transmission-prepass.mjs scripts/check-cover-print-cache.mjs scripts/check-metal-data.mjs scripts/check-instance-visibility.mjs scripts/check-cover-cache.mjs`：13/13 通过。其中封面测试使用模拟 Canvas，验证复用/释放，不验证像素或真实 GPU 上传。
- 本地独立 CARGO_HOME/CARGO_TARGET_DIR 下，`scripts/cargo.sh check --offline --locked --manifest-path metal-lab/Cargo.toml --all-targets` 通过。为避开网络重试，把现有公共 crate registry 复制进 worktree 缓存，未复制 Cargo 凭据/配置，未复用旧 target；仅检查而未执行 GPU 工具。
- Python 构建/参考翻译脚本语法检查通过。32 个迁入文件与冻结哈希逐项比对，仅构建适配及 README 有有意差异；QQ 契约/配置文件无 diff。
- `CARGO_NET_OFFLINE=true python3 scripts/build-metal.py` 成功生成 `releases/metal/Rhine Music Metal Preview.app`，从本 worktree 独立 target 完整编译；`codesign --verify --deep --strict` 通过。主程序 SHA-256：`c6e4f848b92b7e2f9841fd88978de35999837d2fe27b366a2c920254605f0d46`。仅本地保留产物，不随 PR 上传；没有启动应用。
- 实验生成副本的 QQ native.ts/qq.rs/audio.rs 与源文件一致，Metal 命令仅注入副本，npm 依赖链接均指向当前 worktree。22 个 QQ/config/native 文件与 origin/main 完全一致。
- 原有 bokeh 检查通过：10,000 个随机 HDR 邻域的数学结果与 41 taps 参考在 1e-12 内一致。它不是整场景视觉验收。
- 已知构建警告：Vite 大 chunk、旧 objc 宏 unexpected cfg 和实验工具未使用符号；未为迁移扩大修改范围。
- Metal GUI/图像一致性、高频交互性能、“流畅优先”收益、真实 QQ 登录播放尚未重验；v2 名称分支未另行打包。标准 QQ 自动测试由本 PR 的 macOS CI 执行，最终运行状态以 PR 检查页为准。

构建成功只证明可编译/打包，不代表 QQ 真实登录播放、Metal 图像一致性或流畅度验收。

默认 QQ GUI 补验结果见 [rendering-default-qa.md](rendering-default-qa.md)：Q1/Q2/Q3/Q2b 已执行；Q4 部分、Q5/Q6 尚未完成，Mac 锁屏中断，测试 app 已退出。该隔离 harness 的视觉回归结果不替代本节尚缺的性能、Metal 或真实 QQ 验收。

## Q5 后续修正（2026-10-05）

原隔离包的 20 秒无输入专辑架记录中，分辨率出现 21 个连续区段（20 次切换），部分全尺寸区段仅约 50 ms；完全停稳的详情页则始终为原分辨率，关闭实验开关后的大画布运动也保持原分辨率。专辑架仍有环境波动，不能称为完全静止。

合成环境波动回归在修复前失败。原因路径为 `field()` 将环境波动叠加到 model/camera 的位置，原分辨率策略用该合成位姿的逐帧变化继续触发降档。修复仅将既有 `idle` 状态传入策略：闲置架子的环境动画不再触发交互降档；用户新操作离开 idle 后仍可降档。沿用现有 idle 判定和 850 ms 运动保持时间，没有改动画、画质预设、位移阈值或渲染架构。30/60 Hz 环境波动、重新交互及停稳回归通过，并接入标准检查。

已同步 main `7d1b9304aa4bbe4bc09fccfd5d3fa7c46d5de50d`；music-app 保留 QQ 面板 dispose 钩子及本分支渲染设置，Rust/QQ 文件保持主线实现。原包对应 11a6aa7，只作修复前证据；修复后实际 GUI 结果另记，不把原包验收用于新组合。
