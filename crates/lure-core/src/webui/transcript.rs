//! WebUI transcript 存储：JSONL 格式持久化完整 user/assistant 消息对。
//!
//! 对齐上游 `nanobot/webui/transcript.py` 的写入语义（append-only JSONL +
//! webui-thread GET 返回 `WebuiThreadPersistedPayload`）。
//!
//! 首版不做 delta 级重放与分页；每轮 turn 完成时追加两条完整消息记录。

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};

/// 写入 field→str 的 JSONL 行。
fn write_jsonl_line(writer: &mut impl Write, record: &Value) -> io::Result<()> {
    let line = serde_json::to_string(record)?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// transcript 磁盘文件路径。
fn transcript_path(webui_dir: &Path, session_key: &str) -> PathBuf {
    let stem = URL_SAFE_NO_PAD.encode(session_key.as_bytes());
    webui_dir.join(format!("{stem}.jsonl"))
}

/// WebUI transcript 存储（workspace 下 `webui/` 目录）。
#[derive(Debug, Clone)]
pub struct TranscripStore {
    webui_dir: PathBuf,
}

impl TranscripStore {
    /// `webui_dir` 为 workspace 下的 `webui/` 目录（由调用方确保已创建）。
    pub fn new(webui_dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir: PathBuf = webui_dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { webui_dir: dir })
    }

    /// 所在 workspace 目录（`webui/` 的父目录），供需要按 workspace 定位其它子存储
    /// （如 `sessions/`）的调用方使用。根目录（无父）返回 `None`。
    pub fn workspace_dir(&self) -> Option<&Path> {
        self.webui_dir.parent()
    }

    /// 追加一轮对话（一条 user + 一条 assistant）。
    pub fn append_turn(
        &mut self,
        session_key: &str,
        user_content: &str,
        assistant_content: &str,
    ) -> io::Result<()> {
        let path = transcript_path(&self.webui_dir, session_key);
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        let now = Utc::now().to_rfc3339();
        write_jsonl_line(
            &mut file,
            &json!({"role": "user", "content": user_content, "timestamp": now}),
        )?;
        write_jsonl_line(
            &mut file,
            &json!({"role": "assistant", "content": assistant_content, "timestamp": now}),
        )?;
        Ok(())
    }

    /// 读取 session 的全部 transcript 消息，返回前端 `WebuiThreadPersistedPayload` 形状。
    ///
    /// 文件不存在或为空时返回 `Ok(None)`（前端按 null 处理）。
    pub fn read_thread(&self, session_key: &str) -> io::Result<Option<Value>> {
        let path = transcript_path(&self.webui_dir, session_key);
        if !path.exists() {
            return Ok(None);
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut messages: Vec<Value> = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(mut record) = serde_json::from_str::<Value>(&line) else {
                continue; // 损坏行跳过
            };
            // 对外只暴露 role + content，丢弃内部字段（timestamp 等）。
            let role = record["role"].take();
            let content = record["content"].take();
            if role.is_null() || content.is_null() {
                continue;
            }
            messages.push(json!({"role": role, "content": content}));
        }
        if messages.is_empty() {
            return Ok(None);
        }
        Ok(Some(json!({
            "schemaVersion": 1,
            "messages": messages,
        })))
    }

    /// 删除 session 的 transcript 文件。不存在返回 `Ok(false)`。
    pub fn delete(&self, session_key: &str) -> io::Result<bool> {
        let path = transcript_path(&self.webui_dir, session_key);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        Ok(true)
    }
}
