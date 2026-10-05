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
| 保存与重启 | 旧包开启、关闭后重启分别读到对应值；新包重启仍显示开启（fixed-restart-enabled.txt）。尚未把最后偏好恢复到关闭 |
| Q4 鼠标续验 | 旧包详情拖出 canvas 至标题区释放，再进入反向拖动，封面可旋转、标题不变、后续设置可用。详情/架子右键、容器左边缘点击未换专辑；面板操作/遮罩关闭未换专辑。新版也观察了拖出释放及反向旋转 |
| Q4 仍有限制 | 两次尝试点非选中卡片未改变选择，未建立正向 raycast 命中证据；不能据此判定回归或通过。无按钮纯 hover/leave、多指触摸和压力笔未覆盖；截图抽样不能排除全部瞬态异常 |

修复机制与 idle 条件准确含义见 [迁移报告](rendering-migration.md#q5-后续修正2026-10-05)。策略单测使用人工 idle 标志，实际原生包观察另列，不能相互替代。

## 检查及未完成项

f0551fc 本地：新增回归先红后绿，14 项渲染检查通过；9 项 QQ 前端测试、music-scene、21 项 music-presentation 检查通过；TypeScript/Vite/PWA 构建、默认 QA release 打包和 codesign 验证通过。云端检查以 PR 当前提交状态为准，不沿用旧提交成功状态。

尚需在新版验证真正跨列快速导航、长距离导航尾段及关闭实验开关即时恢复；之后恢复按流派/流畅优先关闭/诊断覆盖关闭并退出测试实例。继续 GUI 时工具报告 Mac 锁屏，已请求用户解锁，未绕过锁屏。Q6 未覆盖：现有原生 UI 工具不能只删除测试 store 的 smoothMotion 字段；没有修改 WebKit 数据库或正式用户偏好。

初次准备新版截图误调用了捕获旧 app 的工具闭包，导致旧包额外启动。立即核对路径并退出该旧进程；未执行账号或媒体操作。旧 `evidence-resume/fixed-start.*` 无效并排除。本文正式新版测量均在旧进程退出后运行，以新版路径、窗口标题、二进制哈希和匹配 UUID 绑定。
