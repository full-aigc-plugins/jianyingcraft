//! 面板桥的 Rust 侧队列（对应 `docs/derivation/03-panel-bridge.md`）。
//!
//! 面板（WASM 可见 App）与 MCP adapter 之间只传 JSON 字符串：adapter 调
//! [`Bridge::submit`] 入队，UI 线程每帧调 [`FilmcraftApp::pump_bridge`] 执行并把结果放进回复队列，
//! adapter 再调 [`Bridge::take_reply`] 取走。队列上限 32 条 / 8 MiB，超限返回 `false`
//! 而不是丢弃已有请求——这一点由单测锁死。
//!
//! wasm 导出（`apps/filmcraft-web/src/api.rs`）没有 UI 线程的队列可存：请求一入队就投给
//! `ControlRequest`，所以在途请求必须单独记账，否则累计 8 MiB 形同虚设。[`InFlight`] 就是
//! 导出边界的登记簿；[`catalog`] 是 `craft_catalog` 的目录来源。
//!
//! 两条不会让调用方空等的边界：
//! - 在途请求超过 [`REPLY_TIMEOUT`] 没有回复时，[`InFlight::expire`] 把它取出来并合成
//!   `timeout` 错误信封——只回错误，**不重放**请求，队首因此不会永久堵住后面的回复。
//! - 回复超过 8 MiB 预算时，[`Bridge::push_reply`] 用小的 `reply_too_large` /
//!   `reply_evicted` 错误信封代替，而不是静默丢掉被挤掉的请求。
//!
//! [`FilmcraftApp::pump_bridge`]: crate::FilmcraftApp::pump_bridge

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::{Value, json};
use web_time::Instant;

/// 未执行请求的条数上限。
pub const MAX_PENDING: usize = 32;
/// 未执行请求 + 未取走回复的总字节上限（8 MiB）。
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// 在途请求等待可见 App 回复的上限（15 s，与面板心跳 / 断开判定对齐）。
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(15);

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
    /// 回复队列占用的字节数（随入队/取走/替换同步维护）。
    reply_total: usize,
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
    ///
    /// 单条回复本身超过 [`MAX_BYTES`] 时改入一条小的 `reply_too_large` 错误信封；累计预算
    /// 溢出时按最旧优先丢弃，并为被丢的 id 合成 `reply_evicted` 错误信封。两种情况下
    /// **新回复都不会丢**。
    pub fn push_reply(&mut self, id: &str, outcome: Result<Value, String>) {
        let reply = match outcome {
            Ok(result) => json!({"id": id, "ok": true, "result": result}).to_string(),
            Err(message) => error_envelope(id, "command_failed", &message),
        };
        self.push_envelope(reply);
    }

    /// 放入一条已经编码好的回复信封（超时等驱动侧错误也走这里）。
    ///
    /// 与 [`Bridge::push_reply`] 共用同一套预算规则：过大的单条回复换成 `reply_too_large`，
    /// 总预算溢出时把最旧的换成 `reply_evicted`，队尾的新信封永不丢。
    pub fn push_envelope(&mut self, envelope: String) {
        let envelope = if envelope.len() > MAX_BYTES {
            let id = reply_id(&envelope).unwrap_or_default();
            error_envelope(&id, "reply_too_large", &format!("the reply exceeded the {} MiB bridge budget", MAX_BYTES / (1024 * 1024)))
        } else {
            envelope
        };
        self.reply_total = self.reply_total.saturating_add(envelope.len());
        self.replies.push_back(envelope);
        self.enforce_reply_budget();
    }

    /// 取走一条回复 JSON 字符串。
    pub fn take_reply(&mut self) -> Option<String> {
        let reply = self.replies.pop_front()?;
        self.reply_total = self.reply_total.saturating_sub(reply.len());
        Some(reply)
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
        self.reply_total
    }

    /// 回复总预算溢出时，把最旧的回复逐个替换成小的 `reply_evicted` 信封（保留该 id 的
    /// 回答位置与顺序），直到回到 8 MiB 以内。
    ///
    /// 队尾是刚放入的新回复，不参与替换：被挤掉的只能更旧，新回复永不丢。
    fn enforce_reply_budget(&mut self) {
        if self.reply_total <= MAX_BYTES {
            return;
        }
        // 极端情况下新回复本身就接近 8 MiB，连小的错误信封也放不下；下面的兜底循环会
        // 继续丢掉最旧的（仍保留队尾的新回复），保证队列不会无限增长。
        let mut index = 0;
        while self.reply_total > MAX_BYTES && index + 1 < self.replies.len() {
            let Some(old) = self.replies.get(index) else { break };
            let replacement = match reply_id(old) {
                Some(id) => error_envelope(&id, "reply_evicted", "the reply was dropped to stay within the bridge budget"),
                None => error_envelope("", "reply_evicted", "the reply was dropped to stay within the bridge budget"),
            };
            self.reply_total = self.reply_total.saturating_sub(old.len()).saturating_add(replacement.len());
            self.replies[index] = replacement;
            index += 1;
        }
        while self.reply_total > MAX_BYTES && self.replies.len() > 1 {
            let Some(old) = self.replies.pop_front() else { break };
            self.reply_total = self.reply_total.saturating_sub(old.len());
        }
    }
}

