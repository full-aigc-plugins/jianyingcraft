//! 面板的三个宿主导入（`docs/derivation/03-panel-bridge.md` §3/§4）：
//! `craft_read` / `craft_write` / `craft_screenshot`。
//!
//! 编辑宿主会向页面注入三个全局量：`__craftRead(path)`、`__craftWrite(path, bytes)` 与
//! `__craftScreenshot(panel?)`。它们是面板桥的文件系统与截图部分；[`crate::api`] 中的四个
//! `craft_*` 队列导出是另一半。
//!
//! 这些函数是 wasm 模块的 `#[wasm_bindgen]` 导出，与 `craft_submit` 等完全一样：面板在实例化后
//! 的模块上调用 `filmcraft_web.craft_read(path)`。语义遵循 §3：
//!
//! | 导出 | 成功 | 失败 |
//! |---|---|---|
//! | `craft_read(path) -> Uint8Array` | 文件的字节 | **抛出** `Error`，其 `.code` 为 [`HostError::code`]（`path_outside_workspace`、…） |
//! | `craft_write(path, bytes) -> object` | `{ok:true, path, bytes, overwritten}` | 返回 `{ok:false, error:{code, message}}`；绝不抛出，也绝不声称一次未获确认的写入 |
//! | `craft_screenshot(panel?) -> Uint8Array \| undefined` | 该 PNG | 宿主不支持截图时返回 `undefined`：调用方退化为 [`SCREENSHOT_FALLBACK`]（`render_frame`），这不是错误 |
//!
//! 规则位于 [`crate::host_policy`]；本文件只是 JS 绑定。它会再校验一次路径语法与页面可检查的配额，
//! 通过 `js_sys::Reflect` 调用宿主全局量，并把应答映射为策略层的类型。`realpath`、包含关系与原子
//! 重命名由宿主负责，因为页面自身没有文件系统；当宿主的应答缺失或无法识别时，Rust 侧一律失败关闭
//! （fail closed）。`cargo check --target wasm32-unknown-unknown` 会对本文件做类型检查；这里没有
//! 浏览器宿主，因此宿主测试不会执行它（见该文档 §3/§4）。

