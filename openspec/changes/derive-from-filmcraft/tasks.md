# 任务清单

基线：filmcraft 0.8.0 / HEAD `09aaf9bf`（MIT OR Apache-2.0）。

## 0. 阻塞与合规缺口（最高优先级）

- [x] **删除 `docs/brand/`**：已执行 `git rm -r docs/brand`（8 个商标文件 + 边车 +
      `LICENSE-brand.txt`），并同步改写 `NOTICE`、`ATTRIBUTION.md`、`AGENTS.md`、`README.md`、
      `brand.rs` / `links.rs` / `header.rs` / `menus.rs` / `dialogs.rs` / `import_mode.rs` /
      `native_menu.rs` 与 11 个 i18n 目录（移除 Discord / getartcraft 条目共 74 行）。
- [x] **品牌门禁进 CI**（2026-10-19）：新增 `cargo xtask brand`，并在 `ci()` 里排在
      `assets()` 之后。规则：扫描 git 跟踪的代码与配置（跳过 `.md`/`.txt`/`.attribution`/`assets/`），
      出现上游名 `FilmCraft` 即失败；白名单只留"必须说明派生自谁"的地方
      （`NOTICE`/`ATTRIBUTION.md`/`LICENSE`/`docs/`/`openspec/`/`jianyingcraft-plugin/`/
      i18n 的 derived-from 文案/旧工程 fixture/`packaging/`）。
      顺带把 15 个 `Cargo.toml` 的 description、FreeBSD/release/arm64 的 workflow、
      GPU shader 注释、OTIO 的 `generator_kind` 从 FilmCraft 改成 JianyingCraft。
      实测 **exit 0**；`packaging/` 仍指向上游 app id，替换需单独的发行决策（已列入待办）
- [x] **自有图标（第一版）**（2026-10-19）：`assets/brand/jianyingcraft.svg` +
      `jianyingcraft-1024.png` + `make-icon.py`（几何由脚本计算，原创、无第三方图稿），
      三段竖条 + 时间轴基线的抽象标记；Web 端 `favicon.png` 换成同源 128 px 渲染。
      sidecar ×3 与 `ATTRIBUTION.md` 三行已补（注意：`Set-Content -Encoding utf8` 在
      本机会写 BOM，`xtask assets` 会报 "field `asset` missing"，改用
      `[IO.File]::WriteAllText(..., UTF8Encoding($false))`）。`cargo xtask assets` **ok（63）**，
      `cargo xtask web` 后 `target/web/dist/favicon.png` 与源文件 sha256 一致
- [x] **桌面端图标全部换新**（2026-10-19）：`assets/brand/make-icon.py` 一次产出
      `jianyingcraft-1024.png` / `-512.png` / `.ico`（16–256，9 档）/ `.icns`（16–1024）与
      Linux hicolor 全尺寸；`apps/filmcraft/build.rs` 嵌入新 `.ico`、
      `main.rs` 的 macOS 运行时图标指向 `-512.png`；`packaging/icons.sh` 改为调用 Python 脚本
      （不再需要 resvg/iconutil）。上游 6 个图标文件与 sidecar 已删除，ATTRIBUTION 同步。
      验证：`cargo xtask assets` **ok（61）**、`cargo xtask brand` ok、`cargo check -p filmcraft` **0**
- [x] **发行标识迁移**（2026-10-19）：`ai.storyteller.filmcraft` → `ai.jianyingcraft`，
      共改 23 个文件（desktop / metainfo / mime / flatpak ×2 / hicolor 9 / sidecar）与 28 处内容；
      macOS `CFBundleIdentifier`、`CFBundleName/DisplayName/Executable/IconFile`、`.app` 名、
      DMG 卷名、Windows MSI `Manufacturer`、AppStream developer id 一并换成本仓库；
      `application/x-filmcraft` 这个 MIME **保持不变**（改它会断掉已有工程的文件关联）。
      验证：`cargo xtask assets` **ok（61）**、`cargo xtask brand` ok、`cargo check -p filmcraft` **0**、
      `bash -n` 6 个打包脚本全 0、metainfo/mime/wxs 的 XML 解析通过
- [ ] `packaging/macos/dmg/dmg-layout.DS_Store` 是二进制，里面仍是 `FilmCraft.app` 的位置信息；
      需要 macOS（或装了 resvg + ds_store + mac_alias）上跑
      `python3 packaging/macos/dmg/generate.py` 重生成
- [x] **构建环境**：`pwsh` 启动即失败（`SetNamedSecurityInfoW failed (Win32 5)`），
      无法 `git`/`cargo`/`wasm-bindgen`；`craft-ui` 为 git 依赖且本机 GitHub DNS 不可达。
      二者不解决，§2 全部无法执行。
      *2026-10-20：该条已过期——本机 PowerShell 5.1.26100 / git 2.55.0 / cargo 1.99.0 /
      wasm-bindgen 0.2.129 均可用；`craft-ui` 的 git 依赖（`storytold/craft-libs` @ 5c874d51）
      已在本地 cargo registry 缓存中，`cargo build/clippy/test` 全部可跑；§2 各条在本轮之前
      已实际执行并产出证据。*

## 1. 派生骨架（已完成）

- [x] 从 filmcraft 本地克隆派生 `jianyingcraft`，保留 `fork`（离线基线）与 `upstream`（storytold/filmcraft）双远程
- [x] 建分支 `feature/jianyingcraft-derivation`
- [x] 关闭品牌图片依赖：`brand.rs` 改为文字字标，不再加载 ArtCraft 商标
- [x] 显示名全局替换（crates/apps 的 .rs/.tsv/.html 共 152 个文件，仅 `\bFilmCraft\b`）
- [x] 帮助链接标注 "(upstream)"，i18n 中文值同步
- [x] `openspec/project.md` 与派生变更四件套
- [x] 派生设计落地文档：`docs/derivation/02-ia-mapping.md`（剪映式 IA）、`docs/derivation/03-panel-bridge.md`（面板桥契约）

## 2. 构建验证（阻塞）

- [x] `cargo check -p filmcraft-ui-egui` 通过（2026-10-19，`--locked`，exit 0）
- [x] `cargo test --workspace --lib` 通过（2026-10-19，`-j 1 --no-fail-fast`，exit 0）
- [x] `cargo xtask web` 产出 wasm：`cargo xtask wasm` **ok（49 crates）**；
      wasm-bindgen 0.2.129 生成的 node 目标绑定也跑通了插件桥契约（701 命令）
- [x] 桌面壳冒烟：窗口标题为 JianyingCraft（`browser-app-smoke.mjs` 截图复核）

> ~~阻塞：沙箱 shell 不可用 + `craft-ui` git 依赖不可达~~ —— **2026-10-19 已解除**：
> 构建、测试、wasm、插件全链路均在本机实测通过；此条保留为历史记录。

## 3. JianyingCraft 产品化