/// 回复信封里的请求 id；不是本桥自己写的 JSON 时返回 `None`。
fn reply_id(envelope: &str) -> Option<String> {
    serde_json::from_str::<Value>(envelope).ok()?.get("id")?.as_str().map(str::to_string)
}

/// 构造一条错误回复信封。
fn error_envelope(id: &str, code: &str, message: &str) -> String {
    json!({"id": id, "ok": false, "error": {"code": code, "message": message}}).to_string()
}

/// 构造超时错误信封：按原 id 返回，且不重放请求。
fn timeout_envelope(id: &str, timeout: Duration) -> String {
    error_envelope(id, "timeout", &format!("the visible app did not answer within {} s", timeout.as_secs()))
}

/// 一条已投递给控制通道、等待 UI 线程回复的面板桥请求。
#[derive(Debug)]
pub struct InFlightEntry {
    id: String,
    bytes: usize,
    receiver: Receiver<Value>,
    /// 登记时刻（单调时钟），供 [`InFlight::expire`] 判断是否超时。
    started: Instant,
}

impl InFlightEntry {
    /// 调用方给的请求 id（回复信封里原样带回）。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 回复通道；用 `try_recv` 非阻塞轮询（它只借 `&self`）。
    pub fn receiver(&self) -> &Receiver<Value> {
        &self.receiver
    }

    /// 该条目在累计上限里占用的请求字节数。
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// 该条目的登记时刻（单调时钟）。
    pub fn started(&self) -> Instant {
        self.started
    }
}

/// 面板桥「已投递、未取回」请求的登记簿（导出边界）。
///
/// 与 [`Bridge`] 的分工：`Bridge` 是 UI 线程自己消费的队列，请求入队后下一帧就被取走；
/// `InFlight` 记的是已经交给 `ControlRequest`、正在等回复的请求。导出层必须**先在这里记账再
/// 投递**，否则每条请求在 `Bridge` 里只停留一瞬，累计 8 MiB 永远算不到在途的那批
/// （32 条 × 单条 8 MiB 会一起堆在通道里）。
///
/// 上限与 [`Bridge`] 共用 [`MAX_PENDING`] / [`MAX_BYTES`]：条数满、或本条原始 JSON 会让累计
/// 字节越过 8 MiB 时拒绝，且**不改变任何已有条目**。
#[derive(Debug, Default)]
pub struct InFlight {
    entries: VecDeque<InFlightEntry>,
    bytes: usize,
    rejected: u64,
}

impl InFlight {
    /// 空登记簿。
    pub fn new() -> Self {
        Self::default()
    }

    /// 尝试登记一条在途请求；超限返回 `false` 且**不改变任何状态**（已有的仍可逐条取回）。
    ///
    /// `request_json` 只用来计字节，不复制保存：累计上限覆盖的是原始请求载荷，解码后的
    /// 回复字符串另由 [`Bridge`] 的回复队列按 8 MiB 封顶。登记时刻取当前单调时钟。
    pub fn try_push(&mut self, id: &str, request_json: &str, receiver: Receiver<Value>) -> bool {
        self.try_push_at(id, request_json, receiver, Instant::now())
    }

