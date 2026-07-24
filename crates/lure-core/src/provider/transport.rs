//! 基于 `ureq` 的同步 HTTP 传输。
//!
//! 实现 [`HttpTransport`]，为 `OpenAiCompatProvider` 提供真实网络出口。非 2xx 的 HTTP
//! 响应仍作为 `Ok(HttpResponse)` 返回（由上层按状态码分类），仅连接/超时等传输层失败
//! 才返回 `Err`。

use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use crate::provider::http::{HttpRequest, HttpResponse, HttpTransport};

/// 默认请求超时。
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// `ureq` 同步 HTTP 传输。
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// 以默认超时构造。
    pub fn new() -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(DEFAULT_TIMEOUT).build(),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport for UreqTransport {
    fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let body = serde_json::to_string(&request.body).map_err(|e| e.to_string())?;

        let mut http_request = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            http_request = http_request.set(name, value);
        }

        match http_request.send_string(&body) {
            Ok(response) => {
                let status = response.status();
                let text = response.into_string().map_err(|e| e.to_string())?;
                Ok(HttpResponse { status, body: text })
            }
            // 非 2xx：ureq 归为 Status 错误，但对上层是一个可分类的 HTTP 响应。
            Err(ureq::Error::Status(code, response)) => {
                let text = response.into_string().unwrap_or_default();
                Ok(HttpResponse {
                    status: code,
                    body: text,
                })
            }
            // 连接/超时等传输层失败。
            Err(ureq::Error::Transport(transport)) => Err(transport.to_string()),
        }
    }

    fn post_json_streaming(
        &self,
        request: &HttpRequest,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<u16, String> {
        let body = serde_json::to_string(&request.body).map_err(|e| e.to_string())?;

        let mut http_request = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            http_request = http_request.set(name, value);
        }

        // 真正的增量：拿到响应体的 reader 后逐行读取、边收边发。
        let (status, reader): (u16, Box<dyn Read + Send + Sync>) = match http_request
            .send_string(&body)
        {
            Ok(response) => (response.status(), Box::new(response.into_reader())),
            Err(ureq::Error::Status(code, response)) => (code, Box::new(response.into_reader())),
            Err(ureq::Error::Transport(transport)) => return Err(transport.to_string()),
        };

        for line in BufReader::new(reader).lines() {
            let line = line.map_err(|e| e.to_string())?;
            on_line(&line);
        }
        Ok(status)
    }
}
