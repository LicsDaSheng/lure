//! Provider 子系统：LLM provider 契约、占位、OpenAI-compatible 实现与 registry。
//!
//! Phase 3 提供同步 `LlmProvider` trait、契约类型与 `EchoProvider` 占位；
//! Phase 4 增加 HTTP 传输抽象、`OpenAiCompatProvider`（请求/响应/错误分类）与最小
//! provider registry（选择顺序）。stateful model runtime resolver、config 驱动的
//! provider 自动匹配、真实 HTTP 传输留待后续。

mod echo;
mod http;
mod openai;
pub mod registry;
mod types;

pub use echo::EchoProvider;
pub use http::{HttpRequest, HttpResponse, HttpTransport};
pub use openai::{build_chat_request, parse_chat_response, OpenAiCompatProvider};
pub use registry::{find_by_name, match_provider, ProviderSpec, PROVIDERS};
pub use types::{CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError};