- [x] **竖屏短视频预设**（2026-10-19）：New Sequence 弹窗的 Video 区新增
      `Presets:` 一行——`16:9 · 1920×1080 · 30fps`、`9:16 · 1080×1920 · 30fps`、
      `1:1 · 1080×1080 · 30fps`，点击一次同时改宽高与帧率（`apply_aspect_preset`，
      drop-frame 跟随帧率）。单元测试 `preset_tests::aspect_presets_set_size_and_rate`，
      入口渲染测试 `sequence_ui::new_sequence_offers_aspect_presets`；截图
      `_ref/ui-snapshots/new-sequence-presets.png`。
      说明：**没有新增命令**（"新建短视频工程"仍走 `file.newSequence` + 参数），
      符合"不改命令面"的约束；点击后的状态变化只由单元测试覆盖，因为
      `egui::Window` 在该 harness 下收不到合成点击（连 Cancel 都不响应，已写入测试注释）
- [ ] 若要做"短视频工程"一键命令：需要新增一条 engine 命令（会扩命令面），待确认
- [x] 默认语言 zh-cn：首次启动偏好为 `system`，经 `hooks.system_languages` →
      `Language::from_locales` 解析（中文系统即 `zh-cn`；无宿主 hook 时回落 `En`），
      由 `i18n::tests::system_language_preference_uses_the_host_languages` 覆盖
- [x] 单一断点入口：`crates/ui-egui/src/layout.rs` 的 `LayoutClass::from_width`（Compact ≤767 /
      Medium 768–1024 / Wide >1024；非有限或非正宽度回落 Compact）；`ui.inspect` 暴露
      `layoutClass`；6 条单测 + `tests/layout_ui.rs` 三档视口验收
- [x] Compact 主区骨架：`dock_ui::compact_area`（顶部条 + 全宽预览 + 底部时间轴；时间轴 ≥220 px、
      不超过可用高度 55%，三区不重叠；自动化 id `jy.topbar` / `jy.preview` / `jy.timeline`），
      由 `tests/layout_ui.rs::compact_shell_stacks_preview_over_timeline` 断言
- [x] Compact 顶部条：隐藏 Premiere 式全局菜单条（`show_header && !compact`），改由
      `compact_area` 画产品名 + 素材/属性两个抽屉开关（`jy.tab.media` / `jy.tab.inspector`）
- [x] Compact 抽屉：`UiState.jy_drawer`（`layout::Drawer`）承载素材/属性；打开时只压缩预览区，
      时间轴位置与高度不变，`sequence.inspect` 不变（`tests/layout_ui.rs::compact_drawer_compresses_preview_only`）
- [x] Compact 抽屉**可以滚动**（更正此前"内容被裁"的判断）：
      `tests/layout_ui.rs::compact_drawer_scrolls_its_panel` 在抽屉中心滚 -260，
      断言 `project.bin.*` 的 y 坐标变化。之前看到"bins 被切"是因为 egui 浮动滚动条平时不显示，
      不是内容不可达；是否改成常显滚动条待设计确认
- [x] Compact 底部工具条：撤销 / 重做 / 分割 / 删除 / 播放（`jy.tool.undo|redo|split|delete|play`），
      只转发命令（`edit.undo`、`edit.redo`、`sequence.addEdit`、`edit.clear`、`playback.toggle`），
      不新增命令面；`tests/layout_ui.rs::compact_toolbar_splits_at_the_playhead_and_undoes`
      断言"播放头切一刀后视频片段 +1、撤销后还原"，并截图 `layout-compact-toolbar.png`
- [x] Compact 一级 Tab：`Edit` / `Text` / `Audio`（`jy.tab.cut|captions|audio`），分别打开
      `Project` / `Text` / `AudioTrackMixer` 抽屉；当前 Tab 在 `ui.inspect` 的 `ui.jy_tab` 可见；
      再点当前 Tab 收起（`tests/layout_ui.rs::compact_tabs_switch_the_drawer`）
- [x] **移动端顶栏改成剪映式可横滑的 13 个一级 Tab**（2026-10-19）：素材/文本/音频/贴纸/特效/转场/
      字幕/智能包装/滤镜/调节/模板/数字人/插件，横向 `ScrollArea`；前三个保留旧 id 以维持状态契约，
      其余 `jy.tab.<slug>`；点哪个 Tab 抽屉就画哪个面板（`UiState.jy_shell_panel`），
      未实现的 4 个灰显不可点。`tests/layout_ui.rs::compact_top_bar_scrolls_through_the_jianying_tabs`
      与 `compact_tab_bar_disables_unimplemented_modules`；截图 `layout-compact-tabs.png`
- [x] **空态提示改为叠加**（2026-10-19）：全量 UI 套件（82 个测试目标）跑出一个回归——
      `project_panel_ui::double_click_empty_area_imports_with_a_project_loaded` 找不到
      `project.empty`。原因是最早的实现用提示**替换**了图标网格，空工程的拖放/双击导入区随之消失。
      改为 `body()` 照常绘制、提示叠加在上层；`project.empty` 保留。
      全量复跑 **82 targets / 0 failed**，`clippy`/`fmt` 均 0。- [x] **空工程提示**（2026-10-19）：Project 面板在"既没有素材也没有文件夹"时画
      "把素材拖到这里，开始你的大作吧 / 或点 rail 上的「导入」选择文件"，
      自动化 id 用 `project.emptyHint`（`project.empty` 已被图标网格空白区占用，曾撞过一次）。
      `tests/layout_ui.rs::empty_project_shows_the_drop_hint` 覆盖"有素材→无提示、新工程→有提示、
      重新打开 demo→提示消失"；截图 `layout-empty-project.png`
- [x] **空文件夹提示**（2026-10-19）：工程有素材、当前文件夹为空时画
      "「<名称>」是空的 / 把素材拖进来，或从其他文件夹移入"，id `project.emptyBinHint`。
      为可测性新增控制通道方法 `ui.project.bin {bin}`（等价双击进入文件夹，已写进
      `docs/control-protocol.md`）。`tests/layout_ui.rs::empty_bin_shows_its_own_hint`；截图 `layout-empty-bin.png`
- [x] Medium 档：768–1024 也走单页壳（`shell_area`），抽屉改为右侧 240 px 侧栏、时间轴下限
      240 px；`tests/layout_ui.rs::medium_layout_uses_a_right_sidebar` 断言侧栏贴预览右缘、
      与时间轴不重叠、时间轴矩形不变
- [x] **Medium 档也有时间轴工具条**（2026-10-19）：36 px，7 个按钮；
      `tests/layout_ui.rs::medium_has_the_timeline_toolbar` 断言按钮齐全、分割在播放头 +1、撤销还原；
      截图 `layout-medium-toolbar.png`（900×1024）
- [x] ☰ 菜单：`jy.menu` 打开可滚动命令列表（`jy.menu.panel`、`jy.menu.<id>`），
      Compact/Medium 只保留一条顶栏、Wide 仍保留全局菜单条
      （`tests/layout_ui.rs::shell_menu_replaces_the_global_menu_bar`）
