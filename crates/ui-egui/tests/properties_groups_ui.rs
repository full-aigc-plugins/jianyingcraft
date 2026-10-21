//! 剪映式「选中即属性」页签：画面 / 音频 / 变速 / 动画 / 调节。
//!
//! 断言：选中片段后右栏顶部是五个文字页签（自动化 id `props.tab.<slug>`），默认停在「画面」；
//! 每个页签只画自己那一个折叠分组（`props:<Group>`）及其内容，另外四组不出现；分组标题仍可折叠 /
//! 展开，且折叠状态在切页后保留。

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
    /// A demo project in a taller window: the colour rows sit below the fold at 980 px, and
    /// `ui.click` is coordinate-based, so the test needs them on screen.
    fn demo_tall() -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let harness = Harness::builder().with_size(egui::vec2(1600.0, 2400.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(5);
        d
    }

    fn demo() -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let harness = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(4);
        d
    }

    /// 同 [`Driver::demo`]，但用 wgpu 后端（`FILMCRAFT_UI_SNAPSHOT_DIR` 下截图用）与 Wide 档尺寸。
    fn demo_render() -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let harness = Harness::builder().with_size(egui::vec2(1280.0, 1024.0)).with_max_steps(10_000).wgpu().build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(5);
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
        self.tx.send(req).expect("control channel");
        for _ in 0..600 {
            self.frames(1);
            if let Ok(r) = reply.try_recv() {
                assert_eq!(r["ok"], true, "{method} {params}: {r}");
                return r["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("engine.execute", json!({"command": command, "params": params}))
    }

    fn click(&mut self, id: &str) {
        self.ok("ui.click", json!({"id": id}));
        self.frames(2);
    }

    fn ids(&mut self, prefix: &str) -> Vec<String> {
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().expect("elements").iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }

    fn has(&mut self, id: &str) -> bool {
        self.ids(id).iter().any(|i| i == id)
    }

    fn label(&mut self, id: &str) -> String {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        v.as_array().expect("elements").iter().find(|e| e["id"] == json!(id)).and_then(|e| e["label"].as_str().map(str::to_string)).unwrap_or_default()
    }

    /// 元素的矩形 `[x, y, w, h]`（不在屏幕上时 `None`），像 `keyframes_ui` 那样按 id 精确取。
    fn rect(&mut self, id: &str) -> Option<[f64; 4]> {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        let e = v.as_array()?.iter().find(|e| e["id"] == json!(id))?;
        let r = e["rect"].as_array()?;
        Some([r[0].as_f64()?, r[1].as_f64()?, r[2].as_f64()?, r[3].as_f64()?])
    }

    /// 设置 `FILMCRAFT_UI_SNAPSHOT_DIR` 时离屏渲染一张 PNG（用来和剪映截图对照）。
    fn shot(&mut self, name: &str) {
        let Some(dir) = filmcraft_ui_egui::test_support::snapshot_dir() else { return };
        let img = self.harness.render().expect("wgpu render");
        std::fs::create_dir_all(&dir).expect("snapshot dir");
        img.save(dir.join(format!("{name}.png"))).expect("save snapshot");
    }

    /// 片段 Motion ▸ Scale 的当前值（直接读会话里的参数，和面板读的是同一份）。
    fn motion_scale(&mut self, clip: u64) -> f64 {
        let session = &self.harness.state().session;
        let it = session.active_sequence().expect("a sequence").find_item(filmcraft_project::ClipId(clip)).expect("the clip").1;
        it.effect("motion").expect("motion").param("scale").expect("scale").value.as_f64().expect("a number")
    }

    fn app(&mut self) -> &mut FilmcraftApp {
        self.harness.state_mut()
    }
}

/// (tab slug, the group that tab shows, one automation id that only exists while that group is open).
const TABS: [(&str, &str, &str); 5] = [
    ("picture", "Basic", "properties.motion.position.addKeyframe"),
    ("audio", "Audio", "properties.volume.level.addKeyframe"),
    ("speed", "Speed", "properties.speed"),
    ("animation", "Animation", "properties.animation.position"),
    ("color", "Color", "properties.lumetri.temperature.addKeyframe"),
];

#[test]
fn properties_panel_has_five_jianying_tabs_and_each_shows_its_group() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    d.ok("ui.set", json!({"focused": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let video = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    let audio = seq["audio"][0]["items"][0]["clip"].as_u64().expect("a linked audio clip");
    d.exec("timeline.select", json!({"clips": [video, audio]}));
    d.frames(4);

    // 五个页签都在，标题文字非空，默认停在「画面」。
    for (slug, _, _) in TABS {
        let id = format!("props.tab.{slug}");
        assert!(d.has(&id), "{slug} tab missing from {:?}", d.ids("props.tab."));
        assert!(!d.label(&id).is_empty(), "{id} has no label text");
    }
    assert!(d.has("props:Basic"), "the picture tab is active by default: {:?}", d.ids("props:"));
    assert!(d.has("properties.motion.position.addKeyframe"), "the picture tab's content is laid out");
    for (slug, group, marker) in TABS.iter().skip(1) {
        assert!(!d.has(&format!("props:{group}")), "the {slug} tab is inactive, so {group} is not drawn: {:?}", d.ids("props:"));
        assert!(!d.has(marker), "the {slug} tab is inactive, so {marker} is not drawn");
    }

    // 逐个页签：只有该页的分组和内容出现，另外四组（及内容）都不画。
    for (slug, group, marker) in TABS {
        d.click(&format!("props.tab.{slug}"));
        let shown = d.ids("props:");
        assert!(shown.iter().any(|i| i == &format!("props:{group}")), "{slug} shows {group}: {shown:?}");
        assert!(d.has(marker), "{group} content ({marker}) missing on its tab");
        for (other_slug, other, other_marker) in TABS {
            if other_slug == slug {
                continue;
            }
            assert!(!d.has(&format!("props:{other}")), "{slug} must not draw {other}: {shown:?}");
            assert!(!d.has(other_marker), "{slug} must not draw {other} content ({other_marker})");
        }
    }

    // 折「画面」：只剩标题，内容消失，折叠状态被记住；切到「变速」再切回来仍然折叠。
    d.click("props.tab.picture");
    d.click("props:Basic");
    assert!(d.app().ui.collapsed_fx.iter().any(|k| k == "props:Basic"), "the header click records the collapsed state");
    assert!(d.has("props:Basic"), "the Basic header stays while collapsed");
    assert!(!d.has("properties.motion.position.addKeyframe"), "Basic content hides when collapsed");
    d.click("props.tab.speed");
    assert!(d.has("properties.speed"), "the tab switch still shows the Speed tab");
    d.click("props.tab.picture");
    assert!(!d.has("properties.motion.position.addKeyframe"), "the collapsed state survives a tab switch");
    // 再点一次展开。
    d.click("props:Basic");
    assert!(d.has("properties.motion.position.addKeyframe"), "Basic content is back after the second click");

    // 其余四组用同一状态折叠 / 展开：只影响该组内容（各自在自己的页签上）。
    for (slug, group, marker) in TABS.iter().skip(1) {
        d.click(&format!("props.tab.{slug}"));
        d.app().ui.collapsed_fx.push(format!("props:{group}"));
        d.frames(2);
        assert!(d.has(&format!("props:{group}")), "{group} header stays while collapsed");
        assert!(!d.has(marker), "{group} content ({marker}) hides when collapsed");
        d.app().ui.collapsed_fx.retain(|k| k != &format!("props:{group}"));
        d.frames(2);
        assert!(d.has(marker), "{group} content ({marker}) is back when expanded");
    }
}

/// L-2：纯音频选区也必须有「变速」入口。Speed 分组原先包在 `kind == Video` 分支里，
/// 只选音频片段时整组消失；现在它和视频/音频共用，按钮仍走 `clip.speedDuration`。
#[test]
fn audio_only_selection_has_the_speed_group() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let audio = seq["audio"][0]["items"][0]["clip"].as_u64().expect("an audio clip");
    // `linked: false`：只选音频本身，不把链接的视频一起选进来（否则又变成视频选区）。
    d.exec("timeline.select", json!({"clips": [audio], "linked": false}));
    d.frames(4);
    // 「画面」页签对纯音频片段是空的：没有视频 Basic 分组。
    assert!(d.has("props.tab.picture"), "the tabs are there for an audio-only selection");
    assert!(!d.has("props:Basic"), "an audio clip has no video Basic group: {:?}", d.ids("props:"));
    // 「变速」页签对音频片段同样在，且给出速度按钮。
    d.click("props.tab.speed");
    assert!(d.has("props:Speed"), "an audio-only selection must still offer the Speed group");
    assert!(d.has("properties.speed"), "the Speed group's button must be laid out for audio clips");
}

/// 调色组的数值控件走现有的 Lumetri 参数：改动会先应用 Lumetri，再写参数。
#[test]
fn color_group_edits_the_lumetri_effect() {
    let mut d = Driver::demo_tall();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    d.exec("timeline.select", json!({"clips": [clip]}));
    d.frames(4);
    // 调色在「调节」页签里。
    d.click("props.tab.color");
    assert!(d.has("props:Color"), "the Color group is present on the 调节 tab");
    assert!(d.has("properties.lumetri.temperature.addKeyframe"), "its rows are laid out");
    // 真的改一次：点关键帧按钮会先 `effects.apply lumetri` 再写参数——只断言"分组存在"
    // 的用例在删掉 apply 逻辑后仍然会绿，所以这里必须看序列里有没有 lumetri。
    d.ok("ui.click", json!({"id": "properties.lumetri.temperature.addKeyframe"}));
    d.frames(4);
    let after = d.exec("sequence.inspect", json!({}));
    let item = after["video"][0]["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["clip"] == json!(clip)))
        .expect("the selected clip is still in the sequence");
    let text = serde_json::to_string(item).expect("item json");
    assert!(text.contains("lumetri"), "editing a Color row must apply the Lumetri effect: {text}");
    assert!(text.contains("temperature"), "and write the temperature parameter: {text}");
}

/// 剪映页签行：除了五个可用页签，还有灰显的「AI 编辑」「AI效果」两个占位页；点占位页不换页。
#[test]
fn greyed_ai_tabs_are_placeholders_that_do_not_switch_the_group() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    d.exec("timeline.select", json!({"clips": [clip]}));
    d.frames(4);

    // 两个 AI 页签在页签行里，标题按剪映写，且排在「画面」左边 / 「调节」右边。
    assert_eq!(d.label("props.tab.ai-edit"), "AI 编辑");
    assert_eq!(d.label("props.tab.ai-effect"), "AI效果");
    let picture = d.rect("props.tab.picture").expect("the 画面 tab");
    let edit = d.rect("props.tab.ai-edit").expect("the AI 编辑 tab");
    let color = d.rect("props.tab.color").expect("the 调节 tab");
    let effect = d.rect("props.tab.ai-effect").expect("the AI效果 tab");
    assert!(edit[0] < picture[0], "AI 编辑 sits left of 画面");
    assert!(effect[0] > color[0], "AI效果 sits right of 调节");
    // 页签均分一行：相邻页签的宽度一致，且都按行高排在同一行上。
    assert!((picture[3] - edit[3]).abs() < 0.5, "the tabs share one row height");
    assert!((picture[2] - edit[2]).abs() < 0.5, "the tabs share one cell width");

    // 点灰显的 AI 页签不换页：「画面」的基础分组与内容仍在。
    for id in ["props.tab.ai-edit", "props.tab.ai-effect"] {
        d.click(id);
        assert!(d.has("props:Basic"), "{id} is a placeholder: clicking it must not change the tab");
        assert!(d.has("properties.motion.position.addKeyframe"), "{id} must not replace the picture content");
    }
}

/// 「画面」页签下的一排分段按钮（基础 / 抠像 / 蒙版 / 美颜媒体）：默认「基础」；抠像 / 蒙版
/// 没有内容，选中只给一句灰字，切回「基础」内容回来。
#[test]
fn picture_segments_default_to_basic_and_the_rest_are_not_implemented() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    d.exec("timeline.select", json!({"clips": [clip]}));
    d.frames(4);

    for (slug, label) in [("basic", "基础"), ("keying", "抠像"), ("mask", "蒙版"), ("beauty", "美颜媒体")] {
        let id = format!("props.segment.{slug}");
        assert!(d.has(&id), "{label} segment missing from {:?}", d.ids("props.segment."));
        assert_eq!(d.label(&id), label, "{id} label");
    }
    // 默认「基础」：基础分组的内容照常画出来，没有那句灰字。
    assert!(d.has("props:Basic"), "the default segment is 基础");
    assert!(d.has("properties.motion.position.addKeyframe"));
    assert!(!d.has("props.segment.empty"), "the default segment shows no placeholder");

    for (slug, label) in [("keying", "抠像"), ("mask", "蒙版"), ("beauty", "美颜媒体")] {
        d.click(&format!("props.segment.{slug}"));
        assert!(!d.has("props:Basic"), "{label} has no Basic group");
        assert!(!d.has("properties.motion.position.addKeyframe"), "{label} draws no picture rows");
        assert!(d.has("props.segment.empty"), "{label} says the group is not implemented");
        assert_eq!(d.label("props.segment.empty"), "该分组暂未实现");
    }
    // 切回「基础」：内容回来，灰字消失。
    d.click("props.segment.basic");
    assert!(d.has("props:Basic"), "基础 comes back");
    assert!(d.has("properties.motion.position.addKeyframe"));
    assert!(!d.has("props.segment.empty"));
}

/// 剪映式行高与排列：「基础」折叠行 26、数值行 28；缩放 / 旋转是「标签 + 滑块 + 数值框」，
/// 位置是 X、Y 两个数值框；基础分组之下是那串 26 高的可折叠占位行。
#[test]
fn picture_rows_use_the_jianying_row_heights() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    d.exec("timeline.select", json!({"clips": [clip]}));
    d.frames(4);

    let header = d.rect("props:Basic").expect("the Basic header");
    assert!((header[3] - 26.0).abs() < 0.5, "the collapse header is 26 high, got {}", header[3]);

    for param in ["position", "scale", "rotation"] {
        let row = d.rect(&format!("properties.row.motion.{param}")).unwrap_or_else(|| panic!("{param} row missing"));
        assert!((row[3] - 28.0).abs() < 0.5, "the {param} row is 28 high, got {}", row[3]);
        if param == "position" {
            // 位置是 X、Y 两个数值框：并排、都在行内、Y 在 X 右边。
            let x = d.rect("properties.value.motion.position.x").expect("the X box");
            let y = d.rect("properties.value.motion.position.y").expect("the Y box");
            assert!(x[0] + x[2] <= y[0] + 0.5, "X sits left of Y: {x:?} {y:?}");
            assert!(y[0] + y[2] <= row[0] + row[2], "the Y box stays inside the row");
        } else {
            // 滑块在数值框左边，两者都在行内。
            let slider = d.rect(&format!("properties.slider.motion.{param}")).expect("the slider");
            let value = d.rect(&format!("properties.value.motion.{param}")).expect("the value box");
            assert!(slider[0] + slider[2] <= value[0] + 0.5, "the {param} slider sits left of its value box: {slider:?} {value:?}");
            assert!(value[0] + value[2] <= row[0] + row[2], "the {param} value box stays inside the row");
            assert!(slider[0] >= row[0] + 60.0, "the {param} slider starts after the label column: {slider:?} {row:?}");
        }
    }

    // 基础分组之下是剪映那串折叠行：26 高、在基础行下面、AI 类也在。
    let warp = d.rect("props.op.warp").expect("the 变形 row");
    assert!((warp[3] - 26.0).abs() < 0.5, "a placeholder collapse row is 26 high, got {}", warp[3]);
    assert!(warp[1] > header[1], "the placeholder rows sit below the Basic group");
    for slug in ["stroke", "shadow", "stabilize", "quality", "superres", "denoise", "repair", "relight", "upscale"] {
        assert!(d.has(&format!("props.op.{slug}")), "{slug} placeholder row missing");
    }
    // 占位行默认为收起（没有灰字），点一下展开一句「该分组暂未实现」。
    assert!(!d.has("props.op.warp.detail"), "the placeholder starts collapsed");
    d.click("props.op.warp");
    assert_eq!(d.label("props.op.warp.detail"), "该分组暂未实现");
    d.click("props.op.warp");
    assert!(!d.has("props.op.warp.detail"), "clicking again collapses the placeholder");

    // 数值框右侧的上下小箭头按一个显示精度步进：点一下「缩放」的向上箭头就加 1%。
    let box_rect = d.rect("properties.value.motion.scale").expect("the scale value box");
    let before = d.motion_scale(clip);
    d.ok("ui.click", json!({"x": box_rect[0] + box_rect[2] - 8.0, "y": box_rect[1] + 4.0}));
    d.frames(3);
    assert_eq!(d.motion_scale(clip), before + 1.0, "the value box's up arrow steps the scale by one percent");
}

/// 截图（设置 `FILMCRAFT_UI_SNAPSHOT_DIR` 时）：Wide 1280×1024 的「画面」页签，用来和剪映截图对照。
#[test]
fn picture_tab_renders_for_comparison() {
    let mut d = Driver::demo_render();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    d.exec("timeline.select", json!({"clips": [clip]}));
    d.frames(6);
    d.shot("properties-picture");
    d.click("props.segment.keying");
    d.frames(2);
    d.shot("properties-keying");
}
