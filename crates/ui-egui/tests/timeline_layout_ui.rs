//! 剪映式时间轴布局：轨道头 ≤96 px、字幕轨置顶、拖动片段吸附到相邻片段边缘。

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_time::Tick;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn demo() -> Self {
        Self::sized(egui::vec2(1600.0, 980.0))
    }

    /// 剪映工作区的轨道头走 96 px 的紧凑布局。`size` 是窗口逻辑点数（控制通道看到的坐标）。
    ///
    /// 设 `JY_HEADER_PNG=1` 时用 wgpu 渲染器建 harness，测试末尾会把窗口存成 PNG（需要 GPU）；
    /// 不设时保持无渲染器的快速路径，测试照常跑。
    fn sized(size: egui::Vec2) -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let mut b = Harness::builder().with_size(size).with_max_steps(10_000);
        if std::env::var_os("JY_HEADER_PNG").is_some() {
            b = b.wgpu();
        }
        let harness = b.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(4);
        // 剪映工作区：轨道头走 96 px 的紧凑布局。
        d.ok("ui.set", json!({"workspace": "Jianying"}));
        d.ok("ui.panel.show", json!({"panel": "Timeline"}));
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

    fn rect(&mut self, id: &str) -> [f64; 4] {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        let e = v.as_array().expect("elements").iter().find(|e| e["id"] == json!(id)).unwrap_or_else(|| panic!("no element {id}")).clone();
        let r = e["rect"].as_array().expect("rect");
        [r[0].as_f64().expect("x"), r[1].as_f64().expect("y"), r[2].as_f64().expect("w"), r[3].as_f64().expect("h")]
    }

    fn has(&mut self, id: &str) -> bool {
        self.ok("ui.elements", json!({"prefix": id})).as_array().is_some_and(|list| list.iter().any(|element| element["id"] == json!(id)))
    }
}

const MAX_HEADER_W: f64 = 96.0;

/// #235 字幕轨头必须和视频轨头一样宽：剪映工作区两者都是收敛后的 ≤96 px。
#[test]
fn caption_track_header_matches_the_video_track_header_width() {
    let mut d = Driver::demo();
    d.exec("captions.newTrack", json!({"format": "Subtitle", "name": "C1"}));
    d.frames(4);
    let cap = d.rect("timeline.captionTrack.C1.header");
    let v1 = d.rect("timeline.track.V1.header");
    assert!(cap[2] > 0.0, "caption header is laid out: {cap:?}");
    assert!(cap[2] <= MAX_HEADER_W, "caption header is {:.1} px wide, must be <= {MAX_HEADER_W}", cap[2]);
    assert!((cap[2] - v1[2]).abs() < 0.5, "caption header {:.1} px must match the video header {:.1} px", cap[2], v1[2]);
}

#[test]
fn jianying_track_headers_are_at_most_96_px_wide() {
    let mut d = Driver::demo();
    for id in ["timeline.track.V1.header", "timeline.track.A1.header"] {
        let r = d.rect(id);
        assert!(r[2] > 0.0, "{id} is laid out: {r:?}");
        assert!(r[2] <= MAX_HEADER_W, "{id} is {:.1} px wide, must be <= {MAX_HEADER_W}", r[2]);
    }
}

#[test]
fn caption_tracks_sit_above_the_video_tracks() {
    let mut d = Driver::demo();
    d.exec("captions.newTrack", json!({"format": "Subtitle", "name": "C1"}));
    d.frames(4);
    let cap = d.rect("timeline.captionTrack.C1");
    let v1 = d.rect("timeline.track.V1.header");
    assert!(cap[1] < v1[1], "caption header y {} must be above video header y {}", cap[1], v1[1]);
    assert!(cap[1] + cap[3] <= v1[1] + 0.5, "caption rows {}..{} end at or above the video rows' top {}", cap[1], cap[1] + cap[3], v1[1]);
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][0]["items"][0]["clip"].as_u64().expect("a video clip");
    let body = d.rect(&format!("timeline.clip.{clip}"));
    assert!(cap[1] < body[1], "caption track is above the video clip: caption {}, clip {}", cap[1], body[1]);
}