- [x] ☰ 菜单过滤框：`jy.menu.filter` 单行输入，按"菜单路径 + 标签"子串过滤（忽略大小写）；
      `tests/layout_ui.rs::shell_menu_filters_commands` 断言过滤后行数变少且每行都匹配
- [x] ☰ 两级菜单：`jy_menu_group` 记住展开的菜单根，根列表显示 `File (51)` 这类计数
      （实测 9 个根：File 51 / Edit 66 / Clip 49 / Sequence 46 / Markers 30 /
      Graphics and Titles 43 / View 50 / Window 43 / Help 6），点进去显示该根的命令并给
      `< 返回`；有过滤词时退回扁平结果。
      `tests/layout_ui.rs::shell_menu_drills_into_a_menu_root` 断言"根列表 → 子项 → 返回"，截图 `shell-menu-roots`
- [x] 分组视图是悬浮二级面板（不再整页替换）；分隔符刻意用 ASCII，
      因为 `▸` 在实测字体里渲染成豆腐块
      *2026-10-20：`tests/panel_float_ui.rs::grouped_menu_opens_a_floating_second_level_panel` 断言
      分组面板宽 < 90%、高 < 75%、面积 < 半屏且不出界，返回后根列表仍在。*
- [x] Wide 档：新增内置工作区 `Jianying`（左素材 / 中预览 / 右属性+关键帧 / 底时间轴），
      `WORKSPACES` 由 9 项增至 10 项，11 个语言目录补 `Jianying` 词条；
      `tests/layout_ui.rs::wide_default_workspace_is_the_four_regions` 断言四区存在、
      不含 Source/Tools、Project 在 Program 左侧。**默认工作区仍是上游 `Editing`**（可切换）
- [x] **右上角「导出」按钮**（2026-10-19）：Tab 条右侧 72×28 的强调色按钮，
      调 `file.exportMedia`（无 path）→ 引擎/菜单层把它变成 `Mode::Export`。
      `tests/layout_ui.rs::jianying_workspace_has_a_prominent_export_button` 断言
      `ui.inspect` 的 `ui.mode` 变成 `"Export"`；截图 `wide-export-button.png`
- [x] **移动端顶栏也有「导出」**（2026-10-19）：52×28，固定在右上角，「属性」按钮左移让位；
      Tab 条的可用宽度相应收窄（预留 160 px）。`tests/layout_ui.rs::compact_top_bar_has_the_export_button`
      断言按钮 x 贴右缘、点击后 `ui.mode == "Export"`；截图 `compact-export.png`
- [x] **被裁掉的 Tab 可以滚进可视区**（2026-10-19）：新增控制方法 `ui.shell.tab {tab}`——
      切面板 + 设置 `UiState.jy_tab_scroll`，Tab 条本帧 `scroll_to_rect` 把目标滚到中间；
      `SHELL_TABS` 常量化，避免两处 mapping 漂移。`tests/layout_ui.rs::shell_tab_command_scrolls_a_clipped_tab_into_view`
      断言"特效"从裁切区（x>222）滚到更左，且抽屉出现 `panel.Effects`；已写进 `docs/control-protocol.md`
- [x] **一级 Tab 左右循环**（2026-10-19）：`shell_tab_step(current, ±1)` 跳过未实现的 4 个并首尾相接；
      控制方法 `ui.shell.tab.step {step}` 暴露同一逻辑，顶栏里 `Alt+←/→` 调它。
      `tests/layout_ui.rs::shell_tabs_cycle_with_left_right`：素材→文本（面板确实换成 Text），
      左键回素材；从"调节"继续右移要绕回"素材"（跳过智能包装/模板/数字人/插件）
- [x] **对标剪映专业版的一级 Tab 条**（Wide + `Jianying` 工作区，42 px）：
      13 个 Tab = 素材 / 音频 / 文本 / 贴纸 / 特效 / 转场 / 字幕 / 智能包装 / 滤镜 / 调节 /
      模板 / 数字人 / 插件。有面板的 Tab 调 `show_panel` 真导航（素材→Project、文本/字幕→Text、
      贴纸→EssentialGraphics、特效/转场→Effects、滤镜/调节→LumetriColor）；
      **未实现的 4 个（智能包装/模板/数字人/插件）画灰且不可点**，不摆假入口。
      `tests/layout_ui.rs::jianying_workspace_shows_the_tab_strip` 断言 ≥12 个 Tab 且点特效后
      `panel.Effects` 出现；截图 `wide-tabstrip`
- [x] **"音频"Tab 真的过滤素材**（2026-10-19）：`UiState.jy_media_filter` 被 Project 的
      `matches_filter` 读取——点"音频"只列音频素材，点"素材"恢复全部（两档 Tab 条都生效）。
      `tests/layout_ui.rs::audio_tab_filters_the_project_to_audio_media`：
      Footage bin 6 条 → 音频 Tab 后 0 条（都是视频）→ 素材 Tab 恢复 6 条
- [ ] Tab 的**内容**仍不全：贴纸/模板/数字人/插件没有素材库（云生态，不做）；
      目前只有"音频"做了类型过滤，"贴纸"仍只是跳到 EssentialGraphics 面板
- [x] **右栏"草稿参数"**（未选中片段时）：名称 / 分辨率 / 帧率 / 时长(帧) / 轨道 V·A / 素材数，
      数据来自 `sequence.inspect` + `project.inspect`（查询命令，只读）。实测 demo：
      `Main Edit` · `1920 x 1080` · `23.98 fps` · `642 帧` · `V3 / A3` · `12 项`。
      `tests/layout_ui.rs::jianying_workspace_shows_draft_parameters_when_nothing_is_selected`
- [x] **时间轴工具条**（Wide `Jianying` 工作区）：撤销 / 重做 / 分割 / 删除 / 播放 / 定格 / 倒放，
      挂在 Timeline 面板顶部 38 px；定格=`clip.frameHold`、倒放=`clip.speedDuration{reverse:true}`。
      `tests/layout_ui.rs::jianying_workspace_puts_the_toolbar_on_the_timeline` 断言分割 +1、撤销还原
- [x] **工具条按钮跟随命令启用条件**（2026-10-19）：每个按钮先问
      `find_command(cmd).enabled(session)`，再叠加"需要选区"的判断（删除/定格/倒放）；
      不可用时画灰且不接点击。`tests/layout_ui.rs::toolbar_buttons_follow_command_enablement`：
      无选区点删除**不删**，选中后点删除 -1，撤销还原
- [x] **播放中按钮变"暂停"**（2026-10-19）：同一颗按钮按 `playback.playing` 换文案，
      命令始终是 `playback.toggle`。`tests/layout_ui.rs::play_button_switches_to_pause_while_playing`：
      初始"播放"→ 点击后"暂停"→ 再点回"播放"；截图 `layout-wide-toolbar-playing.png`
