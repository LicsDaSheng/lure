//! 基于 `reqwest` 的异步 HTTP 传输（Stage 2）。
//!
//! 实现 [`HttpTransport`]，为 `OpenAiCompatProvider` 提供真实异步网络出口。非 2xx 的
//! HTTP 响应仍作为 `Ok(HttpResponse)` 返回（由上层按状态码分类），仅连接/超时等传输层
//! 失败才返回 `Err`。流式 POST 用 `bytes_stream` 边收边按行回调（SSE 增量）。

use std::time::Duration;

use futures::StreamExt;

use crate::provider::http::{HttpRequest, HttpResponse, HttpTransport};

/// 默认请求超时。
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// `reqwest` 异步 HTTP 传输。
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// 以默认超时构造。
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(DEFAULT_TIMEOUT)
                .build()
                .expect("构建 reqwest client 失败"),
        }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HttpTransport for ReqwestTransport {
    async fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let body = serde_json::to_string(&request.body).map_err(|e| e.to_string())?;

        let mut builder = self.client.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        let response = builder.body(body).send().await.map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(|e| e.to_string())?;
        Ok(HttpResponse { status, body: text })
    }

    async fn post_json_streaming(
        &self,
        request: &HttpRequest,
        on_line: &mut (dyn FnMut(String) + Send),
    ) -> Result<u16, String> {
        let body = serde_json::to_string(&request.body).map_err(|e| e.to_string())?;

        let mut builder = self.client.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        let response = builder.body(body).send().await.map_err(|e| e.to_string())?;
        let status = response.status().as_u16();

        // 真正的增量：按字节流读取，按行切分、边收边发（SSE 逐行回调）。
        let mut stream = response.bytes_stream();
        let mut pending = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            for &byte in chunk.iter() {
                if byte == b'\n' {
                    let line = String::from_utf8_lossy(&pending).into_owned();
                    pending.clear();
                    on_line(line);
                } else {
                    pending.push(byte);
                }
            }
        }
        if !pending.is_empty() {
            on_line(String::from_utf8_lossy(&pending).into_owned());
        }
        Ok(status)
    }
}