/// L-1：`minimize all` 的矮轨道（22 px）里，轨道头的第二行控件不能画到下一行。
///
/// 旧代码把 `lower_y` 固定成 `hrect.min.y + 32.0`，22 px 的轨道放不下，眼球 / M / S 就落到
/// 下一行；现在行高不足时整行隐藏，所有留下的控件矩形都在自己的轨道头内。
#[test]
fn short_track_headers_keep_the_second_row_inside_the_header() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"timeline": {"videoTrackHeight": 22.0, "audioTrackHeight": 22.0}}));
    d.frames(6);
    let v1 = d.rect("timeline.track.V1.header");
    assert!((v1[3] - 22.0).abs() < 0.5, "the V1 header should be 22 px high: {v1:?}");

    // 第二行控件整行隐藏——不是画在轨道头外，更不是画到下一行。
    for id in ["timeline.track.V1.enabled", "timeline.track.A1.muted", "timeline.track.A1.solo"] {
        assert!(!d.has(id), "{id} must be hidden on a 22 px track");
    }
    let inside = |id: &str, d: &mut Driver, header: [f64; 4]| {
        let r = d.rect(id);
        assert!(r[1] >= header[1] - 0.5 && r[1] + r[3] <= header[1] + header[3] + 0.5, "{id} left its track header {header:?}: {r:?}");
    };
    // 第一行控件仍然画着，而且都在轨道头里。
    for id in ["timeline.track.V1.sourcePatch", "timeline.track.V1.locked", "timeline.track.V1.target", "timeline.track.V1.syncLock"] {
        inside(id, &mut d, v1);
    }
    let a1 = d.rect("timeline.track.A1.header");
    assert!((a1[3] - 22.0).abs() < 0.5, "the A1 header should be 22 px high: {a1:?}");
    for id in ["timeline.track.A1.sourcePatch", "timeline.track.A1.locked", "timeline.track.A1.target", "timeline.track.A1.syncLock"] {
        inside(id, &mut d, a1);
    }

    // 恢复常规高度后第二行回来，仍然落在轨道头内。
    d.ok("ui.set", json!({"timeline": {"videoTrackHeight": 64.0}}));
    d.frames(6);
    let v1 = d.rect("timeline.track.V1.header");
    assert!(d.has("timeline.track.V1.enabled"), "the second row should return on a 64 px track");
    inside("timeline.track.V1.enabled", &mut d, v1);
}

/// 主轨（V1）上的片段拖动到相邻片段边缘附近时吸附到该边缘（SNAP_PX = 9）。
#[test]
fn dragging_a_clip_snaps_to_the_neighbour_edge() {
    let mut d = Driver::demo();
    d.exec("sequence.snap", json!({"on": true}));
    d.frames(3);
    let seq = d.exec("sequence.inspect", json!({}));
    let items = seq["video"][0]["items"].as_array().expect("V1 items").clone();
    assert!(items.len() >= 2, "demo V1 has at least two clips");
    let a = items[items.len() - 2].clone();
    let b = items[items.len() - 1].clone();
    let a_end = a["end"].as_i64().expect("a end");
    let b_clip = b["clip"].as_u64().expect("b clip");
    assert_eq!(b["start"].as_i64(), Some(a_end), "the demo's last two V1 clips are contiguous");

    // 先把最后一个片段拖进右侧空白，制造一个间隙再拖回来。
    let rb = d.rect(&format!("timeline.clip.{b_clip}"));
    let y = rb[1] + rb[3] / 2.0;
    let from = (rb[0] + rb[2] / 2.0, y);
    d.ok("ui.drag", json!({"from": {"x": from.0, "y": from.1}, "to": {"x": from.0 + 80.0, "y": from.1}, "steps": 12}));
    d.frames(3);
    let rb2 = d.rect(&format!("timeline.clip.{b_clip}"));
    assert!(rb2[0] > rb[0] + 20.0, "the clip moved right: {} -> {}", rb[0], rb2[0]);

    // 释放点让 b 的起点落在 a.end 右侧约 3 px：磁吸应把它拉回 a.end。
    let ra = d.rect(&format!("timeline.clip.{}", a["clip"].as_u64().expect("a clip")));
    let a_end_x = ra[0] + ra[2];
    let dx = a_end_x + 3.0 - rb2[0];
    d.ok("ui.drag", json!({"from": {"x": rb2[0] + rb2[2] / 2.0, "y": y}, "to": {"x": rb2[0] + rb2[2] / 2.0 + dx, "y": y}, "steps": 12}));
    d.frames(3);
    let after = d.exec("sequence.inspect", json!({}));
    let moved = after["video"][0]["items"].as_array().expect("V1 items").iter().find(|i| i["clip"].as_u64() == Some(b_clip)).cloned().expect("the moved clip");
    assert_eq!(moved["start"].as_i64(), Some(a_end), "snapped to the neighbour's edge, not {:.1} px off", 3.0);
}