- [x] **左栏竖向 rail**（Wide `Jianying` 工作区，56 px）：导入 / 素材 / 子草稿 / 我的 /
      AI / 云素材 / 官方 / 即梦 八个入口；**导入**发 `file.import`、**素材**调 `show_panel(Project)`，
      其余 6 个在本仓库没有对应实现，画灰不可点。
      `tests/layout_ui.rs::jianying_workspace_has_the_left_rail` 断言八个入口存在、素材可切换面板
- [ ] 子草稿 / 我的 / AI / 云素材 / 官方 / 即梦：需要先有对应能力（草稿库、账号、云），当前明确不实现
- [x] **首次启动默认进 `Jianying` 工作区**（`apps/filmcraft/src/main.rs::first_run_workspace`）：
      判据是"数据目录里还没有 `workspaces.json`"——全新安装得到剪映式首屏，老用户保留上次选择；
      测试 `tests::first_run_lands_in_the_jianying_workspace` 覆盖三种情况（首次 / 已有文件 / 无配置目录）。
      选择在 app 层而不是改 `UiState::default`，避免影响直接构造 `FilmcraftApp` 的上游测试
- [x] 单页壳不绘制浮动面板：当前一旦存在浮动窗口就退回 dock 树（用户显式打开的浮动监视器
      不能因窗口变窄而消失）；后续应把浮动层画在壳之上
      *2026-10-20：浮动层改为画在壳之上——壳照常绘制（预览/时间轴仍在），浮动监视器叠在上层，
      `tests/panel_float_ui.rs` 3 条通过（☰ 分组二级面板是浮动而非整页替换、Compact 壳 + 浮动监视器共存且不出界、
      打开「滤镜」Lumetri Color 不再盖住 Program 监视器）。*

## 6. CI 实测（2026-10-18）

| 门禁 | 结果 |
|---|---|
| `cargo xtask layers` | **ok** |
| `cargo xtask wasm` | **ok（49 crates）** |
| `cargo xtask assets` | **ok（61 assets）**；生成的截图已移出仓库（`_ref/ui-snapshots`） |
| `cargo xtask brand`（2026-10-19 新增） | **ok**：代码/配置里不再出现上游名，白名单只留派生说明、旧 fixture 与打包脚本 |
| `cargo xtask fmt`（2026-10-19 起） | **ok**：整工作区形式在 Windows 会 `os error 206`，xtask 自动退化为逐包 `cargo fmt -p <pkg> --check`（53 包，语义相同） |
| `cargo fmt -p <pkg> --check` × 54 包 | **0 失败** |
| `cargo clippy -p filmcraft-ui-egui --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p filmcraft-ui-egui`（lib + 全部集成目标） | 全绿（timecode_ui 的浮动监视器回归已修复） |
| `cargo test --workspace`（全目标） | 未跑完：构建阶段 55 分钟仍在编译依赖，随后改为只跑 lib（见下一行） |
| `cargo test --workspace --lib -j 1`（**2026-10-19**） | **全绿**：54 包 lib 单测 0 失败；引擎 639、导出 119、UI 205 均通过 |
| 集成测试分批（`-p X --tests`，2026-10-19） | **10 个核心包全绿**：export 33s、interchange 244s、media 42s、captions 137s、text 20s、frame 52s、format 83s、render 153s、platform 125s、cli 32s；0 failed |
| 未跑：编解码包集成测试 | `h264/hevc/av1/vp9/vp8/prores/dnx/aac/...` 等上游未改动包，留给上游 CI；本 fork 的改动集中在 UI/导出/品牌 |
| 插件侧 `node --test`（2026-10-19 复跑） | **44 tests / 44 pass / 0 fail / 0 skip**（`CHROME_PATH` + `JIANYINGCRAFT_WASM_JS` 两个前置都置好） |
| 插件 `skill-smoke`（`FILMCRAFT_CLI` + 真实导出） | captions 2 条、batch-remix 2 段 / 17.98 MB、text-to-video 1080×1920 / 450 帧 —— 全 `ok` |

> **2026-10-19 由全量测试抓到的真实崩溃**：DPX 导出在 `crates/export/src/raster.rs` 把 13 字节的
> `"JianyingCraft"` 复制进 9 字节切片（上游名字 `"FilmCraft"` 正好 9 字节），`copy_from_slice` 直接
> panic，导致 `filmcraft-export` 2 条测试与引擎的 `every_builtin_preset_exports...` 失败。
> 改成按 creator 字段（100 字节）写入后，`filmcraft-export` **119 passed**、引擎预设导出测试通过、
> `cargo test --workspace --lib` **exit 0**。这是品牌替换引入的崩溃，属于上游 §0 "never crash" 红线。

> **2026-10-19 fmt 门禁修复**：`cargo xtask ci` 第一步 `cargo fmt --check` 在 Windows 上必挂
> （`os error 206`：命令行超长，rustfmt 调用方式的工具限制）。新增 `xtask::fmt_check()`：
> 识别到该错误就退化为逐包 `cargo fmt -p <pkg> --check`（53 包），其它失败原样上抛；
> 同时加了独立子命令 `cargo xtask fmt`。实测 **exit 0**，`cargo xtask layers`（ok）与
> `cargo xtask assets`（61 assets）也复跑通过。

> **2026-10-18 环境事故**：`target/debug/incremental` 涨到 **19.9 GB**，E: 盘剩余 0 字节，
> 导致 `cargo clippy --all-targets` 报 `E0463`（不是代码错误）。删除增量缓存后恢复 13.1 GB，
> clippy 与全量测试复跑通过。以后在 Windows 上跑全量测试前先看 `Get-PSDrive E`。
- [x] 快照模式稳定性：`FILMCRAFT_UI_SNAPSHOT_DIR` 下同一测试进程创建多个 wgpu harness 会
      偶发 `STATUS_ACCESS_VIOLATION`（截图已产出，但进程崩溃）；需改为单 harness 复用或按测试隔离
      *2026-10-20：根因是同进程多个 wgpu harness 并发；按测试串行后稳定（`--test-threads=1` 复跑 29/29，
      26 张 PNG 全部产出）。现在有唯一入口 `cargo xtask snapshots [--skip-run]`：解析绝对目录
      （尊重 `CARGO_TARGET_DIR`）、先删旧图、串行跑 `layout_ui`、校验
      `layout-{mobile,tablet,desktop}.png` 与 `layout-medium-sidebar.png` 存在，输出目录/张数/耗时。
      测试侧的 `FILMCRAFT_UI_SNAPSHOT_DIR` 相对路径也改为按工作区根解析（此前会写出
      `crates/ui-egui/crates/ui-egui/target/ui-snapshots/` 的嵌套垃圾目录）。*
- [x] 顶部 3 Tab（剪辑/字幕/音频），切换不中断播放、不清选区
      *2026-10-20：顶栏只横排 3 个主 Tab（沿用 `jy.tab.cut`/`jy.tab.captions`/`jy.tab.audio` 旧 id），
      其余 10 个收进 `jy.tab.more`「更多」菜单，仍可点开、仍有命令与自动化 id；未实现模块保持灰显。
      契约由 `tests/tab_switch_ui.rs` 2 条与 `tests/layout_ui.rs::top_bar_shows_three_primary_tabs_plus_more`
      覆盖。
