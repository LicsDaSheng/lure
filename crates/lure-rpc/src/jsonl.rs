use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

use crate::RpcError;

/// 按严格 LF 边界读取 Pi RPC JSONL 记录。
///
/// # Errors
///
/// 读取失败、记录过长、UTF-8 非法或 JSON 非法时返回类型化错误。
pub async fn read_json_lines<R>(
    reader: R,
    frame_limit: usize,
    sender: mpsc::UnboundedSender<Value>,
) -> Result<(), RpcError>
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer).await?;
        if read == 0 {
            return Ok(());
        }
        if buffer.len() > frame_limit {
            return Err(RpcError::FrameTooLarge { limit: frame_limit });
        }

        if buffer.last() == Some(&b'\n') {
            buffer.pop();
        }
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
        if buffer.is_empty() {
            continue;
        }

        std::str::from_utf8(&buffer).map_err(|error| RpcError::InvalidUtf8(error.to_string()))?;
        let value = serde_json::from_slice(&buffer)
            .map_err(|error| RpcError::InvalidJson(error.to_string()))?;
        if sender.send(value).is_err() {
            return Ok(());
        }
    }
}