/// #235 主轨磁吸（时间）：把 V2 上的片段拖到主轨（V1）片段边缘附近时，落点吸附到 V1 的边缘，
/// 即使另一条轨的片段边缘像素上更近。结果按 `sequence.inspect` 的 start 断言，不看像素。
#[test]
fn dragging_a_clip_snaps_to_the_primary_track_edge() {
    let mut d = Driver::demo();
    d.exec("sequence.snap", json!({"on": true}));
    // 缩小时间轴，让 V1 的边缘与 V2 的边缘同时落在 SNAP_PX（9 px）内。
    d.ok("ui.set", json!({"timeline": {"pps": 20.0, "scroll": 0.0}}));
    d.frames(60);
    let pps = d.ok("ui.inspect", json!({}))["ui"]["timeline"]["pps"].as_f64().expect("timeline pps");
    assert!((14.0..26.0).contains(&pps), "the test needs 14 < pps < 26 px/s, got {pps}");

    let before = d.exec("sequence.inspect", json!({}));
    let overlay = before["video"][1]["items"][0].clone();
    let clip = overlay["clip"].as_u64().expect("the V2 overlay's clip");
    let item = overlay["item"].as_u64().expect("the V2 overlay's item");
    let start = Tick(overlay["start"].as_i64().expect("overlay start"));
    // 主轨 V1 上第三个片段的起点（约 9.5 s）：主轨磁吸要吸到这条边缘。
    let primary_edge = Tick(before["video"][0]["items"][2]["start"].as_i64().expect("the V1 edge"));
    // 在 V2 的 10 s 处再放一个片段：它离落点更近，没有主轨磁吸时会先吸到它。
    let placed = d.exec("timeline.place", json!({"item": item, "track": "V2", "seconds": 10.0, "duration": Tick::from_seconds_f64(0.25).0}));
    let placed_clip = placed["clips"][0].as_u64().expect("the placed V2 clip");
    d.frames(4);
    let seq = d.exec("sequence.inspect", json!({}));
    let v2_edge = Tick(
        seq["video"][1]["items"].as_array().expect("V2 items").iter().find(|i| i["clip"].as_u64() == Some(placed_clip)).expect("the placed V2 clip")["start"]
            .as_i64()
            .expect("placed start"),
    );
    // 落点取两条边缘之间、离 V2 边缘更近的位置（o > gap/2），但仍在 V1 边缘的 SNAP_PX 内（o < 9）。
    let gap_px = (v2_edge.seconds() - primary_edge.seconds()) * pps;
    assert!((4.0..16.0).contains(&gap_px), "the two edges are {gap_px:.1} px apart at {pps:.1} px/s");
    let offset_px = (gap_px / 2.0 + 9.0) / 2.0;
    let dx = (primary_edge.seconds() + offset_px / pps - start.seconds()) * pps;

    let r = d.rect(&format!("timeline.clip.{clip}"));
    let from = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    d.ok("ui.drag", json!({"from": {"x": from.0, "y": from.1}, "to": {"x": from.0 + dx, "y": from.1}, "steps": 12}));
    d.frames(4);
    let after = d.exec("sequence.inspect", json!({}));
    let moved = after["video"][1]["items"].as_array().expect("V2 items").iter().find(|i| i["clip"].as_u64() == Some(clip)).cloned().expect("the moved overlay");
    assert_eq!(moved["start"].as_i64(), Some(primary_edge.0), "snapped to V1's edge at {:.3} s, not to the nearer V2 clip", primary_edge.seconds());
    assert_eq!(d.ok("ui.inspect", json!({}))["timeline"]["primaryTrackSnapped"], json!(true), "ui.inspect reports the primary-track magnet");
}

/// #235 主轨磁吸的语义收紧：在 V1 行内原地拖动（轨道没变、时间边缘也没真的被拉到 V1 边缘）
/// 不算主轨磁吸，`primaryTrackSnapped` 必须为 false；从 V2 拖到 V1 的用例仍是 true。
#[test]
fn dragging_in_place_on_the_primary_track_is_not_a_magnet() {
    let mut d = Driver::demo();
    d.exec("sequence.snap", json!({"on": true}));
    d.frames(3);
    let seq = d.exec("sequence.inspect", json!({}));
    let items = seq["video"][0]["items"].as_array().expect("V1 items").clone();
    assert!(items.len() >= 2, "demo V1 has at least two clips");
    let target = items[1].clone();
    let clip = target["clip"].as_u64().expect("clip");
    let start = target["start"].as_i64().expect("start");
    let r = d.rect(&format!("timeline.clip.{clip}"));

    // 只竖直移动 8 px，指针仍在 V1 行内；水平不动，时间边缘没有真的换位。
    let x = r[0] + r[2] / 2.0;
    let y = r[1] + r[3] / 2.0;
    d.ok("ui.drag", json!({"from": {"x": x, "y": y}, "to": {"x": x, "y": y + 8.0}, "steps": 10}));
    d.frames(4);

    let after = d.exec("sequence.inspect", json!({}));
    let moved = after["video"][0]["items"]
        .as_array()
        .expect("V1 items")
        .iter()
        .find(|i| i["clip"].as_u64() == Some(clip))
        .cloned()
        .expect("an in-place drag keeps the clip on V1");
    assert_eq!(moved["start"].as_i64(), Some(start), "an in-place drag must not move the clip in time");
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["timeline"]["primaryTrackSnapped"], json!(false), "staying on V1 is not a primary-track magnet: {info}");
}

