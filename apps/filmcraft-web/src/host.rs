//! The panel's three host imports (§3/§4 of `docs/derivation/03-panel-bridge.md`):
//! `craft_read` / `craft_write` / `craft_screenshot`.
//!
//! The editing host injects three globals into the page: `__craftRead(path)`,
//! `__craftWrite(path, bytes)` and `__craftScreenshot(panel?)`. They are the file-system and
//! screenshot half of the panel bridge; the four `craft_*` queue exports in [`crate::api`] are the
//! other half.
//!
//! These functions are `#[wasm_bindgen]` exports of the wasm module, exactly like `craft_submit`
//! and friends: the panel calls `filmcraft_web.craft_read(path)` on the instantiated module. The
//! semantics follow §3:
//!
//! | export | success | failure |
//! |---|---|---|
//! | `craft_read(path) -> Uint8Array` | the file's bytes | **throws** an `Error` whose `.code` is a [`HostError::code`] (`path_outside_workspace`, …) |
//! | `craft_write(path, bytes) -> object` | `{ok:true, path, bytes, overwritten}` | returns `{ok:false, error:{code, message}}`; never throws, never claims a write it did not see confirmed |
//! | `craft_screenshot(panel?) -> Uint8Array \| undefined` | the PNG | `undefined` when the host has no screenshots: the caller degrades to [`SCREENSHOT_FALLBACK`] (`render_frame`), which is not an error |
//!
//! The rules live in [`crate::host_policy`]; this file is only the JS binding. It re-checks the path
//! syntax and the quotas a page can check, calls the host global through `js_sys::Reflect`, and maps
//! the answer into the policy's types. The host owns `realpath`, containment and the atomic rename
//! because a page has no file system of its own; the Rust side fails closed when the host's answer is
//! missing or unrecognisable. `cargo check --target wasm32-unknown-unknown` type-checks this file;
//! there is no browser host here, so it is not executed by the host tests (see §3/§4 of the doc).

use js_sys::{ArrayBuffer, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::host_policy::{HostError, MAX_ARTIFACT_BYTES, MAX_IMPORT_BYTES, Quota, SCREENSHOT_FALLBACK, Screenshot, WriteOutcome, check_syntax};

/// The host global `name`, when the host defined it.
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

/// Map a value the host threw into a [`HostError`], keeping its `code` when it is one of ours.
fn host_error(error: &JsValue) -> HostError {
    let message = field_string(error, "message").or_else(|| error.as_string()).unwrap_or_else(|| format!("{error:?}"));
    match field_string(error, "code").as_deref() {
        Some("path_outside_workspace") => HostError::OutsideWorkspace { path: message },
        Some(code) => HostError::Io { operation: "host", path: String::new(), detail: format!("{code}: {message}") },
        None => HostError::Io { operation: "host", path: String::new(), detail: message },
    }
}

/// Throw an `Error` carrying the policy's error code (§3: an escape throws).
fn throw(error: &HostError) -> JsValue {
    let js = js_sys::Error::new(&error.to_string());
    let _ = Reflect::set(&js, &JsValue::from_str("code"), &JsValue::from_str(error.code()));
    js.into()
}

fn set(object: &Object, name: &str, value: &JsValue) {
    let _ = Reflect::set(object, &JsValue::from_str(name), value);
}

/// `{ok:false, error:{code, message}}` — the write failure shape of §3.
fn write_error(error: &HostError) -> JsValue {
    let detail = Object::new();
    set(&detail, "code", &JsValue::from_str(error.code()));
    set(&detail, "message", &JsValue::from_str(&error.to_string()));
    let result = Object::new();
    set(&result, "ok", &JsValue::FALSE);
    set(&result, "error", &detail);
    result.into()
}

/// `{ok:true, path, bytes, overwritten}` — an overwrite names the path it replaced (§4.3).
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

/// A `Uint8Array` or `ArrayBuffer` from the host, as bytes.
fn bytes_of(value: &JsValue) -> Option<Vec<u8>> {
    if let Some(bytes) = value.dyn_ref::<Uint8Array>() {
        return Some(bytes.to_vec());
    }
    if value.is_instance_of::<ArrayBuffer>() {
        return Some(Uint8Array::new(value).to_vec());
    }
    None
}

/// Read a workspace file through the host (§3). A path that escapes the workspace throws
/// `path_outside_workspace`; it never falls back to another location.
#[wasm_bindgen]
pub fn craft_read(path: &str) -> Result<Vec<u8>, JsValue> {
    check_syntax(path).map_err(|e| throw(&e))?;
    let read = host_global("__craftRead")
        .ok_or_else(|| throw(&HostError::Io { operation: "__craftRead", path: path.to_string(), detail: "the host does not provide __craftRead".into() }))?;
    let reply = read.call1(&JsValue::NULL, &JsValue::from_str(path)).map_err(|e| throw(&host_error(&e)))?;
    let bytes = bytes_of(&reply)
        .ok_or_else(|| throw(&HostError::Io { operation: "__craftRead", path: path.to_string(), detail: "the host did not return bytes".into() }))?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(throw(&HostError::Quota { quota: Quota::ImportBytes, requested: bytes.len() as u64 }));
    }
    Ok(bytes)
}

/// Write a workspace file through the host (§3/§4.3). Returns a result object instead of throwing.
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

/// Read the host's write reply. A missing reply is an error: claiming success for a write nobody
/// confirmed would break the "never leave a half-written file" promise in the other direction.
fn parse_write(path: &str, len: u64, reply: &JsValue) -> Result<WriteOutcome, HostError> {
    if reply.is_null() || reply.is_undefined() {
        return Err(HostError::Io {
            operation: "__craftWrite",
            path: path.to_string(),
            detail: "the host returned no result; refusing to assume the write happened".into(),
        });
    }
    if field_bool(reply, "ok") == Some(false) {
        let error = Reflect::get(reply, &JsValue::from_str("error")).unwrap_or(JsValue::NULL);
        return Err(host_error(&error));
    }
    let reported = field_string(reply, "path").unwrap_or_else(|| path.to_string());
    let bytes = Reflect::get(reply, &JsValue::from_str("bytes"))
        .ok()
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map_or(len, |n| (n as u64).min(len));
    Ok(WriteOutcome { path: reported, bytes, overwritten: field_string(reply, "overwritten") })
}

/// Screenshot the panel/window through the host (§3). `undefined` when the host has no screenshot
/// support (or returned `null`): the caller degrades to `render_frame`, which is not an error.
#[wasm_bindgen]
pub fn craft_screenshot(panel: Option<String>) -> Option<Vec<u8>> {
    let Some(screenshot) = host_global("__craftScreenshot") else {
        return None;
    };
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
    match Screenshot::from_png(bytes_of(&reply)) {
        Screenshot::Png(png) => Some(png),
        Screenshot::Unsupported => None,
    }
}
