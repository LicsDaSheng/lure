//! 最小 context builder。
//!
//! 对齐上游 `nanobot/agent/context.py` 的职责边界：把可选 system prompt 与会话
//! 历史投影为 provider 输入消息序列。
//!
//! Phase 3 做 `{role, content}` 投影；Phase 6 增加 memory 注入。注入顺序为
//! system → memory → 历史。上游的 runtime context 块、富历史处理留待后续。

use serde_json::{json, Value};

/// 最小 context builder。
#[derive(Debug, Clone, Default)]
pub struct ContextBuilder {
    system_prompt: Option<String>,
    memory_context: Option<String>,
}

impl ContextBuilder {
    /// 新建 context builder，可选 system prompt。
    pub fn new(system_prompt: Option<String>) -> Self {
        Self {
            system_prompt,
            memory_context: None,
        }
    }

    /// 设置注入的长期记忆块（空串视为无注入）。
    pub fn with_memory(mut self, memory_context: Option<String>) -> Self {
        self.memory_context = memory_context.filter(|m| !m.is_empty());
        self
    }

    /// 由历史构建 provider 输入消息：system → memory → 历史的 `{role, content}` 投影。
    pub fn build(&self, history: &[Value]) -> Vec<Value> {
        let mut out = Vec::with_capacity(history.len() + 2);
        if let Some(system) = &self.system_prompt {
            out.push(json!({"role": "system", "content": system}));
        }
        if let Some(memory) = &self.memory_context {
            out.push(json!({"role": "system", "content": memory}));
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
