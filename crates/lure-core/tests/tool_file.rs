//! 文件工具的 workspace 边界与读写行为。
//!
//! 对齐 Phase 5 验收：文件工具不能越过允许 workspace；越界返回结构化错误结果。

use lure_core::tool::{ReadFileTool, Tool, WriteFileTool};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn write_then_read_within_workspace() {
    let dir = tempdir().unwrap();
    let workspace = dir.path();

    let writer = WriteFileTool::new(workspace);
    let write_result = writer.execute(&json!({"path": "notes/todo.txt", "content": "买菜"}));
    assert!(!write_result.is_error, "{}", write_result.content);
    assert!(workspace.join("notes/todo.txt").exists());

    let reader = ReadFileTool::new(workspace);
    let read_result = reader.execute(&json!({"path": "notes/todo.txt"}));
    assert!(!read_result.is_error);
    assert_eq!(read_result.content, "买菜");
}

#[tokio::test]
async fn read_outside_workspace_is_rejected() {
    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(dir.path().join("secret.txt"), "顶级机密").unwrap();

    let reader = ReadFileTool::new(&workspace);
    let result = reader.execute(&json!({"path": "../secret.txt"}));

    assert!(result.is_error);
    assert!(result.content.contains("outside allowed directory"));
    assert!(!result.content.contains("顶级机密"), "不得泄露越界文件内容");
}

#[tokio::test]
async fn write_outside_workspace_is_rejected() {
    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let writer = WriteFileTool::new(&workspace);
    let result = writer.execute(&json!({"path": "../escaped.txt", "content": "x"}));

    assert!(result.is_error);
    assert!(!dir.path().join("escaped.txt").exists(), "越界写入不得落盘");
}

#[tokio::test]
async fn missing_arguments_are_reported() {
    let dir = tempdir().unwrap();
    let reader = ReadFileTool::new(dir.path());
    let result = reader.execute(&json!({}));
    assert!(result.is_error);
    assert!(result.content.contains("path"));
}
