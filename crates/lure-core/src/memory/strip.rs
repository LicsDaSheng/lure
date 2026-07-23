//! `strip_think`：移除思考块与模板级泄漏。
//!
//! 对齐上游 `nanobot/utils/helpers.py::strip_think` 的核心子集：
//! - 完整 `<think|thinking|thought>...</...>` 块。
//! - 开头未闭合的 `<think...` 前缀（到结尾）。
//! - 开头的 harmony channel 标记 `<channel|>` / `<|channel|>`。
//! - 缺 `>` 的畸形开标签（后续字符不能构成合法标签名）。
//!
//! 在持久化到 history 前应用，避免模板泄漏经由回放/整合重新污染上下文。
//! 上游还覆盖 self-closing、孤立闭标签、流式截断尾部等，这里留待需要时补齐。

use std::sync::OnceLock;

use regex::Regex;

struct Patterns {
    well_formed: Regex,
    unclosed_prefix: Regex,
    channel_marker: Regex,
    malformed_open: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        well_formed: Regex::new(r"(?s)<(think|thinking|thought)>.*?</(think|thinking|thought)>")
            .expect("valid regex"),
        unclosed_prefix: Regex::new(r"(?s)^\s*<(think|thinking|thought)>.*$").expect("valid regex"),
        channel_marker: Regex::new(r"^\s*<\|?channel\|>\s*").expect("valid regex"),
        malformed_open: Regex::new(r"<(think|thinking|thought)([^A-Za-z0-9_\-:>/]|$)")
            .expect("valid regex"),
    })
}

/// 移除思考块与模板级泄漏，随后去除尾部空白。
pub fn strip_think(text: &str) -> String {
    let p = patterns();
    let mut out = p.well_formed.replace_all(text, "").into_owned();
    out = p.unclosed_prefix.replace(&out, "").into_owned();
    out = p.channel_marker.replace(&out, "").into_owned();
    // 畸形开标签：保留触发的后一个字符（若非标签结束）。
    out = p.malformed_open.replace_all(&out, "$2").into_owned();
    out.trim_end().to_string()
}
