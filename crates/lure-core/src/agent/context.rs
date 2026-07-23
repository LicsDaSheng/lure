//! 最小 context builder。
//!
//! 对齐上游 `nanobot/agent/context.py` 的职责边界：把可选 system prompt 与会话
//! 历史投影为 provider 输入消息序列。
//!
//! Phase 3 只做 `{role, content}` 投影；上游的 runtime context 块、memory 注入、
//! 富历史处理留待 Phase 6 与 context builder 专项（见 upstream-test-ledger）。

use serde_json::{json, Value};

/// 最小 context builder。
#[derive(Debug, Clone, Default)]
pub struct ContextBuilder {
    system_prompt: Option<String>,
}

impl ContextBuilder {
    /// 新建 context builder，可选 system prompt。
    pub fn new(system_prompt: Option<String>) -> Self {
        Self { system_prompt }
    }

    /// 由历史构建 provider 输入消息：可选 system 行 + 历史的 `{role, content}` 投影。
    pub fn build(&self, history: &[Value]) -> Vec<Value> {
        let mut out = Vec::with_capacity(history.len() + 1);
        if let Some(system) = &self.system_prompt {
            out.push(json!({"role": "system", "content": system}));
        }
        out.extend(history.iter().map(project_message));
        out
    }
}

/// 只保留 `role` 与 `content`，丢弃 timestamp 等内部字段。
fn project_message(message: &Value) -> Value {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = message.get("content").cloned().unwrap_or_else(|| json!(""));
    json!({"role": role, "content": content})
}
