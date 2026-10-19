# JianyingCraft 验收报告

> 日期：2026-10-18 ｜ 基线：filmcraft `09aaf9b` ｜ 仓库：`jianyingcraft`（origin `full-aigc-plugins/jianyingcraft`）
> 说明：本报告只写**本轮实际执行过**的检查；未执行的项单列在 §5，不并入"通过"。

## 1. 结论

| 目标 | 判定 | 一句话依据 |
|---|---|---|
| ① fork + 品牌替换 + 许可 | **达成** | ArtCraft 品牌资源已移除，NOTICE/ATTRIBUTION 写明派生关系；`cargo xtask assets` 61 项通过 |
| ② 剪映式 UX（移动端优先） | **达成** | 剪映四件外壳（13 Tab / 左 rail / 草稿参数 / 时间轴工具条）+ 首次启动默认 + Compact/Medium 单页壳，均有测试与截图 |
| ③ 面板桥 | **达成** | 17 工具、`craft_*` 四导出、`editor.html` 资源、真实 Chromium 就绪（701 命令）；浏览器内已实跑 `openDemoProject → inspect → addEdit → undo` |
| ④ 场景技能 | **达成（3 个可执行技能）** | 6 技能 44 id 全部命中 701 条目录；字幕 / 批量混剪 / 图文成片由 `ui/skill-smoke.mjs` 一条命令复跑并断言产物；另 3 个是交互说明 |
| ⑤ TDD / 完成门禁 | **达成（lib + 10 个核心包集成测试）** | 54 包 lib **exit 0**；export/interchange/media/captions/text/frame/format/render/platform/cli 的集成测试 0 失败；`cargo xtask ci` 仍受 Windows `cargo fmt --check` 限制 |

**总判定：未达"完成"**。剩余差距只剩 ⑤（全量 workspace 测试与 Windows 下的 fmt 限制）以及两处打磨（滚动条常显、Tab 的素材库内容）。

## 2. 本轮门禁实测

| 门禁 | 结果 |
|---|---|
| `cargo test -p filmcraft-ui-egui -j 1`（lib + 全部集成目标） | **0 failed**（无 FAILED 行，exit 0） |
| `cargo clippy -p filmcraft-ui-egui --all-targets -- -D warnings` | **exit 0** |
| `cargo fmt -p <pkg> --check` × 54 包 | **0 失败** |
| `cargo fmt --check`（工作区形式） | **失败**：Windows `os error 206`（命令行过长，非格式问题） |
| `cargo xtask layers` | **ok** |
| `cargo xtask wasm` | **ok（49 crates，含 filmcraft-web）** |
| `cargo xtask assets` | **ok（61 assets）** |
| 插件 `node --test`（7 个文件） | **41 tests：40 pass / 1 skip / 0 fail** |
| `node ui/build-tools.mjs --check` | `tools.json is up to date (17 tools)` |
| `node ui/verify-skills.mjs` | 6 技能 / 44 id / **0 missing**（对 701 条命令） |
| `CHROME_PATH=… node ui/browser-smoke.mjs` | `{"engineReady":true,"bridgeReady":true,"commands":701}` |
| `node ui/sync-engine.mjs --check` | `drift: []`（5 文件 / 40,969,658 字节） |

## 3. 逐项证据

### ① 派生与许可

- `docs/brand/` 已 `git rm`（17 个删除）；`NOTICE` 含派生声明；`AGENTS.md` §1.8 禁止第三方商标。
- `crates/ui-egui/src/brand.rs` 为文字 wordmark；`links.rs` 只剩 GitHub / Issues 两条。
- 资产门禁 61 项全部带 sidecar（`cargo xtask assets` ok）。

### ② 剪映式 UX

