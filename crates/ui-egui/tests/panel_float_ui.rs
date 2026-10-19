//! 悬浮层面的三条验收：☰ 菜单的分组二级面板、单页壳之上的浮动面板、以及打开「滤镜」
//! （Lumetri Color）时不得盖住 Program 监视器。
//!
//! 断言的都是可观察事实：`ui.inspect` 的状态、`ui.elements` 的矩形，以及控制通道点击之后
//! 面板是否仍然存在。写法沿用 `tests/layout_ui.rs` 的 Driver（egui_kittest Harness + 控制通道）。

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

    fn has(&mut self, id: &str) -> bool {
        self.elements(id).as_array().is_some_and(|list| list.iter().any(|element| element["id"] == json!(id)))
    }

    fn rect(&mut self, id: &str) -> [f64; 4] {
        let elements = self.elements(id);
        let element = elements
            .as_array()
            .and_then(|list| list.iter().find(|element| element["id"] == json!(id)))
            .unwrap_or_else(|| panic!("{id} is not drawn: {elements}"));
        let rect = element["rect"].as_array().expect("rect");
        [rect[0].as_f64().unwrap(), rect[1].as_f64().unwrap(), rect[2].as_f64().unwrap(), rect[3].as_f64().unwrap()]
    }
}

/// 两个矩形是否有面积重叠（相切不算）。
fn overlaps(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// OpenSpec 任务「分组视图目前是整页替换而不是悬浮二级面板」：
/// 点开 ☰ 菜单里的一级分组后，原根列表仍然画着，分组命令出现在一个悬浮二级面板里，
/// 而且这个面板不铺满内容区（它只是叠在上面的一层）。
#[test]
fn grouped_menu_opens_a_floating_second_level_panel() {
    for (width, height) in [(390.0_f64, 884.0_f64), (900.0, 1024.0)] {
        let mut d = Driver::new(width as f32, height as f32);
        d.frames(3);
        d.ok("ui.click", json!({"id": "jy.menu"}));
        d.frames(3);
        let roots = d.elements("jy.menu.group.");
        assert!(roots.as_array().is_some_and(|list| list.len() > 3), "{width}: the ☰ menu should list its roots: {roots}");

        d.ok("ui.click", json!({"id": "jy.menu.group.File"}));
        d.frames(3);
        // 二级面板出现，且列着该分组的命令。
        let panel = d.rect("group.panel");
        let rows = d.elements("group.row.");
        assert!(rows.as_array().is_some_and(|list| !list.is_empty()), "{width}: the floating group panel should list File's commands: {rows}");
        // 原来的内容（根列表）没有被替换：File 根仍然存在、仍可点。
        assert!(
            d.elements("jy.menu.group.").as_array().is_some_and(|list| list.iter().any(|element| element["id"] == json!("jy.menu.group.File"))),
            "{width}: opening a group must keep the original root list visible"
        );
        // 悬浮而不整页：在窗口内，且宽、高、面积都不占满内容区。
        let info = d.ok("ui.inspect", json!({}));
        let w = info["window"][0].as_f64().expect("window width");
        let h = info["window"][1].as_f64().expect("window height");
        assert!((w - width).abs() < 0.5 && (h - height).abs() < 0.5, "{width}: the harness should be {width}x{height}, got {w}x{h}");
        assert!(panel[0] >= 0.0 && panel[0] + panel[2] <= w + 0.5, "{width}: the group panel starts or ends off screen: {panel:?}");
        assert!(panel[1] >= 0.0 && panel[1] + panel[3] <= h + 0.5, "{width}: the group panel overflows vertically: {panel:?}");
        assert!(panel[2] < w * 0.9 && panel[3] < h * 0.75, "{width}: the group panel is a full-page replacement, not a floating panel: {panel:?}");
        assert!(panel[2] * panel[3] < w * h * 0.5, "{width}: the group panel covers over half the page: {panel:?}");

        // 标题栏上的「返回」收起二级面板，根列表还在。
        d.ok("ui.click", json!({"id": "group.back"}));
        d.frames(3);
        assert!(!d.has("group.panel"), "{width}: back should close the floating group panel");
        assert!(d.has("jy.menu.group.File"), "{width}: closing the panel must keep the root list");
    }
}

/// OpenSpec 任务「单页壳不绘制浮动面板」：
/// Compact 下即使存在浮动窗口，壳（顶栏 / 预览 / 时间轴）也照常画，浮动监视器叠在壳之上。
#[test]
fn compact_shell_keeps_a_floating_monitor_above_the_shell() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    // 先确认当前确实是单页壳，否则下面的断言说明不了问题。
    for id in ["jy.topbar", "jy.preview", "jy.timeline"] {
        assert!(d.has(id), "the compact shell should draw {id} before any floating panel");
    }
    assert!(d.ok("ui.inspect", json!({}))["ui"]["floating"].as_array().is_some_and(Vec::is_empty), "no floating panels yet");

    d.ok("engine.execute", json!({"command": "window.floatPanel", "params": {"panel": "Source", "rect": [24.0, 140.0, 220.0, 200.0]}}));
    d.frames(4);
    let info = d.ok("ui.inspect", json!({}));
    let floating = info["ui"]["floating"].as_array().expect("floating state");
    assert_eq!(floating.len(), 1, "the Source monitor should be floating: {info}");
    assert_eq!(floating[0]["panels"], json!(["Source"]), "the floating group holds Source: {info}");
    assert_eq!(floating[0]["rect"], json!([24.0, 140.0, 220.0, 200.0]), "the requested rect is kept: {info}");

    // 壳没有退回 dock 树：单页壳的骨架元素仍在（旧行为下它们会被 dock 组替换掉）。
    for id in ["jy.topbar", "jy.preview", "jy.timeline", "panel.Program"] {
        assert!(d.has(id), "the shell (and its preview) must keep drawing {id} with a floating panel on top");
    }
    // 浮动监视器也真的画出来了，且落在请求的矩形里。
    let source = d.rect("panel.Source");
    assert!((source[0] - 24.0).abs() < 1.5 && (source[2] - 220.0).abs() < 1.5, "the floating Source monitor is misplaced: {source:?}");
    assert!(source[1] >= 140.0 && source[1] + source[3] <= 340.5, "the floating Source monitor left its rect: {source:?}");
    assert!(d.has("panel.float.0.move"), "the floating monitor should expose its move handle");
}