use js_sys::{ArrayBuffer, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::host_policy::{HostError, MAX_ARTIFACT_BYTES, MAX_IMPORT_BYTES, Quota, SCREENSHOT_FALLBACK, Screenshot, WriteOutcome, check_syntax};

/// 宿主定义过的全局量 `name`；宿主未定义时为 `None`。
fn host_global(name: &str) -> Option<Function> {
    Reflect::get(&js_sys::global(), &JsValue::from_str(name)).ok()?.dyn_into().ok()
}

fn field_string(value: &JsValue, name: &str) -> Option<String> {
    if value.is_null() || value.is_undefined() {
        return None;
    }
    Reflect::get(value, &JsValue::from_str(name)).ok().and_then(|v| v.as_string())
}

fn field_bool(value: &JsValue, name: &str) -> Option<bool> {
    if value.is_null() || value.is_undefined() {
        return None;
    }
    Reflect::get(value, &JsValue::from_str(name)).ok().and_then(|v| v.as_bool())
}

/// 把宿主抛出的值映射为 [`HostError`]；若其 `code` 属于本模块的错误码，则原样保留。
fn host_error(error: &JsValue) -> HostError {
    let message = field_string(error, "message").or_else(|| error.as_string()).unwrap_or_else(|| format!("{error:?}"));
    match field_string(error, "code").as_deref() {
        Some("path_outside_workspace") => HostError::OutsideWorkspace { path: message },
        Some(code) => HostError::Io { operation: "host", path: String::new(), detail: format!("{code}: {message}") },
        None => HostError::Io { operation: "host", path: String::new(), detail: message },
    }
}

/// 抛出一个携带策略错误码的 `Error`（§3：越界即抛出）。
fn throw(error: &HostError) -> JsValue {
    let js = js_sys::Error::new(&error.to_string());
    let _ = Reflect::set(&js, &JsValue::from_str("code"), &JsValue::from_str(error.code()));
    js.into()
}

fn set(object: &Object, name: &str, value: &JsValue) {
    let _ = Reflect::set(object, &JsValue::from_str(name), value);
}

/// `{ok:false, error:{code, message}}` —— §3 规定的写入失败形状。
fn write_error(error: &HostError) -> JsValue {
    let detail = Object::new();
    set(&detail, "code", &JsValue::from_str(error.code()));
    set(&detail, "message", &JsValue::from_str(&error.to_string()));
    let result = Object::new();
    set(&result, "ok", &JsValue::FALSE);
    set(&result, "error", &detail);
    result.into()
}

/// `{ok:true, path, bytes, overwritten}` —— 覆盖写入会指明被替换的路径（§4.3）。
fn write_ok(outcome: &WriteOutcome) -> JsValue {
    let result = Object::new();
    set(&result, "ok", &JsValue::TRUE);
    set(&result, "path", &JsValue::from_str(&outcome.path));
    set(&result, "bytes", &JsValue::from(outcome.bytes as f64));
    match &outcome.overwritten {
        Some(path) => set(&result, "overwritten", &JsValue::from_str(path)),
        None => set(&result, "overwritten", &JsValue::NULL),
    }
    result.into()
}

/// 把宿主给出的 `Uint8Array` 或 `ArrayBuffer` 转为字节。
fn bytes_of(value: &JsValue) -> Option<Vec<u8>> {
    if let Some(bytes) = value.dyn_ref::<Uint8Array>() {
        return Some(bytes.to_vec());
    }
    if value.is_instance_of::<ArrayBuffer>() {
        return Some(Uint8Array::new(value).to_vec());
    }
    None
}

/// 在不复制的前提下取得宿主缓冲区的长度：调用方先检查配额，因此超限应答会在页面付出整份拷贝的
/// 代价之前被拒绝（AGENTS.md §0.4：按输入规模限制分配）。
fn bytes_len_of(value: &JsValue) -> Option<usize> {
    if let Some(bytes) = value.dyn_ref::<Uint8Array>() {
        return Some(bytes.length() as usize);
    }
    if value.is_instance_of::<ArrayBuffer>() {
        return Some(Uint8Array::new(value).length() as usize);
    }
    None
}

/// 经宿主读取一个工作区文件（§3）。越出工作区的路径会抛出 `path_outside_workspace`；
/// 它绝不会回退到其他位置。
#[wasm_bindgen]
pub fn craft_read(path: &str) -> Result<Vec<u8>, JsValue> {
    check_syntax(path).map_err(|e| throw(&e))?;
    let read = host_global("__craftRead")
        .ok_or_else(|| throw(&HostError::Io { operation: "__craftRead", path: path.to_string(), detail: "the host does not provide __craftRead".into() }))?;
    let reply = read.call1(&JsValue::NULL, &JsValue::from_str(path)).map_err(|e| throw(&host_error(&e)))?;
    let len = bytes_len_of(&reply)
        .ok_or_else(|| throw(&HostError::Io { operation: "__craftRead", path: path.to_string(), detail: "the host did not return bytes".into() }))?;
    if len as u64 > MAX_IMPORT_BYTES {
        return Err(throw(&HostError::Quota { quota: Quota::ImportBytes, requested: len as u64 }));
    }
    bytes_of(&reply).ok_or_else(|| throw(&HostError::Io { operation: "__craftRead", path: path.to_string(), detail: "the host did not return bytes".into() }))
}

/// 经宿主写入一个工作区文件（§3/§4.3）。返回结果对象，而不抛出异常。
#[wasm_bindgen]
pub fn craft_write(path: &str, bytes: &[u8]) -> JsValue {
    match write(path, bytes) {
        Ok(outcome) => write_ok(&outcome),
        Err(error) => write_error(&error),
    }
}

fn write(path: &str, bytes: &[u8]) -> Result<WriteOutcome, HostError> {
    check_syntax(path)?;
    let len = bytes.len() as u64;
    if len > MAX_ARTIFACT_BYTES {
        return Err(HostError::Quota { quota: Quota::ArtifactBytes, requested: len });
    }
    let write = host_global("__craftWrite").ok_or_else(|| HostError::Io {
        operation: "__craftWrite",
        path: path.to_string(),
        detail: "the host does not provide __craftWrite".into(),
    })?;
    let data = Uint8Array::from(bytes);
    let reply = write.call2(&JsValue::NULL, &JsValue::from_str(path), &data).map_err(|e| host_error(&e))?;
    parse_write(path, len, &reply)
}

/// 解析宿主的写入应答。应答缺失即视为错误：为一次无人确认的写入声称成功，会从相反方向破坏
/// “绝不留下只写了一半的文件”的承诺。
fn parse_write(path: &str, len: u64, reply: &JsValue) -> Result<WriteOutcome, HostError> {
    if reply.is_null() || reply.is_undefined() {
        return Err(HostError::Io {
            operation: "__craftWrite",
            path: path.to_string(),
            detail: "the host returned no result; refusing to assume the write happened".into(),
        });
    }
    // 只有明确的 `ok: true` 才算写成功：缺字段、非布尔、未知形状一律失败关闭。
    if field_bool(reply, "ok") != Some(true) {
        let error = Reflect::get(reply, &JsValue::from_str("error")).unwrap_or(JsValue::NULL);
        if error.is_null() || error.is_undefined() {
            return Err(HostError::Io { operation: "__craftWrite", path: path.to_string(), detail: "the host did not confirm the write with ok:true".into() });
        }
        return Err(host_error(&error));
    }
    // 回报的路径必须存在、语法合法，且与请求一致；否则产物可能落到了别处。
    let reported = field_string(reply, "path").ok_or_else(|| HostError::Io {
        operation: "__craftWrite",
        path: path.to_string(),
        detail: "the host confirmed a write without naming the path".into(),
    })?;
    check_syntax(&reported)?;
    if reported != path {
        return Err(HostError::Io { operation: "__craftWrite", path: path.to_string(), detail: format!("the host wrote {reported} instead of {path}") });
    }
    // 字节数必须等于请求；少写或多写都不是成功。
    let bytes = match Reflect::get(reply, &JsValue::from_str("bytes")).ok().and_then(|v| v.as_f64()) {
        Some(n) if n.is_finite() && n >= 0.0 && (n as u64) == len => len,
        Some(n) => {
            return Err(HostError::Io { operation: "__craftWrite", path: path.to_string(), detail: format!("the host wrote {n} of {len} bytes") });
        }
        None => {
            return Err(HostError::Io { operation: "__craftWrite", path: path.to_string(), detail: "the host did not report how many bytes it wrote".into() });
        }
    };
    let overwritten = field_string(reply, "overwritten");
    if let Some(previous) = &overwritten {
        check_syntax(previous)?;
    }
    Ok(WriteOutcome { path: reported, bytes, overwritten })
}

/// 经宿主截取面板/窗口画面（§3）。宿主没有截图能力（或返回 `null`）时为 `undefined`：
/// 调用方退化为 `render_frame`，这不算错误。
#[wasm_bindgen]
pub fn craft_screenshot(panel: Option<String>) -> Option<Vec<u8>> {
    let screenshot = host_global("__craftScreenshot")?;
    let reply = match panel {
        Some(panel) => screenshot.call1(&JsValue::NULL, &JsValue::from_str(&panel)),
        None => screenshot.call0(&JsValue::NULL),
    };
    let reply = match reply {
        Ok(reply) => reply,
        Err(e) => {
            log::warn!("__craftScreenshot failed ({e:?}); falling back to {SCREENSHOT_FALLBACK}");
            return None;
        }
    };
    let Some(len) = bytes_len_of(&reply) else {
        log::warn!("__craftScreenshot returned no bytes; falling back to {SCREENSHOT_FALLBACK}");
        return None;
    };
    if len as u64 > MAX_IMPORT_BYTES {
        log::warn!("__craftScreenshot returned {len} bytes (over the {MAX_IMPORT_BYTES} limit); falling back to {SCREENSHOT_FALLBACK}");
        return None;
    }
    match Screenshot::from_png(bytes_of(&reply)) {
        Screenshot::Png(png) => Some(png),
        Screenshot::Unsupported => None,
    }
}
