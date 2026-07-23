//! `MemoryStore`：memory 文件与 history.jsonl 的纯文件 I/O。
//!
//! 对齐上游 `nanobot/agent/memory.py::MemoryStore` 的核心：
//! - `MEMORY.md`（长期事实）、`SOUL.md`、`USER.md` 读写。
//! - `history.jsonl` append-only，自增 cursor，追加前经 `strip_think`。
//! - `read_unprocessed_history` 按 cursor 过滤；prompt 历史按 session 过滤。
//! - `.dream_cursor` 记录已整合进度。
//!
//! Phase 6 不做：GitStore 版本化、unified session 内部会话过滤、compact 的完整策略
//! （见 upstream-test-ledger）。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::memory::strip::strip_think;

/// history 单条最大字符数（应急上限）。
const HISTORY_ENTRY_HARD_CAP: usize = 64_000;

/// 一条 history 记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// 自增游标。
    pub cursor: u64,
    /// 时间戳（`YYYY-MM-DD HH:MM`）。
    pub timestamp: String,
    /// 清洗后的内容。
    pub content: String,
    /// 来源会话 key。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

/// memory 文件与 history 的存储。
pub struct MemoryStore {
    memory_file: PathBuf,
    soul_file: PathBuf,
    user_file: PathBuf,
    history_file: PathBuf,
    cursor_file: PathBuf,
    dream_cursor_file: PathBuf,
}

impl MemoryStore {
    /// 在 `workspace/memory` 下初始化存储目录。
    pub fn new(workspace: impl AsRef<Path>) -> std::io::Result<Self> {
        let workspace = workspace.as_ref();
        let memory_dir = workspace.join("memory");
        fs::create_dir_all(&memory_dir)?;
        let store = Self {
            memory_file: memory_dir.join("MEMORY.md"),
            soul_file: workspace.join("SOUL.md"),
            user_file: workspace.join("USER.md"),
            history_file: memory_dir.join("history.jsonl"),
            cursor_file: memory_dir.join(".cursor"),
            dream_cursor_file: memory_dir.join(".dream_cursor"),
        };
        store.maybe_migrate_legacy_history()?;
        Ok(store)
    }

    /// history.jsonl 路径。
    pub fn history_file(&self) -> &Path {
        &self.history_file
    }

    /// 读取文件文本，缺失返回空串。
    pub fn read_file(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    /// 读取 MEMORY.md。
    pub fn read_memory(&self) -> String {
        Self::read_file(&self.memory_file)
    }

    /// 写入 MEMORY.md。
    pub fn write_memory(&self, content: &str) {
        let _ = fs::write(&self.memory_file, content);
    }

    /// 读取 SOUL.md。
    pub fn read_soul(&self) -> String {
        Self::read_file(&self.soul_file)
    }

    /// 写入 SOUL.md。
    pub fn write_soul(&self, content: &str) {
        let _ = fs::write(&self.soul_file, content);
    }

    /// 读取 USER.md。
    pub fn read_user(&self) -> String {
        Self::read_file(&self.user_file)
    }

    /// 写入 USER.md。
    pub fn write_user(&self, content: &str) {
        let _ = fs::write(&self.user_file, content);
    }

    /// 构建注入上下文的长期记忆块；无内容返回空串。
    pub fn get_memory_context(&self) -> String {
        let long_term = self.read_memory();
        if long_term.is_empty() {
            String::new()
        } else {
            format!("## Long-term Memory\n{long_term}")
        }
    }

    /// 追加一条 history 并返回其自增 cursor。
    ///
    /// 追加前经 `strip_think`；清洗为空但原文非空时持久化空串（不回退到原始泄漏）。
    pub fn append_history(&self, entry: &str, session_key: Option<&str>) -> std::io::Result<u64> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let mut raw: String = entry.trim_end().to_string();
        if raw.chars().count() > HISTORY_ENTRY_HARD_CAP {
            raw = raw.chars().take(HISTORY_ENTRY_HARD_CAP).collect();
        }
        let content = strip_think(&raw);

        let cursor = self.next_cursor();
        let record = HistoryEntry {
            cursor,
            timestamp,
            content,
            session_key: session_key.map(str::to_string),
        };
        let line = serde_json::to_string(&record).expect("history 记录可序列化");

        let mut existing = Self::read_file(&self.history_file);
        existing.push_str(&line);
        existing.push('\n');
        fs::write(&self.history_file, existing)?;
        fs::write(&self.cursor_file, cursor.to_string())?;
        Ok(cursor)
    }

