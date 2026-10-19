# 剪映式信息架构映射

> 依据：`openspec/changes/derive-from-filmcraft/`（唯一规格事实源）任务 3「剪映式布局取向」。
> 约束：只动 `crates/ui-egui`；命令 id 一律复用上游 165 条命令面；不改 engine/project/render 公共 API。

## 1. 为什么不是"换个皮肤"

上游是 Premiere 的复刻：6 个工作区、Source/Program 双监视器、面板可任意浮动拆分。
剪映/CapCut 的模型是**单页流水线**：素材在左、预览在中、属性在右、时间轴在底，
高频动作在同一屏完成。差别在信息架构，不在引擎。

```mermaid
flowchart TB
  subgraph 上游["上游：6 工作区"]
    A1[Assembly] --- A2[Edit] --- A3[Color] --- A4[Audio] --- A5[Graphics] --- A6[Export]
  end
  subgraph 目标["JianyingCraft：单页 4 区 + 3 Tab"]
    T[顶部：项目名 · 剪辑/字幕/音频 · 导出]
    L[左：素材宫格]
    C[中：预览]
    R[右：选中即属性]
    B[底：时间轴]
    T --> L --> C --> R --> B
  end
  上游 -->|"保留命令，收敛入口"| 目标
```

## 2. 布局模型

| 断点 | 判定 | 行为 |
|---|---|---|
| Compact | 宽度 < 768 | 单列：预览 → 时间轴 → 底部抽屉（素材/属性） || Medium | 768–1024 | 预览上 / 时间轴下；素材与属性为可收起侧栏（240 px） |
| Wide | > 1024 | 四区常驻；时间轴高度 = 视口 38%，可拖拽 |

**实现状态**：Compact 与 Medium 均已走单页壳（`dock_ui::shell_area`）。两档共用一条顶栏：
`☰` + 可横滑的 **13 个剪映式一级 Tab** + 右侧 `Properties`；全局 Premiere 菜单条在这两档隐藏，
菜单收进 `☰` 的两级列表（`jy.menu` → 根 → 命令）。点 Tab 决定抽屉画哪个面板
（`UiState.jy_shell_panel`），未实现的 4 个（智能包装/模板/数字人/插件）灰显不可点。
Compact 抽屉从底部展开、只压缩预览；Medium 抽屉改为右侧 240 px 侧栏，时间轴下限 240 px。两档的时间轴上方都有 7 个按钮的工具条（Medium 36 px、Compact 40 px）。
两档都不改时间轴矩形、不改 `sequence.inspect`。

Wide 档的内置工作区 **`Jianying`** 已按剪映专业版补齐四件外壳（全部有测试）：

| 外壳 | 位置 | 状态 |
|---|---|---|
| 13 个一级 Tab（素材/音频/文本/贴纸/特效/转场/字幕/智能包装/滤镜/调节/模板/数字人/插件） | 顶部 42 px | 9 个可导航；「音频」按类型过滤素材、其余只做面板导航；智能包装/模板/数字人/插件灰显 |
| 左栏竖向 rail（导入/素材/子草稿/我的/AI/云素材/官方/即梦） | 左侧 56 px | 导入发 `file.import`、素材切 Project；其余 6 个灰显 |
| 右栏草稿参数（未选中片段时） | Properties 面板 | 名称/分辨率/帧率/时长/轨道/素材数 |
| 时间轴工具条（撤销/重做/分割/删除/播放/定格/倒放） | Timeline 顶部 38 px | 只转发既有命令；按钮跟随 `enabled` + 选区状态置灰；播放中显示"暂停" |
| 空工程提示（"把素材拖到这里，开始你的大作吧"） | Project 面板居中 | 自动化 id `project.emptyHint`；有素材后消失 |
| 右上角「导出」 | Wide Tab 条右侧 / 移动端顶栏最右 | 调 `file.exportMedia`（无 path）进入 Export 模式 |

首屏默认：全新安装（数据目录里没有 `workspaces.json`）进 `Jianying`；老用户保留上次选择
（`apps/filmcraft/src/main.rs::first_run_workspace`）。Compact/Medium 的 ☰ 菜单已改成两级
（根菜单 → 命令），并保留过滤框。

尚未完成：抽屉内面板滚动条常显、Tab 的素材库内容（音乐/贴纸/模板库属于云生态，不做）。

**实现要求**：断点判断只允许出现在 `crates/ui-egui/src/layout.rs` 的 `LayoutClass::from_width`
一处（已落地，6 条单测覆盖边界与恶意宽度），其余面板只读结果。禁止在各面板里散落
`available_width < 768` —— 否则三档行为必然漂移。当前档位可通过控制通道读到：
`ui.inspect` → `layoutClass`（`compact` / `medium` / `wide`）。

## 3. 面板 → 区域映射

