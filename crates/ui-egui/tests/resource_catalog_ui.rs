//! 剪映资源 Tab 的浏览态（`panels::resource_catalog`）：
//!
//! - 运行时从 `JIANYINGCRAFT_CATALOGS` 读 `transitions.json`（不 `include_str!`）；
//! - 顶部搜索按名称/编号过滤，chips 按 `vip` 过滤，右侧「N 条」跟着变；
//! - 6 列网格的卡片数量与目录条目一致；
//! - 点卡片只把「名称 + resource_id」写进状态栏，不执行副作用；
//! - 目录不存在时出现可操作的空态（提示设置 `JIANYINGCRAFT_CATALOGS`），不崩溃。
//!
//! 环境变量是**进程级**的，而 Rust 2024 里 `std::env::set_var` 是 `unsafe`（workspace 的
//! `unsafe_code = "forbid"` 不允许）：所以入口测试用 `Command::env` 在子进程里真正设置
//! `JIANYINGCRAFT_CATALOGS`，场景本身在子进程里跑——和产品运行时的读取路径完全一致。

use std::path::Path;
use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

/// 子进程标记：存在时当前进程直接跑场景，否则当前进程只负责起子进程。
const CHILD_MARKER: &str = "JY_RESOURCE_CATALOG_TEST_CHILD";
const TEST_NAME: &str = "transition_catalog_browses_searches_filters_and_reports_selection";

const VIP_ID: &str = "7397337005387944457";
const FREE_ID: &str = "7049979667406656014";
const FREE_ID_2: &str = "6724846395116753416";

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

    fn click(&mut self, id: &str) {
        self.ok("ui.click", json!({"id": id}));
        self.frames(2);
    }

    fn inspect(&mut self) -> Value {
        self.ok("ui.inspect", json!({}))
    }

    fn ids(&mut self, prefix: &str) -> Vec<String> {
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().expect("elements").iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }

    fn label(&mut self, id: &str) -> String {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        v.as_array().and_then(|a| a.iter().find(|e| e["id"] == json!(id))).and_then(|e| e["label"].as_str()).unwrap_or_default().to_string()
    }

    fn cards(&mut self) -> Vec<String> {
        self.ids("resource.card.").into_iter().filter(|id| id.starts_with("resource.card.")).collect()
    }
}

fn write_catalog(dir: &Path) {
    std::fs::create_dir_all(dir).expect("temp catalogs dir");
    let fixture = json!([
        {"name": "3D空间", "resource_id": FREE_ID, "vip": false, "effect_id": "1506926"},
        {"name": "叠化", "resource_id": FREE_ID_2, "vip": false, "effect_id": "2917279"},
        {"name": "会员光效", "resource_id": VIP_ID, "vip": true, "effect_id": "77055399"}
    ]);
    std::fs::write(dir.join("transitions.json"), serde_json::to_vec_pretty(&fixture).expect("fixture json")).expect("write transitions.json");
}

/// 入口：建临时目录与 3 条转场（2 免费 + 1 会员），再在真正的 `JIANYINGCRAFT_CATALOGS` 下跑场景。
#[test]
fn transition_catalog_browses_searches_filters_and_reports_selection() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        catalog_scenario();
        return;
    }
    let dir = std::env::temp_dir().join(format!("jy-resource-catalog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_catalog(&dir);
    let exe = std::env::current_exe().expect("the test binary path");
    let status = std::process::Command::new(exe)
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env("JIANYINGCRAFT_CATALOGS", &dir)
        .env(CHILD_MARKER, "1")
        .status()
        .expect("spawn the catalog scenario");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.success(), "the resource catalog scenario failed in the child process");
}

/// 子进程里的场景：Compact 顶栏切到「转场」，抽屉里的 Effects 面板应变成资源网格。
fn catalog_scenario() {
    let dir = std::env::var_os("JIANYINGCRAFT_CATALOGS").expect("JIANYINGCRAFT_CATALOGS is set by the parent");
    assert!(Path::new(&dir).join("transitions.json").is_file(), "fixture catalog is readable");

    // 390×884 Compact：顶栏 Tab 决定抽屉画哪个面板（Effects）。
    let mut d = Driver::new(390.0, 884.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.shell.tab", json!({"tab": "transitions"}));
    d.frames(4);
    assert_eq!(d.inspect()["ui"]["jy_shell_slug"], json!("transitions"), "the transitions tab is active");

    // 初始：3 条、6 列网格、chips 与搜索框都在。
    assert_eq!(d.cards(), vec![format!("resource.card.{FREE_ID}"), format!("resource.card.{FREE_ID_2}"), format!("resource.card.{VIP_ID}")]);
    assert_eq!(d.label("resource.count"), "3 条");
    for id in ["resource.search", "resource.grid", "resource.chip.all", "resource.chip.free", "resource.chip.vip"] {
        assert!(!d.ids(id).is_empty(), "{id} is not drawn in the resource tab");
    }

    // 搜索：按名称过滤（逗号/编号也行，这里用名称前缀）。
    d.click("resource.search");
    d.ok("ui.type", json!({"text": "3D"}));
    d.frames(3);
    assert_eq!(d.cards(), vec![format!("resource.card.{FREE_ID}")], "search narrows the grid to 3D空间");
    assert_eq!(d.label("resource.count"), "1 条");

    // 清空搜索：全部回来。
    d.click("resource.search");
    d.ok("ui.key", json!({"key": "Cmd+A"}));
    d.ok("ui.key", json!({"key": "Backspace"}));
    d.frames(3);
    assert_eq!(d.cards().len(), 3, "clearing the search restores every card");

    // chips：免费 2 条，会员 1 条，全部 3 条。
    d.click("resource.chip.free");
    assert_eq!(d.cards().len(), 2, "the free chip drops the vip entry");
    assert_eq!(d.label("resource.count"), "2 条");
    d.click("resource.chip.vip");
    assert_eq!(d.cards(), vec![format!("resource.card.{VIP_ID}")], "the vip chip keeps only the vip entry");
    assert_eq!(d.label("resource.count"), "1 条");
    d.click("resource.chip.all");
    assert_eq!(d.cards().len(), 3);

    // 点卡片：只写状态栏（名称 + resource_id），没有别的副作用。
    d.click(&format!("resource.card.{FREE_ID}"));
    let after = d.inspect();
    assert_eq!(after["ui"]["resource_selection"], json!(FREE_ID), "the clicked card is selected: {after}");
    let status = after["ui"]["status"].as_str().unwrap_or_default();
    assert!(status.contains(FREE_ID), "the status line carries the resource id: {status}");
    assert!(status.contains("3D空间"), "the status line carries the resource name: {status}");
    assert!(status.contains("转场"), "the status line names the resource kind: {status}");

    // 目录消失：可操作的空态，不崩溃，也不继续画旧卡片。
    std::fs::remove_dir_all(Path::new(&dir)).expect("remove the fixture catalog");
    d.frames(3);
    assert!(d.cards().is_empty(), "no cards are drawn once the catalog directory is gone");
    let empty = d.ids("resource.empty");
    assert!(!empty.is_empty(), "a missing catalog directory shows the empty state");
    let message = d.label("resource.empty");
    assert!(message.contains("JIANYINGCRAFT_CATALOGS"), "the empty state tells the user how to fix it: {message}");
    // 应用还活着（下一次帧继续画，不 panic）。
    d.frames(2);
    assert_eq!(d.inspect()["ui"]["jy_shell_slug"], json!("transitions"));
}
