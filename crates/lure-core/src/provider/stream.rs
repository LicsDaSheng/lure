//! OpenAI-compatible SSE 流式解析与增量组装。
//!
//! `parse_sse_line` 把一条 `data: {...}` 行解析为 [`StreamChunk`]；`StreamAssembler`
//! 把有序增量折叠为最终 [`LlmResponse`]（内容/推理拼接，tool_calls 按 index 累积参数）。
//! `data: [DONE]`、注释行、空行返回 `None`。

use std::collections::BTreeMap;

use serde_json::Value;

use crate::provider::types::{LlmResponse, ProviderError, StreamChunk, ToolCall, ToolCallDelta};

/// 解析一条 SSE 行为 [`StreamChunk`]；非 `data:` 行 / `[DONE]` / 空返回 `None`。
pub fn parse_sse_line(line: &str) -> Result<Option<StreamChunk>, ProviderError> {
    let trimmed = line.trim();
    let Some(data) = trimmed.strip_prefix("data:") else {
        return Ok(None);
    };
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }

    let value: Value = serde_json::from_str(data)
        .map_err(|e| ProviderError::Response(format!("SSE chunk 不是合法 JSON: {e}")))?;
    let Some(choice) = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
    else {
        return Ok(Some(StreamChunk::default()));
    };

    let delta = choice.get("delta");
    let content_delta = delta
        .and_then(|d| d.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let reasoning_delta = delta
        .and_then(|d| d.get("reasoning_content"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let tool_call_deltas = parse_tool_call_deltas(delta);
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(Some(StreamChunk {
        content_delta,
        reasoning_delta,
        tool_call_deltas,
        finish_reason,
    }))
}

fn parse_tool_call_deltas(delta: Option<&Value>) -> Vec<ToolCallDelta> {
    delta
        .and_then(|d| d.get("tool_calls"))
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .map(|call| {
                    let function = call.get("function");
                    ToolCallDelta {
                        index: call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        id: call.get("id").and_then(Value::as_str).map(str::to_string),
                        name: function
                            .and_then(|f| f.get("name"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        arguments: function
                            .and_then(|f| f.get("arguments"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 把有序流式增量折叠为最终 [`LlmResponse`]。
#[derive(Default)]
pub struct StreamAssembler {
    content: String,
    reasoning: String,
    finish_reason: Option<String>,
    tool_calls: BTreeMap<usize, PartialToolCall>,
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl StreamAssembler {
    /// 新建空组装器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 累积一个增量。
    pub fn push(&mut self, chunk: &StreamChunk) {
        if let Some(content) = &chunk.content_delta {
            self.content.push_str(content);
        }
        if let Some(reasoning) = &chunk.reasoning_delta {
            self.reasoning.push_str(reasoning);
        }
        if let Some(reason) = &chunk.finish_reason {
            self.finish_reason = Some(reason.clone());
        }
        for delta in &chunk.tool_call_deltas {
            let entry = self.tool_calls.entry(delta.index).or_default();
            if let Some(id) = &delta.id {
                entry.id = id.clone();
            }
            if let Some(name) = &delta.name {
                entry.name = name.clone();
            }
            if let Some(arguments) = &delta.arguments {
                entry.arguments.push_str(arguments);
            }
        }
    }

    /// 收尾组装为 [`LlmResponse`]（无 finish_reason 时默认 `stop`）。
    pub fn finish(self) -> LlmResponse {
        let content = if self.content.is_empty() {
            None
        } else {
            Some(self.content)
        };
        let reasoning_content = if self.reasoning.is_empty() {
            None
        } else {
            Some(self.reasoning)
        };
        let tool_calls = self
            .tool_calls
            .into_values()
            .map(|p| ToolCall {
                id: p.id,
                name: p.name,
                arguments: p.arguments,
            })
            .collect();

        LlmResponse {
            content,
            reasoning_content,
            finish_reason: self.finish_reason.unwrap_or_else(|| "stop".to_string()),
            usage: serde_json::Map::new(),
            tool_calls,
        }
    }
}
