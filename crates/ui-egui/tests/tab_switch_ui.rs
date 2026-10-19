//! 任务 C 的回归测试：切换一级 Tab 只切面板，不打断播放、不清选区。
//!
//! 覆盖两个真实入口：
//!
//! - **Wide 1280×1024 + `Jianying` 工作区**：点击 `dock_ui::jianying_tabstrip` 画的
//!   `jy.tabstrip.captions` / `jy.tabstrip.audio`，走 `show_panel` 的 dock 重排；再点
//!   `jy.tabstrip.filters`（其面板不在默认 dock 里，强制插入并重排一次）；
//! - **Compact 390×884**：走控制通道 `ui.shell.tab`（手机端靠后的 Tab 可能被横向滚动裁掉，
//!   自动化用该方法切面板，见 `control.rs`）。
//!
//! 每次切换后都用 `ui.inspect` 断言三条不变量：
//! `playback.playing` 仍为 `true`、`selection` 与切换前完全一致且仍包含同一个 clip id、
//! `activeSequence` 不变。若某个 Tab 触发 `playback.pause` 或 `edit.deselectAll`，这里会先红。

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
        let harness = Harness::builder().with_size(egui::vec2(width, height)).with_max_steps(10_000).build_eframe(move |_cc| app);
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
            if let Ok(v) = reply.try_recv() {
                assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
                return v["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn inspect(&mut self) -> Value {
        self.ok("ui.inspect", json!({}))
    }
}

/// 选中 Demo 序列的第一条视频片段，并开始播放；返回 `(clip, 播放中的 inspect)`。
fn playing_with_one_video_clip(d: &mut Driver) -> (u64, Value) {
    let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    let clip = sequence["video"][0]["items"][0]["clip"].as_u64().expect("the demo sequence has a video clip");
    d.ok("engine.execute", json!({"command": "timeline.select", "params": {"clips": [clip]}}));
    d.frames(2);
    let selected = d.inspect();
    // Demo 的视频片段带链接音频，选中视频会一并选中它；这里只要求包含目标片段。
    let selected_ids = selected["selection"].as_array().expect("selection array");
    assert!(!selected_ids.is_empty(), "timeline.select selected nothing: {selected}");
    assert!(selected_ids.iter().any(|id| id.as_u64() == Some(clip)), "timeline.select dropped clip {clip}: {selected}");
    assert!(!selected["activeSequence"].is_null(), "no active sequence to play: {selected}");
    d.ok("ui.playback", json!({"action": "play"}));
    d.frames(2);
    let playing = d.inspect();
    assert_eq!(playing["playback"]["playing"], json!(true), "playback did not start: {playing}");
    (clip, playing)
}

/// 一次 Tab 切换后的三条不变量：仍在播放、选区一字不差、活动序列不变。
fn assert_switch_kept_state(after: &Value, baseline: &Value, clip: u64, what: &str) {
    assert_eq!(after["playback"]["playing"], json!(true), "{what} stopped playback: {after}");
    assert_eq!(after["selection"], baseline["selection"], "{what} cleared or changed the selection: {after}");
    let selection = after["selection"].as_array().expect("selection array");
    assert!(selection.iter().any(|id| id.as_u64() == Some(clip)), "{what} dropped clip {clip}: {after}");
    assert_eq!(after["activeSequence"], baseline["activeSequence"], "{what} changed the active sequence: {after}");
    assert!(!after["activeSequence"].is_null(), "{what} left no active sequence: {after}");
}

/// Wide + Jianying：点 `jy.tabstrip.*` 切「文本」再切「音频」，播放与选区全程不变。
#[test]
fn wide_tabstrip_switches_keep_playback_and_selection() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    // 先确认这条路径真的画出了可点的 Tab，否则下面的点击是空转。
    let strip = d.ok("ui.elements", json!({"prefix": "jy.tabstrip."}));
    for id in ["jy.tabstrip.captions", "jy.tabstrip.audio"] {
        assert!(
            strip.as_array().is_some_and(|elements| elements.iter().any(|element| element["id"] == json!(id))),
            "{id} is not drawn in the Jianying tab strip: {strip}"
        );
    }

    let (clip, baseline) = playing_with_one_video_clip(&mut d);
    // captions / audio 在默认 Jianying dock 里已有宿主，只做 Activate；filters 的
    // LumetriColor 不在 dock 中，`show_panel` 会先 `open_near` 真正插入面板并重排——
    // 这一条覆盖"面板切换重新布局"的路径。
    for (id, slug) in [("jy.tabstrip.captions", "captions"), ("jy.tabstrip.audio", "audio"), ("jy.tabstrip.filters", "filters")] {
        d.ok("ui.click", json!({"id": id}));
        d.frames(3);
        let after = d.inspect();
        assert_switch_kept_state(&after, &baseline, clip, id);
        assert_eq!(after["ui"]["jianying_tab"], json!(slug), "clicking {id} did not activate that tab: {after}");
        if slug == "filters" {
            // 该面板此前不在 dock 里：它必须真的被插进来（重排发生），断言才有意义。
            let panels = d.ok("ui.elements", json!({"prefix": "panel."}));
            assert!(
                panels.as_array().is_some_and(|elements| elements.iter().any(|element| element["id"] == json!("panel.LumetriColor"))),
                "clicking {id} did not open the LumetriColor panel: {panels}"
            );
        }
    }
    d.ok("ui.playback", json!({"action": "stop"}));
}

/// Compact：用 `ui.shell.tab` 切「文本」再切「音频」，同样不中断播放、不清选区。
#[test]
fn compact_shell_tab_switches_keep_playback_and_selection() {
    let mut d = Driver::new(390.0, 884.0);
    let (clip, baseline) = playing_with_one_video_clip(&mut d);
    for slug in ["captions", "audio"] {
        d.ok("ui.shell.tab", json!({"tab": slug}));
        d.frames(3);
        let after = d.inspect();
        assert_switch_kept_state(&after, &baseline, clip, &format!("ui.shell.tab {slug}"));
        assert_eq!(after["ui"]["jy_shell_slug"], json!(slug), "ui.shell.tab {slug} did not activate that tab: {after}");
        assert!(!after["ui"]["jy_drawer"].is_null(), "ui.shell.tab {slug} opened no drawer: {after}");
    }
    d.ok("ui.playback", json!({"action": "stop"}));
}