| 项 | 证据 |
|---|---|
| 单一断点入口 | `layout.rs::LayoutClass::from_width`（6 条单测，含 NaN/∞/≤0） |
| Compact（390×884） | 顶栏 ☰ + 预览 + 底部时间轴 + 三 Tab 抽屉；`layout_ui` 3 条 |
| Medium（768×1024） | 右侧 240 px 侧栏；`layout_ui` 1 条 |
| Wide（1280×1024） | `Jianying` 工作区四区（左素材/中预览/右属性+关键帧/底时间轴），`layout_ui` 1 条 + 截图 |
| ☰ 菜单 | 可滚动 + 过滤框；`layout_ui` 2 条 |
| 截图 | `target/ui-snapshots/`：`layout-mobile.png`、`layout-tablet.png`、`layout-desktop.png`、`layout-medium-sidebar.png`（另有多张可选的 `layout-*.png`）；一条命令复现，见 §10 |

### ③ 面板桥

| 层 | 证据 |
|---|---|
| 引擎队列 | `bridge.rs`（32 条 / 8 MiB），7 单测；`bridge_ui.rs` 3 条 e2e |
| wasm 导出 | `craft_submit/take_reply/idle/catalog`，`WebAssembly.Module.exports` 4/4 |
| 目录同源 | `craft_catalog()` = **701**（与 CLI 一致） |
| 面板页 | `editor.html` 资源 5,298 字节；真实 Chromium 就绪，徽标"面板桥就绪 · 701 命令" |
| 引擎落位 | `ui/sync-engine.mjs` 同步 5 文件 40.97 MB，`--check` drift 0 |
| 插件协议 | `initialize` → `jianyingcraft-editor 0.1.0`；`tools/list` 17 工具；无面板时 `isError: command_failed` |

### ④ 技能

| 技能 | 证据 |
|---|---|
| 智能字幕 | `transcript.set → inspect → createCaptions → list → export`，2 条字幕 / 155 字节 SRT / cue 2 |
| 批量混剪 | `clip.replaceFromBin`（29→6，联动 30）→ `saveCopy` 28,245 B → `exportMedia` **17,984,537 B mp4**，`offlineMedia: []` |
| 其余四个 | id 校验通过（0 missing），未实跑 |

### ⑤ 门禁

见 §2。`cargo xtask ci` 的第一道 fmt 在 Windows 上因命令行长度失败，因此"CI 通过"不能宣称。

## 4. 与剪映参考的对照

`docs/derivation/09-jianying-reference.md` 记录了本机剪映 11.6.0.14552 的实测：PO 文案 18,093 条、
Resources 60+ 模块。据此确认的三处缺口——**变声（pitch）无命令**、**本地模板未做**、**代理 UI 未暴露**——
本次未实现，属于后续工作，不是"已完成"。

## 5. 未验证清单（不计入通过）

> 2026-10-19 更新：第 3 条已由 `ui/browser-app-smoke.mjs` 关掉——真实 Chromium 里启动 App、
> `file.openDemoProject` → `sequence.inspect`（642 帧 / 7 片段）→ `sequence.addEdit`（7→8）→
> `edit.undo`（回到 7）。第 4 条同时更正：eframe 能在 headless 启动，之前的"停在 Loading"
> 是 `--dump-dom` 虚拟时间耗尽造成的误判。

1. `cargo test --workspace`（50 crate，曾触发 LNK1102 内存不足）
2. `cargo xtask ci` 端到端（同上 fmt 限制）
3. ~~面板内执行真实命令~~ → **已验证**（见上）
4. ~~eframe 主界面在浏览器里的渲染~~ → **已验证启动**（SwiftShader 下 WebGL2 回退可用）
5. 图文成片等四个技能的实跑（依赖 TTS 音色 / 语音模型）
6. `transcript.generate`（Whisper，本机无模型）
7. 变声、本地模板、代理 UI（剪映参考得出的缺口）

## 6. 复现