- [x] 右侧"选中即属性"：基础/变速/动画/音频/调色 分组折叠
      （2026-10-20：`properties_groups_ui` 2 条通过——五个 `props:<Group>` 标题、点击折叠/展开、
      任一组折叠不影响其它组；位置见 `crates/ui-egui/src/panels/effect_controls.rs::properties_panel`）
- [x] 底部时间轴：轨道头 ≤96 px、主轨磁吸、字幕轨置顶
      （2026-10-20：`timeline_layout_ui` 6 条通过——V1/A1 与字幕轨头 ≤96、字幕轨在视频轨之上、
      相邻片段边缘吸附、V1 竖直/时间双磁吸；剪映工作区生效，Premiere 工作区保留原宽度）
- [x] 左侧素材宫格：筛选/搜索/拖入导入
      （2026-10-20：`project_media_grid_ui` 3 条通过——搜索框过滤并清空还原、全部/视频/音频/图像
      类型筛选、OS 拖入文件经 `handle_drops` 落为 1 个素材项）
- [x] 三档视口验收 IA-1..IA-5（见 `docs/derivation/02-ia-mapping.md` §5）
      *2026-10-20：IA-1..IA-5 全部由 `tests/ia_acceptance_ui.rs`（9 条，无 `#[ignore]`）与
      `tests/layout_ui.rs`（28 条）覆盖并通过。IA-2 的真因是**同一窗口算了两遍宽度**：
      `frame` 用窗口宽度选档，`dock_area` 又用内缩 1 px 的 body 重算（768 → 766 落入 Compact），
      与 `ui.inspect.layoutClass` 打架；现在每帧只算一次 `LayoutClass` 并传给所有消费方。
      IA-1 的「0 次页面切换」在 `02-ia-mapping.md` §5 由 2026-10-20 的注记定义为「同一单窗口工作区」
      （导出进入窗口内 Export 屏幕，不算换页）。截图仍受 §231 的 wgpu 多 harness 崩溃影响，
      三档 PNG 不能在同一进程里一次产出。*
- [x] 剪映功能/界面参考：`docs/derivation/09-jianying-reference.md`
      （本机 11.6.0.14552；PO 文案 18,093 条、Resources 60+ 模块、与 701 条命令面逐项对照）
- [x] 代理 UI：已具备（`project_views` 的 proxy/offline 徽标、`View ▸ Toggle Proxies`、`Clip ▸ Proxy` 的创建/附加/分离/重连命令与预设、`ProxyDraft` 对话框）——此前的"未暴露"结论已过期
- [ ] 仍缺：**变声（pitch）无对应命令**（需要音频 DSP 命令，属于扩命令面）；本地模板库未做
      （数字人/云素材/AI 生成/支付明确不做）
- [x] 自有品牌资产（logo/图标）替换 favicon 与关于页：`assets/brand/` 生成的 favicon 已进 web 产物（`apps/filmcraft-web/web/favicon.png`），关于页使用自有名称与链接
- [x] 自有帮助页与 GitHub 链接：`links.rs` 只指向本仓仓库与 issue tracker，上游社区链接刻意不出现（原因是上游品牌许可）

## 4. 面板桥（智能体宿主内嵌）

- [x] 四导出契约落地：submit / take_reply / idle / catalog（细节见 `docs/derivation/03-panel-bridge.md` §2、§10、§15）。
      实现分布在 `crates/ui-egui/src/bridge.rs`（`Bridge` 校信封 / `InFlight` 在途记账 / `catalog()` 目录）
      与 `apps/filmcraft-web/src/api.rs`（四个 `#[wasm_bindgen]` 导出，`craft_take_reply` 空队列返回 `null`）。
      *原文引用的 `bridge.rs.in` 在本仓与插件仓都不存在（只存在于 10 个兄弟插件仓的通用模板，且契约不同），已更正为上述实现位置。*
      *2026-10-20 审计修复的两处真实偏差：① 累计 8 MiB 此前只在单条请求上校验（`api.rs` 每次 submit 即排空 `Bridge`，
      在途请求不带字节数，32 × 8 MiB 可堆积）→ 新增宿主可测的 `InFlight` 记账，同时约束 32 条与累计 8 MiB；
      ② `craft_take_reply` 空队列返回 `undefined` → 改为 `JsValue::NULL`。五条新单测覆盖累计预算、第 33 条被拒、
      空队列、回滚释放、取队首；`bridge_ui::craft_catalog_ids_match_command_list` 断言
      `catalog` / `command.list` / `command_specs()` 三方 id 集合相等（T1）。*
- [x] 宿主三导入：`__craftRead` / `__craftWrite` / `__craftScreenshot`
      （2026-10-20：`apps/filmcraft-web/src/host_policy.rs` 承载平台无关策略，`host.rs` 是 wasm 胶水并导出
      `craft_read` / `craft_write` / `craft_screenshot`。11 条宿主单测实测：`../` 与绝对路径越界、UNC/盘符相对、
      junction/symlink 逃逸（本机真实 junction）、导入配额 256 文件 / 128 MiB、单产物 128 MiB、临时文件 + 原子改名、
      覆盖回执、截图不支持返回 None。仅类型检查：wasm 侧对 `__craft*` 的调用与回包映射——本机没有注入这三个全局的宿主页。）
- [x] 引擎侧面板桥队列：`crates/ui-egui/src/bridge.rs`（32 条 / 8 MiB、FIFO、失败也回包、
      `idle()`、`rejected()`），7 条单测覆盖非法载荷与上限
- [x] UI 线程执行：`FilmcraftApp::pump_bridge` 每帧把请求执行在可见 App 上；
      `tests/bridge_ui.rs` 三条端到端（副作用可见、错误回传、第 33 条被拒且前 32 条各回一包）
- [x] wasm 四导出已实现并**通过 wasm32 类型检查**（`cargo check -p filmcraft-web
      --target wasm32-unknown-unknown` exit 0）：
      `api::craft_submit` / `craft_take_reply` / `craft_idle` / `craft_catalog`
      （解析、32 条上限、回复信封复用 `Bridge`；执行走现有 `ControlRequest` 通道）
- [x] 四导出**运行验证通过**（2026-10-18）：
      `cargo xtask web` 产出 40.8 MB wasm + 182 KB glue；
      `WebAssembly.Module.exports` 含 `craft_catalog/craft_idle/craft_submit/craft_take_reply`；
      用 `wasm-bindgen --target nodejs` 在 Node 里真实调用：`craft_catalog()` = **701 条命令**、
      `craft_submit(合法)` = true、`craft_take_reply()` 返回
      `{"id":"t-1","ok":false,"error":{"code":"command_failed","message":"JianyingCraft is not running yet"}}`、
      `craft_idle()` = true、`craft_submit("not json")` = false
      *2026-10-20 复核（新构建）：`cargo xtask web` 后从 **cargo 原始 wasm** 生成 nodejs 胶水，
      插件仓 `node --test tests/editor-wasm.test.mjs` 通过（catalog 701 条、submit true、
      未启动时回包 `command_failed`、idle true、非法输入 false）。此前「CLI 与 crate 版本不一致」
      的判断是错的：真因是把已处理的 `_bg.wasm` 又喂回 wasm-bindgen；步骤已写进 `docs/web.md`。*