/// #235 主轨磁吸（轨道）：把片段拖到主轨 V1 附近（指针还在 V2 行的下缘）时，落点磁吸到 V1。
/// 落点按 `sequence.inspect` 的轨道归属断言，不看像素。
#[test]
fn dragging_a_clip_near_the_primary_track_lands_on_it() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"timeline": {"pps": 20.0, "scroll": 0.0}}));
    d.frames(60);
    let seq = d.exec("sequence.inspect", json!({}));
    let clip = seq["video"][1]["items"][0]["clip"].as_u64().expect("the V2 overlay's clip");
    let r = d.rect(&format!("timeline.clip.{clip}"));
    let v1 = d.rect("timeline.track.V1.header");
    // 指针停在 V1 行上缘上方 4 px：仍在 V2 行里，但已在 SNAP_PX（9 px）的磁吸范围内。
    let to_y = v1[1] - 4.0;
    d.ok("ui.drag", json!({"from": {"x": r[0] + r[2] / 2.0, "y": r[1] + r[3] / 2.0}, "to": {"x": r[0] + r[2] / 2.0, "y": to_y}, "steps": 10}));
    d.frames(4);
    let after = d.exec("sequence.inspect", json!({}));
    let on = |track: usize| after["video"][track]["items"].as_array().expect("track items").iter().any(|i| i["clip"].as_u64() == Some(clip));
    assert!(on(0), "the clip landed on the primary track V1: {after}");
    assert!(!on(1), "the clip left V2 instead of staying on its row: {after}");
    assert_eq!(d.ok("ui.inspect", json!({}))["timeline"]["primaryTrackSnapped"], json!(true), "ui.inspect reports the primary-track magnet");
}