| 上游文件 | 归属 | 改造动作 |
|---|---|---|
| `panels/media_browser.rs` | 左·素材 | 列表改宫格；加类型筛选、搜索、拖入导入 |
| `panels/import_mode.rs` | 左·素材（导入态） | 去掉字标绘制，改为拖放区 |
| `panels/monitor.rs` + `monitor_view.rs` | 中·预览 | 去掉 Source/Program 双监视器，保留源预览浮层 |
| `panels/effect_controls*.rs` | 右·属性 | 合并为分组折叠：基础/变速/动画/音频/调色 |
| `panels/lumetri.rs` | 右·属性（调色） | 收进"调色"分组，控件不删减 |
| `panels/effects.rs` | 右·属性（效果库） | 改可搜索列表，不占独立面板 |
| `panels/timeline*.rs`（6 个文件） | 底·时间轴 | 轨道头 ≤96 px；主轨磁吸；字幕轨置顶 |
| `panels/text.rs` + `tts.rs` | Tab·字幕 | 文案 + 配音 + 字幕样式三块 |
| `panels/mixer.rs` + `meters.rs` + `audio_fx_editor.rs` | Tab·音频 | 混音器与响度表同屏 |
| `panels/export_mode.rs` | 顶部·导出 | 改右侧抽屉，预设置于首屏 |
| `panels/multicam.rs` / `scopes.rs` / `reference.rs` | 高级模式 | 默认隐藏，命令可达 |

## 4. 交互契约（不得违背）

1. **选中即属性**：右侧属性区的唯一输入是 `sequence.inspect` 的 selection；切换 Tab 不清选区。
2. **播放不中断**：Tab 切换、抽屉开合、属性折叠都不得触发 `playback.stop`。
3. **主轨磁吸是视图约定**：主轨 = 第一条 V 轨，不是新数据字段；`timeline.place` 的帧栅格语义不变。
4. **命令优先**：任何控件必须有对应命令 id，不允许"只能点、不能命令"。
5. **automation id**：新增控件注册 `jy.<region>.<control>`，region ∈ `media|preview|inspector|timeline|topbar|drawer`。

## 5. 三档验收

| 编号 | 验收 | 证据 |
|---|---|---|
| IA-1 | 390×884 无横向溢出；导入/分割/字幕/导出 4 个动作 **0 次页面切换**——定义为「留在同一个单窗口工作区：不新开 OS 窗口、不换工作区、不改 dock 树」；「导出」按剪映参考行为进入**窗口内**的 Export 屏幕（`ui.inspect.ui.mode` 由 `Edit` 变 `Export`，属于该屏幕内的模式切换，不算页面切换） | 截图 + `ui.elements` 矩形断言 |
| IA-2 | 768×1024 侧栏可收起且不遮挡预览 | 截图 |
| IA-3 | 1280×1024 四区常驻，时间轴高度可拖拽 | 截图 |
| IA-4 | 重排前后 `sequence.inspect` 的 clips/effects 完全一致 | 两次 JSON diff |
| IA-5 | Tab 切换期间播放不中断、选区不丢 | `ui.inspect` 前后对比 |

> **2026-10-20 澄清 IA-1 的「0 次页面切换」**：原表述没有区分「换页面」与「进入导出屏幕」，
> 而剪映的参考行为是导出进入窗口内的专属屏幕——它不新开 OS 窗口、不换工作区、不改 dock 树，
> 也不发生路由/导航跳转。因此把该条款定义为上述单窗口约束；`mode` 的 `Edit → Export`
> 是同一窗口内的屏幕切换，不算页面切换。
>
> 做出这一取舍的理由：剪映产品对照（§7）里导出是独立屏幕而非抽屉/浮层，而本项目已按此实现并
> 有测试锁定（`crates/ui-egui/tests/layout_ui.rs::compact_top_bar_has_the_export_button`）。
> IA-1 的导出断言是
> `crates/ui-egui/tests/ia_acceptance_ui.rs::ia1_export_enters_export_mode_without_leaving_the_workspace`
> （工作区、dock、窗口尺寸不变，`mode == "Export"`）；把「0 次页面切换」读成「导出后 `mode` 不变」
> 的旧断言已删除，因为它编码的是产品已拒绝的读法。

## 6. 不做

- 不新增"主轨/字幕轨"的数据概念（沿用上游 caption track 与第一条 V 轨）。
- 不为移动端单独写一套命令（命令面唯一）。
- 不删除上游命令与快捷键，只是不再占据默认入口。

## 7. 剪映实测界面清单（截图基准 2026-10-18）

用户在 1280×1024 窗口下截取的剪映（JianYingPro）空工程界面，作为"非核心剪辑能力"的对照基线。
界面分六区：顶部 Tab 条、左侧素材栏、中部预览、右侧草稿参数、底部时间轴、时间轴工具条。

### 7.1 顶部一级 Tab（13 个）

