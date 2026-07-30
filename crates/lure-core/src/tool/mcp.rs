//! MCP（Model Context Protocol）工具接入的纯变换核心。
//!
//! 对齐上游 `nanobot/agent/tools/mcp.py` 的纯函数：把 MCP 服务器暴露的工具名与 JSON Schema
//! 归一为模型 API（OpenAI-compatible）可消费的形状，并检测畸形 JSON-RPC 进度通知。
//!
//! 本模块只覆盖确定性可测的纯变换。真实 MCP 连接/会话/传输（stdio/HTTP/SSE）、enabled-tools
//! 过滤、资源/提示包装、重连/瞬时重试依赖 MCP 客户端 SDK + 异步运行时，lure 同步模型待引入
//! 异步运行时与 MCP 客户端库后回补（见 upstream-test-ledger）。

use serde_json::{Map, Value};
use sha1::{Digest, Sha1};

/// 工具名最大长度（对齐上游 `_MAX_TOOL_NAME_LENGTH`）。
const MAX_TOOL_NAME_LENGTH: usize = 64;
/// 限长时保留的 hash 后缀长度（对齐上游 `_HASH_LENGTH`）。
const HASH_LENGTH: usize = 8;

/// 净化 MCP 派生名以兼容模型 API：非 `[a-zA-Z0-9_-]` 替换为 `_`，再折叠连续 `_`。
///
/// 对齐上游 `_sanitize_name`。
pub fn sanitize_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    collapse_underscores(&replaced)
}

/// 折叠连续下划线为单个（对齐上游 `_SANITIZE_RE = r"_+"`）。
fn collapse_underscores(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for c in s.chars() {
        if c == '_' {
            if !prev_underscore {
                out.push('_');
            }
            prev_underscore = true;
        } else {
            out.push(c);
            prev_underscore = false;
        }
    }
    out
}

/// 限长工具名，短名保持不变；超长则 `<prefix>_<8 位 sha1>`。
///
/// 对齐上游 `_limit_tool_name`（sha1 hexdigest 前 8 位）。
pub fn limit_tool_name(name: &str) -> String {
    if name.len() <= MAX_TOOL_NAME_LENGTH {
        return name.to_string();
    }
    let mut hasher = Sha1::new();
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest
        .iter()
        .take(HASH_LENGTH / 2)
        .map(|b| format!("{b:02x}"))
        .collect();
    let prefix_length = MAX_TOOL_NAME_LENGTH - HASH_LENGTH - 1;
    format!("{}_{hex}", &name[..prefix_length])
}

/// 净化并限长 MCP 派生工具名（对齐上游 `_sanitize_mcp_tool_name`）。
pub fn sanitize_mcp_tool_name(name: &str) -> String {
    limit_tool_name(&sanitize_name(name))
}

/// 归一 nullable JSON Schema（仅处理 nullable 模式），供工具定义暴露给 OpenAI-compatible API。
///
/// 对齐上游 `_normalize_schema_for_openai`：`type: [T, "null"]` → `{type: T, nullable: true}`；
/// `oneOf`/`anyOf` 单非空分支 + null → 合并该分支 + `nullable: true`；递归 properties/items；
/// object 补 properties/required 默认值。非 object schema 回落 `{type: object, properties: {}}`。
pub fn normalize_schema_for_openai(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return json_object([
            ("type", Value::from("object")),
            ("properties", empty_object()),
        ]);
    };

    let mut normalized = obj.clone();

    // type 为列表且含 "null" 且余一项 → 拍平 type + nullable。
    if let Some(Value::Array(types)) = normalized.get("type") {
        let non_null: Vec<&Value> = types
            .iter()
            .filter(|t| t.as_str() != Some("null"))
            .collect();
        let has_null = types.iter().any(|t| t.as_str() == Some("null"));
        if has_null && non_null.len() == 1 {
            let single = non_null[0].clone();
            normalized.insert("type".to_string(), single);
            normalized.insert("nullable".to_string(), Value::Bool(true));
        }
    }

    // oneOf/anyOf 的 nullable 分支合并。
    for key in ["oneOf", "anyOf"] {
        if let Some((branch, _)) = extract_nullable_branch(normalized.get(key)) {
            let mut merged: Map<String, Value> = normalized
                .iter()
                .filter(|(k, _)| k.as_str() != key)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if let Some(branch_obj) = branch.as_object() {
                for (k, v) in branch_obj {
                    merged.insert(k.clone(), v.clone());
                }
            }
            merged.insert("nullable".to_string(), Value::Bool(true));
            normalized = merged;
            break;
        }
    }

    // 递归 properties。
    if let Some(Value::Object(props)) = normalized.get("properties").cloned() {
        let recursed: Map<String, Value> = props
            .into_iter()
            .map(|(name, prop)| {
                if prop.is_object() {
                    (name, normalize_schema_for_openai(&prop))
                } else {
                    (name, prop)
                }
            })
            .collect();
        normalized.insert("properties".to_string(), Value::Object(recursed));
    }

    // 递归 items。
    if let Some(items) = normalized.get("items").cloned() {
        if items.is_object() {
            normalized.insert("items".to_string(), normalize_schema_for_openai(&items));
        }
    }

    if normalized.get("type").and_then(Value::as_str) != Some("object") {
        return Value::Object(normalized);
    }

    normalized
        .entry("properties".to_string())
        .or_insert_with(empty_object);
    normalized
        .entry("required".to_string())
        .or_insert_with(|| Value::Array(vec![]));
    Value::Object(normalized)
}

/// 返回 nullable 联合的唯一非空分支（对齐上游 `_extract_nullable_branch`）。
///
/// 输入为 `[{...}, {"type":"null"}]` 形式的列表；恰好一个非空分支 + 至少一个 null 时返回该分支。
pub fn extract_nullable_branch(options: Option<&Value>) -> Option<(Value, bool)> {
    let arr = options?.as_array()?;
    let mut non_null: Vec<&Value> = Vec::new();
    let mut saw_null = false;
    for option in arr {
        let obj = option.as_object()?;
        if obj.get("type").and_then(Value::as_str) == Some("null") {
            saw_null = true;
        } else {
            non_null.push(option);
        }
    }
    if saw_null && non_null.len() == 1 {
        Some((non_null[0].clone(), true))
    } else {
        None
    }
}

/// 检测畸形 JSON-RPC 进度通知：method 为 `notifications/progress` 但 params 缺 `progressToken`。
///
/// 对齐上游 `_is_malformed_mcp_progress_notification`（JSON 形态）。
pub fn is_malformed_progress_notification(message: &Value) -> bool {
    if message.get("method").and_then(Value::as_str) != Some("notifications/progress") {
        return false;
    }
    let has_token = message
        .get("params")
        .and_then(Value::as_object)
        .is_some_and(|p| p.contains_key("progressToken"));
    !has_token
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

fn json_object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    Value::Object(map)
}
