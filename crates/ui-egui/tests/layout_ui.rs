//! 三档视口的布局档位验收（剪映式：Mobile / Tablet / Desktop）。
//!
//! 断言控制通道 `ui.inspect` 报告的 `layoutClass` 与 `docs/derivation/02-ia-mapping.md` §2
//! 的断点一致。设置 `FILMCRAFT_UI_SNAPSHOT_DIR=<dir>` 时还会离屏渲染三张 `layout-*.png`。

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    snapshots: Option<std::path::PathBuf>,
}

impl Driver {
    fn new(width: f32, height: f32) -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let snapshots = filmcraft_ui_egui::test_support::snapshot_dir();
        let mut b = Harness::builder().with_size(egui::vec2(width, height)).with_max_steps(10_000);
        if snapshots.is_some() {
            b = b.wgpu();
        }
        let harness = b.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx, snapshots };
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

    fn snapshot(&mut self, name: &str) {
        let Some(dir) = self.snapshots.clone() else { return };
        self.frames(2);
        match self.harness.render() {
            Ok(img) => {
                std::fs::create_dir_all(&dir).unwrap();
                img.save(dir.join(format!("layout-{name}.png"))).unwrap();
            }
            Err(e) => eprintln!("snapshot {name} skipped: {e}"),
        }
    }
}

/// 顶栏「更多」溢出菜单里的 10 个一级 Tab（沿用收敛前的自动化 id）。
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

/// `ui.elements {prefix:"jy.tab."}` 报出的 id（同时带出 `jy.tab.more` 与 `jy.tab.inspector`）。
fn tab_ids(d: &mut Driver) -> Vec<String> {
    let elements = d.ok("ui.elements", json!({"prefix": "jy.tab."}));
    elements.as_array().expect("elements").iter().filter_map(|element| element["id"].as_str().map(str::to_string)).collect()
}

/// 顶栏直排的 3 个主 Tab（「更多」与「属性」不算），顺序即绘制顺序。
fn primary_tab_ids(ids: &[String]) -> Vec<String> {
    ids.iter().filter(|id| matches!(id.as_str(), "jy.tab.cut" | "jy.tab.captions" | "jy.tab.audio")).cloned().collect()
}

/// 元素矩形 `[x, y, w, h]`；缺了就是测试失败。
fn rect_of(d: &mut Driver, id: &str) -> [f64; 4] {
    let elements = d.ok("ui.elements", json!({"prefix": id}));
    let element = elements.as_array().and_then(|list| list.iter().find(|element| element["id"] == json!(id))).unwrap_or_else(|| panic!("{id} missing in {elements}"));
    let r = element["rect"].as_array().expect("rect");
    [r[0].as_f64().unwrap(), r[1].as_f64().unwrap(), r[2].as_f64().unwrap(), r[3].as_f64().unwrap()]
}

/// 该视口下 `ui.inspect` 报告的布局档位，并顺带完成一次空态截图。
#[test]
fn layout_class_matches_the_three_target_viewports() {
    let cases = [("mobile", 390.0, 884.0, "compact"), ("tablet", 768.0, 1024.0, "medium"), ("desktop", 1280.0, 1024.0, "wide")];
    for (name, w, h, expected) in cases {
        let mut d = Driver::new(w, h);
        let info = d.ok("ui.inspect", json!({}));
        assert_eq!(info["layoutClass"], json!(expected), "{name}: {w}x{h} should be {expected}, got {}", info["layoutClass"]);
        // 报告的实际窗口宽度必须落在该档位内，避免"档位对了但尺寸没生效"。
        let reported = info["window"][0].as_f64().unwrap_or(0.0) as f32;
        assert!(
            matches!(expected, "compact") && reported < 768.0
                || matches!(expected, "medium") && (768.0..=1024.0).contains(&reported)
                || matches!(expected, "wide") && reported > 1024.0,
            "{name}: reported width {reported} is outside {expected}"
        );
        d.snapshot(name);
    }
}

/// 同一序列在重排前后结构不变：这里用空态 + 演示工程各取一次 `sequence_inspect` 做基线，
/// 后续布局改造必须保持该 JSON 的 clips/effects 不变（IA-4 的前置断言）。
#[test]
fn demo_sequence_inspect_is_stable_across_viewports() {
    let mut mobile = Driver::new(390.0, 884.0);
    let mobile_seq = mobile.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    let mut desktop = Driver::new(1280.0, 1024.0);
    let desktop_seq = desktop.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    assert_eq!(mobile_seq["clips"], desktop_seq["clips"], "clips differ across viewports");
    assert_eq!(mobile_seq["effects"], desktop_seq["effects"], "effects differ across viewports");
}

