//! 面板桥的端到端测试：请求经 `Bridge` 入队，由 UI 线程执行在**可见 App** 上，结果回到回复队列。
//!
//! 覆盖 `docs/derivation/03-panel-bridge.md` 的 T1（目录一致）、T2（结果同构）、T3（队列上限）、
//! T5（可见性）与 T7（错误回传）。

use std::collections::BTreeSet;
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
        let harness = Harness::builder().with_size(egui::vec2(1280.0, 1024.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
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
}

/// 一条合法请求会被执行在可见 App 上，结果按 id 回传，且副作用能被控制通道读到。
#[test]
fn bridge_executes_commands_against_the_visible_app() {
    let mut d = Driver::demo();
    let raw = json!({"id": "q-1", "command": "file.newBin", "params": {"name": "FromBridge"}}).to_string();
    assert!(d.harness.state_mut().bridge.submit(&raw), "valid request must be accepted");
    d.frames(3);
    let reply = d.harness.state_mut().bridge.take_reply().expect("a reply for q-1");
    let reply: Value = serde_json::from_str(&reply).expect("reply is JSON");
    assert_eq!(reply["id"], json!("q-1"));
    assert_eq!(reply["ok"], json!(true), "reply: {reply}");
    let project = d.ok("engine.execute", json!({"command": "project.inspect", "params": {}}));
    assert!(project.to_string().contains("FromBridge"), "the bin was not created in the visible app: {project}");
    assert!(d.harness.state_mut().bridge.idle(), "bridge should be idle after the reply is taken");
}

/// 命令失败也要回包（`ok:false` + `command_failed`），不能静默丢弃。
#[test]
fn bridge_reports_command_failures_instead_of_dropping_them() {
    let mut d = Driver::demo();
    let raw = json!({"id": "q-2", "command": "no.suchCommand"}).to_string();
    assert!(d.harness.state_mut().bridge.submit(&raw));
    d.frames(3);
    let reply = d.harness.state_mut().bridge.take_reply().expect("a reply for q-2");
    let reply: Value = serde_json::from_str(&reply).expect("reply is JSON");
    assert_eq!(reply["id"], json!("q-2"));
    assert_eq!(reply["ok"], json!(false));
    assert_eq!(reply["error"]["code"], json!("command_failed"));
    assert!(reply["error"]["message"].as_str().is_some_and(|m| !m.is_empty()), "the error must carry a message: {reply}");
}

/// 队列上限在 App 边界上生效：第 33 条被拒，前 32 条仍可执行。
#[test]
fn bridge_limits_are_enforced_at_the_app_boundary() {
    let mut d = Driver::demo();
    for index in 0..32 {
        let raw = json!({"id": format!("q-{index}"), "command": "file.newBin"}).to_string();
        assert!(d.harness.state_mut().bridge.submit(&raw), "request {index} must be accepted");
    }
    let overflow = json!({"id": "overflow", "command": "file.newBin"}).to_string();
    assert!(!d.harness.state_mut().bridge.submit(&overflow), "the 33rd request must be refused");
    d.frames(4);
    let mut replies = 0;
    while d.harness.state_mut().bridge.take_reply().is_some() {
        replies += 1;
    }
    assert_eq!(replies, 32, "every accepted request must produce exactly one reply");
}

/// 单条回复超过 8 MiB 时，App 边界的桥回 `reply_too_large` 而不是静默丢弃；
/// 紧随其后的真实命令回复仍然可取（新回复不被挤掉）。
#[test]
fn an_oversized_bridge_reply_is_reported_and_keeps_the_next_command_reply() {
    let mut d = Driver::demo();
    let padding = "x".repeat(filmcraft_ui_egui::bridge::MAX_BYTES);
    d.harness.state_mut().bridge.push_reply("huge", Ok(json!({"padding": padding})));
    let raw = json!({"id": "q-after", "command": "file.newBin", "params": {"name": "AfterHuge"}}).to_string();
    assert!(d.harness.state_mut().bridge.submit(&raw), "the normal request is still accepted");
    d.frames(3);

    let huge: Value = serde_json::from_str(&d.harness.state_mut().bridge.take_reply().expect("the reply_too_large envelope")).expect("json");
    assert_eq!(huge["id"], json!("huge"));
    assert_eq!(huge["ok"], json!(false));
    assert_eq!(huge["error"]["code"], json!("reply_too_large"));

    let after: Value = serde_json::from_str(&d.harness.state_mut().bridge.take_reply().expect("the next command's reply")).expect("json");
    assert_eq!(after["id"], json!("q-after"));
    assert_eq!(after["ok"], json!(true), "the later reply must survive: {after}");
    assert!(d.harness.state_mut().bridge.idle(), "both replies were taken");
}

/// T2：同一批查询命令在"面板桥（可见 App）"与 headless Session 上必须返回深度相等的结果。
///
/// 只选静态查询（不带生成 id、不带时间戳），对比才有意义；`command.list` 也顺带证明
/// 两个后端的命令目录与 enabled 状态一致。
#[test]
fn bridge_results_match_headless_for_representative_commands() {
    // `command.list` 不参与比较：面板侧的会话额外注册了 UI 命令（tool.*、playback.toggle、
    // window.*），两边目录本就不同，这属于架构事实而不是回归。
    const CASES: [&str; 12] = [
        "export.presets.list",
        "export.queue.list",
        "export.formats",
        "lut.list",
        "presets.list",
        "effects.list",
        "graphics.template.list",
        "jobs.list",
        "media.findMissing",
        "project.columns.list",
        "project.view.get",
        "project.viewPreset.list",
    ];

    // headless：独立会话 + 同样的 demo 工程
    let mut headless_session = Session::default();
    headless_session.execute("file.openDemoProject", json!({})).expect("demo project");
    let headless: Vec<Value> =
        CASES.iter().map(|command| headless_session.execute(command, json!({})).unwrap_or_else(|error| panic!("{command}: {error}"))).collect();

    // 面板桥：同一批命令走 submit → pump → take_reply
    let mut d = Driver::demo();
    for (index, command) in CASES.iter().enumerate() {
        let raw = json!({"id": format!("cmp-{index}"), "command": command, "params": {}}).to_string();
        assert!(d.harness.state_mut().bridge.submit(&raw), "{command} must be accepted");
        d.frames(3);
        let reply = d.harness.state_mut().bridge.take_reply().unwrap_or_else(|| panic!("no reply for {command}"));
        let reply: Value = serde_json::from_str(&reply).expect("reply is JSON");
        assert_eq!(reply["ok"], json!(true), "{command} failed: {reply}");
        assert_eq!(reply["result"], headless[index], "{command}: panel and headless results differ");
    }
}

/// T1：`craft_catalog` 的命令 id 集合必须与执行 `command.list` 得到的 id 集合完全相等。
///
/// `craft_catalog` 在 wasm 里就是 `bridge::catalog()` 的字符串化，所以这里测的是它在 host 上
/// 同一份实现——导出被换掉、或 `command_specs()` 与 `command.list` 分叉，这个测试都会红。
#[test]
fn craft_catalog_ids_match_command_list() {
    let mut session = Session::default();
    let listed = session.execute("command.list", json!({})).expect("command.list");
    let listed: BTreeSet<&str> =
        listed.as_array().expect("command.list returns an array").iter().filter_map(|command| command.get("id").and_then(Value::as_str)).collect();
    let catalog = filmcraft_ui_egui::bridge::catalog();
    let catalog: BTreeSet<&str> = catalog
        .get("commands")
        .and_then(Value::as_array)
        .expect("catalog has a commands array")
        .iter()
        .filter_map(|command| command.get("id").and_then(Value::as_str))
        .collect();
    let specs: BTreeSet<&str> = filmcraft_engine::command_specs().iter().map(|command| command.id).collect();
    assert!(!listed.is_empty(), "command.list returned no commands at all");
    assert_eq!(catalog, listed, "craft_catalog and command.list disagree on the command id set");
    assert_eq!(specs, listed, "command_specs() and command.list disagree on the command id set");
}