- [x] 跨仓接线与契约测试（插件仓）：
      `ui/editor-boot.mjs` 把 `craft_*` 包成稳定接口（形状校验、回复解析、超时），
      `ui/tests/editor-boot.test.mjs` 8 条单测；`ui/tests/editor-wasm.test.mjs` 直接加载
      派生引擎的 node 版 wasm：catalog=701、submit 受理、失败回包 `command_failed`、
      idle=true、畸形载荷在进引擎前被拒（无产物时 **skip 而非假通过**）
- [x] 编辑器面板页成为**第二个 MCP App 资源**（2026-10-18）：
      `ui/panel-editor.html` + `panel-assets.mjs::editorHtml` 把 `editor-boot.mjs` 内联成单文档
      （`export` 语法在 inline module 里会坏，已剥离）；adapter `resources/list` 返回
      `panel.html` 与 `editor.html` 两个资源，`resources/read` 实测返回 5,298 字节 HTML，
      含 `#editor-canvas`、内联 `createEditorBridge`、启动段与就绪标志
- [x] 引擎产物同步与校验：`ui/sync-engine.mjs`（`--check` 检 drift：缺文件/多文件/哈希不同），
      6 条单测；实测把 `cargo xtask web` 的 **5 个文件 / 40,969,658 字节**同步进 `ui/engine/`，
      `--check` 返回 `drift: []`；`ui/engine/` 已加入 `.gitignore`（构建产物不进 git）
- [x] **更正**：`cargo xtask wasm` **已经**覆盖 `filmcraft-web`
      （`WEB_CRATES = ["filmcraft-ui-egui", "filmcraft-web"]`；实跑输出含 `Checking filmcraft-web`）。
      上一轮"只覆盖 L0–L4"的说法是错的
- [x] **真实浏览器验证通过**（2026-10-18，headless Chromium）：
      `ui/browser-smoke.mjs` 起本地静态服务器 → Chromium 加载编辑器面板页 →
      读出 `{"engineReady":true,"bridgeReady":true,"commands":701}`；
      未设 `CHROME_PATH` 时 skip（退出 0，不假装通过）
- [x] **更正：eframe 在 headless Chromium 里能启动**（2026-10-19）。之前判定"停在 Loading"
      是 `--dump-dom` 的虚拟时间截断了异步启动，不是渲染失败。实测启动链
      `import → init → start` 全部成功（`{"ok":true,"imported":true,"initialised":true,"started":true}`），
      且 `window.filmcraft` 控制通道可用
- [x] **浏览器内执行真实命令**（2026-10-19）：`ui/browser-app-smoke.mjs` 在真实 Chromium 里
      打开 demo（`{sequence:21}`）→ `sequence.inspect`（3 条视频轨 / 642 帧 / 7 个视频片段）→
      `sequence.addEdit`（`cuts:3`，视频片段 7→8）→ `edit.undo`（`undone:"Add Edit"`，回到 7）。
      未设 `CHROME_PATH`/`JIANYINGCRAFT_WEB_DIST` 时 skip（退出 0）
- [x] **剪映式外壳在 wasm/Web 下同样成立**（2026-10-19）：`cargo xtask wasm` ok（49 crates）；
      `cargo xtask web` 重新产出 40.8 MB wasm；`ui/sync-engine.mjs` 同步 5 文件 / 40,990,095 字节，
      `--check` drift `[]`；浏览器冒烟 `{"engineReady":true,"bridgeReady":true,"commands":701}`
- [x] 三档截图归档（`_ref/ui-snapshots/`）：`layout-compact-toolbar.png`（390×884）、
      `layout-medium-sidebar.png`（768×1024）、`layout-wide-toolbar.png` / `layout-wide-draft-parameters.png`
      / `layout-wide-left-rail.png`（1280×1024）
- [x] **插件 MCP 运行时已验证**（`npm ci` → 102 包；2026-10-18）：
      `initialize` 返回 `jianyingcraft-editor 0.1.0`；`tools/list` 返回 **17 个工具**
      （14 公开 + 3 `_panel_*`），与 `dist/tools.json` 完全一致；无面板时
      `editor_status` / `command_run` 均 `isError:true` + "Call open_editor first"（fail-closed）
- [x] **adapter ↔ 浏览器面板的最后一公里**（2026-10-19 调查）：adapter 其实**已经有**
      `/<token>/poll`、`/<token>/reply` 两个 HTTP 端点（与 `_panel_*` 同一套队列），
      `/app/` 也由它服务真实 web 应用；缺的是页面侧没有任何脚本去轮询 `/poll`——
      `adapter.mjs` 里那条 `connect.js` 路由要读的 `ui/connect.js` **根本不存在**。
      本轮尝试补齐（写 `connect.js` + 注入 `<script>` + Playwright 端到端），但页面在
      Playwright headless 里始终没有启动（`window.filmcraftLoad` 为 null，90 s 超时），
      无法证明这条链路可用。为避免把**未验证**的连接代码留在发布物里，已全部回退
      （adapter 注入、build 复制、`connect.js`、新用例），`panel.test` 恢复 1/1 通过。
      下一步：先用 `--virtual-time-budget --dump-dom` 的 chrome CLI 方式（browser-smoke 已验证可行）
      确认页面能启动，再把轮询脚本接到 wasm 的 `craft_*` 上。
      **2026-10-19 第二次尝试**：已用 chrome CLI + `--virtual-time-budget=60000` 加注入的
      `connect.js` 复测，`editor_status.ready` 在 60 s 内始终为 false——说明页面并没有去轮询
      （不是 Playwright 特有的问题）。需要先用 DevTools 协议看页面的 console/网络，确认注入的
      `<script>` 是否执行、`/poll` 是否被发出；在此之前不再往发布物里加连接代码（本次已回退）
      **2026-10-19 第三次探测（决定性）**：直接用 fetch 取 `editorUrl`，返回的是 **180 字节**的文档，
      里面没有 `connect.js`、也不是应用页——`/app/` 根本没在服务 `ui/engine/index.html`。
      所以"页面不轮询"的真因在 adapter 的静态路由，而不是页面脚本。下一步：修 `app/` 路由
      （确认 rel 解析与 `realpath` 分支），再谈连接脚本。
