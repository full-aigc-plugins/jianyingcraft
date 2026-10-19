//! 矮窗口下的两个单页壳弹层：顶栏「更多」溢出菜单与 ☰ 分组悬浮面板。
//!
//! 两个弹层都在「内容区（body）」里浮动，旧实现把宽 / 高各自夹在 96×120 / 140 的下限上，
//! 并且「更多」的 top 没有上限、分组面板的 y 由 `body.max.y` 反推，窗口一矮弹层就会压出
//! body 的右 / 下边界。这里断言的可观察事实是：`ui.elements` 报告的矩形必须落在窗口和
//! 内容区之内，分组面板的高度不能超过可用高度。
//!
//! 写法沿用 `tests/layout_ui.rs` 的 Driver（egui_kittest Harness + 控制通道）。

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

    fn rect(&mut self, id: &str) -> [f64; 4] {
        let elements = self.elements(id);
        let element = elements
            .as_array()
            .and_then(|list| list.iter().find(|element| element["id"] == json!(id)))
            .unwrap_or_else(|| panic!("{id} is not drawn: {elements}"));
        rect_of(element)
    }
}

/// 元素矩形 `[x, y, w, h]`。
fn rect_of(element: &Value) -> [f64; 4] {
    let rect = element["rect"].as_array().expect("rect");
    [rect[0].as_f64().unwrap(), rect[1].as_f64().unwrap(), rect[2].as_f64().unwrap(), rect[3].as_f64().unwrap()]
}

/// 断言 `rect` 落在 `bounds` 里（`bounds` 是 `[x, y, w, h]`，0.5 px 容差来自浮点取整）。
fn assert_inside(rect: [f64; 4], bounds: [f64; 4], what: &str) {
    let (right, bottom) = (bounds[0] + bounds[2], bounds[1] + bounds[3]);
    assert!(rect[0] >= bounds[0] - 0.5, "{what}: left edge {} is outside {bounds:?}", rect[0]);
    assert!(rect[1] >= bounds[1] - 0.5, "{what}: top edge {} is outside {bounds:?}", rect[1]);
    assert!(rect[0] + rect[2] <= right + 0.5, "{what}: right edge {} crosses {right} ({rect:?})", rect[0] + rect[2]);
    assert!(rect[1] + rect[3] <= bottom + 0.5, "{what}: bottom edge {} crosses {bottom} ({rect:?})", rect[1] + rect[3]);
}

/// 内容区 `[x, y, w, h]`：`jy.topbar` 的左上角到 `jy.timeline` 的右下角，也就是 body。
/// 弹层画在 body 之上，`jy.timeline` 仍然注册着，所以这两个元素是 body 的可靠代理。
fn body_of(d: &mut Driver) -> [f64; 4] {
    let topbar = d.rect("jy.topbar");
    let timeline = d.rect("jy.timeline");
    [topbar[0], topbar[1], timeline[0] + timeline[2] - topbar[0], timeline[1] + timeline[3] - topbar[1]]
}

/// 「更多」溢出菜单里的 10 个一级 Tab（行 id 沿用顶栏的 `jy.tab.<slug>`）。
const OVERFLOW_TAB_IDS: [&str; 10] = [
    "jy.tab.sticker",
    "jy.tab.effects",
    "jy.tab.transitions",
    "jy.tab.subtitle",
    "jy.tab.smart",
    "jy.tab.filters",
    "jy.tab.adjust",
    "jy.tab.templates",
    "jy.tab.avatar",
    "jy.tab.plugins",
];

