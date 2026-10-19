# 面板桥实现契约

> 依据：`openspec/changes/derive-from-filmcraft/design.md` §4「与智能体宿主的接线」。
> 本文只把该节的四条导出与三条宿主导入**落到可实现细节**，不改变契约本身。

## 1. 拓扑

```mermaid
flowchart LR
  A[Agent / MCP 客户端] -->|MCP stdio| P[plugin: ui/dist/adapter.mjs]
  P -->|craft_submit / craft_take_reply| W[面板 WASM 可见 App]
  W -->|__craftRead / __craftWrite / __craftScreenshot| H[宿主 ZCode / Codex]
  W --> E[engine 165 条命令]
```

关键约束：**执行发生在 UI 线程、针对可见 App**。禁止在后台克隆一个不可见项目来跑命令，
否则用户看到的画面与 Agent 改的数据会分叉。

## 2. 面板导出（JS 侧）

| 导出 | 签名 | 语义 | 上限/错误 |
|---|---|---|---|
| `craft_submit` | `(requestJson: string) -> boolean` | 入队一条命令请求；返回是否入队成功 | 队列 32 条、累计 8 MiB；超限返回 false 并保留原队列，**不丢帧、不静默截断** |
| `craft_take_reply` | `() -> string \| null` | 取回一条结果 JSON；无结果返回 null | 结果同样受 8 MiB 约束 |
| `craft_idle` | `() -> boolean` | 队列空且无在跑命令 | — |
| `craft_catalog` | `() -> string` | 命令目录静态副本（等价 `command_list`） | 启动时生成一次，命令注册变化时失效重取 |

请求/结果信封：

```jsonc
// submit
{"id":"q-7","id_command":"timeline.razor","params":{"seconds":3.5}}
// reply ok
{"id":"q-7","ok":true,"result":{"cuts":2}}
// reply error
{"id":"q-7","ok":false,"error":{"code":"command_disabled","message":"no clips selected"}}
```

`result` 必须与 `jianyingcraft-cli --demo exec <id>` 的 JSON 同构，否则 Agent 无法在
"可见面板"与"无头"两种后端之间迁移脚本。

## 3. 宿主导入（由面板调用，宿主注入）

| 导入 | 用途 | 失败语义 |
|---|---|---|
| `__craftRead(path)` | 读工作区内文件 | 越界 → 抛错，不回退到任意路径 |
| `__craftWrite(path, bytes)` | 原子写工作区内文件 | 失败 → 返回错误对象，不落半成品 |
| `__craftScreenshot(panel?)` | 取面板/窗口 PNG | 宿主不支持 → 返回 null，由调用方降级为 `render_frame` |

**实现（2026-10-20）**：策略在 `apps/filmcraft-web/src/host_policy.rs`（纯 `std`，不依赖 `web_sys`），
wasm 侧薄绑定在 `apps/filmcraft-web/src/host.rs`，导出 `craft_read` / `craft_write` / `craft_screenshot`。
它们和 §10 的四个 `craft_*` 一样是 wasm-bindgen 的模块导出（面板直接 `filmcraft_web.craft_read(path)`），
不另开端口、不进 `window.filmcraft`。返回/异常形状严格按上表：

| 导出 | 成功 | 失败 |
|---|---|---|
| `craft_read(path)` | `Uint8Array` | **抛出** `Error`，`.code` 为 `path_outside_workspace` / `quota_exceeded` / `invalid_path` / `io_error` |
| `craft_write(path, bytes)` | `{ok:true, path, bytes, overwritten}` | 返回 `{ok:false, error:{code, message}}`（不抛） |
| `craft_screenshot(panel?)` | `Uint8Array`（PNG） | `undefined`（宿主不支持），调用方降级 `render_frame`，不算错误 |

宿主的写回必须带 `overwritten`（覆盖时为被覆盖的路径，新建时为 `null`）；Rust 侧把"没有返回值"的写回
判为 `io_error`，不会把未确认的写当成成功。

## 4. 路径与配额

1. 所有路径先解析 `realpath`，再判断是否位于工作区根内；符号链接逃逸一律拒绝。
2. 单次导入 ≤ 256 个文件 / 128 MiB；单产物 ≤ 128 MiB；宿主保存对话框另限 50 MiB。
3. 写入先临时文件再原子改名；覆盖必须在结果里报告被覆盖的路径。

### 4.1 实现与证据（2026-10-20）