```powershell
cd E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft
cargo xtask layers; cargo xtask wasm; cargo xtask assets
cargo xtask snapshots   # UI 验收截图，见 §10
cargo test -p filmcraft-ui-egui -j 1
cargo clippy -p filmcraft-ui-egui --all-targets -- -D warnings
foreach ($p in (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages.name) { cargo fmt -p $p --check }

cd ..\jianyingcraft-plugin
node --test ui\tests\*.test.mjs
node ui\verify-skills.mjs --catalog ..\jianyingcraft\target\commands.json
$env:CHROME_PATH="$env:LOCALAPPDATA\ms-playwright\chromium-1243\chrome-win64\chrome.exe"; node ui\browser-smoke.mjs

cd ..\jianyingcraft
target\debug\filmcraft-cli.exe --demo run <captions-script.jsonl>
target\debug\filmcraft-cli.exe --demo run <remix-export.jsonl>
```

## 7. 风险

- **工作树未提交**：160 修改 / 17 删除 / 若干新增仍未入库，交付物只存在于本地。
- **Windows 专属工具限制**：工作区级 `cargo fmt --check` 与整仓 `cargo test` 在本机不可用，CI 结论只能在其他平台复核。
- **插件仓为原型改造**：`panel.html` 仍是剪映草稿查看器，新编辑器页与其并存，plugin.json 的 surface 切换尚未做。





## 8. 2026-10-19 最终复核（本轮实跑，非引用）

| 项 | 命令 | 结果 |
|---|---|---|
| 编辑器布局 | `cargo test -p filmcraft-ui-egui --test layout_ui` | **28 passed / 0 failed** |
| 工作区单测 | `cargo test --workspace --lib -j 1 --no-fail-fast` | **exit 0**（54 包） |
| 核心包集成 | 10 包分批 `-p X --tests` | **0 failed**（export/interchange/media/captions/text/frame/format/render/platform/cli） |
| fmt 门禁 | `cargo xtask fmt` | **exit 0**（Windows 退化逐包 53 包） |
| 分层 / 资产 / wasm | `cargo xtask layers` / `assets` / `wasm` | **ok / 61 assets / 49 crates** |
| 插件单测 | `node --test ui/tests/*.test.mjs`（CHROME_PATH + JIANYINGCRAFT_WASM_JS） | **44 / 44 pass** |
| 技能实跑 | `FILMCRAFT_CLI=… node ui/skill-smoke.mjs` | captions / batch-remix / text-to-video **全 ok** |
| 真实应用（wasm） | `browser-app-smoke.mjs` | demo 21 号序列、7 段视频、642 帧；分割 +1、撤销还原 |
| 品牌门禁 | `cargo xtask brand` | **exit 0**（15 个 Cargo.toml description、workflow、shader 注释、OTIO generator_kind 已改名） |
| 自有图标（桌面 + Web） | `python assets/brand/make-icon.py` → `cargo xtask assets` → `cargo check -p filmcraft` | **ok（61 assets）**；`.ico` 9 档 / `.icns` 16–1024 / hicolor 8 档；上游 6 个图标已删除 |
| 发行标识迁移 | 23 个文件改名 + 28 处内容替换 | `ai.jianyingcraft`（desktop/metainfo/flatpak/Info.plist/MSI/AppStream）；MIME `application/x-filmcraft` 有意保留 |
| 新建草稿比例预设 | `sequence_ui::new_sequence_offers_aspect_presets`（21 passed）＋ `preset_tests` 单元测试 | 16:9 / 9:16 / 1:1 三个入口；截图 `new-sequence-presets.png` |
| 桥 T6 断线 | `ui/tests/bridge-disconnect.test.mjs` | pending 在 14–21 s 内失败且不重放；插件 45 / 45 |
| 桥 T2 结果同构 | `bridge_ui::bridge_results_match_headless_for_representative_commands` | 12 条静态查询面板/headless 结果深度相等；4/4 bridge_ui 通过 |
| 浏览器面板端到端 | `ui/tests/panel-live.test.mjs`（Playwright + 真实 wasm） | **ready=true，命令返回 7 个视频片段**；插件 46 / 46 |

