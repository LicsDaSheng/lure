//! 会话存储与内存缓存 `SessionManager`。
//!
//! 对齐上游 `nanobot/session/manager.py` 的核心存储语义：
//! - 存储文件名用 base64url（无 padding）编码 key，可逆、抗冲突、避免非法字符。
//! - JSONL：首行 metadata 记录，其后每行一条消息。
//! - `save` 用 temp + rename 原子写；`fsync=true` 时刷 file 与父目录（unix）。
//! - `get_or_create` 命中缓存直接返回；未命中从磁盘加载或新建。
//! - 缓存为有界 LRU，超过上限淘汰最久未用条目。
//! - 加载损坏行时跳过（合并了上游 `_load` 与 `_repair` 的容错读取）。
//!
//! Phase 2 暂不实现：weak-overflow 身份保留、file cap 归档、retention。详见 upstream-test-ledger。

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Local};
use serde_json::{json, Map, Value};

use crate::session::model::Session;

/// 会话缓存默认上限。
pub const SESSION_CACHE_MAX_SIZE: usize = 128;

/// 会话存储错误。
#[derive(Debug)]
pub enum SessionError {
    /// 读写会话文件失败。
    Io { path: PathBuf, source: io::Error },
    /// 请求保存的 key 不在缓存中。
    NotCached { key: String },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Io { path, source } => {
                write!(f, "会话文件 IO 失败 {}: {source}", path.display())
            }
            SessionError::NotCached { key } => write!(f, "会话未缓存: {key}"),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SessionError::Io { source, .. } => Some(source),
            SessionError::NotCached { .. } => None,
        }
    }
}

/// 会话管理器：负责加载、缓存与持久化会话。
pub struct SessionManager {
    sessions_dir: PathBuf,
    cache: HashMap<String, Session>,
    lru: VecDeque<String>,
    max_cached: usize,
}

impl SessionManager {
    /// 在 `workspace/sessions` 下创建/使用会话目录。
    pub fn new(workspace: impl AsRef<Path>) -> io::Result<Self> {
        let sessions_dir = workspace.as_ref().join("sessions");
        fs::create_dir_all(&sessions_dir)?;
        Ok(Self {
            sessions_dir,
            cache: HashMap::new(),
            lru: VecDeque::new(),
            max_cached: SESSION_CACHE_MAX_SIZE,
        })
    }

    /// 会话目录。
    pub fn sessions_dir(&self) -> &Path {
        &self.sessions_dir
    }

    /// 设置缓存上限（测试用）。
    pub fn set_max_cached(&mut self, limit: usize) {
        self.max_cached = limit;
    }

    /// 当前强缓存条目数。
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// 按 LRU 顺序（最旧在前）返回缓存 key。
    pub fn cache_keys(&self) -> Vec<String> {
        self.lru.iter().cloned().collect()
    }

    /// base64url（无 padding）编码存储 key。
    pub fn storage_key(key: &str) -> String {
        URL_SAFE_NO_PAD.encode(key.as_bytes())
    }

    /// 反解 [`Self::storage_key`]，失败返回 `None`。
    pub fn decode_storage_key(stem: &str) -> Option<String> {
        let bytes = URL_SAFE_NO_PAD.decode(stem).ok()?;
        String::from_utf8(bytes).ok()
    }

    /// 会话 JSONL 文件路径。
    pub fn session_path(&self, key: &str) -> PathBuf {
        self.sessions_dir
            .join(format!("{}.jsonl", Self::storage_key(key)))
    }

    /// 旧版 workspace 内会话文件名：把 `:` 有损替换为 `_`。
    fn legacy_lossy_path(&self, key: &str) -> PathBuf {
        self.sessions_dir
            .join(format!("{}.jsonl", safe_filename(&key.replace(':', "_"))))
    }

