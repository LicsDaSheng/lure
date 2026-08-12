//! MCP 客户端：JSON-RPC 会话 + stdio 传输。

use std::io;
use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// 默认 MCP 协议版本（2025-06-18）。
pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

/// 瞬时错误码集合（重试触发）。
const TRANSIENT_CODES: &[i64] = &[-32000, -32099];

/// MCP 工具（tools/list 项的最小视图）。
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// MCP 客户端错误。
#[derive(Debug)]
pub enum McpError {
    /// 传输层（IO/进程）错误。
    Transport(String),
    /// JSON-RPC 错误响应。
    Rpc { code: i64, message: String },
    /// 响应形状不符合协议。
    Protocol(String),
    /// 消息超时（无响应）。
    Timeout,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Transport(e) => write!(f, "MCP 传输错误: {e}"),
            McpError::Rpc { code, message } => write!(f, "MCP RPC 错误 ({code}): {message}"),
            McpError::Protocol(e) => write!(f, "MCP 协议错误: {e}"),
            McpError::Timeout => write!(f, "MCP 响应超时"),
        }
    }
}

impl std::error::Error for McpError {}

impl From<io::Error> for McpError {
    fn from(e: io::Error) -> Self {
        McpError::Transport(e.to_string())
    }
}

/// MCP 客户端：泛型于读/写传输，newline-delimited JSON-RPC。
pub struct McpClient<R, W> {
    reader: BufReader<R>,
    writer: W,
    next_id: u64,
    /// 瞬时错误重试次数（超时/传输错误/瞬时 RPC 码）。
    retries: usize,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> McpClient<R, W> {
    /// 新建客户端；`retries` 为瞬时错误重试次数。读端内部包 `BufReader`。
    pub fn new(reader: R, writer: W, retries: usize) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 0,
            retries,
        }
    }

    /// `initialize` 握手：协商协议版本，返回服务端 capabilities 载荷。
    pub async fn initialize(
        &mut self,
        protocol_version: &str,
        client_name: &str,
    ) -> Result<Value, McpError> {
        let params = json!({
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {"name": client_name, "version": env!("CARGO_PKG_VERSION")},
        });
        self.request("initialize", params).await
    }

    /// `tools/list`；`enabled` 白名单过滤（None 表示全部）。
    pub async fn tools_list(
        &mut self,
        enabled: Option<&[String]>,
    ) -> Result<Vec<McpTool>, McpError> {
        let result = self.request("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| McpError::Protocol("tools/list 缺 tools 数组".into()))?;
        let mut out = Vec::new();
        for tool in tools {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(enabled) = enabled {
                if !enabled.iter().any(|e| e == &name) {
                    continue;
                }
            }
            out.push(McpTool {
                name,
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                input_schema: tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            });
        }
        Ok(out)
    }

    /// `tools/call`：调用工具，返回完整响应载荷。
    pub async fn tools_call(&mut self, name: &str, args: Value) -> Result<Value, McpError> {
        let params = json!({"name": name, "arguments": args});
        self.request("tools/call", params).await
    }

    /// 发一个 JSON-RPC 请求并读回同 id 的响应；瞬时错误按 `retries` 重试。
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        let mut attempt = 0usize;
        loop {
            self.next_id += 1;
            let id = self.next_id;
            let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            let mut line =
                serde_json::to_string(&req).map_err(|e| McpError::Protocol(e.to_string()))?;
            line.push('\n');
            self.writer.write_all(line.as_bytes()).await?;
            self.writer.flush().await?;

            match self.read_response(id).await {
                Ok(result) => return Ok(result),
                Err(e) if self.retries > 0 && attempt < self.retries && is_transient(&e) => {
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 读行直到匹配 `id` 的响应；返回 result 载荷。
    async fn read_response(&mut self, id: u64) -> Result<Value, McpError> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.reader.read_line(&mut line).await?;
            if n == 0 {
                return Err(McpError::Transport("对端关闭".into()));
            }
            let value: Value =
                serde_json::from_str(line.trim()).map_err(|e| McpError::Protocol(e.to_string()))?;
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue; // 通知或其它请求的响应 → 跳过
            }
            if let Some(result) = value.get("result") {
                return Ok(result.clone());
            }
            if let Some(error) = value.get("error") {
                return Err(McpError::Rpc {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or(-1),
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("rpc error")
                        .to_string(),
                });
            }
            return Err(McpError::Protocol("响应缺 result/error".into()));
        }
    }
}

/// 瞬时错误判定：传输错误、RPC 瞬时码。
fn is_transient(e: &McpError) -> bool {
    match e {
        McpError::Transport(_) | McpError::Timeout => true,
        McpError::Rpc { code, .. } => TRANSIENT_CODES.contains(code),
        McpError::Protocol(_) => false,
    }
}

/// stdio 连接的客户端（持有子进程，drop 时 kill）。
pub struct StdioMcpClient {
    child: Child,
    client: McpClient<ChildStdout, ChildStdin>,
}

impl StdioMcpClient {
    /// 访问底层传输无关客户端。
    pub fn client_mut(&mut self) -> &mut McpClient<ChildStdout, ChildStdin> {
        &mut self.client
    }

    /// 等待子进程退出（测试用）。
    pub async fn wait(mut self) -> io::Result<Option<i32>> {
        self.child.wait().await.map(|s| s.code())
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// 通过 stdio 连接一个 MCP 服务器子进程。
pub async fn connect_stdio(
    command: &str,
    args: &[String],
    retries: usize,
) -> Result<StdioMcpClient, McpError> {
    let mut child = Command::new(command)
        .args(args.iter().map(String::as_str))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| McpError::Transport(format!("spawn {command}: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpError::Transport("无 stdout".into()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| McpError::Transport("无 stdin".into()))?;
    let client = McpClient::new(stdout, stdin, retries);
    Ok(StdioMcpClient { child, client })
}