/// 矮窗口点开「更多」：弹层与它的 10 行入口都必须在窗口内、且不越过 body 的右 / 下边界；
/// 窗口矮到连菜单下限都放不下时（390×160），弹层本身也必须收进 body。
#[test]
fn more_menu_stays_inside_the_body_in_a_short_window() {
    for (width, height) in [(390.0_f64, 360.0_f64), (390.0, 160.0)] {
        let mut d = Driver::new(width as f32, height as f32);
        d.frames(3);
        d.ok("ui.click", json!({"id": "jy.tab.more"}));
        d.frames(4);
        let info = d.ok("ui.inspect", json!({}));
        assert_eq!(info["ui"]["jy_tab_more"], json!(true), "{width}x{height}: 更多 did not open: {info}");
        assert_eq!(info["window"], json!([width, height]), "{width}x{height}: the harness should be that size");

        let window = [0.0, 0.0, width, height];
        let body = body_of(&mut d);
        let panel = d.rect("jy.tab.more.panel");
        assert_inside(panel, window, &format!("{width}x{height}: the 更多 popup"));
        assert_inside(panel, body, &format!("{width}x{height}: the 更多 popup"));
        assert!(panel[2] > 0.0 && panel[3] > 0.0, "{width}x{height}: the 更多 popup collapsed: {panel:?}");

        let mut seen = Vec::new();
        for element in d.elements("jy.tab.").as_array().expect("elements").clone() {
            let id = element["id"].as_str().unwrap_or("");
            if !OVERFLOW_TAB_IDS.contains(&id) {
                continue;
            }
            seen.push(id.to_string());
            let rect = rect_of(&element);
            assert!(rect[2] <= window[2] + 0.5, "{width}x{height}: {id} is wider than the window: {rect:?}");
            // 390×360 时 10 行整条放得下，行本身也不许越界；160 px 高时列表必须滚动，
            // 行矩形是滚动内容（被 ScrollArea 裁掉），只校验弹层外壳。
            if height >= 360.0 {
                assert_inside(rect, window, &format!("{width}x{height}: {id}"));
                assert_inside(rect, body, &format!("{width}x{height}: {id}"));
            }
        }
        assert_eq!(seen.len(), OVERFLOW_TAB_IDS.len(), "{width}x{height}: the 更多 menu should list all 10 overflow tabs: {seen:?}");
    }
}

/// 900×420 打开 ☰ 菜单并展开一个分组：悬浮二级面板必须整体在 body 内，高度不超过可用高度。
/// 900×160 时 body 连旧实现的 140 px 高度下限都放不下，面板同样必须收进 body。
#[test]
fn group_panel_stays_inside_the_body_in_a_short_window() {
    for (width, height) in [(900.0_f64, 420.0_f64), (900.0, 160.0)] {
        let mut d = Driver::new(width as f32, height as f32);
        d.frames(3);
        d.ok("ui.click", json!({"id": "jy.menu"}));
        d.frames(3);
        d.ok("ui.click", json!({"id": "jy.menu.group.File"}));
        d.frames(4);
        let info = d.ok("ui.inspect", json!({}));
        assert_eq!(info["window"], json!([width, height]), "{width}x{height}: the harness should be that size");

        let window = [0.0, 0.0, width, height];
        let body = body_of(&mut d);
        let panel = d.rect("group.panel");
        assert_inside(panel, window, &format!("{width}x{height}: the group panel"));
        assert_inside(panel, body, &format!("{width}x{height}: the group panel"));
        // 面板与 body.shrink(8.0) 求交，所以至少留出 8 px 边距；高度也不可能超过可用高度。
        let inset = [body[0] + 8.0, body[1] + 8.0, body[2] - 16.0, body[3] - 16.0];
        assert_inside(panel, inset, &format!("{width}x{height}: the group panel must respect the body inset"));
        assert!(panel[3] <= inset[3] + 0.5, "{width}x{height}: the group panel ({}) is taller than the available height ({})", panel[3], inset[3]);
        assert!(panel[2] > 0.0 && panel[3] > 0.0, "{width}x{height}: the group panel collapsed: {panel:?}");

        // 面板里真的列着 File 的命令（不是只剩一个空壳）。
        let rows = d.elements("group.row.");
        assert!(rows.as_array().is_some_and(|list| !list.is_empty()), "{width}x{height}: the group panel should list File's commands: {rows}");
    }
}
