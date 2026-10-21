//! 剪映资源 Tab 的浏览态（`panels::resource_catalog`）：
//!
//! - 运行时从 `JIANYINGCRAFT_CATALOGS` 读 `transitions.json` / `video_scene_effects.json`（不 `include_str!`）；
//! - 左侧分类列按关键词规则显示「全部」+ 非空分类，空分类不出现；
//! - 点分类只筛该分类，并与搜索、免费/会员 chips 叠加；右侧「N 条」跟着变；
//! - 切换一级 Tab 时分类回到「全部」，规则表也换成新 Tab 的规则；
//! - 6 列网格的卡片数量与过滤结果一致；
//! - 点卡片只把「名称 + resource_id」写进状态栏，不执行副作用；
//! - 底部 44 px 详情条显示选中资源的名称 / 完整编号 / 会员标签 / 时长，右侧「复制 action」
//!   把 `{"action":…,"resource_id":…,"resource_name":…}` 写进剪贴板并在状态栏回显；
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

const DISSOLVE_ID: &str = "9100000000000000001";
const SLIDE_ID: &str = "9100000000000000002";
const MOTION_ID: &str = "9100000000000000003";
const BLUR_ID: &str = "9100000000000000004";
const GLITCH_ID: &str = "9100000000000000005";
const LIGHT_ID: &str = "9100000000000000006";

const BORDER_EFFECT_ID: &str = "9200000000000000001";
const EFFECT_GLITCH_ID: &str = "9200000000000000002";
const EFFECT_LIGHT_ID: &str = "9200000000000000003";
const EFFECT_COMIC_ID: &str = "9200000000000000004";

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

    fn rect(&mut self, id: &str) -> [f64; 4] {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        let rect = v.as_array().and_then(|a| a.iter().find(|e| e["id"] == json!(id))).map(|e| &e["rect"]).expect("element rect");
        std::array::from_fn(|i| rect[i].as_f64().expect("numeric rect"))
    }

    fn cards(&mut self) -> Vec<String> {
        self.ids("resource.card.").into_iter().filter(|id| id.starts_with("resource.card.")).collect()
    }
}

fn write_catalog(dir: &Path) {
    std::fs::create_dir_all(dir).expect("temp catalogs dir");
    // 六条转场，每条命中规则表里的一个不同分类（「叠化 / 幻灯片 / 运镜 / 模糊 / 故障 / 光效」）。
    let transitions = json!([
        {"name": "叠化", "resource_id": DISSOLVE_ID, "vip": false, "effect_id": "2917279", "duration_us": 1_500_000},
        {"name": "上下翻页", "resource_id": SLIDE_ID, "vip": false, "effect_id": "2917280"},
        {"name": "推近", "resource_id": MOTION_ID, "vip": false, "effect_id": "2917281"},
        {"name": "模糊", "resource_id": BLUR_ID, "vip": false, "effect_id": "2917282"},
        {"name": "故障闪屏", "resource_id": GLITCH_ID, "vip": false, "effect_id": "2917283"},
        {"name": "星光", "resource_id": LIGHT_ID, "vip": true, "effect_id": "2917284"}
    ]);
    std::fs::write(dir.join("transitions.json"), serde_json::to_vec_pretty(&transitions).expect("fixture json")).expect("write transitions.json");

    // 特效目录：至少一条「边框」，并各留一条故障 / 光效 / 漫画，验证规则表随 Tab 切换。
    let effects = json!([
        {"name": "金色边框", "resource_id": BORDER_EFFECT_ID, "vip": false},
        {"name": "故障闪屏", "resource_id": EFFECT_GLITCH_ID, "vip": false},
        {"name": "发光星河", "resource_id": EFFECT_LIGHT_ID, "vip": true},
        {"name": "漫画手绘", "resource_id": EFFECT_COMIC_ID, "vip": false}
    ]);
    std::fs::write(dir.join("video_scene_effects.json"), serde_json::to_vec_pretty(&effects).expect("fixture json")).expect("write video_scene_effects.json");
}