    /// 与 [`InFlight::try_push`] 相同，但显式指定登记时刻。
    ///
    /// 供嵌入方注入单调时钟，也让测试无需 `sleep` 就能驱动 [`InFlight::expire`]。
    pub fn try_push_at(&mut self, id: &str, request_json: &str, receiver: Receiver<Value>, started: Instant) -> bool {
        let bytes = request_json.len();
        if self.entries.len() >= MAX_PENDING || self.bytes.saturating_add(bytes) > MAX_BYTES {
            self.rejected += 1;
            return false;
        }
        self.entries.push_back(InFlightEntry { id: id.to_string(), bytes, receiver, started });
        self.bytes = self.bytes.saturating_add(bytes);
        true
    }

    /// 取出所有登记后超过 `timeout` 仍未回复的条目，并为每个 id 合成一个 `timeout` 错误信封。
    ///
    /// `now` 由调用方传入（wasm 侧用 `web_time::Instant::now()`），所以测试可以用固定时刻
    /// 驱动，不需要 `sleep`。返回的信封按提交顺序排列；条目被移出登记簿，字节预算随之释放。
    /// 超时只回错误、**不重放**请求——重放可能让同一条命令执行两次。
    pub fn expire(&mut self, now: Instant, timeout: Duration) -> Vec<String> {
        let mut expired = Vec::new();
        let mut index = 0;
        while index < self.entries.len() {
            let stale = self.entries.get(index).is_some_and(|entry| now.saturating_duration_since(entry.started) >= timeout);
            if !stale {
                index += 1;
                continue;
            }
            let Some(entry) = self.entries.remove(index) else { break };
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            expired.push(timeout_envelope(&entry.id, timeout));
        }
        expired
    }

    /// 队首（最早投递、必须最先回复的那条），用于非阻塞轮询；不释放字节。
    pub fn front(&self) -> Option<&InFlightEntry> {
        self.entries.front()
    }

    /// 取走队首（回复已收下），同时释放它占用的字节。
    pub fn pop_front(&mut self) -> Option<InFlightEntry> {
        let entry = self.entries.pop_front()?;
        self.bytes = self.bytes.saturating_sub(entry.bytes);
        Some(entry)
    }

    /// 回滚刚登记的最后一条（投递 `ControlRequest` 失败时用）：wasm 单线程下它就是
    /// `try_push` 刚放进去的那条。释放字节，避免一条永远不会有回复的条目占着预算。
    pub fn pop_back(&mut self) -> Option<InFlightEntry> {
        let entry = self.entries.pop_back()?;
        self.bytes = self.bytes.saturating_sub(entry.bytes);
        Some(entry)
    }

    /// 是否有在途请求（`craft_idle` 用）。
    pub fn queued(&self) -> bool {
        !self.entries.is_empty()
    }

    /// 在途请求条数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否没有在途请求。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 在途请求占用的字节数（按原始请求 JSON 计）。
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// 被拒绝的登记数，便于诊断 adapter 与上限配置。
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
}

