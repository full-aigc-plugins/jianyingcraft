//! 剪映式三档 IA 的验收测试（`docs/derivation/02-ia-mapping.md` §5，IA-1..IA-5）。
//!
//! 复用 `tests/layout_ui.rs` 的 Driver 写法：`egui_kittest` Harness + 控制通道
//! （`ControlRequest` / `ui.click` / `ui.elements` / `ui.inspect` / `ui.drag`），
//! 断言的是**可观察的矩形、状态与序列 JSON**，不碰生产代码。
//!
//! 两条曾经互相矛盾的条目已按规格收敛，现在都是可执行断言（本文件没有 `#[ignore]`）：
//!
//! - IA-1 的「导出动作 0 次页面切换」按 §5 的定义读作「留在同一个单窗口工作区：不新开 OS
//!   窗口、不换工作区、不改 dock 树」；导出按剪映参考行为进入**窗口内**的 Export 屏幕
//!   （`ui.inspect.ui.mode` 由 `Edit` 变 `Export`），由
//!   [`ia1_export_enters_export_mode_without_leaving_the_workspace`] 断言。
//! - IA-2 的「768×1024 右侧栏」：布局档位每帧只按窗口宽度算一次（`lib.rs::frame`），
//!   内容区左右各内缩 1 px 不再把 768 px 算成 766 px 的 Compact；768 px 与 900 px 同为
//!   Medium，属性抽屉是贴预览右缘的 240 px 侧栏，收起后预览恢复原宽。

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn new(width: f32, height: f32) -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let builder = Harness::builder().with_size(egui::vec2(width, height)).with_max_steps(10_000);
        let harness = builder.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(4);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
                return v["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn elements(&mut self, prefix: &str) -> Value {
        self.ok("ui.elements", json!({ "prefix": prefix }))
    }

    fn rect(&mut self, id: &str) -> [f64; 4] {
        let elements = self.elements("jy.");
        require_rect(&elements, id)
    }
}

/// 一个元素矩形：`[x, y, w, h]`。
fn rect_of(elements: &Value, id: &str) -> Option<[f64; 4]> {
    let element = elements.as_array()?.iter().find(|element| element["id"] == json!(id))?;
    let rect = element["rect"].as_array()?;
    Some([rect.first()?.as_f64()?, rect.get(1)?.as_f64()?, rect.get(2)?.as_f64()?, rect.get(3)?.as_f64()?])
}

fn require_rect(elements: &Value, id: &str) -> [f64; 4] {
    rect_of(elements, id).unwrap_or_else(|| panic!("{id} is not drawn: {elements}"))
}

