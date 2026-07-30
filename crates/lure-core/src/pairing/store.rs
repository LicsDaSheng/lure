//! DM 发送者配对码存储。
//!
//! 对齐上游 `nanobot/pairing/store.py`：`~/.nanobot/pairing.json` 持久化每个 channel
//! 的已审批发送者与待审批配对码。上游用模块级 `threading.Lock` + 每次调用重新 `_load`
//! 保证 CLI/async 两路可调用；lure 以 `PairingStore`（按路径参数化、每次调用 load/save）
//! 复刻同一语义——每次操作都从磁盘读最新状态，天然容纳外部手工编辑与并发写者。
//!
//! 时间为显式注入（秒，对齐上游 `time.time()` 浮点秒），与 `cron::service::tick(now_ms)`
//! 的注入约定一致，便于确定性测试 TTL 过期。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use uuid::Uuid;

/// 配对码字符集：大写字母 + 数字（对齐上游 `string.ascii_uppercase + string.digits`）。
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// 原始码长度（分组前，对齐上游 `_CODE_LENGTH = 8`）。
const CODE_LENGTH: usize = 8;
/// 默认 TTL 秒数（对齐上游 `_TTL_DEFAULT_S = 600`，10 分钟）。
pub const DEFAULT_TTL_SECS: f64 = 600.0;

/// 单条待审批配对请求（`list_pending` 返回项，含 code）。
#[derive(Debug, Clone, PartialEq)]
pub struct PendingRequest {
    pub code: String,
    pub channel: String,
    pub sender_id: String,
    pub created_at: f64,
    pub expires_at: f64,
}

/// `clear_channel` 的清理计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearCounts {
    pub approved: usize,
    pub pending: usize,
}

/// 内存中的配对数据（load 后已把所有 sender_id 归一为字符串）。
#[derive(Debug, Default)]
struct PairingData {
    /// channel -> 已审批 sender_id 集合。
    approved: BTreeMap<String, BTreeSet<String>>,
    /// code -> 待审批条目。
    pending: BTreeMap<String, PendingEntry>,
}

#[derive(Debug, Clone)]
struct PendingEntry {
    channel: String,
    sender_id: String,
    created_at: f64,
    expires_at: f64,
}

/// 配对码存储。持有文件路径，每个操作 load/mutate/save。
pub struct PairingStore {
    path: PathBuf,
}

impl PairingStore {
    /// 以显式 `pairing.json` 路径构造。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 为 `sender_id`@`channel` 生成新配对码（形如 `ABCD-EFGH`），持久化并返回。
    pub fn generate_code(
        &self,
        channel: &str,
        sender_id: &str,
        ttl_secs: f64,
        now_secs: f64,
    ) -> io::Result<String> {
        let mut data = self.load();
        gc_pending(&mut data, now_secs);
        let code = new_code();
        data.pending.insert(
            code.clone(),
            PendingEntry {
                channel: channel.to_string(),
                sender_id: sender_id.to_string(),
                created_at: now_secs,
                expires_at: now_secs + ttl_secs,
            },
        );
        self.save(&data)?;
        Ok(code)
    }

    /// 审批一个待审批码：成功返回 `(channel, sender_id)`，不存在或已过期返回 `None`。
    pub fn approve_code(&self, code: &str, now_secs: f64) -> io::Result<Option<(String, String)>> {
        let mut data = self.load();
        gc_pending(&mut data, now_secs);
        let Some(entry) = data.pending.remove(code) else {
            return Ok(None);
        };
        data.approved
            .entry(entry.channel.clone())
            .or_default()
            .insert(entry.sender_id.clone());
        self.save(&data)?;
        Ok(Some((entry.channel, entry.sender_id)))
    }

