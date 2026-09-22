use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::error::SessionError;
use crate::layout::{normalized_cwd, session_directory};
use crate::summary::{SessionPage, SessionSummary};
use crate::time::iso8601_to_epoch_ms;

/// 同时扫描的会话文件上限，避免一次性打开过多文件描述符。
const MAX_CONCURRENT_SCANS: usize = 10;
/// 单次最多向桌面端返回的摘要数量，防止异常参数重新造成全量传输。
const MAX_PAGE_SIZE: usize = 100;

/// 列出某个工作目录下的 Pi 会话，按最近活动时间倒序。
///
/// 优先读取该工作目录对应的默认会话目录；目录不存在时退回扫描全部项目目录，
/// 并按会话头部声明的 `cwd` 过滤。目录本身不存在时返回空列表。
///
/// # Errors
///
/// 会话目录无法读取时返回错误。
pub async fn list_sessions(
    cwd: &Path,
    agent_dir: &Path,
) -> Result<Vec<SessionSummary>, SessionError> {
    let directory = session_directory(cwd, agent_dir);
    if is_directory(&directory).await {
        return collect_summaries(&session_files(&directory).await?, None).await;
    }

    let projects_root = agent_dir.join("sessions");
    if !is_directory(&projects_root).await {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(&projects_root).await.map_err(|error| {
        SessionError::DirectoryUnreadable {
            path: projects_root.clone(),
            message: error.to_string(),
        }
    })?;
    while let Some(entry) =
        entries
            .next_entry()
            .await
            .map_err(|error| SessionError::DirectoryUnreadable {
                path: projects_root.clone(),
                message: error.to_string(),
            })?
    {
        let path = entry.path();
        if is_directory(&path).await {
            files.extend(session_files(&path).await?);
        }
    }

    collect_summaries(&files, Some(cwd)).await
}

/// 分页列出某个工作目录下的 Pi 会话。
///
/// 返回值最多包含 `limit` 条记录，并通过 `has_more` 告知调用方是否可继续读取。
/// `limit` 为零时返回空页，避免调用方意外拉取全量数据。
///
/// # Errors
///
/// 会话目录无法读取时返回错误。
pub async fn list_sessions_page(
    cwd: &Path,
    agent_dir: &Path,
    offset: usize,
    limit: usize,
) -> Result<SessionPage, SessionError> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Ok(SessionPage {
            sessions: Vec::new(),
            has_more: false,
        });
    }

    let sessions = list_sessions(cwd, agent_dir).await?;
    let has_more = sessions.len() > offset.saturating_add(limit);
    let sessions = sessions.into_iter().skip(offset).take(limit).collect();
    Ok(SessionPage { sessions, has_more })
}

/// 读取单个会话文件的摘要；文件缺失或不是有效会话文件时返回 `None`。
///
/// # Errors
///
/// 文件存在但无法读取时返回错误。
pub async fn read_session_summary(path: &Path) -> Result<Option<SessionSummary>, SessionError> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SessionError::FileUnreadable {
                path: path.to_path_buf(),
                message: error.to_string(),
            });
        }
    };

    let fallback_ms = file_modified_ms(path).await;
    let mut lines = BufReader::new(file).lines();
    let mut header: Option<Header> = None;
    let mut name: Option<String> = None;
    let mut message_count = 0_usize;
    let mut first_message: Option<String> = None;
    let mut last_activity_ms: Option<i64> = None;
    let mut saw_entry = false;

    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(error) => {
                return Err(SessionError::FileUnreadable {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                });
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        if !saw_entry {
            saw_entry = true;
            let Some(parsed) = parse_header(&entry) else {
                return Ok(None);
            };
            header = Some(parsed);
            continue;
        }

        match entry["type"].as_str() {
            Some("session_info") => {
                name = entry["name"]
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned);
            }
            Some("message") => {
                message_count += 1;
                let message = &entry["message"];
                let role = message["role"].as_str().unwrap_or_default();
                if !matches!(role, "user" | "assistant") {
                    continue;
                }
                if let Some(activity) = message_activity_ms(message, &entry) {
                    last_activity_ms =
                        Some(last_activity_ms.map_or(activity, |max| max.max(activity)));
                }
                if role == "user" && first_message.is_none() {
                    first_message = message_text(message).filter(|text| !text.is_empty());
                }
            }
            _ => {}
        }
    }

    let Some(header) = header else {
        return Ok(None);
    };

    let created_at_ms = header.created_at_ms.or(fallback_ms).unwrap_or_default();
    Ok(Some(SessionSummary {
        path: path.to_path_buf(),
        id: header.id,
        cwd: header.cwd,
        name,
        parent_session_path: header.parent_session_path,
        created_at_ms,
        modified_at_ms: last_activity_ms
            .or(header.created_at_ms)
            .or(fallback_ms)
            .unwrap_or_default(),
        message_count,
        first_message,
    }))
}

