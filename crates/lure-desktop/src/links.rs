//! 终端可见文本中的路径与 URL 识别。
//!
//! iTerm2、Terminal.app 等终端在按住修饰键悬停时识别屏幕上的路径与网址，
//! 该能力来自终端自身的文本扫描，不依赖程序输出 `OSC 8`。Pi 的 TUI 只对
//! Markdown 链接与图片回退发送 `OSC 8`，工具卡片里的文件路径是纯文本，
//! 因此需要在展示层补一条「仅识别路径/URL 形态」的通道。
//!
//! 本模块只做形态识别：不解析 Pi 的 Agent、工具、会话或 TUI 语义，也不
//! 拼接业务含义。识别对象是带网格坐标的字符流，因此软换行与宽字符都能
//! 精确映射回单元格。

use std::ops::Range;

/// 网格中的一个可见字符及其位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridChar {
    /// 逻辑行号：软换行延续的行沿用起始行号。
    pub row: u16,
    /// 字符起始列。
    pub column: u16,
    /// 字符占用的列数（宽字符为 2）。
    pub width: u16,
    pub ch: char,
}

/// 链接目标的类型，决定打开时的解析方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// 完整的 URI，可直接交给系统（OSC 8 的目标）。
    Uri,
    /// 文本中出现的完整 URL（`http`、`https`、`mailto`、`ftp`）。
    Url,
    /// 文件系统路径；相对路径需要基准目录才能解析。
    Path,
}

/// 一次识别结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedLink {
    /// 目标原文，未做百分号解码或 `~` 展开。
    pub target: String,
    pub kind: LinkKind,
    /// 在输入字符切片中的区间。
    pub range: Range<usize>,
    /// 可选的编辑器行号（`file.rs:42`）。
    pub line: Option<u32>,
}

/// URL scheme 白名单；只有带显式 scheme 的串会按 URL 处理。
const URL_SCHEMES: [&str; 4] = ["http://", "https://", "mailto:", "ftp://"];

/// 形如域名而非文件扩展名的尾段；用于避免把裸域名当成文件名。
const DOMAIN_SUFFIXES: [&str; 14] = [
    "com", "org", "net", "io", "dev", "ai", "app", "co", "me", "info", "edu", "gov", "cn", "sh",
];

/// 前导可能出现的标点，识别时剥离。
const LEADING_PUNCTUATION: [char; 8] = ['(', '[', '{', '<', '\'', '"', '`', '（'];

/// 全角标点：在中文正文里紧邻路径时是明确的分隔符。
const CJK_PUNCTUATION: [char; 30] = [
    '（', '）', '［', '］', '｛', '｝', '「', '」', '『', '』', '【', '】', '《', '》', '〈', '〉',
    '、', '，', '。', '：', '；', '！', '？', '…', '—', '～', '·', '“', '”', '‘',
];

/// 是否是 CJK 汉字（含扩展 A 与兼容区），中文正文中的普通文字。
fn is_cjk_ideograph(ch: char) -> bool {
    matches!(ch,
        '\u{3400}'..='\u{4dbf}'
        | '\u{4e00}'..='\u{9fff}'
        | '\u{f900}'..='\u{faff}'
    )
}

/// 尾部可能出现的标点，识别时剥离。
const TRAILING_PUNCTUATION: [char; 12] =
    [')', ']', '}', '>', ',', ';', '!', '?', '\'', '"', '`', '。'];

