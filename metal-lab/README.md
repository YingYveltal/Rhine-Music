# Rhine Metal 实验

这里包含 Rust + Metal 离屏验证工具，以及原版完整着色器的实验渲染器。正式应用仍保留 WebView/Three.js；另有可运行的 Metal Preview，把同一场景的绘制交给原生 Metal，控件仍使用 WKWebView。迁移前完整场景、真实互动与限制的汇总见 [迁移记录与历史证据](../docs/rendering-migration.md)。

先在本 worktree 的根目录及 `frontend` 中分别执行 `npm ci --cache <本 worktree 的 .local/npm-cache 绝对路径>`。预览版构建：在项目根运行 `python3 scripts/build-metal.py`。构建需要已安装官方 Homebrew `glslang`、`spirv-cross`、`spirv-tools`；产物会打包工具及许可证，运行时无需 Python、Homebrew 或完整 Xcode。输出为 `releases/metal/Rhine Music Metal Preview.app`，不会替换正式或 QQ 版。启动后按 Ctrl+Option+N 准备/切换原生图层，再按一次回到 WebGL；历史版本首次准备约 15 秒（本次未复测），期间请等待场景准备完成。

完整场景路径：`frame_renderer.rs` 重建原版 GL 状态与材质；`shader_translate.rs` 使用随包附带的 Khronos 工具翻译着色器；`src-tauri/src/metal_bridge.rs` 接入原生窗口图层。该模块只由独立预览构建注册。稳态绘制不逐帧读回像素，使用增量资源更新，最多一个在途帧和一个最新待处理姿态。当前仍有高频操作长帧，预览版默认保留 WebGL。

下面是较早的离屏后处理/光追工具说明，不能代替完整播放器结果：

- 后处理：原 41 点景深、原近景精确双线性化简、景深与 ACES/sRGB 调色合并。输入是原版 AO 后的 RGBA16F 和原版打包深度；逐像素对照最终图像。合并仍保留中间半浮点量化。
- 几何：原 GLB 归一化后的顶点、当前实例矩阵和实际相机。在 Metal 中对照光栅化与硬件光追的第一表面查询；同一几何只建一次 BLAS，实例使用 TLAS。另测每像素 1/4/16 条额外遮挡射线，以及全部实例以合成 8 Hz 运动时的 TLAS refit。最后与重建 TLAS 对照覆盖和深度。
- 计时：使用 `MTLCommandBuffer.GPUStartTime/GPUEndTime`，另记 CPU 编码、提交至完成时间。每项预热 10 次；普通项 100 次、附加射线 60 次、动态 refit 120 次，最多一帧在途。运行时编译 MSL，不要求安装完整 Xcode。

## 复现

在开发播放器中，Ctrl+Option+K 导出当前帧。只在显式按键时读取 GPU 数据，普通播放不执行导出。

数据保存在当前应用 identifier 对应的 Application Support 目录下 `metal-captures/<capture-id>/`，`scene.json` 是完整导出的标记。数据可能包含当前专辑封面，应保持本地，不能无检查地公开发布。

在项目根目录运行：

```sh
CARGO_HOME="$PWD/.local/rendering-build/cargo" CARGO_TARGET_DIR="$PWD/metal-lab/target" ./scripts/cargo.sh build --locked --manifest-path metal-lab/Cargo.toml --release
metal-lab/target/release/rhine-metal-lab '<capture-directory>' '<output-directory>'
```

输出 `results.json`、原 WebGL 参考图、三种 Metal 后处理结果和光栅化/光追的几何识别图。比较时关闭播放器窗口，避免另一渲染进程争用 GPU。不要同时构建或运行其他图形测试。

## 如何解释

后处理验证使用完整原画面，但其 GPU 计时**不包括**几何着色、阴影、AO、景深深度生成、上传、WebView/原生纹理互通与显示。不能换算成播放器帧率，也不能拿它直接与含同步屏障的 WebGL 分阶段墙钟时间比较。

几何实验把可见网格三角形当作双面不透明表面。它没有实现原玻璃 PBR、粗糙折射、软阴影、透明遮罩或 AO，输出仅用于几何核对。附加遮挡射线的样本数不是原版 AO 的等质量替代。共面三角形在不同遍历顺序下可能命中不同实例；报告实例 ID 差异，并单独检查覆盖与深度。

合成动态实验能测更新成本，但不代表真实 8 Hz 用户操作。播放器中的 111 次交互回放另行记录应用到帧提交的延迟，仍不代表物理输入到屏幕发光的延迟。

早期结果与光追判断见 [迁移记录](../docs/rendering-migration.md)。未迁入本地帧捕获、图像、生成着色器和构建日志。