    /// 枚举 sessions 目录下所有已存储会话 key。
    ///
    /// 用于 WebUI session list 等需要遍历会话的场景；优先反解 base64url 文件名，
    /// 无法反解时读取 metadata 中的 `key`，并把旧版有损 stem 迁移到 canonical 文件名。
    pub fn list_stored_keys(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.sessions_dir) else {
            return Vec::new();
        };
        let mut keys: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    return None;
                }
                let stem = path.file_stem()?.to_str()?;
                if let Some(key) = Self::decode_storage_key(stem) {
                    return Some(key);
                }
                let key = stored_key_for_path(&path)?;
                self.migrate_legacy_path(&path, &key);
                Some(key)
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// 命中缓存返回；否则加载或新建。
    pub fn get_or_create(&mut self, key: &str) -> Result<&mut Session, SessionError> {
        if self.cache.contains_key(key) {
            self.touch(key);
            return Ok(self.cache.get_mut(key).expect("刚确认存在"));
        }
        let session = self.load(key)?.unwrap_or_else(|| Session::new(key));
        self.insert(key, session);
        Ok(self.cache.get_mut(key).expect("刚插入"))
    }

    /// 将缓存中该 key 的会话原子写入磁盘。
    pub fn save(&mut self, key: &str, fsync: bool) -> Result<(), SessionError> {
        let path = self.session_path(key);
        {
            let session = self.cache.get(key).ok_or_else(|| SessionError::NotCached {
                key: key.to_string(),
            })?;
            write_session_to_disk(&path, &self.sessions_dir, session, fsync).map_err(|source| {
                SessionError::Io {
                    path: path.clone(),
                    source,
                }
            })?;
        }
        self.touch(key);
        Ok(())
    }

    /// 重存所有缓存会话（带 fsync），返回成功数；单条失败不影响其余。
    pub fn flush_all(&mut self) -> usize {
        let keys: Vec<String> = self.cache.keys().cloned().collect();
        let mut flushed = 0;
        for key in keys {
            if self.save(&key, true).is_ok() {
                flushed += 1;
            }
        }
        flushed
    }

    /// 从内存缓存移除会话（不删磁盘）。
    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
        self.lru.retain(|k| k != key);
    }

    fn load(&self, key: &str) -> Result<Option<Session>, SessionError> {
        let path = self.session_path(key);
        if !path.exists() {
            let legacy_path = self.legacy_lossy_path(key);
            if legacy_path.exists() {
                let stored_key = stored_key_for_path(&legacy_path);
                if stored_key.as_deref().is_none_or(|stored| stored == key) {
                    let _ = fs::rename(&legacy_path, &path);
                }
            }
        }
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Some(parse_session(key, &text))),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SessionError::Io { path, source }),
        }
    }

    fn insert(&mut self, key: &str, session: Session) {
        self.lru.retain(|k| k != key);
        self.cache.insert(key.to_string(), session);
        self.lru.push_back(key.to_string());
        while self.cache.len() > self.max_cached {
            if let Some(evicted) = self.lru.pop_front() {
                self.cache.remove(&evicted);
            } else {
                break;
            }
        }
    }

    fn touch(&mut self, key: &str) {
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            self.lru.remove(pos);
            self.lru.push_back(key.to_string());
        }
    }

    fn migrate_legacy_path(&self, legacy_path: &Path, key: &str) {
        let canonical = self.session_path(key);
        if legacy_path == canonical || canonical.exists() {
            return;
        }
        let _ = fs::rename(legacy_path, canonical);
    }
}

fn safe_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn stored_key_for_path(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(line) else {
            return None;
        };
        if obj.get("_type").and_then(Value::as_str) == Some("metadata") {
            return obj.get("key").and_then(Value::as_str).map(str::to_string);
        }
        return None;
    }
    None
}

/// 容错解析 JSONL：跳过无法解析或非对象的行；首个 metadata 行提供会话元数据。
fn parse_session(key: &str, text: &str) -> Session {
    let mut messages: Vec<Value> = Vec::new();
    let mut metadata: Map<String, Value> = Map::new();
    let mut created_at: Option<DateTime<Local>> = None;
    let mut updated_at: Option<DateTime<Local>> = None;
    let mut last_consolidated: Value = json!(0);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if obj.get("_type").and_then(Value::as_str) == Some("metadata") {
            if let Some(Value::Object(m)) = obj.get("metadata") {
                metadata = m.clone();
            }
            created_at = obj.get("created_at").and_then(parse_dt);
            updated_at = obj.get("updated_at").and_then(parse_dt);
            if let Some(value) = obj.get("last_consolidated") {
                last_consolidated = value.clone();
            }
        } else {
            messages.push(Value::Object(obj));
        }
    }

    let now = Local::now();
    Session::from_loaded(
        key,
        messages,
        metadata,
        created_at.unwrap_or(now),
        updated_at.unwrap_or(now),
        &last_consolidated,
    )
}

fn parse_dt(value: &Value) -> Option<DateTime<Local>> {
    let text = value.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

/// temp + rename 原子写会话 JSONL；`fsync` 时刷 file 与父目录（unix）。
fn write_session_to_disk(
    path: &Path,
    sessions_dir: &Path,
    session: &Session,
    fsync: bool,
) -> io::Result<()> {
    let tmp = path.with_extension("jsonl.tmp");

    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;

        let meta_line = json!({
            "_type": "metadata",
            "key": session.key,
            "created_at": session.created_at.to_rfc3339(),
            "updated_at": session.updated_at.to_rfc3339(),
            "metadata": Value::Object(session.metadata.clone()),
            "last_consolidated": session.last_consolidated(),
        });
        writeln!(file, "{}", serde_json::to_string(&meta_line)?)?;
        for msg in &session.messages {
            writeln!(file, "{}", serde_json::to_string(msg)?)?;
        }

        if fsync {
            file.flush()?;
            file.sync_all()?;
        }
        drop(file);

        fs::rename(&tmp, path)?;

        if fsync {
            fsync_dir(sessions_dir)?;
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// 在 unix 上 fsync 目录，使 rename 持久化；共享文件系统拒绝时（EINVAL）忽略。
#[cfg(unix)]
fn fsync_dir(dir: &Path) -> io::Result<()> {
    match fs::File::open(dir) {
        Ok(handle) => match handle.sync_all() {
            Ok(()) => Ok(()),
            Err(e) if e.raw_os_error() == Some(libc_einval()) => Ok(()),
            Err(e) => Err(e),
        },
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn libc_einval() -> i32 {
    22 // EINVAL
}

/// 非 unix 平台跳过目录 fsync。
#[cfg(not(unix))]
fn fsync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}