/// Compact 下主区是单列：顶部条 → 预览 → 时间轴，时间轴 ≥ 220 px 且区域互不重叠；
/// 宽屏仍走 dock 树，不出现 `jy.*` 这套壳。
#[test]
fn compact_shell_stacks_preview_over_timeline() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let elements = d.ok("ui.elements", json!({"prefix": "jy."}));
    let rect = |id: &str| -> [f64; 4] {
        let found = elements.as_array().expect("elements").iter().find(|e| e["id"] == json!(id)).unwrap_or_else(|| panic!("{id} missing in {elements}"));
        let r = found["rect"].as_array().expect("rect");
        [r[0].as_f64().unwrap(), r[1].as_f64().unwrap(), r[2].as_f64().unwrap(), r[3].as_f64().unwrap()]
    };
    let topbar = rect("jy.topbar");
    let preview = rect("jy.preview");
    let timeline = rect("jy.timeline");
    assert!(timeline[3] >= 220.0, "timeline too short: {timeline:?}");
    assert!(topbar[1] + topbar[3] <= preview[1] + 0.5, "topbar overlaps preview");
    assert!(preview[1] + preview[3] <= timeline[1] + 0.5, "preview overlaps timeline");
    assert!(preview[2] > 380.0, "preview is not full width: {preview:?}");
    d.snapshot("compact-shell");

    let mut wide = Driver::new(1280.0, 1024.0);
    wide.frames(3);
    let none = wide.ok("ui.elements", json!({"prefix": "jy."}));
    assert!(none.as_array().is_none_or(|a| a.is_empty()), "wide layout must not draw the compact shell: {none}");
}

/// Compact 抽屉只压缩预览区：打开素材抽屉后时间轴位置与高度不变、文档结构不变，
/// 关闭后恢复；再次点击同一按钮会收起。
#[test]
fn compact_drawer_compresses_preview_only() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let rect_of = |value: &Value, id: &str| -> [f64; 4] {
        let found = value.as_array().expect("elements").iter().find(|e| e["id"] == json!(id)).unwrap_or_else(|| panic!("{id} missing in {value}"));
        let r = found["rect"].as_array().expect("rect");
        [r[0].as_f64().unwrap(), r[1].as_f64().unwrap(), r[2].as_f64().unwrap(), r[3].as_f64().unwrap()]
    };
    let before = d.ok("ui.elements", json!({"prefix": "jy."}));
    let seq_before = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    let timeline_before = rect_of(&before, "jy.timeline");

    d.ok("ui.click", json!({"id": "jy.tab.cut"}));
    d.frames(3);
    let opened = d.ok("ui.elements", json!({"prefix": "jy."}));
    let timeline_open = rect_of(&opened, "jy.timeline");
    let drawer = rect_of(&opened, "jy.drawer");
    let preview_open = rect_of(&opened, "jy.preview");
    assert_eq!(timeline_before, timeline_open, "opening a drawer must not move or resize the timeline");
    assert!(preview_open[1] + preview_open[3] <= drawer[1] + 0.5, "preview overlaps drawer");
    assert!(drawer[1] + drawer[3] <= timeline_open[1] + 0.5, "drawer overlaps timeline");
    assert!(drawer[3] >= 160.0, "drawer too short: {drawer:?}");
    let seq_open = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    assert_eq!(seq_before["clips"], seq_open["clips"], "opening a drawer changed the sequence");
    d.snapshot("compact-drawer");

    d.ok("ui.click", json!({"id": "jy.tab.cut"}));
    d.frames(3);
    let closed = d.ok("ui.elements", json!({"prefix": "jy."}));
    assert!(closed.as_array().is_some_and(|a| a.iter().all(|e| e["id"] != json!("jy.drawer"))), "drawer did not close: {closed}");
    assert_eq!(rect_of(&closed, "jy.timeline"), timeline_before, "timeline did not return to its place");
}

/// 三个一级 Tab 各自打开对应抽屉，切换时时间轴不动；再点当前 Tab 收起。
#[test]
fn compact_tabs_switch_the_drawer() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let timeline = |d: &mut Driver| -> [f64; 4] {
        let elements = d.ok("ui.elements", json!({"prefix": "jy.timeline"}));
        let r = elements[0]["rect"].as_array().expect("rect");
        [r[0].as_f64().unwrap(), r[1].as_f64().unwrap(), r[2].as_f64().unwrap(), r[3].as_f64().unwrap()]
    };
    let baseline = timeline(&mut d);
    for (id, tab) in [("jy.tab.cut", "cut"), ("jy.tab.captions", "captions"), ("jy.tab.audio", "audio")] {
        d.ok("ui.click", json!({"id": id}));
        d.frames(3);
        let info = d.ok("ui.inspect", json!({}));
        assert_eq!(info["ui"]["jy_tab"], json!(tab), "tab state after clicking {id}");
        assert!(!info["ui"]["jy_drawer"].is_null(), "{id} should open a drawer");
        let elements = d.ok("ui.elements", json!({"prefix": "jy.drawer"}));
        assert!(!elements.as_array().expect("elements").is_empty(), "{id} drawer is not drawn");
        if tab == "captions" {
            d.snapshot("compact-captions");
        }
        assert_eq!(timeline(&mut d), baseline, "switching to {id} moved the timeline");
    }
    // 再点当前 Tab 收起。
    d.ok("ui.click", json!({"id": "jy.tab.audio"}));
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    assert!(info["ui"]["jy_drawer"].is_null(), "clicking the active tab should close the drawer: {info}");
    assert_eq!(timeline(&mut d), baseline, "closing the drawer moved the timeline");
}

