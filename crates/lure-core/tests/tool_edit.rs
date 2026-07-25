//! `edit_file` 工具：精确 / 行 trim 回退匹配、CRLF 保留、replace_all、歧义与错误。
//!
//! 对齐上游 `tests/tools/test_filesystem_tools.py`（`TestFindMatch` + `TestEditFileTool`）。

use lure_core::tool::{find_match, EditFileTool, Tool};
use serde_json::json;
use tempfile::tempdir;

// --- find_match 契约 ---

#[test]
fn find_match_exact() {
    let (m, count) = find_match("hello world", "world");
    assert_eq!(m.as_deref(), Some("world"));
    assert_eq!(count, 1);
}

#[test]
fn find_match_exact_no_match() {
    let (m, count) = find_match("hello world", "xyz");
    assert_eq!(m, None);
    assert_eq!(count, 0);
}

#[test]
fn find_match_line_trim_fallback_returns_original_indent() {
    let (m, count) = find_match("    def foo():\n        pass\n", "def foo():\n    pass");
    assert_eq!(count, 1);
    assert!(m.as_deref().unwrap().contains("    def foo():"));
}

#[test]
fn find_match_line_trim_multiple_candidates() {
    let (_m, count) = find_match("  a\n  b\n  a\n  b\n", "a\nb");
    assert_eq!(count, 2);
}

#[test]
fn find_match_empty_old_text() {
    let (m, _count) = find_match("hello", "");
    assert_eq!(m.as_deref(), Some(""));
}

// --- EditFileTool 行为 ---

fn edit(tool: &EditFileTool, args: serde_json::Value) -> lure_core::tool::ToolResult {
    tool.execute(&args)
}

#[test]
fn edit_exact_match_replaces_and_reports_success() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("a.py"), "hello world").unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(
        &tool,
        json!({"path": "a.py", "old_text": "world", "new_text": "earth"}),
    );
    assert!(!result.is_error, "{}", result.content);
    assert!(result.content.contains("Successfully"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.py")).unwrap(),
        "hello earth"
    );
}

#[test]
fn edit_preserves_crlf_line_endings() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("crlf.py"), b"line1\r\nline2\r\nline3").unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(
        &tool,
        json!({"path": "crlf.py", "old_text": "line1\nline2", "new_text": "LINE1\nLINE2"}),
    );
    assert!(!result.is_error, "{}", result.content);
    let raw = std::fs::read(dir.path().join("crlf.py")).unwrap();
    assert!(raw.windows(5).any(|w| w == b"LINE1"));
    assert!(raw.windows(2).any(|w| w == b"\r\n"), "CRLF 应保留");
}

#[test]
fn edit_line_trim_fallback_replaces_indented_block() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("indent.py"),
        "    def foo():\n        pass\n",
    )
    .unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(
        &tool,
        json!({"path": "indent.py", "old_text": "def foo():\n    pass", "new_text": "def bar():\n    return 1"}),
    );
    assert!(!result.is_error, "{}", result.content);
    assert!(std::fs::read_to_string(dir.path().join("indent.py"))
        .unwrap()
        .contains("bar"));
}

#[test]
fn edit_ambiguous_match_warns_without_replace_all() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("dup.py"), "aaa\nbbb\naaa\nbbb\n").unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(
        &tool,
        json!({"path": "dup.py", "old_text": "aaa\nbbb", "new_text": "xxx"}),
    );
    let lower = result.content.to_lowercase();
    assert!(
        lower.contains("appears") || lower.contains("warning"),
        "{}",
        result.content
    );
    // 未写入：内容不变。
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dup.py")).unwrap(),
        "aaa\nbbb\naaa\nbbb\n"
    );
}

#[test]
fn edit_replace_all_replaces_every_occurrence() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("multi.py"), "foo bar foo bar foo").unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(
        &tool,
        json!({"path": "multi.py", "old_text": "foo", "new_text": "baz", "replace_all": true}),
    );
    assert!(!result.is_error, "{}", result.content);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("multi.py")).unwrap(),
        "baz bar baz bar baz"
    );
}

#[test]
fn edit_not_found_returns_error() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("nf.py"), "hello").unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(
        &tool,
        json!({"path": "nf.py", "old_text": "xyz", "new_text": "abc"}),
    );
    assert!(result.is_error);
    assert!(result.content.contains("Error"));
    assert!(result.content.contains("not found"));
}

#[test]
fn edit_missing_new_text_returns_clear_error() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("a.py"), "hello").unwrap();
    let tool = EditFileTool::new(dir.path());

    let result = edit(&tool, json!({"path": "a.py", "old_text": "hello"}));
    assert_eq!(result.content, "Error editing file: Unknown new_text");
}

#[test]
fn edit_outside_workspace_is_rejected() {
    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(dir.path().join("secret.txt"), "机密").unwrap();
    let tool = EditFileTool::new(&workspace);

    let result = edit(
        &tool,
        json!({"path": "../secret.txt", "old_text": "机密", "new_text": "x"}),
    );
    assert!(result.is_error);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("secret.txt")).unwrap(),
        "机密",
        "越界文件不得被改写"
    );
}
