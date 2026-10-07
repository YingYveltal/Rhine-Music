# 版权、署名与资源来源

本说明区分代码授权、音乐适配的开发者署名和第三方素材的权利。公开源码不表示放弃版权，也不表示获得全部原作素材的再授权。

## 开发者与上游

| 范围 | 署名与来源 |
| --- | --- |
| 本仓库的 macOS 衍生版本 | 由 **[YingYveltal / Rhine-Music](https://github.com/YingYveltal/Rhine-Music)** 独立维护，包括本仓库的 Rust / Tauri 移植、音乐来源接入及后续修改。它不是上游作者发布的安装包；本仓库新增工作不一概归为 RonaldDeng 的创作。 |
| 上游音乐播放器适配 | **Copyright (c) 2026 RonaldDeng**。[Rhine-Music-Demo](https://github.com/RonaldDeng/Rhine-Music-Demo) 的本地曲库、音乐界面、专辑模型适配及该项目的后续修改。本仓库的上游来源为 **v0.3.0**，提交 [`e910899eb005240e5eba3e680d0e0809e852b10d`](https://github.com/RonaldDeng/Rhine-Music-Demo/tree/e910899eb005240e5eba3e680d0e0809e852b10d)。 |
| 原版三维档案界面 | **Copyright (c) 2026 LBEILC**。[RhineLabUI](https://github.com/LBEILC/RhineLabUI) 提供三维档案界面、渲染与动效基础及相关建模脚本；上游音乐适配初始基于提交 [`5abab02367465d9189f4ae65bcb6f17fdb5938f7`](https://github.com/LBEILC/RhineLabUI/tree/5abab02367465d9189f4ae65bcb6f17fdb5938f7)。 |

本仓库从本地原生 QQ 移植快照开始建立自己的 Git 提交历史，来源记录见 [baseline-source.json](docs/baseline-source.json)。保留上游代码、许可、署名与历史说明，**不声称包含两层上游的完整 Git 提交历史**，也不代表已经合入 Rhine-Music-Demo 后续版本的全部功能。

有权授权的程序代码、建模脚本和技术文档沿用 [MIT License](LICENSE)，保留许可证中的 LBEILC 与 RonaldDeng 版权声明。分发代码或其重要部分时须一并保留版权声明及完整许可证；本仓库维护者的署名不取代上游署名。MIT 允许使用、修改及商业分发，软件按原样提供；本说明不在 MIT 上另加用途限制。具体以许可证原文为准。

[frontend/README.md](frontend/README.md) 保留上游音乐版说明，[frontend/README.original.md](frontend/README.original.md) 保留原版界面说明，其中“本项目”“原创”等措辞归属于各自上游文档的语境。YingYveltal、RonaldDeng 与 LBEILC 分别代表自己的项目；衍生版本不表示上游作者参与、认可或提供支持。

## 原作及非代码资产

视觉参考为《明日方舟》特别映像[「莱茵生命：访问」](https://www.bilibili.com/video/BV1rr4y1b7sz/)。相关作品、名称、标志、设定、视觉设计、原 PV 及其声音的权利归相应权利人所有。本项目是独立爱好者工程，与官方制作方无隶属、赞助或授权合作关系，不主张拥有原作版权或商标。

- `frontend/art/*.blend`、`frontend/public/assets/*.glb`、界面图标、截图、动图及原作相关演示文本**不因代码采用 MIT 而自动获得原作素材许可**。上游未另行将全部非代码资产声明为 MIT；建模脚本的 MIT 授权也不等于原作相关视觉元素可任意再分发。
- `frontend/public/audio/atmosphere.ogg`、`frontend/public/audio/motif.ogg`、`frontend/public/audio/pulse.ogg` 和 `frontend/public/audio/observatory-preview.mp3` 为继承自上游的程序编配，按其[音频说明](frontend/public/audio/README.md)随项目采用 LICENSE；不将其重新署名为音乐适配者原创。
- `frontend/public/audio/typing-preview.wav` 与 `frontend/src/typing-samples.ts` 的三个 38ms 短音来自上述原 PV，来源、时间码及处理记录见 [typing-source.json](frontend/public/audio/typing-source.json)。**这些采样不属于 MIT 授权范围；来源标注不是原权利人的再分发许可。** 本项目不能为复用者授予相应权利。仅用于本地比对的 `frontend/reference/typing-original.wav` 不随本次源码快照和 ZIP 分发；本仓库不附带该原始对照文件。
- [演示封面](frontend/public/demo-covers/README.md)是上游早期音乐适配阶段生成的三张抽象图像，用于空库演示和上游历史界面展示，不冒充真实唱片或音乐库。V0.2.0 历史截图由该版本运行画面采集，V0.3.0 界面回归截图使用隔离示例库采集；截图不改变其中字体、模型与原作元素各自的权利。
- 真实歌曲、商业专辑封面、私人曲库索引、缓存和在线介绍不随本仓库及发布包分发。

代码复用者可替换这些资产；使用或再分发涉及第三方权利的内容时，需另行遵守对应许可或取得必要授权。本文件不作“全部资源已获商业授权”的保证。

## 字体与直接依赖

| 组件 | 权利人与许可 | 随包文本 |
| --- | --- | --- |
| MiSans | 小米／Beijing Xiaomi Mobile Software Co., Ltd.，按 MiSans 字体协议随应用原样嵌入；不是 MIT，不将字体作为独立商品或字体包发布 | [署名](frontend/public/fonts/NOTICE.txt)、[完整字体协议](frontend/public/fonts/MiSans-license.pdf) |
| Three.js | three.js authors，MIT | [three.txt](frontend/public/licenses/three.txt) |
| Rolling Number | Kit Langton，MIT | [rolling-number.txt](frontend/public/licenses/rolling-number.txt) |
| music-metadata | Borewit，MIT | [music-metadata.txt](frontend/public/licenses/music-metadata.txt) |
| OpenCC-JS | nk2028，MIT；所用 OpenCC 字典数据另受 Apache-2.0 约束 | [MIT](frontend/public/licenses/opencc-js.txt)、[第三方说明](frontend/public/licenses/opencc-js-third-party.md)、[Apache-2.0](frontend/public/licenses/apache-2.0.txt) |

这些文本保留依赖包原文。前端依赖版本见 `frontend/package-lock.json`；本仓库构建与原生依赖另见根目录 `package-lock.json`、`src-tauri/Cargo.lock` 和 `qq-connector/Cargo.lock`；其他构建工具和间接依赖按安装包各自的 LICENSE / NOTICE 使用，不被本项目重新授权。

## 在线资料

Wikipedia 摘要、Wikidata 与 MusicBrainz 数据按其来源许可处理。客户端保存介绍时同时保存来源链接、取得时间和许可说明；此类用户自行查询的缓存不纳入项目 MIT 授权，也不进入发布包。MusicBrainz 将核心数据和补充数据分别授权，详见[官方数据许可](https://musicbrainz.org/doc/About/Data_License)；不能仅因为程序开源就统一将数据改授 MIT。

如发现署名遗漏或具体权利问题，请通过本仓库 [Issues](https://github.com/YingYveltal/Rhine-Music/issues) 提供涉及文件、来源及说明，维护者可据此核对。