    /// 返回 cursor > `since_cursor` 的 history 条目。
    pub fn read_unprocessed_history(&self, since_cursor: u64) -> Vec<HistoryEntry> {
        self.valid_entries()
            .into_iter()
            .filter(|e| e.cursor > since_cursor)
            .collect()
    }

    /// 返回可安全注入 prompt 的近程 history（按 session 过滤）。
    ///
    /// `session_key` 为 `None` 时返回全部；否则仅保留同 session 的条目。
    pub fn read_recent_history_for_prompt(
        &self,
        since_cursor: u64,
        session_key: Option<&str>,
    ) -> Vec<HistoryEntry> {
        let entries = self.read_unprocessed_history(since_cursor);
        match session_key {
            None => entries,
            Some(key) => entries
                .into_iter()
                .filter(|e| e.session_key.as_deref() == Some(key))
                .collect(),
        }
    }

    /// 读取上次 dream 整合的 cursor（缺失或损坏为 0）。
    pub fn get_last_dream_cursor(&self) -> u64 {
        Self::read_file(&self.dream_cursor_file)
            .trim()
            .parse()
            .unwrap_or(0)
    }

    /// 写入 dream 整合 cursor。
    pub fn set_last_dream_cursor(&self, cursor: u64) {
        let _ = fs::write(&self.dream_cursor_file, cursor.to_string());
    }

    fn next_cursor(&self) -> u64 {
        let counter: u64 = Self::read_file(&self.cursor_file)
            .trim()
            .parse()
            .unwrap_or(0);
        let max_entry = self
            .valid_entries()
            .iter()
            .map(|e| e.cursor)
            .max()
            .unwrap_or(0);
        counter.max(max_entry) + 1
    }

    fn valid_entries(&self) -> Vec<HistoryEntry> {
        Self::read_file(&self.history_file)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| Self::parse_entry(&value))
            .collect()
    }

    /// 校验并解析一条记录；非法 cursor（负数/布尔/非整数）或缺字段则丢弃。
    fn parse_entry(value: &Value) -> Option<HistoryEntry> {
        let obj = value.as_object()?;
        let cursor_raw = obj.get("cursor")?;
        // 布尔在 JSON 中不是数字，负数/浮点 as_u64 为 None。
        let cursor = cursor_raw.as_u64()?;
        let timestamp = obj.get("timestamp")?.as_str()?.to_string();
        let content = obj.get("content")?.as_str()?.to_string();
        let session_key = match obj.get("session_key") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return None,
        };
        Some(HistoryEntry {
            cursor,
            timestamp,
            content,
            session_key,
        })
    }

    fn maybe_migrate_legacy_history(&self) -> std::io::Result<()> {
        let legacy_file = self.history_file.with_file_name("HISTORY.md");
        if !legacy_file.exists() {
            return Ok(());
        }
        if self.history_file.exists() && fs::metadata(&self.history_file)?.len() > 0 {
            return Ok(());
        }

        let bytes = fs::read(&legacy_file)?;
        let legacy_text = String::from_utf8_lossy(&bytes).into_owned();
        let entries = self.parse_legacy_history(&legacy_text, &legacy_file);
        if !entries.is_empty() {
            write_history_entries(&self.history_file, &entries)?;
            let last_cursor = entries.last().map(|entry| entry.cursor).unwrap_or(0);
            fs::write(&self.cursor_file, last_cursor.to_string())?;
            // 升级后默认视为已被 Dream 处理，避免首次启动重放整段历史。
            fs::write(&self.dream_cursor_file, last_cursor.to_string())?;
        }

        fs::rename(&legacy_file, next_legacy_backup_path(&legacy_file))?;
        Ok(())
    }

    fn parse_legacy_history(&self, text: &str, legacy_file: &Path) -> Vec<HistoryEntry> {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let normalized = normalized.trim();
        if normalized.is_empty() {
            return Vec::new();
        }

        let fallback_timestamp = legacy_fallback_timestamp(legacy_file);
        split_legacy_history_chunks(normalized)
            .into_iter()
            .enumerate()
            .map(|(idx, chunk)| {
                let (timestamp, content) = parse_legacy_timestamp(&chunk)
                    .unwrap_or_else(|| (fallback_timestamp.clone(), chunk));
                HistoryEntry {
                    cursor: (idx + 1) as u64,
                    timestamp,
                    content,
                    session_key: None,
                }
            })
            .collect()
    }
}