**位置与理由**：策略放在 `apps/filmcraft-web/src/host_policy.rs`，不放进 `crates/platform`——`platform`
是唯一允许 `unsafe` 的 OS 媒体 FFI crate（硬解/硬编），把纯 `std` 的路径策略塞进去会破坏它的职责边界，
还会让 wasm 构建多背一个媒体 crate；`apps/filmcraft-web` 本来就是"浏览器侧宿主服务"的归属地，且已有同款
先例：`recovery_policy.rs` 是纯模块，`tests/recovery_policy.rs` 用 `#[path]` 在宿主上执行它。

| 规则 | 实现 | 证明（`apps/filmcraft-web/tests/host_policy.rs`） |
|---|---|---|
| realpath + 根内判断 | `Workspace::open` 先 canonicalize 根；`resolve` 对已存在的目标 canonicalize，对新文件 canonicalize 其父目录；`is_within` 按路径分量比较（Windows 忽略大小写） | `absolute_path_outside_the_root_is_rejected`、`absolute_path_inside_the_root_is_allowed` |
| 拒绝 `../` | `check_syntax` 在碰文件系统之前直接拒绝任何 `ParentDir` 分量（不做归一化） | `parent_dir_escape_is_rejected_for_reads_and_writes` |
| 拒绝符号链接逃逸 | realpath 之后再做前缀判断 | `a_link_pointing_outside_the_workspace_is_rejected`（Windows 用无需管理员权限的 junction；两种链接都建不出来时打印 skip，不冒充通过）、`a_resolved_path_that_leaves_the_root_is_rejected`（注入 realpath，任何平台都执行） |
| 拒绝 UNC / 盘符相对路径 | `check_syntax` 拒绝 `\\`、`//`、`\\?\` 与 `C:foo` | `unc_drive_relative_and_nul_paths_are_rejected` |
| 单次导入 ≤ 256 文件 / 128 MiB | `ImportBatch::admit` 计数累加；第 257 个或超字节的那次被拒，且不入账 | `an_import_admits_at_most_256_files_and_128_mib` |
| 单产物 ≤ 128 MiB | `Workspace::write` 在解析路径、建临时文件**之前**检查长度 | `a_write_over_the_artifact_quota_leaves_the_previous_file_untouched` |
| 临时文件 + 原子改名 | 同目录 `create_new` 临时文件 → `write_all` + `sync_all` → `rename`；任一步失败删临时文件并返回 `io_error` | 同上（断言目录里仍只有原文件）；`an_overwrite_reports_the_path_it_replaced`（断言无 `.tmp` 残留） |
| 覆盖报告被覆盖路径 | `WriteOutcome.overwritten = Some(相对路径)`，新建为 `None` | `an_overwrite_reports_the_path_it_replaced` |
| 截图不支持 → null | `Screenshot::from_png(None) == Unsupported`，`.png() == None` | `an_unsupported_screenshot_is_none_not_an_error` |

**执行到哪一步（诚实边界）**：

- **真实执行**（宿主 `cargo test -p filmcraft-web`）：上表的路径、配额、原子写、覆盖报告与截图分类。
  测试直接对临时目录做真实文件操作，没有 mock 文件系统。
- **只做类型检查**（`cargo check -p filmcraft-web --target wasm32-unknown-unknown`）：`host.rs` 里的
  `js_sys::Reflect` / `Function` 调用、`Uint8Array` 编码，以及把宿主返回值映射进策略类型。本环境没有
  "注入了三个全局的宿主页面"，因此这三个导出**没有在浏览器里被真实调用过**，也没有进入 §10 的 Node wasm
  探针（那组探针只调用了四个 `craft_*`）。
- **页面内真正的文件系统约束由宿主执行**：浏览器页面没有自己的文件系统，`__craftRead` / `__craftWrite`
  必须自己完成 realpath、根内判断与原子改名。Rust 侧做它能做的：`check_syntax`、配额、对缺失/不可识别
  返回值 fail-closed；它不假装替宿主完成了 realpath。宿主实现不在本仓库，本仓库只能约束接口与结果形状。

## 5. 长任务

- 导出沿用上游：`file.exportMedia` + `wait`，进度以"已完成帧数/总帧数"上报（≤100 ms 一次）。
- 取消 → 下一个批次中止、删除半成品、不回包。
- 面板断开时正在跑的导出在面板内继续，但结果无人接收；**不得自动重放**。

## 6. 一致性测试（TDD 门禁）

| 测试 | 断言 |
|---|---|
| T1 目录一致 | `craft_catalog` 与 `command_list` 的命令 id 集合完全相等 |
| T2 结果同构 | 12 条代表性命令在面板与 headless 两种后端的 `result` 深度相等 |
| T3 队列上限 | 第 33 条返回 false，前 32 条仍可正常 take_reply |
| T4 路径逃逸 | `../`、符号链接、UNC 全部拒绝且报 `path_outside_workspace` |
| T5 可见性 | 面板命令改动的结果能被下一次 `sequence.inspect` 读到（证明改的是可见 App） |
| T6 断线 | 面板关闭后 pending 请求在 15 s 内失败，不重放 |
| T7 参数严格 | 未知参数报 `-32602` 并点名参数 |

**T4 实现（2026-10-20）**：`apps/filmcraft-web/tests/host_policy.rs` 的
`parent_dir_escape_is_rejected_for_reads_and_writes`、`absolute_path_outside_the_root_is_rejected`、
`a_link_pointing_outside_the_workspace_is_rejected`、`unc_drive_relative_and_nul_paths_are_rejected`
都断言错误码为 `path_outside_workspace`，并进一步证明工作区外的目标文件字节未变（读不到、写不进）；
`a_resolved_path_that_leaves_the_root_is_rejected` 用注入 realpath 固定住前缀判断本身。详见 §4.1。

**T1 实现（2026-10-20）**：`crates/ui-egui/tests/bridge_ui.rs::craft_catalog_ids_match_command_list`
把 `bridge::catalog()`（`craft_catalog` 在 wasm 里的同一份实现）的命令 id 集合与
`Session::default().execute("command.list")` 的 id 集合逐项比较，并同时断言
`filmcraft_engine::command_specs()` 与 `command.list` 同源。三方任一漂移都会红，不再依赖
"catalog = 701" 这种只查数量的抽查。

**T2 实现（2026-10-19）**：`crates/ui-egui/tests/bridge_ui.rs::bridge_results_match_headless_for_representative_commands`
拿 12 条静态查询命令分别走面板桥与 headless `Session`，断言结果 JSON 深度相等。两处有意排除：
`command.list`（面板会话多注册了 `tool.*` / `playback.toggle` / `window.*`，目录本就不同）与
`sequence.settings` / `sequence.colorSettings`（UI 路径会打开对话框，返回 `{"dialog": …}`）。

**T5 实现**：`bridge_executes_commands_against_the_visible_app` —— 经桥提交 `file.newBin`，
随后控制通道 `project.inspect` 能看到它，证明改的是可见 App。T3/T7 由
`bridge_limits_are_enforced_at_the_app_boundary` 与 `bridge_reports_command_failures_instead_of_dropping_them` 覆盖；
导出边界（`InFlight`）的 32 条 / 累计 8 MiB 另由 `bridge::tests` 的三条单测锁死（见 §15）。

**T6 实现（2026-10-19）**：adapter 的 pending 超时从 45 s 收紧到 `HEARTBEAT_MS`（15 s）——
面板 15 s 没有心跳就视为掉线；超时同时把这条从队列里移除，面板回来不会当新请求重放
（`ui/tests/bridge-disconnect.test.mjs`：心跳一次 → 停止轮询 → `command_run` 在 14–21 s 内
以 `timed out` 失败 → 再 poll 返回 `null`）。实测全套插件测试 **45 / 45**。

## 7. 与上游 MCP 的关系

插件侧的 `command_list` / `command_run` / `save_project` 只做**编排**：
- `command_list` → 优先 `craft_catalog`，无面板时退回 `jianyingcraft-cli mcp --demo` 的 `tools/list`；
- `command_run` → `craft_submit` + `craft_take_reply`；
- `save_project` → 映射到上游保存命令（`file.save` / `file.saveAs`，以 `command_list` 实际 id 为准），
  不接受覆盖已有文件，除非调用方显式确认。

原型期的 21 条命令（`project.new` / `track.add` / `clip.add` / `asset.register` …）**作废**，
任何文档与技能不得再引用。

## 8. 实现状态（引擎侧）

- Rust 侧队列已落地：`crates/ui-egui/src/bridge.rs`。
  - 上限：`MAX_PENDING = 32`、`MAX_BYTES = 8 MiB`；超限 `submit` 返回 `false`，已入队请求不丢。
  - 信封：接受 `{"id","command","params"}`，同时兼容设计稿里的 `id_command` 拼写。
  - `take_request`（UI 线程取）、`take_reply`（adapter 取）、`idle`、`pending_bytes`、`rejected`。
  - 回复信封：`{"id","ok":true,"result":…}` / `{"id","ok":false,"error":{"code":"command_failed","message":…}}`。
- 导出边界记账：`bridge::InFlight` 保存「已投递、未取回」的请求及其原始 JSON 字节数，
  `try_push` 同时校验 `MAX_PENDING` 与累计 `MAX_BYTES`；超限时**不改任何状态**，已登记请求仍可逐条取回。
  `apps/filmcraft-web/src/api.rs` 的 `craft_submit` 在投递 `ControlRequest` **之前**登记，
  所以累计上限真正落在导出边界，而不是刚入队就被排空的 `Bridge`。
- 命令目录：`bridge::catalog()` 是 `craft_catalog` 的唯一来源（`command_specs()` 的静态副本）。
- UI 线程执行：`FilmcraftApp::pump_bridge(ctx)` 每帧在 `eframe::App::logic` 里执行队列，
  命令落在**可见 App** 上；失败也回包，不重放。
- 测试：`bridge::tests`（12 条单测，含 InFlight 的累计字节 / 第 33 条 / 空取 / 回滚）
  + `tests/bridge_ui.rs`（5 条端到端：副作用可见、错误回传、第 33 条被拒且前 32 条各回一包、
  T1 目录 id 集合相等）。
- 插件侧工具目录：`jianyingcraft-plugin/ui/tools.mjs` 为唯一来源（17 工具 / 14 公开 + 3 面板内部），
  `ui/build-tools.mjs` 生成 `ui/dist/tools.json`，`build.mjs` 随构建产出并写进 `build-info.json`；
  `ui/tests/tools.test.mjs` 4 条校验目录与来源同步。
- wasm 侧四个 JS 导出（`craft_submit` / `craft_take_reply` / `craft_idle` / `craft_catalog`）
  已在 `apps/filmcraft-web/src/api.rs` 实现（`#[wasm_bindgen]`），并通过 wasm32 类型检查；
  `craft_take_reply` 空队列返回 `null`（`JsValue::NULL`，不是 `undefined`）。运行时验证见 §10、§14，
  2026-10-20 的审计修复见 §15。

