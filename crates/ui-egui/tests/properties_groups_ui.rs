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