| Tab | 作用 | FilmCraft 落点 | 状态 |
|---|---|---|---|
| 素材 | 媒体库与导入 | `panels/media_browser.rs` + `media_import` | 复用 |
| 音频 | 音乐/音效库、配音 | `panels/mixer.rs`、`meters.rs`、`tts.rs` | 复用（无在线曲库） |
| 文本 | 字幕、花字、文字模板 | `panels/text.rs` | 复用 |
| 贴纸 | 贴纸/动图素材 | `panels/graphics.rs` + `graphics_templates.rs` | 部分复用（无在线素材） |
| 特效 | 画面特效 | `panels/effects.rs` | 复用 |
| 转场 | 转场库 | `crates/edit/src/transitions.rs` | 复用 |
| 字幕 | 识别字幕/导入字幕 | `crates/captions` + `panels/timeline_captions.rs` | 复用，需重排入口 |
| 智能包装 | 一键字幕+花字+封面+标题组合 | 无 | **新建（P3）** |
| 滤镜 | LUT 滤镜库 | `crates/color/src/lut.rs` + `panels/lumetri.rs` | 复用 |
| 调节 | 曝光/色温/HSL 等参数 | `panels/lumetri.rs` | 复用 |
| 模板 | 成片模板（替换素材即成片） | 无 | **新建（P3，先本地模板）** |
| 数字人 | 云端数字人播报 | 无 | 不做（依赖云服务） |
| 插件 | 第三方扩展市场 | ZCode 插件面板本身 | 不做（不重复造） |

### 7.2 左侧素材栏（二级）

导入 / 素材 / 子草稿 / 我的 / AI 生成 / 云素材 / 官方素材 / 即梦 AI。

| 项 | 落点 | 状态 |
|---|---|---|
| 导入 + 素材 | `import_mode.rs`、`media_browser.rs`、`file.newBin` | 复用 |
| 子草稿 | 上游嵌套序列（nested sequence） | 复用，补入口 |
| 我的 | 工程内 bin 收藏 | 复用 |
| AI 生成 / 云素材 / 官方素材 / 即梦 AI | 无（全部依赖云端账号） | 不做，留扩展位 |

### 7.3 右侧「草稿参数」（未选中任何片段时）

草稿名称、保存位置、导出路径、导入方式（保留在原位置/复制到草稿）、自由层级、素材代理、
代理模式、时间线名称、比例（适应）、分辨率（适应）、草稿帧率（30 fps）、修改按钮。

| 参数 | FilmCraft 落点 | 状态 |
|---|---|---|
| 草稿名称 / 时间线名称 | 工程与序列名 | 复用 |
| 保存位置 / 导出路径 | 文件服务（桌面原子写、Web 下载） | 复用，补只读展示 |
| 导入方式 | `media_import` 的引用/复制策略 | **新增开关** |
| 自由层级 | 同轨重叠（overlap） | **需确认上游策略**，差异点 |
| 素材代理 / 代理模式 | 上游 proxies 作业 | 复用，补 UI |
| 比例 / 分辨率 / 帧率 | 序列设置 | 复用，改为竖屏默认 |

这条正好验证了第 4 节「选中即属性」的设计：**未选中时右栏就是工程/序列参数**。

### 7.4 播放器条

时间帧显示（`00:00:00:00`）、播放/暂停、逐帧、缩放、全屏、比例切换。

落点：`panels/monitor.rs`、`monitor_view.rs`；自动化 id 前缀 `jy.preview.*`。

### 7.5 时间轴工具条

左：撤销、重做、分割、删除、定格、倒放、镜像、旋转、裁剪、速度（`1.0x`）、音量、降噪、变声。
右：吸附、主轨磁吸、联动、自动吸附、时间轴缩放、更多。

| 工具 | FilmCraft 命令 | 状态 |
|---|---|---|
| 撤销/重做 | `edit.undo` / `edit.redo` | 复用 |
| 分割 | `timeline.razor` | 复用 |
| 删除/定格/倒放/镜像/旋转/裁剪 | 上游对应命令（以 `command_list` 为准） | 复用，需按剪映顺序重排 |
| 速度 | 变速命令 + `1.0x` 浮层 | 复用，补浮层 |
| 音量/降噪/变声 | 音频命令（降噪/变声可能缺） | 部分新建 |
| 吸附/主轨磁吸/联动 | UI 状态 | 新增（视图层） |

### 7.6 空态

时间轴空态提示"素材拖拽到这里，开始你的大作吧~"，左栏与预览也有对应空态。

落点：新增空态组件；验收要求空态下不出现空白面板（IA-1 补充）。

### 7.7 由此产生的优先级修正

- **P2 必做**：13 Tab 收敛为 3 个主 Tab + "更多"溢出菜单；左栏层级；右栏工程参数；底部工具条
  前 8 个高频工具；空态引导。
- **P3 再做**：智能包装、本地模板、代理模式 UI、子草稿入口。
- **不做**：数字人、云素材、官方/即梦素材、插件市场。

> 结论：剪映"除核心剪辑之外"的体积，大头是**素材生态与云端能力**（不做），
> 其次是**一键包装/模板**（P3 本地化），真正影响首屏可用性的只有
> 左栏层级、右栏工程参数、底部工具条与空态四件事。




