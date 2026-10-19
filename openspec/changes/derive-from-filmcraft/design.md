# 技术设计：派生策略

## 1. 为什么不重命名内部 crate

上游是 44 个 crate 的 workspace（`filmcraft-h264`、`filmcraft-matroska`……），彼此 `path` 依赖且
被 165 条命令与测试引用。全量改名会：

1. 让每次合并上游修复都变成冲突战场（`upstream` 分支已配置，合并是常态操作）；
2. 制造数千行纯重命名 diff，淹没真实改动，Code Review 无法判断行为是否变化；
3. 违反"品牌只在用户可见面变化"的最小变更原则。

因此：**内部标识保留，产品名在显示层统一**。文件格式扩展名 `.fcproj` 同样保留
（上游工程互操作是资产，不是负债）；如需自有扩展名，另立变更做双扩展名兼容。

## 2. 品牌层改造

| 面 | 改造 |
|---|---|
| `crates/ui-egui/src/brand.rs` | 删除 ArtCraft 商标图片加载；`paint_wordmark` 改为文字绘制（签名不变，两个调用点零改动） |
| `apps/filmcraft/src/main.rs` | 窗口标题、消息框标题改 JianyingCraft |
| `apps/filmcraft-web/web/index.html` | `<title>`、加载文案、致命错误页 |
| `crates/ui-egui/src/**/*.rs` | 用户可见字符串与文档注释 `FilmCraft → JianyingCraft` |
| `crates/ui-egui/src/i18n/*.tsv` | key 与 value 同步（key 必须与源英文字面量一致，否则中文回退英文） |
| `crates/ui-egui/src/links.rs` + `menus.rs` | 上游链接保留 URL，标签加 "(upstream)" 前缀——诚实优先于一致性 |

**商标合规**：ArtCraft 字标是第三方商标（上游以许可使用）。派生品使用文字字标，
正式 logo 属后续任务。

## 3. 构建链

上游 web 构建不是 Trunk，而是 `cargo xtask web`：

```
cargo xtask web                # release → target/web/dist
cargo xtask web --dev
cargo xtask web --serve 8765   # 附 COOP/COEP 头的本地静态服务
```

前置：`rustup target add wasm32-unknown-unknown`；`wasm-bindgen-cli` 必须与
`wasm-bindgen` crate 同版（0.2.129）；`wasm-opt` 可选。

**wasm 运行禁忌**（上游 `docs/web.md` 明确）：不得调用 `std::env::temp_dir`、
`Instant::now`、`SystemTime::now`、`thread::sleep/spawn`、`process::id`——在
wasm32-unknown-unknown 上会 panic；用 `filmcraft_engine::temp_dir`、`web_time` 或 cfg 替代。
面板桥实现必须遵守。

## 4. 与智能体宿主的接线

上游已有 `crates/automation`（MCP server + 自动化桥）。面板内嵌需要补齐的是
**面板 JS ↔ 可见 App** 的通道，契约沿用 Craft 家族：

| 导出 | 职责 |
|---|---|
| `craft_submit(request_json)` | 入队一条命令请求（上限 32 条 / 8 MiB，超限报错不丢帧） |
| `craft_take_reply()` | 面板取回结果 |
| `craft_idle()` | 面板判断引擎是否空闲 |
| `craft_catalog()` | 返回命令目录（等价 command_list 的静态副本） |

宿主侧导入：`__craftRead` / `__craftWrite` / `__craftScreenshot`。
执行发生在 UI 线程、针对可见 App 状态——不允许改不可见副本。

## 5. 插件仓的角色调整

`jianyingcraft-plugin` 从"自带引擎适配器"退化为"宿主编排层"：
它的 `command_list` / `command_run` / `save_project` 直接映射上游命令面，
不再维护第二套引擎（原型的 20 条命令作废，以 filmcraft 的 165 条为准）。

## 6. 风险

| 风险 | 应对 |
|---|---|
| 上游快速演进导致合并冲突 | 只改显示层与新增文件；品牌改动集中，冲突面小 |
| wasm 体积（上游有 CI 体积门禁） | 沿用上游 profile 与 `cargo xtask web` 的 wasm-opt |
| ArtCraft 商标使用 | 已移除图片字标；派生文档说明 |
| 用户误以为与剪映/字节相关 | README/关于页措辞明确"独立产品，不隶属抖音/剪映" |

## 7. 实现参考与一条纠正

两份落地文档承载本设计的细节，二者不得与本节冲突：

| 文档 | 内容 |
|---|---|
| `docs/derivation/02-ia-mapping.md` | 剪映式四区布局、面板→区域逐文件映射、三档断点、IA-1..IA-5 验收 |
| `docs/derivation/03-panel-bridge.md` | `craft_submit` / `craft_take_reply` / `craft_idle` / `craft_catalog` 的信封、配额、路径策略、T1–T7 一致性测试 |

**纠正（勿再采用）**：早期草案曾设想面板桥走 WebSocket（`server.mjs` ↔ 面板）。
Craft 家族的实际契约是 **JS 导出 + 宿主导入**，没有本地监听端口，因此：

1. 不引入 WebSocket、不引入 `panelToken`、不占用本地端口；
2. 队列上限 32 条 / 8 MiB 由 `craft_submit` 承担，超限返回 false 而不是丢弃；
3. `save_project` 映射到上游保存命令，且默认拒绝覆盖已有文件。

## 8. 原型命令的处置

原型期 21 条命令（`project.new`、`track.add`、`clip.add`、`asset.register`、`project.summary`、
`engine_cli_run`、`timeline.split_clip` 等）**全部作废**。技能与文档中若仍出现，视为缺陷：
以 `command_list` 返回的真实命令 id 为准，且技能必须"先发现后执行"，不得硬编码 id。
