# Issue #7：Q4/Q5 续验与环境动效修复

2026-10-05。PR #10 保持 draft。已确认并修复一项分辨率策略缺陷；这不是帧率收益或完整 GUI 验收结论。原 QA 方案、Q1–Q3/Q2b 和隔离 harness 说明见 [首轮报告](rendering-default-qa.md)。

## 版本与环境

| 项目 | 修复前 | 修复后 |
| --- | --- | --- |
| 源码 | `11a6aa7d188a1d01a167377a5f13772f5bc44b01` | `f0551fc89929db1360bc82d9329716e8008e1d8f` |
| 主程序 SHA-256 | `55da51edade6d0875ca02a86afe2b6f786000438dae6f8ec4cbfca41f8f578de` | `2714345b6f29d2d016bcdca76395174fd1cbb7d88cafb9f29119a6ddde9454f3` |
| QA main SHA-256 | `5515e6da48039e59a8f21a79adfd40d00d96dafe11585bf7924b7d8d4daf8c04` | `211a509adac6f7d2ba22596e54efffd63cd0243e34c2ebaebce7d295a39709d5` |
| 20 秒 archive 样本 | 物理 DPR 2，无诊断覆盖 | 物理 DPR 1，既有 DPR 诊断覆盖 2 |
| 全尺寸 / 运动尺寸 | 1920×1182 / 1279×787 | 1920×1182 / 1279×787 |

两包均为 macOS 26.3.1 arm64 默认 WebGL 隔离 QA。修复后产品名和窗口标题另外标注 Rendering QA/f0551fc，原包保留且哈希复核未变。二者使用同一专用 identifier `com.rhine.music.issue7-default-qa`、读回确认的 WK store `C886D1F6-8483-4DF9-9BD0-C8688AACEE05` 及 `.local/default-qq-qa/data-final`，仅含 16 专辑合成 fixture。每次启动移除 QQ 会话/密钥环境变量并检查无 remember-connection；未登录、保存连接、导入账号或播放真实媒体。

同尺寸 buffer 不能消除物理 DPR/诊断覆盖差异。下面仅比较策略行为，不比较 FPS、输入到显示延迟或完全同画质性能。内部 valid=true 也不证明这些结论。

新组合先合入 main `7d1b9304aa4bbe4bc09fccfd5d3fa7c46d5de50d`（合并提交 `089938a`），保留 QQ 面板 dispose 钩子，再加入 `f0551fc` 的环境动效修复及回归测试。Rust/QQ 契约与配置保持该 main 实现。冻结来源哈希和后续有意变化分别列在 rendering-source.json。

## 已观察结果

以下 benchmark 文件名均在 `.local/default-qq-qa/data-final/benchmarks/`；截图/AX 在旧包 `evidence-resume/` 和新版 `.local/default-qq-qa-fix/evidence/`。哈希绑定见 [续验证据清单](rendering-qa-followup-evidence.json)，原图与日志不上传。

| 检查 | 结果及边界 |
| --- | --- |
| 修复前无输入 archive，20 秒 | `20261005-135948-optimized-archive-night.json`：21 个连续分辨率区段，即 20 次切换；部分全尺寸区段约 50 ms。环境动画仍在运行，不能称场景静止 |
| 修复前详情与关闭策略 | `20261005-140128-optimized-detail-night.json` 详情 20 秒全尺寸；`20261005-140418-optimized-motion-night.json` 关闭策略后的全屏运动仅 1920×1080 |
| 修复后运动及随后无输入 | `20261005-141505-optimized-motion-night.json` 采到全尺寸及运动尺寸，结束 scale=1；`20261005-141538-optimized-archive-night.json` 随后 20 秒仅 1920×1182、零切档 |
| 修复后再次交互 | `20261005-141710-optimized-stress-night.json`：111 次处理器事件完成；仍采到两档，结束 scale=1，最终 QA01/detail/pending=false，与 expectedFinalAlbum 匹配。当时按流派只有一列，不把 genre-burst 当作真正跨列验证 |
| 保存与重启 | 旧包开启、关闭后重启分别读到对应值；新包重启仍显示开启（fixed-restart-enabled.txt）。后续收尾已恢复关闭，页面重载后再次确认 |
| Q4 鼠标续验 | 旧包详情拖出 canvas 至标题区释放，再进入反向拖动，封面可旋转、标题不变、后续设置可用。详情/架子右键、容器左边缘点击未换专辑；面板操作/遮罩关闭未换专辑。新版也观察了拖出释放及反向旋转 |
| Q4 仍有限制 | 前两次尝试点非选中卡片未改变选择，未据此判定回归或通过；解锁收尾已建立正向点选证据，见下节。无按钮纯 hover/leave、多指触摸和压力笔未覆盖；截图抽样不能排除全部瞬态异常 |

