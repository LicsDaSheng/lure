//! 会话内存模型 `Session`。
//!
//! 对齐上游 `nanobot/session/manager.py` 的 `Session` dataclass：
//! - `last_consolidated` 越界或类型损坏时重置为 0（避免隐藏全部历史）。
//! - `get_history` 返回未整合消息（`messages[last_consolidated..]`）。
//!
//! Phase 2 的 `get_history` 仅做按条数切片；上游的富回放逻辑（media 面包屑、
//! cli_apps、token 预算、tool 边界对齐）属 context builder 关注点，留待 Phase 3/6。

use chrono::{DateTime, Local};
use serde_json::{Map, Value};

/// 单文件历史默认最大消息条数。
pub const FILE_MAX_MESSAGES: usize = 2000;

/// 一个会话。
#[derive(Debug, Clone)]
pub struct Session {
    /// 会话 key，形如 `channel:chat_id`。
    pub key: String,
    /// 全量消息（任意 JSON 对象）。
    pub messages: Vec<Value>,
    /// 会话级 metadata。
    pub metadata: Map<String, Value>,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 最近更新时间。
    pub updated_at: DateTime<Local>,
    /// 已整合到文件的消息数（clamp 后不变量：`0 <= last_consolidated <= messages.len()`）。
    last_consolidated: usize,
}

impl Session {
    /// 新建空会话，时间戳取当前。
    pub fn new(key: impl Into<String>) -> Self {
        let now = Local::now();
        Self {
            key: key.into(),
            messages: Vec::new(),
            metadata: Map::new(),
            created_at: now,
            updated_at: now,
            last_consolidated: 0,
        }
    }

    /// 从已加载数据构造，并对 `last_consolidated` 做 clamp。
    pub fn from_loaded(
        key: impl Into<String>,
        messages: Vec<Value>,
        metadata: Map<String, Value>,
        created_at: DateTime<Local>,
        updated_at: DateTime<Local>,
        last_consolidated: &Value,
    ) -> Self {
        let clamped = clamp_offset(last_consolidated, messages.len());
        Self {
            key: key.into(),
            messages,
            metadata,
            created_at,
            updated_at,
            last_consolidated: clamped,
        }
    }

    /// 以消息和原始 offset 构造（测试与内存构造用），对 offset 做 clamp。
    pub fn with_messages(
        key: impl Into<String>,
        messages: Vec<Value>,
        last_consolidated: &Value,
    ) -> Self {
        let now = Local::now();
        Self::from_loaded(key, messages, Map::new(), now, now, last_consolidated)
    }

    /// clamp 后的已整合偏移。
    pub fn last_consolidated(&self) -> usize {
        self.last_consolidated
    }

    /// 追加一条消息并刷新更新时间。
    pub fn add_message(&mut self, role: &str, content: &str) {
        let mut msg = Map::new();
        msg.insert("role".to_string(), Value::String(role.to_string()));
        msg.insert("content".to_string(), Value::String(content.to_string()));
        msg.insert(
            "timestamp".to_string(),
            Value::String(Local::now().to_rfc3339()),
        );
        self.messages.push(Value::Object(msg));
        self.updated_at = Local::now();
    }

    /// 返回未整合消息的近程窗口。
    ///
    /// `max_messages == 0` 时使用 [`FILE_MAX_MESSAGES`]；按条数从尾部切片。
    pub fn get_history(&self, max_messages: usize) -> Vec<Value> {
        let unconsolidated = &self.messages[self.last_consolidated..];
        let max = if max_messages == 0 {
            FILE_MAX_MESSAGES
        } else {
            max_messages
        };
        let start = unconsolidated.len().saturating_sub(max);
        unconsolidated[start..].to_vec()
    }

    /// 清空会话并重置状态。
    pub fn clear(&mut self) {
        self.messages.clear();
        self.last_consolidated = 0;
        self.updated_at = Local::now();
        self.metadata.remove("_last_summary");
    }
}

/// 只有落在 `[0, len]` 内的非负整数才是合法偏移；其余（越界/负数/浮点/布尔/字符串/空）重置为 0。
fn clamp_offset(raw: &Value, len: usize) -> usize {
    match raw.as_u64() {
        Some(value) if value <= len as u64 => value as usize,
        _ => 0,
    }
}