/// 剪映式轨道头的徽标：26×18、贴在轨道头左侧 2 px、整块落在自己的行里；徽标就是源补丁
/// （只换画法，命中与命令不变），类型小字与一行图标都排在徽标右侧。
///
/// 控制通道跑在 1280×1024 的逻辑点上（桌面尺寸），矩形由 `ui.elements` 读出，不是从代码推的。
/// `JY_HEADER_PNG=1` 时另存一张 `ui-screenshots/jianying-track-badges-1280x1024.png` 供人工比对。
#[test]
fn jianying_track_badges_sit_at_the_left_of_their_row() {
    let mut d = Driver::sized(egui::vec2(1280.0, 1024.0));
    // 字幕轨头也统一成同一枚徽标：加一条 C1（并选中一条字幕，走活动行的白边/行底高亮），
    // 和 V1/A1 并排断言。
    d.exec("captions.newTrack", json!({"format": "Subtitle", "name": "C1"}));
    let cap = d.exec("captions.add", json!({"track": "C1", "text": "Hi", "seconds": 1.0}))["caption"].as_u64().expect("a caption");
    d.exec("captions.select", json!({"captions": [cap]}));
    d.frames(4);
    for label in ["V1", "A1"] {
        let header = d.rect(&format!("timeline.track.{label}.header"));
        let badge = d.rect(&format!("timeline.track.{label}.badge"));
        eprintln!("1280x1024 {label}: header = {header:?}, badge = {badge:?}");
        assert!(header[2] > 0.0 && header[2] <= MAX_HEADER_W, "{label} header is {:.1} px wide", header[2]);
        assert!(header[3] >= 40.0, "{label} row is {:.1} px high: the badge layout needs the two-row header", header[3]);
        assert!((badge[2] - 26.0).abs() < 0.5, "{label} badge is {:.1} px wide, want 26", badge[2]);
        assert!((badge[3] - 18.0).abs() < 0.5, "{label} badge is {:.1} px high, want 18", badge[3]);
        assert!((badge[0] - header[0] - 2.0).abs() < 0.5, "{label} badge starts at x {:.1}, want the header's left + 2", badge[0]);
        assert!(badge[1] >= header[1] - 0.5 && badge[1] + badge[3] <= header[1] + header[3] + 0.5, "{label} badge {badge:?} leaves its row {header:?}");
        // 徽标与源补丁是同一个矩形：命中不改，只是画成了剪映的彩色徽标。
        assert_eq!(badge, d.rect(&format!("timeline.track.{label}.sourcePatch")), "{label} badge must be the source-patch hit rect");
        // 类型小字 + 右上角的图标（眼睛/静音、锁）在徽标右侧，且与徽标同一条中心线。
        let icons = if label == "V1" {
            vec![format!("timeline.track.{label}.enabled"), format!("timeline.track.{label}.locked")]
        } else {
            vec![format!("timeline.track.{label}.muted"), format!("timeline.track.{label}.locked")]
        };
        for id in std::iter::once(format!("timeline.track.{label}.target")).chain(icons) {
            let r = d.rect(&id);
            assert!(r[0] >= badge[0] + badge[2], "{id} starts at x {:.1}, left of the badge's right edge {:.1}", r[0], badge[0] + badge[2]);
            assert!(r[1] >= header[1] - 0.5 && r[1] + r[3] <= header[1] + header[3] + 0.5, "{id} leaves its row: {r:?} vs {header:?}");
            assert!((r[1] + r[3] / 2.0 - (badge[1] + badge[3] / 2.0)).abs() < 1.0, "{id} is not on the badge's centre line: {r:?} vs {badge:?}");
            eprintln!("1280x1024 {label}: {id} -> {r:?}");
        }
    }
    // 视频轨在音频轨上方：V1 的徽标比 A1 高。
    let (v1, a1) = (d.rect("timeline.track.V1.badge"), d.rect("timeline.track.A1.badge"));
    assert!(v1[1] < a1[1], "V1 badge y {} must be above A1 badge y {}", v1[1], a1[1]);

    // 字幕轨头：同为 26×18、贴头左 2 px 的徽标（C1），旧的 `timeline.captionTrack.C1`
    // 只读别名保留同一个矩形；锁 / 眼睛是徽标右侧、与徽标同一条中心线的 11 px 图标。
    let cheader = d.rect("timeline.captionTrack.C1.header");
    let cbadge = d.rect("timeline.track.C1.badge");
    eprintln!("1280x1024 C1: header = {cheader:?}, badge = {cbadge:?}");
    assert!(cheader[2] > 0.0 && cheader[2] <= MAX_HEADER_W, "C1 header is {:.1} px wide", cheader[2]);
    assert!((cbadge[2] - 26.0).abs() < 0.5, "C1 badge is {:.1} px wide, want 26", cbadge[2]);
    assert!((cbadge[3] - 18.0).abs() < 0.5, "C1 badge is {:.1} px high, want 18", cbadge[3]);
    assert!((cbadge[0] - cheader[0] - 2.0).abs() < 0.5, "C1 badge starts at x {:.1}, want the header's left + 2", cbadge[0]);
    assert!(cbadge[1] >= cheader[1] - 0.5 && cbadge[1] + cbadge[3] <= cheader[1] + cheader[3] + 0.5, "C1 badge {cbadge:?} leaves its row {cheader:?}");
    assert_eq!(cbadge, d.rect("timeline.captionTrack.C1"), "the legacy caption badge alias keeps the same rect");
    for id in ["timeline.captionTrack.C1.locked", "timeline.captionTrack.C1.enabled"] {
        let r = d.rect(id);
        eprintln!("1280x1024 C1: {id} -> {r:?}");
        assert!(r[0] >= cbadge[0] + cbadge[2], "{id} starts at x {:.1}, left of the C1 badge's right edge {:.1}", r[0], cbadge[0] + cbadge[2]);
        assert!(r[1] >= cheader[1] - 0.5 && r[1] + r[3] <= cheader[1] + cheader[3] + 0.5, "{id} leaves its row: {r:?} vs {cheader:?}");
        assert!((r[1] + r[3] / 2.0 - (cbadge[1] + cbadge[3] / 2.0)).abs() < 1.0, "{id} is not on the badge's centre line: {r:?} vs {cbadge:?}");
    }

    if std::env::var_os("JY_HEADER_PNG").is_some() {
        d.frames(2);
        if let Ok(img) = d.harness.render() {
            let png = filmcraft_ui_egui::control::encode_png(img.as_raw(), img.width(), img.height()).expect("png");
            let path = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ui-screenshots").join("jianying-track-badges-1280x1024.png");
            std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
            std::fs::write(&path, png).expect("write");
            eprintln!("badge screenshot: {}", path.display());
        }
    }
}