/// 从可见字符流中识别所有路径与 URL。
///
/// 输入应当是按显示顺序拼接的字符（软换行不插入分隔符），每个字符携带它
/// 在网格中的位置。返回结果按出现顺序排列，区间互不重叠。
#[must_use]
pub fn detect_links(chars: &[GridChar]) -> Vec<DetectedLink> {
    let mut links = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].ch.is_whitespace() || CJK_PUNCTUATION.contains(&chars[index].ch) {
            index += 1;
            continue;
        }
        let mut start = index;
        while index < chars.len() {
            let ch = chars[index].ch;
            if ch.is_whitespace() || CJK_PUNCTUATION.contains(&ch) {
                break;
            }
            // 路径起点必须是 ASCII：`读取crates/a.rs` 中的中文是正文而不是路径。
            // 一旦 token 已包含分隔符，就允许中文出现在路径内部（`/tmp/日志.md`）。
            if is_cjk_ideograph(ch) && !chars[start..index].iter().any(|c| c.ch == '/') {
                start = index;
                index += 1;
                continue;
            }
            index += 1;
        }
        // 标点剥离同时缩短两端：`(见 a/b.rs)。` 与 `` `a/b.rs` `` 都应命中中间部分。
        while start < index && LEADING_PUNCTUATION.contains(&chars[start].ch) {
            start += 1;
        }
        let mut end = index;
        while end > start && TRAILING_PUNCTUATION.contains(&chars[end - 1].ch) {
            end -= 1;
        }
        if end - start < 2 {
            continue;
        }
        let token: String = chars[start..end].iter().map(|c| c.ch).collect();
        if let Some(link) = classify(&token, start..end) {
            links.push(link);
        }
    }
    links
}

/// 判断单个 token 是否为可打开的目标。
fn classify(token: &str, range: Range<usize>) -> Option<DetectedLink> {
    let lower = token.to_ascii_lowercase();
    if URL_SCHEMES.iter().any(|scheme| lower.starts_with(scheme)) {
        return Some(DetectedLink {
            target: token.to_string(),
            kind: LinkKind::Url,
            range,
            line: None,
        });
    }
    // 其他显式 scheme（如 `javascript:`、`vscode:`）不参与识别，
    // 与 OSC 8 通道的白名单保持一致。
    if has_unknown_scheme(token) {
        return None;
    }
    split_line_suffix(token).and_then(|(path, line)| {
        is_path_like(path).then(|| DetectedLink {
            target: path.to_string(),
            kind: LinkKind::Path,
            range,
            line,
        })
    })
}

/// `token` 是否带有非白名单的显式 scheme。
///
/// 形如 `识别：` 的中文冒号不会命中；只有 `[a-zA-Z][a-zA-Z0-9+.-]*:` 前缀
/// 且冒号后不是路径分隔符时才认为是 scheme。
fn has_unknown_scheme(token: &str) -> bool {
    let Some(colon) = token.find(':') else {
        return false;
    };
    let (scheme, rest) = token.split_at(colon);
    if scheme.is_empty() || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return false;
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return false;
    }
    // `C:` 这类本地盘符在 Unix 上不是路径，统一按 scheme 处理并拒绝。
    let _ = rest;
    true
}

/// 剥离 `file.rs:42` 或 `file.rs:42:7` 结尾的行号，返回路径与行号。
fn split_line_suffix(token: &str) -> Option<(&str, Option<u32>)> {
    let mut parts = token.rsplitn(3, ':');
    let last = parts.next()?;
    let Some(middle) = parts.next() else {
        return Some((token, None));
    };
    if !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()) {
        // 可能是 `path:42` 或 `path:42:7`（列号忽略）。
        if let Ok(line) = last.parse::<u32>() {
            if middle.is_empty() {
                return None;
            }
            if middle.chars().all(|c| c.is_ascii_digit()) {
                // `path:42:7`：最后一段是列号，middle 才是行号。
                return parts
                    .next()
                    .and_then(|path| middle.parse::<u32>().ok().map(|line| (path, Some(line))));
            }
            return Some((middle, Some(line)));
        }
    }
    Some((token, None))
}