## 9. 运行时验证（2026-10-18）

插件仓 `npm ci`（102 包）后，用 stdio 探针对 adapter 做真实握手：

| 探针 | 结果 |
|---|---|
| `initialize` | `jianyingcraft-editor 0.1.0` |
| `tools/list` | **17 个工具**（14 公开 + 3 `_panel_*`），与 `dist/tools.json` 完全一致 |
| 无面板 `editor_status` | `isError: true` — "Call open_editor first" |
| 无面板 `command_run` | `isError: true` — 同上（fail-closed，不改看不见的副本） |

**下一步的关键发现**：`apps/filmcraft-web/src/api.rs` 已有 `Sender<ControlRequest>`、
`request(method, params) -> Promise`、`poll_replies()` —— "JS → UI 线程 → 命令" 通道已存在。
`craft_*` 四导出应做成它的薄封装（同步队列语义用 `Bridge` 补齐），不要另建传输。

## 10. wasm 四导出（已实现，待运行时验证）

`apps/filmcraft-web/src/api.rs` 新增四个 `#[wasm_bindgen]` 导出：

| 导出 | 行为 |
|---|---|
| `craft_submit(json) -> bool` | 复用 `Bridge` 解析；32 条 / 累计 8 MiB 由 `InFlight` 在投递前记账，超限返回 `false` 且不执行 |
| `craft_take_reply() -> string \| null` | 按提交顺序收取回复（轮询 `InFlight` 队首的 `Receiver`），空队列返回 `null`；信封为 `{"id","ok",...}` |
| `craft_idle() -> bool` | 无在途、无排队、无待取回复 |
| `craft_catalog() -> string` | `command_specs()` 的 `{commands:[{id,label,menu,shortcut,params}]}` |