- [x] **浏览器面板真的执行命令了**（2026-10-19，第 44 轮起）：`ui/tests/panel-live.test.mjs`
      在真实 Chromium 里加载 adapter 提供的应用页，页面通过 `connect.js` 轮询 `/<token>/poll`，
      用 **页面自己初始化好的 wasm 模块**（`window.__jianyingcraftEngine`）执行
      `craft_submit` / `craft_take_reply`，再 `POST /reply`。实测：
      `editor_status.ready=true`、`file.openDemoProject` + `sequence.inspect` 返回 **7 个视频片段**。
      这条链路的关键修复有三处：
      1. `ui/connect.js` 从未存在（adapter 早就注入 `<script src="../connect.js">`）——补上；
      2. `connect.js` 用 `new URL('../')` 推 token 前缀，导致 `/poll` 变 403——改成 `'./'`；
      3. 连接脚本**自己 import 第二份 wasm** 会得到独立实例（`__wbindgen_malloc` 未初始化），
         命令落不到可见文档——改为页面启动时把模块挂到 `window.__jianyingcraftEngine`，
         连接脚本复用同一实例（`apps/filmcraft-web/web/index.html` 的一行导出）。
      插件全套因此变成 **46 / 46**（含 Chrome + wasm 桥契约）。
- [x] adapter 的 `_panel_*` 通道就是通往引擎 `Bridge` 的传输层：页面侧 `ui/connect.js` 用
      `craft_submit` / `craft_take_reply` 在**可见 wasm 实例**上执行，再把结果 `POST /reply` 回来
      *2026-10-20 复核：`ui/adapter.mjs` 的 `command_list` / `command_run` / `inspect` / `save_project`
      全部走同一个 `dispatch()` → `/<token>/poll` → 页面执行 → `/reply`；`connect.js` 明确复用
      `window.__jianyingcraftEngine`（不另起第二份 wasm）。真实 Chrome 下
      `node --test tests/panel-live.test.mjs`（`CHROME_PATH` 指向本机 Chrome）= 1 passed，
      断言 `editor_status.ready=true`、`file.openDemoProject` + `sequence.inspect` 返回 7 个片段。*
- [x] 插件 `tools.json`：单一来源 `ui/tools.mjs`（17 工具 / 14 公开 + 3 面板内部），
      `ui/build-tools.mjs` 生成、`build.mjs` 随构建产出并计入 `build-info.json`，
      `ui/tests/tools.test.mjs` 4 条校验（同步、描述、内部工具不公开、command_run 文档）
- [x] adapter 的 `schemas`/`descriptions` 已迁到 `ui/tools.mjs`；**2026-10-19 补齐 `npm ci` 并实跑**
      （102 包）：`node --test tests/*.test.mjs` → **44 tests, 42 pass, 0 fail, 2 skip**
- [x] **`dist` 缺 `config.json` 的构建 bug**（2026-10-19）：`build.mjs` 只复制 panel.html /
      cli.lock.json / tools.json，漏了 adapter 启动必读的 `config.json`，导致 `dist/adapter.mjs`
      一启动就 ENOENT 退出——e2e 表现为 `MCP error -32000: Connection closed`。
      修法：`copyFile('config.json', 'dist/config.json')` 并入 `build-info.json`
- [x] **e2e 用例按新架构重写**（2026-10-19）：旧用例断言"宿主 CLI 跑 command_run / open_file 出 model"，
      与"引擎在面板 WASM 里"的新架构冲突。重写为宿主侧链路：`engine_diagnose` → `open_editor`
      （断言 `engineRuntime.version` 与 `ready:false`）→ `engine_cli_list`（178 条目录 + 过滤）→
      `engine_cli_run project build`（真实落盘 draft_content.json）→ `engine_cli_run inspect` →
      `list_files` → `save_file`（含 EEXIST 与逃逸拒绝）。**2/2 通过**
- [x] **第二个构建漏项 + viewer 用例重写**（2026-10-19）：`dist` 还漏了 `panel-editor.html`
      与 `editor-boot.mjs`，导致读 `ui://jianyingcraft/editor.html` 资源时 ENOENT；
      `build.mjs` 一并复制并计入 `build-info.json`。旧 viewer 用例重写为
      `面板/编辑器页：MCP 资源 + 真实浏览器跑到引擎与桥都就绪`：读 MCP 资源 → 起静态服务 →
      Chromium 断言 `#engine`=引擎已加载、`#bridge`=面板桥就绪且命令数 ≥700；截图
      `runtime/panel-evidence/editor-page.png`
- [x] **wasm 桥契约实测**（2026-10-19）：`wasm-bindgen --target nodejs --out-dir %TEMP%\\jc-node-wasm
      target/wasm32-unknown-unknown/release/filmcraft_web.wasm` 后设
      `JIANYINGCRAFT_WASM_JS` 跑 `tests/editor-wasm.test.mjs`——目录 701 条、受理/回包/空闲全过
- [x] 与 `jianyingcraft-plugin` 的 adapter 契约对齐（`command_list` / `command_run` / `save_project`
      映射到引擎命令面，动作走 `Bridge`）
      *2026-10-20：三个工具均经 `dispatch()` 进入页面桥并在可见 wasm 应用上执行（见上一条的复核）；
      命令条数以 `craft_catalog()` 实际返回为准（本机 nodejs glue 实测 701 条，不是 165）。*
- [x] 面板内命令执行走可见 App 状态（不得改不可见副本）
      *2026-10-20：`connect.js` 只调用页面已初始化的 `window.__jianyingcraftEngine`（曾因第二个
      wasm 实例导致 `__wbindgen_malloc` 未初始化而落不到可见文档，已修）；`panel-live` 在真实 Chrome
      里加载 adapter 提供的应用页执行命令并通过，证明改的是可见实例。*
- [x] **一致性测试 T6（断线不重放）**（2026-10-19）：adapter 的 pending 超时从 45 s 收紧到
      `HEARTBEAT_MS`（15 s），超时同时从 `queue` 移除该条，面板回来不会重放；
      `ui/tests/bridge-disconnect.test.mjs`：心跳一次 → 停止轮询 → `command_run` 在 14–21 s 内
      以 `timed out` 失败 → 再 poll 返回 `null`。插件全套 **45 / 45**
- [x] **一致性测试 T2（结果同构）**（2026-10-19）：`bridge_ui::bridge_results_match_headless_for_representative_commands`
      拿 12 条静态查询分别走面板桥与 headless `Session`，断言结果 JSON 深度相等。
      有意排除两类：`command.list`（面板会话多注册 `tool.*`/`playback.toggle`/`window.*`，
      目录不同是架构事实）与 `sequence.settings`/`sequence.colorSettings`
      （UI 路径打开对话框、返回 `{"dialog":…}`，headless 返回数据——这是**真实的 UI/headless 差异**，
      已在文档中写明，不当作回归）。
- [x] **T5（可见性）**：`bridge_executes_commands_against_the_visible_app` 已经证明
      "桥提交 `file.newBin` → 控制通道 `project.inspect` 读到"，无需新用例
