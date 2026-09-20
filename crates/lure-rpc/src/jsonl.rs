use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

use crate::RpcError;
use crate::capture::StdoutCapture;

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
    read_json_lines_with_capture(reader, frame_limit, sender, None).await
}

/// 同 [`read_json_lines`]，并在解析前把每一行的原始字节交给采集器。
///
/// 采集是尽力而为的：写入失败只记录在采集器里，不会中断读取。
///
/// # Errors
///
/// 读取失败、记录过长、UTF-8 非法或 JSON 非法时返回类型化错误。
pub async fn read_json_lines_with_capture<R>(
    reader: R,
    frame_limit: usize,
    sender: mpsc::UnboundedSender<Value>,
    mut capture: Option<&mut StdoutCapture>,
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

        if let Some(capture) = capture.as_deref_mut() {
            capture.record(&buffer).await;
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