/// Medium（768–1024）：抽屉从右侧展开成 240 px 侧栏（不是底部），时间轴 ≥240 px 且不被遮挡。
#[test]
fn medium_layout_uses_a_right_sidebar() {
    let mut d = Driver::new(900.0, 1024.0);
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["layoutClass"], json!("medium"), "900 px should be the medium class");
    let rect_of = |value: &Value, id: &str| -> [f64; 4] {
        let found = value.as_array().expect("elements").iter().find(|e| e["id"] == json!(id)).unwrap_or_else(|| panic!("{id} missing in {value}"));
        let r = found["rect"].as_array().expect("rect");
        [r[0].as_f64().unwrap(), r[1].as_f64().unwrap(), r[2].as_f64().unwrap(), r[3].as_f64().unwrap()]
    };
    let before = d.ok("ui.elements", json!({"prefix": "jy."}));
    let timeline = rect_of(&before, "jy.timeline");
    assert!(timeline[3] >= 240.0, "medium timeline too short: {timeline:?}");

    d.ok("ui.click", json!({"id": "jy.tab.captions"}));
    d.frames(3);
    let opened = d.ok("ui.elements", json!({"prefix": "jy."}));
    let preview = rect_of(&opened, "jy.preview");
    let drawer = rect_of(&opened, "jy.drawer");
    assert!((drawer[2] - 240.0).abs() < 1.5, "sidebar width should be 240 px: {drawer:?}");
    assert!((preview[0] + preview[2] - drawer[0]).abs() < 1.5, "sidebar is not on the preview's right edge: {preview:?} {drawer:?}");
    assert!((drawer[1] - preview[1]).abs() < 1.5, "sidebar should start at the content top: {drawer:?}");
    assert!(drawer[1] + drawer[3] <= timeline[1] + 0.5, "sidebar overlaps the timeline");
    assert_eq!(rect_of(&opened, "jy.timeline"), timeline, "opening the sidebar moved the timeline");
    d.snapshot("medium-sidebar");
}

