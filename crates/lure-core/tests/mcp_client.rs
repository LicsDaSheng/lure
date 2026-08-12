//! Stage 6 MCP 异步客户端契约测试。
//!
//! 对齐上游 `tests/agent/test_mcp_connection.py` / `test_mcp_transient_retry.py`：
//! JSON-RPC 2.0 会话（initialize 握手、tools/list + enabled-tools 过滤、tools/call）、
//! 瞬时错误重试、stdio 子进程传输。协议层用内存 duplex 驱动；stdio 用 python3 mock 服务器。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{split, AsyncBufReadExt, AsyncWriteExt};

use lure_core::mcp::{connect_stdio, McpClient, McpError};

/// mock MCP 服务器：读 newline JSON-RPC，按 method 应答。
/// `tools_call_failures` > 0 时 tools/call 返回瞬时错误并递减（测重试）。
async fn mock_server(
    reader: impl tokio::io::AsyncRead + Unpin,
    mut writer: impl tokio::io::AsyncWrite + Unpin,
    tools_call_failures: Arc<AtomicUsize>,
) {
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await.unwrap() == 0 {
            break;
        }
        let req: Value = serde_json::from_str(&line).unwrap();
        let id = req["id"].clone();
        let resp = match req["method"].as_str().unwrap_or_default() {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {"protocolVersion": "2025-06-18", "capabilities": {}, "serverInfo": {"name": "mock", "version": "1.0"}}
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {"tools": [
                    {"name": "add", "description": "add two numbers", "inputSchema": {"type": "object"}},
                    {"name": "remove", "description": "remove a thing", "inputSchema": {"type": "object"}}
                ]}
            }),
            "tools/call" => {
                // 前 N 次瞬时错误（测重试），随后成功。
                let prev =
                    tools_call_failures.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                        if n > 0 {
                            Some(n - 1)
                        } else {
                            None
                        }
                    });
                if prev.is_ok() {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "transient"}})
                } else {
                    json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "done"}], "isError": false}})
                }
            }
            _ => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}})
            }
        };
        writer
            .write_all(format!("{resp}\n").as_bytes())
            .await
            .unwrap();
        writer.flush().await.unwrap();
    }
}

/// 建 duplex 对 + mock 服务器 task，返回客户端。
fn mock_harness(
    failures: usize,
    retries: usize,
) -> McpClient<impl tokio::io::AsyncRead + Unpin, impl tokio::io::AsyncWrite + Unpin> {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (cr, cw) = split(client_io);
    let (sr, sw) = split(server_io);
    let failures = Arc::new(AtomicUsize::new(failures));
    tokio::spawn(async move {
        mock_server(sr, sw, failures).await;
    });
    McpClient::new(cr, cw, retries)
}

// ---- 协议层契约 -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initialize_handshake_returns_capabilities() {
    let mut client = mock_harness(0, 0);
    let init = client.initialize("2025-06-18", "lure-test").await.unwrap();
    assert_eq!(init["protocolVersion"], "2025-06-18");
    assert_eq!(init["serverInfo"]["name"], "mock");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tools_list_filters_enabled() {
    let mut client = mock_harness(0, 0);
    let all = client.tools_list(None).await.unwrap();
    assert_eq!(all.len(), 2, "无过滤应返回全部工具");

    let enabled = vec!["add".to_string()];
    let filtered = client.tools_list(Some(&enabled)).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "add");
    assert_eq!(filtered[0].description, "add two numbers");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tools_call_returns_result() {
    let mut client = mock_harness(0, 0);
    let result = client
        .tools_call("add", json!({"a": 1, "b": 2}))
        .await
        .unwrap();
    assert_eq!(result["content"][0]["text"], "done");
    assert_eq!(result["isError"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_on_transient_rpc_error() {
    // 前 2 次 tools/call 返回瞬时错误 → 客户端重试 2 次后成功。
    let mut client = mock_harness(2, 2);
    let result = client.tools_call("add", json!({"a": 1})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "done");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_transient_rpc_error_is_not_retried() {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (cr, cw) = split(client_io);
    let (sr, mut sw) = split(server_io);
    tokio::spawn(async move {
        // 只响应一次：非瞬时 RPC 错误（-32602）。
        let mut reader = tokio::io::BufReader::new(sr);
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap() == 0 {
            return;
        }
        let req: Value = serde_json::from_str(&line).unwrap();
        let resp = json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32602, "message": "bad params"}});
        sw.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
        sw.flush().await.unwrap();
    });
    let mut client = McpClient::new(cr, cw, 2);
    let err = client.tools_list(None).await.unwrap_err();
    match err {
        McpError::Rpc { code, .. } => assert_eq!(code, -32602),
        other => panic!("应返回 Rpc 错误: {other:?}"),
    }
}

// ---- stdio 传输 -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_stdio_handshakes_with_mock_server() {
    let script = r#"
import sys, json
for line in sys.stdin:
    req = json.loads(line)
    if req.get("method") == "initialize":
        resp = {"jsonrpc":"2.0","id":req["id"],"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"mock","version":"1.0"}}}
    elif req.get("method") == "tools/list":
        resp = {"jsonrpc":"2.0","id":req["id"],"result":{"tools":[{"name":"add","description":"a","inputSchema":{"type":"object"}}]}}
    else:
        resp = {"jsonrpc":"2.0","id":req["id"],"result":{"content":[{"type":"text","text":"ok"}]}}
    sys.stdout.write(json.dumps(resp)+"\n"); sys.stdout.flush()
"#;
    let mut stdio = connect_stdio("python3", &["-c".to_string(), script.to_string()], 0)
        .await
        .expect("spawn python3 mock 失败");
    let init = stdio
        .client_mut()
        .initialize("2025-06-18", "lure-test")
        .await
        .unwrap();
    assert_eq!(init["serverInfo"]["name"], "mock");
    let tools = stdio.client_mut().tools_list(None).await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "add");
    // drop 时 kill 子进程（StdioMcpClient::drop）。
}
