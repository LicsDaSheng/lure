use lure_core::LureEvent;
use serde_json::{Value, json};

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
        "message_end" => {
            if value.pointer("/message/role").and_then(Value::as_str) == Some("assistant") {
                let (text, thinking) = extract_assistant_content(&value["message"]["content"]);
                events.push(LureEvent::AssistantMessageCompleted { text, thinking });
            }
        }
        "tool_execution_start" => events.push(LureEvent::ToolStarted {
            tool_call_id: string_field(value, "toolCallId"),
            tool_name: string_field(value, "toolName"),
        }),
        "tool_execution_update" => events.push(LureEvent::ToolUpdated {
            tool_call_id: string_field(value, "toolCallId"),
            tool_name: string_field(value, "toolName"),
        }),
        "tool_execution_end" => events.push(LureEvent::ToolCompleted {
            tool_call_id: string_field(value, "toolCallId"),
            tool_name: string_field(value, "toolName"),
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
            message: value["errorMessage"].as_str().map(ToOwned::to_owned),
        }),
        "auto_retry_end" => events.push(LureEvent::RetryChanged {
            active: false,
            attempt: value["attempt"].as_u64(),
            max_attempts: None,
            message: value["finalError"].as_str().map(ToOwned::to_owned),
        }),
        "compaction_start" => events.push(LureEvent::CompactionChanged {
            active: true,
            reason: value["reason"].as_str().map(ToOwned::to_owned),
            aborted: None,
        }),
        "compaction_end" => events.push(LureEvent::CompactionChanged {
            active: false,
            reason: value["reason"].as_str().map(ToOwned::to_owned),
            aborted: value["aborted"].as_bool(),
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
            events: vec![LureEvent::ExtensionUiUnsupported {
                method,
                title: value["title"].as_str().map(ToOwned::to_owned),
            }],
            automatic_response: value["id"]
                .as_str()
                .map(|id| json!({"type":"extension_ui_response","id":id,"cancelled":true})),
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

fn extract_assistant_content(content: &Value) -> (String, String) {
    let mut text = String::new();
    let mut thinking = String::new();
    if let Some(parts) = content.as_array() {
        for part in parts {
            match part["type"].as_str() {
                Some("text") => text.push_str(part["text"].as_str().unwrap_or_default()),
                Some("thinking") => {
                    thinking.push_str(part["thinking"].as_str().unwrap_or_default());
                }
                _ => {}
            }
        }
    }
    (text, thinking)
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
    fn interactive_extension_requests_are_cancelled() {
        let frame = normalize_event(&json!({
            "type":"extension_ui_request",
            "id":"ui-1",
            "method":"confirm",
            "title":"确认",
            "message":"继续吗"
        }));
        assert!(matches!(
            frame.events.first(),
            Some(LureEvent::ExtensionUiUnsupported { method, .. }) if method == "confirm"
        ));
        assert_eq!(frame.automatic_response.unwrap()["cancelled"], true);
    }
}