/// Medium（平板）也应有时间轴工具条，并且分割按钮真的生效。
#[test]
fn medium_has_the_timeline_toolbar() {
    let mut d = Driver::new(900.0, 1024.0);
    d.frames(3);
    let tools = d.ok("ui.elements", json!({"prefix": "jy.tool."}));
    let count = tools.as_array().map_or(0, Vec::len);
    assert!(count >= 7, "medium should show the timeline toolbar, got {count}: {tools}");
    fn video_clips(d: &mut Driver) -> usize {
        let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
        sequence["video"].as_array().map(|tracks| tracks.iter().map(|track| track["items"].as_array().map_or(0, Vec::len)).sum()).unwrap_or(0)
    }
    let before = video_clips(&mut d);
    d.ok("engine.execute", json!({"command": "playhead.set", "params": {"seconds": 2.0}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "jy.tool.split"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), before + 1, "the medium toolbar's split should cut at the playhead");
    d.ok("ui.click", json!({"id": "jy.tool.undo"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), before);
    d.snapshot("medium-toolbar");
}

/// Compact/Medium 只保留一条顶栏：全局菜单条隐藏，菜单收进 ☰；Wide 仍保留全局菜单条。
#[test]
fn shell_menu_replaces_the_global_menu_bar() {
    let mut medium = Driver::new(900.0, 1024.0);
    medium.frames(3);
    let header = medium.ok("ui.elements", json!({"prefix": "header."}));
    assert!(header.as_array().is_none_or(|a| a.is_empty()), "global menu bar must be hidden at 900 px: {header}");

    medium.ok("ui.click", json!({"id": "jy.menu"}));
    medium.frames(3);
    let open = medium.ok("ui.elements", json!({"prefix": "jy.menu"}));
    let open = open.as_array().expect("elements");
    assert!(open.iter().any(|e| e["id"] == json!("jy.menu.panel")), "☰ did not open the menu: {open:?}");
    assert!(open.len() > 2, "☰ menu should list commands, got {}", open.len());
    medium.snapshot("shell-menu");

    medium.ok("ui.click", json!({"id": "jy.menu"}));
    medium.frames(3);
    let closed = medium.ok("ui.elements", json!({"prefix": "jy.menu.panel"}));
    assert!(closed.as_array().is_none_or(|a| a.is_empty()), "☰ did not close the menu: {closed}");

    let mut wide = Driver::new(1280.0, 1024.0);
    wide.frames(3);
    let header = wide.ok("ui.elements", json!({"prefix": "header."}));
    assert!(header.as_array().is_some_and(|a| !a.is_empty()), "wide keeps the global menu bar: {header}");
}

/// Wide 的 `Jianying` 工作区是剪映四区：左素材、中预览、右属性、底时间轴；
/// 上游 Source / Tools 不在这套布局里（默认 `Editing` 仍保留上游布局，供老用户切换）。
#[test]
fn wide_default_workspace_is_the_four_regions() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let elements = d.ok("ui.elements", json!({"prefix": "panel."}));
    let ids: Vec<String> = elements.as_array().expect("elements").iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect();
    for want in ["panel.Project", "panel.Program", "panel.Properties", "panel.Timeline"] {
        assert!(ids.iter().any(|id| id == want), "default wide layout is missing {want}: {ids:?}");
    }
    for gone in ["panel.Source", "panel.Tools"] {
        assert!(!ids.iter().any(|id| id == gone), "{gone} should not be on the default first screen: {ids:?}");
    }
    let project = elements.as_array().expect("elements").iter().find(|e| e["id"] == json!("panel.Project")).expect("project");
    let program = elements.as_array().expect("elements").iter().find(|e| e["id"] == json!("panel.Program")).expect("program");
    let left = project["rect"][0].as_f64().unwrap_or(0.0);
    let middle = program["rect"][0].as_f64().unwrap_or(0.0);
    assert!(left < middle, "Project must sit left of Program: project x={left}, program x={middle}");
    d.snapshot("wide-four-regions");
}

/// ☰ 菜单的过滤框：输入后只剩匹配项，且每一条都真的匹配过滤词。
#[test]
fn shell_menu_filters_commands() {
    let mut d = Driver::new(900.0, 1024.0);
    d.frames(3);
    d.ok("ui.click", json!({"id": "jy.menu"}));
    d.frames(3);
    let row_count = |value: &Value| -> usize {
        value
            .as_array()
            .expect("elements")
            .iter()
            .filter(|e| {
                let id = e["id"].as_str().unwrap_or("");
                id.starts_with("jy.menu.") && id != "jy.menu.panel" && id != "jy.menu.filter" && id != "jy.menu.back" && !id.starts_with("jy.menu.group.")
            })
            .count()
    };
    // 打开时是菜单根列表（两级菜单），命令行还没出现。
    let roots = d.ok("ui.elements", json!({"prefix": "jy.menu.group."}));
    let root_count = roots.as_array().map_or(0, Vec::len);
    assert!(root_count > 3, "the menu should group commands by menu root, got {root_count}");
    assert_eq!(row_count(&d.ok("ui.elements", json!({"prefix": "jy.menu."}))), 0, "no command rows before choosing a group");

    d.ok("ui.click", json!({"id": "jy.menu.filter"}));
    d.frames(1);
    d.ok("ui.type", json!({"text": "export"}));
    d.frames(3);
    let filtered = d.ok("ui.elements", json!({"prefix": "jy.menu."}));
    let filtered_rows = row_count(&filtered);
    assert!(filtered_rows > 0 && filtered_rows < 40, "filter did not narrow: {filtered_rows} rows");
    for element in filtered.as_array().expect("elements") {
        let id = element["id"].as_str().unwrap_or("");
        if !id.starts_with("jy.menu.") || id == "jy.menu.panel" || id == "jy.menu.filter" {
            continue;
        }
        assert!(id.to_lowercase().contains("export"), "row {id} does not match the filter");
    }
    d.snapshot("shell-menu-filtered");
}

/// 空工程给剪映式提示（不是一张空网格）；有素材后提示消失。
#[test]
fn empty_bin_shows_its_own_hint() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    d.ok("engine.execute", json!({"command": "file.newBin", "params": {"name": "Empty One"}}));
    d.frames(3);
    let project = d.ok("engine.execute", json!({"command": "project.inspect", "params": {}}));
    fn find_bin(node: &Value, name: &str) -> Option<u64> {
        for child in node.get("children")?.as_array()? {
            if child.get("children").is_some() {
                if child.get("name").and_then(Value::as_str) == Some(name) {
                    return child.get("id").and_then(Value::as_u64);
                }
                if let Some(found) = find_bin(child, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    let id = find_bin(&project["root"], "Empty One").expect("the new bin should be in the tree");
    d.ok("ui.project.bin", json!({"bin": id}));
    d.frames(4);
    let hint = d.ok("ui.elements", json!({"prefix": "project.emptyBinHint"}));
    assert!(hint.as_array().is_some_and(|list| !list.is_empty()), "an empty folder should say so: {hint}");
    d.snapshot("empty-bin");
}

/// 空工程给剪映式提示（不是一张空网格）；有素材后提示消失。
#[test]
fn empty_project_shows_the_drop_hint() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    let demo = d.ok("ui.elements", json!({"prefix": "project.emptyHint"}));
    assert!(demo.as_array().is_none_or(|list| list.is_empty()), "the demo project has media, so no hint: {demo}");

    d.ok("engine.execute", json!({"command": "file.closeAllProjects", "params": {"force": true}}));
    d.frames(3);
    d.ok("engine.execute", json!({"command": "file.newProject", "params": {"name": "Blank"}}));
    d.frames(4);
    let empty = d.ok("ui.elements", json!({"prefix": "project.emptyHint"}));
    assert!(empty.as_array().is_some_and(|list| !list.is_empty()), "a blank project should show the drop hint: {empty}");
    d.snapshot("empty-project");

    d.ok("engine.execute", json!({"command": "file.openDemoProject", "params": {}}));
    d.frames(4);
    let back = d.ok("ui.elements", json!({"prefix": "project.emptyHint"}));
    assert!(back.as_array().is_none_or(|list| list.is_empty()), "the hint must disappear once media exists: {back}");
}

/// 一级 Tab 用左右键循环（跳过未实现的 4 个），面板跟着换；循环范围包含「更多」里的溢出 Tab。
#[test]
fn shell_tabs_cycle_with_left_right() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    // 没有当前 Tab 时，向右落到第一个可用的「素材」。
    let first = d.ok("ui.shell.tab.step", json!({"step": 1}));
    assert_eq!(first["tab"], json!("cut"), "the first implemented tab is media");
    let second = d.ok("ui.shell.tab.step", json!({"step": 1}));
    assert_eq!(second["tab"], json!("captions"), "right goes to the text tab");
    d.frames(3);
    let text = d.ok("ui.elements", json!({"prefix": "panel.Text"}));
    assert!(text.as_array().is_some_and(|list| !list.is_empty()), "the text panel should be in the drawer: {text}");
    let back = d.ok("ui.shell.tab.step", json!({"step": -1}));
    assert_eq!(back["tab"], json!("cut"), "left goes back to media");

    // 三个主 Tab 之后是「更多」里的溢出 Tab：键盘也必须够得着（贴纸是溢出第一项）。
    d.ok("ui.shell.tab", json!({"tab": "audio"}));
    let overflow = d.ok("ui.shell.tab.step", json!({"step": 1}));
    assert_eq!(overflow["tab"], json!("sticker"), "cycling must continue into the 更多 overflow");

    // 从最后一个可用 Tab 继续向右要跳过 4 个未实现的并绕回开头。
    d.ok("ui.shell.tab", json!({"tab": "adjust"}));
    d.frames(2);
    let wrapped = d.ok("ui.shell.tab.step", json!({"step": 1}));
    assert_eq!(wrapped["tab"], json!("cut"), "cycling skips smart/templates/avatar/plugins and wraps");
}

/// `ui.shell.tab` 能打开不在顶栏直排的 Tab：切到「滤镜」时「更多」菜单会展开，该入口出现在
/// 屏幕上（390 px 的顶栏放不下 13 个），面板也真的换过去。
#[test]
fn shell_tab_command_scrolls_a_clipped_tab_into_view() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    // 溢出入口默认不在屏幕上——先确认"看不见"是真的，否则下面的断言没有意义。
    let before = tab_ids(&mut d);
    assert!(!before.iter().any(|id| id == "jy.tab.filters"), "the filters tab should not be inline: {before:?}");
    // 「更多」自己必须整颗落在窗口里（390 px 下才点得到）。
    let more = rect_of(&mut d, "jy.tab.more");
    assert!(more[0] >= 0.0 && more[0] + more[2] <= 390.0 + 0.5, "更多 must sit inside the window: {more:?}");

    d.ok("ui.shell.tab", json!({"tab": "filters"}));
    d.frames(4);
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["ui"]["jy_tab_more"], json!(true), "an overflow tab must reveal the 更多 menu: {info}");
    let after = tab_ids(&mut d);
    assert!(after.iter().any(|id| id == "jy.tab.filters"), "the revealed menu is missing the tab: {after:?}");
    let rect = rect_of(&mut d, "jy.tab.filters");
    assert!(
        rect[0] >= 0.0 && rect[0] + rect[2] <= 390.0 + 0.5 && rect[1] >= 0.0 && rect[1] + rect[3] <= 884.0 + 0.5,
        "the revealed entry must be on screen: {rect:?}"
    );
    let lumetri = d.ok("ui.elements", json!({"prefix": "panel.LumetriColor"}));
    assert!(lumetri.as_array().is_some_and(|list| !list.is_empty()), "the tab's panel should be drawn in the drawer: {lumetri}");
}

