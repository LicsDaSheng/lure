//! MCP 工具接入的纯变换核心测试。
//!
//! 对照上游 `nanobot/agent/tools/mcp.py` 的纯函数：`_sanitize_name` / `_limit_tool_name` /
//! `_sanitize_mcp_tool_name`（模型 API 兼容的工具名净化与限长）、`_normalize_schema_for_openai`
//! /`_extract_nullable_branch`（nullable JSON Schema 归一），以及 JSON-RPC 畸形进度通知检测。
//!
//! 暂缓：真实 MCP 连接/会话/传输（stdio/HTTP/SSE）、enabled-tools 过滤、资源/提示包装——
//! 依赖 MCP SDK 客户端 + asyncio，lure 同步模型待引入异步运行时与 MCP 客户端库后回补。

use lure_core::tool::mcp::{
    is_malformed_progress_notification, limit_tool_name, normalize_schema_for_openai,
    sanitize_mcp_tool_name, sanitize_name,
};
use serde_json::json;

// —— 名称净化 —— //

#[test]
fn sanitize_replaces_illegal_chars() {
    assert_eq!(sanitize_name("foo bar"), "foo_bar");
    assert_eq!(sanitize_name("a@b#c"), "a_b_c");
    // 合法字符（字母数字、下划线、连字符）保留。
    assert_eq!(sanitize_name("foo-bar_1"), "foo-bar_1");
}

#[test]
fn sanitize_collapses_repeated_underscores() {
    // 多个非法字符相邻折叠为单个下划线。
    assert_eq!(sanitize_name("a  b"), "a_b");
    assert_eq!(sanitize_name("x@@@y"), "x_y");
    assert_eq!(sanitize_name("a_b__c"), "a_b_c");
}

#[test]
fn limit_keeps_short_names_unchanged() {
    assert_eq!(limit_tool_name("short_name"), "short_name");
    let exactly_64 = "a".repeat(64);
    assert_eq!(limit_tool_name(&exactly_64), exactly_64);
}

#[test]
fn limit_truncates_long_names_with_hash_suffix() {
    let long = "a".repeat(100);
    let limited = limit_tool_name(&long);
    assert_eq!(limited.chars().count(), 64, "限长至 64");
    // 形如 <prefix>_<8 位 hash>。
    let (prefix, digest) = limited.rsplit_once('_').unwrap();
    assert_eq!(digest.len(), 8);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(prefix.len(), 64 - 8 - 1);
    assert!(long.starts_with(prefix));
    // 确定性：同输入同输出。
    assert_eq!(limit_tool_name(&long), limited);
}

#[test]
fn sanitize_mcp_tool_name_combines_sanitize_and_limit() {
    let name = format!("mcp test {}", "z".repeat(100));
    let result = sanitize_mcp_tool_name(&name);
    assert!(result.chars().count() <= 64);
    assert!(!result.contains(' '));
    assert!(result.starts_with("mcp_test_"));
}

// —— schema 归一 —— //

#[test]
fn preserves_non_nullable_unions() {
    let schema = json!({
        "type": "object",
        "properties": {
            "value": {"anyOf": [{"type": "string"}, {"type": "integer"}]}
        }
    });
    let out = normalize_schema_for_openai(&schema);
    assert_eq!(
        out["properties"]["value"]["anyOf"],
        json!([{"type": "string"}, {"type": "integer"}])
    );
}

#[test]
fn normalizes_nullable_type_union() {
    let schema = json!({
        "type": "object",
        "properties": {"name": {"type": ["string", "null"]}}
    });
    let out = normalize_schema_for_openai(&schema);
    assert_eq!(
        out["properties"]["name"],
        json!({"type": "string", "nullable": true})
    );
}

#[test]
fn normalizes_nullable_anyof_merging_branch() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "anyOf": [{"type": "string"}, {"type": "null"}],
                "description": "optional name"
            }
        }
    });
    let out = normalize_schema_for_openai(&schema);
    assert_eq!(
        out["properties"]["name"],
        json!({"type": "string", "description": "optional name", "nullable": true})
    );
}

#[test]
fn non_object_schema_falls_back() {
    let out = normalize_schema_for_openai(&json!("not a schema"));
    assert_eq!(out, json!({"type": "object", "properties": {}}));
}

#[test]
fn object_gets_properties_and_required_defaults() {
    let out = normalize_schema_for_openai(&json!({"type": "object"}));
    assert_eq!(out["properties"], json!({}));
    assert_eq!(out["required"], json!([]));
}

#[test]
fn recurses_into_items() {
    let schema = json!({
        "type": "array",
        "items": {"type": ["string", "null"]}
    });
    let out = normalize_schema_for_openai(&schema);
    assert_eq!(out["items"], json!({"type": "string", "nullable": true}));
}

// —— JSON-RPC 畸形进度通知检测 —— //

#[test]
fn malformed_progress_without_token_is_detected() {
    let msg = json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {"progress": 0.5}
    });
    assert!(is_malformed_progress_notification(&msg));
}

#[test]
fn progress_with_token_is_valid() {
    let msg = json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {"progressToken": "abc", "progress": 0.5}
    });
    assert!(!is_malformed_progress_notification(&msg));
}

#[test]
fn non_progress_method_is_not_malformed() {
    let msg = json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {}
    });
    assert!(!is_malformed_progress_notification(&msg));
}