验证程度：`cargo check -p filmcraft-web --target wasm32-unknown-unknown --locked` **exit 0**；
`cargo xtask web` 产出 **40.8 MB wasm + 182 KB glue**，`WebAssembly.Module.exports` 中四个
`craft_*` 全部存在。用 `wasm-bindgen --target nodejs` 在 Node 里**真实调用**的结果：

| 调用 | 结果 |
|---|---|
| `craft_catalog()` | `commands` = **701**（与 `filmcraft-cli commands` 一致） |
| `craft_submit({id:"t-1",command:"file.newBin"})` | `true` |
| `craft_take_reply()` | `{"id":"t-1","ok":false,"error":{"code":"command_failed","message":"JianyingCraft is not running yet"}}` |
| `craft_idle()` | `true` |
| `craft_submit("not json")` | `false` |

**未做**：在真实运行中的 App（浏览器/面板）里执行命令；面板 JS 仍走原型通道 `_panel_*`。

## 11. 跨仓接线与契约测试（2026-10-18）

插件仓新增 `ui/editor-boot.mjs`：把四个 `craft_*` 包成稳定接口（形状校验 →
`craft_submit` → 轮询 `craft_take_reply` → 解析回复），并提供 `run()` 的提交后等待与超时。

| 测试 | 覆盖 |
|---|---|
| `ui/tests/editor-boot.test.mjs`（8 条） | 合法请求转发、四类畸形载荷在进引擎前被拒、队列满不是静默丢弃、回复解析、坏回复转错误信封、`run()` 成功/超时 |
| `ui/tests/editor-wasm.test.mjs`（跨仓） | 直接加载派生引擎的 node 版 wasm：catalog=**701**、submit 受理、失败回包 `command_failed`、`idle()`、畸形载荷被拒 |

