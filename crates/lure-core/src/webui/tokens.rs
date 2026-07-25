//! WebUI token 签发与校验。
//!
//! 对齐上游 `nanobot/webui/gateway_tokens.py`：WS 连接 token 与 REST api_token 分离签发、
//! 各自带 TTL 与容量上限；check 前 purge 过期项。desktop 场景下二者均由 bootstrap 一次性签发。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 一对刚签发的 token。
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedTokens {
    /// WebSocket 连接 token（`?token=` 查询参数）。
    pub token: String,
    /// REST api token（`Authorization: Bearer`）。
    pub api_token: String,
}

/// token 签发器：TTL + 容量上限。
pub struct TokenIssuer {
    ttl: Duration,
    max_tokens: usize,
    ws_tokens: HashMap<String, Instant>,
    api_tokens: HashMap<String, Instant>,
}

impl TokenIssuer {
    /// `ttl_secs` 为 token 寿命（秒）；`max_tokens` 为每类 token 的在册上限。
    pub fn new(ttl_secs: u64, max_tokens: usize) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_secs),
            max_tokens,
            ws_tokens: HashMap::new(),
            api_tokens: HashMap::new(),
        }
    }

    /// 签发一对 token；容量满时 panic（调用方应先 `try_issue` 确认）。
    pub fn issue(&mut self) -> IssuedTokens {
        self.try_issue().expect("token 签发容量已满")
    }

    /// 尝试签发一对 token；任一类别在册已满（purge 后）返回 `None`。
    pub fn try_issue(&mut self) -> Option<IssuedTokens> {
        self.purge_expired();
        if self.ws_tokens.len() >= self.max_tokens || self.api_tokens.len() >= self.max_tokens {
            return None;
        }
        let expiry = Instant::now() + self.ttl;
        let issued = IssuedTokens {
            token: random_token(),
            api_token: random_token(),
        };
        self.ws_tokens.insert(issued.token.clone(), expiry);
        self.api_tokens.insert(issued.api_token.clone(), expiry);
        Some(issued)
    }

    /// 校验 WS token 是否在册且未过期。
    pub fn check_ws_token(&mut self, token: &str) -> bool {
        check_map(&mut self.ws_tokens, token)
    }

    /// 校验 api token 是否在册且未过期。
    pub fn check_api_token(&mut self, token: &str) -> bool {
        check_map(&mut self.api_tokens, token)
    }

    fn purge_expired(&mut self) {
        let now = Instant::now();
        self.ws_tokens.retain(|_, expiry| *expiry > now);
        self.api_tokens.retain(|_, expiry| *expiry > now);
    }
}

fn check_map(tokens: &mut HashMap<String, Instant>, token: &str) -> bool {
    let now = Instant::now();
    tokens.retain(|_, expiry| *expiry > now);
    tokens.contains_key(token)
}

/// 上游 `secrets.token_urlsafe` 的等价物：两段 uuid4 拼成 64 字符十六进制。
fn random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
