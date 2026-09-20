use lure_core::{LureEvent, MessageBlock, MessageBlockKind};
use serde_json::Value;

pub(crate) struct NormalizedFrame {
    pub events: Vec<LureEvent>,
    pub automatic_response: Option<Value>,
}

pub(crate) fn normalize_event(value: &Value) -> NormalizedFrame {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut events = Vec::new();
    let mut automatic_response = None;

    match event_type {
        "agent_start" => events.push(LureEvent::RunStarted),
        "message_start" => {
            if value.pointer("/message/role").and_then(Value::as_str) == Some("assistant") {
                events.push(LureEvent::AssistantMessageStarted);
            }
        }
        "message_update" => events.extend(normalize_stream_update(value)),
        "message_end" => events.extend(normalize_assistant_end(value)),
        "tool_execution_start" => events.push(LureEvent::ToolStarted {
            tool_call_id: string_field(value, "toolCallId"),
            tool_name: string_field(value, "toolName"),
            input: pretty_json(&value["args"]),
        }),
        "tool_execution_update" => events.push(LureEvent::ToolUpdated {
            tool_call_id: string_field(value, "toolCallId"),
            tool_name: string_field(value, "toolName"),
            input: pretty_json(&value["args"]),
            output: extract_result_text(&value["partialResult"]),
            truncated_lines: extract_truncated_lines(&value["partialResult"]),
        }),
        "tool_execution_end" => events.push(LureEvent::ToolCompleted {
            tool_call_id: string_field(value, "toolCallId"),
            tool_name: string_field(value, "toolName"),
            input: pretty_json(&value["args"]),
            output: extract_result_text(&value["result"]),
            truncated_lines: extract_truncated_lines(&value["result"]),
            is_error: value["isError"].as_bool().unwrap_or(false),
        }),
        "agent_end" => events.push(LureEvent::RunFinished {
            will_retry: value["willRetry"].as_bool().unwrap_or(false),
        }),
        "agent_settled" => events.push(LureEvent::RunSettled),
        "auto_retry_start" => events.push(LureEvent::RetryChanged {
            active: true,
            attempt: value["attempt"].as_u64(),
            max_attempts: value["maxAttempts"].as_u64(),
            delay_ms: value["delayMs"].as_u64(),
            message: value["errorMessage"].as_str().map(ToOwned::to_owned),
        }),
        "auto_retry_end" => events.push(LureEvent::RetryChanged {
            active: false,
            attempt: value["attempt"].as_u64(),
            max_attempts: None,
            delay_ms: None,
            message: value["finalError"].as_str().map(ToOwned::to_owned),
        }),
        "compaction_start" => events.push(LureEvent::CompactionChanged {
            active: true,
            reason: value["reason"].as_str().map(ToOwned::to_owned),
            aborted: None,
            summary: None,
            tokens_before: None,
            error_message: None,
        }),
        "compaction_end" => events.push(LureEvent::CompactionChanged {
            active: false,
            reason: value["reason"].as_str().map(ToOwned::to_owned),
            aborted: value["aborted"].as_bool(),
            summary: value
                .pointer("/result/summary")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            tokens_before: value
                .pointer("/result/tokensBefore")
                .and_then(Value::as_u64),
            error_message: value["errorMessage"].as_str().map(ToOwned::to_owned),
        }),
        "extension_ui_request" => {
            let extension = normalize_extension_request(value);
            events.extend(extension.events);
            automatic_response = extension.automatic_response;
        }
        _ => {}
    }

    NormalizedFrame {
        events,
        automatic_response,
    }
}