/// 移动端顶栏收敛成 3 个主 Tab + 「更多」（Compact 与 Medium 都一样）：
/// 溢出里的 Tab 仍然能从「更多」打开，`ui.inspect` 与面板都跟着变。
#[test]
fn top_bar_shows_three_primary_tabs_plus_more() {
    for (width, height) in [(390.0, 884.0), (900.0, 1024.0)] {
        let mut d = Driver::new(width, height);
        d.frames(3);
        let inline = tab_ids(&mut d);
        assert_eq!(
            primary_tab_ids(&inline),
            ["jy.tab.cut", "jy.tab.captions", "jy.tab.audio"],
            "{width}x{height}: the top bar must leave exactly the 3 primary tabs inline: {inline:?}"
        );
        assert!(inline.iter().any(|id| id == "jy.tab.more"), "{width}x{height}: 更多 is missing: {inline:?}");
        for hidden in OVERFLOW_TAB_IDS {
            assert!(!inline.iter().any(|id| id == hidden), "{width}x{height}: {hidden} must live in 更多, not inline: {inline:?}");
        }

        // 溢出里的 Tab 从「更多」打开：面板真的换过去，状态也记在 ui.inspect 里。
        d.ok("ui.click", json!({"id": "jy.tab.more"}));
        d.frames(3);
        assert_eq!(d.ok("ui.inspect", json!({}))["ui"]["jy_tab_more"], json!(true), "{width}x{height}: 更多 did not open");
        d.ok("ui.click", json!({"id": "jy.tab.effects"}));
        d.frames(4);
        let after = d.ok("ui.inspect", json!({}));
        assert_eq!(after["ui"]["jy_shell_slug"], json!("effects"), "{width}x{height}: the overflow tab did not activate: {after}");
        assert!(!after["ui"]["jy_drawer"].is_null(), "{width}x{height}: the overflow tab opened no drawer: {after}");
        assert!(!after["ui"]["jy_tab_more"].as_bool().unwrap_or(false), "{width}x{height}: the menu should close after a pick: {after}");
        let effects = d.ok("ui.elements", json!({"prefix": "panel.Effects"}));
        assert!(effects.as_array().is_some_and(|list| !list.is_empty()), "{width}x{height}: the effects panel is not drawn: {effects}");
        d.snapshot(&format!("tabs-{}-primary", width as u32));
    }
}

/// 移动端顶栏右上角也有「导出」，点了进 Export 模式。
#[test]
fn compact_top_bar_has_the_export_button() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let button = d.ok("ui.elements", json!({"prefix": "jy.export"}));
    let rect =
        button.as_array().and_then(|list| list.first()).map(|element| element["rect"].clone()).expect("the compact top bar should show the export button");
    let x = rect[0].as_f64().unwrap_or(0.0);
    assert!(x > 390.0 - 80.0, "export should sit at the right edge, got x={x}");
    d.snapshot("compact-export");
    d.ok("ui.click", json!({"id": "jy.export"}));
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    assert_eq!(info["ui"]["mode"], json!("Export"), "the export button should switch to Export mode");
}