/// 形态判断：绝对路径、`~/` 路径、含分隔符的相对路径，或带扩展名的文件名。
fn is_path_like(path: &str) -> bool {
    if path.starts_with('/') {
        // 排除单独的 `/` 与 `//`（多为注释或分隔线）。
        return path.len() > 1 && !path.trim_matches('/').is_empty();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return !rest.is_empty();
    }
    if path.contains('/') {
        // 相对路径要求末段带扩展名（`src/main.rs`），或至少三段（`a/b/c`）。
        // 借此排除 `image/png` 这类 MIME 类型与 `1/2` 这类比式表达式。
        let mut segments = path.split('/');
        let last = segments.next_back().unwrap_or_default();
        if has_extension(last) {
            return true;
        }
        // 段数统计含空段（`a//b`），因此用非空段判断更稳。
        return path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .count()
            >= 3
            && last.chars().any(|c| c.is_ascii_alphanumeric());
    }
    has_extension(path)
}

/// 是否形如 `name.ext`；扩展名是字母且不是常见域名尾段。
fn has_extension(name: &str) -> bool {
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    let (stem, ext) = name.split_at(dot);
    let ext = &ext[1..];
    if stem.is_empty() || stem.contains('.') {
        return false;
    }
    if stem.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if !(1..=10).contains(&ext.len()) || !ext.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    !DOMAIN_SUFFIXES.contains(&ext.to_ascii_lowercase().as_str())
}