/// 入口：建临时目录与两份目录夹具，再在真正的 `JIANYINGCRAFT_CATALOGS` 下跑场景。
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
    assert!(Path::new(&dir).join("video_scene_effects.json").is_file(), "effect fixture catalog is readable");

    // 390×884 Compact：顶栏 Tab 决定抽屉画哪个面板（Effects）。
    let mut d = Driver::new(390.0, 884.0);
    d.ok("ui.set", json!({"workspace": "Jianying"}));
    d.ok("ui.shell.tab", json!({"tab": "transitions"}));
    d.frames(4);
    assert_eq!(d.inspect()["ui"]["jy_shell_slug"], json!("transitions"), "the transitions tab is active");

    // 分类列：全部 + 六条夹具命中的六个非空分类；数量为 0 的「其他」不出现。
    let categories = d.ids("resource.category.");
    for (slug, label) in
        [("all", "全部"), ("dissolve", "叠化"), ("slide", "幻灯片"), ("motion", "运镜"), ("blur", "模糊"), ("glitch", "故障"), ("light", "光效")]
    {
        let id = format!("resource.category.{slug}");
        assert!(categories.contains(&id), "category column draws {id}; got {categories:?}");
        assert_eq!(d.label(&id), label, "category {id} has its Chinese label");
    }
    assert!(!categories.iter().any(|id| id == "resource.category.other"), "empty categories are not listed: {categories:?}");

    // 布局：[分类列 96 px][右侧内容]；每行 26 px，「全部」在最上面。
    let all = d.rect("resource.category.all");
    let dissolve = d.rect("resource.category.dissolve");
    assert!((all[2] - 96.0).abs() < 0.01, "the category column is 96 px wide: {all:?}");
    assert!((all[3] - 26.0).abs() < 0.01, "category rows are 26 px high: {all:?}");
    assert!((dissolve[0] - all[0]).abs() < 0.01, "category rows share the left column: {all:?} vs {dissolve:?}");
    assert!((dissolve[1] - all[1] - 26.0).abs() < 0.01, "「全部」is the first row, the next category starts 26 px below: {all:?} vs {dissolve:?}");
    let search = d.rect("resource.search");
    assert!(search[0] >= all[0] + 96.0, "the search field starts to the right of the category column: {search:?} vs {all:?}");

    // 初始：6 条、搜索框、chips 与网格都在。
    assert_eq!(d.cards().len(), 6, "the grid lists every fixture transition");
    assert_eq!(d.label("resource.count"), "6 条");
    for id in ["resource.search", "resource.grid", "resource.chip.all", "resource.chip.free", "resource.chip.vip"] {
        assert!(!d.ids(id).is_empty(), "{id} is not drawn in the resource tab");
    }

    // 未选中时详情条给提示；「复制 action」按钮已经就位（未选中时不可用）。
    assert_eq!(d.label("resource.detail"), "选一条资源，查看编号与 action 片段");
    assert!(!d.ids("resource.copyAction").is_empty(), "the detail bar draws its copy button before a selection");

    // 点分类「叠化」：只保留那一条，右侧计数跟着变。
    d.click("resource.category.dissolve");
    assert_eq!(d.cards(), vec![format!("resource.card.{DISSOLVE_ID}")], "the dissolve category keeps only its own entry");
    assert_eq!(d.label("resource.count"), "1 条");

    // 点「全部」：恢复整库。
    d.click("resource.category.all");
    assert_eq!(d.cards().len(), 6, "the all row restores every card");
    assert_eq!(d.label("resource.count"), "6 条");

    // 搜索与分类叠加：先搜到「星光」，再点「叠化」→ 0 条，出现空态（分类列仍在）。
    d.click("resource.search");
    d.ok("ui.type", json!({"text": "星光"}));
    d.frames(3);
    assert_eq!(d.cards(), vec![format!("resource.card.{LIGHT_ID}")], "search narrows the grid to 星光");
    d.click("resource.category.dissolve");
    assert!(d.cards().is_empty(), "search + category intersect to nothing");
    assert_eq!(d.label("resource.count"), "0 条");
    assert!(!d.ids("resource.empty").is_empty(), "an empty intersection shows the empty state");
    assert!(!d.ids("resource.category.dissolve").is_empty(), "the category column stays usable while the grid is empty");

    // 清空搜索：叠化分类回到 1 条。
    d.click("resource.search");
    d.ok("ui.key", json!({"key": "Cmd+A"}));
    d.ok("ui.key", json!({"key": "Backspace"}));
    d.frames(3);
    assert_eq!(d.cards(), vec![format!("resource.card.{DISSOLVE_ID}")], "clearing the search restores the current category");

    // chips 与分类叠加：星光为会员，免费 + 叠化仍是 1 条，会员 + 叠化是 0 条。
    d.click("resource.chip.free");
    assert_eq!(d.cards(), vec![format!("resource.card.{DISSOLVE_ID}")], "the free chip composes with the category");
    d.click("resource.chip.vip");
    assert!(d.cards().is_empty(), "the vip chip composes with the category");
    d.click("resource.chip.all");
    d.click("resource.category.all");
    assert_eq!(d.cards().len(), 6);

    // 点卡片：只写状态栏（名称 + resource_id），没有别的副作用。
    d.click(&format!("resource.card.{DISSOLVE_ID}"));
    let after = d.inspect();
    assert_eq!(after["ui"]["resource_selection"], json!(DISSOLVE_ID), "the clicked card is selected: {after}");
    let status = after["ui"]["status"].as_str().unwrap_or_default();
    assert!(status.contains(DISSOLVE_ID), "the status line carries the resource id: {status}");
    assert!(status.contains("叠化"), "the status line carries the resource name: {status}");
    assert!(status.contains("转场"), "the status line names the resource kind: {status}");

    // 底部详情条：固定 44 px、贴住面板底边、留在面板内，显示名称 / 完整编号 / 免费 / 1.5s。
    let detail = d.rect("resource.detail");
    let panel = d.rect("panel.Effects");
    assert!((detail[3] - 44.0).abs() < 0.01, "the detail bar is 44 px high: {detail:?}");
    assert!((detail[1] + detail[3] - (panel[1] + panel[3])).abs() < 0.01, "the detail bar is pinned to the panel bottom: {detail:?} vs panel {panel:?}");
    assert!(
        detail[0] >= panel[0] - 0.01 && detail[0] + detail[2] <= panel[0] + panel[2] + 0.01 && detail[1] >= panel[1] - 0.01,
        "the detail bar stays inside the panel: {detail:?} vs panel {panel:?}"
    );
    assert_eq!(d.label("resource.detail.name"), "叠化", "the detail bar shows the selected name");
    assert_eq!(d.label("resource.detail.id"), DISSOLVE_ID, "the detail bar shows the full resource id");
    let meta = d.label("resource.detail.meta");
    assert!(meta.contains("免费"), "the detail bar marks a non-VIP resource: {meta}");
    assert!(meta.contains("1.5s"), "the detail bar renders duration_us as seconds: {meta}");
    let copy = d.rect("resource.copyAction");
    assert!((copy[2] - 68.0).abs() < 0.01, "the copy button is 68 px wide: {copy:?}");
    assert!((copy[3] - 26.0).abs() < 0.01, "the copy button is 26 px high: {copy:?}");
    assert!(
        copy[1] >= detail[1] - 0.01 && copy[1] + copy[3] <= detail[1] + detail[3] + 0.01 && copy[0] + copy[2] <= detail[0] + detail[2] + 0.01,
        "the copy button sits in the detail bar's right edge: {copy:?} vs {detail:?}"
    );

    // 点「复制 action」：action 片段写进剪贴板，状态栏回显 action 名与完整编号。
    d.click("resource.copyAction");
    let copied = d.inspect();
    let copied_status = copied["ui"]["status"].as_str().unwrap_or_default();
    assert!(copied_status.contains("已复制 action 片段"), "the status reports the copied fragment: {copied_status}");
    assert!(copied_status.contains("add_transition"), "the status names the transition action: {copied_status}");
    assert!(copied_status.contains(DISSOLVE_ID), "the status carries the full resource id: {copied_status}");
    assert_eq!(copied["ui"]["resource_selection"], json!(DISSOLVE_ID), "copying keeps the card selected");

    // 切到「特效」：分类规则表换成特效表，分类选中重置为「全部」，网格读特效目录。
    d.click("resource.category.dissolve");
    d.ok("ui.shell.tab", json!({"tab": "effects"}));
    d.frames(4);
    assert_eq!(d.inspect()["ui"]["jy_shell_slug"], json!("effects"), "the effects tab is active");
    let effect_categories = d.ids("resource.category.");
    for slug in ["all", "border", "glitch", "light", "comic"] {
        let id = format!("resource.category.{slug}");
        assert!(effect_categories.contains(&id), "effect category column draws {id}; got {effect_categories:?}");
    }
    assert!(
        !effect_categories.iter().any(|id| id == "resource.category.dissolve"),
        "transition-only categories disappear on the effects tab: {effect_categories:?}"
    );
    assert_eq!(d.label("resource.category.border"), "边框", "the effects rules show the 边框 category");
    assert_eq!(d.cards().len(), 4, "the all row is selected again after the tab switch");
    assert_eq!(d.label("resource.count"), "4 条");

    // 特效分类同样只筛自己那一类。
    d.click("resource.category.border");
    assert_eq!(d.cards(), vec![format!("resource.card.{BORDER_EFFECT_ID}")], "the border category keeps only the border effect");
    assert_eq!(d.label("resource.count"), "1 条");

    // 切回转场：分类再次重置为「全部」（不是保留特效的 border）。
    d.ok("ui.shell.tab", json!({"tab": "transitions"}));
    d.frames(4);
    assert_eq!(d.cards().len(), 6, "switching back to transitions resets the category to 全部");
    assert_eq!(d.label("resource.count"), "6 条");

    // 目录消失：可操作的空态，不崩溃，也不继续画旧卡片；分类列只剩「全部」。
    std::fs::remove_dir_all(Path::new(&dir)).expect("remove the fixture catalog");
    d.frames(3);
    assert!(d.cards().is_empty(), "no cards are drawn once the catalog directory is gone");
    let empty = d.ids("resource.empty");
    assert!(!empty.is_empty(), "a missing catalog directory shows the empty state");
    let message = d.label("resource.empty");
    assert!(message.contains("JIANYINGCRAFT_CATALOGS"), "the empty state tells the user how to fix it: {message}");
    let categories = d.ids("resource.category.");
    assert!(categories.contains(&"resource.category.all".to_string()), "the all row remains while the catalog is missing: {categories:?}");
    assert!(!categories.iter().any(|id| id == "resource.category.dissolve"), "stale categories are not drawn: {categories:?}");
    // 应用还活着（下一次帧继续画，不 panic）。
    d.frames(2);
    assert_eq!(d.inspect()["ui"]["jy_shell_slug"], json!("transitions"));
}
