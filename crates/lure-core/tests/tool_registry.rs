//! 映射上游 `tests/tools/test_tool_registry.py` 与 schema 校验、结果截断的最小场景。

use lure_core::tool::{truncate_result, Tool, ToolError, ToolRegistry, ToolResult};
use serde_json::{json, Value};

struct FakeTool {
    name: String,
    schema: Value,
}

impl FakeTool {
    fn new(name: &str, schema: Value) -> Self {
        Self {
            name: name.to_string(),
            schema,
        }
    }
}

impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "fake tool"
    }
    fn parameters(&self) -> Value {
        self.schema.clone()
    }
    fn execute(&self, args: &Value) -> ToolResult {
        ToolResult::ok(args.to_string())
    }
}

fn obj_schema() -> Value {
    json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]})
}

#[test]
fn get_definitions_emits_openai_function_shape_in_registration_order() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FakeTool::new("read_file", obj_schema())));
    registry.register(Box::new(FakeTool::new("write_file", obj_schema())));

    let defs = registry.get_definitions();
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0]["type"], "function");
    assert_eq!(defs[0]["function"]["name"], "read_file");
    assert_eq!(defs[1]["function"]["name"], "write_file");
    assert_eq!(defs[0]["function"]["parameters"]["type"], "object");
}

#[test]
fn execute_dispatches_and_returns_result() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FakeTool::new("read_file", obj_schema())));

    let result = registry
        .execute("read_file", &json!({"path": "foo.txt"}))
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("foo.txt"));
}

#[test]
fn unknown_tool_suggests_near_miss() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FakeTool::new("read_file", obj_schema())));

    let err = registry
        .execute("readFile", &json!({"path": "foo.txt"}))
        .unwrap_err();
    assert_eq!(
        err,
        ToolError::UnknownTool {
            name: "readFile".to_string(),
            suggestion: Some("read_file".to_string()),
        }
    );
}

#[test]
fn invalid_args_are_rejected_with_structured_error() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FakeTool::new("read_file", obj_schema())));

    // 缺少 required path。
    let err = registry.execute("read_file", &json!({})).unwrap_err();
    assert!(matches!(err, ToolError::InvalidArgs { .. }));

    // path 类型错误。
    let err = registry
        .execute("read_file", &json!({"path": 123}))
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArgs { .. }));
}

#[test]
fn schema_validation_enforces_enum_and_numeric_bounds() {
    let schema = json!({
        "type": "object",
        "properties": {
            "mode": {"type": "string", "enum": ["read", "write"]},
            "limit": {"type": "integer", "minimum": 1, "maximum": 10}
        },
        "required": ["mode"]
    });
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FakeTool::new("op", schema)));

    assert!(registry
        .execute("op", &json!({"mode": "read", "limit": 5}))
        .is_ok());
    assert!(matches!(
        registry
            .execute("op", &json!({"mode": "delete"}))
            .unwrap_err(),
        ToolError::InvalidArgs { .. }
    ));
    assert!(matches!(
        registry
            .execute("op", &json!({"mode": "read", "limit": 99}))
            .unwrap_err(),
        ToolError::InvalidArgs { .. }
    ));
}

#[test]
fn truncate_result_appends_marker_only_when_over_limit() {
    assert_eq!(truncate_result("short", 100), "short");
    assert_eq!(truncate_result("", 0), "");
    let truncated = truncate_result("0123456789", 5);
    assert!(truncated.starts_with("01234"));
    assert!(truncated.contains("truncated"));
    assert!(truncated.len() > "0123456789".len());
}
