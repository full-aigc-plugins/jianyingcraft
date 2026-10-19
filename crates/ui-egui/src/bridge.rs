//! 面板桥的 Rust 侧队列（对应 `docs/derivation/03-panel-bridge.md`）。
//!
//! 面板（WASM 可见 App）与 MCP adapter 之间只传 JSON 字符串：adapter 调
//! [`Bridge::submit`] 入队，UI 线程每帧调 [`FilmcraftApp::pump_bridge`] 执行并把结果放进回复队列，
//! adapter 再调 [`Bridge::take_reply`] 取走。队列上限 32 条 / 8 MiB，超限返回 `false`
//! 而不是丢弃已有请求——这一点由单测锁死。
//!
//! [`FilmcraftApp::pump_bridge`]: crate::FilmcraftApp::pump_bridge

use std::collections::VecDeque;

use serde_json::{Value, json};

/// 未执行请求的条数上限。
pub const MAX_PENDING: usize = 32;
/// 未执行请求 + 未取走回复的总字节上限（8 MiB）。
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

/// 一条已校验、待执行的命令请求。
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// 调用方给的请求 id，回复里原样带回。
    pub id: String,
    /// engine 命令 id（与 `command_list` 一致）。
    pub command: String,
    /// 命令参数，原样交给命令面。
    pub params: Value,
    /// 该请求在队列里占用的字节数（出队时释放）。
    bytes: usize,
}

/// 桥的两端队列。
#[derive(Debug, Default)]
pub struct Bridge {
    pending: VecDeque<Request>,
    replies: VecDeque<String>,
    bytes: usize,
    rejected: u64,
}

