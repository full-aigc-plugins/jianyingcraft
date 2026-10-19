//! 剪映式「选中即属性」分组：基础 / 变速 / 动画 / 音频 / 调色 五个可折叠分组。
//!
//! 断言：五个分组标题都存在（自动化 id `props:<Group>`）、默认展开时内容可见、
//! 点标题能折叠 / 展开，且用状态折叠任意一组只影响该组内容。

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

/// (group title, one automation id that only exists while that group is open).
const GROUPS: [(&str, &str); 5] = [
    ("Basic", "properties.motion.position.addKeyframe"),
    ("Speed", "properties.speed"),
    ("Animation", "properties.animation.position"),
    ("Audio", "properties.volume.level.addKeyframe"),
    ("Color", "properties.lumetri.temperature.addKeyframe"),
];

#[test]
fn properties_panel_has_the_five_jianying_groups_and_they_collapse() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    d.ok("ui.set", json!({"focused": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let video = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    let audio = seq["audio"][0]["items"][0]["clip"].as_u64().expect("a linked audio clip");
    d.exec("timeline.select", json!({"clips": [video, audio]}));
    d.frames(4);

    // 五个分组标题都在，且标题文字非空。
    let ids = d.ids("props:");
    for (group, _) in GROUPS {
        let id = format!("props:{group}");
        assert!(ids.iter().any(|i| i == &id), "{group} missing from {ids:?}");
        assert!(!d.label(&id).is_empty(), "{id} has no title text");
    }
    // 默认展开：每组的代表控件都渲染出来。
    for (group, marker) in GROUPS {
        assert!(d.has(marker), "{group} content ({marker}) missing while open");
    }

    // 点标题折叠「基础」：只有基础组的内容消失，折叠状态被记住。
    d.click("props:Basic");
    assert!(d.app().ui.collapsed_fx.iter().any(|k| k == "props:Basic"), "the header click records the collapsed state");
    assert!(!d.has("properties.motion.position.addKeyframe"), "Basic content hides when collapsed");
    for (group, marker) in GROUPS.iter().skip(1) {
        assert!(d.has(marker), "{group} stays open: {marker}");
    }
    // 再点一次展开。
    d.click("props:Basic");
    assert!(d.has("properties.motion.position.addKeyframe"), "Basic content is back after the second click");

    // 其余四组用同一状态折叠 / 展开：只影响该组内容。
    for (group, marker) in GROUPS.iter().skip(1) {
        d.app().ui.collapsed_fx.push(format!("props:{group}"));
        d.frames(2);
        assert!(!d.has(marker), "{group} content ({marker}) hides when collapsed");
        for (other, other_marker) in GROUPS.iter().filter(|(g, _)| g != group) {
            assert!(d.has(other_marker), "collapsing {group} must not hide {other} ({other_marker})");
        }
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
    assert!(d.has("props:Speed"), "an audio-only selection must still offer the Speed group");
    assert!(d.has("properties.speed"), "the Speed group's button must be laid out for audio clips");
    assert!(!d.has("props:Basic"), "an audio clip has no video Basic group: {:?}", d.ids("props:"));
}

/// 调色组的数值控件走现有的 Lumetri 参数：改动会先应用 Lumetri，再写参数。
#[test]
fn color_group_edits_the_lumetri_effect() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.panel.show", json!({"panel": "Properties"}));
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    d.exec("timeline.select", json!({"clips": [clip]}));
    d.frames(4);
    assert!(d.has("props:Color"), "the Color group is present");
    assert!(d.has("properties.lumetri.temperature.addKeyframe"), "its rows are laid out");
}