/// 已记录缺口：菜单 / Tab 激活「滤镜」（Lumetri Color）时，面板不能被插进 Program 所在 tab 组。
/// 断言 Program 仍然画着、位置不变，且与 Lumetri Color 不重叠。
#[test]
fn activating_lumetri_color_keeps_the_program_monitor_visible() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    // Jianying 工作区里没有 Lumetri Color：先确认它真的不在，否则下面的断言没有意义。
    assert!(!d.has("panel.LumetriColor"), "the fixture should not have Lumetri Color docked yet");
    let program_before = d.rect("panel.Program");

    // Wide 的一级 Tab 条「滤镜」：走 `show_panel(LumetriColor)`。
    d.ok("ui.click", json!({"id": "jy.tabstrip.filters"}));
    d.frames(4);
    assert!(d.has("panel.LumetriColor"), "the filters tab should open the Lumetri Color panel");
    // 关键回归：Program 组没有被当成新面板的落点，预览仍然画着、矩形也没动。
    let program_after = d.rect("panel.Program");
    let lumetri = d.rect("panel.LumetriColor");
    assert_eq!(program_before, program_after, "the preview moved when Lumetri Color opened");
    assert!(!overlaps(program_after, lumetri), "Lumetri Color covers the Program monitor: {program_after:?} {lumetri:?}");
}