/// 移动端顶栏是剪映式一级 Tab：只直排 3 个主 Tab + 「更多」，其余 10 个都在「更多」菜单里，
/// 点"特效"真的换抽屉面板（openspec #233 / `02-ia-mapping.md` §7.7）。
#[test]
fn compact_top_bar_scrolls_through_the_jianying_tabs() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let inline = tab_ids(&mut d);
    assert_eq!(primary_tab_ids(&inline), ["jy.tab.cut", "jy.tab.captions", "jy.tab.audio"], "the compact top bar is not the 3 primary tabs: {inline:?}");
    assert!(inline.iter().any(|id| id == "jy.tab.more"), "the compact top bar is missing 更多: {inline:?}");
    for hidden in OVERFLOW_TAB_IDS {
        assert!(!inline.iter().any(|id| id == hidden), "{hidden} must live in 更多, not inline: {inline:?}");
    }
    // 溢出入口一条不少地列在「更多」里（没有实现的模块也在，只是灰显）。
    d.ok("ui.click", json!({"id": "jy.tab.more"}));
    d.frames(3);
    let opened = tab_ids(&mut d);
    for want in OVERFLOW_TAB_IDS {
        assert!(opened.iter().any(|id| id == want), "the 更多 menu is missing {want}: {opened:?}");
    }
    // 主 Tab 仍然直接可点，并且换的是真面板。
    d.ok("ui.click", json!({"id": "jy.tab.captions"}));
    d.frames(3);
    let text = d.ok("ui.elements", json!({"prefix": "panel.Text"}));
    assert!(text.as_array().is_some_and(|list| !list.is_empty()), "the text tab should draw the Text panel in the drawer: {text}");
    d.snapshot("compact-tabs");
}

/// 移动端「更多」里的"模板 / 数字人 / 插件 / 智能包装"没有实现：照样列出来（不能悄悄消失），
/// 但点了不打开任何面板、也不成为当前 Tab。
#[test]
fn compact_tab_bar_disables_unimplemented_modules() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    let before = d.ok("ui.inspect", json!({}));
    assert!(before["ui"]["jy_drawer"].is_null(), "no drawer at start");
    d.ok("ui.click", json!({"id": "jy.tab.more"}));
    d.frames(3);
    let ids = tab_ids(&mut d);
    for disabled in ["jy.tab.smart", "jy.tab.templates", "jy.tab.avatar", "jy.tab.plugins"] {
        assert!(ids.iter().any(|id| id == disabled), "a disabled module must stay listed, not vanish: {ids:?}");
        d.ok("ui.click", json!({"id": disabled}));
        d.frames(3);
        let after = d.ok("ui.inspect", json!({}));
        assert!(after["ui"]["jy_drawer"].is_null(), "a disabled tab ({disabled}) must not open a drawer: {after}");
        assert!(after["ui"]["jy_shell_slug"].is_null(), "a disabled tab ({disabled}) must not become the current tab: {after}");
    }
}

/// 剪映专业版工作区：左栏竖向 rail 八个入口都在，可用的（素材）点了真换面板。
#[test]
fn jianying_workspace_has_the_left_rail() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let rail = d.ok("ui.elements", json!({"prefix": "jy.rail."}));
    let ids: Vec<String> = rail.as_array().expect("elements").iter().filter_map(|element| element["id"].as_str().map(str::to_string)).collect();
    for want in ["jy.rail.import", "jy.rail.media", "jy.rail.subdraft", "jy.rail.mine", "jy.rail.ai", "jy.rail.cloud", "jy.rail.official", "jy.rail.dreamina"] {
        assert!(ids.iter().any(|id| id == want), "the left rail is missing {want}: {ids:?}");
    }
    d.ok("ui.click", json!({"id": "jy.rail.media"}));
    d.frames(3);
    let project = d.ok("ui.elements", json!({"prefix": "panel.Project"}));
    assert!(project.as_array().is_some_and(|list| !list.is_empty()), "the media rail entry should show the Project panel: {project}");
    d.snapshot("wide-left-rail");
}

/// 剪映专业版工作区：工具条贴在时间轴上方，且"分割 / 定格 / 倒放"都在。
#[test]
fn jianying_workspace_shows_draft_parameters_when_nothing_is_selected() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    d.ok("engine.execute", json!({"command": "edit.deselectAll", "params": {}}));
    d.frames(3);
    let rows = d.ok("ui.elements", json!({"prefix": "draft."}));
    let elements = rows.as_array().expect("elements");
    let find = |id: &str| elements.iter().find(|element| element["id"] == json!(id)).cloned();
    for want in ["draft.name", "draft.size", "draft.duration", "draft.tracks", "draft.items"] {
        assert!(find(want).is_some(), "draft parameters are missing {want}: {rows}");
    }
    let size = find("draft.size").expect("size")["label"].as_str().unwrap_or("").to_string();
    assert!(size.contains('x') && size != "0 x 0", "the size row should carry the real frame size, got {size}");
    let tracks = find("draft.tracks").expect("tracks")["label"].as_str().unwrap_or("").to_string();
    assert!(tracks.starts_with('V') && tracks.contains('A'), "the tracks row should read V<n> / A<n>, got {tracks}");
    d.snapshot("wide-draft-parameters");
}