/// 把识别结果映射回网格区间。
///
/// 一段链接可能跨越软换行，因此按行拆成多个片段，`target` 相同。
#[must_use]
pub fn map_to_cells(chars: &[GridChar], link: &DetectedLink) -> Vec<(u16, u16, u16)> {
    let mut spans: Vec<(u16, u16, u16)> = Vec::new();
    for character in &chars[link.range.clone()] {
        match spans.last_mut() {
            Some((row, column, columns))
                if *row == character.row && column.saturating_add(*columns) == character.column =>
            {
                *columns = columns.saturating_add(character.width);
            }
            _ => spans.push((character.row, character.column, character.width)),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(text: &str) -> Vec<GridChar> {
        text.chars()
            .enumerate()
            .map(|(index, ch)| GridChar {
                row: 0,
                column: u16::try_from(index).unwrap(),
                width: 1,
                ch,
            })
            .collect()
    }

    fn targets(text: &str) -> Vec<(String, LinkKind)> {
        detect_links(&stream(text))
            .into_iter()
            .map(|link| (link.target, link.kind))
            .collect()
    }

    fn path(target: &str) -> (String, LinkKind) {
        (target.to_string(), LinkKind::Path)
    }

    fn url(target: &str) -> (String, LinkKind) {
        (target.to_string(), LinkKind::Url)
    }

    #[test]
    fn recognises_absolute_and_home_paths() {
        assert_eq!(
            targets("在 /Users/me/project/src/main.rs 中"),
            [path("/Users/me/project/src/main.rs")]
        );
        assert_eq!(
            targets("见 ~/.pi/projects.yaml"),
            [path("~/.pi/projects.yaml")]
        );
    }

    #[test]
    fn recognises_relative_paths_and_bare_filenames() {
        assert_eq!(
            targets("read crates/lure-desktop/src/opener.rs"),
            [path("crates/lure-desktop/src/opener.rs")]
        );
        assert_eq!(targets("见 Cargo.toml"), [path("Cargo.toml")]);
        assert_eq!(targets("见 README.md"), [path("README.md")]);
    }

    #[test]
    fn chinese_punctuation_separates_paths_without_stalling() {
        assert_eq!(
            targets("已读取 crates/lure-desktop/src/opener.rs（约 300 行）。"),
            [path("crates/lure-desktop/src/opener.rs")]
        );
        assert_eq!(targets("（）（）【】；，。"), []);
    }

    #[test]
    fn recognises_urls_only_with_explicit_allowed_scheme() {
        assert_eq!(
            targets("打开 https://example.dev/a?b=1 看看"),
            [url("https://example.dev/a?b=1")]
        );
        assert_eq!(targets("mailto:a@b.dev"), [url("mailto:a@b.dev")]);
        // 裸域名不识别，避免与文件名混淆。
        assert!(targets("example.dev").is_empty());
        assert!(targets("www.example.com").is_empty());
    }

    #[test]
    fn rejects_version_numbers_measurements_and_plain_words() {
        for text in [
            "pi 1.5.2 版本",
            "14s",
            "56 tokens",
            "Working",
            "read",
            "✓",
            "[click to jump]",
            "2s",
            "100%",
            "a",
            "1/2",
            "image/png",
            "text/plain",
            "application/json",
            "and/or",
        ] {
            assert!(targets(text).is_empty(), "{text:?} 不应识别为链接");
        }
    }

    #[test]
    fn rejects_non_whitelisted_schemes() {
        for text in [
            "javascript:alert(1)",
            "vscode://file/etc/hosts",
            "ssh://host/path",
            "C:/Windows",
        ] {
            assert!(targets(text).is_empty(), "{text:?} 不应识别为链接");
        }
    }

    #[test]
    fn strips_trailing_punctuation_and_keeps_interior_characters() {
        assert_eq!(
            targets("(见 crates/a.rs)。"),
            [path("crates/a.rs")],
            "中文句号也应剥离"
        );
        assert_eq!(targets("参考 /tmp/a.rs, 然后"), [path("/tmp/a.rs")]);
        assert_eq!(targets("路径是 `notes/plan.md`"), [path("notes/plan.md")]);
    }

    #[test]
    fn captures_editor_line_numbers() {
        let links = detect_links(&stream("crates/a.rs:42 出错了"));
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "crates/a.rs");
        assert_eq!(links[0].line, Some(42));

        let links = detect_links(&stream("crates/a.rs:42:7 出错了"));
        assert_eq!(links[0].target, "crates/a.rs");
        assert_eq!(links[0].line, Some(42));

        // 没有路径的 `:42` 不应产生链接。
        assert!(targets(":42").is_empty());
    }

    #[test]
    fn wide_characters_map_to_two_columns_and_soft_wraps_share_a_link() {
        // 中文占两列：`见` 在第 0 列、`crates/a.rs` 从第 2 列开始。
        let mut chars = vec![GridChar {
            row: 0,
            column: 0,
            width: 2,
            ch: '见',
        }];
        chars.extend(
            " crates/a.rs"
                .chars()
                .enumerate()
                .map(|(index, ch)| GridChar {
                    row: 0,
                    column: u16::try_from(index + 2).unwrap(),
                    width: 1,
                    ch,
                }),
        );
        let links = detect_links(&chars);
        assert_eq!(links[0].target, "crates/a.rs");
        let spans = map_to_cells(&chars, &links[0]);
        assert_eq!(spans, [(0, 3, 11)]);

        // 软换行：同一链接拆成两个网格片段，目标保持一致。
        let mut wrapped = Vec::new();
        for (index, ch) in "crates/a".chars().enumerate() {
            wrapped.push(GridChar {
                row: 3,
                column: u16::try_from(index).unwrap(),
                width: 1,
                ch,
            });
        }
        for (index, ch) in ".rs".chars().enumerate() {
            wrapped.push(GridChar {
                row: 4,
                column: u16::try_from(index).unwrap(),
                width: 1,
                ch,
            });
        }
        let links = detect_links(&wrapped);
        assert_eq!(links[0].target, "crates/a.rs");
        assert_eq!(map_to_cells(&wrapped, &links[0]), [(3, 0, 8), (4, 0, 3)]);
    }

    #[test]
    fn multiple_links_on_one_line_keep_their_ranges() {
        let chars = stream("cp a/b.rs /tmp/c.rs");
        let links = detect_links(&chars);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "a/b.rs");
        assert_eq!(links[1].target, "/tmp/c.rs");
        assert_eq!(map_to_cells(&chars, &links[0]), [(0, 3, 6)]);
        assert_eq!(map_to_cells(&chars, &links[1]), [(0, 10, 9)]);
    }
}
