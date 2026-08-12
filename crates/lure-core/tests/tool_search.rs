//! 搜索工具：`list_dir`（目录列举）与 `grep`（内容搜索）。
//!
//! 对齐上游 `tests/tools/test_filesystem_tools.py::TestListDirTool` 与
//! `tests/tools/test_search_tools.py`（GrepTool 部分）。

use std::fs::{self, File};
use std::time::{Duration, SystemTime};

use lure_core::tool::{GrepTool, ListDirTool, Tool};
use serde_json::json;
use tempfile::tempdir;

fn set_mtime(path: &std::path::Path, secs: u64) {
    let f = File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

// --- list_dir ---

fn populate(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.py"), "pass").unwrap();
    fs::write(root.join("src/utils.py"), "pass").unwrap();
    fs::write(root.join("README.md"), "hi").unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "x").unwrap();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
}

#[tokio::test]
async fn list_dir_basic_ignores_noise_dirs() {
    let dir = tempdir().unwrap();
    populate(dir.path());
    let tool = ListDirTool::new(dir.path());

    let result = tool.execute(&json!({"path": "."}));
    assert!(!result.is_error, "{}", result.content);
    assert!(result.content.contains("README.md"));
    assert!(result.content.contains("src"));
    assert!(!result.content.contains(".git"));
    assert!(!result.content.contains("node_modules"));
}

#[tokio::test]
async fn list_dir_recursive_shows_nested_and_skips_ignored() {
    let dir = tempdir().unwrap();
    populate(dir.path());
    let tool = ListDirTool::new(dir.path());

    let result = tool.execute(&json!({"path": ".", "recursive": true}));
    let normalized = result.content.replace('\\', "/");
    assert!(normalized.contains("src/main.py"), "{normalized}");
    assert!(normalized.contains("src/utils.py"));
    assert!(!normalized.contains(".git"));
    assert!(!normalized.contains("node_modules"));
}

#[tokio::test]
async fn list_dir_truncates_at_max_entries() {
    let dir = tempdir().unwrap();
    for i in 0..10 {
        fs::write(dir.path().join(format!("file_{i}.txt")), "x").unwrap();
    }
    let tool = ListDirTool::new(dir.path());

    let result = tool.execute(&json!({"path": ".", "max_entries": 3}));
    assert!(result.content.contains("truncated"), "{}", result.content);
    assert!(result.content.contains("3 of 10"), "{}", result.content);
}

#[tokio::test]
async fn list_dir_empty_dir_reports_empty() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let tool = ListDirTool::new(dir.path());

    let result = tool.execute(&json!({"path": "empty"}));
    assert!(
        result.content.to_lowercase().contains("empty"),
        "{}",
        result.content
    );
}

#[tokio::test]
async fn list_dir_not_found_returns_error() {
    let dir = tempdir().unwrap();
    let tool = ListDirTool::new(dir.path());
    let result = tool.execute(&json!({"path": "nope"}));
    assert!(result.is_error);
    assert!(result.content.contains("Error"));
    assert!(result.content.contains("not found"));
}

#[tokio::test]
async fn list_dir_missing_path_clear_error() {
    let dir = tempdir().unwrap();
    let tool = ListDirTool::new(dir.path());
    let result = tool.execute(&json!({}));
    assert_eq!(result.content, "Error listing directory: Unknown path");
}

// --- grep ---

#[tokio::test]
async fn grep_defaults_to_files_with_matches() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.py"), "match_here\n").unwrap();
    let tool = GrepTool::new(dir.path());

    let result = tool.execute(&json!({"pattern": "match_here", "path": "src"}));
    assert_eq!(
        result.content.lines().collect::<Vec<_>>(),
        vec!["src/main.py"]
    );
    assert!(!result.content.contains("1|"));
}

