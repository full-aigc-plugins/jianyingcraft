//! 剪映式左侧素材宫格：搜索、类型筛选（全部 / 视频 / 音频 / 图像）、把文件拖入导入。

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_project::BinEntry;
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
        d.ok("ui.set", json!({"workspace": "Jianying"}));
        d.ok("ui.panel.show", json!({"panel": "Project"}));
        d.frames(3);
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

    fn click(&mut self, id: &str) {
        self.ok("ui.click", json!({"id": id}));
        self.frames(2);
    }

    fn ids(&mut self, prefix: &str) -> Vec<String> {
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().expect("elements").iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }

    fn app(&mut self) -> &mut FilmcraftApp {
        self.harness.state_mut()
    }

    /// The demo's Footage bin and its items (id, name).
    fn footage(&mut self) -> (u64, Vec<(u64, String)>) {
        let p = &self.app().session.project;
        let bin = p
            .root
            .children
            .iter()
            .find_map(|e| match e {
                BinEntry::Bin(b) if b.name == "Footage" => Some(b),
                _ => None,
            })
            .expect("Footage bin");
        let items = bin
            .children
            .iter()
            .filter_map(|e| match e {
                BinEntry::Item(id) => p.item(*id).map(|it| (id.0, it.name.clone())),
                _ => None,
            })
            .collect();
        (bin.id.0, items)
    }

    /// 向 egui 的 raw input 注入一个 OS 拖入文件，跑一帧让 `handle_drops` 导入它。
    fn drop_file(&mut self, path: std::path::PathBuf) {
        self.harness.input_mut().dropped_files.push(std::sync::Arc::new(DroppedFile(path)));
        self.harness.step();
        self.harness.input_mut().dropped_files.clear();
        self.frames(3);
    }
}

/// `egui::DroppedFile` 在 0.36 是窗口后端实现的句柄；测试里给一个真实路径即可。
#[derive(Debug)]
struct DroppedFile(std::path::PathBuf);

impl egui::DroppedFile for DroppedFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn visible_items(d: &mut Driver) -> Vec<String> {
    d.ids("project.item.").into_iter().filter(|i| !i.contains(".label") && !i.contains(".inOut") && !i.contains(".offline")).collect()
}

#[test]
fn search_filters_the_media_grid() {
    let mut d = Driver::demo();
    let (bin, items) = d.footage();
    assert!(items.len() >= 2, "demo Footage bin: {items:?}");
    d.ok("ui.project.bin", json!({"bin": bin}));
    d.frames(3);
    assert_eq!(visible_items(&mut d).len(), items.len(), "every Footage item is shown before searching");

    // 用第一个素材名前 4 个字符搜索：列表变短，且留下的名字都包含这个串。
    let query: String = items[0].1.chars().take(4).collect();
    d.click("project.search");
    d.ok("ui.type", json!({"text": query}));
    d.frames(3);
    assert_eq!(d.app().ui.project_search, query, "the search box takes the typed text");
    let filtered = visible_items(&mut d);
    assert!(!filtered.is_empty(), "the first item still matches {query}: {filtered:?}");
    assert!(filtered.len() < items.len(), "search narrows {items:?} to {filtered:?}");
    for id in &filtered {
        let n: u64 = id.trim_start_matches("project.item.").parse().expect("item id");
        let name = d.app().session.project.item(filmcraft_project::ItemId(n)).map(|i| i.name.clone()).expect("item");
        assert!(name.to_lowercase().contains(&query.to_lowercase()), "{name} does not match {query}");
    }

    // 清空搜索：全部素材回来。
    d.click("project.search");
    d.ok("ui.key", json!({"key": "Cmd+A"}));
    d.ok("ui.key", json!({"key": "Backspace"}));
    d.frames(3);
    assert_eq!(d.app().ui.project_search, "", "the search box is cleared");
    assert_eq!(visible_items(&mut d).len(), items.len(), "clearing the search restores the grid");
}

#[test]
fn type_chips_filter_the_media_grid() {
    let mut d = Driver::demo();
    let (bin, items) = d.footage();
    d.ok("ui.project.bin", json!({"bin": bin}));
    d.frames(3);
    assert_eq!(visible_items(&mut d).len(), items.len());

    d.click("project.filter.audio");
    assert_eq!(d.app().ui.jy_media_filter.as_deref(), Some("audio"));
    assert!(visible_items(&mut d).is_empty(), "the demo Footage bin has no audio-only items");

    d.click("project.filter.video");
    assert_eq!(d.app().ui.jy_media_filter.as_deref(), Some("video"));
    assert_eq!(visible_items(&mut d).len(), items.len(), "every Footage item is video");

    d.click("project.filter.all");
    assert_eq!(d.app().ui.jy_media_filter, None);
    assert_eq!(visible_items(&mut d).len(), items.len());
}

#[test]
fn a_file_dropped_on_the_window_is_imported() {
    let mut d = Driver::demo();
    let dir = std::env::temp_dir().join(format!("fc-proj-grid-drop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("dropped-take.wav");
    let wav = filmcraft_media::wav::write_wav16(&[0.0; 4800], 2, 48_000);
    std::fs::write(&path, &wav).expect("wav fixture");

    let before = d.app().session.project.items.len();
    d.drop_file(path.clone());
    let after = d.app().session.project.items.len();
    assert_eq!(after, before + 1, "the dropped file becomes one project item");
    assert!(
        d.app().session.project.items.values().any(|i| i.name == "dropped-take.wav"),
        "the item is named after the file: {:?}",
        d.app().session.project.items.values().map(|i| i.name.clone()).collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