    /// 拒绝并丢弃一个待审批码；存在并移除返回 `true`。
    pub fn deny_code(&self, code: &str, now_secs: f64) -> io::Result<bool> {
        let mut data = self.load();
        gc_pending(&mut data, now_secs);
        if data.pending.remove(code).is_some() {
            self.save(&data)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// 判断 `sender_id` 是否已在 `channel` 审批通过。
    pub fn is_approved(&self, channel: &str, sender_id: &str) -> io::Result<bool> {
        let data = self.load();
        Ok(data
            .approved
            .get(channel)
            .is_some_and(|users| users.contains(sender_id)))
    }

    /// 返回所有未过期的待审批请求。
    pub fn list_pending(&self, now_secs: f64) -> io::Result<Vec<PendingRequest>> {
        let mut data = self.load();
        gc_pending(&mut data, now_secs);
        Ok(data
            .pending
            .iter()
            .map(|(code, e)| PendingRequest {
                code: code.clone(),
                channel: e.channel.clone(),
                sender_id: e.sender_id.clone(),
                created_at: e.created_at,
                expires_at: e.expires_at,
            })
            .collect())
    }

    /// 从 `channel` 移除一个已审批发送者；存在并移除返回 `true`。
    pub fn revoke(&self, channel: &str, sender_id: &str) -> io::Result<bool> {
        let mut data = self.load();
        let Some(users) = data.approved.get_mut(channel) else {
            return Ok(false);
        };
        if users.remove(sender_id) {
            if users.is_empty() {
                data.approved.remove(channel);
            }
            self.save(&data)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// 移除 `channel` 的全部已审批发送者，返回移除数量。
    pub fn revoke_channel(&self, channel: &str) -> io::Result<usize> {
        let mut data = self.load();
        let removed = data.approved.remove(channel).map_or(0, |u| u.len());
        if removed == 0 {
            return Ok(0);
        }
        self.save(&data)?;
        Ok(removed)
    }

    /// 移除 `channel` 的已审批发送者与待审批请求，返回各自数量。
    pub fn clear_channel(&self, channel: &str) -> io::Result<ClearCounts> {
        let mut data = self.load();
        let approved = data.approved.remove(channel).map_or(0, |u| u.len());
        let codes: Vec<String> = data
            .pending
            .iter()
            .filter(|(_, e)| e.channel == channel)
            .map(|(code, _)| code.clone())
            .collect();
        let pending = codes.len();
        for code in &codes {
            data.pending.remove(code);
        }
        if approved == 0 && pending == 0 {
            return Ok(ClearCounts {
                approved: 0,
                pending: 0,
            });
        }
        self.save(&data)?;
        Ok(ClearCounts { approved, pending })
    }

    /// 返回 `channel` 的全部已审批 sender_id（有序）。
    pub fn get_approved(&self, channel: &str) -> io::Result<Vec<String>> {
        let data = self.load();
        Ok(data
            .approved
            .get(channel)
            .map(|u| u.iter().cloned().collect())
            .unwrap_or_default())
    }

    /// 执行一条 `/pairing` 子命令并返回回复文本。
    ///
    /// 纯派发（除 store 变更外无副作用），CLI 与 agent 命令路由共用。
    pub fn handle_pairing_command(
        &self,
        channel: &str,
        subcommand_text: &str,
        now_secs: f64,
    ) -> io::Result<String> {
        let parts: Vec<&str> = subcommand_text.split_whitespace().collect();
        let sub = parts.first().copied().unwrap_or("list");
        let arg = parts.get(1).copied();

        let reply = match sub {
            "list" => {
                let pending = self.list_pending(now_secs)?;
                if pending.is_empty() {
                    "No pending pairing requests.".to_string()
                } else {
                    let mut lines = vec!["Pending pairing requests:".to_string()];
                    for item in pending {
                        let expiry = format_expiry(item.expires_at, now_secs);
                        lines.push(format!(
                            "- `{}` | {} | {} | {}",
                            item.code, item.channel, item.sender_id, expiry
                        ));
                    }
                    lines.join("\n")
                }
            }
            "approve" => match arg {
                None => "Usage: `/pairing approve <code>`".to_string(),
                Some(code) => match self.approve_code(code, now_secs)? {
                    None => format!("Invalid or expired pairing code: `{code}`"),
                    Some((ch, sid)) => {
                        format!("Approved pairing code `{code}` — {sid} can now access {ch}")
                    }
                },
            },
            "deny" => match arg {
                None => "Usage: `/pairing deny <code>`".to_string(),
                Some(code) => {
                    if self.deny_code(code, now_secs)? {
                        format!("Denied pairing code `{code}`")
                    } else {
                        format!("Pairing code `{code}` not found or already expired")
                    }
                }
            },
            "revoke" => match parts.len() {
                2 => {
                    let user = parts[1];
                    if self.revoke(channel, user)? {
                        format!("Revoked {user} from {channel}")
                    } else {
                        format!("{user} was not in the approved list for {channel}")
                    }
                }
                3 => {
                    let (ch, user) = (parts[1], parts[2]);
                    if self.revoke(ch, user)? {
                        format!("Revoked {user} from {ch}")
                    } else {
                        format!("{user} was not in the approved list for {ch}")
                    }
                }
                _ => "Usage: `/pairing revoke <user_id>` or `/pairing revoke <channel> <user_id>`"
                    .to_string(),
            },
            _ => "Unknown pairing command.\n\
                  Usage: `/pairing [list|approve <code>|deny <code>|revoke <user_id>|revoke <channel> <user_id>]`"
                .to_string(),
        };
        Ok(reply)
    }

    /// 从磁盘加载。文件缺失、损坏或类型异常一律回落空存储（对齐上游“损坏即重置”）。
    fn load(&self) -> PairingData {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(_) => return PairingData::default(),
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return PairingData::default(),
        };
        parse_data(&value)
    }

    /// 持久化到磁盘（approved 排序为列表，缩进 2，保留非 ASCII），temp + rename 原子写。
    fn save(&self, data: &PairingData) -> io::Result<()> {
        let approved: serde_json::Map<String, Value> = data
            .approved
            .iter()
            .map(|(ch, users)| {
                let list: Vec<Value> = users.iter().map(|u| Value::String(u.clone())).collect();
                (ch.clone(), Value::Array(list))
            })
            .collect();
        let pending: serde_json::Map<String, Value> = data
            .pending
            .iter()
            .map(|(code, e)| {
                let entry = serde_json::json!({
                    "channel": e.channel,
                    "sender_id": e.sender_id,
                    "created_at": e.created_at,
                    "expires_at": e.expires_at,
                });
                (code.clone(), entry)
            })
            .collect();
        let payload = serde_json::json!({ "approved": approved, "pending": pending });
        let json = serde_json::to_string_pretty(&payload).expect("pairing 数据可序列化");
        write_text_atomic(&self.path, &json)
    }
}

/// 当前墙钟秒（对齐上游 `time.time()`），供生产调用点使用。
pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// 返回发给未识别 DM 发送者的配对码提示文本。
pub fn format_pairing_reply(code: &str) -> String {
    format!(
        "Hi there! This assistant only responds to approved users.\n\n\
         Your pairing code is: `{code}`\n\n\
         To get access, ask the owner to approve this request in the nanobot WebUI.\n\
         If the WebUI is not available, the owner can also send `/pairing approve {code}`."
    )
}

/// 返回人类可读的过期字符串（如 `"120s"` 或 `"expired"`）。
pub fn format_expiry(expires_at: f64, now_secs: f64) -> String {
    let remaining = (expires_at - now_secs) as i64;
    if remaining > 0 {
        format!("{remaining}s")
    } else {
        "expired".to_string()
    }
}

/// 就地移除已过期的待审批条目（`expires_at < now`）。
fn gc_pending(data: &mut PairingData, now_secs: f64) {
    data.pending.retain(|_, e| e.expires_at >= now_secs);
}

/// 生成新配对码 `XXXX-XXXX`，随机源用 UUIDv4 字节映射到字符集。
fn new_code() -> String {
    let bytes = *Uuid::new_v4().as_bytes();
    let raw: String = bytes
        .iter()
        .take(CODE_LENGTH)
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", &raw[..4], &raw[4..])
}

/// 从 JSON Value 解析为内存数据，sender_id 一律归一为字符串（对齐上游 `str(u)`）。
fn parse_data(value: &Value) -> PairingData {
    let mut data = PairingData::default();

    if let Some(approved) = value.get("approved").and_then(Value::as_object) {
        for (channel, users) in approved {
            if let Some(list) = users.as_array() {
                let set: BTreeSet<String> = list.iter().filter_map(coerce_id).collect();
                if !set.is_empty() {
                    data.approved.insert(channel.clone(), set);
                }
            }
        }
    }

    if let Some(pending) = value.get("pending").and_then(Value::as_object) {
        for (code, info) in pending {
            let Some(obj) = info.as_object() else {
                continue;
            };
            let channel = obj
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let sender_id = obj.get("sender_id").and_then(coerce_id).unwrap_or_default();
            let created_at = obj.get("created_at").and_then(Value::as_f64).unwrap_or(0.0);
            let expires_at = obj.get("expires_at").and_then(Value::as_f64).unwrap_or(0.0);
            data.pending.insert(
                code.clone(),
                PendingEntry {
                    channel,
                    sender_id,
                    created_at,
                    expires_at,
                },
            );
        }
    }

    data
}

/// 把 JSON 值归一为字符串 sender_id：字符串原样，整数转十进制，其余按显示。
fn coerce_id(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i.to_string())
            } else if let Some(u) = n.as_u64() {
                Some(u.to_string())
            } else {
                Some(n.to_string())
            }
        }
        _ => None,
    }
}

/// temp + rename 原子写文本（与 `config::loader` 同约定：先 fsync 临时文件再重命名覆盖）。
fn write_text_atomic(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pairing.json");
    let tmp = dir.join(format!(".{file_name}.tmp.{}", process::id()));

    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