#[tokio::test]
async fn grep_files_with_matches_sorted_by_mtime_desc() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let a = dir.path().join("src/a.py");
    let b = dir.path().join("src/b.py");
    fs::write(&a, "needle\nneedle\n").unwrap();
    fs::write(&b, "needle\n").unwrap();
    set_mtime(&a, 1);
    set_mtime(&b, 2);
    let tool = GrepTool::new(dir.path());

    let result = tool
        .execute(&json!({"pattern": "needle", "path": "src", "output_mode": "files_with_matches"}));
    assert_eq!(
        result.content.lines().collect::<Vec<_>>(),
        vec!["src/b.py", "src/a.py"]
    );
}

#[tokio::test]
async fn grep_content_mode_with_context() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/main.py"),
        "alpha\nbeta\nmatch_here\ngamma\n",
    )
    .unwrap();
    fs::write(dir.path().join("README.md"), "match_here\n").unwrap();
    let tool = GrepTool::new(dir.path());

    let result = tool.execute(&json!({
        "pattern": "match_here", "path": ".", "glob": "*.py",
        "output_mode": "content", "context_before": 1, "context_after": 1,
    }));
    let c = &result.content;
    assert!(c.contains("src/main.py:3"), "{c}");
    assert!(c.contains("  2| beta"), "{c}");
    assert!(c.contains("> 3| match_here"), "{c}");
    assert!(c.contains("  4| gamma"), "{c}");
    assert!(!c.contains("README.md"), "{c}");
}

#[tokio::test]
async fn grep_case_insensitive() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("memory")).unwrap();
    fs::write(
        dir.path().join("memory/HISTORY.md"),
        "[2026-04-02 10:00] OAuth token rotated\n",
    )
    .unwrap();
    let tool = GrepTool::new(dir.path());

    let result = tool.execute(&json!({
        "pattern": "oauth", "path": "memory/HISTORY.md",
        "case_insensitive": true, "output_mode": "content",
    }));
    assert!(
        result.content.contains("memory/HISTORY.md:1"),
        "{}",
        result.content
    );
    assert!(result.content.contains("OAuth token rotated"));
}

#[tokio::test]
async fn grep_fixed_strings_treats_regex_chars_literally() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("memory")).unwrap();
    fs::write(
        dir.path().join("memory/HISTORY.md"),
        "[2026-04-02 10:00] OAuth token rotated\n",
    )
    .unwrap();
    let tool = GrepTool::new(dir.path());

    let result = tool.execute(&json!({
        "pattern": "[2026-04-02 10:00]", "path": "memory/HISTORY.md",
        "fixed_strings": true, "output_mode": "content",
    }));
    assert!(
        result.content.contains("memory/HISTORY.md:1"),
        "{}",
        result.content
    );
    assert!(result
        .content
        .contains("[2026-04-02 10:00] OAuth token rotated"));
}

#[tokio::test]
async fn grep_type_filter_limits_files() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.py"), "needle\n").unwrap();
    fs::write(dir.path().join("src/b.md"), "needle\n").unwrap();
    let tool = GrepTool::new(dir.path());

    let result = tool.execute(&json!({"pattern": "needle", "path": "src", "type": "py"}));
    assert_eq!(result.content.lines().collect::<Vec<_>>(), vec!["src/a.py"]);
}

#[tokio::test]
async fn grep_files_with_matches_pagination() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    for name in ["a.py", "b.py", "c.py"] {
        fs::write(dir.path().join("src").join(name), "needle\n").unwrap();
    }
    let tool = GrepTool::new(dir.path());

    let result = tool.execute(&json!({
        "pattern": "needle", "path": "src", "head_limit": 1, "offset": 1,
    }));
    assert!(
        result.content.contains("pagination: limit=1, offset=1"),
        "{}",
        result.content
    );
    let file_lines: Vec<_> = result
        .content
        .lines()
        .filter(|l| l.starts_with("src/"))
        .collect();
    assert_eq!(file_lines.len(), 1);
}

#[tokio::test]
async fn grep_no_matches_message() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.py"), "nothing\n").unwrap();
    let tool = GrepTool::new(dir.path());
    let result = tool.execute(&json!({"pattern": "needle", "path": "."}));
    assert!(
        result.content.contains("No matches found"),
        "{}",
        result.content
    );
}