跨仓测试用环境变量开关：设置 `JIANYINGCRAFT_WASM_JS` 时执行，未设置时 **skip**（不会把"没构建"报告成通过）。

## 12. 编辑器面板页（第二个 MCP App 资源）

旧 `panel.html` 是剪映草稿只读查看器（读 `draft_content.json`、调 `jianying-cli`），和 `craft_*`
不是同一条路径。为此新增：

| 文件 | 作用 |
|---|---|
| `ui/panel-editor.html` | 编辑器页骨架：`#editor-canvas` + 引擎/桥状态徽标 + 加载失败提示 |
| `ui/panel-assets.mjs::editorHtml` | 把 `editor-boot.mjs` 内联进页面（去掉 `export`），生成单文档资源 |
| `ui/adapter.mjs` | `resources/list` 返回 `panel.html` 与 `editor.html`；`resources/read` 分别产出 |

实测（MCP stdio 探针）：`resources/list` → 2 个资源；`resources/read editor.html` →
5,298 字节 HTML，含 `#editor-canvas`、内联 `createEditorBridge`、启动段、无 `export` 残留。

**未验证**：浏览器里真正跑起来——需要把 `cargo xtask web` 产物放进 `ui/engine/`，并由宿主注入
`window.__jianyingcraftEngine`；本机没有 Playwright 浏览器二进制，所以这一步保持"未验证"。

## 13. 引擎产物同步（`ui/sync-engine.mjs`）

`cargo xtask web` 的产物不能进 git（约 39 MiB），但面板要加载同一份文件。同步工具做四件事：

| 行为 | 说明 |
|---|---|
| 必需文件检查 | `index.html` / `filmcraft_web.js` / `filmcraft_web_bg.wasm` 缺一个就拒绝，不产出半套 |
| 体积上限 | 默认 60 MiB，超过直接报错（面板要整份下载） |
| 哈希记录 | 每个文件记 `bytes` + `sha256` 到 `engine/build-info.json` |
| `--check` | 报 drift：`missing` / `sha256` / `extra`（多出来的残留文件也算） |

实测：`node ui/sync-engine.mjs --source <engine>/target/web/dist` → `copied: 5,
totalBytes: 40,969,658`；随后 `--check` → `drift: []`。`ui/engine/` 已在 `.gitignore`。

另外更正一条记录：`cargo xtask wasm` **本来就覆盖 `filmcraft-web`**
（`WEB_CRATES = ["filmcraft-ui-egui", "filmcraft-web"]`，49 crates），之前写成"未包含"是错的。

## 14. 真实浏览器验证（2026-10-18）

`ui/browser-smoke.mjs` 做四件事：起本地静态服务器 → 用 `CHROME_PATH` 指定的 Chromium 加载
编辑器面板页 → 等 wasm 初始化 → 读页面上的状态徽标。实测输出：

```json
{"exit":0,"engineReady":true,"bridgeReady":true,"commands":701}
```