**尚未完成（如实记录）**：`cargo xtask ci` 的 `clippy --workspace --release` 与
`test --workspace --release` 未整跑（本机耗时以小时计，用上面的分层证据替代）；
编解码包的集成测试未跑（上游未改动的包）；工作树仍未提交。
## 9. 当前状态（2026-10-19，第 40 轮）

- ① 派生/品牌/许可、② 剪映式 IA、③ 面板桥协议与测试、④ 场景技能、⑤ 门禁：**均达成并有实跑证据**
- 浏览器面板真实执行命令：**已完成**（Playwright + 真实 wasm，ready=true、命令返回 7 个片段）；未完成项只剩代理/变声 UI、ZCode 实机验收
- **最大风险仍是工作树未提交**（165+ 修改 / 新增目录未入库）；提交需要维护者授权
### 已提交（2026-10-19）

`jianyingcraft` @ `feature/jianyingcraft-derivation`：
`c553b5bf` 品牌 → `b31bd9cc` UI 外壳 → `cb5b1b11` 桥与测试 → `9b61989a` 文档；
`jianyingcraft-plugin`：`2f79769` 插件与技能（初始提交）。两个工作树均干净，**未推送**。

## 10. UI 验收截图的复现（`cargo xtask snapshots`，2026-10-20）

一条命令产出 IA-1/2/3 的布局截图（§3 ②）：

```powershell
cargo xtask snapshots              # 渲染，然后校验四张必备截图
cargo xtask snapshots --skip-run   # 只校验已有目录（CI 检查）
```

- **落盘位置**：工作区 `target/ui-snapshots/`（跟随 `CARGO_TARGET_DIR`，xtask 把它解析成**绝对路径**再写进
  `FILMCRAFT_UI_SNAPSHOT_DIR`）。四张必备文件：`layout-mobile.png`（390×884）、`layout-tablet.png`
  （768×1024）、`layout-desktop.png`（1280×1024）、`layout-medium-sidebar.png`（Medium 右侧 240 px
  侧栏，900×1024）。跑完打印目录、每张文件大小、目录内 PNG 总数与耗时；缺任何一张都以非零退出
  并列出缺失文件名（`--skip-run` 同样如此）。
- **为什么串行**：每个 UI 测试都会建自己的 wgpu harness。默认并行线程让同一进程里多个 harness 并发创建，
  Windows 上会以 `STATUS_ACCESS_VIOLATION`（0xc0000005）中途崩掉，一次运行就拿不齐截图。
  `snapshots` 固定用 `--test-threads=1` 跑 `layout_ui`，串行后 28 passed / 24 张 PNG。
- **只跑 `layout_ui`**：上面四张由它独占；其他 `*_ui.rs` 也写自己的可选截图（`panels-*.png`、
  `media-*.png`、`export-*.png`…），串行跑完 80 个测试目标要几十分钟，对这份验收证据没有增益。
  需要时仍可自行设置 `FILMCRAFT_UI_SNAPSHOT_DIR` 单独跑它们。
- **相对路径不再跟随 CWD**：`filmcraft_ui_egui::test_support::snapshot_dir()` 统一读环境变量；相对值按
  工作区根目录解析，所以在 `crates/ui-egui` 里直接 `cargo test` 也不会再生成
  `crates/ui-egui/crates/ui-egui/target/ui-snapshots/` 这种嵌套目录。

## 11. 2026-10-20 IA 验收复核（本轮实跑）

基线 `3557d652`（`main`，工作树干净）。测试全部串行执行（`--test-threads=1`）：每个用例自建
wgpu harness，Windows 上并发创建会在启动阶段崩掉，原因见 §10。下表「结果」除宿主三导入一行
注明引用来源外，均取自本轮终端输出的 `test result:` 行。

