# Issue #18：准备协议（未运行 GPU）

准备基线 `05e8532360b7e5c9121bba757512c76bb8295ec9`。只评估已有 postFusion，不增加算法，不修改正式默认值，不把方案/单测通过写成性能收益。总控交回时段后，先同步其确认的最终 main，核对组合，再冻结源码提交、app SHA、QA main 差异、窗口/DPR/实际 buffer、设备/电源/系统、fixture hash 和实际 WK store；现在没有测试包或性能结果。

## 隔离与入口

- 新测试身份 `com.rhine.music.issue18-postfusion-qa`，store `D075F93A-B745-4856-B372-9B81E7C54173`。`postfusion-qa-config.json` 只覆盖 QA 身份/窗口；`postfusion-qa-harness.patch` 沿用 #7 已验证的显式建窗＋原生 getter 回读方法，但使用新 UUID。补丁只应用于将来的本地 QA 构建副本，生产 main.rs 不变；已通过 apply --check，实际隔离仍须启动时读回确认。最终窗口标题需标注冻结提交。
- 数据只用本 worktree `.local/postfusion-qa/data`（新建空目录）；`fixture-16` 由原有 create-rendering-fixture.py 新生成，没有复制 #7 数据/应用/存储。16 张原创编号 PNG＋16 段静音 WAV，经普通设置扫描导入，按专辑名每 12 张一列。fixture manifest SHA-256 为 `92738d1a7f415be0d466ee877cf4f8506ae453182a7e56f522eb00cfb8a4683f`。
- 每次启动显式移除 RHINE_QQ_SESSION/QQMUSIC_API_KEY，检查独立目录无 qq/remember-connection，并设置 MUSIC_NATIVE_DATA_DIR。禁止真实账号、扫码、钥匙串、个人媒体及网络曲库。新 CUA 句柄按完整新 app 路径绑定；不复用 #7 闭包。build/target/cache 留在本 worktree，不在本准备阶段安装依赖或构建。
- QA 构建环境仅设 `VITE_POSTFUSION_QA=1`，开启 Ctrl+Option+Y 的本地画面检查；普通构建不暴露该入口。入口要求恰好 16 张 QA 命名专辑、无活动测量/待启动回放。Ctrl+Option+F 仍仅切 postFusion，J 仍为原 28 秒/111 事件回放；不使用 O/V 总开关。

## 第一关：画面正确性

保持 renderingOptimized 开启、原始画质、smoothMotion/透射深度/Metal 关闭。Y 在一次同步调用内先暖两条路径，然后按 A1/A2/B/A3 顺序渲染和读取 RGBA；期间不推进 scene.update、不更换 AO 随机状态，仅变 postFusion。成功或异常都会恢复原开关并重绘。禁止与性能测量重叠。

报告 `postfusion-visual-<唯一时间>.json` 通过既有 save_benchmark 写入 QA 数据，包含时间、主题、场景阶段、冻结位姿、画质、viewport/buffer/DPR、三组误差、四张正确翻转的 PNG data URL 和原开关恢复值。图片只留本机；后续提交只绑定 hash/结果。像素门槛：A1=A2=A3 严格一致，A2/B 最大通道差 ≤2/255；另人工检查颜色、玻璃高光、景深边缘及封面身份，不用低均值掩盖局部错误。

在暖昼/深夜各取架子、打开中的前后景、详情、详情快速换专辑的旧新盒重叠、返回中的位姿。每份原图/AX 绑定实际界面阶段；未捕到对应阶段就不算该项完成。任何画面不合格先停止收益测试，记录缺陷交总控，不改阈值、不新增着色算法兜底。此处渲染/readPixels/PNG 成本全部是扰动式画面诊断，不作性能依据。

## 第二关：深夜筛查的固定顺序

第一关由总控确认后才填 manifest 的 visualReviewedPass。固定同一进程/测试包/合成库/物理 DPR 2、viewport 1280×788、实际 buffer 1920×1182；诊断 DPR 覆盖始终关闭。若这组物理条件不能稳定达到，停止并在测量前由总控重新冻结条件，不能混入不同尺寸结果。

每轮先回到 QA01 的专辑架（必须是 archive、无 pending），设置该轮 A/B 后预热 30 秒；J 固定 111 个同一处理器事件，结束应 QA01/detail/pending=false。除该回放外无人工输入、截图、readPixels、GPU 分段剖析、录屏或并行重型任务。两次正式轮之间始终执行同一复位/预热；固定电源状态，不为了好结果换设备/窗口。测量前后 UI 可取证，但不夹入计时段。

| 顺序 | 类型 | 实际执行顺序 |
| --- | --- | --- |
| 1 | A/A 波动 | AA1-A1 → AA1-A2 |
| 2 | A/B | AB1-A → AB1-B |
| 3 | A/A 波动 | AA2-A1 → AA2-A2 |
| 4 | B/A | AB2-B → AB2-A |
| 5 | A/A 波动 | AA3-A1 → AA3-A2 |
| 6 | A/B | AB3-A → AB3-B |

A=postFusion 关闭，B=开启且实际兼容条件生效；其余开关相同。全部 12 轮原始报告保留，按时间填 `postfusion-night-manifest.example.json` 的 file/hash。报告增加起始开关、实际生效状态、renderingOptimized、起始专辑/阶段及排序方式；中途变 postFusion 会标无效。比较器重算原始 RAF 百分位，并检查完整 111 事件、实际跨列位姿和最终身份。