/// 工具条按钮跟着命令的启用条件走：没选中片段时"删除"点了也不该删东西。
/// 播放中工具条显示"暂停"，停下后回到"播放"。
#[test]
fn play_button_switches_to_pause_while_playing() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    let label_of = |d: &mut Driver| -> String {
        let elements = d.ok("ui.elements", json!({"prefix": "jy.tool.play"}));
        elements.as_array().and_then(|list| list.first()).and_then(|element| element["label"].as_str()).unwrap_or("").to_string()
    };
    assert_eq!(label_of(&mut d), "播放", "idle state shows play");
    d.ok("ui.click", json!({"id": "jy.tool.play"}));
    d.frames(3);
    assert_eq!(label_of(&mut d), "暂停", "while playing the button turns into pause");
    d.snapshot("wide-toolbar-playing");
    d.ok("ui.click", json!({"id": "jy.tool.play"}));
    d.frames(3);
    assert_eq!(label_of(&mut d), "播放", "stopping turns it back into play");
}

#[test]
fn toolbar_buttons_follow_command_enablement() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    fn clips(d: &mut Driver) -> usize {
        let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
        sequence["video"].as_array().map(|tracks| tracks.iter().map(|track| track["items"].as_array().map_or(0, Vec::len)).sum()).unwrap_or(0)
    }
    let before = clips(&mut d);
    d.ok("engine.execute", json!({"command": "edit.deselectAll", "params": {}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "jy.tool.delete"}));
    d.frames(3);
    assert_eq!(clips(&mut d), before, "delete is disabled without a selection, so nothing may be removed");
    d.snapshot("wide-toolbar-disabled");

    // 选中第一段视频后再点删除，这次要真的删掉。
    let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    let clip = sequence["video"][0]["items"][0]["clip"].as_u64().expect("first clip");
    d.ok("engine.execute", json!({"command": "timeline.select", "params": {"clips": [clip]}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "jy.tool.delete"}));
    d.frames(3);
    assert_eq!(clips(&mut d), before - 1, "with a selection, delete should remove the clip");
    d.ok("ui.click", json!({"id": "jy.tool.undo"}));
    d.frames(3);
    assert_eq!(clips(&mut d), before, "undo should restore it");
}