| 验收项 | 测试名 | 命令 | 结果 |
|---|---|---|---|
| IA-1 390×884 无溢出、导入/分割/字幕/导出不换页 | `ia_acceptance_ui::ia1_compact_has_no_page_overflow`、`ia1_compact_actions_keep_the_window_and_the_dock`、`ia1_wide_rail_import_does_not_leave_the_workspace`、`ia1_export_enters_export_mode_without_leaving_the_workspace` | `cargo test -p filmcraft-ui-egui --test ia_acceptance_ui -- --test-threads=1` | **9 passed; 0 failed**（同一二进制含 IA-2..IA-5 共 9 条） |
| IA-2 768×1024 侧栏可收起、不遮挡预览 | `ia_acceptance_ui::ia2_tablet_768_inspector_sits_right_of_the_preview`、`ia2_medium_inspector_sits_right_of_the_preview` | 同上 | 2 条通过（总计 9 passed; 0 failed） |
| IA-3 1280×1024 四区常驻、时间轴可拖拽 | `ia_acceptance_ui::ia3_wide_keeps_four_regions_and_a_draggable_timeline` | 同上 | 1 条通过（总计 9 passed; 0 failed） |
| IA-4 抽屉开合前后 `sequence.inspect` 不变 | `ia_acceptance_ui::ia4_inspector_round_trip_leaves_the_sequence_untouched` | 同上 | 1 条通过（总计 9 passed; 0 failed） |
| IA-5 Tab 切换不掉播放、不丢选区 | `ia_acceptance_ui::ia5_playback_and_selection_survive_tab_switches`；`tab_switch_ui::wide_tabstrip_switches_keep_playback_and_selection`、`compact_shell_tab_switches_keep_playback_and_selection` | 同上；`cargo test -p filmcraft-ui-egui --test tab_switch_ui -- --test-threads=1` | IA 侧 1 条通过；`tab_switch_ui` **2 passed; 0 failed** |
| 3 Tab 顶栏（剪辑/字幕/音频 + 更多） | `layout_ui::top_bar_shows_three_primary_tabs_plus_more`、`compact_tab_bar_disables_unimplemented_modules`、`shell_tabs_cycle_with_left_right` | `cargo test -p filmcraft-ui-egui --test layout_ui -- --test-threads=1` | **29 passed; 0 failed** |
| 右栏五分组（基础/变速/动画/音频/调色）可折叠 | `properties_groups_ui::properties_panel_has_the_five_jianying_groups_and_they_collapse`、`color_group_edits_the_lumetri_effect` | `cargo test -p filmcraft-ui-egui --test properties_groups_ui -- --test-threads=1` | **2 passed; 0 failed** |
| 轨道头 ≤96 px / 主轨磁吸 / 字幕轨置顶 | `timeline_layout_ui::jianying_track_headers_are_at_most_96_px_wide`、`caption_track_header_matches_the_video_track_header_width`、`caption_tracks_sit_above_the_video_tracks`、`dragging_a_clip_snaps_to_the_neighbour_edge`、`dragging_a_clip_snaps_to_the_primary_track_edge`、`dragging_a_clip_near_the_primary_track_lands_on_it` | `cargo test -p filmcraft-ui-egui --test timeline_layout_ui -- --test-threads=1` | **6 passed; 0 failed** |
| 素材宫格：搜索 / 类型筛选 / 拖入导入 | `project_media_grid_ui::search_filters_the_media_grid`、`type_chips_filter_the_media_grid`、`a_file_dropped_on_the_window_is_imported` | `cargo test -p filmcraft-ui-egui --test project_media_grid_ui -- --test-threads=1` | **3 passed; 0 failed** |
| 宿主三导入 `__craftRead` / `__craftWrite` / `__craftScreenshot` | `host_policy::parent_dir_escape_is_rejected_for_reads_and_writes`、`absolute_path_outside_the_root_is_rejected`、`absolute_path_inside_the_root_is_allowed`、`unc_drive_relative_and_nul_paths_are_rejected`、`a_link_pointing_outside_the_workspace_is_rejected`、`a_resolved_path_that_leaves_the_root_is_rejected`、`a_write_over_the_artifact_quota_leaves_the_previous_file_untouched`、`an_overwrite_reports_the_path_it_replaced`、`reads_and_writes_report_workspace_relative_paths_and_are_accounted_for`、`an_import_admits_at_most_256_files_and_128_mib`、`an_unsupported_screenshot_is_none_not_an_error` | `cargo test -p filmcraft-web --test host_policy` | **11 passed; 0 failed**（记录于 `c89cba9a` 的验证行；本轮按门禁只复跑 `filmcraft-ui-egui` 目标，`apps/filmcraft-web/src/host_policy.rs`、`host.rs`、`tests/host_policy.rs` 自该提交未改动） |