比较器入口硬锁 viewport 1280×788、物理 DPR 2、buffer 1920×1182，不能靠同时修改 manifest 和报告绕过。111 事件按当前 KeyJ 源码逐项核对 action、顺序和 scheduledMs：专辑 500+i×125 ms（32 次），打开/返回中断 4700/4850/5000/5150 ms，打开 6000 ms，详情切换 7500+i×125 ms（40 次），返回 13000 ms，跨列 14000+i×125 ms（32 次），最终打开 20000 ms。beginMeasurement 的起点略早于 KeyJ 共享起点，仅允许所有事件共用 0–5 ms 偏移；扣除该偏移后的每项数值误差限 0.01 ms。此限制只核验预定时刻，不限制实际 timerDelayMs，不因回调延迟或卡顿排除成绩；共享起点超限也须保留全区块并交总控，不能自行重跑挑选。

来源边界：manifest 中源码、包、fixture、store 和电源字段只是人工声明，非空检查和报告文件 hash 不会自动证明这些声明。总控须另核对实际启动日志、构建源码与包 hash、实际回读的 WK store、测试进程及连续时间、每轮复位和 30 秒预热证据，并确认没有并行负载或额外输入。比较器通过不能替代该核对；证据未确认时不发布收益结论。本项不扩展为自动来源验证框架。

预先排除：内部 valid=false、隐藏/窗口尺寸/DPR/画布改变、环境/画质/开关不符、QQ/私人数据介入、事件缺失或终态错误、原始样本缺失、截图/readback/额外输入/重型任务介入。**长帧、计时器延迟或差的成绩本身不排除。** 任一排除出现，则整个 12 轮区块不形成收益结论，保留全部证据并停下交总控；不自行挑轮次或补跑到通过。准备比较器不提供删除坏轮/自动重试入口。

离线运行：`python3 scripts/evaluate-postfusion.py <已冻结且填全的 manifest>`，只读本地 JSON。错误则 STOP，无收益判定。输出包括绑定、所有报告 hash、每轮重算指标及配对差异；从未启动应用或 GPU。

## 接受与停止的固定算式

- 每对 A/B 的 RAF P95 改善率 `(A−B)/A`；三对都 >0，配对改善率中位数 ≥5%，且严格大于三对 A/A 的最大 `abs(A2−A1)/A1`。
- 对 P99、>50 ms 占比、事件到下一次提交 P95，分别以三对 A/A 的最大绝对差为该指标容忍范围。每对 B−A 都不得超过对应范围（数值运算 epsilon=1e−9，不是另一个性能阈值）。不只看平均 FPS。
- 无稳定收益：保持默认关闭，交证据结束候选，不补完整矩阵。通过仅代表可进入下一阶段，仍须总控确认后补两主题 20 秒架子/28 秒交互、SMAA/景深关闭等兼容回退、点选/拖动/快速返回、现有 QQ 契约/transport 测试及授权的隔离假服务取曲播放；模拟与真实账号验收分开。
- RAF 是回调间隔，CPU 是提交工作耗时，事件指标止于下次提交；WebGL GPU 数据为空就保留为空。没有真实显示或 OS 输入到光子的结论。Metal 维持实验，本单既不调整也不借其完成时间证明 WebGL 收益。

## 准备阶段已检查与待资源项

准备代码提交 `88f9652311c45a19631c884bdc3ab45efb62af9c` 已保存；随后以合并提交 `42052728d2b4ec3a5d30bc3edf73a52d5c85a107` 纳入总控确认的 main `ff30a9a6bd82445d926b26c0ed6ac59a9d0e6d35`。组合后 3 项画面工具逻辑检查、6 项离线比较检查与 QA 补丁 apply --check 通过。此提交仍只是准备源码，不是最终冻结的实测包。

资源成本预估：独立依赖安装/首次 release 构建约 10–25 分钟（视缓存）；隔离回读和两主题画面检查约 15–25 分钟；深夜 12 轮固定预热及回放净计时 11 分 36 秒，连同复位和证据整理约 20–30 分钟。若前关失败即停止，不为失败候选预占完整矩阵；通过后的完整验收另排。上述是排期估计，不是已消耗或实测性能数据。

结束时按本次启动记录的 PID 退出新 QA 进程，并验证已退出，释放设备。保留本 worktree 的原始报告、包 hash、配置和 fixture；不自动删除旧 #7 或正式用户目录。仅在本次 QA 身份内把 postFusion 恢复关闭；不改用户应用偏好。新 WK store 保留作可复核证据，后续需要删除时只针对回读确认的本次 UUID；不执行全局 WebKit/缓存清理。

纯逻辑检查入口：`node --experimental-strip-types --test frontend/scripts/check-postfusion-validation.mjs`、`python3 scripts/check-postfusion-evaluation.py`。覆盖恢复开关、A/A 不稳定/色差拒绝、异常失败、5%门槛、A/A 波动及单对长尾退化拒绝；不是像素/GPU/完整 TypeScript 编译验收。

待总控：核对准备提交和上述协议，交回设备/构建时段，确认最终 main；之后才安装本 worktree 依赖、构建独立包、核对 codesign/hash/实际 WK store，导入 fixture 并执行第一关。正式默认开关仍关闭；未启动 QA，未写实现 PR。