fn write_history_entries(path: &Path, entries: &[HistoryEntry]) -> std::io::Result<()> {
    let mut text = String::new();
    for entry in entries {
        let line = serde_json::to_string(&json!({
            "cursor": entry.cursor,
            "timestamp": entry.timestamp,
            "content": entry.content,
        }))?;
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(path, text)
}

fn split_legacy_history_chunks(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut saw_blank_separator = false;

    for line in text.split('\n') {
        if saw_blank_separator && !line.trim().is_empty() && !current.is_empty() {
            chunks.push(current.join("\n").trim().to_string());
            current = vec![line.to_string()];
            saw_blank_separator = false;
            continue;
        }
        if should_start_new_legacy_chunk(line, &current) {
            chunks.push(current.join("\n").trim().to_string());
            current = vec![line.to_string()];
            saw_blank_separator = false;
            continue;
        }
        current.push(line.to_string());
        saw_blank_separator = line.trim().is_empty();
    }

    if !current.is_empty() {
        chunks.push(current.join("\n").trim().to_string());
    }
    chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn should_start_new_legacy_chunk(line: &str, current: &[String]) -> bool {
    if current.is_empty() || !legacy_entry_start_re().is_match(line) {
        return false;
    }
    if is_raw_legacy_chunk(current) && legacy_raw_message_re().is_match(line) {
        return false;
    }
    true
}

fn is_raw_legacy_chunk(lines: &[String]) -> bool {
    let Some(first_nonempty) = lines.iter().find(|line| !line.trim().is_empty()) else {
        return false;
    };
    let Some(mat) = legacy_timestamp_re().find(first_nonempty) else {
        return false;
    };
    first_nonempty[mat.end()..]
        .trim_start()
        .starts_with("[RAW]")
}

fn parse_legacy_timestamp(chunk: &str) -> Option<(String, String)> {
    let regex = legacy_timestamp_re();
    let captures = regex.captures(chunk)?;
    let timestamp = captures.get(1)?.as_str().to_string();
    let mat = captures.get(0)?;
    let remainder = chunk[mat.end()..].trim_start();
    if remainder.is_empty() {
        None
    } else {
        Some((timestamp, remainder.to_string()))
    }
}

fn legacy_entry_start_re() -> Regex {
    Regex::new(r"^\[(\d{4}-\d{2}-\d{2}[^\]]*)\]\s*").expect("valid regex")
}

fn legacy_timestamp_re() -> Regex {
    Regex::new(r"^\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2})\]\s*").expect("valid regex")
}

fn legacy_raw_message_re() -> Regex {
    Regex::new(r"^\[\d{4}-\d{2}-\d{2}[^\]]*\]\s+[A-Z][A-Z0-9_]*(?:\s+\[tools:\s*[^\]]+\])?:")
        .expect("valid regex")
}

fn legacy_fallback_timestamp(legacy_file: &Path) -> String {
    fs::metadata(legacy_file)
        .and_then(|metadata| metadata.modified())
        .map(DateTime::<Local>::from)
        .unwrap_or_else(|_| Local::now())
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn next_legacy_backup_path(legacy_file: &Path) -> PathBuf {
    let memory_dir = legacy_file.parent().unwrap_or_else(|| Path::new("."));
    let first = memory_dir.join("HISTORY.md.bak");
    if !first.exists() {
        return first;
    }
    let mut suffix = 2;
    loop {
        let candidate = memory_dir.join(format!("HISTORY.md.bak.{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
        suffix += 1;
    }
}