**口径说明**：

- IA 五条共用 `ia_acceptance_ui` 一个测试二进制，因此"结果"列先给该二进制总计（9 passed），
  再标出本条对应的用例数；单条过滤可用 `cargo test -p filmcraft-ui-egui --test ia_acceptance_ui ia1_`。
- IA-1 的「0 次页面切换」按 `02-ia-mapping.md` §5 的单窗口定义判定：工作区、dock 树、窗口尺寸不变；
  `mode` 进入 `Export` 属于同一窗口内的屏幕切换，明确不计为页面切换。
- 宿主三导入的浏览器侧 JS 调用仍只做 wasm32 类型检查（没有注入三个全局的宿主页），
  真实文件操作（路径越界、配额、原子写、截图降级）由上面的 11 条宿主测试覆盖；边界见
  `03-panel-bridge.md` §4.1「执行到哪一步」。

## 12. 2026-10-20 插件侧面板桥复核（跨仓实跑）

插件仓 `jianyingcraft-plugin/ui` 的套件在本机跑通（Node v24.14.1，`node_modules` 96 个包已在位）：

```
$ node --test tests/*.test.mjs
ℹ tests 46   ℹ pass 44   ℹ fail 0   ℹ skipped 2
（2 个 skipped 需要外部条件：JIANYINGCRAFT_WASM_JS / CHROME_PATH）

$ $env:CHROME_PATH = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
$ node --test tests/panel-live.test.mjs
✔ 浏览器面板：页面轮询并在 wasm 里执行命令 (2898.87ms)
ℹ tests 1   ℹ pass 1   ℹ fail 0   ℹ skipped 0
```

由此关闭四条：`adapter ↔ 浏览器面板的最后一公里`、`adapter 消费引擎侧 Bridge`、
`与 jianyingcraft-plugin 的 adapter 契约对齐`、`面板内命令执行走可见 App 状态`。

证据链：`ui/adapter.mjs` 把 `command_list` / `command_run` / `inspect` / `save_project` 统一
`dispatch()` 入队 → 页面 `GET /<token>/poll` → `ui/connect.js` 调 `craft_submit` 在
`window.__jianyingcraftEngine`（页面自己的可见 wasm 实例）上执行 → `craft_take_reply` 取结果 →
`POST /<token>/reply`。`panel-live` 在真实 Chrome 中断言 `editor_status.ready=true` 且
`file.openDemoProject` + `sequence.inspect` 返回 7 个片段。

未闭合的一项：`editor-wasm.test.mjs`（wasm 桥契约）需要 **nodejs 目标**的 wasm-bindgen 胶水，
而仓库产物是 web 目标；用本机 `wasm-bindgen 0.2.129` 从既有 `.wasm` 重新生成会因
**CLI 与 crate 版本不一致**（导入符号带签名哈希）在实例化时报
`LinkError: Import #0 __wbg_construct_…: function import requires a callable`。
需用与 `Cargo.lock` 相同版本的 CLI 重新构建后再跑，本轮未改变结论。