修复机制与 idle 条件准确含义见 [迁移报告](rendering-migration.md#q5-后续修正2026-10-05)。策略单测使用人工 idle 标志，实际原生包观察另列，不能相互替代。

## 检查及未完成项

f0551fc 本地：新增回归先红后绿，14 项渲染检查通过；9 项 QQ 前端测试、music-scene、21 项 music-presentation 检查通过；TypeScript/Vite/PWA 构建、默认 QA release 打包和 codesign 验证通过。云端检查以 PR 当前提交状态为准，不沿用旧提交成功状态。

首次续验因 Mac 锁屏中断；用户解锁及总控交回时段后，下面的有限清单已完成。Q6 仍未覆盖：现有原生 UI 工具不能只删除测试 store 的 smoothMotion 字段；没有修改 WebKit 数据库或正式用户偏好。

初次准备新版截图误调用了捕获旧 app 的工具闭包，导致旧包额外启动。立即核对路径并退出该旧进程；未执行账号或媒体操作。旧 `evidence-resume/fixed-start.*` 无效并排除。本文正式新版测量均在旧进程退出后运行，以新版路径、窗口标题、二进制哈希和匹配 UUID 绑定。

## 解锁后的有限收尾（同日）

仍使用 f0551fc 的同一新包，无重新打包；再次核对主程序 SHA-256 和实际 store UUID，运行 PID 24028。启动绑定、运行日志及下列截图/AX 位于 `.local/default-qq-qa-fix/finish-launch.json`、`runtime-finish.log`、`evidence-finish/`，未覆盖此前证据。新建 CUA app 句柄及捕获函数，未复用旧包闭包。

正向点选时窗口截图 2560×1640，物理 DPR 2：点击可见橙色非选中卡片 `[1080,665]`，标题 QA01→QA02、选择序号 01→02；`pick-before` 与 `pick-orange-after` 的截图和 AX 同时确认。它是三维卡片坐标点击，不是底部选择按钮。

随后通过普通设置切为按专辑名字，每 12 张一列；实际切入 ALBUMS 02/02、QA13。正式测量保持全屏 viewport 1920×1080、物理 DPR 2、诊断覆盖 null；全尺寸 2880×1620，运动尺寸 1919×1079。与前一轮窗口尺寸不同，只验证行为，不用于前后性能比较。

| 报告文件（原 benchmark 目录） | 收尾结果 |
| --- | --- |
| `20261005-145054-optimized-stress-night.json` | 111 次事件、28 秒，valid=true、无中断；双列回放的 genre-burst 位姿 columnCamera 范围 -10.4 至 2.236，实际发生横向运动。终态为预期 QA01/detail/pending=false；两档均有采样，结束全尺寸 |
| `20261005-145147-optimized-archive-night.json` | 20 秒采样期间由 QA01 连续 10 个原生 Down 导航至 QA11。从首个已采样区间累计约 2639 ms 时由运动尺寸恢复全尺寸，后续不反复切档；valid=true、无中断。键盘事件时间另存 fullscreen-long-navigation-keys.json |
| `20261005-145235-optimized-archive-night.json` | 随后独立 20 秒无输入，始终 2880×1620、零切档，valid=true、无中断 |
| `20261005-145407-optimized-motion-night.json` | 设置面板打开，使用既有 M 回放触发运动；开关由 1→0。样本区间累计约 976 ms 开始降档，约 2304 ms 回全尺寸，后续调度运动仍保持全尺寸；结束 enabled=false/scale=1、valid=true、无中断 |

关闭操作墙钟范围为 14:53:49.583–14:53:50.128 UTC，报告测量起点约 14:53:47.796 UTC，恢复区间位于关闭调用范围附近。`disable-timing.json` 与前后 AX 绑定实际操作；raw interval 累计以首个采样区间为原点，与测量起点存在首帧偏移，不能视为精确输入到显示延迟或逐帧硬件时序。生产 setSmoothMotion 的 reset/resize 同步恢复路径另经静态核对；实测未见关闭后仍持续降档。

早期几次 J 尝试未生成报告，不能算完成回放；其密集截图只作诊断，不作为正式性能或回放证据。`20261005-144956-optimized-archive-night.json` 跨越窗口尺寸变化，valid=false、raw 为空，明确排除。初次窗口启动/排序刷新期间控制区域曾未显示，切换窗口全屏后出现；未确定生产根因，也未将此现象归因于本次策略修复。

收尾通过普通 UI 恢复按流派并重载页面；`restored-settings` 确认原始画质、smoothMotion=0、reduced=0、界面音效/BGM=0。诊断覆盖此前关闭且页面重载重置。Cmd+Q 退出自身 PID 24028 后核对进程已不存在、无 remember-connection，资源交回总控。没有真实账户、媒体播放或存储迁移操作。

代码及前期证据提交 `3aa386f` 的 [macOS CI 已通过](https://github.com/YingYveltal/Rhine-Music/actions/runs/37324678369)。本次仅追加文档与本地证据哈希，后续文档提交的 CI 以 PR Checks 为准。Q4/Q5 的上述有限检查完成；Q6、纯 hover/leave、触摸/笔、真实 QQ 和完整性能/Metal 验收的边界不变。
