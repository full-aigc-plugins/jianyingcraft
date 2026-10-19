//! `window.filmcraft`: the control channel / agent API for the page.
//!
//! The same JSON methods as the desktop control channel (`docs/control-protocol.md`) and the
//! same engine commands as MCP, as promises:
//!
//! ```js
//! await filmcraft.execute("sequence.razor", {seconds: 2})   // engine command → its result
//! await filmcraft.request("ui.inspect")                    // control method → its result
//! await filmcraft.commands()                               // command list
//! const {pngBase64} = await filmcraft.screenshot()         // the canvas as PNG (base64)
//! await filmcraft.importUrl("/clip.mp4")                   // fetch + import (tests)
//! await filmcraft.importFiles(input.files)                 // File / FileList / File[]
//! await filmcraft.openProject(file)                        // .fcproj File or URL
//! filmcraft.files(); await filmcraft.readFile("/exports/a.mp4"); filmcraft.info()
//! ```
//!
//! Results resolve with the method's `result`; errors reject with the error message.

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use filmcraft_ui_egui::ControlRequest;
use filmcraft_ui_egui::bridge::{self, Bridge, InFlight};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise};
use web_time::Instant;

type Waiting = (Receiver<Value>, js_sys::Function, js_sys::Function);

thread_local! {
    static TX: RefCell<Option<Sender<ControlRequest>>> = const { RefCell::new(None) };
    static WAITING: RefCell<Vec<Waiting>> = const { RefCell::new(Vec::new()) };
}

pub fn set_sender(tx: Sender<ControlRequest>) {
    TX.with(|t| *t.borrow_mut() = Some(tx));
}

pub fn to_js(v: &Value) -> JsValue {
    js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL)
}