#[derive(Debug)]
struct Header {
    id: String,
    cwd: Option<String>,
    parent_session_path: Option<String>,
    created_at_ms: Option<i64>,
}

fn parse_header(entry: &Value) -> Option<Header> {
    if entry["type"].as_str() != Some("session") {
        return None;
    }
    let id = entry["id"].as_str().filter(|value| !value.is_empty())?;
    Some(Header {
        id: id.to_owned(),
        cwd: entry["cwd"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        parent_session_path: entry["parentSession"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        created_at_ms: entry["timestamp"].as_str().and_then(iso8601_to_epoch_ms),
    })
}

fn message_text(message: &Value) -> Option<String> {
    match &message["content"] {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => {
            let texts: Vec<&str> = blocks
                .iter()
                .filter(|block| block["type"].as_str() == Some("text"))
                .filter_map(|block| block["text"].as_str())
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join(" "))
            }
        }
        _ => None,
    }
}

fn message_activity_ms(message: &Value, entry: &Value) -> Option<i64> {
    if let Some(timestamp) = message["timestamp"].as_i64() {
        return Some(timestamp);
    }
    entry["timestamp"].as_str().and_then(iso8601_to_epoch_ms)
}

async fn collect_summaries(
    files: &[PathBuf],
    required_cwd: Option<&Path>,
) -> Result<Vec<SessionSummary>, SessionError> {
    if files.is_empty() {
        return Ok(Vec::new());
    }

    let expected_cwd = required_cwd.and_then(|path| normalized_cwd(&path.to_string_lossy()));
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_SCANS));
    let mut tasks = JoinSet::new();
    for path in files {
        let semaphore = Arc::clone(&semaphore);
        let path = path.clone();
        tasks.spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return None;
            };
            read_session_summary(&path).await.ok().flatten()
        });
    }

    let mut sessions = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let Ok(Some(summary)) = result else {
            continue;
        };
        if let Some(expected) = &expected_cwd {
            let actual = summary.cwd.as_deref().and_then(normalized_cwd);
            if actual.as_deref() != Some(expected.as_str()) {
                continue;
            }
        }
        sessions.push(summary);
    }

    sessions.sort_by(|left, right| {
        right
            .modified_at_ms
            .cmp(&left.modified_at_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(sessions)
}

async fn is_directory(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
}

async fn session_files(directory: &Path) -> Result<Vec<PathBuf>, SessionError> {
    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(directory).await.map_err(|error| {
        SessionError::DirectoryUnreadable {
            path: directory.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    while let Some(entry) =
        entries
            .next_entry()
            .await
            .map_err(|error| SessionError::DirectoryUnreadable {
                path: directory.to_path_buf(),
                message: error.to_string(),
            })?
    {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

async fn file_modified_ms(path: &Path) -> Option<i64> {
    let metadata = tokio::fs::metadata(path).await.ok()?;
    let modified = metadata.modified().ok()?;
    match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).ok(),
        Err(_) => Some(0),
    }
}