fn normalize_assistant_end(value: &Value) -> Option<LureEvent> {
    if value.pointer("/message/role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let (text, thinking, blocks) = extract_assistant_content(&value["message"]["content"]);
    Some(LureEvent::AssistantMessageCompleted {
        text,
        thinking,
        blocks,
        stop_reason: value["message"]["stopReason"]
            .as_str()
            .map(ToOwned::to_owned),
        error_message: value["message"]["errorMessage"]
            .as_str()
            .map(ToOwned::to_owned),
    })
}

fn normalize_stream_update(value: &Value) -> Option<LureEvent> {
    let update = &value["assistantMessageEvent"];
    let content_index = update["contentIndex"].as_u64().unwrap_or_default();
    let delta = update["delta"].as_str().unwrap_or_default().to_owned();
    match update["type"].as_str() {
        Some("text_delta") => Some(LureEvent::AssistantTextDelta {
            content_index,
            delta,
        }),
        Some("thinking_delta") => Some(LureEvent::AssistantThinkingDelta {
            content_index,
            delta,
        }),
        _ => None,
    }
}

fn normalize_extension_request(value: &Value) -> NormalizedFrame {
    let method = string_field(value, "method");
    match method.as_str() {
        "select" | "confirm" | "input" | "editor" => NormalizedFrame {
            events: vec![LureEvent::ExtensionUiRequested {
                request_id: string_field(value, "id"),
                method,
                title: value["title"].as_str().map(ToOwned::to_owned),
                message: value["message"].as_str().map(ToOwned::to_owned),
                options: value["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|option| option.as_str().or_else(|| option["label"].as_str()))
                    .map(ToOwned::to_owned)
                    .collect(),
                placeholder: value["placeholder"].as_str().map(ToOwned::to_owned),
                default_value: value["defaultValue"].as_str().map(ToOwned::to_owned),
            }],
            automatic_response: None,
        },
        "notify" => NormalizedFrame {
            events: vec![LureEvent::Notification {
                level: value["notifyType"].as_str().unwrap_or("info").to_owned(),
                message: string_field(value, "message"),
            }],
            automatic_response: None,
        },
        _ => NormalizedFrame {
            events: Vec::new(),
            automatic_response: None,
        },
    }
}

fn extract_assistant_content(content: &Value) -> (String, String, Vec<MessageBlock>) {
    let mut text = String::new();
    let mut thinking = String::new();
    let mut blocks = Vec::new();
    if let Some(parts) = content.as_array() {
        for (content_index, part) in parts.iter().enumerate() {
            match part["type"].as_str() {
                Some("text") => {
                    let value = part["text"].as_str().unwrap_or_default();
                    text.push_str(value);
                    blocks.push(MessageBlock {
                        content_index: content_index as u64,
                        kind: MessageBlockKind::Text,
                        text: value.to_owned(),
                    });
                }
                Some("thinking") => {
                    let value = part["thinking"].as_str().unwrap_or_default();
                    thinking.push_str(value);
                    blocks.push(MessageBlock {
                        content_index: content_index as u64,
                        kind: MessageBlockKind::Thinking,
                        text: value.to_owned(),
                    });
                }
                _ => {}
            }
        }
    }
    (text, thinking, blocks)
}

fn pretty_json(value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn extract_result_text(result: &Value) -> String {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_truncated_lines(result: &Value) -> Option<u64> {
    let truncation = result.pointer("/details/truncation")?;
    if !truncation["truncated"].as_bool().unwrap_or(false) {
        return None;
    }
    let total = truncation["totalLines"].as_u64()?;
    let shown = truncation["outputLines"].as_u64()?;
    Some(total.saturating_sub(shown))
}

fn string_field(value: &Value, field: &str) -> String {
    value[field].as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::normalize_event;
    use lure_core::LureEvent;
    use serde_json::json;

    #[test]
    fn unknown_events_are_ignored() {
        let frame = normalize_event(&json!({"type":"future_event","data":1}));
        assert!(frame.events.is_empty());
        assert!(frame.automatic_response.is_none());
    }

    #[test]
    fn tool_events_preserve_input_output_and_truncation_metadata() {
        let started = normalize_event(&json!({
            "type":"tool_execution_start",
            "toolCallId":"tool-1",
            "toolName":"bash",
            "args":{"command":"printf hello"}
        }));
        assert!(matches!(
            started.events.first(),
            Some(LureEvent::ToolStarted { input, .. }) if input.contains("printf hello")
        ));

        let ended = normalize_event(&json!({
            "type":"tool_execution_end",
            "toolCallId":"tool-1",
            "toolName":"bash",
            "result":{
                "content":[{"type":"text","text":"hello"}],
                "details":{"truncation":{"truncated":true,"totalLines":30,"outputLines":20}}
            },
            "isError":false
        }));
        assert!(matches!(
            ended.events.first(),
            Some(LureEvent::ToolCompleted { output, truncated_lines: Some(10), .. }) if output == "hello"
        ));
    }

    #[test]
    fn completed_assistant_and_compaction_keep_display_metadata() {
        let message = normalize_event(&json!({
            "type":"message_end",
            "message":{
                "role":"assistant",
                "content":[
                    {"type":"thinking","thinking":"first"},
                    {"type":"text","text":"answer"},
                    {"type":"thinking","thinking":"second"}
                ],
                "stopReason":"length",
                "errorMessage":null
            }
        }));
        assert!(matches!(
            message.events.first(),
            Some(LureEvent::AssistantMessageCompleted { blocks, stop_reason: Some(reason), .. })
                if blocks.len() == 3 && reason == "length"
        ));

        let compaction = normalize_event(&json!({
            "type":"compaction_end",
            "reason":"threshold",
            "result":{"summary":"condensed", "tokensBefore":1200},
            "aborted":false
        }));
        assert!(matches!(
            compaction.events.first(),
            Some(LureEvent::CompactionChanged { summary: Some(summary), tokens_before: Some(1200), .. })
                if summary == "condensed"
        ));
    }

    #[test]
    fn interactive_extension_requests_are_forwarded_to_the_desktop() {
        let frame = normalize_event(&json!({
            "type":"extension_ui_request",
            "id":"ui-1",
            "method":"select",
            "title":"选择范围",
            "message":"请选择",
            "options":["当前文件", "整个项目"]
        }));
        assert!(matches!(
            frame.events.first(),
            Some(LureEvent::ExtensionUiRequested { request_id, method, options, .. })
                if request_id == "ui-1" && method == "select" && options.len() == 2
        ));
        assert!(frame.automatic_response.is_none());
    }
}