pub fn from_js(v: &JsValue) -> Value {
    if v.is_undefined() || v.is_null() {
        return Value::Null;
    }
    js_sys::JSON::stringify(v).ok().and_then(|s| s.as_string()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

/// Resolve the promises of answered control requests (called every frame).
pub fn poll_replies() {
    WAITING.with(|w| {
        w.borrow_mut().retain(|(rx, resolve, reject)| match rx.try_recv() {
            Ok(v) => {
                if v.get("ok").and_then(Value::as_bool) == Some(true) {
                    let _ = resolve.call1(&JsValue::NULL, &to_js(v.get("result").unwrap_or(&Value::Null)));
                } else {
                    let msg = v.get("error").and_then(Value::as_str).unwrap_or("error").to_string();
                    let _ = reject.call1(&JsValue::NULL, &js_sys::Error::new(&msg));
                }
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                let _ = reject.call1(&JsValue::NULL, &js_sys::Error::new("request dropped"));
                false
            }
        });
    });
}

/// Send a control request; the promise settles when the UI thread answers it.
pub fn request(method: &str, params: Value) -> js_sys::Promise {
    let method = method.to_string();
    let p = js_sys::Promise::new(&mut |resolve, reject| {
        let (req, rx) = ControlRequest::new(method.clone(), params.clone());
        let sent = TX.with(|t| t.borrow().as_ref().map(|tx| tx.send(req).is_ok()).unwrap_or(false));
        if sent {
            WAITING.with(|w| w.borrow_mut().push((rx, resolve, reject)));
        } else {
            let _ = reject.call1(&JsValue::NULL, &js_sys::Error::new("JianyingCraft is not running yet"));
        }
    });
    crate::repaint();
    p
}

// ---------------------------------------------------------------- 面板桥（craft_* 四导出）
//
// 与 `window.filmcraft.request()` 的区别：那套是 Promise；`craft_*` 是面板桥要的**同步队列**
// 语义（submit 立即返回 true/false、take_reply 轮询取回、idle 判断）。信封解析复用
// `filmcraft_ui_egui::bridge::Bridge`，执行仍然走同一条控制通道；导出的边界条件（32 条 /
// 累计 8 MiB）由 `bridge::InFlight` 记账——请求一旦投递就离开 `Bridge`，只在 `Bridge` 上
// 计数会让在途请求完全不设防。

thread_local! {
    static BRIDGE: RefCell<Bridge> = RefCell::new(Bridge::default());
    /// 已投递给控制通道、等待 UI 线程回复的请求（含字节数）。
    static IN_FLIGHT: RefCell<InFlight> = RefCell::new(InFlight::default());
    static REPLIES: RefCell<Bridge> = RefCell::new(Bridge::default());
}

/// 面板桥：提交一条 `{"id","command","params"}`；载荷非法、队列满或累计超 8 MiB 返回 false。
#[wasm_bindgen]
pub fn craft_submit(request_json: &str) -> bool {
    // 信封校验复用 `Bridge`（唯一实现）：非法 JSON、缺字段在这里就被挡下。
    if !BRIDGE.with(|b| b.borrow_mut().submit(request_json)) {
        return false;
    }
    // 立刻取出并在**投递前**记账：InFlight 超限时 `try_push` 不改任何状态并返回 false，
    // 所以被拒的请求既不会执行，也不会影响已登记的请求。
    let Some(request) = BRIDGE.with(|b| b.borrow_mut().take_request()) else {
        return false;
    };
    let (control, rx) = ControlRequest::new("engine.execute", json!({"command": request.command, "params": request.params}));
    let Some(tx) = TX.with(|t| t.borrow().clone()) else {
        REPLIES.with(|r| r.borrow_mut().push_reply(&request.id, Err("JianyingCraft is not running yet".into())));
        crate::repaint();
        return true;
    };
    if !IN_FLIGHT.with(|q| q.borrow_mut().try_push(&request.id, request_json, rx)) {
        return false;
    }
    if tx.send(control).is_err() {
        // 投递失败：回滚刚登记的条目，避免一条永远不会有回复的请求占着字节。
        IN_FLIGHT.with(|q| q.borrow_mut().pop_back());
        REPLIES.with(|r| r.borrow_mut().push_reply(&request.id, Err("JianyingCraft is not running yet".into())));
    }
    crate::repaint();
    true
}

/// 面板桥：取一条回复（JSON 字符串）；没有则返回 **null**（不是 undefined）。
///
/// 返回值用 `JsValue` 而不是 `Option<String>`：wasm-bindgen 会把 `None` 映射成 `undefined`，
/// 而契约要求 `null`（调用方两种都容忍，但契约与测试都以 null 为准）。
#[wasm_bindgen]
pub fn craft_take_reply() -> JsValue {
    collect_bridge_replies();
    match REPLIES.with(|r| r.borrow_mut().take_reply()) {
        Some(reply) => JsValue::from_str(&reply),
        None => JsValue::NULL,
    }
}

/// 面板桥：没有在途命令、没有排队请求且没有待取回复时为 true。
#[wasm_bindgen]
pub fn craft_idle() -> bool {
    !IN_FLIGHT.with(|q| q.borrow().queued()) && BRIDGE.with(|b| b.borrow().idle()) && REPLIES.with(|r| r.borrow().idle())
}

/// 面板桥：命令目录（`{"commands":[{id,label,menu,shortcut,params}]}`）。
#[wasm_bindgen]
pub fn craft_catalog() -> String {
    filmcraft_ui_egui::bridge::catalog().to_string()
}

fn collect_bridge_replies() {
    loop {
        // 只看队首：桥的回复必须与提交顺序一致。
        let ready = IN_FLIGHT.with(|q| {
            let q = q.borrow();
            let entry = q.front()?;
            match entry.receiver().try_recv() {
                Ok(v) => Some((entry.id().to_string(), Some(v))),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some((entry.id().to_string(), None)),
            }
        });
        if let Some((id, value)) = ready {
            match value {
                Some(v) if v.get("ok").and_then(Value::as_bool) == Some(true) => {
                    let result = v.get("result").cloned().unwrap_or(Value::Null);
                    REPLIES.with(|r| r.borrow_mut().push_reply(&id, Ok(result)));
                }
                Some(v) => {
                    let message = v.get("error").and_then(Value::as_str).unwrap_or("command failed").to_string();
                    REPLIES.with(|r| r.borrow_mut().push_reply(&id, Err(message)));
                }
                None => REPLIES.with(|r| r.borrow_mut().push_reply(&id, Err("request dropped".into()))),
            }
            IN_FLIGHT.with(|q| q.borrow_mut().pop_front());
            continue;
        }
        // 队首没有可取的回复：把超过 15 s 的在途请求合成 `timeout` 错误信封并移出登记簿，
        // 否则一条永不回复的请求会永久堵住后面的回复、并一直占着 32 条 / 8 MiB 预算。
        // 只回错误、不重放请求：重放会让同一条命令执行两次。
        let expired = IN_FLIGHT.with(|q| q.borrow_mut().expire(Instant::now(), bridge::REPLY_TIMEOUT));
        if expired.is_empty() {
            break;
        }
        for envelope in expired {
            REPLIES.with(|r| r.borrow_mut().push_envelope(envelope));
        }
    }
}

fn set_fn(obj: &js_sys::Object, name: &str, f: JsValue) -> Result<(), JsValue> {
    js_sys::Reflect::set(obj, &name.into(), &f).map(|_| ())
}

fn files_of(v: &JsValue) -> Vec<web_sys::File> {
    if let Some(f) = v.dyn_ref::<web_sys::File>() {
        return vec![f.clone()];
    }
    if let Some(list) = v.dyn_ref::<web_sys::FileList>() {
        return (0..list.length()).filter_map(|i| list.get(i)).collect();
    }
    if js_sys::Array::is_array(v) {
        return js_sys::Array::from(v).iter().filter_map(|x| x.dyn_into::<web_sys::File>().ok()).collect();
    }
    Vec::new()
}

/// Fetch a URL into a `File` named after it (or `name`).
pub async fn fetch_file(url: &str, name: Option<String>) -> Result<web_sys::File, JsValue> {
    let w = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(w.fetch_with_str(url)).await?.dyn_into()?;
    if !resp.ok() {
        return Err(format!("{url}: HTTP {}", resp.status()).into());
    }
    let blob: web_sys::Blob = JsFuture::from(resp.blob()?).await?.dyn_into()?;
    let name = name.unwrap_or_else(|| url.split(['?', '#']).next().unwrap_or(url).rsplit('/').next().unwrap_or("media").to_string());
    let parts = js_sys::Array::of1(&blob);
    web_sys::File::new_with_blob_sequence(&parts, &name)
}

/// Install `window.filmcraft`.
pub fn install() -> Result<(), JsValue> {
    let obj = js_sys::Object::new();
    set_fn(
        &obj,
        "request",
        Closure::<dyn Fn(String, JsValue) -> js_sys::Promise>::new(|method: String, params: JsValue| {
            let p = from_js(&params);
            request(&method, if p.is_null() { json!({}) } else { p })
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "execute",
        Closure::<dyn Fn(String, JsValue) -> js_sys::Promise>::new(|command: String, params: JsValue| {
            let p = from_js(&params);
            request("engine.execute", json!({"command": command, "params": if p.is_null() { json!({}) } else { p }}))
        })
        .into_js_value(),
    )?;
    set_fn(&obj, "commands", Closure::<dyn Fn() -> js_sys::Promise>::new(|| request("engine.commands", json!({}))).into_js_value())?;
    set_fn(&obj, "inspect", Closure::<dyn Fn() -> js_sys::Promise>::new(|| request("ui.inspect", json!({}))).into_js_value())?;
    set_fn(
        &obj,
        "screenshot",
        Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(|params: JsValue| {
            let p = from_js(&params);
            request("ui.screenshot", if p.is_null() { json!({}) } else { p })
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "info",
        Closure::<dyn Fn() -> JsValue>::new(|| {
            let mut v = crate::info();
            v["webcodecsStats"] = crate::webcodecs::stats();
            v["fetchesInFlight"] = json!(crate::fs::inflight());
            to_js(&v)
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "files",
        Closure::<dyn Fn() -> JsValue>::new(|| {
            to_js(&Value::Array(crate::fs::list().into_iter().map(|(p, n, k)| json!({"path": p, "size": n, "kind": k})).collect()))
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "readFile",
        Closure::<dyn Fn(String) -> js_sys::Promise>::new(|path: String| {
            future_to_promise(caught(async move {
                let blob = crate::fs::blob(&path).ok_or_else(|| JsValue::from_str(&format!("{path}: not found")))?;
                let buf = JsFuture::from(blob.array_buffer()).await?;
                Ok(js_sys::Uint8Array::new(&buf).into())
            }))
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "importFiles",
        Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(|files: JsValue| {
            let files = files_of(&files);
            future_to_promise(async move { Ok(to_js(&crate::import::import_files(files).await)) })
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "importUrl",
        Closure::<dyn Fn(String, JsValue) -> js_sys::Promise>::new(|url: String, name: JsValue| {
            future_to_promise(caught(async move {
                let f = fetch_file(&url, name.as_string()).await?;
                Ok(to_js(&crate::import::import_files(vec![f]).await))
            }))
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "openProject",
        Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(|src: JsValue| {
            future_to_promise(caught(async move {
                let file = match src.as_string() {
                    Some(url) => fetch_file(&url, None).await?,
                    None => files_of(&src).into_iter().next().ok_or("need a File or URL")?,
                };
                crate::import::open_project_file(file).await.map(|v| to_js(&v)).map_err(|e| JsValue::from(js_sys::Error::new(&e)))
            }))
        })
        .into_js_value(),
    )?;
    let w = web_sys::window().ok_or("no window")?;
    js_sys::Reflect::set(&w, &"filmcraft".into(), &obj)?;
    Ok(())
}

/// `f`, rejecting with an [`as_error`] `Error`.
async fn caught(f: impl std::future::Future<Output = Result<JsValue, JsValue>>) -> Result<JsValue, JsValue> {
    f.await.map_err(as_error)
}

/// Every member rejects with an `Error` (#90), as `execute` and `request` do, so `e.message` and
/// `e instanceof Error` work: a plain string (a fetch status, "not found") becomes one, and an
/// `Error` passes through unchanged.
fn as_error(e: JsValue) -> JsValue {
    if e.is_instance_of::<js_sys::Error>() {
        return e;
    }
    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
    js_sys::Error::new(&msg).into()
}
