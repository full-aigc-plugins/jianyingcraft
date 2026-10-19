# JianyingCraft 派生说明

> 本仓由 [storytold/filmcraft](https://github.com/storytold/filmcraft) 派生。
> 派生基线：**filmcraft 0.8.0 / 提交 `09aaf9bf`**（MIT OR Apache-2.0）。
> 分支：`feature/jianyingcraft-derivation`。

## 1. 为什么派生而不是从零写

从零实现的原型（`jianyingcraft-prototype`，已于 2026-10-18 移除）只有 3,055 行，没有解封装、解码、编码、
音频与帧合成能力；上游 filmcraft 有 346,664 行、44 个 crate，覆盖
h264/hevc/av1/vp9/vp8/prores/dnx/mpeg2 视频编码、aac/opus/flac/mpa/ac3 音频、
isobmff/matroska/mxf/avi/mpegts/ogg 容器，且已有 wasm 浏览器构建（WebCodecs/OPFS/WebAudio）。

派生得到的是**真实可剪辑的引擎**，而不是时间轴示意图。

## 2. 改了什么

| 类别 | 内容 |
|---|---|
| 品牌层 | `crates/ui-egui/src/brand.rs` 不再加载 ArtCraft 商标图片，改为文字字标 `JianyingCraft` |
| 商标移除 | 删除 `docs/brand/`（8 个商标文件 + 边车 + `LICENSE-brand.txt`）；`README`/`NOTICE`/`ATTRIBUTION`/`AGENTS` 同步改写；UI 不再提供第三方社区入口 |
| 显示名 | `crates/**` 与 `apps/**` 中用户可见的 `FilmCraft` → `JianyingCraft`（152 个 .rs/.tsv/.html 文件，仅整词替换） |
| 窗口/页面 | 桌面窗口标题、消息框、Web `<title>`、加载与致命错误文案 |
| i18n | 所有语言目录的 key 与 value 同步；中文值同步（key 必须与源英文字面量一致） |
| 帮助链接 | 指向上游页面的链接标签加 `(upstream)`，不冒充自有页面 |
| 新增 | `openspec/`（规格事实源）、本说明 |

**没有改**：内部 crate 名（`filmcraft-*`）、模块路径、工程文件扩展名 `.fcproj`、
165 条引擎命令、任何媒体处理逻辑。

## 3. 为什么内部 crate 不改名

1. 44 个 crate、165 条命令、数千处引用；改名会造成数千行纯重命名 diff，掩盖真实改动。
2. 保留 `upstream` remote 后，合并上游修复是常态操作；改名会把每次合并变成冲突战场。
3. 品牌只在用户可见面变化，是最小变更原则。

工程文件扩展名保留 `.fcproj`：与上游工程互操作是资产。若需自有扩展名，另立变更做双扩展名兼容。

## 4. 许可义务

- 上游为 **MIT OR Apache-2.0**；本仓同样采用 MIT OR Apache-2.0，保留上游 `LICENSE-MIT`、
  `LICENSE-APACHE`、`NOTICE` 与作者版权声明。
- 上游文件中的版权头与来源注释保留。
- **第三方商标已移除**：`docs/brand/` 下的 ArtCraft 字标/图标（8 个文件及其边车）与
  `LICENSE-brand.txt` 已删除（`git rm -r docs/brand`）。上游 `LICENSE-brand.txt` 明确要求
  修改版移除商标，且不得让派生品显得由 ArtCraft 团队制作、赞助或背书；因此 Help 菜单、
  顶栏、About 页、导入页与 README 中的 Discord / getartcraft 入口一并移除，只保留本仓
  仓库与 issue 链接。正式自有 logo 资产属后续任务。

## 5. 构建

```bash
cargo check -p filmcraft-ui-egui     # 原生检查
cargo test --workspace               # 上游全量测试

# 浏览器构建（上游脚本，非 Trunk）
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo xtask web                      # → target/web/dist
cargo xtask web --serve 8765         # 本地服务（带 COOP/COEP）
```

wasm 运行禁忌（上游 `docs/web.md`）：`std::env::temp_dir`、`Instant::now`、`SystemTime::now`、
`thread::sleep/spawn`、`process::id` 在 wasm32-unknown-unknown 会 panic。

## 6. 与上游同步

```bash
git fetch upstream
git merge upstream/main          # 品牌改动集中在显示层，冲突面小
```

冲突预期集中在：`brand.rs`、`apps/*/src/main.rs` 的标题、`web/index.html`、i18n 目录。
这些文件的改动都是"字符串级"的，按上游新文案重新套用 `JianyingCraft` 即可。

## 7. 工作区目录

交付物只有两个仓库，其余历史目录已移除：

| 目录 | 角色 | 状态 |
|---|---|---|
| `jianyingcraft/` | 引擎（本仓库，派生自 filmcraft） | 事实源 |
| `jianyingcraft-plugin/` | ZCode 插件（adapter / 技能 / 面板 / 工具目录） | 事实源 |
| `jianyingcraft-prototype/` | 早期的从零实现（3,055 行、无媒体能力） | 2026-10-18 已移除 |
| `jianyingcraft-seed/` | 更早的规格与插件种子包 | 2026-10-18 已移除 |

判断"该看哪个"的规则：**改引擎或 UI 看 `jianyingcraft`，改 Agent 侧能力看 `jianyingcraft-plugin`**。
本仓库 `openspec/changes/derive-from-filmcraft/` 是这份派生的唯一规格事实源。