/// 面板桥的命令目录（`craft_catalog` 的 Rust 侧实现），`{"commands":[{id,label,menu,shortcut,params}]}`。
///
/// 与 `command.list` 同源（[`filmcraft_engine::command_specs`]），T1 测试锁死两者 id 集合相等。
pub fn catalog() -> Value {
    let commands: Vec<Value> = filmcraft_engine::command_specs()
        .iter()
        .map(|c| json!({"id": c.id, "label": c.label, "menu": c.menu, "shortcut": c.shortcut, "params": c.params}))
        .collect();
    json!({"commands": commands})
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use web_time::Instant;

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

    /// 回复预算按字节封顶；超出的旧回复被换成 `reply_evicted` 信封（不是静默消失），
    /// 因此每条请求仍然有且只有一个回答位置。
    #[test]
    fn replies_are_bounded_as_well() {
        let mut bridge = Bridge::default();
        for index in 0..64 {
            bridge.push_reply(&format!("q-{index}"), Ok(json!({ "padding": "x".repeat(256 * 1024) })));
            assert!(bridge.reply_bytes() <= MAX_BYTES, "the reply budget must hold after reply {index}");
        }
        assert_eq!(bridge.replies.len(), 64, "every request keeps one (small) error envelope");
    }

    /// 单条回复超过 8 MiB：换成小的 `reply_too_large`，且紧随其后的正常回复不被丢掉。
    #[test]
    fn an_oversized_reply_becomes_a_small_error_and_keeps_the_next_reply() {
        let mut bridge = Bridge::default();
        bridge.push_reply("huge", Ok(json!({"padding": "x".repeat(MAX_BYTES)})));
        bridge.push_reply("after", Ok(json!({"cuts": 1})));
        assert!(bridge.reply_bytes() <= MAX_BYTES);

        let huge: Value = serde_json::from_str(&bridge.take_reply().expect("the reply_too_large envelope")).expect("json");
        assert_eq!(huge["id"], json!("huge"));
        assert_eq!(huge["ok"], json!(false));
        assert_eq!(huge["error"]["code"], json!("reply_too_large"));
        assert!(huge["error"]["message"].as_str().is_some_and(|message| message.contains("8 MiB")), "the error must say which budget was exceeded: {huge}");

        let after: Value = serde_json::from_str(&bridge.take_reply().expect("the later reply survives")).expect("json");
        assert_eq!(after["id"], json!("after"));
        assert_eq!(after["ok"], json!(true));
        assert_eq!(after["result"]["cuts"], json!(1));
        assert!(bridge.take_reply().is_none());
    }

    /// 总预算溢出：最旧的回复被 `reply_evicted` 信封取代，最新回复保留。
    #[test]
    fn overflowing_the_reply_budget_evicts_the_oldest_with_an_error_envelope() {
        let mut bridge = Bridge::default();
        for index in 0..40 {
            bridge.push_reply(&format!("q-{index}"), Ok(json!({"padding": "x".repeat(256 * 1024)})));
        }
        assert!(bridge.reply_bytes() <= MAX_BYTES);

        let mut replies = Vec::new();
        while let Some(reply) = bridge.take_reply() {
            replies.push(serde_json::from_str::<Value>(&reply).expect("reply is JSON"));
        }
        assert_eq!(replies.len(), 40, "each request keeps an answer slot");
        let first = replies.first().expect("at least one reply");
        assert_eq!(first["id"], json!("q-0"));
        assert_eq!(first["ok"], json!(false), "the oldest reply must be reported as evicted: {first}");
        assert_eq!(first["error"]["code"], json!("reply_evicted"));
        let last = replies.last().expect("the newest reply");
        assert_eq!(last["id"], json!("q-39"));
        assert_eq!(last["ok"], json!(true), "the newest reply must never be dropped: {last}");
        assert!(replies.iter().filter(|reply| reply["ok"] == json!(false)).count() > 0);
    }

    // ------------------------------------------------ 导出边界：在途登记簿（InFlight）

    /// 累计字节上限：每条略大于 8 MiB / 4，第 4 条必然越界；越界的那条被拒且不改状态，
    /// 前 3 条仍可逐条取回（这就是 `api.rs` 的 `craft_submit` 边界）。
    #[test]
    fn the_cumulative_byte_budget_is_enforced_by_the_bookkeeping() {
        let mut in_flight = InFlight::default();
        let padding = "x".repeat(MAX_BYTES / 4);
        for index in 0..3 {
            let id = format!("q-{index}");
            let raw = json!({"id": &id, "command": "file.newBin", "params": {"name": &padding}}).to_string();
            let (_tx, rx) = channel();
            assert!(in_flight.try_push(&id, &raw, rx), "request {index} is within the budget");
        }
        let before = in_flight.bytes();
        assert!(before <= MAX_BYTES, "three entries alone must stay under the cap");
        let raw = json!({"id": "boundary", "command": "file.newBin", "params": {"name": &padding}}).to_string();
        let (_tx, rx) = channel();
        assert!(!in_flight.try_push("boundary", &raw, rx), "the request that would cross 8 MiB must be refused");
        assert_eq!(in_flight.bytes(), before, "a refused request must not change the byte count");
        assert_eq!(in_flight.len(), 3, "a refused request must not change the entry count");
        assert_eq!(in_flight.rejected(), 1);
        // 被拒之后，先前登记的三条仍完整可用，取走时释放字节。
        let mut ids = Vec::new();
        while let Some(entry) = in_flight.pop_front() {
            ids.push(entry.id().to_string());
        }
        assert_eq!(ids, ["q-0", "q-1", "q-2"]);
        assert_eq!(in_flight.bytes(), 0);
        assert!(!in_flight.queued());
    }

    /// 第 33 条在导出边界（登记簿）被拒，前 32 条仍可逐条取回——与 App 队列（`Bridge`）无关。
    #[test]
    fn the_thirty_third_in_flight_entry_is_refused() {
        let mut in_flight = InFlight::default();
        for index in 0..MAX_PENDING {
            let id = format!("q-{index}");
            let raw = json!({"id": &id, "command": "file.newBin"}).to_string();
            let (_tx, rx) = channel();
            assert!(in_flight.try_push(&id, &raw, rx), "entry {index}");
        }
        assert_eq!(in_flight.len(), MAX_PENDING);
        assert_eq!(in_flight.rejected(), 0);
        let (_tx, rx) = channel();
        assert!(!in_flight.try_push("overflow", "{\"id\":\"overflow\"}", rx), "the 33rd entry must be refused");
        assert_eq!(in_flight.rejected(), 1);
        assert_eq!(in_flight.len(), MAX_PENDING);
        let mut ids = Vec::new();
        while let Some(entry) = in_flight.pop_front() {
            ids.push(entry.id().to_string());
        }
        assert_eq!(ids.len(), MAX_PENDING, "every accepted entry must stay takeable");
        assert_eq!(ids.first().map(String::as_str), Some("q-0"));
        assert_eq!(ids.last().map(String::as_str), Some("q-31"));
    }

    /// 空登记簿取不出东西：`craft_take_reply` 据此返回 `JsValue::NULL`（见 `api.rs`）。
    #[test]
    fn taking_from_an_empty_bookkeeping_yields_none() {
        let mut in_flight = InFlight::default();
        assert!(in_flight.front().is_none());
        assert!(in_flight.pop_front().is_none());
        assert!(in_flight.pop_back().is_none());
        assert!(!in_flight.queued());
        assert!(in_flight.is_empty());
        assert_eq!(in_flight.bytes(), 0);
    }

    /// 投递失败时的回滚：`pop_back` 释放刚登记的字节。
    #[test]
    fn a_rolled_back_entry_releases_its_bytes() {
        let mut in_flight = InFlight::default();
        let raw = "{\"id\":\"q-r\"}";
        let (_tx, rx) = channel();
        assert!(in_flight.try_push("q-r", raw, rx));
        assert_eq!(in_flight.bytes(), raw.len());
        assert!(in_flight.queued());
        assert_eq!(in_flight.pop_back().as_ref().map(|entry| entry.id()), Some("q-r"));
        assert_eq!(in_flight.bytes(), 0);
        assert!(!in_flight.queued());
    }

    /// 队首的回复通道可读，且取走后字节归零（`collect_bridge_replies` 的取数方式）。
    #[test]
    fn the_front_entry_exposes_its_reply_receiver() {
        let mut in_flight = InFlight::default();
        let raw = "{\"id\":\"q-v\"}";
        let (tx, rx) = channel();
        assert!(in_flight.try_push("q-v", raw, rx));
        tx.send(json!({"ok": true, "result": {"cuts": 2}})).expect("send reply");
        let front = in_flight.front().expect("the pushed entry is the front");
        assert_eq!(front.id(), "q-v");
        assert_eq!(front.bytes(), raw.len());
        assert_eq!(front.receiver().try_recv().expect("the reply is ready"), json!({"ok": true, "result": {"cuts": 2}}));
        assert_eq!(in_flight.pop_front().as_ref().map(|entry| entry.id()), Some("q-v"));
        assert_eq!(in_flight.bytes(), 0);
    }

    // ------------------------------------------------ 在途超时（InFlight::expire）

    /// 一条永不回复的请求超时后变成 `timeout` 错误信封；它被移出登记簿后，后面那条
    /// 已经回复的请求不再被队首堵住，可以直接取到回复。超时只回错误、不重放请求。
    #[test]
    fn a_hung_request_times_out_and_unblocks_the_next_reply() {
        let mut in_flight = InFlight::default();
        let started = Instant::now();
        let hung_raw = "{\"id\":\"hung\"}";
        let (_hung_tx, hung_rx) = channel();
        assert!(in_flight.try_push_at("hung", hung_raw, hung_rx, started), "the hung request is registered");

        // 正常请求晚 1 s 登记，而且已经回复；但队首是挂起的那条，FIFO 下暂时取不到。
        let normal_raw = "{\"id\":\"normal\"}";
        let (normal_tx, normal_rx) = channel();
        let normal_started = started + Duration::from_secs(1);
        assert!(in_flight.try_push_at("normal", normal_raw, normal_rx, normal_started));
        normal_tx.send(json!({"ok": true, "result": {"cuts": 2}})).expect("send reply");
        assert_eq!(in_flight.front().map(InFlightEntry::id), Some("hung"), "the hung request owns the front");

        // 15 s 后：只有挂起的那条过期；正常的那条只等了 14 s，仍在预算内。
        let expired = in_flight.expire(started + Duration::from_secs(15), Duration::from_secs(15));
        assert_eq!(expired.len(), 1, "only the hung request times out");
        let envelope: Value = serde_json::from_str(&expired[0]).expect("the timeout envelope is JSON");
        assert_eq!(envelope["id"], json!("hung"), "the timeout carries the original id");
        assert_eq!(envelope["ok"], json!(false));
        assert_eq!(envelope["error"]["code"], json!("timeout"));
        assert_eq!(envelope["error"]["message"], json!("the visible app did not answer within 15 s"));

        // 挂起的那条已被移出（没有重放），字节预算释放，正常的那条成为队首。
        assert_eq!(in_flight.len(), 1);
        assert_eq!(in_flight.bytes(), normal_raw.len());
        let front = in_flight.front().expect("the normal request is now first");
        assert_eq!(front.id(), "normal");
        assert_eq!(front.receiver().try_recv().expect("the waiting reply is reachable"), json!({"ok": true, "result": {"cuts": 2}}));
        assert_eq!(in_flight.pop_front().as_ref().map(InFlightEntry::id), Some("normal"));
        assert!(!in_flight.queued());

        // 再次 expire 不会重复产出同一个 id（不重放、不重复回答）。
        assert!(in_flight.expire(started + Duration::from_secs(60), Duration::from_secs(15)).is_empty());
    }

    /// 未到超时的不动；到点的才取出，且只取一次。
    #[test]
    fn expire_only_takes_entries_past_the_timeout() {
        let mut in_flight = InFlight::default();
        let started = Instant::now();
        for index in 0..2 {
            let id = format!("q-{index}");
            let (_tx, rx) = channel();
            assert!(in_flight.try_push_at(&id, "{\"id\":\"q\"}", rx, started));
        }
        assert!(in_flight.expire(started + Duration::from_secs(14), Duration::from_secs(15)).is_empty(), "nothing is due yet");
        assert_eq!(in_flight.len(), 2);
        let expired = in_flight.expire(started + Duration::from_secs(15), Duration::from_secs(15));
        assert_eq!(expired.len(), 2, "both entries are due at the same instant");
        assert!(expired[0].contains("\"q-0\""), "timeouts keep submission order: {expired:?}");
        assert!(expired[1].contains("\"q-1\""), "timeouts keep submission order: {expired:?}");
        assert!(in_flight.is_empty());
        assert_eq!(in_flight.bytes(), 0);
    }
}