- [x] T1/T3/T4/T7 已有测试：T1 = `bridge_ui::craft_catalog_ids_match_command_list`（三方 id 集合相等，
      2026-10-20 补）；T3 = `bridge_ui::bridge_limits_are_enforced_at_the_app_boundary` + `bridge::tests` 的
      在途记账用例；T4 = `apps/filmcraft-web/tests/host_policy.rs` 的路径逃逸组；T7 = `automation` 的
      严格参数用例（MCP 工具参数层，`craft_submit` 转发参数不做 -32602）

## 5. 交付与验收

- [x] **验收报告**：`docs/derivation/11-acceptance-report.md`（逐项判定、门禁实测、未验证清单、风险）
- [x] `docs/derivation/JianyingCraft-Derivation.md`（派生说明、改动清单、许可义务）
- [x] 插件仓改为消费派生仓 web 构建：`ui/sync-engine.mjs` 同步 + `--check` drift 检测，
      产物在 `ui/engine/`（已 gitignore）
- [x] 场景技能迁移到 filmcraft 命令面：
      `jianyingcraft-scenario-playbooks`、`jianyingcraft-use`、`jianyingcraft-editor` 已改写；
      新增 `jianyingcraft-captions`、`jianyingcraft-text-to-video`、`jianyingcraft-batch-remix`
      （字幕 / 图文成片 / 批量混剪）。6 个技能引用的 44 个命令 id 已用
      `ui/verify-skills.mjs --catalog target/commands.json` 对 **701 条实测命令目录** 校验，缺失 0。
      *注意：文档此前写的"165 条命令"是估算，实测 701 条（含无菜单命令）。*
- [x] **字幕技能实跑验证**（2026-10-18，headless CLI）：
      `transcript.set → transcript.inspect → transcript.createCaptions → captions.list →
      captions.export` 五步全 ok；11 个词切出 2 条字幕（track 48），导出 155 字节 SRT，
      cue 数 2、时间码单调、中文不乱码。详见 `docs/derivation/10-skill-runbook-verification.md`
- [ ] 技能实跑未覆盖：`transcript.generate`（本机无语音模型）、字幕面板样式预览、
      `captions.setStyle` 的渲染效果
- [x] **批量混剪核心链路实跑验证**（2026-10-18，headless CLI）：
      `clip.replaceFromBin`（clip 29 → item 6，联动音频 30 一起换）→ `file.saveCopy`（28,245 字节工作副本）
      → `file.exportMedia`（**17,984,537 字节 mp4**，`offlineMedia: []`）；替换后片段时间不变，
      对应技能里"未自适应，时长按模板"。详见 `docs/derivation/10-skill-runbook-verification.md` §8-10
- [ ] 批量混剪未覆盖：N 条循环与清单生成、失败路径注入、外部 ffprobe 校验（本机无 ffprobe）
- [x] **图文成片核心链路实跑验证**（2026-10-18，headless CLI）：
      1080×1920/30 新序列 → 两段画面（Color Matte + Bars and Tone）→ `tts.create`/`tts.render`
      （clip 60，2.98 s）→ `captions.import`（2 条）→ `captions.list`（1 轨 2 条）→
      `durationFrames=450` → 导出 **909,010 字节 mp4**，`offlineMedia: []`。
      详见 `docs/derivation/10-skill-runbook-verification.md` §11-13
- [ ] 图文成片未覆盖：外部素材 `file.import`、中文配音（`tts.voices` 只有 `en-US`）、
      配音/画面时长自动对齐、成片抽帧校验
- [x] **三个场景技能固化为可重复校验**（2026-10-19，`ui/skill-smoke.mjs`）：
      一条命令跑完三套序列并断言产物 ——
      captions（2 条、SRT 2 cue / 103 字节）、batch-remix（副本 28,245 B + 成片 17,984,537 B，
      `offline 0`）、text-to-video（1080×1920 / 450 帧 / 2 字幕 / 2 片段 / 909,010 B，`offline 0`）。
      未设 `FILMCRAFT_CLI` 时 skip（退出 0）
- [ ] 其余三个技能（编辑器/使用指南/场景手册）是交互说明，没有独立命令流可脚本化；
      它们的命令 id 已由 `verify-skills` 校验
- [ ] ZCode / Codex / Kimi 实机验收（安装 + MCP 调用 + 画面三段证据）

      **更正（同轮复查）**：adapter **本来就**在 `app/index.html` 里注入了
      `<script type="module" src="../connect.js">`，并且有 `/connect.js` 路由去读
      `ui/connect.js` —— 但仓库里**没有这个文件**，请求 404，页面自然不轮询。
      这就是完整根因：补 `ui/connect.js`（轮询 `/poll` + wasm `craft_*` + `/reply`）即可，
      不需要改 `app/` 路由。
      **2026-10-19 第四次（修复后）**：`dist` 版 adapter 已回退到 `ui/engine`，被服务的页面从 180 字节
      变成 5235 字节且带 `connect.js` 注入；Playwright 里应用页**能启动**（`filmcraftLoad.readyMs`
      很快为真），但 `editor_status.ready` 仍为 false——即页面启动了却没轮询 `/poll`。
      下一步用 Playwright 的 `response`/`pageerror` 事件抓 `/poll` 与 `connect.js` 的实际请求，
      确认是 403（token/Origin 校验）、404 还是脚本异常。浏览器级验证仍未完成，不能算达成。
      **2026-10-19 第五次（抓到 403 真因）**：Playwright 的 `response` 事件显示页面确实在轮询，
      但请求打到了 `http://127.0.0.1:PORT/poll`——**丢了 token 前缀**，于是 adapter 返回 403；
      同时 `/token/connect.js` 是 200、应用 `readyMs` 415 ms。根因：`connect.js` 用
      `new URL('../', import.meta.url)` 从 `/TOKEN/connect.js` 推出了根目录。改为 `'./'` 后
      `poll` 应带 token。**修完后的复测脚本本身崩了**（adapter 调用出现
      `__wbindgen_malloc` 异常），因此浏览器级结论仍未取得，留待下一轮用干净脚本复测。
> **2026-10-19 决议（已被后续突破，保留为历史）**：浏览器面板接线**暂停**。已定位并修复两处真实缺陷
> （`dist` 缺 `ui/engine` 回退、`connect.js` 的 token 前缀），但"命令在页面 wasm 里执行"
> 缺少可复现的验证，且连续多轮的探测脚本出现无法解释的异常。该线程需要 Playwright trace / CDP
> 网络日志级别的调试，不再用试探方式推进。剩余工作优先：代理/变声 UI、文档收尾、以及
> **提交整理（待授权）**。
- [ ] **已知取舍（2026-10-20 复查）**：`host_policy::check_syntax` 按"字母+冒号"拒绝盘符相对路径
  （`C:secret.txt`），在 POSIX 上会把 `a:b.png` 这类合法文件名一起拒掉。不能用 `cfg(windows)` 修：
  这段代码编译到 wasm32，页面并不知道宿主 OS。真正的隔离由宿主的 `realpath` 完成，页面这层是
  保守的语法筛查；保留现值并在 `03-panel-bridge.md` 记明，不再放宽。