#[test]
fn jianying_workspace_puts_the_toolbar_on_the_timeline() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let tools = d.ok("ui.elements", json!({"prefix": "jy.tool."}));
    let ids: Vec<String> = tools.as_array().expect("elements").iter().filter_map(|element| element["id"].as_str().map(str::to_string)).collect();
    for want in ["jy.tool.split", "jy.tool.delete", "jy.tool.freeze", "jy.tool.reverse"] {
        assert!(ids.iter().any(|id| id == want), "the timeline toolbar is missing {want}: {ids:?}");
    }
    fn video_clips(d: &mut Driver) -> usize {
        let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
        sequence["video"].as_array().map(|tracks| tracks.iter().map(|track| track["items"].as_array().map_or(0, Vec::len)).sum()).unwrap_or(0)
    }
    let before = video_clips(&mut d);
    d.ok("engine.execute", json!({"command": "playhead.set", "params": {"seconds": 2.0}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "jy.tool.split"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), before + 1, "the wide toolbar's split should cut at the playhead");
    d.ok("ui.click", json!({"id": "jy.tool.undo"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), before, "undo should restore the clip count");
    d.snapshot("wide-toolbar");
}

/// 剪映专业版工作区：顶部 13 个一级 Tab；有面板的能导航，没实现的不可点。
/// 剪映的"音频"Tab 只列音频素材；切回"素材"恢复全部。
#[test]
fn audio_tab_filters_the_project_to_audio_media() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let items = |d: &mut Driver| -> usize {
        let elements = d.ok("ui.elements", json!({"prefix": "project.item."}));
        elements.as_array().map(|list| list.iter().filter(|element| element["id"].as_str().is_some_and(|id| id.matches('.').count() == 2)).count()).unwrap_or(0)
    };
    let project = d.ok("engine.execute", json!({"command": "project.inspect", "params": {}}));
    fn find_bin(node: &Value, name: &str) -> Option<u64> {
        for child in node.get("children")?.as_array()? {
            if child.get("children").is_some() {
                if child.get("name").and_then(Value::as_str) == Some(name) {
                    return child.get("id").and_then(Value::as_u64);
                }
                if let Some(found) = find_bin(child, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    let footage = find_bin(&project["root"], "Footage").expect("demo Footage bin");
    d.ok("ui.project.bin", json!({"bin": footage}));
    d.frames(3);
    let all = items(&mut d);
    assert!(all >= 5, "the Footage bin should list its clips, got {all}");

    d.ok("ui.click", json!({"id": "jy.tabstrip.audio"}));
    d.frames(3);
    assert_eq!(items(&mut d), 0, "the audio tab must hide video-only clips");

    d.ok("ui.click", json!({"id": "jy.tabstrip.media"}));
    d.frames(3);
    assert_eq!(items(&mut d), all, "the media tab should list everything again");
}

/// 剪映专业版右上角的「导出」按钮：点了进 Export 模式。
#[test]
fn jianying_workspace_has_a_prominent_export_button() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(3);
    let button = d.ok("ui.elements", json!({"prefix": "jy.export"}));
    assert!(button.as_array().is_some_and(|list| !list.is_empty()), "the export button is missing: {button}");
    d.ok("ui.click", json!({"id": "jy.export"}));
    d.frames(3);
    let info = d.ok("ui.inspect", json!({}));
    let mode = info["ui"]["mode"].clone();
    assert!(mode == json!("Export"), "the export button should switch to Export mode, got {mode}");
    d.snapshot("wide-export-button");
}

#[test]
fn jianying_workspace_shows_the_tab_strip() {
    let mut d = Driver::new(1280.0, 1024.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.frames(4);
    let strip = d.ok("ui.elements", json!({"prefix": "jy.tabstrip."}));
    let count = strip.as_array().map_or(0, Vec::len);
    assert!(count >= 12, "expected the JianYing tab strip, got {count}: {strip}");
    // 有对应面板的 Tab 会真的把面板调出来。
    d.ok("ui.click", json!({"id": "jy.tabstrip.effects"}));
    d.frames(3);
    let effects = d.ok("ui.elements", json!({"prefix": "panel.Effects"}));
    assert!(effects.as_array().is_some_and(|list| !list.is_empty()), "the effects tab should show the Effects panel: {effects}");
    d.snapshot("wide-tabstrip");
}

/// 两级 ☰ 菜单：先列菜单根，点进去只显示该根的命令，返回能回到根列表。
#[test]
fn shell_menu_drills_into_a_menu_root() {
    let mut d = Driver::new(900.0, 1024.0);
    d.frames(3);
    d.ok("ui.click", json!({"id": "jy.menu"}));
    d.frames(3);
    let roots = d.ok("ui.elements", json!({"prefix": "jy.menu.group."}));
    assert!(roots.as_array().is_some_and(|list| list.len() > 3), "expected menu roots: {roots}");
    d.snapshot("shell-menu-roots");

    d.ok("ui.click", json!({"id": "jy.menu.group.File"}));
    d.frames(3);
    let children = d.ok("ui.elements", json!({"prefix": "jy.menu.file."}));
    assert!(children.as_array().is_some_and(|list| !list.is_empty()), "the File root should list its commands: {children}");
    let back = d.ok("ui.elements", json!({"prefix": "jy.menu.back"}));
    assert!(back.as_array().is_some_and(|list| !list.is_empty()), "a group view needs a way back");
    let groups_gone = d.ok("ui.elements", json!({"prefix": "jy.menu.group."}));
    assert!(groups_gone.as_array().is_none_or(|list| list.is_empty()), "the root list must be replaced by the group view: {groups_gone}");

    d.ok("ui.click", json!({"id": "jy.menu.back"}));
    d.frames(3);
    let roots_again = d.ok("ui.elements", json!({"prefix": "jy.menu.group."}));
    assert!(roots_again.as_array().is_some_and(|list| !list.is_empty()), "back should return to the roots");
}

/// Compact 底部工具条：分割真的在播放头切一刀，撤销能还原——不是只画了五个按钮。
#[test]
fn compact_toolbar_splits_at_the_playhead_and_undoes() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    fn video_clips(d: &mut Driver) -> usize {
        let sequence = d.ok("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
        sequence["video"].as_array().map(|tracks| tracks.iter().map(|track| track["items"].as_array().map_or(0, Vec::len)).sum()).unwrap_or(0)
    }
    let before = video_clips(&mut d);
    assert!(before > 0, "the demo project should start with clips: {before}");
    d.ok("engine.execute", json!({"command": "playhead.set", "params": {"seconds": 2.0}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "jy.tool.split"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), before + 1, "split at the playhead should add one video clip");
    d.ok("ui.click", json!({"id": "jy.tool.undo"}));
    d.frames(3);
    assert_eq!(video_clips(&mut d), before, "undo should restore the clip count");
    d.snapshot("compact-toolbar");
}

/// Compact 抽屉里的面板必须能滚动：滚轮落在抽屉上时，内容位置要变。
#[test]
fn compact_drawer_scrolls_its_panel() {
    let mut d = Driver::new(390.0, 884.0);
    d.frames(3);
    d.ok("ui.click", json!({"id": "jy.tab.cut"}));
    d.frames(4);
    let positions = |value: Value| -> Vec<f64> { value.as_array().expect("elements").iter().filter_map(|element| element["rect"][1].as_f64()).collect() };
    let before = positions(d.ok("ui.elements", json!({"prefix": "project.bin."})));
    assert!(!before.is_empty(), "the media drawer should list bins");
    let panel = d.ok("ui.elements", json!({"prefix": "panel.Project"}));
    let rect = panel.as_array().and_then(|list| list.first()).expect("project panel")["rect"].clone();
    let (x, y, w, h) = (rect[0].as_f64().unwrap_or(0.0), rect[1].as_f64().unwrap_or(0.0), rect[2].as_f64().unwrap_or(0.0), rect[3].as_f64().unwrap_or(0.0));
    d.ok("ui.scroll", json!({"x": x + w / 2.0, "y": y + h / 2.0, "dy": -260.0}));
    d.frames(4);
    let after = positions(d.ok("ui.elements", json!({"prefix": "project.bin."})));
    assert_ne!(before, after, "the wheel inside the drawer must scroll the panel, not the page");
}
