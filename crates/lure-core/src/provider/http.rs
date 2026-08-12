//! HTTP 传输抽象。
//!
//! provider 通过 [`HttpTransport`] 发请求，使单元测试可用假传输注入响应，真实
//! 网络实现（如 blocking HTTP 客户端）留待需要时接入（见 upstream-test-ledger）。

use serde_json::Value;

/// 一次 HTTP POST JSON 请求。
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// 目标 URL。
    pub url: String,
    /// 请求头（键值对）。
    pub headers: Vec<(String, String)>,
    /// JSON 请求体。
    pub body: Value,
}

/// 一次 HTTP 响应。
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应体文本。
    pub body: String,
}

/// HTTP 传输契约。
///
/// `Err` 表示传输/网络层失败（连接、超时等），非 2xx 的 HTTP 响应仍走 `Ok`，
/// 由上层按状态码分类。异步版本（Stage 2）：reqwest 驱动，不再阻塞调用线程。
#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync {
    /// 发送一次 POST JSON 请求。
    async fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, String>;

    /// 发送一次**流式** POST：逐行读取响应体并回调 `on_line`，返回 HTTP 状态码。
    ///
    /// 默认实现回退到 [`post_json`](Self::post_json) 并把整段响应按行回放（非真正增量）；
    /// 真实传输（如 `ReqwestTransport`）覆盖此方法以边收边发。非 2xx 响应仍返回
    /// `Ok(status)`，由上层按状态码分类。
    async fn post_json_streaming(
        &self,
        request: &HttpRequest,
        on_line: &mut (dyn FnMut(String) + Send),
    ) -> Result<u16, String> {
        let response = self.post_json(request).await?;
        let lines: Vec<String> = response.body.lines().map(str::to_string).collect();
        for line in lines {
            on_line(line);
        }
        Ok(response.status)
    }
}