/// 两个矩形是否有面积重叠（相切不算）。
fn overlaps(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

fn ids(elements: &Value) -> Vec<String> {
    elements.as_array().map(|list| list.iter().filter_map(|element| element["id"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// IA-1 的「页面没换」指纹：窗口尺寸、布局档位、工作区、模式、dock 树。
/// 抽屉开合与 Tab 切换只改 `ui.jy_*`，不得改这五项。
fn frame_fingerprint(info: &Value) -> Value {
    json!({
        "window": info["window"],
        "layoutClass": info["layoutClass"],
        "workspace": info["ui"]["workspace"],
        "mode": info["ui"]["mode"],
        "dock": info["ui"]["dock"],
    })
}

fn assert_frame_kept(d: &mut Driver, before: &Value, step: &str) {
    let now = d.ok("ui.inspect", json!({}));
    assert_eq!(&frame_fingerprint(&now), before, "{step} 改变了窗口/布局档位/工作区/模式/dock");
}

fn video_clips(d: &mut Driver) -> usize {
    let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    sequence["video"].as_array().map(|tracks| tracks.iter().map(|track| track["items"].as_array().map_or(0, Vec::len)).sum()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// IA-1：390×884 无横向溢出，导入/分割/字幕/导出 4 个动作 0 次页面切换
// ---------------------------------------------------------------------------

/// Compact 页面级容器一律在 390×884 视口内：顶栏、预览、时间轴、工具条、导出与属性开关都不越界。
///
/// 唯一允许越界的是顶栏那条**可横滑的 13 个一级 Tab**（docs §2）：390 px 放不下 13 个入口，
/// 靠后的 Tab 被滚动区裁掉，但它们的矩形仍记在滚动内容坐标系里（`jy.tab.filters` 的 x 已到 437），
/// 这不是页面横向溢出，而是滚动内容的固有坐标。纵向则一律不得越界。
#[test]
fn ia1_compact_has_no_page_overflow() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["layoutClass"], json!("compact"), "390 px must be the compact class: {info}");
    let width = info["window"][0].as_f64().expect("window width");
    let height = info["window"][1].as_f64().expect("window height");
    assert!((width - 390.0).abs() < 0.5 && (height - 884.0).abs() < 0.5, "the harness should be 390×884, got {width}×{height}");

    let elements = d.elements("jy.");
    let list = elements.as_array().expect("elements");
    assert!(!list.is_empty(), "the compact shell draws nothing: {elements}");
    // 顶栏现在只横排 3 个主 Tab，其余收进「更多」；因此**任何**入口都不允许越过视口右边界
    // （旧断言允许「被裁掉的 jy.tab.*」存在，那是 13 个 Tab 平铺时代的前提）。
    let mut present: Vec<&str> = Vec::new();
    for element in list {
        let id = element["id"].as_str().unwrap_or("");
        let rect = require_rect(&elements, id);
        assert!(rect[0] >= -0.5, "{id} starts left of the viewport: {rect:?}");
        assert!(rect[1] >= -0.5 && rect[1] + rect[3] <= height + 0.5, "{id} overflows vertically: {rect:?}");
        assert!(rect[0] + rect[2] <= width + 0.5, "{id} overflows the 390 px page horizontally: {rect:?}");
        present.push(id);
    }
    // 顶栏只横排 3 个主 Tab + 「更多」（`jy.tab.inspector` 是属性开关，不算 Tab）。
    for id in ["jy.tab.cut", "jy.tab.captions", "jy.tab.audio", "jy.tab.more"] {
        assert!(present.contains(&id), "the compact top bar must show {id}: {elements}");
    }
    // 页面骨架本身必须铺满视口宽度（不越界也不塌陷）。
    for id in ["jy.topbar", "jy.preview", "jy.timeline"] {
        let rect = require_rect(&elements, id);
        assert!(rect[2] >= width - 2.5, "{id} does not span the page: {rect:?}");
    }
}

/// IA-1 的动作流程：素材（移动端导入入口）、工具条分割、文本 Tab 都不换窗口、不换 dock、不进别的模式。
///
/// Compact 是单列壳（docs §2），没有 Wide 的左栏 rail；移动端的「导入」入口就是顶栏「素材」Tab，
/// 它打开素材抽屉（Project 面板）。Wide 的 `jy.rail.import` 另由
/// [`ia1_wide_rail_import_does_not_leave_the_workspace`] 覆盖。
#[test]
fn ia1_compact_actions_keep_the_window_and_the_dock() {
    let mut d = Driver::new(390.0, 884.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    let before = frame_fingerprint(&d.ok("ui.inspect", json!({})));
    assert!(rect_of(&d.elements("jy.rail."), "jy.rail.import").is_none(), "compact is single-column: it has no left rail");

    // 导入（移动端 =「素材」Tab）：真的打开了素材抽屉。
    d.ok("ui.click", json!({"id": "jy.tab.cut"}));
    d.frames(3);
    let media = d.elements("panel.Project");
    assert!(!ids(&media).is_empty(), "the media tab must draw the Project panel in the drawer: {media}");
    assert_frame_kept(&mut d, &before, "打开素材抽屉");

    // 工具条分割：镜头数 +1，证明是真动作而不是只改 UI。
    d.ok("engine.execute", json!({"command": "playhead.set", "params": {"seconds": 2.0}}));
    d.frames(2);
    let clips = video_clips(&mut d);
    d.ok("ui.click", json!({"id": "jy.tool.split"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), clips + 1, "the toolbar split should cut one video clip at the playhead");
    assert_frame_kept(&mut d, &before, "工具条分割");

    // 文本 Tab：抽屉换成 Text 面板，页面仍然不换。
    d.ok("ui.click", json!({"id": "jy.tab.captions"}));
    d.frames(3);
    let text = d.elements("panel.Text");
    assert!(!ids(&text).is_empty(), "the text tab must draw the Text panel in the drawer: {text}");
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["ui"]["jy_tab"], json!("captions"), "the text tab is the active one: {info}");
    assert_frame_kept(&mut d, &before, "切换到文本 Tab");
}

/// Wide 左栏的「导入」rail 元素只发 `file.import`，不换工作区、不换 dock、不进别的模式。
///
/// IA-1 原文要求在 390×884 下点 rail 导入，但 Compact 单列壳按设计没有 rail（docs §2），
/// 因此把这条动作放到 rail 真实存在的 1280×1024 上验证，移动端的导入入口由上一个用例覆盖。
#[test]
fn ia1_wide_rail_import_does_not_leave_the_workspace() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let rail = d.elements("jy.rail.");
    require_rect(&rail, "jy.rail.import");
    let before = frame_fingerprint(&d.ok("ui.inspect", json!({})));

    d.ok("ui.click", json!({"id": "jy.rail.import"}));
    d.frames(4);
    assert_frame_kept(&mut d, &before, "点击导入 rail");
}

/// IA-1 的导出动作：导出按钮**按设计**进入 Export 模式（剪映参考行为），但工作区与 dock 不变。
///
/// §5 把「0 次页面切换」定义为「留在同一个单窗口工作区」，不含「mode 不变」：
/// 导出是全窗口内的 Export 屏幕，不是新窗口、不是路由跳转。
#[test]
fn ia1_export_enters_export_mode_without_leaving_the_workspace() {
    let mut d = Driver::new(390.0, 884.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    let before = frame_fingerprint(&d.ok("ui.inspect", json!({})));
    require_rect(&d.elements("jy.export"), "jy.export");

    d.ok("ui.click", json!({"id": "jy.export"}));
    d.frames(4);
    let after = d.ok("ui.inspect", json!({}));
    assert_eq!(after["ui"]["workspace"], before["workspace"], "export must not switch the workspace: {after}");
    assert_eq!(after["ui"]["dock"], before["dock"], "export must not re-dock panels: {after}");
    assert_eq!(after["ui"]["mode"], json!("Export"), "the export button enters Export mode: {after}");
    assert_eq!(after["window"], before["window"], "export must not resize the window: {after}");
}

// ---------------------------------------------------------------------------
// IA-2：768×1024 属性抽屉贴预览右缘、不覆盖预览，收起后预览变宽
// ---------------------------------------------------------------------------

/// IA-2 的平板断点：768×1024 就是 Medium 的下界，属性抽屉是贴预览右缘的 240 px 侧栏。
///
/// 回归点：窗口内容区左右各内缩 1 px，档位若按内缩后的宽度重算，768 px 会落到
/// 766 px 的 Compact（抽屉从底部展开、预览只变矮不变宽）。
#[test]
fn ia2_tablet_768_inspector_sits_right_of_the_preview() {
    let mut d = Driver::new(768.0, 1024.0);
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["layoutClass"], json!("medium"), "768 px is the Medium breakpoint: {info}");
    let preview = d.rect("jy.preview");
    d.ok("ui.click", json!({"id": "jy.tab.inspector"}));
    d.frames(4);
    let preview_open = d.rect("jy.preview");
    let drawer = d.rect("jy.drawer");
    assert!((drawer[2] - 240.0).abs() < 1.5, "a tablet drawer is a 240 px sidebar: {drawer:?}");
    assert!((preview_open[0] + preview_open[2] - drawer[0]).abs() < 1.5, "the drawer is not on the preview's right edge: {preview_open:?} {drawer:?}");
    assert!(!overlaps(preview_open, drawer), "the drawer covers the preview: {preview_open:?} {drawer:?}");
    assert!(preview_open[2] < preview[2] - 200.0, "opening the sidebar must narrow the preview: {preview:?} → {preview_open:?}");
}

/// Medium（900×1024）：属性抽屉是 240 px 右侧栏，贴预览右缘、不压预览；收起后预览恢复原宽。
#[test]
fn ia2_medium_inspector_sits_right_of_the_preview() {
    let mut d = Driver::new(900.0, 1024.0);
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["layoutClass"], json!("medium"), "900 px is the medium class: {info}");
    let preview = d.rect("jy.preview");
    let timeline = d.rect("jy.timeline");

    d.ok("ui.click", json!({"id": "jy.tab.inspector"}));
    d.frames(4);
    let preview_open = d.rect("jy.preview");
    let drawer = d.rect("jy.drawer");
    assert!((drawer[2] - 240.0).abs() < 1.5, "the sidebar should be 240 px: {drawer:?}");
    assert!((preview_open[0] + preview_open[2] - drawer[0]).abs() < 1.5, "the sidebar is not on the preview's right edge: {preview_open:?} {drawer:?}");
    assert!((drawer[1] - preview_open[1]).abs() < 1.5, "the sidebar should start at the content top: {drawer:?}");
    assert!(!overlaps(preview_open, drawer), "the sidebar covers the preview: {preview_open:?} {drawer:?}");
    assert!(drawer[1] + drawer[3] <= timeline[1] + 0.5, "the sidebar overlaps the timeline: {drawer:?} {timeline:?}");
    assert!((preview[2] - preview_open[2] - 240.0).abs() < 1.5, "opening the sidebar must narrow the preview by 240 px: {preview:?} → {preview_open:?}");
    assert_eq!(d.rect("jy.timeline"), timeline, "opening the sidebar moved the timeline");

    d.ok("ui.click", json!({"id": "jy.tab.inspector"}));
    d.frames(4);
    assert!(rect_of(&d.elements("jy.drawer"), "jy.drawer").is_none(), "clicking the active Properties entry should close the sidebar");
    let preview_closed = d.rect("jy.preview");
    assert!((preview_closed[2] - preview[2]).abs() < 0.5, "closing the sidebar must give the preview its width back: {preview:?} → {preview_closed:?}");
}

// ---------------------------------------------------------------------------
// IA-3：1280×1024 四区常驻，时间轴高度可拖拽
// ---------------------------------------------------------------------------

/// Wide：Project / Program / Properties / Timeline 四个 panel.* 都在；拖根分隔线把时间轴拉高。
#[test]
fn ia3_wide_keeps_four_regions_and_a_draggable_timeline() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let panels = d.elements("panel.");
    let panel_ids = ids(&panels);
    for want in ["panel.Project", "panel.Program", "panel.Properties", "panel.Timeline"] {
        assert!(panel_ids.iter().any(|id| id == want), "the wide four-region layout is missing {want}: {panel_ids:?}");
    }
    let project = require_rect(&panels, "panel.Project");
    let program = require_rect(&panels, "panel.Program");
    let properties = require_rect(&panels, "panel.Properties");
    let timeline = require_rect(&panels, "panel.Timeline");
    assert!(
        project[0] < program[0] && program[0] + program[2] <= properties[0] + 0.5,
        "left→center→right order is broken: {project:?} {program:?} {properties:?}"
    );
    assert!(project[1] + project[3] <= timeline[1] + 0.5, "the timeline must sit below the three upper regions: {project:?} {timeline:?}");

    // 根分隔线：dock 树的第一层 Split（上三区 / 下时间轴），自动化 id 为 `dock.gutter.`。
    let gutters = d.elements("dock.gutter.");
    let gutter = require_rect(&gutters, "dock.gutter.");
    // 分隔线在 dock 组的上沿；组内还有 Tab 条与 38 px 工具条，所以和 `panel.Timeline` 的
    // 内容矩形之间隔着一层组边框（≤ 80 px），但一定在它上方。
    let chrome = timeline[1] - (gutter[1] + gutter[3]);
    assert!((0.0..=80.0).contains(&chrome), "the root gutter should sit right above the timeline's pane: {gutter:?} {timeline:?}");
    let from = json!({"x": gutter[0] + gutter[2] / 2.0, "y": gutter[1] + gutter[3] / 2.0});
    let to = json!({"x": gutter[0] + gutter[2] / 2.0, "y": gutter[1] + gutter[3] / 2.0 - 60.0});
    d.ok("ui.drag", json!({"from": from, "to": to, "steps": 8}));
    d.frames(6);

    let resized = d.elements("panel.");
    let timeline_after = require_rect(&resized, "panel.Timeline");
    let gutter_after = require_rect(&d.elements("dock.gutter."), "dock.gutter.");
    let grew = timeline_after[3] - timeline[3];
    assert!(
        (30.0..=90.0).contains(&grew),
        "dragging the gutter up 60 px should grow the timeline by about 60 px, got {grew} ({timeline:?} → {timeline_after:?})"
    );
    assert!((gutter[1] - gutter_after[1] - grew).abs() < 2.0, "the gutter must follow the timeline's top edge: {gutter:?} → {gutter_after:?}");
    assert!(
        (timeline[1] + timeline[3] - (timeline_after[1] + timeline_after[3])).abs() < 1.0,
        "the timeline's bottom edge must stay on the window edge: {timeline:?} → {timeline_after:?}"
    );
    let after_ids = ids(&resized);
    for want in ["panel.Project", "panel.Program", "panel.Properties", "panel.Timeline"] {
        assert!(after_ids.iter().any(|id| id == want), "resizing dropped {want}: {after_ids:?}");
    }
}

// ---------------------------------------------------------------------------
// IA-4：抽屉开合前后 sequence.inspect 的 clips / effects 完全一致
// ---------------------------------------------------------------------------

/// 打开再收起属性抽屉，`sequence.inspect` 里的片段与效果完全不变（JSON 相等）。
///
/// 当前 `sequence.inspect` 没有顶层的 `clips` / `effects` 键：片段按轨道放在 `video[].items[]`，
/// 每个片段自带 `effects[]`。因此这里按轨道整段比较 `video` 与 `audio` 两个数组——它们同时覆盖
/// 片段（clip/start/end/sourceIn/sourceOut…）与效果（effects 的 effect/enabled/params），
/// 比只比两个扁平数组更严格。
#[test]
fn ia4_inspector_round_trip_leaves_the_sequence_untouched() {
    let mut d = Driver::new(900.0, 1024.0);
    d.frames(3);
    let seq = |d: &mut Driver| d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    let before = seq(&mut d);
    let tracks = |value: &Value| -> (Vec<Value>, Vec<Value>) {
        (value["video"].as_array().cloned().unwrap_or_default(), value["audio"].as_array().cloned().unwrap_or_default())
    };
    let (video_before, audio_before) = tracks(&before);
    assert!(
        video_before.iter().any(|track| track["items"].as_array().is_some_and(|items| !items.is_empty())),
        "the demo sequence should have video clips: {before}"
    );
    let effects_before: usize = video_before
        .iter()
        .chain(audio_before.iter())
        .map(|track| track["items"].as_array().map(|items| items.iter().map(|item| item["effects"].as_array().map_or(0, Vec::len)).sum()).unwrap_or(0))
        .sum();
    assert!(effects_before > 0, "the demo sequence should carry clip effects: {before}");

    d.ok("ui.click", json!({"id": "jy.tab.inspector"}));
    d.frames(4);
    let open = seq(&mut d);
    let (video_open, audio_open) = tracks(&open);
    assert_eq!(video_open, video_before, "opening the inspector changed the video clips/effects");
    assert_eq!(audio_open, audio_before, "opening the inspector changed the audio clips/effects");

    d.ok("ui.click", json!({"id": "jy.tab.inspector"}));
    d.frames(4);
    let closed = seq(&mut d);
    let (video_closed, audio_closed) = tracks(&closed);
    assert_eq!(video_closed, video_before, "closing the inspector changed the video clips/effects");
    assert_eq!(audio_closed, audio_before, "closing the inspector changed the audio clips/effects");
}

// ---------------------------------------------------------------------------
// IA-5：Tab 切换期间播放不中断、选区不丢
// ---------------------------------------------------------------------------

/// 播放中从「文本」切到「音频」：`playback.playing` 始终为真，`selection` 完全不变。
#[test]
fn ia5_playback_and_selection_survive_tab_switches() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    let clip = sequence["video"][0]["items"][0]["clip"].as_u64().expect("the demo sequence has a video clip");
    d.ok("engine.execute", json!({"command": "timeline.select", "params": {"clips": [clip]}}));
    d.frames(2);
    let before = d.ok("ui.inspect", json!({}));
    let selection = before["selection"].clone();
    assert!(selection.as_array().is_some_and(|s| !s.is_empty()), "nothing is selected, so IA-5 cannot be observed: {before}");

    d.ok("ui.playback", json!({"action": "play"}));
    d.frames(2);
    let playing = d.ok("ui.inspect", json!({}));
    assert_eq!(playing["playback"]["playing"], json!(true), "playback did not start: {playing}");

    for (id, slug) in [("jy.tab.captions", "captions"), ("jy.tab.audio", "audio")] {
        d.ok("ui.click", json!({"id": id}));
        d.frames(3);
        let now = d.ok("ui.inspect", json!({}));
        assert_eq!(now["playback"]["playing"], json!(true), "switching to {id} stopped playback: {now}");
        assert_eq!(now["selection"], selection, "switching to {id} changed the selection: {now}");
        assert_eq!(now["ui"]["jy_shell_slug"], json!(slug), "switching to {id} did not activate that tab: {now}");
    }
    d.ok("ui.playback", json!({"action": "stop"}));
}
