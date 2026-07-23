//! 工具结果与结果截断。
//!
//! 对齐上游 `ToolResult` 与工具结果字符截断：失败以结构化 `is_error` 暴露，不靠
//! 字符串控制流程；过长结果按字符截断并追加省略标记。

/// 一次工具调用的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// 结果文本。
    pub content: String,
    /// 是否为错误结果。
    pub is_error: bool,
}

impl ToolResult {
    /// 成功结果。
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    /// 错误结果。
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// 省略标记。
const TRUNCATION_MARKER: &str = "\n… (truncated)";

/// 若文本字符数超过 `max_chars`，截断到 `max_chars` 并追加省略标记。
///
/// `max_chars == 0` 表示不截断。
pub fn truncate_result(text: &str, max_chars: usize) -> String {
    if max_chars == 0 || text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}{TRUNCATION_MARKER}")
}