impl Bridge {
    /// 入队一条请求；`raw` 是 adapter 传来的 JSON 字符串。
    ///
    /// 返回 `false` 表示**没有被接受**：非法 JSON、缺 `id`/命令 id、或超过条数/字节上限。
    /// 已入队的请求不受影响（不丢帧、不静默截断）。
    pub fn submit(&mut self, raw: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            self.rejected += 1;
            return false;
        };
        let Some(id) = value.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) else {
            self.rejected += 1;
            return false;
        };
        let command = value.get("command").or_else(|| value.get("id_command")).and_then(Value::as_str).filter(|command| !command.is_empty());
        let Some(command) = command else {
            self.rejected += 1;
            return false;
        };
        let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() {
            self.rejected += 1;
            return false;
        }
        let bytes = raw.len();
        if self.pending.len() >= MAX_PENDING || self.bytes.saturating_add(bytes).saturating_add(self.reply_bytes()) > MAX_BYTES {
            self.rejected += 1;
            return false;
        }
        self.pending.push_back(Request { id: id.to_string(), command: command.to_string(), params, bytes });
        self.bytes = self.bytes.saturating_add(bytes);
        true
    }

    /// 取出一条待执行请求（FIFO），同时释放它占用的字节。
    pub fn take_request(&mut self) -> Option<Request> {
        let request = self.pending.pop_front()?;
        self.bytes = self.bytes.saturating_sub(request.bytes);
        Some(request)
    }

    /// 放入一条命令结果；`id` 原样带回。
    pub fn push_reply(&mut self, id: &str, outcome: Result<Value, String>) {
        let reply = match outcome {
            Ok(result) => json!({"id": id, "ok": true, "result": result}),
            Err(message) => json!({"id": id, "ok": false, "error": {"code": "command_failed", "message": message}}),
        };
        self.replies.push_back(reply.to_string());
        // 回复同样受 8 MiB 约束：超出时丢最旧的，避免无限增长。
        while self.reply_bytes() > MAX_BYTES {
            self.replies.pop_front();
        }
    }

    /// 取走一条回复 JSON 字符串。
    pub fn take_reply(&mut self) -> Option<String> {
        self.replies.pop_front()
    }

    /// 队列空且没有待取回复时为 `true`。
    pub fn idle(&self) -> bool {
        self.pending.is_empty() && self.replies.is_empty()
    }

    /// 待执行请求占用的字节数（不含回复）。
    pub fn pending_bytes(&self) -> usize {
        self.bytes
    }

    /// 被拒绝的请求数，便于诊断 adapter 与上限配置。
    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    fn reply_bytes(&self) -> usize {
        self.replies.iter().map(String::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str, command: &str) -> String {
        json!({"id": id, "command": command, "params": {"name": "B"}}).to_string()
    }

    #[test]
    fn accepts_a_valid_request_and_returns_it_in_order() {
        let mut bridge = Bridge::default();
        assert!(bridge.submit(&request("q-1", "file.newBin")));
        assert!(!bridge.idle());
        let taken = bridge.take_request().expect("request");
        assert_eq!(taken.id, "q-1");
        assert_eq!(taken.command, "file.newBin");
        assert_eq!(taken.params["name"], json!("B"));
        assert_eq!(bridge.pending_bytes(), 0);
        assert!(bridge.idle());
    }

    #[test]
    fn accepts_the_id_command_spelling_used_by_the_design_doc() {
        let mut bridge = Bridge::default();
        assert!(bridge.submit(&json!({"id": "q-2", "id_command": "timeline.razor"}).to_string()));
        assert_eq!(bridge.take_request().expect("request").command, "timeline.razor");
    }

    #[test]
    fn rejects_malformed_requests_without_touching_the_queue() {
        let mut bridge = Bridge::default();
        assert!(bridge.submit(&request("kept", "file.newBin")));
        for bad in [
            "not json".to_string(),
            json!({"command": "file.newBin"}).to_string(),
            json!({"id": "", "command": "file.newBin"}).to_string(),
            json!({"id": "x"}).to_string(),
            json!({"id": "x", "command": "file.newBin", "params": [1, 2]}).to_string(),
        ] {
            assert!(!bridge.submit(&bad), "{bad} must be rejected");
        }
        assert_eq!(bridge.take_request().expect("kept request").id, "kept");
        assert_eq!(bridge.rejected(), 5);
    }

    #[test]
    fn the_thirty_third_pending_request_is_refused() {
        let mut bridge = Bridge::default();
        for index in 0..MAX_PENDING {
            assert!(bridge.submit(&request(&format!("q-{index}"), "file.newBin")), "request {index}");
        }
        assert!(!bridge.submit(&request("overflow", "file.newBin")), "the 33rd request must be refused");
        assert_eq!(bridge.rejected(), 1);
        // 已入队的 32 条仍然完整可用。
        let mut ids = Vec::new();
        while let Some(request) = bridge.take_request() {
            ids.push(request.id);
        }
        assert_eq!(ids.len(), MAX_PENDING);
        assert_eq!(ids.first().map(String::as_str), Some("q-0"));
        assert_eq!(ids.last().map(String::as_str), Some("q-31"));
    }

    #[test]
    fn oversized_payloads_are_refused_by_the_byte_budget() {
        let mut bridge = Bridge::default();
        let huge = "x".repeat(MAX_BYTES);
        let raw = json!({"id": "big", "command": "file.newBin", "params": {"name": huge}}).to_string();
        assert!(!bridge.submit(&raw), "a payload over 8 MiB must be refused");
        assert_eq!(bridge.rejected(), 1);
    }

    #[test]
    fn replies_carry_the_request_id_and_outcome() {
        let mut bridge = Bridge::default();
        bridge.push_reply("q-9", Ok(json!({"cuts": 2})));
        bridge.push_reply("q-10", Err("no clips selected".into()));
        let first: Value = serde_json::from_str(&bridge.take_reply().expect("ok reply")).expect("json");
        assert_eq!(first["id"], json!("q-9"));
        assert_eq!(first["ok"], json!(true));
        assert_eq!(first["result"]["cuts"], json!(2));
        let second: Value = serde_json::from_str(&bridge.take_reply().expect("error reply")).expect("json");
        assert_eq!(second["id"], json!("q-10"));
        assert_eq!(second["ok"], json!(false));
        assert_eq!(second["error"]["code"], json!("command_failed"));
        assert!(bridge.take_reply().is_none());
    }

    #[test]
    fn replies_are_bounded_as_well() {
        let mut bridge = Bridge::default();
        for index in 0..64 {
            bridge.push_reply(&format!("q-{index}"), Ok(json!({ "padding": "x".repeat(256 * 1024) })));
        }
        assert!(bridge.reply_bytes() <= MAX_BYTES);
        assert!(bridge.replies.len() < 64);
    }
}
