# 提案：以 FilmCraft 为基座派生 JianyingCraft

## Why

`add-jianyingcraft-editor` 走的是从零实现路线，产出 3,055 行原型：契约与骨架成立，
但**没有任何媒体能力**（无解封装、无解码、无编码、无音频、无帧合成），
与 PhotoCraft 级产品的 34.7 万行媒体栈差 113 倍。从零补齐属于人年级投入，且质量难以追上已验证的上游实现。

同组织的 `storytold/filmcraft` 就是 Craft 家族的**视频编辑器**：34.7 万行、44 个 crate、
编码覆盖 h264/hevc/av1/vp9/vp8/prores/dnx/mpeg2 + aac/opus/flac/mpa/ac3、容器覆盖
isobmff/matroska/mxf/avi/mpegts/ogg、含 `apps/filmcraft-web`（wasm32 + WebCodecs + OPFS +
WebAudio）、165 条引擎命令、MCP 自动化与自动化桥。许可 MIT OR Apache-2.0，允许派生。

因此本变更**取代**从零实施的路径，改为在 filmcraft 基座上派生 JianyingCraft。

## What Changes

- `jianyingcraft` 仓由 filmcraft 本地克隆派生，保留 `upstream` remote，分支 `feature/jianyingcraft-derivation`。
- **内部 crate 名保留 `filmcraft-*`**：它们是上游契约，改名会让每次合并上游都付出代价；
  产品名只在用户可见面统一为 JianyingCraft。
- 品牌层改造：不用 ArtCraft 商标字标（第三方商标，仅限上游使用），改为文字字标；
  窗口标题、Web 页、崩溃对话框、i18n 目录统一 `JianyingCraft`。
- 帮助链接诚实标注：指向的仍是上游 FilmCraft 页面，标签写明 "(upstream)"，不冒充自有页面。
- 后续（本变更范围内的任务）：剪映式交互取向、竖屏短视频默认预设、中文默认语言、
  面板桥（craft_submit 等价物）、JianyingCraft 品牌资产与自有帮助页。

## 非目标

- 不对接抖音/剪映任何内部接口、账号体系或私有草稿格式。
- 不在本变更内重命名 44 个内部 crate。
- 不删除上游能力：所有上游命令、导入导出格式继续可用（工程文件扩展名仍为上游 `.fcproj`）。

## Impact

- 引擎仓：`jianyingcraft`（派生自 filmcraft 0.8.0，HEAD 09aaf9bf）。
- 插件仓：`jianyingcraft-plugin` 改为消费派生仓的 web 构建（替换原先指向自研原型的计划）。
- 原型仓（`jianyingcraft-prototype`）与种子包（`jianyingcraft-seed`）**已于 2026-10-18 移除**：
  工作区只保留 `jianyingcraft`（引擎）与 `jianyingcraft-plugin`（插件）两个事实源。
  原型留下的结论（3,055 行、无媒体能力）仍作为"为何派生"的依据保留在本文与
  `docs/derivation/JianyingCraft-Derivation.md`。