页面显示"引擎已加载 / 面板桥就绪 · 701 命令"，说明 `craft_*` 在**真实浏览器的 WebAssembly
实例**里可用，且目录与 CLI 同源（701）。无 `CHROME_PATH` 时脚本打印 skipped 并退出 0——不把
"没装浏览器"报告成通过。

**边界**：eframe 主界面（`/app/index.html`）在无 GPU 的 headless Chromium 里停在
"Loading JianyingCraft…"，因为 SwiftShader 起不来 WebGPU；这是 egui 渲染层的问题，
和面板桥（纯 wasm 调用）无关，两者不要混为一谈。

### 14.1 更正与端到端（2026-10-19）

上面那条"停在 Loading"的结论**是错的**：用 `--dump-dom` 时虚拟时间会在异步启动前耗尽，
看起来像卡死。改成"页面把结果 POST 回本地服务器"后，实测启动链全部成功：

```json
{"ok":true,"started":true,"hasApi":true,"demo":{"sequence":21},
 "videoClipsBefore":7,"durationFrames":642,
 "split":{"cuts":3},"videoClipsAfterSplit":8,
 "undo":{"undone":"Add Edit"},"videoClipsAfterUndo":7}
```

即：**真实浏览器里运行 App → 打开 demo → 读序列 → 分割 → 撤销**，控制通道可用。
脚本：`jianyingcraft-plugin/ui/browser-app-smoke.mjs`（缺 `CHROME_PATH`/`JIANYINGCRAFT_WEB_DIST` 时 skip）。

## 15. 审计修复（2026-10-20）：导出边界的累计上限与 `null` 哨兵

两条真实分歧、两条证明缺口，修复如下：

| 问题 | 修复 | 证明（测试名） |
|---|---|---|
| `api.rs` 每次 submit 后同步排空 `Bridge`，回复又存在另一个 `Bridge`，所以在每次 submit 时 `pending_bytes()` 都是 0；在途条目只有 `(String, Receiver<Value>)`，没有字节数——最多 32 条 × 单条约 8 MiB 一起堆在通道里 | 新增 `bridge::InFlight`（`VecDeque<InFlightEntry>` + 累计 `bytes`）：`try_push` 原子地校验 32 条与累计 8 MiB，超限不改状态；`craft_submit` 先登记再投递，`collect_bridge_replies` 收下队首回复后 `pop_front` 释放字节 | `bridge::tests::the_cumulative_byte_budget_is_enforced_by_the_bookkeeping`、`the_thirty_third_in_flight_entry_is_refused`、`a_rolled_back_entry_releases_its_bytes`、`the_front_entry_exposes_its_reply_receiver` |
| `craft_take_reply` 原先返回 `Option<String>`，wasm-bindgen 把 `None` 映射成 `undefined`，而契约要求 `null` | 改为一律返回 `JsValue`：有回复 `JsValue::from_str`，空队列 `JsValue::NULL` | 空取由 `bridge::tests::taking_from_an_empty_bookkeeping_yields_none` 在 host 上证明；`JsValue::NULL` 只在 wasm 目标编译（`cargo check -p filmcraft-web --target wasm32-unknown-unknown`），**host 测试无法执行** |
| T1（catalog id 集合 == `command.list` id 集合）没有自动化测试 | `bridge::catalog()` 成为 `craft_catalog` 的唯一实现，测试三方比对 | `tests/bridge_ui.rs::craft_catalog_ids_match_command_list` |
| 32 条 / 溢出只在 App 的 `Bridge` 上证明 | 同上，改在导出边界的 `InFlight` 上证明 | `bridge::tests` 的 InFlight 三条（见上） |

另外两处随之收紧：`craft_idle` 现在用 `InFlight::queued()`（旧代码里的 `queued` 标志因 `Bridge`
每帧被排空而永远为 false）；`craft_submit` 去掉了重复的 `len()` 预检，上限判断只有 `InFlight` 一处。

**诚实边界**：`apps/filmcraft-web` 整 crate 是 `#![cfg(target_arch = "wasm32")]`，host 上的
`cargo test` 不编译 `api.rs`。因此 `craft_submit` 的接线（先记账再投递、发送失败 `pop_back` 回滚）与
`craft_take_reply` 的 `JsValue::NULL` 只经过 wasm32 类型检查；真实浏览器/Node 调用沿用 §10、§14
已有的运行时证据，本轮没有新增浏览器运行。
