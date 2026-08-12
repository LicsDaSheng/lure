//! MCP（Model Context Protocol）异步客户端（Stage 6）。
//!
//! 对齐上游 `nanobot/agent/mcp/` 客户端：JSON-RPC 2.0 会话（newline-delimited 传输）、
//! `initialize` 握手、`tools/list`（enabled-tools 过滤）、`tools/call`、瞬时错误重试。
//! stdio 传输为进程管道（`connect_stdio`）；HTTP/SSE 传输留待后续（需真实协议服务器）。
//!
//! 传输无关：`McpClient` 泛型于 `AsyncBufRead`/`AsyncWrite`，测试用内存 duplex 驱动
//! 协议层；`connect_stdio` 包一层子进程管道。

mod client;

pub use client::{
    connect_stdio, McpClient, McpError, McpTool, StdioMcpClient, DEFAULT_PROTOCOL_VERSION,
};
